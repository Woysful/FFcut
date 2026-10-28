//! Updating a portable install (the Windows zip) in place.
//!
//! The release zip is downloaded into the program's own folder (so its files can later be moved, not
//! copied), checked against the size and sha256 GitHub lists for it, and unpacked. Windows does not let
//! a running program or a loaded DLL be overwritten or deleted, but it lets them be renamed: each file is
//! moved aside (`name.ffcut-old`) and the new one put in its place. The old files are deleted on the next
//! start. A failure while replacing puts back everything moved so far.

use crate::update::Asset;
use sha2::{Digest, Sha256};
use std::fmt::Display;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Folder of an update in progress, and of the list of old files still to delete.
const WORK: &str = ".ffcut-update";
const OLD_LIST: &str = "old-files.txt";
const OLD_EXT: &str = "ffcut-old";

#[derive(Debug, Clone, PartialEq)]
pub enum Stage {
    Download {
        done: u64,
        total: u64,
    },
    Unpack,
    Replace,
    /// Installed; the program to start.
    Done(PathBuf),
    Failed(String),
}

pub type Shared = Arc<Mutex<Stage>>;

/// The folder FFcut runs from and the name of its program file.
pub fn location() -> Option<(PathBuf, String)> {
    let exe = std::env::current_exe().ok()?;
    Some((exe.parent()?.to_path_buf(), exe.file_name()?.to_string_lossy().into_owned()))
}

/// Installs `asset` over the running program in a thread; `repaint` wakes the UI on each step.
pub fn start(asset: Asset, repaint: impl Fn() + Send + 'static) -> Shared {
    let state = Arc::new(Mutex::new(Stage::Download { done: 0, total: asset.size }));
    let shared = state.clone();
    std::thread::spawn(move || {
        let report = |st: Stage| {
            *shared.lock().unwrap_or_else(|e| e.into_inner()) = st;
            repaint();
        };
        let end = match location() {
            Some((dir, exe)) => install(&asset, &dir, &exe, &report).map(|()| dir.join(exe)),
            None => Err("cannot tell where FFcut runs from".to_string()),
        };
        report(match end {
            Ok(exe) => Stage::Done(exe),
            Err(e) => Stage::Failed(e),
        });
    });
    state
}

/// Starts the installed program (with the file that was open).
pub fn restart(exe: &Path, open: Option<&Path>) -> bool {
    std::process::Command::new(exe).args(open).spawn().is_ok()
}

fn install(asset: &Asset, dir: &Path, exe: &str, report: &dyn Fn(Stage)) -> Result<(), String> {
    let work = dir.join(WORK);
    fs::create_dir_all(&work)
        .map_err(|e| format!("FFcut cannot write to its folder {} ({e}). Download the zip from the release page instead.", dir.display()))?;
    let zip = work.join("download.zip");
    let got = download(asset, &zip, report);
    let done = got.and_then(|()| install_zip(&zip, dir, exe, report));
    let _ = fs::remove_file(&zip);
    done
}

fn download(asset: &Asset, to: &Path, report: &dyn Fn(Stage)) -> Result<(), String> {
    let fail = |e: &dyn Display| format!("Download failed: {e}");
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_recv_response(Some(Duration::from_secs(30)))
        .timeout_recv_body(Some(Duration::from_secs(30 * 60)))
        .build()
        .into();
    let mut resp = agent.get(&asset.url).header("User-Agent", &format!("FFcut/{}", crate::update::VERSION)).call().map_err(|e| fail(&e))?;
    // one byte over the expected size is enough to know it is not the published file
    let mut body = resp.body_mut().with_config().limit(asset.size + 1).reader();
    let mut file = fs::File::create(to).map_err(|e| fail(&e))?;
    let mut hash = Sha256::new();
    let mut buf = vec![0u8; 64 * 1024];
    let (mut done, mut told) = (0u64, 0u64);
    loop {
        let n = body.read(&mut buf).map_err(|e| fail(&e))?;
        if n == 0 {
            break;
        }
        hash.update(&buf[..n]);
        file.write_all(&buf[..n]).map_err(|e| fail(&e))?;
        done += n as u64;
        if done - told >= 256 * 1024 {
            report(Stage::Download { done, total: asset.size });
            told = done;
        }
    }
    file.flush().map_err(|e| fail(&e))?;
    let sha: String = hash.finalize().iter().map(|b| format!("{b:02x}")).collect();
    if done != asset.size || sha != asset.sha256 {
        return Err("The download is damaged or is not the published file (its size or checksum differs). Nothing was changed.".into());
    }
    Ok(())
}

/// Unpacks a downloaded zip and puts its files in `dir`.
fn install_zip(zip: &Path, dir: &Path, exe: &str, report: &dyn Fn(Stage)) -> Result<(), String> {
    report(Stage::Unpack);
    let new = dir.join(WORK).join("new");
    let _ = fs::remove_dir_all(&new);
    let done = unpack(zip, &new).and_then(|root| {
        if !root.join(exe).is_file() {
            return Err(format!("The update has no {exe}: it is not FFcut for this system. Nothing was changed."));
        }
        report(Stage::Replace);
        let old = replace(dir, &root, &|a, b| fs::rename(a, b))?;
        remember(dir, &old);
        Ok(())
    });
    let _ = fs::remove_dir_all(&new);
    done
}

/// Unpacks into `to`; returns the folder that holds the files (the zip's only folder, when it is made of one).
fn unpack(zip: &Path, to: &Path) -> Result<PathBuf, String> {
    let bad = |e: &dyn Display| format!("Cannot unpack the update: {e}. Nothing was changed.");
    let mut ar = zip::ZipArchive::new(fs::File::open(zip).map_err(|e| bad(&e))?).map_err(|e| bad(&e))?;
    for i in 0..ar.len() {
        let mut f = ar.by_index(i).map_err(|e| bad(&e))?;
        let Some(rel) = f.enclosed_name() else { return Err(bad(&format!("a file outside the folder ({})", f.name().unwrap_or_default()))) };
        let out = to.join(rel);
        if f.is_dir() {
            fs::create_dir_all(&out).map_err(|e| bad(&e))?;
            continue;
        }
        if let Some(p) = out.parent() {
            fs::create_dir_all(p).map_err(|e| bad(&e))?;
        }
        let mut w = fs::File::create(&out).map_err(|e| bad(&e))?;
        io::copy(&mut f, &mut w).map_err(|e| bad(&e))?;
        #[cfg(unix)]
        if let Some(mode) = f.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            let _ = fs::set_permissions(&out, fs::Permissions::from_mode(mode & 0o777));
        }
    }
    let entries: Vec<PathBuf> = fs::read_dir(to).map_err(|e| bad(&e))?.flatten().map(|e| e.path()).collect();
    Ok(match entries.as_slice() {
        [one] if one.is_dir() => one.clone(),
        _ => to.to_path_buf(),
    })
}

fn files_under(root: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = fs::read_dir(root.join(rel)) else { return };
    for e in rd.flatten() {
        let r = rel.join(e.file_name());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            files_under(root, &r, out);
        } else {
            out.push(r);
        }
    }
}

/// A free name to move `target` aside to (an old file still in use from an earlier update keeps its name).
fn aside(target: &Path) -> PathBuf {
    let name = |i: u32| {
        let mut n = target.file_name().unwrap_or_default().to_os_string();
        n.push(if i == 0 { format!(".{OLD_EXT}") } else { format!(".{OLD_EXT}{i}") });
        target.with_file_name(n)
    };
    (0..100).map(name).find(|p| !p.exists() || fs::remove_file(p).is_ok()).unwrap_or_else(|| name(100))
}

/// Moves each file under `new` to the same place in `dir`, moving the file it replaces aside first.
/// Returns the files moved aside. On a failure everything moved so far goes back.
fn replace(dir: &Path, new: &Path, rename: &dyn Fn(&Path, &Path) -> io::Result<()>) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    files_under(new, Path::new(""), &mut files);
    files.sort();
    let mut moved: Vec<(PathBuf, Option<PathBuf>)> = Vec::new();
    for rel in &files {
        let target = dir.join(rel);
        let step = (|| -> io::Result<Option<PathBuf>> {
            if let Some(p) = target.parent() {
                fs::create_dir_all(p)?;
            }
            let old = if target.exists() {
                let o = aside(&target);
                rename(&target, &o)?;
                Some(o)
            } else {
                None
            };
            if let Err(e) = rename(&new.join(rel), &target) {
                if let Some(o) = &old {
                    let _ = fs::rename(o, &target);
                }
                return Err(e);
            }
            Ok(old)
        })();
        match step {
            Ok(old) => moved.push((target, old)),
            Err(e) => {
                for (t, old) in moved.iter().rev() {
                    let _ = match old {
                        Some(o) => fs::rename(o, t),
                        None => fs::remove_file(t),
                    };
                }
                return Err(format!("Cannot replace {} ({e}). The old version was put back.", rel.display()));
            }
        }
    }
    Ok(moved.into_iter().filter_map(|(_, o)| o).collect())
}

fn remember(dir: &Path, old: &[PathBuf]) {
    let list = dir.join(WORK).join(OLD_LIST);
    let mut text = fs::read_to_string(&list).unwrap_or_default();
    for p in old {
        text.push_str(&p.to_string_lossy());
        text.push('\n');
    }
    let _ = fs::write(&list, text);
}

/// Deletes the files an earlier update moved aside. `true` once nothing is left.
/// Only files of `dir` with the aside name are touched, whatever the list says.
pub fn cleanup(dir: &Path) -> bool {
    let work = dir.join(WORK);
    let list = work.join(OLD_LIST);
    let ours = |p: &Path| p.starts_with(dir) && p.file_name().is_some_and(|n| n.to_string_lossy().contains(&format!(".{OLD_EXT}")));
    let text = fs::read_to_string(&list).unwrap_or_default();
    let left: Vec<&str> = text
        .lines()
        .filter(|l| !l.is_empty())
        .filter(|l| {
            let p = Path::new(l);
            ours(p) && p.exists() && fs::remove_file(p).is_err()
        })
        .collect();
    if left.is_empty() {
        fs::remove_dir_all(&work).is_ok() || !work.exists()
    } else {
        let _ = fs::write(&list, left.join("\n") + "\n");
        false
    }
}

/// [`cleanup`] of the running program's folder, retried for a while in a thread: the old version may
/// still be closing when the new one starts.
pub fn cleanup_in_background() {
    let Some((dir, _)) = location() else { return };
    if !dir.join(WORK).is_dir() {
        return;
    }
    std::thread::spawn(move || {
        for _ in 0..30 {
            if cleanup(&dir) {
                break;
            }
            std::thread::sleep(Duration::from_millis(500));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::BufRead;

    fn temp(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("ffcut-install-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    fn make_zip(path: &Path, files: &[(&str, &str)]) {
        let mut w = zip::ZipWriter::new(fs::File::create(path).unwrap());
        for (name, text) in files {
            w.start_file(*name, zip::write::SimpleFileOptions::default()).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        w.finish().unwrap();
    }

    fn read(p: &Path) -> String {
        fs::read_to_string(p).unwrap_or_else(|e| format!("<{e}>"))
    }

    fn old_install(name: &str) -> PathBuf {
        let dir = temp(name);
        fs::write(dir.join("ffcut.exe"), "old exe").unwrap();
        fs::write(dir.join("libmpv-2.dll"), "old dll").unwrap();
        fs::write(dir.join("my notes.txt"), "mine").unwrap();
        dir
    }

    #[test]
    fn replaces_the_files_and_deletes_the_old_ones_later() {
        let dir = old_install("ok");
        let zip = temp("ok-zip").join("u.zip");
        make_zip(
            &zip,
            &[
                ("FFcut-9.0.0-windows-x64/ffcut.exe", "new exe"),
                ("FFcut-9.0.0-windows-x64/libmpv-2.dll", "new dll"),
                ("FFcut-9.0.0-windows-x64/doc/readme.txt", "hi"),
            ],
        );
        let steps = Mutex::new(Vec::new());
        install_zip(&zip, &dir, "ffcut.exe", &|s| steps.lock().unwrap().push(s)).unwrap();
        assert_eq!(*steps.lock().unwrap(), [Stage::Unpack, Stage::Replace]);
        assert_eq!(
            (read(&dir.join("ffcut.exe")), read(&dir.join("libmpv-2.dll")), read(&dir.join("doc/readme.txt"))),
            ("new exe".into(), "new dll".into(), "hi".into())
        );
        assert_eq!(read(&dir.join("ffcut.exe.ffcut-old")), "old exe");
        assert_eq!(read(&dir.join("my notes.txt")), "mine");
        assert!(!dir.join(WORK).join("new").exists());
        // a second update while the first old files are still there
        make_zip(&zip, &[("ffcut.exe", "newer exe")]);
        install_zip(&zip, &dir, "ffcut.exe", &|_| {}).unwrap();
        assert_eq!((read(&dir.join("ffcut.exe")), read(&dir.join("ffcut.exe.ffcut-old"))), ("newer exe".into(), "new exe".into()));
        // the next start
        assert!(cleanup(&dir));
        let mut names: Vec<String> = fs::read_dir(&dir).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        names.sort();
        assert_eq!(names, ["doc", "ffcut.exe", "libmpv-2.dll", "my notes.txt"]);
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(zip.parent().unwrap());
    }

    #[test]
    fn cleanup_deletes_only_moved_aside_files_of_the_folder() {
        let dir = old_install("clean");
        let elsewhere = temp("clean-other").join("x.ffcut-old");
        fs::write(&elsewhere, "not ours").unwrap();
        fs::create_dir_all(dir.join(WORK)).unwrap();
        let list = format!("{}\n{}\n{}\n", dir.join("ffcut.exe").display(), elsewhere.display(), dir.join("gone.ffcut-old").display());
        fs::write(dir.join(WORK).join(OLD_LIST), list).unwrap();
        assert!(cleanup(&dir));
        assert_eq!((read(&dir.join("ffcut.exe")), read(&elsewhere)), ("old exe".into(), "not ours".into()));
        assert!(!dir.join(WORK).exists());
        assert!(cleanup(&dir)); // nothing to do
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(elsewhere.parent().unwrap());
    }

    #[test]
    fn a_failure_puts_the_old_version_back() {
        let dir = old_install("fail");
        let new = temp("fail-new");
        for (n, t) in [("ffcut.exe", "new exe"), ("libmpv-2.dll", "new dll"), ("z.txt", "z")] {
            fs::write(new.join(n), t).unwrap();
        }
        // the 4th move (the new dll into place) fails
        let calls = Mutex::new(0);
        let rename = |a: &Path, b: &Path| {
            let mut n = calls.lock().unwrap();
            *n += 1;
            if *n == 4 {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "in use"))
            } else {
                fs::rename(a, b)
            }
        };
        let e = replace(&dir, &new, &rename).unwrap_err();
        assert!(e.contains("libmpv-2.dll") && e.contains("in use"), "{e}");
        assert_eq!((read(&dir.join("ffcut.exe")), read(&dir.join("libmpv-2.dll"))), ("old exe".into(), "old dll".into()));
        assert!(!dir.join("ffcut.exe.ffcut-old").exists() && !dir.join("libmpv-2.dll.ffcut-old").exists() && !dir.join("z.txt").exists());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&new);
    }

    #[test]
    fn refuses_a_wrong_or_unsafe_zip() {
        let dir = old_install("bad");
        let zip = temp("bad-zip").join("u.zip");
        make_zip(&zip, &[("FFcut/ffcut", "linux build")]);
        let e = install_zip(&zip, &dir, "ffcut.exe", &|_| {}).unwrap_err();
        assert!(e.contains("no ffcut.exe"), "{e}");
        make_zip(&zip, &[("ffcut.exe", "x"), ("../escape.txt", "x")]);
        let e = install_zip(&zip, &dir, "ffcut.exe", &|_| {}).unwrap_err();
        assert!(e.contains("outside"), "{e}");
        fs::write(&zip, "not a zip").unwrap();
        assert!(install_zip(&zip, &dir, "ffcut.exe", &|_| {}).is_err());
        assert_eq!(read(&dir.join("ffcut.exe")), "old exe");
        assert!(!dir.parent().unwrap().join("escape.txt").exists());
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(zip.parent().unwrap());
    }

    /// Serves `body` once over HTTP on a free local port.
    fn serve(body: Vec<u8>) -> String {
        let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}/FFcut.zip", l.local_addr().unwrap());
        std::thread::spawn(move || {
            let (mut s, _) = l.accept().unwrap();
            let mut r = io::BufReader::new(s.try_clone().unwrap());
            let mut line = String::new();
            while r.read_line(&mut line).is_ok_and(|n| n > 2) {
                line.clear();
            }
            let _ = write!(s, "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n", body.len());
            let _ = s.write_all(&body);
        });
        url
    }

    #[test]
    fn a_download_must_match_its_checksum() {
        let dir = temp("dl");
        let body = b"zip bytes".to_vec();
        let sha = "4c4b5f6dd2b4b6a1e0f4b8f2d69b7a0b4d3f0a8d6f3f8f0b5d2c1a9e8f7d6c5b".to_string();
        let real: String = Sha256::digest(&body).iter().map(|b| format!("{b:02x}")).collect();
        let asset = |url: String, sha256: String| Asset { name: "FFcut.zip".into(), url, size: 9, sha256 };
        download(&asset(serve(body.clone()), real.clone()), &dir.join("a.zip"), &|_| {}).unwrap();
        assert_eq!(fs::read(dir.join("a.zip")).unwrap(), body);
        let e = download(&asset(serve(body.clone()), sha), &dir.join("b.zip"), &|_| {}).unwrap_err();
        assert!(e.contains("checksum"), "{e}");
        let mut longer = body.clone();
        longer.extend_from_slice(b" and more");
        assert!(download(&asset(serve(longer), real), &dir.join("c.zip"), &|_| {}).is_err());
        let _ = fs::remove_dir_all(&dir);
    }
}
