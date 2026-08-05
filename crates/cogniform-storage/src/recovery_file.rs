use core::fmt;
use std::{io, path::Path};

use cogniform_engine::{EngineRecoveryPoint, RecoveryPointCodecError};
use cogniform_protocol::FrameId;
use cogniform_replay::ReplayConfig;

use crate::{
    PartialFileCleanup,
    file_io::{FileError, FileOperation, create_new_synced, read_bounded_file},
};

/// Filesystem operation associated with a path-redacted storage failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RecoveryFileOperation {
    /// Inspect the final path component before opening it.
    Inspect,
    /// Open a caller-selected path for bounded loading.
    OpenRead,
    /// Read the complete declared file contents.
    Read,
    /// Create a caller-selected new path without overwriting.
    CreateNew,
    /// Write the complete encoded recovery envelope.
    Write,
    /// Synchronize the new file contents and metadata.
    Sync,
}

impl fmt::Display for RecoveryFileOperation {
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

impl From<FileOperation> for RecoveryFileOperation {
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

/// Explicit result of successfully creating one immutable recovery file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryFileReceipt {
    /// Complete synchronized envelope byte count.
    pub envelope_bytes: u64,
    /// Replay payload byte count represented by the envelope.
    pub replay_bytes: u64,
    /// First renderer frame identity available after restoration.
    pub next_frame_id: FrameId,
}

/// Bounded, path-redacted recovery-file failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RecoveryFileError {
    /// Recovery envelope encoding or validation failed.
    Codec(RecoveryPointCodecError),
    /// A filesystem operation failed without retaining the caller path.
    Io {
        /// Operation that failed.
        operation: RecoveryFileOperation,
        /// Stable standard-library error category.
        kind: io::ErrorKind,
        /// Disposition of a file created by this call, if any.
        cleanup: PartialFileCleanup,
    },
    /// The selected load target is not a regular file.
    NotRegularFile,
    /// The observed file is larger than the effective bounded allocation limit.
    EnvelopeSizeExceeded {
        /// Observed file byte count.
        actual: u64,
        /// Maximum accepted byte count on this platform and configuration.
        limit: u64,
    },
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

impl fmt::Display for RecoveryFileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Codec(error) => write!(formatter, "recovery envelope failed validation: {error}"),
            Self::Io {
                operation,
                kind,
                cleanup,
            } => {
                write!(formatter, "recovery file {operation} failed with {kind:?}")?;
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
                formatter.write_str("recovery file load target is not a regular file")
            }
            Self::EnvelopeSizeExceeded { actual, limit } => write!(
                formatter,
                "recovery file has {actual} bytes; effective limit is {limit}"
            ),
            Self::FileChangedDuringRead { expected } => write!(
                formatter,
                "recovery file grew after its accepted {expected}-byte metadata snapshot"
            ),
            Self::AllocationFailed { requested } => write!(
                formatter,
                "recovery file could not reserve its bounded {requested}-byte buffer"
            ),
        }
    }
}

impl std::error::Error for RecoveryFileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Codec(error) => Some(error),
            Self::Io { .. }
            | Self::NotRegularFile
            | Self::EnvelopeSizeExceeded { .. }
            | Self::FileChangedDuringRead { .. }
            | Self::AllocationFailed { .. } => None,
        }
    }
}

impl From<RecoveryPointCodecError> for RecoveryFileError {
    fn from(value: RecoveryPointCodecError) -> Self {
        Self::Codec(value)
    }
}

impl From<FileError> for RecoveryFileError {
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
            FileError::SizeExceeded { actual, limit } => {
                Self::EnvelopeSizeExceeded { actual, limit }
            }
            FileError::ChangedDuringRead { expected } => Self::FileChangedDuringRead { expected },
            FileError::AllocationFailed { requested } => Self::AllocationFailed { requested },
        }
    }
}

/// Explicit create-new and bounded-load adapter for recovery envelopes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RecoveryFileStore {
    replay_config: ReplayConfig,
    envelope_byte_limit: u64,
}

impl RecoveryFileStore {
    /// Creates an adapter after validating its replay and envelope bound.
    pub fn new(replay_config: ReplayConfig) -> Result<Self, RecoveryFileError> {
        let envelope_byte_limit = EngineRecoveryPoint::envelope_byte_limit(replay_config)?;
        Ok(Self {
            replay_config,
            envelope_byte_limit,
        })
    }

    /// Returns the maximum envelope bytes accepted by this adapter.
    #[must_use]
    pub const fn envelope_byte_limit(&self) -> u64 {
        self.envelope_byte_limit
    }

    /// Creates and synchronizes one immutable recovery file.
    ///
    /// Encoding and bound validation complete before the filesystem is touched.
    /// The target and parent directory must already be selected and authorized
    /// by the caller. Existing targets are never overwritten. A successful
    /// result means [`std::fs::File::sync_all`] completed for the file, not that its
    /// directory entry or storage hardware has a cross-platform durability
    /// guarantee.
    pub fn create_new(
        &self,
        path: &Path,
        recovery: &EngineRecoveryPoint,
    ) -> Result<RecoveryFileReceipt, RecoveryFileError> {
        let encoded = recovery.to_envelope_bytes(self.replay_config)?;
        debug_assert!(encoded.len() as u64 <= self.envelope_byte_limit);
        create_new_synced(path, &encoded)?;
        Ok(RecoveryFileReceipt {
            envelope_bytes: encoded.len() as u64,
            replay_bytes: recovery.replay_bytes().len() as u64,
            next_frame_id: recovery.next_frame_id(),
        })
    }

    /// Loads one complete regular-file recovery envelope within configured bounds.
    ///
    /// The final path component is rejected when it is a symlink at inspection
    /// time. Parent-directory trust and path-race protection remain caller
    /// responsibilities. No partial or merely prefix-valid recovery point is
    /// returned.
    pub fn load(&self, path: &Path) -> Result<EngineRecoveryPoint, RecoveryFileError> {
        let encoded = read_bounded_file(path, self.envelope_byte_limit)?;
        EngineRecoveryPoint::from_envelope_bytes(&encoded, self.replay_config).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use core::{
        num::NonZeroU32,
        sync::atomic::{AtomicU64, Ordering},
    };
    use std::{
        fs::{self, File, OpenOptions},
        io::Write,
        path::{Path, PathBuf},
    };

    use crate::file_io::{SyncFile, persist_created_file, read_file_snapshot};

    use super::*;

    static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

    #[test]
    fn create_new_round_trips_and_never_overwrites() {
        let directory = TestDirectory::new("round-trip");
        let path = directory.path().join("recovery.cnf");
        let store = RecoveryFileStore::new(ReplayConfig::default()).unwrap();
        let first = recovery(17);
        let receipt = store.create_new(&path, &first).unwrap();
        assert_eq!(receipt.replay_bytes, 8);
        assert_eq!(receipt.next_frame_id, FrameId::new(17).unwrap());
        assert_eq!(receipt.envelope_bytes, fs::metadata(&path).unwrap().len());
        assert_eq!(store.load(&path).unwrap(), first);

        let original = fs::read(&path).unwrap();
        let error = store.create_new(&path, &recovery(18)).unwrap_err();
        assert!(matches!(
            error,
            RecoveryFileError::Io {
                operation: RecoveryFileOperation::CreateNew,
                kind: io::ErrorKind::AlreadyExists,
                cleanup: PartialFileCleanup::NotRequired,
            }
        ));
        assert_eq!(fs::read(&path).unwrap(), original);
    }

    #[test]
    fn load_rejects_non_files_oversize_and_invalid_complete_envelopes() {
        let directory = TestDirectory::new("load-rejection");
        let replay_config = ReplayConfig {
            max_log_bytes: NonZeroU32::new(8).unwrap(),
            ..ReplayConfig::default()
        };
        let store = RecoveryFileStore::new(replay_config).unwrap();
        assert_eq!(
            store.load(directory.path()),
            Err(RecoveryFileError::NotRegularFile)
        );

        let oversized = directory.path().join("oversized.cnf");
        let oversized_len = usize::try_from(store.envelope_byte_limit()).unwrap() + 1;
        fs::write(&oversized, vec![0_u8; oversized_len]).unwrap();
        assert_eq!(
            store.load(&oversized),
            Err(RecoveryFileError::EnvelopeSizeExceeded {
                actual: store.envelope_byte_limit() + 1,
                limit: store.envelope_byte_limit(),
            })
        );

        let invalid = directory.path().join("invalid.cnf");
        store.create_new(&invalid, &recovery(19)).unwrap();
        let mut bytes = fs::read(&invalid).unwrap();
        bytes[20] ^= 1;
        fs::write(&invalid, bytes).unwrap();
        assert!(matches!(
            store.load(&invalid),
            Err(RecoveryFileError::Codec(
                RecoveryPointCodecError::IntegrityMismatch
            ))
        ));

        let complete = recovery(20)
            .to_envelope_bytes(ReplayConfig::default())
            .unwrap();
        let default_store = RecoveryFileStore::new(ReplayConfig::default()).unwrap();

        let truncated = directory.path().join("truncated.cnf");
        fs::write(&truncated, &complete[..complete.len() - 1]).unwrap();
        assert!(matches!(
            default_store.load(&truncated),
            Err(RecoveryFileError::Codec(
                RecoveryPointCodecError::LengthMismatch { .. }
            ))
        ));

        let extended = directory.path().join("extended.cnf");
        let mut extended_bytes = complete;
        extended_bytes.push(0);
        fs::write(&extended, extended_bytes).unwrap();
        assert!(matches!(
            default_store.load(&extended),
            Err(RecoveryFileError::Codec(
                RecoveryPointCodecError::LengthMismatch { .. }
            ))
        ));
    }

    #[test]
    fn bounded_reader_rejects_growth_after_accepted_metadata_snapshot() {
        let directory = TestDirectory::new("growth");
        let path = directory.path().join("growing.cnf");
        fs::write(&path, b"four").unwrap();
        let mut file = File::open(path).unwrap();

        assert_eq!(
            read_file_snapshot(&mut file, 3, 4).map_err(RecoveryFileError::from),
            Err(RecoveryFileError::FileChangedDuringRead { expected: 3 })
        );
    }

    #[test]
    fn invalid_encoding_and_missing_parent_fail_without_implicit_filesystem_work() {
        let directory = TestDirectory::new("preflight");
        let replay_config = ReplayConfig {
            max_log_bytes: NonZeroU32::new(8).unwrap(),
            ..ReplayConfig::default()
        };
        let store = RecoveryFileStore::new(replay_config).unwrap();
        let oversized_recovery =
            EngineRecoveryPoint::from_parts(vec![0_u8; 9], FrameId::new(1).unwrap());
        let target = directory.path().join("not-created.cnf");
        assert!(matches!(
            store.create_new(&target, &oversized_recovery),
            Err(RecoveryFileError::Codec(
                RecoveryPointCodecError::ReplaySizeExceeded { .. }
            ))
        ));
        assert!(!target.exists());

        let nested = directory.path().join("missing").join("recovery.cnf");
        assert!(matches!(
            store.create_new(&nested, &recovery(20)),
            Err(RecoveryFileError::Io {
                operation: RecoveryFileOperation::CreateNew,
                kind: io::ErrorKind::NotFound,
                cleanup: PartialFileCleanup::NotRequired,
            })
        ));
        assert!(!directory.path().join("missing").exists());
    }

    #[test]
    fn injected_write_and_sync_failures_report_cleanup_without_paths() {
        let directory = TestDirectory::new("injected-failure");
        for failure in [InjectedFailure::Write, InjectedFailure::Sync] {
            let path = directory.path().join(format!("{failure:?}.cnf"));
            let file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            let error =
                persist_created_file(&path, FailingFile { file, failure }, b"bounded fixture")
                    .map_err(RecoveryFileError::from)
                    .unwrap_err();
            assert!(matches!(
                error,
                RecoveryFileError::Io {
                    kind: io::ErrorKind::Other,
                    cleanup: PartialFileCleanup::Removed,
                    ..
                }
            ));
            assert!(!path.exists());
            assert!(!format!("{error:?}").contains(directory.marker()));
            assert!(!error.to_string().contains(directory.marker()));
        }
    }

    fn recovery(frame: u64) -> EngineRecoveryPoint {
        EngineRecoveryPoint::from_parts(b"CNFRPL1\n".to_vec(), FrameId::new(frame).unwrap())
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
                    "cogniform-storage-{label}-{}-{}",
                    std::process::id(),
                    NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
                );
                let path = std::env::temp_dir().join(&marker);
                match fs::create_dir(&path) {
                    Ok(()) => return Self { path, marker },
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                    Err(error) => panic!("failed to create storage test directory: {error:?}"),
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
