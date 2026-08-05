//! Explicit bounded persistence adapters for Cogniform recovery state.
//!
//! This crate owns opt-in filesystem authority outside the engine, world,
//! renderer, and replay domains. It creates immutable recovery files and loads
//! complete bounded envelopes; it does not select paths, create directories,
//! overwrite files, schedule checkpoints, or restore services automatically.

#![forbid(unsafe_code)]

mod recovery_file;

pub use recovery_file::{
    PartialFileCleanup, RecoveryFileError, RecoveryFileOperation, RecoveryFileReceipt,
    RecoveryFileStore,
};
