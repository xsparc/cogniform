//! Bounded transport-neutral encoding for owned Cogniform observation payloads.
//!
//! Causal [`ObservationMetadata`](cogniform_protocol::ObservationMetadata)
//! remains in `cogniform-protocol`; this crate keeps bulk payload bytes in a
//! separate, versioned, integrity-checked envelope. It performs no I/O and
//! owns no renderer, service, transport, or shared-memory resource.

#![forbid(unsafe_code)]

mod codec;
mod error;

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_protocol::{ObservationKind, StableEntityId};

pub use codec::{
    OBSERVATION_NORMAL_LENGTH_SQUARED_TOLERANCE, OBSERVATION_PAYLOAD_ENVELOPE_HEADER_BYTES,
    OBSERVATION_PAYLOAD_ENVELOPE_VERSION, decode_payload, encode_payload,
};
pub use error::ObservationEnvelopeError;

/// Stable visibility summary for one entity in one frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EntityVisibility {
    /// Stable world identity.
    pub entity_id: StableEntityId,
    /// Exact non-zero number of pixels carrying this identity.
    pub visible_pixels: u64,
}

/// Owned bulk data associated with one causal observation envelope.
#[derive(Debug, Clone, PartialEq)]
pub enum ObservationPayload {
    /// Linear RGBA8 pixels in row-major order.
    Color(Vec<[u8; 4]>),
    /// Normalized finite depth pixels in row-major order.
    Depth(Vec<f32>),
    /// World-space unit normals; background pixels are `None`.
    Normal(Vec<Option<[f32; 3]>>),
    /// Exact stable identity per pixel; background is `None`.
    EntityId(Vec<Option<StableEntityId>>),
    /// Stable-identity visibility counts sorted by identity.
    Visibility(Vec<EntityVisibility>),
}

impl ObservationPayload {
    /// Returns the metadata kind required by this payload variant.
    #[must_use]
    pub const fn kind(&self) -> ObservationKind {
        match self {
            Self::Color(_) => ObservationKind::Color,
            Self::Depth(_) => ObservationKind::Depth,
            Self::Normal(_) => ObservationKind::Normal,
            Self::EntityId(_) => ObservationKind::EntityId,
            Self::Visibility(_) => ObservationKind::Visibility,
        }
    }

    /// Returns the number of pixels or visibility entries in this payload.
    #[must_use]
    pub fn item_count(&self) -> u64 {
        let length = match self {
            Self::Color(values) => values.len(),
            Self::Depth(values) => values.len(),
            Self::Normal(values) => values.len(),
            Self::EntityId(values) => values.len(),
            Self::Visibility(values) => values.len(),
        };
        u64::try_from(length).unwrap_or(u64::MAX)
    }
}

/// Independent resource limits for one encoded observation payload envelope.
///
/// Image item counts are additionally constrained by the supplied protocol
/// runtime limits and causal metadata. Visibility has no image dimensions, so
/// it receives an explicit entry bound here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObservationPayloadLimits {
    /// Maximum complete header-plus-payload envelope bytes.
    pub max_envelope_bytes: NonZeroU64,
    /// Maximum stable-identity entries in one visibility payload.
    pub max_visibility_entries: NonZeroU32,
}

impl ObservationPayloadLimits {
    /// Constructs explicit non-zero envelope limits.
    #[must_use]
    pub const fn new(max_envelope_bytes: NonZeroU64, max_visibility_entries: NonZeroU32) -> Self {
        Self {
            max_envelope_bytes,
            max_visibility_entries,
        }
    }
}

impl Default for ObservationPayloadLimits {
    fn default() -> Self {
        Self {
            max_envelope_bytes: NonZeroU64::new(4_194_304).expect("constant is non-zero"),
            max_visibility_entries: NonZeroU32::new(4_096).expect("constant is non-zero"),
        }
    }
}
