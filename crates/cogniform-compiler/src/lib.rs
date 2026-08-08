//! Pure deterministic compilation from bounded semantic imagination to patches.
//!
//! This crate owns no world, renderer, transport, storage, or model state. A
//! caller supplies an immutable scene view and receives a normalized patch plus
//! structured decisions or unresolved constraints.

#![forbid(unsafe_code)]

mod compiler;
mod error;

pub use cogniform_compilation::{
    COMPILATION_SCHEMA_VERSION, CompilationCodecError, CompilationDecision,
    CompilationDecisionCode, CompilationLimits, CompilationResult, CompilationValidationError,
    CompilationValidationKind, UnresolvedConstraint, UnresolvedConstraintCode,
};
pub use compiler::{CompilationSceneView, CompilerConfig, DeterministicCompiler};
pub use error::CompileError;
