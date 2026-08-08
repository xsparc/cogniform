//! Bounded synchronous framing over caller-owned local byte streams.
//!
//! This crate frames schema-owned control bytes and complete observation
//! values over [`std::io::Read`] and [`std::io::Write`] implementations. It
//! opens no process, pipe, socket, listener, file, or shared-memory resource
//! and owns no session, authentication, authorization, or scheduling policy.

#![forbid(unsafe_code)]

mod codec;
mod error;

use core::{
    fmt,
    num::{NonZeroU32, NonZeroU64},
};

use cogniform_observation::{ObservationPayload, ObservationPayloadLimits};
use cogniform_protocol::{ObservationMetadata, RuntimeLimits};

pub use codec::{
    LOCAL_FRAME_HEADER_BYTES, LOCAL_FRAME_VERSION, decode_frame, encode_frame, read_frame,
    write_frame,
};
pub use error::{LocalFrameError, LocalFrameIoOperation, LocalFrameSection};

/// Supported version-one local frame kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalFrameKind {
    /// Schema-owned control bytes with no bulk section.
    Control,
    /// Canonical observation metadata plus its CF038 payload envelope.
    Observation,
}

/// One decoded bounded frame.
#[derive(Clone, PartialEq)]
pub enum LocalFrame {
    /// Exact schema-owned bytes. Their schema is selected by a future session.
    Control {
        /// Non-zero value used by the future session to correlate work.
        correlation_id: NonZeroU64,
        /// Exact non-empty schema-owned control bytes.
        bytes: Vec<u8>,
    },
    /// One complete causal observation value.
    Observation {
        /// Non-zero value used by the future session to correlate work.
        correlation_id: NonZeroU64,
        /// Validated canonical causal metadata.
        metadata: ObservationMetadata,
        /// Validated owned payload bound to `metadata` by the inner envelope.
        payload: ObservationPayload,
    },
}

impl LocalFrame {
    /// Returns the fixed frame kind.
    #[must_use]
    pub const fn kind(&self) -> LocalFrameKind {
        match self {
            Self::Control { .. } => LocalFrameKind::Control,
            Self::Observation { .. } => LocalFrameKind::Observation,
        }
    }

    /// Returns the non-zero session-owned correlation value.
    #[must_use]
    pub const fn correlation_id(&self) -> NonZeroU64 {
        match self {
            Self::Control { correlation_id, .. } | Self::Observation { correlation_id, .. } => {
                *correlation_id
            }
        }
    }
}

impl fmt::Debug for LocalFrame {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Control {
                correlation_id,
                bytes,
            } => formatter
                .debug_struct("Control")
                .field("correlation_id", correlation_id)
                .field("control_bytes", &bytes.len())
                .finish(),
            Self::Observation {
                correlation_id,
                metadata,
                payload,
            } => formatter
                .debug_struct("Observation")
                .field("correlation_id", correlation_id)
                .field("kind", &metadata.kind)
                .field("payload_items", &payload.item_count())
                .finish_non_exhaustive(),
        }
    }
}

/// Independent limits for one complete local stream frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LocalFrameLimits {
    /// Maximum fixed-header plus control plus bulk bytes.
    pub max_frame_bytes: NonZeroU64,
    /// Maximum schema-owned control or canonical metadata bytes.
    pub max_control_bytes: NonZeroU64,
    /// Maximum observation payload-envelope bytes.
    pub max_bulk_bytes: NonZeroU64,
}

impl LocalFrameLimits {
    /// Constructs explicit non-zero frame limits.
    #[must_use]
    pub const fn new(
        max_frame_bytes: NonZeroU64,
        max_control_bytes: NonZeroU64,
        max_bulk_bytes: NonZeroU64,
    ) -> Self {
        Self {
            max_frame_bytes,
            max_control_bytes,
            max_bulk_bytes,
        }
    }
}

impl Default for LocalFrameLimits {
    fn default() -> Self {
        Self {
            max_frame_bytes: NonZeroU64::new(5_242_948).expect("constant is non-zero"),
            max_control_bytes: NonZeroU64::new(1_048_576).expect("constant is non-zero"),
            max_bulk_bytes: NonZeroU64::new(4_194_304).expect("constant is non-zero"),
        }
    }
}

/// Complete validation configuration for the local framing boundary.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct LocalFrameConfig {
    /// Outer stream-frame limits.
    pub frame_limits: LocalFrameLimits,
    /// Canonical metadata and image-dimension limits.
    pub runtime_limits: RuntimeLimits,
    /// Inner observation payload-envelope limits.
    pub payload_limits: ObservationPayloadLimits,
}

impl LocalFrameConfig {
    /// Constructs an explicit complete configuration.
    #[must_use]
    pub const fn new(
        frame_limits: LocalFrameLimits,
        runtime_limits: RuntimeLimits,
        payload_limits: ObservationPayloadLimits,
    ) -> Self {
        Self {
            frame_limits,
            runtime_limits,
            payload_limits,
        }
    }

    /// Constructs a complete configuration from independent payload bounds.
    #[must_use]
    pub const fn with_payload_bounds(
        frame_limits: LocalFrameLimits,
        runtime_limits: RuntimeLimits,
        max_observation_envelope_bytes: NonZeroU64,
        max_visibility_entries: NonZeroU32,
    ) -> Self {
        Self::new(
            frame_limits,
            runtime_limits,
            ObservationPayloadLimits::new(max_observation_envelope_bytes, max_visibility_entries),
        )
    }
}
