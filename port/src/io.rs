//! Asset file access for the core: every read of the game's files (HDB, movies, PFF pictures, DLL / EXE resources,
//! fonts) goes through one installable backend. Native default = `std::fs` (behaviour unchanged); the web front
//! (`port/web/`) installs a backend that fetches the files from the local `xf-serve` over HTTP Range requests.
//!
//! A backend that sets [`Fs::ranged`] is read by ranges where the core can avoid whole-file reads (movie headers and
//! video samples, [`crate::media::MoviePlayer`]); with `std::fs` the core reads whole files as before.
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// One directory entry ([`Fs::read_dir`]).
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    pub path: PathBuf,
    pub is_dir: bool,
}

pub trait Fs: Send + Sync {
    /// The whole file.
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>>;
    /// The entries of a directory (any order).
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>>;
    /// File size in bytes.
    fn size(&self, p: &Path) -> std::io::Result<u64>;
    /// Up to `buf.len()` bytes at `off`; returns the count read (short only at the end of the file).
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize>;
    /// Whether the core should prefer range reads over whole-file reads (remote files).
    fn ranged(&self) -> bool {
        false
    }
    /// Whether `p` names a file.
    fn is_file(&self, p: &Path) -> bool {
        self.size(p).is_ok()
    }
    /// Replace the file `p` with `bytes` atomically (the save slots, H402); read-only backends refuse.
    fn write(&self, p: &Path, _bytes: &[u8]) -> std::io::Result<()> {
        Err(std::io::Error::new(std::io::ErrorKind::Unsupported, format!("{}: read-only file access", p.display())))
    }
}

/// `std::fs`.
pub struct StdFs;

impl Fs for StdFs {
    fn read(&self, p: &Path) -> std::io::Result<Vec<u8>> {
        std::fs::read(p)
    }
    fn read_dir(&self, p: &Path) -> std::io::Result<Vec<Entry>> {
        Ok(std::fs::read_dir(p)?
            .flatten()
            .map(|e| {
                let path = e.path();
                Entry { name: e.file_name().to_string_lossy().into_owned(), is_dir: path.is_dir(), path }
            })
            .collect())
    }
    fn size(&self, p: &Path) -> std::io::Result<u64> {
        Ok(std::fs::metadata(p)?.len())
    }
    fn is_file(&self, p: &Path) -> bool {
        p.is_file()
    }
    /// The parent directories are created; the bytes go to a temporary file next to `p`, renamed over `p`.
    fn write(&self, p: &Path, bytes: &[u8]) -> std::io::Result<()> {
        write_atomic(p, bytes)
    }
    fn read_at(&self, p: &Path, off: u64, buf: &mut [u8]) -> std::io::Result<usize> {
        use std::io::{Read, Seek, SeekFrom};
        let mut f = std::fs::File::open(p)?;
        f.seek(SeekFrom::Start(off))?;
        let mut n = 0;
        while n < buf.len() {
            match f.read(&mut buf[n..])? {
                0 => break,
                k => n += k,
            }
        }
        Ok(n)
    }
}

/// `bytes` to a temporary file in `p`'s directory (created if missing), then renamed over `p`: a reader sees the old
/// file or the new one, never a half-written one.
pub fn write_atomic(p: &Path, bytes: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let dir = p.parent().filter(|d| !d.as_os_str().is_empty()).unwrap_or(Path::new("."));
    std::fs::create_dir_all(dir)?;
    let name = p.file_name().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::InvalidInput, "no file name"))?;
    let tmp = dir.join(format!(".{}.{}-{}.tmp", name.to_string_lossy(), std::process::id(), SEQ.fetch_add(1, Ordering::Relaxed)));
    let res = (|| {
        let mut f = std::fs::File::create(&tmp)?;
        f.write_all(bytes)?;
        f.sync_all()?;
        std::fs::rename(&tmp, p)
    })();
    if res.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    res
}

static FS: OnceLock<Box<dyn Fs>> = OnceLock::new();

/// Install the backend (once, before the first read); false if one is installed already.
pub fn install(fs: Box<dyn Fs>) -> bool {
    FS.set(fs).is_ok()
}

pub fn fs() -> &'static dyn Fs {
    FS.get_or_init(|| Box::new(StdFs)).as_ref()
}

pub fn read(p: &Path) -> std::io::Result<Vec<u8>> {
    fs().read(p)
}

pub fn read_dir(p: &Path) -> std::io::Result<Vec<Entry>> {
    fs().read_dir(p)
}

pub fn is_file(p: &Path) -> bool {
    fs().is_file(p)
}

/// Replace the file `p` with `bytes` ([`Fs::write`]).
pub fn write(p: &Path, bytes: &[u8]) -> std::io::Result<()> {
    fs().write(p, bytes)
}

pub fn ranged() -> bool {
    fs().ranged()
}

/// `len` bytes at `off` (shorter at the end of the file).
pub fn read_range(p: &Path, off: u64, len: usize) -> std::io::Result<Vec<u8>> {
    let mut b = vec![0; len];
    let n = fs().read_at(p, off, &mut b)?;
    b.truncate(n);
    Ok(b)
}

#[cfg(test)]
mod tests {
    #[test]
    fn std_read_at() {
        let p = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
        let all = super::read(&p).unwrap();
        assert_eq!(super::read_range(&p, 3, 10).unwrap(), all[3..13]);
        assert_eq!(super::read_range(&p, all.len() as u64 - 2, 10).unwrap(), all[all.len() - 2..]);
        assert_eq!(super::fs().size(&p).unwrap(), all.len() as u64);
    }

    #[test]
    fn atomic_write_replaces() {
        let d = std::env::temp_dir().join(format!("xf-io-write-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let p = d.join("sub/a.x");
        super::write(&p, b"one").unwrap();
        super::write(&p, b"two!").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"two!");
        assert!(super::is_file(&p) && !super::is_file(&d.join("sub")));
        // no temporary file left behind
        assert_eq!(std::fs::read_dir(d.join("sub")).unwrap().count(), 1);
        let _ = std::fs::remove_dir_all(&d);
    }
}
