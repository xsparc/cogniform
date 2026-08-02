//! Composition boundary for Cogniform's world, render, and observation domains.
//!
//! The engine applies world transactions, forwards compact immutable render
//! extractions, submits revision-linked frames, and completes observations on
//! a bounded worker path. It exposes no mutable ECS or GPU handle.

#![forbid(unsafe_code)]

mod engine;
mod error;
mod observation;

pub use engine::{CogniformEngine, EngineConfig};
pub use error::{EngineError, ObservationError};
pub use observation::{
    EntityVisibility, Observation, ObservationPayload, ObservationQueue, ObservationRequest,
};
