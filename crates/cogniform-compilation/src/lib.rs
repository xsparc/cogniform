//! Bounded transport-neutral results produced by deterministic compilation.
//!
//! This crate owns only versioned values, validation, and canonical JSON. It
//! performs no compilation, world mutation, transport I/O, or endpoint work.

#![forbid(unsafe_code)]

mod codec;
mod error;
mod limits;
mod result;

pub use error::{CompilationCodecError, CompilationValidationError, CompilationValidationKind};
pub use limits::CompilationLimits;
pub use result::{
    COMPILATION_SCHEMA_VERSION, CompilationDecision, CompilationDecisionCode, CompilationResult,
    UnresolvedConstraint, UnresolvedConstraintCode,
};
