//! Pure, bounded built-in scene procedures.
//!
//! Procedures consume explicit parameters and seeds and return ordinary scene
//! patches. They have no filesystem, network, clock, random-device, world, or
//! renderer access, so accepted output can be replayed through the same public
//! transaction path as any other patch.

#![forbid(unsafe_code)]

mod error;
mod grid;

pub use error::ProcedureError;
pub use grid::{
    BuiltinProcedure, CuboidGrid, ProcedureArtifact, ProcedureLimits, ProcedureRequest, execute,
};
