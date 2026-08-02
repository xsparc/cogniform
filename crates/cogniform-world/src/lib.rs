//! Authoritative, deterministic scene-world ownership for Cogniform.
//!
//! The crate keeps ECS handles private, validates complete ordered patches
//! before mutation, and exposes only stable-ID logical snapshots and receipts.

#![forbid(unsafe_code)]

mod error;
mod snapshot;
mod world;

pub use error::{WorldApplyError, WorldInvariantError, WorldInvariantErrorKind};
pub use snapshot::{EntitySnapshot, WorldSnapshot};
pub use world::{AuthoritativeWorld, WorldConfig};
