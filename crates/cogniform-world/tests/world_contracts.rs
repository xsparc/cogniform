//! Authoritative-world behavioral and deterministic invariant contracts.

use std::{collections::BTreeMap, num::NonZeroU32};

use cogniform_protocol::{
    ApplyStatus, CameraComponent, ColorRgb, ColorRgba, ComponentKind, ComponentValue,
    ConflictPolicy, CreateEntity, DeleteEntity, DeliverySemantic, DiagnosticCode, FiniteF32,
    FrameId, IdempotencyKey, LightComponent, LightKind, LocalTransform, MaterialComponent,
    NameComponent, NonNegativeF32, PatchBudget, PositiveF32, PositiveVec3, PrimitiveComponent,
    PrimitiveShape, Quaternion, RemoveComponent, ReparentEntity, SceneOperation, ScenePatch,
    SceneRevision, SceneText, SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32,
    Vec3,
};
use cogniform_world::{AuthoritativeWorld, WorldApplyError, WorldConfig};

fn stable_id(value: u128) -> StableEntityId {
    StableEntityId::new(value).expect("fixture ID is non-zero")
}

fn frame(value: u64) -> FrameId {
    FrameId::new(value).expect("fixture frame is non-zero")
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).expect("fixture number is finite")
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).expect("fixture number is positive")
}

fn unit(value: f32) -> UnitF32 {
    UnitF32::new(value).expect("fixture number is in the unit interval")
}

fn name(value: &str) -> ComponentValue {
    ComponentValue::Name(NameComponent {
        value: SceneText::new(value).expect("fixture name is valid"),
    })
}

fn patch(base_revision: SceneRevision, nonce: u128, operations: Vec<SceneOperation>) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(nonce * 2).expect("fixture transaction is non-zero"),
        idempotency_key: IdempotencyKey::new((nonce * 2) + 1)
            .expect("fixture idempotency key is non-zero"),
        base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations,
    }
}

fn create(entity_id: StableEntityId, components: Vec<ComponentValue>) -> SceneOperation {
    SceneOperation::Create(CreateEntity {
        entity_id,
        components,
    })
}

#[test]
fn ordered_patch_commits_all_component_kinds_once() {
    let entity_id = stable_id(7);
    let mut world = AuthoritativeWorld::default();
    let patch = patch(
        SceneRevision::INITIAL,
        1,
        vec![
            create(
                entity_id,
                vec![
                    ComponentValue::Light(LightComponent {
                        kind: LightKind::Point,
                        color: ColorRgb {
                            r: unit(1.0),
                            g: unit(0.8),
                            b: unit(0.6),
                        },
                        intensity: NonNegativeF32::new(12.0).unwrap(),
                    }),
                    ComponentValue::Material(MaterialComponent {
                        base_color: ColorRgba {
                            r: unit(0.2),
                            g: unit(0.3),
                            b: unit(0.4),
                            a: unit(1.0),
                        },
                        metallic: unit(0.1),
                        roughness: unit(0.7),
                    }),
                    ComponentValue::Primitive(PrimitiveComponent {
                        shape: PrimitiveShape::Cuboid,
                        dimensions: PositiveVec3 {
                            x: positive(1.0),
                            y: positive(2.0),
                            z: positive(3.0),
                        },
                    }),
                    ComponentValue::LocalTransform(LocalTransform {
                        translation: Vec3 {
                            x: finite(1.0),
                            y: finite(2.0),
                            z: finite(3.0),
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
                    }),
                    ComponentValue::Camera(CameraComponent {
                        vertical_fov_radians: positive(1.0),
                        near: positive(0.1),
                        far: positive(100.0),
                    }),
                    name("before"),
                ],
            ),
            SceneOperation::SetComponent(SetComponent {
                entity_id,
                component: name("after"),
            }),
            SceneOperation::RemoveComponent(RemoveComponent {
                entity_id,
                component: ComponentKind::Light,
            }),
        ],
    );

    let receipt = world.apply_patch(&patch, frame(1)).unwrap();
    assert_eq!(receipt.status, ApplyStatus::Applied);
    assert_eq!(receipt.previous_revision, SceneRevision::INITIAL);
    assert_eq!(receipt.new_revision, SceneRevision::new(1));
    assert_eq!(receipt.operation_count.get(), 3);
    assert_eq!(world.revision(), SceneRevision::new(1));
    assert_eq!(world.idempotency_record_count(), 1);
    world.validate_invariants().unwrap();

    let snapshot = world.snapshot().unwrap();
    let entity = snapshot.entity(entity_id).unwrap();
    let kinds = entity
        .components()
        .iter()
        .map(ComponentValue::kind)
        .collect::<Vec<_>>();
    assert_eq!(
        kinds,
        vec![
            ComponentKind::Name,
            ComponentKind::LocalTransform,
            ComponentKind::Primitive,
            ComponentKind::Material,
            ComponentKind::Camera,
        ]
    );
    assert_eq!(entity.component(ComponentKind::Name), Some(&name("after")));
    assert_eq!(entity.component(ComponentKind::Light), None);
}

#[test]
fn invalid_late_operation_preserves_revision_index_and_snapshot() {
    let entity_id = stable_id(1);
    let missing_id = stable_id(999);
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &patch(
                SceneRevision::INITIAL,
                1,
                vec![create(entity_id, vec![name("original")])],
            ),
            frame(1),
        )
        .unwrap();
    let before = world.snapshot().unwrap();
    let records_before = world.idempotency_record_count();

    let rejected = patch(
        world.revision(),
        2,
        vec![
            SceneOperation::SetComponent(SetComponent {
                entity_id,
                component: name("must-not-commit"),
            }),
            SceneOperation::Delete(DeleteEntity {
                entity_id: missing_id,
            }),
        ],
    );
    assert_eq!(
        world.apply_patch(&rejected, frame(2)).unwrap_err(),
        WorldApplyError::EntityNotFound {
            operation_index: 1,
            entity_id: missing_id,
        }
    );
    assert_eq!(world.snapshot().unwrap(), before);
    assert_eq!(world.revision(), SceneRevision::new(1));
    assert_eq!(world.idempotency_record_count(), records_before);
    assert!(world.contains(entity_id));
    world.validate_invariants().unwrap();
}

#[test]
fn protocol_and_duplicate_entity_errors_precede_mutation() {
    let entity_id = stable_id(1);
    let mut world = AuthoritativeWorld::default();
    let invalid = patch(
        world.revision(),
        1,
        vec![create(entity_id, vec![name("one"), name("two")])],
    );
    let WorldApplyError::InvalidPatch(validation) =
        world.apply_patch(&invalid, frame(1)).unwrap_err()
    else {
        panic!("duplicate component should fail protocol validation")
    };
    assert_eq!(validation.code(), DiagnosticCode::DuplicateComponent);
    assert!(world.snapshot().unwrap().is_empty());

    world
        .apply_patch(
            &patch(world.revision(), 2, vec![create(entity_id, Vec::new())]),
            frame(2),
        )
        .unwrap();
    let before = world.snapshot().unwrap();
    assert_eq!(
        world
            .apply_patch(
                &patch(world.revision(), 3, vec![create(entity_id, Vec::new())],),
                frame(3),
            )
            .unwrap_err(),
        WorldApplyError::EntityAlreadyExists {
            operation_index: 0,
            entity_id,
        }
    );
    assert_eq!(world.snapshot().unwrap(), before);
}

#[test]
fn stable_ids_remain_correct_after_deletion_and_storage_reuse() {
    let first_id = stable_id(1);
    let second_id = stable_id(2);
    let mut world = AuthoritativeWorld::default();

    world
        .apply_patch(
            &patch(
                world.revision(),
                1,
                vec![create(first_id, vec![name("first")])],
            ),
            frame(1),
        )
        .unwrap();
    world
        .apply_patch(
            &patch(
                world.revision(),
                2,
                vec![SceneOperation::Delete(DeleteEntity {
                    entity_id: first_id,
                })],
            ),
            frame(2),
        )
        .unwrap();
    world
        .apply_patch(
            &patch(
                world.revision(),
                3,
                vec![create(second_id, vec![name("second")])],
            ),
            frame(3),
        )
        .unwrap();

    let snapshot = world.snapshot().unwrap();
    assert_eq!(snapshot.entities().len(), 1);
    assert_eq!(snapshot.entities()[0].entity_id(), second_id);
    assert!(!world.contains(first_id));
    assert!(world.contains(second_id));
    world.validate_invariants().unwrap();
}

#[test]
fn snapshots_sort_entities_independently_of_operation_order() {
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &patch(
                world.revision(),
                1,
                vec![
                    create(stable_id(9), Vec::new()),
                    create(stable_id(2), Vec::new()),
                    create(stable_id(5), Vec::new()),
                ],
            ),
            frame(1),
        )
        .unwrap();

    let ordered_ids = world
        .snapshot()
        .unwrap()
        .entities()
        .iter()
        .map(cogniform_world::EntitySnapshot::entity_id)
        .collect::<Vec<_>>();
    assert_eq!(ordered_ids, vec![stable_id(2), stable_id(5), stable_id(9)]);
}

#[test]
fn accepted_idempotency_key_replays_without_duplicate_effects() {
    let entity_id = stable_id(1);
    let mut world = AuthoritativeWorld::default();
    let original = patch(
        SceneRevision::INITIAL,
        1,
        vec![create(entity_id, vec![name("once")])],
    );

    let first = world.apply_patch(&original, frame(1)).unwrap();
    let replay = world.apply_patch(&original, frame(99)).unwrap();
    let mut expected_replay = first.clone();
    expected_replay.status = ApplyStatus::IdempotentReplay;
    assert_eq!(replay, expected_replay);
    assert_eq!(world.revision(), SceneRevision::new(1));
    assert_eq!(world.entity_count(), 1);
    assert_eq!(world.idempotency_record_count(), 1);

    let mut conflicting = original.clone();
    conflicting.transaction_id = TransactionId::new(99).unwrap();
    assert!(matches!(
        world.apply_patch(&conflicting, frame(2)),
        Err(WorldApplyError::IdempotencyKeyConflict { .. })
    ));
    assert_eq!(world.revision(), SceneRevision::new(1));
}

#[test]
fn stale_reparent_and_missing_component_rejections_are_atomic() {
    let entity_id = stable_id(1);
    let parent_id = stable_id(2);
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &patch(
                world.revision(),
                1,
                vec![create(entity_id, Vec::new()), create(parent_id, Vec::new())],
            ),
            frame(1),
        )
        .unwrap();
    let before = world.snapshot().unwrap();

    let stale = patch(
        SceneRevision::INITIAL,
        2,
        vec![create(stable_id(3), vec![])],
    );
    assert!(matches!(
        world.apply_patch(&stale, frame(2)),
        Err(WorldApplyError::BaseRevisionMismatch { .. })
    ));

    let reparent = patch(
        world.revision(),
        3,
        vec![SceneOperation::Reparent(ReparentEntity {
            entity_id,
            parent_id: Some(parent_id),
        })],
    );
    assert_eq!(
        world.apply_patch(&reparent, frame(3)).unwrap_err(),
        WorldApplyError::UnsupportedOperation {
            operation_index: 0,
            entity_id,
        }
    );

    let remove = patch(
        world.revision(),
        4,
        vec![SceneOperation::RemoveComponent(RemoveComponent {
            entity_id,
            component: ComponentKind::Name,
        })],
    );
    assert_eq!(
        world.apply_patch(&remove, frame(4)).unwrap_err(),
        WorldApplyError::ComponentNotFound {
            operation_index: 0,
            entity_id,
            component: ComponentKind::Name,
        }
    );
    assert_eq!(world.snapshot().unwrap(), before);
}

#[test]
fn entity_and_idempotency_capacities_fail_closed() {
    let config = WorldConfig {
        max_entities: NonZeroU32::new(1).unwrap(),
        max_idempotency_records: NonZeroU32::new(8).unwrap(),
        ..WorldConfig::default()
    };
    let mut world = AuthoritativeWorld::new(config);
    let over_capacity = patch(
        world.revision(),
        1,
        vec![
            create(stable_id(1), Vec::new()),
            create(stable_id(2), Vec::new()),
        ],
    );
    assert_eq!(
        world.apply_patch(&over_capacity, frame(1)).unwrap_err(),
        WorldApplyError::EntityCapacityExceeded {
            operation_index: 1,
            entity_id: stable_id(2),
            limit: 1,
        }
    );
    assert!(world.snapshot().unwrap().is_empty());

    let config = WorldConfig {
        max_idempotency_records: NonZeroU32::new(1).unwrap(),
        ..WorldConfig::default()
    };
    let mut world = AuthoritativeWorld::new(config);
    world
        .apply_patch(
            &patch(world.revision(), 1, vec![create(stable_id(1), Vec::new())]),
            frame(1),
        )
        .unwrap();
    let before = world.snapshot().unwrap();
    let delete = patch(
        world.revision(),
        2,
        vec![SceneOperation::Delete(DeleteEntity {
            entity_id: stable_id(1),
        })],
    );
    assert_eq!(
        world.apply_patch(&delete, frame(2)).unwrap_err(),
        WorldApplyError::IdempotencyCapacityExceeded { limit: 1 }
    );
    assert_eq!(world.snapshot().unwrap(), before);
}

#[test]
fn deterministic_randomized_sequences_match_a_logical_model() {
    let mut world = AuthoritativeWorld::default();
    let mut model = BTreeMap::<StableEntityId, Option<String>>::new();
    let mut expected_revision = SceneRevision::INITIAL;
    let mut random = Lcg::new(0x5eed_cafe);

    for step in 0_u64..256 {
        let before = world.snapshot().unwrap();
        let mut candidate = model.clone();
        let mut operations = Vec::new();
        let operation_count = 1 + (random.next() % 4);

        for operation_index in 0..operation_count {
            let choice = random.next() % 4;
            if candidate.is_empty() || (choice == 0 && candidate.len() < 16) {
                let entity_id = next_available_id(&candidate, &mut random);
                let initial_name = random
                    .next()
                    .is_multiple_of(2)
                    .then(|| format!("entity-{step}-{operation_index}"));
                let components = initial_name
                    .as_deref()
                    .map(|value| vec![name(value)])
                    .unwrap_or_default();
                operations.push(create(entity_id, components));
                candidate.insert(entity_id, initial_name);
                continue;
            }

            let entity_id = choose_live_id(&candidate, &mut random);
            match choice {
                0 | 1 => {
                    let value = format!("set-{step}-{operation_index}");
                    operations.push(SceneOperation::SetComponent(SetComponent {
                        entity_id,
                        component: name(&value),
                    }));
                    candidate.insert(entity_id, Some(value));
                }
                2 if candidate[&entity_id].is_some() => {
                    operations.push(SceneOperation::RemoveComponent(RemoveComponent {
                        entity_id,
                        component: ComponentKind::Name,
                    }));
                    candidate.insert(entity_id, None);
                }
                _ => {
                    operations.push(SceneOperation::Delete(DeleteEntity { entity_id }));
                    candidate.remove(&entity_id);
                }
            }
        }

        let should_reject = step % 7 == 0;
        if should_reject {
            operations.push(SceneOperation::Delete(DeleteEntity {
                entity_id: stable_id(10_000),
            }));
        }
        let patch = patch(world.revision(), u128::from(step) + 1, operations);

        if should_reject {
            assert!(matches!(
                world.apply_patch(&patch, frame(step + 1)),
                Err(WorldApplyError::EntityNotFound { .. })
            ));
            assert_eq!(world.snapshot().unwrap(), before);
        } else {
            world.apply_patch(&patch, frame(step + 1)).unwrap();
            model = candidate;
            expected_revision = expected_revision.checked_next().unwrap();
        }

        let snapshot = world.snapshot().unwrap();
        assert_eq!(snapshot.revision(), expected_revision);
        assert_eq!(snapshot_names(&snapshot), model);
        world.validate_invariants().unwrap();
    }
}

fn snapshot_names(
    snapshot: &cogniform_world::WorldSnapshot,
) -> BTreeMap<StableEntityId, Option<String>> {
    snapshot
        .entities()
        .iter()
        .map(|entity| {
            let name = entity.component(ComponentKind::Name).map(|value| {
                let ComponentValue::Name(name) = value else {
                    unreachable!("component lookup returned the requested kind")
                };
                name.value.as_str().to_owned()
            });
            (entity.entity_id(), name)
        })
        .collect()
}

fn choose_live_id(
    entities: &BTreeMap<StableEntityId, Option<String>>,
    random: &mut Lcg,
) -> StableEntityId {
    let index = usize::try_from(random.next()).unwrap() % entities.len();
    *entities
        .keys()
        .nth(index)
        .expect("index is bounded by live entity count")
}

fn next_available_id(
    entities: &BTreeMap<StableEntityId, Option<String>>,
    random: &mut Lcg,
) -> StableEntityId {
    for _ in 0..16 {
        let candidate = stable_id(u128::from((random.next() % 16) + 1));
        if !entities.contains_key(&candidate) {
            return candidate;
        }
    }
    (1_u128..=16)
        .map(stable_id)
        .find(|candidate| !entities.contains_key(candidate))
        .expect("caller checked that fewer than 16 entities are live")
}

struct Lcg(u64);

impl Lcg {
    const fn new(seed: u64) -> Self {
        Self(seed)
    }

    fn next(&mut self) -> u64 {
        self.0 = self
            .0
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        self.0
    }
}
