//! Compact render-extraction contracts for sparse world changes.

use core::num::NonZeroU32;

use cogniform_protocol::{
    ColorRgba, ComponentValue, ConflictPolicy, CreateEntity, DeleteEntity, DeliverySemantic,
    FiniteF32, FrameId, IdempotencyKey, LocalTransform, MaterialComponent, NameComponent,
    PatchBudget, PositiveF32, PositiveVec3, PrimitiveComponent, PrimitiveShape, Quaternion,
    RenderChange, ReparentEntity, SceneOperation, ScenePatch, SceneRevision, SceneText,
    SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32, Vec3,
};
use cogniform_world::{AuthoritativeWorld, WorldApplyError, WorldConfig};

fn id(value: u128) -> StableEntityId {
    StableEntityId::new(value).unwrap()
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

fn unit(value: f32) -> UnitF32 {
    UnitF32::new(value).unwrap()
}

fn transform(x: f32) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(x),
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

fn primitive() -> ComponentValue {
    ComponentValue::Primitive(PrimitiveComponent {
        shape: PrimitiveShape::Cuboid,
        dimensions: PositiveVec3 {
            x: positive(1.0),
            y: positive(1.0),
            z: positive(1.0),
        },
    })
}

fn material(red: f32) -> ComponentValue {
    ComponentValue::Material(MaterialComponent {
        base_color: ColorRgba {
            r: unit(red),
            g: unit(0.4),
            b: unit(0.6),
            a: unit(1.0),
        },
        metallic: unit(0.0),
        roughness: unit(0.5),
    })
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

fn create(entity_id: StableEntityId, x: f32) -> SceneOperation {
    SceneOperation::Create(CreateEntity {
        entity_id,
        components: vec![
            ComponentValue::LocalTransform(transform(x)),
            primitive(),
            material(0.2),
        ],
    })
}

fn extracted_branch() -> (
    AuthoritativeWorld,
    StableEntityId,
    StableEntityId,
    StableEntityId,
) {
    let root = id(1);
    let child = id(2);
    let unrelated = id(3);
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &patch(
                world.revision(),
                1,
                vec![
                    create(root, 0.0),
                    create(child, 1.0),
                    create(unrelated, -1.0),
                    SceneOperation::Reparent(ReparentEntity {
                        entity_id: child,
                        parent_id: Some(root),
                    }),
                ],
            ),
            FrameId::new(1).unwrap(),
        )
        .unwrap();

    let initial = world.take_render_extraction().unwrap();
    assert_eq!(initial.base_revision(), SceneRevision::INITIAL);
    assert_eq!(initial.scene_revision(), SceneRevision::new(1));
    assert_eq!(
        initial
            .changes()
            .iter()
            .map(RenderChange::entity_id)
            .collect::<Vec<_>>(),
        vec![root, child, unrelated]
    );
    (world, root, child, unrelated)
}

#[test]
fn extraction_coalesces_sparse_changes_and_advances_logical_only_revisions() {
    let (mut world, root, child, unrelated) = extracted_branch();
    world
        .apply_patch(
            &patch(
                world.revision(),
                2,
                vec![SceneOperation::SetComponent(SetComponent {
                    entity_id: unrelated,
                    component: material(0.8),
                })],
            ),
            FrameId::new(2).unwrap(),
        )
        .unwrap();
    world
        .apply_patch(
            &patch(
                world.revision(),
                3,
                vec![SceneOperation::SetComponent(SetComponent {
                    entity_id: unrelated,
                    component: material(0.6),
                })],
            ),
            FrameId::new(3).unwrap(),
        )
        .unwrap();
    assert_eq!(world.pending_render_change_count(), 1);
    let coalesced = world.take_render_extraction().unwrap();
    assert_eq!(coalesced.base_revision(), SceneRevision::new(1));
    assert_eq!(coalesced.scene_revision(), SceneRevision::new(3));
    assert_eq!(coalesced.changes().len(), 1);
    assert_eq!(coalesced.changes()[0].entity_id(), unrelated);

    world
        .apply_patch(
            &patch(
                world.revision(),
                4,
                vec![SceneOperation::SetComponent(SetComponent {
                    entity_id: root,
                    component: ComponentValue::Name(NameComponent {
                        value: SceneText::new("logical-only").unwrap(),
                    }),
                })],
            ),
            FrameId::new(4).unwrap(),
        )
        .unwrap();
    let logical_only = world.take_render_extraction().unwrap();
    assert!(logical_only.changes().is_empty());
    assert_eq!(logical_only.base_revision(), SceneRevision::new(3));
    assert_eq!(logical_only.scene_revision(), SceneRevision::new(4));

    world
        .apply_patch(
            &patch(
                world.revision(),
                5,
                vec![SceneOperation::SetComponent(SetComponent {
                    entity_id: root,
                    component: ComponentValue::LocalTransform(transform(2.0)),
                })],
            ),
            FrameId::new(5).unwrap(),
        )
        .unwrap();
    let branch = world.take_render_extraction().unwrap();
    assert_eq!(
        branch
            .changes()
            .iter()
            .map(RenderChange::entity_id)
            .collect::<Vec<_>>(),
        vec![root, child]
    );

    world
        .apply_patch(
            &patch(
                world.revision(),
                6,
                vec![SceneOperation::Delete(DeleteEntity {
                    entity_id: unrelated,
                })],
            ),
            FrameId::new(6).unwrap(),
        )
        .unwrap();
    let removed = world.take_render_extraction().unwrap();
    assert_eq!(removed.changes(), &[RenderChange::remove(unrelated)]);
}

#[test]
fn extraction_capacity_rejects_before_world_mutation() {
    let config = WorldConfig {
        max_pending_render_changes: NonZeroU32::new(1).unwrap(),
        ..WorldConfig::default()
    };
    let mut world = AuthoritativeWorld::new(config);
    let before = world.snapshot().unwrap();
    let rejected = patch(
        world.revision(),
        1,
        vec![create(id(1), 0.0), create(id(2), 1.0)],
    );

    assert_eq!(
        world.apply_patch(&rejected, FrameId::new(1).unwrap()),
        Err(WorldApplyError::RenderExtractionCapacityExceeded { limit: 1 })
    );
    assert_eq!(world.snapshot().unwrap(), before);
    assert_eq!(world.pending_render_change_count(), 0);
}
