//! Authoritative, deterministic scene-world ownership for Cogniform.
//!
//! The crate keeps ECS handles private, validates complete ordered patches
//! before mutation, and exposes only stable-ID logical snapshots and receipts.

#![forbid(unsafe_code)]

mod error;
mod hash;
mod snapshot;
mod transform;
mod world;

pub use error::{
    WorldApplyError, WorldExtractionError, WorldInvariantError, WorldInvariantErrorKind,
};
pub use hash::LogicalSceneHash;
pub use snapshot::{EntitySnapshot, WorldSnapshot};
pub use transform::WorldTransform;
pub use world::{AuthoritativeWorld, WorldConfig};
