use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use serde::{Deserialize, Serialize};

/// Runtime admission limits applied to every public protocol message.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RuntimeLimits {
    /// Maximum encoded JSON bytes accepted or emitted.
    pub max_encoded_bytes: NonZeroU64,
    /// Maximum deterministic logical decoded bytes in one message.
    pub max_decoded_bytes: NonZeroU64,
    /// Maximum JSON object/array nesting depth accepted before decoding.
    pub max_json_nesting_depth: NonZeroU16,
    /// Maximum operations in one patch.
    pub max_operations: NonZeroU32,
    /// Maximum total component values in one patch.
    pub max_components: NonZeroU32,
    /// Maximum component values in one create operation.
    pub max_components_per_entity: NonZeroU32,
    /// Maximum aggregate scene-text bytes in one message.
    pub max_text_bytes: NonZeroU64,
    /// Maximum diagnostics carried by one receipt.
    pub max_diagnostics: NonZeroU32,
    /// Maximum admitted capacity for one queue.
    pub max_queue_capacity: NonZeroU32,
    /// Maximum observation width in pixels.
    pub max_observation_width: NonZeroU32,
    /// Maximum observation height in pixels.
    pub max_observation_height: NonZeroU32,
    /// Maximum width-by-height pixel count for one observation.
    pub max_observation_pixels: NonZeroU64,
}

impl Default for RuntimeLimits {
    fn default() -> Self {
        Self {
            max_encoded_bytes: NonZeroU64::new(1_048_576).expect("constant is non-zero"),
            max_decoded_bytes: NonZeroU64::new(4_194_304).expect("constant is non-zero"),
            max_json_nesting_depth: NonZeroU16::new(32).expect("constant is non-zero"),
            max_operations: NonZeroU32::new(1_024).expect("constant is non-zero"),
            max_components: NonZeroU32::new(8_192).expect("constant is non-zero"),
            max_components_per_entity: NonZeroU32::new(64).expect("constant is non-zero"),
            max_text_bytes: NonZeroU64::new(65_536).expect("constant is non-zero"),
            max_diagnostics: NonZeroU32::new(128).expect("constant is non-zero"),
            max_queue_capacity: NonZeroU32::new(1_024).expect("constant is non-zero"),
            max_observation_width: NonZeroU32::new(4_096).expect("constant is non-zero"),
            max_observation_height: NonZeroU32::new(4_096).expect("constant is non-zero"),
            max_observation_pixels: NonZeroU64::new(16_777_216).expect("constant is non-zero"),
        }
    }
}

/// Resource budget declared by a patch before admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PatchBudget {
    /// Maximum operations the sender declares for this patch.
    pub max_operations: NonZeroU32,
    /// Maximum component values the sender declares for this patch.
    pub max_components: NonZeroU32,
    /// Maximum aggregate scene-text bytes the sender declares for this patch.
    pub max_text_bytes: NonZeroU64,
    /// Maximum logical decoded bytes the sender declares for this patch.
    pub max_decoded_bytes: NonZeroU64,
}

impl PatchBudget {
    /// Constructs a non-zero declared patch budget.
    #[must_use]
    pub const fn new(
        max_operations: NonZeroU32,
        max_components: NonZeroU32,
        max_text_bytes: NonZeroU64,
        max_decoded_bytes: NonZeroU64,
    ) -> Self {
        Self {
            max_operations,
            max_components,
            max_text_bytes,
            max_decoded_bytes,
        }
    }
}

impl Default for PatchBudget {
    fn default() -> Self {
        Self {
            max_operations: NonZeroU32::new(256).expect("constant is non-zero"),
            max_components: NonZeroU32::new(2_048).expect("constant is non-zero"),
            max_text_bytes: NonZeroU64::new(16_384).expect("constant is non-zero"),
            max_decoded_bytes: NonZeroU64::new(262_144).expect("constant is non-zero"),
        }
    }
}
