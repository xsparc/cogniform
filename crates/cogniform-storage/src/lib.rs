//! Explicit bounded persistence adapters for Cogniform recovery state.
//!
//! This crate owns opt-in filesystem authority outside the engine, world,
//! renderer, and replay domains. It creates immutable recovery files and loads
//! complete bounded envelopes; it does not select paths, create directories,
//! overwrite files, schedule checkpoints, or restore services automatically.

#![forbid(unsafe_code)]

mod asset_file;
mod file_io;
mod recovery_file;

pub use asset_file::{AssetFileError, AssetFileOperation, AssetFileReceipt, AssetFileStore};
pub use file_io::PartialFileCleanup;
pub use recovery_file::{
    RecoveryFileError, RecoveryFileOperation, RecoveryFileReceipt, RecoveryFileStore,
};
