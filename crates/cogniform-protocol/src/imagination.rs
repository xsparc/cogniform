use core::num::NonZeroU32;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::{
    CodecError, ComponentValue, DeliverySemantic, DiagnosticCode, IdempotencyKey, ImaginationId,
    LocalTransform, MaterialComponent, NonNegativeF32, PatchBudget, PrimitiveComponent,
    RuntimeLimits, SceneRevision, SceneText, SchemaVersion, StableEntityId, TransactionId,
    ValidationError,
    codec::{self, Validate},
};

/// Sender-declared bounds for deterministic imagination compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaginationBudget {
    /// Maximum entities accepted from this imagination.
    pub max_entities: NonZeroU32,
    /// Maximum relations accepted from this imagination.
    pub max_relations: NonZeroU32,
    /// Maximum constraints accepted from this imagination.
    pub max_constraints: NonZeroU32,
    /// Maximum normalized patch resources produced by compilation.
    pub patch: PatchBudget,
}

impl Default for ImaginationBudget {
    fn default() -> Self {
        Self {
            max_entities: NonZeroU32::new(64).expect("constant is non-zero"),
            max_relations: NonZeroU32::new(128).expect("constant is non-zero"),
            max_constraints: NonZeroU32::new(64).expect("constant is non-zero"),
            patch: PatchBudget::default(),
        }
    }
}

/// Minimal semantic description of one built-in primitive entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaginedEntity {
    /// Local non-empty key used by relations and deterministic ordering.
    pub key: SceneText,
    /// Preferred stable identity, or `None` for deterministic derivation.
    pub preferred_id: Option<StableEntityId>,
    /// Optional display name; the local key is substituted when absent.
    pub name: Option<SceneText>,
    /// Required primitive geometry.
    pub primitive: PrimitiveComponent,
    /// Optional explicit transform; identity is substituted when absent.
    pub transform: Option<LocalTransform>,
    /// Optional explicit material; a documented neutral material is substituted when absent.
    pub material: Option<MaterialComponent>,
}

/// Deterministic relation subset supported by the initial compiler.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "relation",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ImaginationRelation {
    /// Parent one imagined entity beneath another.
    Parent {
        /// Child local key.
        child: SceneText,
        /// Parent local key.
        parent: SceneText,
    },
    /// Place one imagined entity directly above another along positive Y.
    Above {
        /// Entity to move.
        subject: SceneText,
        /// Entity used as the placement anchor.
        anchor: SceneText,
        /// Non-negative gap between the primitive bounds.
        clearance: NonNegativeF32,
    },
    /// Place one imagined entity beside another along positive X.
    RightOf {
        /// Entity to move.
        subject: SceneText,
        /// Entity used as the placement anchor.
        anchor: SceneText,
        /// Non-negative gap between the primitive bounds.
        gap: NonNegativeF32,
    },
}

impl ImaginationRelation {
    fn text_bytes(&self) -> u64 {
        let (first, second) = match self {
            Self::Parent { child, parent } => (child, parent),
            Self::Above {
                subject, anchor, ..
            }
            | Self::RightOf {
                subject, anchor, ..
            } => (subject, anchor),
        };
        text_len(first).saturating_add(text_len(second))
    }

    fn logical_size_bytes(&self) -> u64 {
        let scalar = match self {
            Self::Parent { .. } => 0,
            Self::Above { .. } | Self::RightOf { .. } => 4,
        };
        1_u64
            .saturating_add(8)
            .saturating_add(self.text_bytes())
            .saturating_add(scalar)
    }

    fn adds_patch_operation(&self) -> bool {
        matches!(self, Self::Parent { .. })
    }
}

/// Scene-view precondition checked before compilation emits a patch.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "constraint",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ImaginationConstraint {
    /// Require a stable entity to exist in the supplied scene view.
    EntityExists {
        /// Stable identity that must already exist.
        entity_id: StableEntityId,
    },
    /// Require a stable entity not to exist in the supplied scene view.
    EntityAbsent {
        /// Stable identity that must remain available.
        entity_id: StableEntityId,
    },
}

/// Versioned semantic request compiled into one atomic scene patch.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImaginationEnvelope {
    /// Public schema version.
    pub schema_version: SchemaVersion,
    /// Identity of this semantic request.
    pub imagination_id: ImaginationId,
    /// Transaction identity assigned to the normalized patch.
    pub transaction_id: TransactionId,
    /// Key used for gateway and world idempotency routing.
    pub idempotency_key: IdempotencyKey,
    /// Exact scene revision represented by the compiler view.
    pub base_revision: SceneRevision,
    /// Admission behavior for the bounded command queue.
    pub delivery: DeliverySemantic,
    /// Explicit deterministic seed used for derived identities and tie-breaking.
    pub seed: u64,
    /// Sender-declared compilation and output bounds.
    pub declared_budget: ImaginationBudget,
    /// Primitive entities, normalized by local key during compilation.
    pub entities: Vec<ImaginedEntity>,
    /// Bounded supported relation subset.
    pub relations: Vec<ImaginationRelation>,
    /// Bounded scene-view preconditions.
    pub constraints: Vec<ImaginationConstraint>,
}

impl ImaginationEnvelope {
    /// Validates schema invariants and declared/runtime compilation bounds.
    pub fn validate_with_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        Validate::validate(self, limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(&self, limits: &RuntimeLimits) -> Result<Vec<u8>, CodecError> {
        codec::encode(self, limits)
    }

    /// Decodes bounded JSON and validates the resulting imagination.
    pub fn from_json(encoded: &[u8], limits: &RuntimeLimits) -> Result<Self, CodecError> {
        codec::decode(encoded, limits)
    }
}

impl Validate for ImaginationEnvelope {
    fn validate(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        validate_schema(self.schema_version)?;
        self.validate_collection_limits(limits)?;
        validate_patch_budget(self.declared_budget.patch, limits)?;
        self.validate_entities_and_output(limits)?;
        if self.logical_size_bytes() > limits.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "imagination",
            ));
        }
        Ok(())
    }
}

impl ImaginationEnvelope {
    fn validate_collection_limits(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        let entity_count = count(self.entities.len());
        if entity_count == 0 {
            return Err(ValidationError::new(
                DiagnosticCode::EmptyImagination,
                "entities",
            ));
        }
        if entity_count > self.declared_budget.max_entities.get()
            || entity_count > limits.max_imagination_entities.get()
            || self.declared_budget.max_entities.get() > limits.max_imagination_entities.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::ImaginationEntityLimitExceeded,
                "entities",
            ));
        }

        let relation_count = count(self.relations.len());
        if relation_count > self.declared_budget.max_relations.get()
            || relation_count > limits.max_imagination_relations.get()
            || self.declared_budget.max_relations.get() > limits.max_imagination_relations.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::ImaginationRelationLimitExceeded,
                "relations",
            ));
        }

        let constraint_count = count(self.constraints.len());
        if constraint_count > self.declared_budget.max_constraints.get()
            || constraint_count > limits.max_imagination_constraints.get()
            || self.declared_budget.max_constraints.get() > limits.max_imagination_constraints.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::ImaginationConstraintLimitExceeded,
                "constraints",
            ));
        }
        Ok(())
    }

    fn validate_entities_and_output(&self, limits: &RuntimeLimits) -> Result<(), ValidationError> {
        let entity_count = count(self.entities.len());
        let mut keys = BTreeSet::new();
        let mut text_bytes = u64::try_from(self.delivery.text_bytes()).unwrap_or(u64::MAX);
        let mut output_text_bytes = text_bytes;
        for entity in &self.entities {
            if !keys.insert(&entity.key) {
                return Err(ValidationError::new(
                    DiagnosticCode::DuplicateImaginationEntity,
                    "entities[].key",
                ));
            }
            ComponentValue::Primitive(entity.primitive).validate()?;
            if let Some(transform) = entity.transform {
                ComponentValue::LocalTransform(transform).validate()?;
            }
            if let Some(material) = entity.material {
                ComponentValue::Material(material).validate()?;
            }
            text_bytes = text_bytes.saturating_add(text_len(&entity.key));
            if let Some(name) = &entity.name {
                text_bytes = text_bytes.saturating_add(text_len(name));
                output_text_bytes = output_text_bytes.saturating_add(text_len(name));
            } else {
                output_text_bytes = output_text_bytes.saturating_add(text_len(&entity.key));
            }
        }
        for relation in &self.relations {
            text_bytes = text_bytes.saturating_add(relation.text_bytes());
        }

        if text_bytes > limits.max_text_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::TextLimitExceeded,
                "imagination.text",
            ));
        }
        if output_text_bytes > self.declared_budget.patch.max_text_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::TextLimitExceeded,
                "declared_budget.patch.max_text_bytes",
            ));
        }

        let output_operations = entity_count.saturating_add(
            u32::try_from(
                self.relations
                    .iter()
                    .filter(|relation| relation.adds_patch_operation())
                    .count(),
            )
            .unwrap_or(u32::MAX),
        );
        if output_operations > self.declared_budget.patch.max_operations.get()
            || output_operations > limits.max_operations.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::OperationLimitExceeded,
                "declared_budget.patch.max_operations",
            ));
        }
        let output_components = entity_count.saturating_mul(4);
        if limits.max_components_per_entity.get() < 4
            || output_components > self.declared_budget.patch.max_components.get()
            || output_components > limits.max_components.get()
        {
            return Err(ValidationError::new(
                DiagnosticCode::ComponentLimitExceeded,
                "declared_budget.patch.max_components",
            ));
        }
        if self.normalized_patch_size_bytes() > self.declared_budget.patch.max_decoded_bytes.get() {
            return Err(ValidationError::new(
                DiagnosticCode::DecodedSizeLimitExceeded,
                "declared_budget.patch.max_decoded_bytes",
            ));
        }

        Ok(())
    }

    fn normalized_patch_size_bytes(&self) -> u64 {
        const PATCH_FIXED_BYTES: u64 = 2 + 16 + 16 + 8 + 1 + 24 + 4;
        const CREATE_FIXED_BYTES: u64 = 1 + 16 + 4;
        const TRANSFORM_COMPONENT_BYTES: u64 = 1 + (10 * 4);
        const PRIMITIVE_COMPONENT_BYTES: u64 = 1 + 1 + (3 * 4);
        const MATERIAL_COMPONENT_BYTES: u64 = 1 + (6 * 4);
        const REPARENT_OPERATION_BYTES: u64 = 1 + 16 + 1 + 16;

        let creates = self.entities.iter().fold(0_u64, |total, entity| {
            let name = entity.name.as_ref().unwrap_or(&entity.key);
            total
                .saturating_add(CREATE_FIXED_BYTES)
                .saturating_add(1 + 4 + text_len(name))
                .saturating_add(TRANSFORM_COMPONENT_BYTES)
                .saturating_add(PRIMITIVE_COMPONENT_BYTES)
                .saturating_add(MATERIAL_COMPONENT_BYTES)
        });
        let reparents = u64::from(count(
            self.relations
                .iter()
                .filter(|relation| relation.adds_patch_operation())
                .count(),
        ))
        .saturating_mul(REPARENT_OPERATION_BYTES);
        PATCH_FIXED_BYTES
            .saturating_add(self.delivery.logical_size_bytes())
            .saturating_add(creates)
            .saturating_add(reparents)
    }

    fn logical_size_bytes(&self) -> u64 {
        let entities = self.entities.iter().fold(0_u64, |total, entity| {
            total
                .saturating_add(16)
                .saturating_add(text_len(&entity.key))
                .saturating_add(entity.name.as_ref().map_or(0, text_len))
                .saturating_add(1 + u64::from(entity.preferred_id.is_some()) * 16)
                .saturating_add(1 + (3 * 4))
                .saturating_add(1 + u64::from(entity.transform.is_some()) * (10 * 4))
                .saturating_add(1 + u64::from(entity.material.is_some()) * (6 * 4))
        });
        let relations = self.relations.iter().fold(0_u64, |total, relation| {
            total.saturating_add(relation.logical_size_bytes())
        });
        2_u64
            .saturating_add(16 * 3)
            .saturating_add(8 * 2)
            .saturating_add(self.delivery.logical_size_bytes())
            .saturating_add(40)
            .saturating_add(4 * 3)
            .saturating_add(entities)
            .saturating_add(relations)
            .saturating_add(u64::from(count(self.constraints.len())) * 17)
    }
}

fn validate_patch_budget(
    budget: PatchBudget,
    limits: &RuntimeLimits,
) -> Result<(), ValidationError> {
    if budget.max_operations.get() > limits.max_operations.get() {
        return Err(ValidationError::new(
            DiagnosticCode::OperationLimitExceeded,
            "declared_budget.patch.max_operations",
        ));
    }
    if budget.max_components.get() > limits.max_components.get() {
        return Err(ValidationError::new(
            DiagnosticCode::ComponentLimitExceeded,
            "declared_budget.patch.max_components",
        ));
    }
    if budget.max_text_bytes.get() > limits.max_text_bytes.get() {
        return Err(ValidationError::new(
            DiagnosticCode::TextLimitExceeded,
            "declared_budget.patch.max_text_bytes",
        ));
    }
    if budget.max_decoded_bytes.get() > limits.max_decoded_bytes.get() {
        return Err(ValidationError::new(
            DiagnosticCode::DecodedSizeLimitExceeded,
            "declared_budget.patch.max_decoded_bytes",
        ));
    }
    Ok(())
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

fn text_len(value: &SceneText) -> u64 {
    u64::try_from(value.len_bytes()).unwrap_or(u64::MAX)
}
