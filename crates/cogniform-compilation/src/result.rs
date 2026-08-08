use core::cmp::Ordering;

use cogniform_protocol::{
    ImaginationId, ScenePatch, SceneRevision, SceneText, SchemaVersion, StableEntityId,
};
use serde::{Deserialize, Serialize};

use crate::{
    CompilationCodecError, CompilationLimits, CompilationValidationError,
    CompilationValidationKind, codec,
};

/// The only compilation-result schema version supported by this build.
pub const COMPILATION_SCHEMA_VERSION: SchemaVersion = SchemaVersion::V1;

/// Stable explanation code for one compiler choice or substitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum CompilationDecisionCode {
    /// A stable entity ID was derived from the request identity, seed, and key.
    GeneratedEntityId,
    /// An unavailable preferred ID was replaced with a deterministic derived ID.
    PreferredEntityIdSubstituted,
    /// The local entity key was used as the display name.
    DefaultName,
    /// An identity transform was supplied.
    DefaultTransform,
    /// The documented neutral primitive material was supplied.
    DefaultMaterial,
    /// A parent relation was normalized into a reparent operation.
    ParentRelationApplied,
    /// An above relation was resolved into an explicit transform.
    AboveRelationApplied,
    /// A right-of relation was resolved into an explicit transform.
    RightOfRelationApplied,
}

impl CompilationDecisionCode {
    const fn canonical_rank(self) -> u8 {
        match self {
            Self::GeneratedEntityId => 0,
            Self::PreferredEntityIdSubstituted => 1,
            Self::DefaultName => 2,
            Self::DefaultTransform => 3,
            Self::DefaultMaterial => 4,
            Self::ParentRelationApplied => 5,
            Self::AboveRelationApplied => 6,
            Self::RightOfRelationApplied => 7,
        }
    }
}

/// One bounded, machine-readable compiler explanation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilationDecision {
    /// Stable decision classification.
    pub code: CompilationDecisionCode,
    /// Local entity key affected by the choice.
    pub entity_key: SceneText,
    /// Original relation index when the choice resolves a relation.
    pub relation_index: Option<u32>,
    /// Concrete entity identity when the choice creates or substitutes one.
    pub entity_id: Option<StableEntityId>,
}

impl CompilationDecision {
    /// Compares decisions by the version-one canonical field and code order.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.entity_key
            .cmp(&other.entity_key)
            .then_with(|| self.code.canonical_rank().cmp(&other.code.canonical_rank()))
            .then_with(|| self.relation_index.cmp(&other.relation_index))
    }
}

/// Stable reason a declared relation or constraint could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum UnresolvedConstraintCode {
    /// A relation references a local entity key not declared by the request.
    UnknownEntityReference,
    /// A relation references the same entity as both subject and anchor/parent.
    SelfRelation,
    /// More than one relation tries to assign incompatible placement or parent state.
    ConflictingRelation,
    /// Parent relations contain a cycle.
    HierarchyCycle,
    /// Spatial placement relations contain a dependency cycle.
    PlacementCycle,
    /// A required stable entity is absent from the supplied scene view.
    RequiredEntityMissing,
    /// A stable entity required to be absent already exists.
    RequiredEntityPresent,
    /// Resolving a spatial relation would produce a non-finite transform.
    NonFinitePlacement,
    /// A spatial relation uses a rotated transform outside the initial axis-aligned subset.
    UnsupportedSpatialRotation,
}

impl UnresolvedConstraintCode {
    const fn canonical_rank(self) -> u8 {
        match self {
            Self::UnknownEntityReference => 0,
            Self::SelfRelation => 1,
            Self::ConflictingRelation => 2,
            Self::HierarchyCycle => 3,
            Self::PlacementCycle => 4,
            Self::RequiredEntityMissing => 5,
            Self::RequiredEntityPresent => 6,
            Self::NonFinitePlacement => 7,
            Self::UnsupportedSpatialRotation => 8,
        }
    }
}

/// One unresolved relation or scene-view precondition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct UnresolvedConstraint {
    /// Stable unresolved classification.
    pub code: UnresolvedConstraintCode,
    /// Original relation index, when relation-specific.
    pub relation_index: Option<u32>,
    /// Original constraint index, when precondition-specific.
    pub constraint_index: Option<u32>,
    /// Local subject/child key when available.
    pub entity_key: Option<SceneText>,
    /// Local anchor/parent key when available.
    pub related_key: Option<SceneText>,
    /// Stable scene entity involved in a precondition when available.
    pub entity_id: Option<StableEntityId>,
}

impl UnresolvedConstraint {
    /// Compares unresolved entries by the version-one canonical field and code order.
    #[must_use]
    pub fn canonical_cmp(&self, other: &Self) -> Ordering {
        self.relation_index
            .cmp(&other.relation_index)
            .then_with(|| self.constraint_index.cmp(&other.constraint_index))
            .then_with(|| self.code.canonical_rank().cmp(&other.code.canonical_rank()))
            .then_with(|| self.entity_key.cmp(&other.entity_key))
            .then_with(|| self.related_key.cmp(&other.related_key))
            .then_with(|| self.entity_id.cmp(&other.entity_id))
    }
}

/// Pure compiler output containing either one patch or unresolved constraints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CompilationResult {
    /// Public compilation-result schema version.
    pub schema_version: SchemaVersion,
    /// Identity of the source imagination.
    pub imagination_id: ImaginationId,
    /// Exact immutable scene revision used during compilation.
    pub scene_revision: SceneRevision,
    /// Normalized patch, absent when any relation or constraint is unresolved.
    pub patch: Option<ScenePatch>,
    /// Stable ordered compiler choices and substitutions.
    pub decisions: Vec<CompilationDecision>,
    /// Stable ordered unresolved relations and preconditions.
    pub unresolved: Vec<UnresolvedConstraint>,
}

impl CompilationResult {
    /// Reports whether compilation produced a patch with no unresolved items.
    #[must_use]
    pub const fn is_compiled(&self) -> bool {
        self.patch.is_some() && self.unresolved.is_empty()
    }

    /// Validates schema invariants and explicit report limits.
    pub fn validate_with_limits(
        &self,
        limits: &CompilationLimits,
    ) -> Result<(), CompilationValidationError> {
        self.validate(limits)
    }

    /// Encodes one validated canonical JSON line with a trailing LF.
    pub fn to_canonical_json(
        &self,
        limits: &CompilationLimits,
    ) -> Result<Vec<u8>, CompilationCodecError> {
        codec::encode(self, limits)
    }

    /// Decodes and validates one exact canonical JSON line under explicit limits.
    pub fn from_canonical_json(
        encoded: &[u8],
        limits: &CompilationLimits,
    ) -> Result<Self, CompilationCodecError> {
        codec::decode(encoded, limits)
    }

    pub(crate) fn validate(
        &self,
        limits: &CompilationLimits,
    ) -> Result<(), CompilationValidationError> {
        if self.schema_version != COMPILATION_SCHEMA_VERSION {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::UnsupportedSchema,
                "schema_version",
            ));
        }

        let decision_count = u32::try_from(self.decisions.len()).unwrap_or(u32::MAX);
        if decision_count > limits.max_decisions.get() {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::DecisionLimitExceeded,
                "decisions",
            ));
        }
        let unresolved_count = u32::try_from(self.unresolved.len()).unwrap_or(u32::MAX);
        if unresolved_count > limits.max_unresolved_constraints.get() {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::UnresolvedLimitExceeded,
                "unresolved",
            ));
        }

        match (&self.patch, self.unresolved.is_empty()) {
            (Some(patch), true) => {
                if patch.schema_version != self.schema_version {
                    return Err(CompilationValidationError::new(
                        CompilationValidationKind::PatchSchemaMismatch,
                        "patch.schema_version",
                    ));
                }
                if patch.base_revision != self.scene_revision {
                    return Err(CompilationValidationError::new(
                        CompilationValidationKind::PatchRevisionMismatch,
                        "patch.base_revision",
                    ));
                }
                patch
                    .validate_with_limits(&limits.patch_limits)
                    .map_err(|error| CompilationValidationError::protocol("patch", error))?;
            }
            (None, false) => {}
            (Some(_), false) | (None, true) => {
                return Err(CompilationValidationError::new(
                    CompilationValidationKind::InvalidOutcome,
                    "patch",
                ));
            }
        }

        validate_decisions(&self.decisions)?;
        validate_unresolved(&self.unresolved)?;

        if self.text_bytes() > limits.max_text_bytes.get() {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::TextLimitExceeded,
                "result",
            ));
        }
        if self.logical_size_bytes() > limits.max_decoded_bytes.get() {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::DecodedSizeLimitExceeded,
                "result",
            ));
        }

        Ok(())
    }

    fn text_bytes(&self) -> u64 {
        let patch = self.patch.as_ref().map_or(0, ScenePatch::text_bytes);
        let decisions = self.decisions.iter().fold(0_u64, |total, decision| {
            total.saturating_add(u64::try_from(decision.entity_key.len_bytes()).unwrap_or(u64::MAX))
        });
        self.unresolved
            .iter()
            .fold(patch.saturating_add(decisions), |total, unresolved| {
                total
                    .saturating_add(optional_text_bytes(unresolved.entity_key.as_ref()))
                    .saturating_add(optional_text_bytes(unresolved.related_key.as_ref()))
            })
    }

    fn logical_size_bytes(&self) -> u64 {
        let patch = self
            .patch
            .as_ref()
            .map_or(0, ScenePatch::logical_size_bytes);
        let decisions = self.decisions.iter().fold(0_u64, |total, decision| {
            total.saturating_add(decision_logical_size(decision))
        });
        self.unresolved.iter().fold(
            35_u64.saturating_add(patch).saturating_add(decisions),
            |total, unresolved| total.saturating_add(unresolved_logical_size(unresolved)),
        )
    }
}

fn validate_decisions(decisions: &[CompilationDecision]) -> Result<(), CompilationValidationError> {
    for decision in decisions {
        let valid = match decision.code {
            CompilationDecisionCode::GeneratedEntityId
            | CompilationDecisionCode::PreferredEntityIdSubstituted => {
                decision.relation_index.is_none() && decision.entity_id.is_some()
            }
            CompilationDecisionCode::DefaultName
            | CompilationDecisionCode::DefaultTransform
            | CompilationDecisionCode::DefaultMaterial => {
                decision.relation_index.is_none() && decision.entity_id.is_none()
            }
            CompilationDecisionCode::ParentRelationApplied
            | CompilationDecisionCode::AboveRelationApplied
            | CompilationDecisionCode::RightOfRelationApplied => {
                decision.relation_index.is_some() && decision.entity_id.is_none()
            }
        };
        if !valid {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::InvalidDecisionShape,
                "decisions[]",
            ));
        }
    }
    for pair in decisions.windows(2) {
        match pair[0].canonical_cmp(&pair[1]) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(CompilationValidationError::new(
                    CompilationValidationKind::DuplicateDecision,
                    "decisions",
                ));
            }
            Ordering::Greater => {
                return Err(CompilationValidationError::new(
                    CompilationValidationKind::NonCanonicalDecisionOrder,
                    "decisions",
                ));
            }
        }
    }
    Ok(())
}

fn validate_unresolved(
    unresolved: &[UnresolvedConstraint],
) -> Result<(), CompilationValidationError> {
    for issue in unresolved {
        let valid = match issue.code {
            UnresolvedConstraintCode::UnknownEntityReference
            | UnresolvedConstraintCode::SelfRelation
            | UnresolvedConstraintCode::ConflictingRelation
            | UnresolvedConstraintCode::HierarchyCycle
            | UnresolvedConstraintCode::PlacementCycle
            | UnresolvedConstraintCode::NonFinitePlacement
            | UnresolvedConstraintCode::UnsupportedSpatialRotation => {
                issue.relation_index.is_some()
                    && issue.constraint_index.is_none()
                    && issue.entity_key.is_some()
                    && issue.related_key.is_some()
                    && issue.entity_id.is_none()
            }
            UnresolvedConstraintCode::RequiredEntityMissing
            | UnresolvedConstraintCode::RequiredEntityPresent => {
                issue.relation_index.is_none()
                    && issue.constraint_index.is_some()
                    && issue.entity_key.is_none()
                    && issue.related_key.is_none()
                    && issue.entity_id.is_some()
            }
        };
        if !valid {
            return Err(CompilationValidationError::new(
                CompilationValidationKind::InvalidUnresolvedShape,
                "unresolved[]",
            ));
        }
    }
    for pair in unresolved.windows(2) {
        match pair[0].canonical_cmp(&pair[1]) {
            Ordering::Less => {}
            Ordering::Equal => {
                return Err(CompilationValidationError::new(
                    CompilationValidationKind::DuplicateUnresolved,
                    "unresolved",
                ));
            }
            Ordering::Greater => {
                return Err(CompilationValidationError::new(
                    CompilationValidationKind::NonCanonicalUnresolvedOrder,
                    "unresolved",
                ));
            }
        }
    }
    Ok(())
}

fn optional_text_bytes(value: Option<&SceneText>) -> u64 {
    value.map_or(0, |text| {
        u64::try_from(text.len_bytes()).unwrap_or(u64::MAX)
    })
}

fn decision_logical_size(decision: &CompilationDecision) -> u64 {
    8_u64
        .saturating_add(u64::try_from(decision.entity_key.len_bytes()).unwrap_or(u64::MAX))
        .saturating_add(u64::from(decision.relation_index.is_some()) * 4)
        .saturating_add(u64::from(decision.entity_id.is_some()) * 16)
}

fn unresolved_logical_size(issue: &UnresolvedConstraint) -> u64 {
    14_u64
        .saturating_add(u64::from(issue.relation_index.is_some()) * 4)
        .saturating_add(u64::from(issue.constraint_index.is_some()) * 4)
        .saturating_add(optional_text_bytes(issue.entity_key.as_ref()))
        .saturating_add(optional_text_bytes(issue.related_key.as_ref()))
        .saturating_add(u64::from(issue.entity_id.is_some()) * 16)
}
