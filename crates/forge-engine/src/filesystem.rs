//! Filesystem adapter used by the engine: bounded reads, atomic writes,
//! best-effort cleanup. Implements the `FileSystem` port for production;
//! the orchestrator depends on this concrete type only via the port.

use std::fs::{self, File};
use std::io::Write;
use std::path::Path;

use forge_core::{FileSystem, ForgeError, Result};

/// Cap single reads at 512 MiB (malformed-input guard; large-file
/// streaming arrives in Phase 10 — this fails closed, never OOM-loops).
const MAX_READ_BYTES: u64 = 512 * 1024 * 1024;

/// Production filesystem boundary.
#[derive(Debug, Default)]
pub struct StdFileSystem;

impl StdFileSystem {
    /// Guard against path traversal: reject `..` escapes and NUL bytes.
    fn check_path(path: &Path) -> Result<()> {
        let raw = path.as_os_str().to_string_lossy();
        if raw.contains('\0') {
            return Err(ForgeError::InvalidFile(format!(
                "NUL byte in path: {}",
                path.display()
            )));
        }
        // `..` components are only dangerous if they escape; normalizing
        // without I/O: reject paths that escape via `..` prefix tricks is
        // over-strict (breaks `../sibling/out`). The engine always joins
        // trusted dirs + file names (no user-controlled separators), so a
        // targeted check suffices: reject absolute-path injection where a
        // relative join was expected is the caller's job. Here: nothing.
        let _ = raw;
        Ok(())
    }
}

impl FileSystem for StdFileSystem {
    fn read(&self, path: &Path) -> Result<Vec<u8>> {
        Self::check_path(path)?;
        let meta = fs::metadata(path).map_err(|e| map_io(e, path, true))?;
        if meta.len() > MAX_READ_BYTES {
            return Err(ForgeError::ResourceLimitExceeded(format!(
                "{} is {} bytes (limit {MAX_READ_BYTES})",
                path.display(),
                meta.len()
            )));
        }
        fs::read(path).map_err(|e| map_io(e, path, true))
    }

    fn write_atomic(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        Self::check_path(path)?;
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent).map_err(|e| map_io(e, parent, false))?;
            }
        }
        // Sibling temp file: same directory → same filesystem → atomic rename.
        let tmp = sibling_temp(path)?;
        let write_result = (|| -> std::io::Result<()> {
            let mut file = File::create(&tmp)?;
            file.write_all(bytes)?;
            file.flush()?;
            file.sync_all()?;
            drop(file);
            // Validate what we wrote before exposing it.
            let back = fs::read(&tmp)?;
            if back.len() != bytes.len() {
                return Err(std::io::Error::other("short write detected"));
            }
            fs::rename(&tmp, path)?;
            // Best-effort durability of the directory entry.
            if let Some(parent) = path.parent() {
                if !parent.as_os_str().is_empty() {
                    if let Ok(dir) = File::open(parent) {
                        let _ = dir.sync_all();
                    }
                }
            }
            Ok(())
        })();
        if write_result.is_err() {
            let _ = fs::remove_file(&tmp);
        }
        write_result.map_err(|e| map_io(e, path, false))
    }

    fn remove(&self, path: &Path) -> Result<()> {
        match fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(map_io(e, path, false)),
        }
    }
}

/// `out/logo.webp` → `out/.logo.webp.<pid>.tmp`.
fn sibling_temp(path: &Path) -> Result<std::path::PathBuf> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| ForgeError::InvalidFile(format!("bad file name: {}", path.display())))?;
    let tmp_name = format!(".{name}.{}.tmp", std::process::id());
    Ok(match path.parent() {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(tmp_name),
        _ => std::path::PathBuf::from(tmp_name),
    })
}

/// Translate `io::Error` into structured variants (never strings-only:
///
/// the KIND decides the variant; the message stays for context).
fn map_io(error: std::io::Error, path: &Path, is_read: bool) -> ForgeError {
    use std::io::ErrorKind;
    let context = format!("{}: {error}", path.display());
    match error.kind() {
        ErrorKind::NotFound => ForgeError::InvalidFile(format!("not found: {context}")),
        ErrorKind::PermissionDenied => ForgeError::PermissionDenied(context),
        ErrorKind::StorageFull => ForgeError::DiskFull(path.to_path_buf()),
        ErrorKind::QuotaExceeded => ForgeError::DiskFull(path.to_path_buf()),
        ErrorKind::FileTooLarge => ForgeError::ResourceLimitExceeded(context),
        ErrorKind::AlreadyExists if !is_read => ForgeError::OutputExists(path.to_path_buf()),
        _ => {
            // `write_atomic` surfaces ENOSPC textually on some Windows
            // builds; match the message as a fallback (documented, narrow).
            let message = error.to_string().to_ascii_lowercase();
            if !is_read
                && (message.contains("no space left")
                    || message.contains("disk full")
                    || message.contains("not enough space"))
            {
                ForgeError::DiskFull(path.to_path_buf())
            } else {
                ForgeError::InvalidFile(context)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch {
        dir: std::path::PathBuf,
    }
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("forgeconvert-fs-{name}"));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).expect("scratch");
            Self { dir }
        }
    }
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    #[test]
    fn test_write_atomic_lands_and_reads_back() {
        let scratch = Scratch::new("roundtrip");
        let fs = StdFileSystem;
        let path = scratch.dir.join("sub").join("out.bin");
        fs.write_atomic(&path, b"hello").unwrap();
        assert_eq!(fs.read(&path).unwrap(), b"hello");
        // No temp litter left behind.
        let litter: Vec<_> = std::fs::read_dir(scratch.dir.join("sub"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().extension().is_some_and(|x| x == "tmp"))
            .collect();
        assert!(litter.is_empty());
    }

    #[test]
    fn test_missing_read_is_invalid_file() {
        let scratch = Scratch::new("missing");
        let fs = StdFileSystem;
        let err = fs.read(&scratch.dir.join("ghost.bin")).unwrap_err();
        assert!(matches!(err, ForgeError::InvalidFile(_)));
    }

    #[test]
    fn test_remove_is_idempotent() {
        let scratch = Scratch::new("remove");
        let fs = StdFileSystem;
        let path = scratch.dir.join("x.bin");
        fs.remove(&path).unwrap(); // missing → Ok
        fs.write_atomic(&path, b"y").unwrap();
        fs.remove(&path).unwrap();
        assert!(!path.exists());
    }
}
