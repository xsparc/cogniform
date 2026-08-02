//! Hierarchy, transform propagation, and canonical hash contracts.

use core::num::NonZeroU32;

use cogniform_protocol::{
    ComponentValue, ConflictPolicy, CreateEntity, DeliverySemantic, FiniteF32, FrameId,
    IdempotencyKey, LocalTransform, PatchBudget, PositiveF32, PositiveVec3, Quaternion,
    ReparentEntity, SceneOperation, ScenePatch, SceneRevision, SchemaVersion, SetComponent,
    StableEntityId, TransactionId, Vec3,
};
use cogniform_world::{AuthoritativeWorld, WorldApplyError, WorldConfig};

fn id(value: u128) -> StableEntityId {
    StableEntityId::new(value).unwrap()
}

fn patch(base: SceneRevision, nonce: u128, operations: Vec<SceneOperation>) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(nonce * 2).unwrap(),
        idempotency_key: IdempotencyKey::new((nonce * 2) + 1).unwrap(),
        base_revision: base,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations,
    }
}

fn create(entity_id: StableEntityId, local: Option<LocalTransform>) -> SceneOperation {
    SceneOperation::Create(CreateEntity {
        entity_id,
        components: local
            .map(ComponentValue::LocalTransform)
            .into_iter()
            .collect(),
    })
}

fn reparent(entity_id: StableEntityId, parent_id: StableEntityId) -> SceneOperation {
    SceneOperation::Reparent(ReparentEntity {
        entity_id,
        parent_id: Some(parent_id),
    })
}

fn transform(translation: [f32; 3], rotation: [f32; 4], scale: [f32; 3]) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: FiniteF32::new(translation[0]).unwrap(),
            y: FiniteF32::new(translation[1]).unwrap(),
            z: FiniteF32::new(translation[2]).unwrap(),
        },
        rotation: Quaternion {
            x: FiniteF32::new(rotation[0]).unwrap(),
            y: FiniteF32::new(rotation[1]).unwrap(),
            z: FiniteF32::new(rotation[2]).unwrap(),
            w: FiniteF32::new(rotation[3]).unwrap(),
        },
        scale: PositiveVec3 {
            x: PositiveF32::new(scale[0]).unwrap(),
            y: PositiveF32::new(scale[1]).unwrap(),
            z: PositiveF32::new(scale[2]).unwrap(),
        },
    }
}

fn translation(matrix: &[f64; 16]) -> [f64; 3] {
    [matrix[12], matrix[13], matrix[14]]
}

fn assert_translation(actual: [f64; 3], expected: [f64; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual - expected).abs() < 1.0e-9);
    }
}

#[test]
fn cycles_depth_and_dangling_children_reject_the_complete_patch() {
    let root = id(1);
    let child = id(2);
    let grandchild = id(3);
    let leaf = id(4);
    let config = WorldConfig {
        max_hierarchy_depth: NonZeroU32::new(2).unwrap(),
        ..WorldConfig::default()
    };
    let mut world = AuthoritativeWorld::new(config);
    world
        .apply_patch(
            &patch(
                world.revision(),
                1,
                vec![
                    create(root, None),
                    create(child, None),
                    create(grandchild, None),
                ],
            ),
            FrameId::new(1).unwrap(),
        )
        .unwrap();

    let before = world.snapshot().unwrap();
    let before_hash = world.logical_hash().unwrap();
    let before_generation = world.world_transform(child).unwrap().generation();
    let cycle = patch(
        world.revision(),
        2,
        vec![
            reparent(child, root),
            reparent(grandchild, child),
            reparent(root, grandchild),
        ],
    );
    assert!(matches!(
        world.apply_patch(&cycle, FrameId::new(2).unwrap()),
        Err(WorldApplyError::HierarchyCycle { .. })
    ));
    assert_eq!(world.snapshot().unwrap(), before);
    assert_eq!(world.logical_hash().unwrap(), before_hash);
    assert_eq!(
        world.world_transform(child).unwrap().generation(),
        before_generation
    );

    let too_deep = patch(
        world.revision(),
        3,
        vec![
            create(leaf, None),
            reparent(child, root),
            reparent(grandchild, child),
            reparent(leaf, grandchild),
        ],
    );
    assert!(matches!(
        world.apply_patch(&too_deep, FrameId::new(3).unwrap()),
        Err(WorldApplyError::HierarchyDepthExceeded {
            depth: 3,
            limit: 2,
            ..
        })
    ));
    assert_eq!(world.snapshot().unwrap(), before);

    world
        .apply_patch(
            &patch(world.revision(), 4, vec![reparent(child, root)]),
            FrameId::new(4).unwrap(),
        )
        .unwrap();
    let before_delete = world.snapshot().unwrap();
    let delete_parent = patch(
        world.revision(),
        5,
        vec![SceneOperation::Delete(cogniform_protocol::DeleteEntity {
            entity_id: root,
        })],
    );
    assert_eq!(
        world
            .apply_patch(&delete_parent, FrameId::new(5).unwrap())
            .unwrap_err(),
        WorldApplyError::HierarchyParentNotFound {
            entity_id: child,
            parent_id: root,
        }
    );
    assert_eq!(world.snapshot().unwrap(), before_delete);
}

#[test]
fn sparse_propagation_is_stable_parent_before_child() {
    let root = id(10);
    let child = id(20);
    let grandchild = id(30);
    let sibling = id(40);
    let identity = [0.0, 0.0, 0.0, 1.0];
    let one = [1.0, 1.0, 1.0];
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &patch(
                world.revision(),
                10,
                vec![
                    create(root, Some(transform([10.0, 0.0, 0.0], identity, one))),
                    create(child, Some(transform([1.0, 0.0, 0.0], identity, one))),
                    create(grandchild, Some(transform([2.0, 0.0, 0.0], identity, one))),
                    create(sibling, Some(transform([0.0, 5.0, 0.0], identity, one))),
                    reparent(child, root),
                    reparent(grandchild, child),
                ],
            ),
            FrameId::new(1).unwrap(),
        )
        .unwrap();

    assert_translation(
        translation(world.world_transform(grandchild).unwrap().matrix()),
        [13.0, 0.0, 0.0],
    );
    let root_generation = world.world_transform(root).unwrap().generation();
    let sibling_generation = world.world_transform(sibling).unwrap().generation();

    world
        .apply_patch(
            &patch(
                world.revision(),
                11,
                vec![SceneOperation::SetComponent(SetComponent {
                    entity_id: child,
                    component: ComponentValue::LocalTransform(transform(
                        [4.0, 0.0, 0.0],
                        identity,
                        one,
                    )),
                })],
            ),
            FrameId::new(2).unwrap(),
        )
        .unwrap();

    let child_generation = world.world_transform(child).unwrap().generation();
    assert_translation(
        translation(world.world_transform(grandchild).unwrap().matrix()),
        [16.0, 0.0, 0.0],
    );
    assert_eq!(
        world.world_transform(root).unwrap().generation(),
        root_generation
    );
    assert_eq!(
        world.world_transform(sibling).unwrap().generation(),
        sibling_generation
    );
    assert_eq!(
        world.world_transform(grandchild).unwrap().generation(),
        child_generation
    );

    world
        .apply_patch(
            &patch(world.revision(), 12, vec![reparent(grandchild, sibling)]),
            FrameId::new(3).unwrap(),
        )
        .unwrap();
    assert_translation(
        translation(world.world_transform(grandchild).unwrap().matrix()),
        [2.0, 5.0, 0.0],
    );
    assert_eq!(
        world.world_transform(child).unwrap().generation(),
        child_generation
    );
    assert!(world.world_transform(grandchild).unwrap().generation() > child_generation);
    assert_eq!(
        world.children(root).unwrap().collect::<Vec<_>>(),
        vec![child]
    );
}

#[test]
fn matrix_composition_normalizes_rotation_and_applies_parent_scale() {
    let parent = id(100);
    let child = id(200);
    let half_sqrt = core::f32::consts::FRAC_1_SQRT_2 * 3.0;
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &patch(
                world.revision(),
                20,
                vec![
                    create(
                        parent,
                        Some(transform(
                            [0.0, 0.0, 0.0],
                            [0.0, 0.0, half_sqrt, half_sqrt],
                            [2.0, 2.0, 2.0],
                        )),
                    ),
                    create(
                        child,
                        Some(transform(
                            [1.0, 0.0, 0.0],
                            [0.0, 0.0, 0.0, 1.0],
                            [1.0, 1.0, 1.0],
                        )),
                    ),
                    reparent(child, parent),
                ],
            ),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    let position = translation(world.world_transform(child).unwrap().matrix());
    assert!(position[0].abs() < 1.0e-12);
    assert!((position[1] - 2.0).abs() < 1.0e-6);
    assert!(position[2].abs() < 1.0e-12);
}

#[test]
fn canonical_hash_ignores_revision_and_operation_order_but_tracks_parentage() {
    let first = id(7);
    let second = id(9);
    let local = transform([1.0, 2.0, 3.0], [0.0, 0.0, 0.0, 2.0], [1.0, 1.0, 1.0]);
    let mut left = AuthoritativeWorld::default();
    assert_eq!(
        left.logical_hash().unwrap().to_string(),
        "85d3bc72c8357ea5fcf8aaf9b6569c1c202825e270ad3e7c4bfd44314751612f"
    );
    let mut right = AuthoritativeWorld::default();
    left.apply_patch(
        &patch(
            left.revision(),
            30,
            vec![create(second, None), create(first, Some(local))],
        ),
        FrameId::new(1).unwrap(),
    )
    .unwrap();
    right
        .apply_patch(
            &patch(
                right.revision(),
                31,
                vec![create(first, Some(local)), create(second, None)],
            ),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    assert_eq!(left.logical_hash().unwrap(), right.logical_hash().unwrap());

    left.apply_patch(
        &patch(left.revision(), 32, vec![reparent(first, second)]),
        FrameId::new(2).unwrap(),
    )
    .unwrap();
    assert_ne!(left.logical_hash().unwrap(), right.logical_hash().unwrap());
}

#[test]
fn non_finite_derived_transform_rejects_before_commit() {
    let identity = [0.0, 0.0, 0.0, 1.0];
    let huge = [f32::MAX, f32::MAX, f32::MAX];
    let mut operations = Vec::new();
    for value in 1..=12 {
        operations.push(create(
            id(value),
            Some(transform([0.0, 0.0, 0.0], identity, huge)),
        ));
        if value > 1 {
            operations.push(reparent(id(value), id(value - 1)));
        }
    }
    let mut world = AuthoritativeWorld::default();
    let before_hash = world.logical_hash().unwrap();
    assert!(matches!(
        world.apply_patch(
            &patch(world.revision(), 40, operations),
            FrameId::new(1).unwrap()
        ),
        Err(WorldApplyError::TransformOverflow { .. })
    ));
    assert_eq!(world.revision(), SceneRevision::INITIAL);
    assert_eq!(world.entity_count(), 0);
    assert_eq!(world.logical_hash().unwrap(), before_hash);
}
