use core::num::NonZeroU32;
use std::collections::{BTreeMap, BTreeSet};

use cogniform_protocol::{
    ColorRgba, ComponentValue, ConflictPolicy, CreateEntity, FiniteF32, ImaginationConstraint,
    ImaginationEnvelope, ImaginationRelation, LocalTransform, MaterialComponent, NameComponent,
    PositiveF32, PositiveVec3, Quaternion, ReparentEntity, RuntimeLimits, SceneOperation,
    ScenePatch, SceneRevision, SceneText, StableEntityId, UnitF32, Vec3,
};
use sha2::{Digest, Sha256};

use crate::{
    CompilationDecision, CompilationDecisionCode, CompilationResult, CompileError,
    UnresolvedConstraint, UnresolvedConstraintCode,
};

const ENTITY_ID_DOMAIN: &[u8] = b"cogniform.imagination.entity.v1\0";

/// Immutable stable-identity view supplied to the pure compiler.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompilationSceneView {
    revision: SceneRevision,
    entity_ids: BTreeSet<StableEntityId>,
}

impl CompilationSceneView {
    /// Creates a deterministic scene view from stable IDs.
    #[must_use]
    pub fn new(
        revision: SceneRevision,
        entity_ids: impl IntoIterator<Item = StableEntityId>,
    ) -> Self {
        Self {
            revision,
            entity_ids: entity_ids.into_iter().collect(),
        }
    }

    /// Returns the represented scene revision.
    #[must_use]
    pub const fn revision(&self) -> SceneRevision {
        self.revision
    }

    /// Reports whether a stable identity exists in the represented scene.
    #[must_use]
    pub fn contains(&self, entity_id: StableEntityId) -> bool {
        self.entity_ids.contains(&entity_id)
    }
}

/// Bounds used by the deterministic compiler implementation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompilerConfig {
    /// Public runtime limits applied before compilation.
    pub runtime_limits: RuntimeLimits,
    /// Maximum deterministic hash attempts used to avoid stable-ID collisions.
    pub max_entity_id_attempts: NonZeroU32,
}

impl CompilerConfig {
    /// Creates a compiler configuration for the active runtime limits.
    #[must_use]
    pub const fn new(runtime_limits: RuntimeLimits) -> Self {
        Self {
            runtime_limits,
            max_entity_id_attempts: NonZeroU32::new(32).expect("constant is non-zero"),
        }
    }
}

impl Default for CompilerConfig {
    fn default() -> Self {
        Self::new(RuntimeLimits::default())
    }
}

/// Stateless deterministic compiler for the initial primitive relation subset.
#[derive(Debug, Clone, Copy)]
pub struct DeterministicCompiler {
    config: CompilerConfig,
}

impl DeterministicCompiler {
    /// Creates a compiler with explicit deterministic bounds.
    #[must_use]
    pub const fn new(config: CompilerConfig) -> Self {
        Self { config }
    }

    /// Compiles one imagination against an immutable exact-revision scene view.
    pub fn compile(
        &self,
        imagination: &ImaginationEnvelope,
        scene: &CompilationSceneView,
    ) -> Result<CompilationResult, CompileError> {
        imagination
            .validate_with_limits(&self.config.runtime_limits)
            .map_err(CompileError::InvalidImagination)?;
        if imagination.base_revision != scene.revision {
            return Err(CompileError::SceneRevisionMismatch {
                requested: imagination.base_revision,
                actual: scene.revision,
            });
        }

        let (mut entities, mut decisions) = self.plan_entities(imagination, scene)?;
        let mut unresolved = check_scene_constraints(imagination, scene);
        let key_index: BTreeMap<_, _> = entities
            .iter()
            .enumerate()
            .map(|(index, entity)| (entity.key.clone(), index))
            .collect();
        assign_relations(imagination, &mut entities, &key_index, &mut unresolved);
        reject_parented_placement_anchors(&entities, &key_index, &mut unresolved);
        mark_cycles(&entities, &key_index, &mut unresolved);

        if unresolved.is_empty() {
            resolve_placements(&mut entities, &key_index, &mut unresolved);
        }
        if !unresolved.is_empty() {
            unresolved.sort_by(unresolved_order);
            unresolved.dedup();
            return Ok(CompilationResult {
                imagination_id: imagination.imagination_id,
                scene_revision: scene.revision,
                patch: None,
                decisions,
                unresolved,
            });
        }

        append_relation_decisions(&entities, &mut decisions);
        decisions.sort_by(decision_order);
        let patch = build_patch(imagination, &entities);
        patch
            .validate_with_limits(&self.config.runtime_limits)
            .map_err(CompileError::InvalidNormalizedPatch)?;

        Ok(CompilationResult {
            imagination_id: imagination.imagination_id,
            scene_revision: scene.revision,
            patch: Some(patch),
            decisions,
            unresolved,
        })
    }

    fn plan_entities(
        &self,
        imagination: &ImaginationEnvelope,
        scene: &CompilationSceneView,
    ) -> Result<(Vec<EntityPlan>, Vec<CompilationDecision>), CompileError> {
        let mut decisions = Vec::new();
        let mut used_ids = BTreeSet::new();
        let mut entities = Vec::with_capacity(imagination.entities.len());
        let mut sorted_entities: Vec<_> = imagination.entities.iter().collect();
        sorted_entities.sort_by(|left, right| left.key.cmp(&right.key));

        for (normalized_index, entity) in sorted_entities.into_iter().enumerate() {
            let preferred_unavailable = entity
                .preferred_id
                .is_some_and(|id| scene.contains(id) || used_ids.contains(&id));
            let entity_id =
                if let Some(preferred) = entity.preferred_id.filter(|_| !preferred_unavailable) {
                    preferred
                } else {
                    let derived = self.derive_entity_id(
                        imagination,
                        &entity.key,
                        u32::try_from(normalized_index).unwrap_or(u32::MAX),
                        scene,
                        &used_ids,
                    )?;
                    decisions.push(CompilationDecision {
                        code: if preferred_unavailable {
                            CompilationDecisionCode::PreferredEntityIdSubstituted
                        } else {
                            CompilationDecisionCode::GeneratedEntityId
                        },
                        entity_key: entity.key.clone(),
                        relation_index: None,
                        entity_id: Some(derived),
                    });
                    derived
                };
            used_ids.insert(entity_id);
            entities.push(plan_entity(entity, entity_id, &mut decisions));
        }
        Ok((entities, decisions))
    }

    fn derive_entity_id(
        &self,
        imagination: &ImaginationEnvelope,
        key: &SceneText,
        entity_index: u32,
        scene: &CompilationSceneView,
        used_ids: &BTreeSet<StableEntityId>,
    ) -> Result<StableEntityId, CompileError> {
        for attempt in 0..self.config.max_entity_id_attempts.get() {
            let mut hasher = Sha256::new();
            hasher.update(ENTITY_ID_DOMAIN);
            hasher.update(imagination.imagination_id.get().to_be_bytes());
            hasher.update(imagination.seed.to_be_bytes());
            hasher.update(
                u64::try_from(key.len_bytes())
                    .unwrap_or(u64::MAX)
                    .to_be_bytes(),
            );
            hasher.update(key.as_str().as_bytes());
            hasher.update(attempt.to_be_bytes());
            let digest = hasher.finalize();
            let mut bytes = [0_u8; 16];
            bytes.copy_from_slice(&digest[..16]);
            if let Ok(candidate) = StableEntityId::new(u128::from_be_bytes(bytes))
                && !scene.contains(candidate)
                && !used_ids.contains(&candidate)
            {
                return Ok(candidate);
            }
        }
        Err(CompileError::EntityIdDerivationExhausted {
            entity_index,
            attempts: self.config.max_entity_id_attempts.get(),
        })
    }
}

#[derive(Debug, Clone)]
struct EntityPlan {
    key: SceneText,
    entity_id: StableEntityId,
    name: SceneText,
    primitive: cogniform_protocol::PrimitiveComponent,
    transform: LocalTransform,
    material: MaterialComponent,
    parent: Option<SceneText>,
    parent_relation: Option<u32>,
    placement: Option<Placement>,
}

fn plan_entity(
    entity: &cogniform_protocol::ImaginedEntity,
    entity_id: StableEntityId,
    decisions: &mut Vec<CompilationDecision>,
) -> EntityPlan {
    let name = entity.name.clone().unwrap_or_else(|| {
        decisions.push(default_decision(
            CompilationDecisionCode::DefaultName,
            &entity.key,
        ));
        entity.key.clone()
    });
    let transform = entity.transform.unwrap_or_else(|| {
        decisions.push(default_decision(
            CompilationDecisionCode::DefaultTransform,
            &entity.key,
        ));
        identity_transform()
    });
    let material = entity.material.unwrap_or_else(|| {
        decisions.push(default_decision(
            CompilationDecisionCode::DefaultMaterial,
            &entity.key,
        ));
        neutral_material()
    });
    EntityPlan {
        key: entity.key.clone(),
        entity_id,
        name,
        primitive: entity.primitive,
        transform,
        material,
        parent: None,
        parent_relation: None,
        placement: None,
    }
}

#[derive(Debug, Clone)]
struct Placement {
    relation_index: u32,
    anchor: SceneText,
    kind: PlacementKind,
}

#[derive(Debug, Clone, Copy)]
enum PlacementKind {
    Above { clearance: f32 },
    RightOf { gap: f32 },
}

fn check_scene_constraints(
    imagination: &ImaginationEnvelope,
    scene: &CompilationSceneView,
) -> Vec<UnresolvedConstraint> {
    imagination
        .constraints
        .iter()
        .enumerate()
        .filter_map(|(index, constraint)| {
            let (code, entity_id) = match constraint {
                ImaginationConstraint::EntityExists { entity_id }
                    if !scene.contains(*entity_id) =>
                {
                    (UnresolvedConstraintCode::RequiredEntityMissing, *entity_id)
                }
                ImaginationConstraint::EntityAbsent { entity_id } if scene.contains(*entity_id) => {
                    (UnresolvedConstraintCode::RequiredEntityPresent, *entity_id)
                }
                ImaginationConstraint::EntityExists { .. }
                | ImaginationConstraint::EntityAbsent { .. } => return None,
            };
            Some(UnresolvedConstraint {
                code,
                relation_index: None,
                constraint_index: Some(u32::try_from(index).unwrap_or(u32::MAX)),
                entity_key: None,
                related_key: None,
                entity_id: Some(entity_id),
            })
        })
        .collect()
}

fn assign_relations(
    imagination: &ImaginationEnvelope,
    entities: &mut [EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    unresolved: &mut Vec<UnresolvedConstraint>,
) {
    for (index, relation) in imagination.relations.iter().enumerate() {
        let relation_index = u32::try_from(index).unwrap_or(u32::MAX);
        match relation {
            ImaginationRelation::Parent { child, parent } => {
                let Some(&child_index) = key_index.get(child) else {
                    unresolved.push(unknown_reference(relation_index, child, parent));
                    continue;
                };
                if !key_index.contains_key(parent) {
                    unresolved.push(unknown_reference(relation_index, child, parent));
                    continue;
                }
                if child == parent {
                    unresolved.push(relation_issue(
                        UnresolvedConstraintCode::SelfRelation,
                        relation_index,
                        child,
                        parent,
                    ));
                    continue;
                }
                let child_plan = &mut entities[child_index];
                if child_plan.parent.is_some() || child_plan.placement.is_some() {
                    unresolved.push(relation_issue(
                        UnresolvedConstraintCode::ConflictingRelation,
                        relation_index,
                        child,
                        parent,
                    ));
                    continue;
                }
                child_plan.parent = Some(parent.clone());
                child_plan.parent_relation = Some(relation_index);
            }
            ImaginationRelation::Above {
                subject,
                anchor,
                clearance,
            } => assign_placement(
                entities,
                key_index,
                unresolved,
                relation_index,
                subject,
                anchor,
                PlacementKind::Above {
                    clearance: clearance.get(),
                },
            ),
            ImaginationRelation::RightOf {
                subject,
                anchor,
                gap,
            } => assign_placement(
                entities,
                key_index,
                unresolved,
                relation_index,
                subject,
                anchor,
                PlacementKind::RightOf { gap: gap.get() },
            ),
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn assign_placement(
    entities: &mut [EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    unresolved: &mut Vec<UnresolvedConstraint>,
    relation_index: u32,
    subject: &SceneText,
    anchor: &SceneText,
    kind: PlacementKind,
) {
    let Some(&subject_index) = key_index.get(subject) else {
        unresolved.push(unknown_reference(relation_index, subject, anchor));
        return;
    };
    if !key_index.contains_key(anchor) {
        unresolved.push(unknown_reference(relation_index, subject, anchor));
        return;
    }
    if subject == anchor {
        unresolved.push(relation_issue(
            UnresolvedConstraintCode::SelfRelation,
            relation_index,
            subject,
            anchor,
        ));
        return;
    }
    let subject_plan = &mut entities[subject_index];
    if subject_plan.parent.is_some() || subject_plan.placement.is_some() {
        unresolved.push(relation_issue(
            UnresolvedConstraintCode::ConflictingRelation,
            relation_index,
            subject,
            anchor,
        ));
        return;
    }
    subject_plan.placement = Some(Placement {
        relation_index,
        anchor: anchor.clone(),
        kind,
    });
}

fn mark_cycles(
    entities: &[EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    unresolved: &mut Vec<UnresolvedConstraint>,
) {
    mark_relation_cycles(
        entities,
        key_index,
        |entity| {
            entity.parent.as_ref().map(|parent| {
                (
                    parent,
                    entity.parent_relation.expect("parent relation is recorded"),
                )
            })
        },
        UnresolvedConstraintCode::HierarchyCycle,
        unresolved,
    );
    mark_relation_cycles(
        entities,
        key_index,
        |entity| {
            entity
                .placement
                .as_ref()
                .map(|placement| (&placement.anchor, placement.relation_index))
        },
        UnresolvedConstraintCode::PlacementCycle,
        unresolved,
    );
}

fn reject_parented_placement_anchors(
    entities: &[EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    unresolved: &mut Vec<UnresolvedConstraint>,
) {
    for entity in entities {
        let Some(placement) = &entity.placement else {
            continue;
        };
        let anchor = &entities[key_index[&placement.anchor]];
        if anchor.parent.is_some() {
            unresolved.push(relation_issue(
                UnresolvedConstraintCode::ConflictingRelation,
                placement.relation_index,
                &entity.key,
                &placement.anchor,
            ));
        }
    }
}

fn mark_relation_cycles<'a>(
    entities: &'a [EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    next: impl Fn(&'a EntityPlan) -> Option<(&'a SceneText, u32)>,
    code: UnresolvedConstraintCode,
    unresolved: &mut Vec<UnresolvedConstraint>,
) {
    let mut reported = BTreeSet::new();
    for start in 0..entities.len() {
        let mut path: Vec<usize> = Vec::new();
        let mut positions: BTreeMap<usize, usize> = BTreeMap::new();
        let mut current = start;
        loop {
            if let Some(&cycle_start) = positions.get(&current) {
                for &cycle_index in &path[cycle_start..] {
                    if let Some((related, relation_index)) = next(&entities[cycle_index])
                        && reported.insert(relation_index)
                    {
                        unresolved.push(relation_issue(
                            code,
                            relation_index,
                            &entities[cycle_index].key,
                            related,
                        ));
                    }
                }
                break;
            }
            positions.insert(current, path.len());
            path.push(current);
            let Some((related, _)) = next(&entities[current]) else {
                break;
            };
            current = key_index[related];
        }
    }
}

fn resolve_placements(
    entities: &mut [EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    unresolved: &mut Vec<UnresolvedConstraint>,
) {
    let mut resolved = vec![false; entities.len()];
    for index in 0..entities.len() {
        if let Err(failure) = resolve_placement(index, entities, key_index, &mut resolved) {
            unresolved.push(UnresolvedConstraint {
                code: failure.code,
                relation_index: Some(failure.relation_index),
                constraint_index: None,
                entity_key: Some(failure.subject),
                related_key: Some(failure.anchor),
                entity_id: None,
            });
        }
    }
}

#[derive(Debug, Clone)]
struct PlacementResolutionFailure {
    code: UnresolvedConstraintCode,
    relation_index: u32,
    subject: SceneText,
    anchor: SceneText,
}

fn resolve_placement(
    index: usize,
    entities: &mut [EntityPlan],
    key_index: &BTreeMap<SceneText, usize>,
    resolved: &mut [bool],
) -> Result<(), PlacementResolutionFailure> {
    if resolved[index] {
        return Ok(());
    }
    let Some(placement) = entities[index].placement.clone() else {
        resolved[index] = true;
        return Ok(());
    };
    let anchor_index = key_index[&placement.anchor];
    resolve_placement(anchor_index, entities, key_index, resolved)?;

    let subject_transform = entities[index].transform;
    let subject_dimensions = entities[index].primitive.dimensions;
    let anchor_transform = entities[anchor_index].transform;
    let anchor_dimensions = entities[anchor_index].primitive.dimensions;
    if !is_unrotated(subject_transform) || !is_unrotated(anchor_transform) {
        return Err(placement_failure(
            UnresolvedConstraintCode::UnsupportedSpatialRotation,
            &entities[index],
            &placement,
        ));
    }
    let mut translation = subject_transform.translation;
    match placement.kind {
        PlacementKind::Above { clearance } => {
            let value = anchor_transform.translation.y.get()
                + (anchor_dimensions.y.get() * anchor_transform.scale.y.get() * 0.5)
                + clearance
                + (subject_dimensions.y.get() * subject_transform.scale.y.get() * 0.5);
            translation.x = anchor_transform.translation.x;
            translation.y = FiniteF32::new(value).map_err(|_| {
                placement_failure(
                    UnresolvedConstraintCode::NonFinitePlacement,
                    &entities[index],
                    &placement,
                )
            })?;
            translation.z = anchor_transform.translation.z;
        }
        PlacementKind::RightOf { gap } => {
            let value = anchor_transform.translation.x.get()
                + (anchor_dimensions.x.get() * anchor_transform.scale.x.get() * 0.5)
                + gap
                + (subject_dimensions.x.get() * subject_transform.scale.x.get() * 0.5);
            translation.x = FiniteF32::new(value).map_err(|_| {
                placement_failure(
                    UnresolvedConstraintCode::NonFinitePlacement,
                    &entities[index],
                    &placement,
                )
            })?;
            translation.y = anchor_transform.translation.y;
            translation.z = anchor_transform.translation.z;
        }
    }
    entities[index].transform.translation = translation;
    resolved[index] = true;
    Ok(())
}

fn is_unrotated(transform: LocalTransform) -> bool {
    transform.rotation.x.get() == 0.0
        && transform.rotation.y.get() == 0.0
        && transform.rotation.z.get() == 0.0
}

fn placement_failure(
    code: UnresolvedConstraintCode,
    subject: &EntityPlan,
    placement: &Placement,
) -> PlacementResolutionFailure {
    PlacementResolutionFailure {
        code,
        relation_index: placement.relation_index,
        subject: subject.key.clone(),
        anchor: placement.anchor.clone(),
    }
}

fn append_relation_decisions(entities: &[EntityPlan], decisions: &mut Vec<CompilationDecision>) {
    for entity in entities {
        if let Some(relation_index) = entity.parent_relation {
            decisions.push(CompilationDecision {
                code: CompilationDecisionCode::ParentRelationApplied,
                entity_key: entity.key.clone(),
                relation_index: Some(relation_index),
                entity_id: None,
            });
        }
        if let Some(placement) = &entity.placement {
            decisions.push(CompilationDecision {
                code: match placement.kind {
                    PlacementKind::Above { .. } => CompilationDecisionCode::AboveRelationApplied,
                    PlacementKind::RightOf { .. } => {
                        CompilationDecisionCode::RightOfRelationApplied
                    }
                },
                entity_key: entity.key.clone(),
                relation_index: Some(placement.relation_index),
                entity_id: None,
            });
        }
    }
}

fn build_patch(imagination: &ImaginationEnvelope, entities: &[EntityPlan]) -> ScenePatch {
    let mut operations = Vec::with_capacity(
        entities.len()
            + entities
                .iter()
                .filter(|entity| entity.parent.is_some())
                .count(),
    );
    for entity in entities {
        operations.push(SceneOperation::Create(CreateEntity {
            entity_id: entity.entity_id,
            components: vec![
                ComponentValue::Name(NameComponent {
                    value: entity.name.clone(),
                }),
                ComponentValue::LocalTransform(entity.transform),
                ComponentValue::Primitive(entity.primitive),
                ComponentValue::Material(entity.material),
            ],
        }));
    }
    for entity in entities {
        if let Some(parent) = &entity.parent {
            let parent_id = entities
                .iter()
                .find(|candidate| &candidate.key == parent)
                .map(|candidate| candidate.entity_id)
                .expect("validated parent key exists");
            operations.push(SceneOperation::Reparent(ReparentEntity {
                entity_id: entity.entity_id,
                parent_id: Some(parent_id),
            }));
        }
    }
    ScenePatch {
        schema_version: imagination.schema_version,
        transaction_id: imagination.transaction_id,
        idempotency_key: imagination.idempotency_key,
        base_revision: imagination.base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: imagination.delivery.clone(),
        declared_budget: imagination.declared_budget.patch,
        operations,
    }
}

fn default_decision(code: CompilationDecisionCode, key: &SceneText) -> CompilationDecision {
    CompilationDecision {
        code,
        entity_key: key.clone(),
        relation_index: None,
        entity_id: None,
    }
}

fn identity_transform() -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(0.0),
            y: finite(0.0),
            z: finite(0.0),
        },
        rotation: Quaternion {
            x: finite(0.0),
            y: finite(0.0),
            z: finite(0.0),
            w: finite(1.0),
        },
        scale: PositiveVec3 {
            x: positive(1.0),
            y: positive(1.0),
            z: positive(1.0),
        },
    }
}

fn neutral_material() -> MaterialComponent {
    MaterialComponent {
        base_color: ColorRgba {
            r: unit(0.7),
            g: unit(0.7),
            b: unit(0.7),
            a: unit(1.0),
        },
        metallic: unit(0.0),
        roughness: unit(0.8),
    }
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).expect("compiler constant is finite")
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).expect("compiler constant is positive")
}

fn unit(value: f32) -> UnitF32 {
    UnitF32::new(value).expect("compiler constant is in the unit interval")
}

fn unknown_reference(
    relation_index: u32,
    entity: &SceneText,
    related: &SceneText,
) -> UnresolvedConstraint {
    relation_issue(
        UnresolvedConstraintCode::UnknownEntityReference,
        relation_index,
        entity,
        related,
    )
}

fn relation_issue(
    code: UnresolvedConstraintCode,
    relation_index: u32,
    entity: &SceneText,
    related: &SceneText,
) -> UnresolvedConstraint {
    UnresolvedConstraint {
        code,
        relation_index: Some(relation_index),
        constraint_index: None,
        entity_key: Some(entity.clone()),
        related_key: Some(related.clone()),
        entity_id: None,
    }
}

fn decision_order(left: &CompilationDecision, right: &CompilationDecision) -> std::cmp::Ordering {
    left.entity_key
        .cmp(&right.entity_key)
        .then_with(|| left.code.cmp(&right.code))
        .then_with(|| left.relation_index.cmp(&right.relation_index))
}

fn unresolved_order(
    left: &UnresolvedConstraint,
    right: &UnresolvedConstraint,
) -> std::cmp::Ordering {
    left.relation_index
        .cmp(&right.relation_index)
        .then_with(|| left.constraint_index.cmp(&right.constraint_index))
        .then_with(|| left.code.cmp(&right.code))
        .then_with(|| left.entity_key.cmp(&right.entity_key))
        .then_with(|| left.related_key.cmp(&right.related_key))
        .then_with(|| left.entity_id.cmp(&right.entity_id))
}
