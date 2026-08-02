//! Integrity-checked append-only replay for authoritative Cogniform worlds.
//!
//! Accepted canonical patches are chained with SHA-256 and can be loaded from
//! a bounded byte stream without discarding the last verified prefix.

#![forbid(unsafe_code)]

mod error;
mod log;
mod recorded;

pub use error::{
    RecordedApplyError, ReplayError, ReplayIntegrityError, ReplayIntegrityErrorKind,
    ReplayTailError, ReplayTailErrorKind,
};
pub use log::{
    ReplayConfig, ReplayConfigError, ReplayEntry, ReplayEntryHash, ReplayLoad, ReplayLog,
    ReplayVerification,
};
pub use recorded::RecordedWorld;
