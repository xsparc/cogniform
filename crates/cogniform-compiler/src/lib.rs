//! Pure deterministic compilation from bounded semantic imagination to patches.
//!
//! This crate owns no world, renderer, transport, storage, or model state. A
//! caller supplies an immutable scene view and receives a normalized patch plus
//! structured decisions or unresolved constraints.

#![forbid(unsafe_code)]

mod compiler;
mod error;
mod report;

pub use compiler::{CompilationSceneView, CompilerConfig, DeterministicCompiler};
pub use error::CompileError;
pub use report::{
    CompilationDecision, CompilationDecisionCode, CompilationResult, UnresolvedConstraint,
    UnresolvedConstraintCode,
};
