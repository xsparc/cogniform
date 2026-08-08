use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use cogniform_protocol::RuntimeLimits;
use serde::{Deserialize, Serialize};

// Version one can explain identity plus three defaults per entity. Patch text,
// decisions, and unresolved relation context together repeat admitted
// imagination text no more than six times.
const MAX_DECISIONS_PER_ENTITY: u32 = 4;
const RESULT_TEXT_MULTIPLIER: u64 = 6;
// These fixed maxima include every tag, option marker, length, index, and ID;
// aggregate text is accounted separately.
const DECISION_LOGICAL_OVERHEAD: u64 = 27;
const UNRESOLVED_LOGICAL_OVERHEAD: u64 = 38;
const RESULT_LOGICAL_OVERHEAD: u64 = 35;
const MAX_ESCAPED_BYTES_PER_TEXT_BYTE: u64 = 6;
const DECISION_ENCODED_OVERHEAD: u64 = 192;
const UNRESOLVED_ENCODED_OVERHEAD: u64 = 256;
const RESULT_ENCODED_OVERHEAD: u64 = 1_024;
const COMPILATION_JSON_NESTING_DEPTH: u16 = 9;

/// Explicit bounds for one encoded or decoded compilation result.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilationLimits {
    /// Maximum canonical JSON bytes, including the trailing LF.
    pub max_encoded_bytes: NonZeroU64,
    /// Maximum deterministic logical bytes in the complete result.
    pub max_decoded_bytes: NonZeroU64,
    /// Maximum JSON object/array nesting depth accepted before decoding.
    pub max_json_nesting_depth: NonZeroU16,
    /// Maximum aggregate scene-text bytes in the patch and report entries.
    pub max_text_bytes: NonZeroU64,
    /// Maximum ordered compiler decisions.
    pub max_decisions: NonZeroU32,
    /// Maximum ordered unresolved constraints.
    pub max_unresolved_constraints: NonZeroU32,
    /// Core protocol limits applied to an optional normalized patch.
    pub patch_limits: RuntimeLimits,
}

impl CompilationLimits {
    /// Derives report bounds that admit every result from the version-one
    /// compiler under the supplied core protocol limits.
    #[must_use]
    pub const fn for_runtime_limits(patch_limits: RuntimeLimits) -> Self {
        let decisions = patch_limits
            .max_imagination_entities
            .get()
            .saturating_mul(MAX_DECISIONS_PER_ENTITY)
            .saturating_add(patch_limits.max_imagination_relations.get());
        let unresolved = patch_limits
            .max_imagination_relations
            .get()
            .saturating_add(patch_limits.max_imagination_constraints.get());
        let text = patch_limits
            .max_text_bytes
            .get()
            .saturating_mul(RESULT_TEXT_MULTIPLIER);
        let decoded = patch_limits
            .max_decoded_bytes
            .get()
            .saturating_add(text)
            .saturating_add((decisions as u64).saturating_mul(DECISION_LOGICAL_OVERHEAD))
            .saturating_add((unresolved as u64).saturating_mul(UNRESOLVED_LOGICAL_OVERHEAD))
            .saturating_add(RESULT_LOGICAL_OVERHEAD);
        let encoded = patch_limits
            .max_encoded_bytes
            .get()
            .saturating_add(text.saturating_mul(MAX_ESCAPED_BYTES_PER_TEXT_BYTE))
            .saturating_add((decisions as u64).saturating_mul(DECISION_ENCODED_OVERHEAD))
            .saturating_add((unresolved as u64).saturating_mul(UNRESOLVED_ENCODED_OVERHEAD))
            .saturating_add(RESULT_ENCODED_OVERHEAD);
        let nested_patch = patch_limits.max_json_nesting_depth.get().saturating_add(1);
        let nesting = if nested_patch < COMPILATION_JSON_NESTING_DEPTH {
            COMPILATION_JSON_NESTING_DEPTH
        } else {
            nested_patch
        };

        Self {
            max_encoded_bytes: NonZeroU64::new(encoded).expect("derived limit is non-zero"),
            max_decoded_bytes: NonZeroU64::new(decoded).expect("derived limit is non-zero"),
            max_json_nesting_depth: NonZeroU16::new(nesting).expect("derived limit is non-zero"),
            max_text_bytes: NonZeroU64::new(text).expect("derived limit is non-zero"),
            max_decisions: NonZeroU32::new(decisions).expect("derived limit is non-zero"),
            max_unresolved_constraints: NonZeroU32::new(unresolved)
                .expect("derived limit is non-zero"),
            patch_limits,
        }
    }
}

impl Default for CompilationLimits {
    fn default() -> Self {
        Self::for_runtime_limits(RuntimeLimits::default())
    }
}
