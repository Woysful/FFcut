//! Runs an argv exactly as given. Progress via `-progress pipe:1`, cancel by killing the child.

use std::collections::VecDeque;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::thread;

#[derive(Debug, Clone, PartialEq)]
pub enum Event {
    /// `percent` is None when the total duration is unknown.
    Progress { percent: Option<f32>, out_time: f64, speed: Option<f32>, fps: Option<f32> },
    Finished,
    Failed(String),
    Cancelled,
}

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
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW
    }
    cmd
}

impl Job {
    /// `argv[0]` is replaced by `program` (so a bundled ffmpeg can be used).
    /// `total` = expected output duration in seconds, for the percentage.
    pub fn spawn(program: &Path, argv: &[String], total: Option<f64>) -> std::io::Result<Job> {
        let output = argv.last().map(PathBuf::from);

        let mut cmd = command(program);
        // "-nostdin": ffmpeg must never wait for keyboard input; "-progress": machine-readable stats.
        cmd.args(["-nostdin", "-progress", "pipe:1", "-nostats"]);
        cmd.args(argv.iter().skip(1));
        cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());

        let mut child = cmd.spawn()?;
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let child = Arc::new(Mutex::new(Some(child)));
        let cancelled = Arc::new(AtomicBool::new(false));
        let (tx, rx) = channel();

        // stderr: keep only the tail, that is where the error message lives.
        let tail = Arc::new(Mutex::new(VecDeque::<String>::new()));
        let t_tail = tail.clone();
        let err_thread = thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let mut t = t_tail.lock().unwrap();
                if t.len() == 12 {
                    t.pop_front();
                }
                t.push_back(line);
            }
        });

        let (c2, cancel2) = (child.clone(), cancelled.clone());
        thread::spawn(move || {
            let mut p = Progress::default();
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if let Some(ev) = p.feed(&line, total) {
                    let _ = tx.send(ev);
                }
            }
            // stdout closed => process exited (or was killed)
            let status = c2.lock().unwrap().take().and_then(|mut c| c.wait().ok());
            let _ = err_thread.join();

            let was_cancelled = cancel2.load(Ordering::SeqCst);
            let ok = status.map_or(false, |s| s.success());
            if was_cancelled || !ok {
                if let Some(o) = &output {
                    let _ = std::fs::remove_file(o); // never leave a half-written file behind
                }
            }
            let ev = if was_cancelled {
                Event::Cancelled
            } else if ok {
                Event::Finished
            } else {
                let msg = tail.lock().unwrap().iter().cloned().collect::<Vec<_>>().join("\n");
                Event::Failed(msg)
            };
            let _ = tx.send(ev);
        });

        Ok(Job { child, cancelled, events: rx })
    }

    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
        if let Some(c) = self.child.lock().unwrap().as_mut() {
            let _ = c.kill();
        }
    }
}

/// Parser for `-progress` key=value blocks (a block ends with `progress=continue|end`).
#[derive(Default)]
pub struct Progress {
    out_time: f64,
    speed: Option<f32>,
    fps: Option<f32>,
}

impl Progress {
    pub fn feed(&mut self, line: &str, total: Option<f64>) -> Option<Event> {
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
}
