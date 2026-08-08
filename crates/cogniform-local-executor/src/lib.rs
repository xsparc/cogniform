//! Bounded caller-driven execution for one local Cogniform session.
//!
//! The executor owns one in-process [`cogniform_engine::LocalService`], performs
//! explicit limit negotiation, correlates bounded command and observation work,
//! and advances at most one command and one observation completion per call. It
//! opens no process, thread, pipe, file, socket, listener, or shared-memory
//! resource and performs no ambient I/O.

#![forbid(unsafe_code)]

mod error;
mod executor;

pub use error::LocalExecutorError;
pub use executor::{
    LocalExecutorConfig, LocalExecutorPhase, LocalExecutorStatus, LocalSessionExecutor,
    MAX_OUTPUT_FRAMES_PER_CALL,
};
