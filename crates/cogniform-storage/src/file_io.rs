use std::{
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::Path,
};

const READ_CHUNK_BYTES: usize = 8 * 1_024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileOperation {
    Inspect,
    OpenRead,
    Read,
    CreateNew,
    Write,
    Sync,
}

/// Cleanup disposition after a new file failed during write or sync.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartialFileCleanup {
    /// No file was created by the failed operation.
    NotRequired,
    /// The partial file was absent or removed successfully.
    Removed,
    /// The partial file may remain and requires caller inspection.
    Retained {
        /// Path-redacted filesystem error from the cleanup attempt.
        kind: io::ErrorKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FileError {
    Io {
        operation: FileOperation,
        kind: io::ErrorKind,
        cleanup: PartialFileCleanup,
    },
    NotRegularFile,
    SizeExceeded {
        actual: u64,
        limit: u64,
    },
    ChangedDuringRead {
        expected: u64,
    },
    AllocationFailed {
        requested: u64,
    },
}

pub(crate) fn create_new_synced(path: &Path, encoded: &[u8]) -> Result<(), FileError> {
    let file = open_new_file(path)?;
    persist_created_file(path, file, encoded)
}

fn open_new_file(path: &Path) -> Result<File, FileError> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path).map_err(|error| {
        io_error(
            FileOperation::CreateNew,
            &error,
            PartialFileCleanup::NotRequired,
        )
    })
}

pub(crate) trait SyncFile: Write {
    fn sync_all(&self) -> io::Result<()>;
}

impl SyncFile for File {
    fn sync_all(&self) -> io::Result<()> {
        File::sync_all(self)
    }
}

pub(crate) fn persist_created_file<W: SyncFile>(
    path: &Path,
    mut file: W,
    encoded: &[u8],
) -> Result<(), FileError> {
    if let Err(error) = file.write_all(encoded) {
        return Err(created_file_error(path, file, FileOperation::Write, &error));
    }
    if let Err(error) = file.sync_all() {
        return Err(created_file_error(path, file, FileOperation::Sync, &error));
    }
    Ok(())
}

fn created_file_error<W>(
    path: &Path,
    file: W,
    operation: FileOperation,
    error: &io::Error,
) -> FileError {
    let kind = error.kind();
    drop(file);
    let cleanup = match fs::remove_file(path) {
        Ok(()) => PartialFileCleanup::Removed,
        Err(cleanup_error) if cleanup_error.kind() == io::ErrorKind::NotFound => {
            PartialFileCleanup::Removed
        }
        Err(cleanup_error) => PartialFileCleanup::Retained {
            kind: cleanup_error.kind(),
        },
    };
    FileError::Io {
        operation,
        kind,
        cleanup,
    }
}

pub(crate) fn read_bounded_file(path: &Path, byte_limit: u64) -> Result<Vec<u8>, FileError> {
    let path_metadata = fs::symlink_metadata(path).map_err(|error| {
        io_error(
            FileOperation::Inspect,
            &error,
            PartialFileCleanup::NotRequired,
        )
    })?;
    if path_metadata.file_type().is_symlink() || !path_metadata.is_file() {
        return Err(FileError::NotRegularFile);
    }

    let mut file = File::open(path).map_err(|error| {
        io_error(
            FileOperation::OpenRead,
            &error,
            PartialFileCleanup::NotRequired,
        )
    })?;
    let metadata = file.metadata().map_err(|error| {
        io_error(
            FileOperation::Inspect,
            &error,
            PartialFileCleanup::NotRequired,
        )
    })?;
    if !metadata.is_file() {
        return Err(FileError::NotRegularFile);
    }

    let effective_limit = byte_limit.min(u64::try_from(usize::MAX).unwrap_or(u64::MAX));
    read_file_snapshot(&mut file, metadata.len(), effective_limit)
}

pub(crate) fn read_file_snapshot(
    file: &mut File,
    actual: u64,
    effective_limit: u64,
) -> Result<Vec<u8>, FileError> {
    if actual > effective_limit {
        return Err(FileError::SizeExceeded {
            actual,
            limit: effective_limit,
        });
    }
    let capacity = usize::try_from(actual).map_err(|_| FileError::SizeExceeded {
        actual,
        limit: effective_limit,
    })?;
    let mut encoded = Vec::new();
    encoded
        .try_reserve_exact(capacity)
        .map_err(|_| FileError::AllocationFailed { requested: actual })?;
    read_declared_bytes(file, &mut encoded, capacity)?;
    let mut growth_probe = [0_u8; 1];
    if file
        .read(&mut growth_probe)
        .map_err(|error| io_error(FileOperation::Read, &error, PartialFileCleanup::NotRequired))?
        != 0
    {
        return Err(FileError::ChangedDuringRead { expected: actual });
    }
    Ok(encoded)
}

fn read_declared_bytes(
    file: &mut File,
    encoded: &mut Vec<u8>,
    expected: usize,
) -> Result<(), FileError> {
    let mut chunk = [0_u8; READ_CHUNK_BYTES];
    while encoded.len() < expected {
        let remaining = expected - encoded.len();
        let read = file
            .read(&mut chunk[..remaining.min(READ_CHUNK_BYTES)])
            .map_err(|error| {
                io_error(FileOperation::Read, &error, PartialFileCleanup::NotRequired)
            })?;
        if read == 0 {
            break;
        }
        encoded.extend_from_slice(&chunk[..read]);
    }
    Ok(())
}

fn io_error(operation: FileOperation, error: &io::Error, cleanup: PartialFileCleanup) -> FileError {
    FileError::Io {
        operation,
        kind: error.kind(),
        cleanup,
    }
}
