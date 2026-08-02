use core::num::NonZeroU32;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    CodecError, ComponentKind, ComponentValue, DiagnosticCode, RuntimeLimits, SceneRevision,
    SchemaVersion, StableEntityId, ValidationError,
    codec::{self, Validate},
};

/// Exact-revision logical scene query for the local gateway.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneQuery {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Exact authoritative revision the caller expects to query.
    pub scene_revision: SceneRevision,
    /// Stable IDs to select; empty selects every entity within `limit`.
    pub entity_ids: Vec<StableEntityId>,
    /// Component kinds to retain; empty retains every component.
    pub component_kinds: Vec<ComponentKind>,
    /// Maximum entities the caller is prepared to receive.
    pub limit: NonZeroU32,
}

impl SceneQuery {
    /// Validates schema, filter uniqueness, and query bounds.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting query.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for SceneQuery {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;
        if self.limit.get() > limits.max_query_entities.get()
            || count(self.entity_ids.len()) > self.limit.get()
            || count(self.entity_ids.len()) > limits.max_query_entities.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::QueryEntityLimitExceeded,
                "query.entity_ids",
            ));
        }
        if !all_unique(&self.entity_ids) || !all_unique(&self.component_kinds) {
            return Err(ValidationError::new(
                DiagnosticCode::DuplicateQueryFilter,
                "query.filters",
            ));
        }
        if self.logical_size_bytes() > limits.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "query",
            ));
        }
        Ok(())
    }
}

impl SceneQuery {
    fn logical_size_bytes(&self) -> u64 {
        2_u64
            .saturating_add(8)
            .saturating_add(4)
            .saturating_add(u64::from(count(self.entity_ids.len())) * 16)
            .saturating_add(4)
            .saturating_add(u64::from(count(self.component_kinds.len())))
            .saturating_add(4)
    }
}

/// Backend-neutral logical state returned for one queried entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneEntityView {
    /// Stable external identity.
    pub entity_id: StableEntityId,
    /// Stable parent identity, or `None` for a root.
    pub parent_id: Option<StableEntityId>,
    /// Component values in canonical component-kind order.
    pub components: Vec<ComponentValue>,
}

/// Deterministically ordered result for one exact-revision scene query.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SceneQueryResult {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Exact authoritative revision represented by every returned entity.
    pub scene_revision: SceneRevision,
    /// Entities in strictly increasing stable-ID order.
    pub entities: Vec<SceneEntityView>,
}

impl SceneQueryResult {
    /// Validates canonical ordering and response resource bounds.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting query response.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for SceneQueryResult {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;
        if count(self.entities.len()) > limits.max_query_entities.get() {
            return Err(ValidationError::new(
                DiagnosticCode::QueryEntityLimitExceeded,
                "query_result.entities",
            ));
        }

        let mut previous_id = None;
        let mut text_bytes = 0_u64;
        let mut component_count = 0_u64;
        for entity in &self.entities {
            if previous_id.is_some_and(|previous| previous >= entity.entity_id) {
                return Err(ValidationError::new(
                    DiagnosticCode::NonCanonicalQueryResult,
                    "query_result.entities",
                ));
            }
            previous_id = Some(entity.entity_id);

            if count(entity.components.len()) > limits.max_components_per_entity.get() {
                return Err(ValidationError::new(
                    DiagnosticCode::ComponentLimitExceeded,
                    "query_result.entities[].components",
                ));
            }

            let mut previous_kind = None;
            for component in &entity.components {
                let kind = component.kind();
                if previous_kind.is_some_and(|previous| previous >= kind) {
                    return Err(ValidationError::new(
                        DiagnosticCode::NonCanonicalQueryResult,
                        "query_result.entities[].components",
                    ));
                }
                previous_kind = Some(kind);
                component.validate()?;
                component_count = component_count.saturating_add(1);
                text_bytes = text_bytes
                    .saturating_add(u64::try_from(component.text_bytes()).unwrap_or(u64::MAX));
            }
        }

        if component_count > u64::from(limits.max_components.get()) {
            return Err(ValidationError::new(
                DiagnosticCode::ComponentLimitExceeded,
                "query_result.entities[].components",
            ));
        }
        if text_bytes > limits.max_text_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::TextLimitExceeded,
                "query_result.entities[].components",
            ));
        }
        if self.logical_size_bytes() > limits.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "query_result",
            ));
        }
        Ok(())
    }
}

impl SceneQueryResult {
    fn logical_size_bytes(&self) -> u64 {
        self.entities.iter().fold(2_u64 + 8 + 4, |total, entity| {
            entity.components.iter().fold(
                total
                    .saturating_add(16)
                    .saturating_add(1 + u64::from(entity.parent_id.is_some()) * 16)
                    .saturating_add(4),
                |component_total, component| {
                    component_total.saturating_add(component.logical_size_bytes())
                },
            )
        })
    }
}

fn validate_schema(schema_version: SchemaVersion) -> Result<(), ValidationError> {
    if schema_version == SchemaVersion::V1 {
        Ok(())
    } else {
        Err(ValidationError::new(
            DiagnosticCode::UnsupportedSchema,
            "schema_version",
        ))
    }
}

fn count(length: usize) -> u32 {
    u32::try_from(length).unwrap_or(u32::MAX)
}

fn all_unique<T: Ord>(values: &[T]) -> bool {
    let mut seen = BTreeSet::new();
    values.iter().all(|value| seen.insert(value))
}
