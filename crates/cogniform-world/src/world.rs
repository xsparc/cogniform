use core::num::NonZeroU32;
use std::{
    borrow::Cow,
    collections::{BTreeMap, BTreeSet},
    time::Instant,
};

use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ApplyTiming, CameraComponent, ComponentKind, ComponentValue,
    ConflictPolicy, CreateEntity, DeleteEntity, FrameId, IdempotencyKey, LightComponent,
    LocalTransform, MaterialComponent, NameComponent, PrimitiveComponent, RemoveComponent,
    ReparentEntity, RuntimeLimits, SceneOperation, ScenePatch, SceneRevision, SetComponent,
    StableEntityId, TransactionId,
};
use hecs::{Entity, EntityBuilder, World};

use crate::{
    EntitySnapshot, LogicalSceneHash, WorldApplyError, WorldInvariantError,
    WorldInvariantErrorKind, WorldSnapshot, WorldTransform,
};

/// Explicit memory and admission bounds for one authoritative world.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldConfig {
    /// Protocol message limits enforced before world preflight.
    pub runtime_limits: RuntimeLimits,
    /// Maximum simultaneously live entities.
    pub max_entities: NonZeroU32,
    /// Maximum retained accepted idempotency records.
    pub max_idempotency_records: NonZeroU32,
    /// Maximum number of parent edges from a root to any descendant.
    pub max_hierarchy_depth: NonZeroU32,
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            runtime_limits: RuntimeLimits::default(),
            max_entities: NonZeroU32::new(65_536).expect("constant is non-zero"),
            max_idempotency_records: NonZeroU32::new(4_096).expect("constant is non-zero"),
            max_hierarchy_depth: NonZeroU32::new(256).expect("constant is non-zero"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct StoredStableId(StableEntityId);

#[derive(Debug, Clone)]
struct RecordedApply {
    transaction_id: TransactionId,
    receipt: ApplyReceipt,
}

#[derive(Debug, Clone, Copy)]
struct EntityState {
    exists: bool,
    components: ComponentMask,
}

impl EntityState {
    const MISSING: Self = Self {
        exists: false,
        components: ComponentMask::EMPTY,
    };
}

#[derive(Debug, Clone, Copy)]
struct ComponentMask(u8);

impl ComponentMask {
    const EMPTY: Self = Self(0);

    fn from_values(values: &[ComponentValue]) -> Self {
        values.iter().fold(Self::EMPTY, |mut mask, value| {
            mask.insert(value.kind());
            mask
        })
    }

    const fn bit(kind: ComponentKind) -> u8 {
        match kind {
            ComponentKind::Name => 1 << 0,
            ComponentKind::LocalTransform => 1 << 1,
            ComponentKind::Primitive => 1 << 2,
            ComponentKind::Material => 1 << 3,
            ComponentKind::Camera => 1 << 4,
            ComponentKind::Light => 1 << 5,
        }
    }

    const fn contains(self, kind: ComponentKind) -> bool {
        self.0 & Self::bit(kind) != 0
    }

    fn insert(&mut self, kind: ComponentKind) {
        self.0 |= Self::bit(kind);
    }

    fn remove(&mut self, kind: ComponentKind) {
        self.0 &= !Self::bit(kind);
    }
}

struct CommitPlan {
    previous_revision: SceneRevision,
    new_revision: SceneRevision,
    operations: Vec<SceneOperation>,
    parents: Option<BTreeMap<StableEntityId, StableEntityId>>,
    children: Option<BTreeMap<StableEntityId, BTreeSet<StableEntityId>>>,
    transform_updates: BTreeMap<StableEntityId, WorldTransform>,
    transform_generation: u64,
}

struct PreflightState<'a> {
    overlay: BTreeMap<StableEntityId, EntityState>,
    parents: Cow<'a, BTreeMap<StableEntityId, StableEntityId>>,
    local_overrides: BTreeMap<StableEntityId, Option<LocalTransform>>,
    dirty_roots: BTreeSet<StableEntityId>,
    hierarchy_changed: bool,
    live_entities: u32,
    operations: Vec<SceneOperation>,
}

/// Sole owner of authoritative ECS state, stable identity, and scene revision.
pub struct AuthoritativeWorld {
    storage: World,
    stable_index: BTreeMap<StableEntityId, Entity>,
    parents: BTreeMap<StableEntityId, StableEntityId>,
    children: BTreeMap<StableEntityId, BTreeSet<StableEntityId>>,
    world_transforms: BTreeMap<StableEntityId, WorldTransform>,
    transform_generation: u64,
    revision: SceneRevision,
    idempotency_records: BTreeMap<IdempotencyKey, RecordedApply>,
    config: WorldConfig,
}

impl AuthoritativeWorld {
    /// Creates an empty world at revision zero with explicit bounds.
    #[must_use]
    pub fn new(config: WorldConfig) -> Self {
        Self {
            storage: World::new(),
            stable_index: BTreeMap::new(),
            parents: BTreeMap::new(),
            children: BTreeMap::new(),
            world_transforms: BTreeMap::new(),
            transform_generation: 0,
            revision: SceneRevision::INITIAL,
            idempotency_records: BTreeMap::new(),
            config,
        }
    }

    /// Returns the current authoritative revision.
    #[must_use]
    pub const fn revision(&self) -> SceneRevision {
        self.revision
    }

    /// Returns the number of live stable entities.
    #[must_use]
    pub fn entity_count(&self) -> usize {
        self.stable_index.len()
    }

    /// Returns the number of retained accepted idempotency records.
    #[must_use]
    pub fn idempotency_record_count(&self) -> usize {
        self.idempotency_records.len()
    }

    /// Returns the transaction recorded for an accepted idempotency key.
    #[must_use]
    pub fn recorded_transaction(&self, idempotency_key: IdempotencyKey) -> Option<TransactionId> {
        self.idempotency_records
            .get(&idempotency_key)
            .map(|record| record.transaction_id)
    }

    /// Reports whether a stable entity is currently live.
    #[must_use]
    pub fn contains(&self, entity_id: StableEntityId) -> bool {
        self.stable_index.contains_key(&entity_id)
    }

    /// Returns the stable parent for a live entity.
    ///
    /// The outer option is `None` when the entity is not live. The inner option
    /// is `None` for a hierarchy root.
    #[must_use]
    pub fn parent_id(&self, entity_id: StableEntityId) -> Option<Option<StableEntityId>> {
        self.contains(entity_id)
            .then(|| self.parents.get(&entity_id).copied())
    }

    /// Iterates a live entity's children in stable-ID order.
    #[must_use]
    pub fn children(
        &self,
        entity_id: StableEntityId,
    ) -> Option<impl Iterator<Item = StableEntityId> + '_> {
        self.contains(entity_id).then(|| {
            self.children
                .get(&entity_id)
                .into_iter()
                .flat_map(BTreeSet::iter)
                .copied()
        })
    }

    /// Returns a live entity's cached derived world transform.
    #[must_use]
    pub fn world_transform(&self, entity_id: StableEntityId) -> Option<WorldTransform> {
        self.world_transforms.get(&entity_id).copied()
    }

    /// Returns the canonical hash of current logical state.
    pub fn logical_hash(&self) -> Result<LogicalSceneHash, WorldInvariantError> {
        self.snapshot().map(|snapshot| snapshot.logical_hash())
    }

    /// Verifies that private ECS handles and stable-ID markers agree with the index.
    pub fn validate_invariants(&self) -> Result<(), WorldInvariantError> {
        if usize::try_from(self.storage.len()).ok() != Some(self.stable_index.len()) {
            return Err(WorldInvariantError::new(
                WorldInvariantErrorKind::EntityCountMismatch,
                None,
            ));
        }

        for (&stable_id, &entity) in &self.stable_index {
            if !self.storage.contains(entity) {
                return Err(WorldInvariantError::new(
                    WorldInvariantErrorKind::MissingStorageEntity,
                    Some(stable_id),
                ));
            }
            match self.storage.get::<&StoredStableId>(entity) {
                Ok(stored) if stored.0 == stable_id => {}
                Ok(_) | Err(_) => {
                    return Err(WorldInvariantError::new(
                        WorldInvariantErrorKind::StableIdMismatch,
                        Some(stable_id),
                    ));
                }
            }
            if !self.world_transforms.contains_key(&stable_id) {
                return Err(WorldInvariantError::new(
                    WorldInvariantErrorKind::WorldTransformMissing,
                    Some(stable_id),
                ));
            }
        }
        for &entity_id in self.world_transforms.keys() {
            if !self.stable_index.contains_key(&entity_id) {
                return Err(WorldInvariantError::new(
                    WorldInvariantErrorKind::WorldTransformOrphan,
                    Some(entity_id),
                ));
            }
        }
        for (&child_id, &parent_id) in &self.parents {
            if !self.stable_index.contains_key(&child_id)
                || !self.stable_index.contains_key(&parent_id)
            {
                return Err(WorldInvariantError::new(
                    WorldInvariantErrorKind::HierarchyEntityMissing,
                    Some(child_id),
                ));
            }
            if !self
                .children
                .get(&parent_id)
                .is_some_and(|children| children.contains(&child_id))
            {
                return Err(WorldInvariantError::new(
                    WorldInvariantErrorKind::HierarchyIndexMismatch,
                    Some(child_id),
                ));
            }
        }
        let indexed_children = self.children.values().map(BTreeSet::len).sum::<usize>();
        if indexed_children != self.parents.len() {
            return Err(WorldInvariantError::new(
                WorldInvariantErrorKind::HierarchyIndexMismatch,
                None,
            ));
        }
        let live_ids = self.stable_index.keys().copied().collect::<BTreeSet<_>>();
        if hierarchy_depths(
            &live_ids,
            &self.parents,
            self.config.max_hierarchy_depth.get(),
        )
        .is_err()
        {
            return Err(WorldInvariantError::new(
                WorldInvariantErrorKind::HierarchyTopologyInvalid,
                None,
            ));
        }
        Ok(())
    }

    /// Builds a sorted backend-neutral view of the current logical state.
    pub fn snapshot(&self) -> Result<WorldSnapshot, WorldInvariantError> {
        self.validate_invariants()?;
        let entities = self
            .stable_index
            .iter()
            .map(|(&entity_id, &entity)| {
                EntitySnapshot::new(
                    entity_id,
                    self.parents.get(&entity_id).copied(),
                    self.snapshot_components(entity),
                )
            })
            .collect();
        Ok(WorldSnapshot::new(self.revision, entities))
    }

    /// Applies one complete patch or returns a rejection without mutation.
    ///
    /// `estimated_visible_frame` is supplied by the orchestration boundary
    /// because the world does not own render scheduling. Decode time is zero for
    /// this already-decoded in-process request.
    pub fn apply_patch(
        &mut self,
        patch: &ScenePatch,
        estimated_visible_frame: FrameId,
    ) -> Result<ApplyReceipt, WorldApplyError> {
        let validate_started = Instant::now();
        patch
            .validate_with_limits(&self.config.runtime_limits)
            .map_err(WorldApplyError::InvalidPatch)?;

        if let Some(recorded) = self.idempotency_records.get(&patch.idempotency_key) {
            if recorded.transaction_id != patch.transaction_id {
                return Err(WorldApplyError::IdempotencyKeyConflict {
                    idempotency_key: patch.idempotency_key,
                    recorded_transaction: recorded.transaction_id,
                    supplied_transaction: patch.transaction_id,
                });
            }
            let mut receipt = recorded.receipt.clone();
            receipt.status = ApplyStatus::IdempotentReplay;
            return Ok(receipt);
        }

        if self.idempotency_records.len()
            >= usize::try_from(self.config.max_idempotency_records.get())
                .expect("u32 idempotency capacity fits usize")
        {
            return Err(WorldApplyError::IdempotencyCapacityExceeded {
                limit: self.config.max_idempotency_records.get(),
            });
        }
        if patch.conflict_policy == ConflictPolicy::RequireExactBase
            && patch.base_revision != self.revision
        {
            return Err(WorldApplyError::BaseRevisionMismatch {
                current: self.revision,
                supplied: patch.base_revision,
            });
        }

        let new_revision = self
            .revision
            .checked_next()
            .map_err(|_| WorldApplyError::RevisionOverflow)?;
        let plan = self.preflight(patch, new_revision)?;
        let validate_micros = elapsed_micros(validate_started);
        let previous_revision = plan.previous_revision;
        let operation_count = NonZeroU32::new(
            u32::try_from(plan.operations.len()).expect("validated operation count fits u32"),
        )
        .expect("validated patch is non-empty");

        let commit_started = Instant::now();
        self.commit(plan);
        let commit_micros = elapsed_micros(commit_started);

        let receipt = ApplyReceipt {
            schema_version: patch.schema_version,
            transaction_id: patch.transaction_id,
            idempotency_key: patch.idempotency_key,
            status: ApplyStatus::Applied,
            previous_revision,
            new_revision,
            operation_count,
            diagnostics: Vec::new(),
            timing: ApplyTiming {
                decode_micros: 0,
                validate_micros,
                commit_micros,
            },
            estimated_visible_frame,
        };
        debug_assert!(
            receipt
                .validate_with_limits(&self.config.runtime_limits)
                .is_ok()
        );
        self.idempotency_records.insert(
            patch.idempotency_key,
            RecordedApply {
                transaction_id: patch.transaction_id,
                receipt: receipt.clone(),
            },
        );
        Ok(receipt)
    }

    fn preflight(
        &self,
        patch: &ScenePatch,
        new_revision: SceneRevision,
    ) -> Result<CommitPlan, WorldApplyError> {
        let mut state = PreflightState {
            overlay: BTreeMap::new(),
            parents: Cow::Borrowed(&self.parents),
            local_overrides: BTreeMap::new(),
            dirty_roots: BTreeSet::new(),
            hierarchy_changed: false,
            live_entities: u32::try_from(self.stable_index.len()).map_err(|_| {
                WorldApplyError::InvariantViolation(WorldInvariantError::new(
                    WorldInvariantErrorKind::EntityCountMismatch,
                    None,
                ))
            })?,
            operations: Vec::with_capacity(patch.operations.len()),
        };

        for (position, operation) in patch.operations.iter().enumerate() {
            let operation_index =
                u32::try_from(position).expect("validated operation count fits u32");
            match operation {
                SceneOperation::Create(create) => {
                    self.preflight_create(&mut state, create, operation_index)?;
                }
                SceneOperation::Delete(delete) => {
                    self.preflight_delete(&mut state, delete, operation_index)?;
                }
                SceneOperation::SetComponent(set) => {
                    self.preflight_set(&mut state, set, operation_index)?;
                }
                SceneOperation::RemoveComponent(remove) => {
                    self.preflight_remove(&mut state, remove, operation_index)?;
                }
                SceneOperation::Reparent(reparent) => {
                    self.preflight_reparent(&mut state, reparent, operation_index)?;
                }
            }
            state.operations.push(operation.clone());
        }
        self.complete_preflight(state, new_revision)
    }

    fn complete_preflight(
        &self,
        state: PreflightState<'_>,
        new_revision: SceneRevision,
    ) -> Result<CommitPlan, WorldApplyError> {
        let rebuilt_children = if state.hierarchy_changed {
            let mut live_ids = self.stable_index.keys().copied().collect::<BTreeSet<_>>();
            for (&entity_id, entity_state) in &state.overlay {
                if entity_state.exists {
                    live_ids.insert(entity_id);
                } else {
                    live_ids.remove(&entity_id);
                }
            }
            hierarchy_depths(
                &live_ids,
                &state.parents,
                self.config.max_hierarchy_depth.get(),
            )?;
            let mut children = BTreeMap::<StableEntityId, BTreeSet<StableEntityId>>::new();
            for (&child_id, &parent_id) in state.parents.as_ref() {
                children.entry(parent_id).or_default().insert(child_id);
            }
            Some(children)
        } else {
            None
        };
        let children = rebuilt_children.as_ref().unwrap_or(&self.children);
        let (transform_updates, transform_generation) = self.plan_transform_updates(
            &state.parents,
            children,
            &state.local_overrides,
            &state.dirty_roots,
        )?;
        let parents = match state.parents {
            Cow::Borrowed(_) => None,
            Cow::Owned(parents) => Some(parents),
        };

        Ok(CommitPlan {
            previous_revision: self.revision,
            new_revision,
            operations: state.operations,
            parents,
            children: rebuilt_children,
            transform_updates,
            transform_generation,
        })
    }

    fn preflight_create(
        &self,
        state: &mut PreflightState<'_>,
        create: &CreateEntity,
        operation_index: u32,
    ) -> Result<(), WorldApplyError> {
        if self.overlay_state(&state.overlay, create.entity_id)?.exists {
            return Err(WorldApplyError::EntityAlreadyExists {
                operation_index,
                entity_id: create.entity_id,
            });
        }
        state.live_entities =
            state
                .live_entities
                .checked_add(1)
                .ok_or(WorldApplyError::EntityCapacityExceeded {
                    operation_index,
                    entity_id: create.entity_id,
                    limit: self.config.max_entities.get(),
                })?;
        if state.live_entities > self.config.max_entities.get() {
            return Err(WorldApplyError::EntityCapacityExceeded {
                operation_index,
                entity_id: create.entity_id,
                limit: self.config.max_entities.get(),
            });
        }
        state.overlay.insert(
            create.entity_id,
            EntityState {
                exists: true,
                components: ComponentMask::from_values(&create.components),
            },
        );
        state.hierarchy_changed = true;
        let local_transform = create.components.iter().find_map(|component| {
            if let ComponentValue::LocalTransform(value) = component {
                Some(*value)
            } else {
                None
            }
        });
        state
            .local_overrides
            .insert(create.entity_id, local_transform);
        state.dirty_roots.insert(create.entity_id);
        Ok(())
    }

    fn preflight_delete(
        &self,
        state: &mut PreflightState<'_>,
        delete: &DeleteEntity,
        operation_index: u32,
    ) -> Result<(), WorldApplyError> {
        if !self.overlay_state(&state.overlay, delete.entity_id)?.exists {
            return Err(WorldApplyError::EntityNotFound {
                operation_index,
                entity_id: delete.entity_id,
            });
        }
        state.live_entities = state
            .live_entities
            .checked_sub(1)
            .expect("preflight live count follows entity state");
        state.overlay.insert(delete.entity_id, EntityState::MISSING);
        if state.parents.contains_key(&delete.entity_id) {
            state.parents.to_mut().remove(&delete.entity_id);
        }
        state.hierarchy_changed = true;
        state.local_overrides.remove(&delete.entity_id);
        state.dirty_roots.remove(&delete.entity_id);
        Ok(())
    }

    fn preflight_set(
        &self,
        state: &mut PreflightState<'_>,
        set: &SetComponent,
        operation_index: u32,
    ) -> Result<(), WorldApplyError> {
        let mut entity_state = self.overlay_state(&state.overlay, set.entity_id)?;
        if !entity_state.exists {
            return Err(WorldApplyError::EntityNotFound {
                operation_index,
                entity_id: set.entity_id,
            });
        }
        if let ComponentValue::LocalTransform(value) = &set.component {
            if self.effective_local_transform(&state.local_overrides, set.entity_id) != Some(*value)
            {
                state.dirty_roots.insert(set.entity_id);
            }
            state.local_overrides.insert(set.entity_id, Some(*value));
        }
        entity_state.components.insert(set.component.kind());
        state.overlay.insert(set.entity_id, entity_state);
        Ok(())
    }

    fn preflight_remove(
        &self,
        state: &mut PreflightState<'_>,
        remove: &RemoveComponent,
        operation_index: u32,
    ) -> Result<(), WorldApplyError> {
        let mut entity_state = self.overlay_state(&state.overlay, remove.entity_id)?;
        if !entity_state.exists {
            return Err(WorldApplyError::EntityNotFound {
                operation_index,
                entity_id: remove.entity_id,
            });
        }
        if !entity_state.components.contains(remove.component) {
            return Err(WorldApplyError::ComponentNotFound {
                operation_index,
                entity_id: remove.entity_id,
                component: remove.component,
            });
        }
        if remove.component == ComponentKind::LocalTransform {
            state.local_overrides.insert(remove.entity_id, None);
            state.dirty_roots.insert(remove.entity_id);
        }
        entity_state.components.remove(remove.component);
        state.overlay.insert(remove.entity_id, entity_state);
        Ok(())
    }

    fn preflight_reparent(
        &self,
        state: &mut PreflightState<'_>,
        reparent: &ReparentEntity,
        operation_index: u32,
    ) -> Result<(), WorldApplyError> {
        if !self
            .overlay_state(&state.overlay, reparent.entity_id)?
            .exists
        {
            return Err(WorldApplyError::EntityNotFound {
                operation_index,
                entity_id: reparent.entity_id,
            });
        }
        if let Some(parent_id) = reparent.parent_id
            && !self.overlay_state(&state.overlay, parent_id)?.exists
        {
            return Err(WorldApplyError::EntityNotFound {
                operation_index,
                entity_id: parent_id,
            });
        }
        let previous = state.parents.get(&reparent.entity_id).copied();
        if previous != reparent.parent_id {
            match reparent.parent_id {
                Some(parent_id) => {
                    state.parents.to_mut().insert(reparent.entity_id, parent_id);
                }
                None => {
                    state.parents.to_mut().remove(&reparent.entity_id);
                }
            }
            state.dirty_roots.insert(reparent.entity_id);
            state.hierarchy_changed = true;
        }
        Ok(())
    }

    fn plan_transform_updates(
        &self,
        parents: &BTreeMap<StableEntityId, StableEntityId>,
        children: &BTreeMap<StableEntityId, BTreeSet<StableEntityId>>,
        local_overrides: &BTreeMap<StableEntityId, Option<LocalTransform>>,
        dirty_roots: &BTreeSet<StableEntityId>,
    ) -> Result<(BTreeMap<StableEntityId, WorldTransform>, u64), WorldApplyError> {
        let ordered = affected_entities(parents, children, dirty_roots);
        let generation = if ordered.is_empty() {
            self.transform_generation
        } else {
            self.transform_generation
                .checked_add(1)
                .ok_or(WorldApplyError::TransformGenerationOverflow)?
        };
        let mut updates = BTreeMap::<StableEntityId, WorldTransform>::new();
        for entity_id in ordered {
            let local = WorldTransform::from_local(
                self.effective_local_transform(local_overrides, entity_id),
                generation,
            )
            .ok_or(WorldApplyError::TransformOverflow { entity_id })?;
            let world =
                self.compose_world_transform(entity_id, local, generation, parents, &updates)?;
            updates.insert(entity_id, world);
        }
        Ok((updates, generation))
    }

    fn compose_world_transform(
        &self,
        entity_id: StableEntityId,
        local: WorldTransform,
        generation: u64,
        parents: &BTreeMap<StableEntityId, StableEntityId>,
        updates: &BTreeMap<StableEntityId, WorldTransform>,
    ) -> Result<WorldTransform, WorldApplyError> {
        let Some(parent_id) = parents.get(&entity_id).copied() else {
            return Ok(local);
        };
        let parent = updates
            .get(&parent_id)
            .or_else(|| self.world_transforms.get(&parent_id))
            .copied()
            .ok_or_else(|| {
                WorldApplyError::InvariantViolation(WorldInvariantError::new(
                    WorldInvariantErrorKind::WorldTransformMissing,
                    Some(parent_id),
                ))
            })?;
        parent
            .compose(local, generation)
            .ok_or(WorldApplyError::TransformOverflow { entity_id })
    }

    fn overlay_state(
        &self,
        overlay: &BTreeMap<StableEntityId, EntityState>,
        entity_id: StableEntityId,
    ) -> Result<EntityState, WorldApplyError> {
        overlay
            .get(&entity_id)
            .copied()
            .map_or_else(|| self.stored_entity_state(entity_id), Ok)
    }

    fn effective_local_transform(
        &self,
        overrides: &BTreeMap<StableEntityId, Option<LocalTransform>>,
        entity_id: StableEntityId,
    ) -> Option<LocalTransform> {
        if let Some(value) = overrides.get(&entity_id) {
            return *value;
        }
        self.stable_index.get(&entity_id).and_then(|&entity| {
            self.storage
                .get::<&LocalTransform>(entity)
                .ok()
                .map(|value| *value)
        })
    }

    fn stored_entity_state(
        &self,
        entity_id: StableEntityId,
    ) -> Result<EntityState, WorldApplyError> {
        let Some(&entity) = self.stable_index.get(&entity_id) else {
            return Ok(EntityState::MISSING);
        };
        if !self.storage.contains(entity) {
            return Err(WorldApplyError::InvariantViolation(
                WorldInvariantError::new(
                    WorldInvariantErrorKind::MissingStorageEntity,
                    Some(entity_id),
                ),
            ));
        }
        let marker_matches = self
            .storage
            .get::<&StoredStableId>(entity)
            .is_ok_and(|stored| stored.0 == entity_id);
        if !marker_matches {
            return Err(WorldApplyError::InvariantViolation(
                WorldInvariantError::new(
                    WorldInvariantErrorKind::StableIdMismatch,
                    Some(entity_id),
                ),
            ));
        }
        Ok(EntityState {
            exists: true,
            components: self.component_mask(entity),
        })
    }

    fn component_mask(&self, entity: Entity) -> ComponentMask {
        let mut mask = ComponentMask::EMPTY;
        if self.storage.get::<&NameComponent>(entity).is_ok() {
            mask.insert(ComponentKind::Name);
        }
        if self.storage.get::<&LocalTransform>(entity).is_ok() {
            mask.insert(ComponentKind::LocalTransform);
        }
        if self.storage.get::<&PrimitiveComponent>(entity).is_ok() {
            mask.insert(ComponentKind::Primitive);
        }
        if self.storage.get::<&MaterialComponent>(entity).is_ok() {
            mask.insert(ComponentKind::Material);
        }
        if self.storage.get::<&CameraComponent>(entity).is_ok() {
            mask.insert(ComponentKind::Camera);
        }
        if self.storage.get::<&LightComponent>(entity).is_ok() {
            mask.insert(ComponentKind::Light);
        }
        mask
    }

    fn snapshot_components(&self, entity: Entity) -> Vec<ComponentValue> {
        let mut components = Vec::with_capacity(6);
        if let Ok(value) = self.storage.get::<&NameComponent>(entity) {
            components.push(ComponentValue::Name((*value).clone()));
        }
        if let Ok(value) = self.storage.get::<&LocalTransform>(entity) {
            components.push(ComponentValue::LocalTransform(*value));
        }
        if let Ok(value) = self.storage.get::<&PrimitiveComponent>(entity) {
            components.push(ComponentValue::Primitive(*value));
        }
        if let Ok(value) = self.storage.get::<&MaterialComponent>(entity) {
            components.push(ComponentValue::Material(*value));
        }
        if let Ok(value) = self.storage.get::<&CameraComponent>(entity) {
            components.push(ComponentValue::Camera(*value));
        }
        if let Ok(value) = self.storage.get::<&LightComponent>(entity) {
            components.push(ComponentValue::Light(*value));
        }
        components
    }

    fn commit(&mut self, plan: CommitPlan) {
        for operation in &plan.operations {
            match operation {
                SceneOperation::Create(create) => {
                    let mut builder = EntityBuilder::new();
                    builder.add(StoredStableId(create.entity_id));
                    for component in &create.components {
                        add_component_to_builder(&mut builder, component);
                    }
                    let entity = self.storage.spawn(builder.build());
                    let previous = self.stable_index.insert(create.entity_id, entity);
                    assert!(previous.is_none(), "preflight rejected duplicate stable ID");
                }
                SceneOperation::Delete(delete) => {
                    let entity = self
                        .stable_index
                        .remove(&delete.entity_id)
                        .expect("preflight resolved deleted entity");
                    self.storage
                        .despawn(entity)
                        .expect("preflight resolved live storage entity");
                }
                SceneOperation::SetComponent(set) => {
                    let entity = self.stable_index[&set.entity_id];
                    set_component(&mut self.storage, entity, &set.component);
                }
                SceneOperation::RemoveComponent(remove) => {
                    let entity = self.stable_index[&remove.entity_id];
                    remove_component(&mut self.storage, entity, remove.component);
                }
                SceneOperation::Reparent(_) => {}
            }
        }
        self.world_transforms
            .retain(|entity_id, _| self.stable_index.contains_key(entity_id));
        self.world_transforms.extend(plan.transform_updates);
        if let Some(parents) = plan.parents {
            self.parents = parents;
        }
        if let Some(children) = plan.children {
            self.children = children;
        }
        self.transform_generation = plan.transform_generation;
        self.revision = plan.new_revision;
        debug_assert!(self.validate_invariants().is_ok());
    }
}

fn affected_entities(
    parents: &BTreeMap<StableEntityId, StableEntityId>,
    children: &BTreeMap<StableEntityId, BTreeSet<StableEntityId>>,
    dirty_roots: &BTreeSet<StableEntityId>,
) -> Vec<StableEntityId> {
    let roots = dirty_roots
        .iter()
        .copied()
        .filter(|&entity_id| {
            let mut current = entity_id;
            while let Some(parent_id) = parents.get(&current).copied() {
                if dirty_roots.contains(&parent_id) {
                    return false;
                }
                current = parent_id;
            }
            true
        })
        .collect::<BTreeSet<_>>();
    let mut affected = Vec::new();
    let mut seen = BTreeSet::new();
    let mut pending = roots.iter().rev().copied().collect::<Vec<_>>();
    while let Some(entity_id) = pending.pop() {
        if !seen.insert(entity_id) {
            continue;
        }
        affected.push(entity_id);
        if let Some(descendants) = children.get(&entity_id) {
            pending.extend(descendants.iter().rev().copied());
        }
    }
    affected
}

fn hierarchy_depths(
    live_ids: &BTreeSet<StableEntityId>,
    parents: &BTreeMap<StableEntityId, StableEntityId>,
    max_depth: u32,
) -> Result<BTreeMap<StableEntityId, u32>, WorldApplyError> {
    for (&child_id, &parent_id) in parents {
        if !live_ids.contains(&child_id) || !live_ids.contains(&parent_id) {
            return Err(WorldApplyError::HierarchyParentNotFound {
                entity_id: child_id,
                parent_id,
            });
        }
    }

    let mut depths = BTreeMap::<StableEntityId, u32>::new();
    for &start in live_ids {
        if depths.contains_key(&start) {
            continue;
        }
        let mut path = Vec::<StableEntityId>::new();
        let mut visiting = BTreeSet::<StableEntityId>::new();
        let mut current = start;
        loop {
            if let Some(&known_depth) = depths.get(&current) {
                assign_path_depths(&mut depths, &path, known_depth, max_depth)?;
                break;
            }
            if !visiting.insert(current) {
                return Err(WorldApplyError::HierarchyCycle { entity_id: current });
            }
            path.push(current);
            if let Some(&parent_id) = parents.get(&current) {
                current = parent_id;
            } else {
                let root = path.pop().expect("hierarchy path starts non-empty");
                depths.insert(root, 0);
                assign_path_depths(&mut depths, &path, 0, max_depth)?;
                break;
            }
        }
    }
    Ok(depths)
}

fn assign_path_depths(
    depths: &mut BTreeMap<StableEntityId, u32>,
    path: &[StableEntityId],
    base_depth: u32,
    max_depth: u32,
) -> Result<(), WorldApplyError> {
    let mut depth = base_depth;
    for &entity_id in path.iter().rev() {
        depth = depth
            .checked_add(1)
            .ok_or(WorldApplyError::HierarchyDepthExceeded {
                entity_id,
                depth: u32::MAX,
                limit: max_depth,
            })?;
        if depth > max_depth {
            return Err(WorldApplyError::HierarchyDepthExceeded {
                entity_id,
                depth,
                limit: max_depth,
            });
        }
        depths.insert(entity_id, depth);
    }
    Ok(())
}

impl Default for AuthoritativeWorld {
    fn default() -> Self {
        Self::new(WorldConfig::default())
    }
}

fn add_component_to_builder(builder: &mut EntityBuilder, component: &ComponentValue) {
    match component {
        ComponentValue::Name(value) => {
            builder.add(value.clone());
        }
        ComponentValue::LocalTransform(value) => {
            builder.add(*value);
        }
        ComponentValue::Primitive(value) => {
            builder.add(*value);
        }
        ComponentValue::Material(value) => {
            builder.add(*value);
        }
        ComponentValue::Camera(value) => {
            builder.add(*value);
        }
        ComponentValue::Light(value) => {
            builder.add(*value);
        }
    }
}

fn set_component(storage: &mut World, entity: Entity, component: &ComponentValue) {
    let result = match component {
        ComponentValue::Name(value) => storage.insert_one(entity, value.clone()),
        ComponentValue::LocalTransform(value) => storage.insert_one(entity, *value),
        ComponentValue::Primitive(value) => storage.insert_one(entity, *value),
        ComponentValue::Material(value) => storage.insert_one(entity, *value),
        ComponentValue::Camera(value) => storage.insert_one(entity, *value),
        ComponentValue::Light(value) => storage.insert_one(entity, *value),
    };
    result.expect("preflight resolved live storage entity");
}

fn remove_component(storage: &mut World, entity: Entity, component: ComponentKind) {
    match component {
        ComponentKind::Name => drop(
            storage
                .remove_one::<NameComponent>(entity)
                .expect("preflight resolved present component"),
        ),
        ComponentKind::LocalTransform => drop(
            storage
                .remove_one::<LocalTransform>(entity)
                .expect("preflight resolved present component"),
        ),
        ComponentKind::Primitive => drop(
            storage
                .remove_one::<PrimitiveComponent>(entity)
                .expect("preflight resolved present component"),
        ),
        ComponentKind::Material => drop(
            storage
                .remove_one::<MaterialComponent>(entity)
                .expect("preflight resolved present component"),
        ),
        ComponentKind::Camera => drop(
            storage
                .remove_one::<CameraComponent>(entity)
                .expect("preflight resolved present component"),
        ),
        ComponentKind::Light => drop(
            storage
                .remove_one::<LightComponent>(entity)
                .expect("preflight resolved present component"),
        ),
    }
}

fn elapsed_micros(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use cogniform_protocol::{
        ConflictPolicy, CreateEntity, DeleteEntity, DeliverySemantic, FrameId, IdempotencyKey,
        PatchBudget, SceneOperation, ScenePatch, SchemaVersion, TransactionId,
    };

    use super::*;

    fn patch(revision: SceneRevision, nonce: u128, operation: SceneOperation) -> ScenePatch {
        ScenePatch {
            schema_version: SchemaVersion::V1,
            transaction_id: TransactionId::new(nonce * 2).unwrap(),
            idempotency_key: IdempotencyKey::new((nonce * 2) + 1).unwrap(),
            base_revision: revision,
            conflict_policy: ConflictPolicy::RequireExactBase,
            delivery: DeliverySemantic::MustApply,
            declared_budget: PatchBudget::default(),
            operations: vec![operation],
        }
    }

    #[test]
    fn recycled_ecs_slot_cannot_reassign_a_stable_id() {
        let first_id = StableEntityId::new(1).unwrap();
        let second_id = StableEntityId::new(2).unwrap();
        let mut world = AuthoritativeWorld::default();

        world
            .apply_patch(
                &patch(
                    world.revision(),
                    1,
                    SceneOperation::Create(CreateEntity {
                        entity_id: first_id,
                        components: Vec::new(),
                    }),
                ),
                FrameId::new(1).unwrap(),
            )
            .unwrap();
        let first_handle = world.stable_index[&first_id];

        world
            .apply_patch(
                &patch(
                    world.revision(),
                    2,
                    SceneOperation::Delete(DeleteEntity {
                        entity_id: first_id,
                    }),
                ),
                FrameId::new(2).unwrap(),
            )
            .unwrap();
        world
            .apply_patch(
                &patch(
                    world.revision(),
                    3,
                    SceneOperation::Create(CreateEntity {
                        entity_id: second_id,
                        components: Vec::new(),
                    }),
                ),
                FrameId::new(3).unwrap(),
            )
            .unwrap();
        let second_handle = world.stable_index[&second_id];

        assert_eq!(first_handle.id(), second_handle.id());
        assert_ne!(first_handle.to_bits(), second_handle.to_bits());
        assert!(!world.contains(first_id));
        assert!(world.contains(second_id));
        world.validate_invariants().unwrap();
    }

    #[test]
    fn revision_overflow_rejects_before_mutation() {
        let entity_id = StableEntityId::new(1).unwrap();
        let mut world = AuthoritativeWorld {
            revision: SceneRevision::new(u64::MAX),
            ..AuthoritativeWorld::default()
        };
        let rejected = patch(
            world.revision(),
            1,
            SceneOperation::Create(CreateEntity {
                entity_id,
                components: Vec::new(),
            }),
        );

        assert_eq!(
            world.apply_patch(&rejected, FrameId::new(1).unwrap()),
            Err(WorldApplyError::RevisionOverflow)
        );
        assert_eq!(world.revision(), SceneRevision::new(u64::MAX));
        assert_eq!(world.entity_count(), 0);
        assert_eq!(world.idempotency_record_count(), 0);
    }
}
