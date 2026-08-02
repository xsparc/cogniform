use core::num::NonZeroU32;
use std::{collections::BTreeMap, time::Instant};

use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ApplyTiming, CameraComponent, ComponentKind, ComponentValue,
    ConflictPolicy, FrameId, IdempotencyKey, LightComponent, LocalTransform, MaterialComponent,
    NameComponent, PrimitiveComponent, RuntimeLimits, SceneOperation, ScenePatch, SceneRevision,
    StableEntityId, TransactionId,
};
use hecs::{Entity, EntityBuilder, World};

use crate::{
    EntitySnapshot, WorldApplyError, WorldInvariantError, WorldInvariantErrorKind, WorldSnapshot,
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
}

impl Default for WorldConfig {
    fn default() -> Self {
        Self {
            runtime_limits: RuntimeLimits::default(),
            max_entities: NonZeroU32::new(65_536).expect("constant is non-zero"),
            max_idempotency_records: NonZeroU32::new(4_096).expect("constant is non-zero"),
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
}

/// Sole owner of authoritative ECS state, stable identity, and scene revision.
pub struct AuthoritativeWorld {
    storage: World,
    stable_index: BTreeMap<StableEntityId, Entity>,
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

    /// Reports whether a stable entity is currently live.
    #[must_use]
    pub fn contains(&self, entity_id: StableEntityId) -> bool {
        self.stable_index.contains_key(&entity_id)
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
                EntitySnapshot::new(entity_id, self.snapshot_components(entity))
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

        let commit_started = Instant::now();
        self.commit(&plan);
        let commit_micros = elapsed_micros(commit_started);

        let operation_count = NonZeroU32::new(
            u32::try_from(plan.operations.len()).expect("validated operation count fits u32"),
        )
        .expect("validated patch is non-empty");
        let receipt = ApplyReceipt {
            schema_version: patch.schema_version,
            transaction_id: patch.transaction_id,
            idempotency_key: patch.idempotency_key,
            status: ApplyStatus::Applied,
            previous_revision: plan.previous_revision,
            new_revision: plan.new_revision,
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
        let mut overlay = BTreeMap::<StableEntityId, EntityState>::new();
        let mut live_entities = u32::try_from(self.stable_index.len()).map_err(|_| {
            WorldApplyError::InvariantViolation(WorldInvariantError::new(
                WorldInvariantErrorKind::EntityCountMismatch,
                None,
            ))
        })?;
        let mut operations = Vec::with_capacity(patch.operations.len());

        for (position, operation) in patch.operations.iter().enumerate() {
            let operation_index =
                u32::try_from(position).expect("validated operation count fits u32");
            match operation {
                SceneOperation::Create(create) => {
                    let state = self.overlay_state(&overlay, create.entity_id)?;
                    if state.exists {
                        return Err(WorldApplyError::EntityAlreadyExists {
                            operation_index,
                            entity_id: create.entity_id,
                        });
                    }
                    live_entities = live_entities.checked_add(1).ok_or(
                        WorldApplyError::EntityCapacityExceeded {
                            operation_index,
                            entity_id: create.entity_id,
                            limit: self.config.max_entities.get(),
                        },
                    )?;
                    if live_entities > self.config.max_entities.get() {
                        return Err(WorldApplyError::EntityCapacityExceeded {
                            operation_index,
                            entity_id: create.entity_id,
                            limit: self.config.max_entities.get(),
                        });
                    }
                    overlay.insert(
                        create.entity_id,
                        EntityState {
                            exists: true,
                            components: ComponentMask::from_values(&create.components),
                        },
                    );
                }
                SceneOperation::Delete(delete) => {
                    let state = self.overlay_state(&overlay, delete.entity_id)?;
                    if !state.exists {
                        return Err(WorldApplyError::EntityNotFound {
                            operation_index,
                            entity_id: delete.entity_id,
                        });
                    }
                    live_entities = live_entities
                        .checked_sub(1)
                        .expect("preflight live count follows entity state");
                    overlay.insert(delete.entity_id, EntityState::MISSING);
                }
                SceneOperation::SetComponent(set) => {
                    let mut state = self.overlay_state(&overlay, set.entity_id)?;
                    if !state.exists {
                        return Err(WorldApplyError::EntityNotFound {
                            operation_index,
                            entity_id: set.entity_id,
                        });
                    }
                    state.components.insert(set.component.kind());
                    overlay.insert(set.entity_id, state);
                }
                SceneOperation::RemoveComponent(remove) => {
                    let mut state = self.overlay_state(&overlay, remove.entity_id)?;
                    if !state.exists {
                        return Err(WorldApplyError::EntityNotFound {
                            operation_index,
                            entity_id: remove.entity_id,
                        });
                    }
                    if !state.components.contains(remove.component) {
                        return Err(WorldApplyError::ComponentNotFound {
                            operation_index,
                            entity_id: remove.entity_id,
                            component: remove.component,
                        });
                    }
                    state.components.remove(remove.component);
                    overlay.insert(remove.entity_id, state);
                }
                SceneOperation::Reparent(reparent) => {
                    return Err(WorldApplyError::UnsupportedOperation {
                        operation_index,
                        entity_id: reparent.entity_id,
                    });
                }
            }
            operations.push(operation.clone());
        }

        Ok(CommitPlan {
            previous_revision: self.revision,
            new_revision,
            operations,
        })
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

    fn commit(&mut self, plan: &CommitPlan) {
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
                SceneOperation::Reparent(_) => {
                    unreachable!("preflight rejects hierarchy operations")
                }
            }
        }
        self.revision = plan.new_revision;
        debug_assert!(self.validate_invariants().is_ok());
    }
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
