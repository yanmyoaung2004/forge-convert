//! Output naming + collision handling (spec §§15, 24).
//!
//! Default: input dir (or explicit dir), same basename + new extension.
//! Collisions: Rename-auto appends ` (1)`, ` (2)`, …; Replace overwrites;
//! Skip returns `None` (caller records a skip); Fail errors `OutputExists`.

use std::path::{Path, PathBuf};

use forge_core::{CollisionPolicy, ImageFormat, Result, canonical_stem_of, ForgeError};

/// Resolve `input` → output path under `output_dir` (or beside input).
/// Applies `policy` for existing paths.
pub fn resolve_output(
    input: &Path,
    format: ImageFormat,
    output_dir: Option<&Path>,
    policy: CollisionPolicy,
) -> Result<PathBuf> {
    let stem = canonical_stem_of(input);
    let file = format!("{stem}.{ext}", ext = format.extension());
    let base = match output_dir {
        Some(dir) => dir.join(file),
        None => input.with_file_name(file),
    };
    match apply_collision(&base, policy)? {
        Some(path) => Ok(path),
        None => Ok(base), // Skip: caller treats existing path as skipped
    }
}

/// Apply `policy` to `candidate`. Returns `None` for Skip-when-exists
/// (caller records the skip without writing).
pub fn apply_collision(candidate: &Path, policy: CollisionPolicy) -> Result<Option<PathBuf>> {
    if !candidate.exists() {
        return Ok(Some(candidate.to_path_buf()));
    }
    match policy {
        CollisionPolicy::Replace => Ok(Some(candidate.to_path_buf())),
        CollisionPolicy::Fail => Err(ForgeError::OutputExists(candidate.to_path_buf())),
        CollisionPolicy::Skip => Ok(None),
        CollisionPolicy::RenameAuto => Ok(Some(first_free_sibling(candidate))),
    }
}

/// `stem.webp` → `stem (1).webp`, `stem (2).webp`, … (bounded search).
fn first_free_sibling(candidate: &Path) -> PathBuf {
    let stem = candidate
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("output");
    let ext = candidate
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    let parent = candidate.parent();
    for index in 1..=9999 {
        let name = format!("{stem} ({index}){ext}");
        let path = match parent {
            Some(dir) if !dir.as_os_str().is_empty() => dir.join(&name),
            _ => PathBuf::from(&name),
        };
        if !path.exists() {
            return path;
        }
    }
    // Practically unreachable; fall back to a timestamped name.
    let fallback = format!(
        "{stem} ({}).{ext}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );
    match parent {
        Some(dir) if !dir.as_os_str().is_empty() => dir.join(fallback),
        _ => PathBuf::from(fallback),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Scratch {
        dir: PathBuf,
    }
    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir().join(format!("forgeconvert-naming-{name}"));
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
    fn test_default_naming_same_basename_new_ext() {
        let path = resolve_output(
            Path::new("/img/logo.png"),
            ImageFormat::Webp,
            None,
            CollisionPolicy::Replace,
        )
        .unwrap();
        assert_eq!(path, PathBuf::from("/img/logo.webp"));
    }

    #[test]
    fn test_rename_auto_increments() {
        let scratch = Scratch::new("rename");
        let first = scratch.dir.join("logo.webp");
        std::fs::write(&first, b"x").expect("seed");
        let second = resolve_output(
            Path::new("/img/logo.png"),
            ImageFormat::Webp,
            Some(&scratch.dir),
            CollisionPolicy::RenameAuto,
        )
        .unwrap();
        assert_eq!(second, scratch.dir.join("logo (1).webp"));
    }

    #[test]
    fn test_fail_and_skip_policies() {
        let scratch = Scratch::new("fail-skip");
        let existing = scratch.dir.join("a.webp");
        std::fs::write(&existing, b"x").expect("seed");
        assert!(matches!(
            apply_collision(&existing, CollisionPolicy::Fail),
            Err(ForgeError::OutputExists(_))
        ));
        assert_eq!(
            apply_collision(&existing, CollisionPolicy::Skip).unwrap(),
            None
        );
        assert_eq!(
            apply_collision(&existing, CollisionPolicy::Replace).unwrap(),
            Some(existing)
        );
    }
}
