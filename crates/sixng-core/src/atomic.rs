//! Safe file writes for exports.
//!
//! [`write`] and [`AtomicFile`] write to a temp file in the target's folder
//! and rename it over the target when done. A reader sees the old file or
//! the new one, never a mix: a killed run leaves the old file (plus a temp
//! file that the next locked run of that folder removes), and two runs
//! writing one file leave one complete copy, not a short file followed by
//! the tail of a longer one.
//!
//! [`lock_dir`] takes a per-folder lock, so two runs (or two threads) can't
//! export into the same folder at once. The lock is held on an open handle,
//! so the OS drops it when the process dies.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// Name of the lock file [`lock_dir`] keeps in each locked folder.
pub const LOCK_NAME: &str = ".sixng.lock";

const TEMP_SUFFIX: &str = ".sixngtmp";

static SEQ: AtomicU64 = AtomicU64::new(0);

/// A temp name next to `target`: hidden, unique per process and call.
fn temp_path(target: &Path) -> PathBuf {
    let name = target.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    target.with_file_name(format!(".{name}.{}-{n}{TEMP_SUFFIX}", std::process::id()))
}

/// Drop-in for `std::fs::write`, done as temp file + rename.
pub fn write(path: impl AsRef<Path>, data: impl AsRef<[u8]>) -> io::Result<()> {
    let mut f = AtomicFile::create(path)?;
    f.write_all(data.as_ref())?;
    f.commit()
}

/// A buffered file that only replaces its target on [`AtomicFile::commit`].
/// Dropped without a commit, it removes its temp file and leaves the
/// target as it was.
pub struct AtomicFile {
    target: PathBuf,
    temp: PathBuf,
    w: Option<BufWriter<File>>,
}

impl AtomicFile {
    pub fn create(path: impl AsRef<Path>) -> io::Result<Self> {
        let target = path.as_ref().to_path_buf();
        let temp = temp_path(&target);
        let f = OpenOptions::new().write(true).create_new(true).open(&temp)?;
        Ok(Self { target, temp, w: Some(BufWriter::with_capacity(1 << 16, f)) })
    }

    pub fn path(&self) -> &Path {
        &self.target
    }

    /// Flush and rename over the target. There is no fsync: it costs
    /// several ms a file, and a killed process loses nothing written
    /// before the rename (only an OS crash could).
    pub fn commit(mut self) -> io::Result<()> {
        let w = self.w.take().expect("committed once");
        let r = w.into_inner().map_err(|e| e.into_error()).and_then(|f| {
            drop(f);
            rename_over(&self.temp, &self.target)
        });
        if r.is_err() {
            let _ = fs::remove_file(&self.temp);
        }
        r
    }
}

impl Write for AtomicFile {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.w.as_mut().expect("not committed").write(buf)
    }
    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.w.as_mut().expect("not committed").write_all(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.w.as_mut().expect("not committed").flush()
    }
}

impl Drop for AtomicFile {
    fn drop(&mut self) {
        if let Some(w) = self.w.take() {
            drop(w);
            let _ = fs::remove_file(&self.temp);
        }
    }
}

/// Rename `from` over `to`. On Windows the rename fails while a reader
/// (the viewer, a game) has `to` open without delete sharing, so retry
/// for a short while.
fn rename_over(from: &Path, to: &Path) -> io::Result<()> {
    let start = Instant::now();
    let mut wait = Duration::from_millis(5);
    loop {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied && start.elapsed() < Duration::from_secs(5) => {
                std::thread::sleep(wait);
                wait = (wait * 2).min(Duration::from_millis(250));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Holds a folder's lock until dropped.
#[derive(Debug)]
pub struct DirLock {
    _file: File,
}

/// Lock `dir` (created if missing) against other exports. Waits up to
/// `timeout` for another holder, then fails with `WouldBlock`.
pub fn lock_dir_timeout(dir: &Path, timeout: Duration) -> io::Result<DirLock> {
    fs::create_dir_all(dir)?;
    let file = OpenOptions::new().read(true).write(true).create(true).truncate(false).open(dir.join(LOCK_NAME))?;
    let start = Instant::now();
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(DirLock { _file: file }),
            Err(fs::TryLockError::WouldBlock) if start.elapsed() < timeout => std::thread::sleep(Duration::from_millis(50)),
            Err(fs::TryLockError::WouldBlock) => {
                return Err(io::Error::new(io::ErrorKind::WouldBlock, format!("{} is being written by another export", dir.display())))
            }
            Err(fs::TryLockError::Error(e)) => return Err(e),
        }
    }
}

/// [`lock_dir_timeout`] with a two-minute wait.
pub fn lock_dir(dir: &Path) -> io::Result<DirLock> {
    lock_dir_timeout(dir, Duration::from_secs(120))
}

/// Remove temp files left in `dir` by killed runs. Call it only while
/// holding the folder's [`DirLock`], so no live writer owns them.
pub fn clean_temps(dir: &Path) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let n = e.file_name();
        let n = n.to_string_lossy();
        if n.starts_with('.') && n.ends_with(TEMP_SUFFIX) {
            let _ = fs::remove_file(e.path());
        }
    }
}

/// True if `path` holds valid JSON. A truncated, zero-filled or doubled
/// file reads as missing, so a re-run rewrites it.
pub fn json_ok(path: &Path) -> bool {
    let Ok(f) = File::open(path) else { return false };
    serde_json::from_reader::<_, serde::de::IgnoredAny>(io::BufReader::new(f)).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmpdir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sixng_atomic_{name}_{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn overlapping_writers_leave_one_whole_file() {
        let d = tmpdir("overlap");
        let p = d.join("model.json");
        let long = serde_json::to_vec(&serde_json::json!({ "pad": "x".repeat(200_000) })).unwrap();
        let short = serde_json::to_vec(&serde_json::json!({ "short": true })).unwrap();
        write(&p, &long).unwrap();
        // Many threads replace the same file with a long and a short copy at
        // once; plain fs::write can leave the short copy plus the long tail.
        std::thread::scope(|s| {
            for i in 0..16 {
                let (p, long, short) = (&p, &long, &short);
                s.spawn(move || {
                    for _ in 0..20 {
                        write(p, if i % 2 == 0 { long } else { short }).unwrap();
                    }
                });
            }
        });
        let got = fs::read(&p).unwrap();
        assert!(got == long || got == short, "mixed file of {} bytes", got.len());
        assert!(json_ok(&p));
        let left: Vec<_> = fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name()).collect();
        assert_eq!(left.len(), 1, "temp files left: {left:?}");
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn killed_write_keeps_old_file() {
        let d = tmpdir("killed");
        let p = d.join("model.json");
        write(&p, br#"{"old":1}"#).unwrap();
        // A run killed mid-write: the temp file is never committed or
        // dropped (as when the process dies).
        let mut f = AtomicFile::create(&p).unwrap();
        f.write_all(&[0u8; 4096]).unwrap();
        f.flush().unwrap();
        std::mem::forget(f);
        assert_eq!(fs::read(&p).unwrap(), br#"{"old":1}"#);
        // The next run takes the lock and clears the leftover.
        let _l = lock_dir(&d).unwrap();
        clean_temps(&d);
        let names: Vec<String> = fs::read_dir(&d).unwrap().flatten().map(|e| e.file_name().to_string_lossy().into_owned()).collect();
        assert!(names.iter().all(|n| !n.ends_with(TEMP_SUFFIX)), "{names:?}");
        drop(_l);
        // A write dropped before commit (an error part way) also keeps the old file.
        let mut f = AtomicFile::create(&p).unwrap();
        f.write_all(b"{\"half").unwrap();
        drop(f);
        assert_eq!(fs::read(&p).unwrap(), br#"{"old":1}"#);
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn bad_json_reads_as_missing() {
        let d = tmpdir("json");
        let p = d.join("model.json");
        assert!(!json_ok(&p));
        fs::write(&p, vec![0u8; 512]).unwrap();
        assert!(!json_ok(&p), "zero-filled");
        fs::write(&p, br#"{"a":1}"#.iter().chain(b"ld\": 2}\n}").copied().collect::<Vec<u8>>()).unwrap();
        assert!(!json_ok(&p), "stale tail");
        fs::write(&p, br#"{"a":1}"#).unwrap();
        assert!(json_ok(&p));
        let _ = fs::remove_dir_all(&d);
    }

    #[test]
    fn second_lock_waits_then_fails() {
        let d = tmpdir("lock");
        let a = lock_dir(&d).unwrap();
        let e = lock_dir_timeout(&d, Duration::from_millis(100)).unwrap_err();
        assert_eq!(e.kind(), io::ErrorKind::WouldBlock);
        drop(a);
        lock_dir_timeout(&d, Duration::from_millis(100)).unwrap();
        let _ = fs::remove_dir_all(&d);
    }
}
