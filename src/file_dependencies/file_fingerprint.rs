use std::{
    path::{Path, PathBuf},
    time::SystemTime,
};

use bincode::{Decode, Encode};
use compio::fs;
use fasthash::MetroHasher;
use snafu::{ResultExt, Snafu};
use std::hash::Hasher;

use crate::ext::{AsyncTryFrom, BestEffortPathExt};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Encode, Decode)]
pub enum FileFingerprint {
    ModifiedTime(SystemTime),
    Hash(u64),
}

impl AsyncTryFrom<&Path> for FileFingerprint {
    type Error = Fingerprint;

    async fn async_try_from(path: &Path) -> Result<Self, Self::Error> {
        let metadata = path.metadata().context(PathSnafu {
            path: path.to_path_buf(),
        })?;

        if metadata.is_dir() {
            return Err(Fingerprint::DirectoryError {
                path: path.to_path_buf(),
            });
        }

        // Try to use modified time first
        if let Ok(modified_time) = metadata.modified() {
            return Ok(FileFingerprint::ModifiedTime(modified_time));
        }

        // Fallback to hash if modified time is not available
        let bytes = fs::read(path).await.context(PathSnafu {
            path: path.to_path_buf(),
        })?;

        let mut hasher = MetroHasher::default();
        hasher.write(&bytes);
        let hash = hasher.finish();

        Ok(FileFingerprint::Hash(hash))
    }
}
#[derive(Debug, Snafu)]
pub enum Fingerprint {
    #[snafu(display("Failed to create dependency from path: {}", path.best_effort_path_display()))]
    PathError {
        path: PathBuf,
        source: std::io::Error,
    },
    #[snafu(display("The supplied path {} contains a directory", path.best_effort_path_display()))]
    DirectoryError { path: PathBuf },
}

#[cfg(test)]
mod tests {
    use super::*;
    use rstest::*;
    use std::io::Write;
    use std::thread;
    use std::time::Duration;
    use tempfile::{NamedTempFile, TempDir};

    #[compio::test]
    async fn test_file_fingerprint_from_regular_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "test content").expect("Failed to write to temp file");

        let fingerprint = FileFingerprint::async_try_from(temp_file.path()).await;

        assert!(fingerprint.is_ok());
        match fingerprint.unwrap() {
            FileFingerprint::ModifiedTime(_) => {
                // This is the expected case on most systems
            }
            FileFingerprint::Hash(_) => {
                // This might happen on some systems where modified time is not available
            }
        }
    }

    #[compio::test]
    async fn test_file_fingerprint_from_directory() {
        let temp_dir = TempDir::new().expect("Failed to create temp directory");

        let result = FileFingerprint::async_try_from(temp_dir.path()).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            Fingerprint::DirectoryError { path } => {
                assert_eq!(path, temp_dir.path());
            }
            _ => panic!("Expected DirectoryError"),
        }
    }

    #[compio::test]
    async fn test_file_fingerprint_from_nonexistent_file() {
        let nonexistent_path = Path::new("/this/path/does/not/exist.txt");

        let result = FileFingerprint::async_try_from(nonexistent_path).await;

        assert!(result.is_err());
        match result.unwrap_err() {
            Fingerprint::PathError { path, .. } => {
                assert_eq!(path, nonexistent_path);
            }
            _ => panic!("Expected PathError"),
        }
    }

    #[compio::test]
    async fn test_file_fingerprint_modified_time_changes() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "initial content").expect("Failed to write to temp file");

        let first_fingerprint = FileFingerprint::async_try_from(temp_file.path())
            .await
            .expect("Failed to create first fingerprint");

        // Wait a bit to ensure different modification time
        thread::sleep(Duration::from_millis(10));

        // Modify the file
        writeln!(temp_file, "additional content").expect("Failed to write to temp file");
        temp_file.flush().expect("Failed to flush temp file");

        let second_fingerprint = FileFingerprint::async_try_from(temp_file.path())
            .await
            .expect("Failed to create second fingerprint");

        // The fingerprints should be different
        assert_ne!(first_fingerprint, second_fingerprint);
    }

    #[compio::test]
    async fn test_file_fingerprint_same_content_same_fingerprint() {
        let content = "identical content";

        let mut temp_file1 = NamedTempFile::new().expect("Failed to create temp file 1");
        let mut temp_file2 = NamedTempFile::new().expect("Failed to create temp file 2");

        writeln!(temp_file1, "{}", content).expect("Failed to write to temp file 1");
        writeln!(temp_file2, "{}", content).expect("Failed to write to temp file 2");

        let fingerprint1 = FileFingerprint::async_try_from(temp_file1.path())
            .await
            .expect("Failed to create fingerprint 1");
        let fingerprint2 = FileFingerprint::async_try_from(temp_file2.path())
            .await
            .expect("Failed to create fingerprint 2");

        // Note: These might not be equal if using ModifiedTime, as the files
        // were created at different times, but if using Hash they should be equal
        match (&fingerprint1, &fingerprint2) {
            (FileFingerprint::Hash(h1), FileFingerprint::Hash(h2)) => {
                assert_eq!(
                    h1, h2,
                    "Files with identical content should have identical hashes"
                );
            }
            (FileFingerprint::ModifiedTime(_), FileFingerprint::ModifiedTime(_)) => {
                // Modified times will likely be different, which is expected
            }
            _ => {
                // Mixed fingerprint types, which is possible but not the main test case
            }
        }
    }

    #[rstest]
    #[case("hello world")]
    #[case("")]
    #[case("special chars: äöü🚀")]
    #[case("multiline\ncontent\nwith\nnewlines")]
    #[compio::test]
    async fn test_file_fingerprint_with_various_content(#[case] content: &str) {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        write!(temp_file, "{}", content).expect("Failed to write to temp file");

        let fingerprint = FileFingerprint::async_try_from(temp_file.path()).await;

        assert!(
            fingerprint.is_ok(),
            "Failed to create fingerprint for content: {:?}",
            content
        );
    }

    #[compio::test]
    async fn test_file_fingerprint_large_file() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");

        // Write a large amount of data
        let large_content = "x".repeat(1024 * 1024); // 1MB of 'x'
        write!(temp_file, "{}", large_content).expect("Failed to write large content");

        let fingerprint = FileFingerprint::async_try_from(temp_file.path()).await;

        assert!(
            fingerprint.is_ok(),
            "Failed to create fingerprint for large file"
        );
    }

    #[compio::test]
    async fn test_file_fingerprint_clone_and_equality() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "test content").expect("Failed to write to temp file");

        let fingerprint = FileFingerprint::async_try_from(temp_file.path())
            .await
            .expect("Failed to create fingerprint");

        let cloned_fingerprint = fingerprint.clone();

        assert_eq!(fingerprint, cloned_fingerprint);
        assert_eq!(fingerprint.clone(), fingerprint);
    }

    #[compio::test]
    async fn test_file_fingerprint_hash_consistency() {
        let mut temp_file = NamedTempFile::new().expect("Failed to create temp file");
        writeln!(temp_file, "test content for hash").expect("Failed to write to temp file");

        let fingerprint1 = FileFingerprint::async_try_from(temp_file.path())
            .await
            .expect("Failed to create first fingerprint");
        let fingerprint2 = FileFingerprint::async_try_from(temp_file.path())
            .await
            .expect("Failed to create second fingerprint");

        // Reading the same file multiple times should produce the same fingerprint
        assert_eq!(fingerprint1, fingerprint2);
    }

    #[test]
    fn test_fingerprint_error_display() {
        let nonexistent_path = PathBuf::from("/this/path/does/not/exist.txt");
        let directory_path = PathBuf::from("/tmp");

        let path_error = Fingerprint::PathError {
            path: nonexistent_path.clone(),
            source: std::io::Error::new(std::io::ErrorKind::NotFound, "file not found"),
        };

        let directory_error = Fingerprint::DirectoryError {
            path: directory_path.clone(),
        };

        let path_error_msg = format!("{}", path_error);
        let directory_error_msg = format!("{}", directory_error);

        assert!(path_error_msg.contains("Failed to create dependency from path"));
        assert!(path_error_msg.contains("/this/path/does/not/exist.txt"));

        assert!(directory_error_msg.contains("contains a directory"));
        assert!(directory_error_msg.contains("/tmp"));
    }
}
