use core::{fmt, num::NonZeroU64};
use std::{io, path::Path};

use cogniform_assets::{AssetLimits, content_hash};
use cogniform_protocol::ContentHash;

use crate::{
    PartialFileCleanup,
    file_io::{FileError, FileOperation, create_new_synced, read_bounded_file},
};

/// Filesystem operation associated with a path-redacted asset-file failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssetFileOperation {
    /// Inspect the final path component before opening it.
    Inspect,
    /// Open a caller-selected path for bounded loading.
    OpenRead,
    /// Read the complete declared file contents.
    Read,
    /// Create a caller-selected new path without overwriting.
    CreateNew,
    /// Write the complete exact asset source.
    Write,
    /// Synchronize the new file contents and metadata.
    Sync,
}

impl fmt::Display for AssetFileOperation {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Inspect => "inspect",
            Self::OpenRead => "open for read",
            Self::Read => "read",
            Self::CreateNew => "create new",
            Self::Write => "write",
            Self::Sync => "sync",
        })
    }
}

impl From<FileOperation> for AssetFileOperation {
    fn from(value: FileOperation) -> Self {
        match value {
            FileOperation::Inspect => Self::Inspect,
            FileOperation::OpenRead => Self::OpenRead,
            FileOperation::Read => Self::Read,
            FileOperation::CreateNew => Self::CreateNew,
            FileOperation::Write => Self::Write,
            FileOperation::Sync => Self::Sync,
        }
    }
}

/// Explicit result of successfully creating one immutable asset-source file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFileReceipt {
    /// Exact SHA-256 identity verified before file creation.
    pub content_hash: ContentHash,
    /// Complete synchronized source byte count.
    pub source_bytes: u64,
}

/// Bounded, path-redacted asset-source file failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum AssetFileError {
    /// Source bytes exceed the configured per-file bound.
    SourceSizeExceeded {
        /// Supplied or observed byte count.
        actual: u64,
        /// Maximum accepted byte count on this platform and configuration.
        limit: u64,
    },
    /// Complete source bytes do not match the caller's expected identity.
    ContentHashMismatch {
        /// Identity required by the caller or logical scene reference.
        expected: ContentHash,
        /// Identity computed from the complete supplied or loaded bytes.
        actual: ContentHash,
    },
    /// A filesystem operation failed without retaining the caller path.
    Io {
        /// Operation that failed.
        operation: AssetFileOperation,
        /// Stable standard-library error category.
        kind: io::ErrorKind,
        /// Disposition of a file created by this call, if any.
        cleanup: PartialFileCleanup,
    },
    /// The selected load target is not a regular file.
    NotRegularFile,
    /// The file grew after its bounded metadata snapshot was accepted.
    FileChangedDuringRead {
        /// File byte count accepted before allocation.
        expected: u64,
    },
    /// The declared bounded allocation could not be reserved.
    AllocationFailed {
        /// Exact requested byte capacity.
        requested: u64,
    },
}

impl fmt::Display for AssetFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceSizeExceeded { actual, limit } => {
                write!(
                    formatter,
                    "asset source has {actual} bytes; file limit is {limit}"
                )
            }
            Self::ContentHashMismatch { expected, actual } => write!(
                formatter,
                "asset source hash mismatch: expected {expected}, computed {actual}"
            ),
            Self::Io {
                operation,
                kind,
                cleanup,
            } => {
                write!(formatter, "asset file {operation} failed with {kind:?}")?;
                match cleanup {
                    PartialFileCleanup::NotRequired => Ok(()),
                    PartialFileCleanup::Removed => formatter.write_str("; partial file removed"),
                    PartialFileCleanup::Retained { kind } => write!(
                        formatter,
                        "; partial file may remain because cleanup failed with {kind:?}"
                    ),
                }
            }
            Self::NotRegularFile => {
                formatter.write_str("asset file load target is not a regular file")
            }
            Self::FileChangedDuringRead { expected } => write!(
                formatter,
                "asset file grew after its accepted {expected}-byte metadata snapshot"
            ),
            Self::AllocationFailed { requested } => write!(
                formatter,
                "asset file could not reserve its bounded {requested}-byte buffer"
            ),
        }
    }
}

impl std::error::Error for AssetFileError {}

impl From<FileError> for AssetFileError {
    fn from(value: FileError) -> Self {
        match value {
            FileError::Io {
                operation,
                kind,
                cleanup,
            } => Self::Io {
                operation: operation.into(),
                kind,
                cleanup,
            },
            FileError::NotRegularFile => Self::NotRegularFile,
            FileError::SizeExceeded { actual, limit } => Self::SourceSizeExceeded { actual, limit },
            FileError::ChangedDuringRead { expected } => Self::FileChangedDuringRead { expected },
            FileError::AllocationFailed { requested } => Self::AllocationFailed { requested },
        }
    }
}

/// Explicit create-new and exact-hash bounded-load adapter for asset sources.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AssetFileStore {
    max_source_bytes: NonZeroU64,
}

impl AssetFileStore {
    /// Creates an adapter with one explicit per-file source-byte bound.
    #[must_use]
    pub const fn new(max_source_bytes: NonZeroU64) -> Self {
        Self { max_source_bytes }
    }

    /// Returns the maximum source bytes accepted by this adapter.
    #[must_use]
    pub const fn max_source_bytes(&self) -> NonZeroU64 {
        self.max_source_bytes
    }

    /// Creates and synchronizes one immutable exact-hash asset-source file.
    ///
    /// Size and SHA-256 identity validation complete before the filesystem is
    /// touched. The target and parent directory must already be selected and
    /// authorized by the caller. Existing targets are never overwritten.
    pub fn create_new(
        &self,
        path: &Path,
        expected_hash: ContentHash,
        source: &[u8],
    ) -> Result<AssetFileReceipt, AssetFileError> {
        let source_bytes = u64::try_from(source.len()).unwrap_or(u64::MAX);
        self.validate_size(source_bytes)?;
        validate_hash(expected_hash, source)?;
        create_new_synced(path, source)?;
        Ok(AssetFileReceipt {
            content_hash: expected_hash,
            source_bytes,
        })
    }

    /// Loads one complete regular-file source and verifies its expected identity.
    ///
    /// The final path component is rejected when it is a symlink at inspection
    /// time. Parent-directory trust and path-race protection remain caller
    /// responsibilities. No partial or hash-mismatched bytes are returned.
    pub fn load(&self, path: &Path, expected_hash: ContentHash) -> Result<Vec<u8>, AssetFileError> {
        let source = read_bounded_file(path, self.max_source_bytes.get())?;
        validate_hash(expected_hash, &source)?;
        Ok(source)
    }

    fn validate_size(self, actual: u64) -> Result<(), AssetFileError> {
        let limit = self.max_source_bytes.get();
        if actual > limit {
            return Err(AssetFileError::SourceSizeExceeded { actual, limit });
        }
        Ok(())
    }
}

impl Default for AssetFileStore {
    fn default() -> Self {
        Self::new(AssetLimits::default().max_source_bytes)
    }
}

fn validate_hash(expected: ContentHash, source: &[u8]) -> Result<(), AssetFileError> {
    let actual = content_hash(source);
    if actual != expected {
        return Err(AssetFileError::ContentHashMismatch { expected, actual });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use core::sync::atomic::{AtomicU64, Ordering};
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        path::{Path, PathBuf},
    };

    use crate::file_io::{SyncFile, persist_created_file, read_file_snapshot};

    use super::*;

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn create_new_round_trips_exact_source_and_never_overwrites() {
        let directory = TestDirectory::new("round-trip");
        let path = directory.path().join("asset.glb");
        let store = AssetFileStore::default();
        let source = b"immutable asset fixture";
        let hash = content_hash(source);
        let receipt = store.create_new(&path, hash, source).unwrap();
        assert_eq!(receipt.content_hash, hash);
        assert_eq!(receipt.source_bytes, source.len() as u64);
        assert_eq!(store.load(&path, hash).unwrap(), source);

        let original = fs::read(&path).unwrap();
        let replacement = b"different valid source";
        let error = store
            .create_new(&path, content_hash(replacement), replacement)
            .unwrap_err();
        assert!(matches!(
            error,
            AssetFileError::Io {
                operation: AssetFileOperation::CreateNew,
                kind: io::ErrorKind::AlreadyExists,
                cleanup: PartialFileCleanup::NotRequired,
            }
        ));
        assert_eq!(fs::read(path).unwrap(), original);
    }

    #[test]
    fn preflight_rejects_size_hash_and_missing_parent_without_hidden_work() {
        let directory = TestDirectory::new("preflight");
        let store = AssetFileStore::new(NonZeroU64::new(4).unwrap());
        let oversized = directory.path().join("oversized.glb");
        assert_eq!(
            store.create_new(&oversized, content_hash(b"12345"), b"12345"),
            Err(AssetFileError::SourceSizeExceeded {
                actual: 5,
                limit: 4,
            })
        );
        assert!(!oversized.exists());

        let mismatched = directory.path().join("mismatched.glb");
        assert!(matches!(
            store.create_new(&mismatched, content_hash(b"other"), b"1234"),
            Err(AssetFileError::ContentHashMismatch { .. })
        ));
        assert!(!mismatched.exists());

        let full_store = AssetFileStore::default();
        let nested = directory.path().join("missing").join("asset.glb");
        assert!(matches!(
            full_store.create_new(&nested, content_hash(b"1234"), b"1234"),
            Err(AssetFileError::Io {
                operation: AssetFileOperation::CreateNew,
                kind: io::ErrorKind::NotFound,
                cleanup: PartialFileCleanup::NotRequired,
            })
        ));
        assert!(!directory.path().join("missing").exists());
    }

    #[test]
    fn load_rejects_non_files_oversize_growth_and_hash_mismatch() {
        let directory = TestDirectory::new("load-rejection");
        let store = AssetFileStore::new(NonZeroU64::new(4).unwrap());
        assert_eq!(
            store.load(directory.path(), content_hash(b"")),
            Err(AssetFileError::NotRegularFile)
        );

        #[cfg(unix)]
        {
            let target = directory.path().join("symlink-target.glb");
            let symlink = directory.path().join("symlink.glb");
            fs::write(&target, b"four").unwrap();
            std::os::unix::fs::symlink(&target, &symlink).unwrap();
            assert_eq!(
                store.load(&symlink, content_hash(b"four")),
                Err(AssetFileError::NotRegularFile)
            );
        }

        let oversized = directory.path().join("oversized.glb");
        fs::write(&oversized, b"12345").unwrap();
        assert_eq!(
            store.load(&oversized, content_hash(b"12345")),
            Err(AssetFileError::SourceSizeExceeded {
                actual: 5,
                limit: 4,
            })
        );

        let corrupt = directory.path().join("corrupt.glb");
        fs::write(&corrupt, b"1234").unwrap();
        assert!(matches!(
            store.load(&corrupt, content_hash(b"4321")),
            Err(AssetFileError::ContentHashMismatch { .. })
        ));

        let growing = directory.path().join("growing.glb");
        fs::write(&growing, b"four").unwrap();
        let mut file = File::open(growing).unwrap();
        assert_eq!(
            read_file_snapshot(&mut file, 3, 4).map_err(AssetFileError::from),
            Err(AssetFileError::FileChangedDuringRead { expected: 3 })
        );
    }

    #[test]
    fn injected_write_and_sync_failures_report_cleanup_without_paths() {
        let directory = TestDirectory::new("injected-failure");
        for (failure, expected_operation) in [
            (InjectedFailure::Write, AssetFileOperation::Write),
            (InjectedFailure::Sync, AssetFileOperation::Sync),
        ] {
            let path = directory.path().join(format!("{failure:?}.glb"));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            let error =
                persist_created_file(&path, FailingFile { file, failure }, b"bounded fixture")
                    .map_err(AssetFileError::from)
                    .unwrap_err();
            assert!(matches!(
                error,
                AssetFileError::Io {
                    operation,
                    kind: io::ErrorKind::Other,
                    cleanup: PartialFileCleanup::Removed,
                } if operation == expected_operation
            ));
            assert!(!path.exists());
            assert!(!format!("{error:?}").contains(directory.marker()));
            assert!(!error.to_string().contains(directory.marker()));
        }
    }

    #[derive(Debug, Clone, Copy)]
    enum InjectedFailure {
        Write,
        Sync,
    }

    struct FailingFile {
        file: File,
        failure: InjectedFailure,
    }

    impl Write for FailingFile {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if matches!(self.failure, InjectedFailure::Write) {
                Err(io::Error::other("injected write failure"))
            } else {
                self.file.write(buffer)
            }
        }

        fn flush(&mut self) -> io::Result<()> {
            self.file.flush()
        }
    }

    impl SyncFile for FailingFile {
        fn sync_all(&self) -> io::Result<()> {
            if matches!(self.failure, InjectedFailure::Sync) {
                Err(io::Error::other("injected sync failure"))
            } else {
                self.file.sync_all()
            }
        }
    }

    struct TestDirectory {
        path: PathBuf,
        marker: String,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            loop {
                let marker = format!(
                    "cogniform-asset-file-{label}-{}-{}",
                    std::process::id(),
                    NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
                );
                let path = std::env::temp_dir().join(&marker);
                match fs::create_dir(&path) {
                    Ok(()) => return Self { path, marker },
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("failed to create asset-file test directory: {error:?}"),
                }
            }
        }

        fn path(&self) -> &Path {
            &self.path
        }

        fn marker(&self) -> &str {
            &self.marker
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.path).unwrap();
        }
    }
}
