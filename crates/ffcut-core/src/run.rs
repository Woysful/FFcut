//! Runs an argv exactly as given. Progress via `-progress pipe:1`, cancel by killing the child.

use crate::error::Error;
use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Output, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

/// How much of ffmpeg's stderr a failure reports: the error message is at the end.
const STDERR_TAIL_LINES: usize = 12;
/// How often the runner looks whether the process has exited once its output closed.
const EXIT_POLL: Duration = Duration::from_millis(10);
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `percent` is None when the total duration is unknown.
    Progress {
        percent: Option<f32>,
        out_time: f64,
        speed: Option<f32>,
        fps: Option<f32>,
    },
    Finished,
    Failed(String),
    Cancelled,
}

/// A running ffmpeg. Dropping it stops the process (and removes what it wrote, like a cancel).
pub struct Job {
    child: Arc<Mutex<Option<Child>>>,
    cancelled: Arc<AtomicBool>,
    pub events: Receiver<Event>,
}

/// `Command::new` that never flashes a console window on Windows (GUI app spawning ffmpeg/ffprobe).
/// Every external tool must be started through this.
pub fn command<P: AsRef<std::ffi::OsStr>>(program: P) -> Command {
    #[allow(unused_mut)]
    let mut cmd = Command::new(program);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(CREATE_NO_WINDOW);
    }
    cmd
}

/// Runs `cmd` to the end with no input and returns its output, or an error saying what went wrong:
/// it could not be started, or it exited with a failure (then the error carries its stderr).
pub(crate) fn run_capture(tool: &str, cmd: &mut Command) -> Result<Output, Error> {
    let out = cmd.stdin(Stdio::null()).output().map_err(|source| Error::Spawn { tool: tool.into(), source })?;
    if !out.status.success() {
        return Err(Error::Tool { tool: tool.into(), stderr: String::from_utf8_lossy(&out.stderr).trim().to_string() });
    }
    Ok(out)
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Size and modification time of a file; `None` = it does not exist.
type Stamp = Option<(u64, Option<SystemTime>)>;

fn stamp(p: &Path) -> Stamp {
    std::fs::metadata(p).ok().map(|m| (m.len(), m.modified().ok()))
}

/// Did the job create or write this file? Then a failed or cancelled job must not leave it behind.
/// A file it never touched (`-n` and the file exists, or it failed before opening its outputs) stays.
fn written(before: &Stamp, after: &Stamp) -> bool {
    after.is_some() && before != after
}

/// The exit status decides: a job that succeeded is finished even if a cancel came too late to stop it.
fn outcome(success: bool, cancelled: bool, stderr_tail: impl FnOnce() -> String) -> Event {
    if success {
        Event::Finished
    } else if cancelled {
        Event::Cancelled
    } else {
        Event::Failed(stderr_tail())
    }
}

/// Waits for the process in `slot` to exit, without holding the lock while it runs (so it can be killed).
fn wait_exit(slot: &Mutex<Option<Child>>) -> Option<ExitStatus> {
    loop {
        {
            let mut guard = lock(slot);
            let child = guard.as_mut()?;
            match child.try_wait() {
                Ok(Some(status)) => {
                    *guard = None;
                    return Some(status);
                }
                Ok(None) => {}
                Err(_) => {
                    let status = child.wait().ok();
                    *guard = None;
                    return status;
                }
            }
        }
        thread::sleep(EXIT_POLL);
    }
}

impl Job {
    /// `argv[0]` is replaced by `program` (so a bundled ffmpeg can be used).
    /// `outputs` = the files the command writes; when it fails or is cancelled, those it created or
    /// changed are deleted (never a file it left alone).
    /// `total` = expected output duration in seconds, for the percentage.
    pub fn spawn(program: &Path, argv: &[String], outputs: &[PathBuf], total: Option<f64>) -> std::io::Result<Job> {
        let before: Vec<(PathBuf, Stamp)> = outputs.iter().map(|p| (p.clone(), stamp(p))).collect();

        let mut cmd = command(program);
        // "-nostdin": ffmpeg must never wait for keyboard input; "-progress": machine-readable stats.
        cmd.args(["-nostdin", "-progress", "pipe:1", "-nostats"]);
        cmd.args(argv.iter().skip(1));
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
            let _ = child.kill();
            let _ = child.wait();
            return Err(std::io::Error::other("the pipes to ffmpeg could not be opened"));
        };
        let child = Arc::new(Mutex::new(Some(child)));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();

        // stderr: keep only the tail, that is where the error message lives.
        let err_thread = thread::spawn(move || {
            let mut tail = VecDeque::with_capacity(STDERR_TAIL_LINES);
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                if tail.len() == STDERR_TAIL_LINES {
                    tail.pop_front();
                }
                tail.push_back(line);
            }
            tail
        });

        let (slot, cancel_flag) = (child.clone(), cancelled.clone());
        thread::spawn(move || {
            let mut p = Progress::default();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(ev) = p.feed(&line, total) {
                    let _ = tx.send(ev);
                }
            }
            // stdout closed => the process is exiting (or was killed)
            let success = wait_exit(&slot).is_some_and(|s| s.success());
            let tail = err_thread.join().unwrap_or_default();
            if !success {
                for (path, was) in &before {
                    if written(was, &stamp(path)) {
                        let _ = std::fs::remove_file(path); // never leave a half-written file behind
                    }
                }
            }
            let _ = tx.send(outcome(success, cancel_flag.load(Ordering::SeqCst), || Vec::from(tail).join("\n")));
        });

        Ok(Job { child, cancelled, events: rx })
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(c) = lock(&self.child).as_mut() {
            let _ = c.kill();
        }
    }

    /// Cancels and waits (up to `timeout`) for the job to end, partial outputs removed. For shutting down:
    /// the runner's own thread would not get to clean up once the program exits.
    /// Returns the final event, `None` if it did not come in time (or was already taken from `events`).
    pub fn cancel_and_wait(&self, timeout: Duration) -> Option<Event> {
        self.cancel();
        let deadline = Instant::now() + timeout;
        loop {
            match self.events.recv_timeout(deadline.saturating_duration_since(Instant::now())) {
                Ok(Event::Progress { .. }) => continue,
                Ok(e) => return Some(e),
                Err(RecvTimeoutError::Timeout | RecvTimeoutError::Disconnected) => return None,
            }
        }
    }
}

impl Drop for Job {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Parser for `-progress` key=value blocks (a block ends with `progress=continue|end`).
#[derive(Default)]
pub(crate) struct Progress {
    out_time: f64,
    speed: Option<f32>,
    fps: Option<f32>,
}

impl Progress {
    pub(crate) fn feed(&mut self, line: &str, total: Option<f64>) -> Option<Event> {
        let (k, v) = line.split_once('=')?;
        let v = v.trim();
        match k.trim() {
            // NB: despite the name, out_time_ms is in microseconds. Use out_time_us.
            "out_time_us" | "out_time_ms" => {
                if let Ok(us) = v.parse::<i64>() {
                    self.out_time = (us.max(0) as f64) / 1_000_000.0;
                }
            }
            "speed" => self.speed = v.trim_end_matches('x').trim().parse().ok(),
            "fps" => self.fps = v.parse().ok(),
            "progress" => {
                return Some(Event::Progress {
                    percent: total.filter(|t| *t > 0.0).map(|t| ((self.out_time / t) * 100.0).clamp(0.0, 100.0) as f32),
                    out_time: self.out_time,
                    speed: self.speed,
                    fps: self.fps,
                });
            }
            _ => {}
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_progress_block() {
        let mut p = Progress::default();
        let mut last = None;
        for l in ["frame=10", "fps=25.0", "out_time_us=2500000", "speed=1.5x", "progress=continue"] {
            last = p.feed(l, Some(10.0)).or(last);
        }
        assert_eq!(last, Some(Event::Progress { percent: Some(25.0), out_time: 2.5, speed: Some(1.5), fps: Some(25.0) }));
    }

    #[test]
    fn exit_status_decides_the_outcome() {
        let tail = || "boom".to_string();
        assert_eq!(outcome(true, false, tail), Event::Finished);
        assert_eq!(outcome(true, true, tail), Event::Finished, "a cancel that came too late");
        assert_eq!(outcome(false, true, tail), Event::Cancelled);
        assert_eq!(outcome(false, false, tail), Event::Failed("boom".into()));
    }

    #[test]
    fn only_written_files_are_removed() {
        let t0 = Some(SystemTime::UNIX_EPOCH);
        let t1 = Some(SystemTime::UNIX_EPOCH + Duration::from_secs(5));
        assert!(written(&None, &Some((10, t0))), "created");
        assert!(written(&Some((10, t0)), &Some((0, t1))), "overwritten");
        assert!(written(&Some((10, t0)), &Some((10, t1))), "rewritten, same size");
        assert!(!written(&Some((10, t0)), &Some((10, t0))), "untouched");
        assert!(!written(&None, &None), "never there");
        assert!(!written(&Some((10, t0)), &None), "already gone");
    }

    fn ffmpeg() -> Option<PathBuf> {
        let f = crate::tools::find_tool("ffmpeg", None);
        if f.is_none() {
            eprintln!("skipped: ffmpeg is not on PATH");
        }
        f
    }

    fn tmp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ffcut-run-{}", std::process::id()));
        std::fs::create_dir_all(&d).unwrap();
        d.join(name)
    }

    fn finish(job: &Job) -> Event {
        loop {
            match job.events.recv().unwrap() {
                Event::Progress { .. } => continue,
                e => return e,
            }
        }
    }

    fn argv(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn refused_overwrite_keeps_the_existing_file() {
        let Some(ff) = ffmpeg() else { return };
        let out = tmp("keep.mkv");
        std::fs::write(&out, b"precious").unwrap();
        let o = out.to_string_lossy();
        let job = Job::spawn(&ff, &argv(&["ffmpeg", "-n", "-f", "lavfi", "-i", "testsrc=duration=1", &o]), std::slice::from_ref(&out), Some(1.0)).unwrap();
        // ffmpeg 6 reports a refused overwrite with exit status 0, older versions as a failure: either way
        // the file it never opened must be left alone
        assert!(matches!(finish(&job), Event::Failed(_) | Event::Finished));
        assert_eq!(std::fs::read(&out).unwrap(), b"precious");
    }

    #[test]
    fn failure_before_the_output_is_opened_keeps_the_existing_file() {
        let Some(ff) = ffmpeg() else { return };
        let out = tmp("keep2.mkv");
        std::fs::write(&out, b"precious").unwrap();
        let o = out.to_string_lossy();
        let job = Job::spawn(&ff, &argv(&["ffmpeg", "-y", "-i", "/definitely/not/here.mkv", &o]), std::slice::from_ref(&out), None).unwrap();
        assert!(matches!(finish(&job), Event::Failed(m) if m.contains("No such file")));
        assert_eq!(std::fs::read(&out).unwrap(), b"precious");
    }

    #[test]
    fn cancelled_overwrite_removes_the_half_written_file() {
        let Some(ff) = ffmpeg() else { return };
        let out = tmp("overwritten.mkv");
        std::fs::write(&out, b"old").unwrap();
        let initial = stamp(&out);
        let o = out.to_string_lossy();
        let a = argv(&["ffmpeg", "-y", "-f", "lavfi", "-i", "testsrc=size=1280x720:rate=60", "-c:v", "libx264", "-preset", "veryslow", &o]);
        let job = Job::spawn(&ff, &a, std::slice::from_ref(&out), None).unwrap();
        // wait until ffmpeg has opened (truncated) the output
        let t = Instant::now();
        while stamp(&out) == initial && t.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(20));
        }
        assert_eq!(job.cancel_and_wait(Duration::from_secs(10)), Some(Event::Cancelled));
        assert!(!out.exists());
    }

    #[test]
    fn dropping_a_job_stops_ffmpeg() {
        let Some(ff) = ffmpeg() else { return };
        let out = tmp("dropped.mkv");
        let o = out.to_string_lossy();
        let job = Job::spawn(&ff, &argv(&["ffmpeg", "-y", "-f", "lavfi", "-i", "testsrc", "-f", "matroska", &o]), std::slice::from_ref(&out), None).unwrap();
        let slot = job.child.clone();
        drop(job);
        let t = Instant::now();
        while lock(&slot).is_some() && t.elapsed() < Duration::from_secs(10) {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(lock(&slot).is_none(), "ffmpeg still running");
        let t = Instant::now();
        while out.exists() && t.elapsed() < Duration::from_secs(5) {
            thread::sleep(Duration::from_millis(20));
        }
        assert!(!out.exists());
    }
}
