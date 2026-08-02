use cogniform_protocol::{ImaginationId, ScenePatch, SceneRevision, SceneText, StableEntityId};

/// Stable explanation code for one compiler choice or substitution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// One bounded, machine-readable compiler explanation.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Stable reason a declared relation or constraint could not be resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

/// One unresolved relation or scene-view precondition.
#[derive(Debug, Clone, PartialEq, Eq)]
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

/// Pure compiler output containing either one patch or unresolved constraints.
#[derive(Debug, Clone, PartialEq)]
pub struct CompilationResult {
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
        self.patch.is_some()
    }
}
