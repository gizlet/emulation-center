//! Downloads a release with curl into the center's save folder on the card,
//! resuming a partial file, then checks its size and SHA-256 against the
//! Store's before anything installs it.

use crate::store::Release;
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

/// The running curl, so quitting the center can stop it.
static ACTIVE: AtomicU32 = AtomicU32::new(0);

/// Stops a download in progress; its partial file is resumed next time.
pub fn stop_active() {
    let pid = ACTIVE.swap(0, Ordering::SeqCst);
    #[cfg(unix)]
    if pid != 0 {
        unsafe { libc::kill(pid as i32, libc::SIGTERM) };
    }
    #[cfg(not(unix))]
    let _ = pid;
}

#[derive(Default)]
pub struct Progress {
    pub done: AtomicU64,
    pub total: AtomicU64,
}

impl Progress {
    pub fn fraction(&self) -> f32 {
        let total = self.total.load(Ordering::Relaxed);
        if total == 0 {
            return 0.0;
        }
        (self.done.load(Ordering::Relaxed) as f64 / total as f64).clamp(0.0, 1.0) as f32
    }
}

pub fn sha256_file(path: &Path, progress: Option<&Progress>) -> std::io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 20];
    let mut done = 0u64;
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        done += n as u64;
        if let Some(p) = progress {
            p.done.store(done, Ordering::Relaxed);
        }
    }
    Ok(hasher.finalize().iter().map(|b| format!("{b:02x}")).collect())
}

/// Free bytes on the filesystem holding `dir`.
#[cfg(unix)]
pub fn free_space(dir: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(dir.as_os_str().as_bytes()).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    (unsafe { libc::statvfs(path.as_ptr(), &mut st) } == 0).then(|| st.f_bavail as u64 * st.f_frsize as u64)
}

#[cfg(not(unix))]
pub fn free_space(_: &Path) -> Option<u64> {
    None
}

/// Space an install needs: the package, its unpacked copy beside the
/// installed version, and some room to spare.
pub fn space_needed(release: &Release) -> u64 {
    release.size.saturating_mul(2) + (64 << 20)
}

/// The partial download of `release` already in `dir`, which a retry resumes.
pub fn partial_bytes(release: &Release, dir: &Path) -> u64 {
    std::fs::metadata(dir.join(format!("{}.part", release.file))).map(|m| m.len().min(release.size)).unwrap_or(0)
}

/// Removes downloads in `dir` other than `release`'s.
pub fn remove_others(release: &Release, dir: &Path) {
    let keep = [dir.join(&release.file), dir.join(format!("{}.part", release.file))];
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            if !keep.contains(&entry.path()) {
                let _ = std::fs::remove_file(entry.path());
            }
        }
    }
}

pub enum Stage {
    Downloading,
    Verifying,
}

/// Downloads `release` into `dir` and returns the verified package's path.
pub fn fetch(release: &Release, dir: &Path, progress: &Progress, stage: &dyn Fn(Stage)) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(&release.file);
    let part = dir.join(format!("{}.part", release.file));
    remove_others(release, dir);
    progress.total.store(release.size, Ordering::Relaxed);
    let have = std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0);
    if have > release.size {
        let _ = std::fs::remove_file(&part);
    }
    stage(Stage::Downloading);
    if std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0) < release.size {
        let mut child = Command::new("curl")
            .args(["--fail", "--silent", "--show-error", "--location", "--retry", "5", "--retry-delay", "2"])
            .args(["--connect-timeout", "30", "--speed-limit", "1024", "--speed-time", "60", "--continue-at", "-"])
            .arg("--output")
            .arg(&part)
            .arg(&release.url)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("cannot run curl: {e}"))?;
        ACTIVE.store(child.id(), Ordering::SeqCst);
        let status = loop {
            progress.done.store(std::fs::metadata(&part).map(|m| m.len()).unwrap_or(0), Ordering::Relaxed);
            if let Some(status) = child.try_wait().map_err(|e| e.to_string())? {
                break status;
            }
            std::thread::sleep(Duration::from_millis(200));
        };
        ACTIVE.store(0, Ordering::SeqCst);
        if !status.success() {
            let mut err = String::new();
            if let Some(mut stderr) = child.stderr.take() {
                let _ = stderr.read_to_string(&mut err);
            }
            let err = err.trim().trim_start_matches("curl: ");
            return Err(if err.is_empty() { format!("download failed ({status})") } else { err.to_string() });
        }
    }
    stage(Stage::Verifying);
    let size = std::fs::metadata(&part).map_err(|e| e.to_string())?.len();
    let sha256 = sha256_file(&part, Some(progress)).map_err(|e| e.to_string())?;
    if size != release.size || sha256 != release.sha256 {
        let _ = std::fs::remove_file(&part);
        return Err(format!("{} did not match the Store's checksum", release.file));
    }
    std::fs::rename(&part, &dest).map_err(|e| e.to_string())?;
    Ok(dest)
}
