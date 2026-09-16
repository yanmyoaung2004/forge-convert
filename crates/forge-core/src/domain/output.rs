//! Output targets: where converted bytes land.

use std::path::PathBuf;

/// Destination of one conversion output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OutputTarget {
    /// Write to an explicit file path.
    File(PathBuf),
    /// Write into a directory, deriving the file name from the input
    /// (same basename + new extension; see `output_path_for`).
    Directory(PathBuf),
}

impl OutputTarget {
    /// Resolve to a concrete file path for `input_stem` + `extension`.
    #[must_use]
    pub fn resolve(&self, input_stem: &str, extension: &str) -> PathBuf {
        match self {
            Self::File(path) => path.clone(),
            Self::Directory(dir) => dir.join(format!("{input_stem}.{extension}")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_output_target_resolve() {
        let dir = OutputTarget::Directory(PathBuf::from("/out"));
        assert_eq!(dir.resolve("logo", "webp"), PathBuf::from("/out/logo.webp"));
        let file = OutputTarget::File(PathBuf::from("/out/custom.png"));
        assert_eq!(
            file.resolve("logo", "webp"),
            PathBuf::from("/out/custom.png"),
            "explicit file wins over derivation"
        );
    }
}
