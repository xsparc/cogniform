//! Controlled adapter contract for verified GLB decode, upload, and rendering.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use cogniform_assets::{AssetMeshKey, AssetState, AssetStore, AssetVertex, content_hash};
use cogniform_protocol::{
    AssetMeshComponent, CameraComponent, ComponentValue, ConflictPolicy, CreateEntity,
    DeliverySemantic, FiniteF32, FrameId, IdempotencyKey, LocalTransform, PatchBudget, PositiveF32,
    PositiveVec3, Quaternion, SceneOperation, ScenePatch, SceneRevision, SchemaVersion,
    StableEntityId, TransactionId, Vec3,
};
use cogniform_renderer::{AssetUploadAdmission, HeadlessRenderer, RendererConfig};
use cogniform_world::AuthoritativeWorld;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn approved_glb_fixture_renders_with_identity_color_depth_and_winding_normal() {
    let bytes = decode_hex(include_str!("../../../tests/assets/triangle.glb.hex"));
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let upload = assets.upload_job(key).unwrap();
    let expected_normal = triangle_normal(upload.vertices());

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    assert_eq!(
        renderer.enqueue_asset_upload(upload).unwrap(),
        AssetUploadAdmission::Queued { key }
    );
    assert_eq!(renderer.asset_stats().pending_uploads, 1);
    assert_eq!(renderer.asset_stats().resident_meshes, 0);
    let uploaded = renderer
        .process_next_asset_upload()
        .expect("one upload should be processed");
    assert_eq!(uploaded.key, key);
    assert_eq!(uploaded.vertex_count, 3);
    assert_eq!(renderer.asset_stats().pending_uploads, 0);
    assert_eq!(renderer.asset_stats().resident_meshes, 1);

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &scene_patch(camera, triangle, content_hash),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    let extraction = world.take_render_extraction().unwrap();
    renderer.apply_extraction(&extraction).unwrap();
    let frame = renderer.submit_scene(camera).unwrap().read().unwrap();

    let center = (WIDTH / 2, HEIGHT / 2);
    assert_eq!(
        frame.stable_entity_id_at(center.0, center.1),
        Some(triangle)
    );
    for (actual, expected) in frame
        .color_at(center.0, center.1)
        .unwrap()
        .into_iter()
        .zip([51, 153, 230, 255])
    {
        assert!(actual.abs_diff(expected) <= 2);
    }
    let depth = frame.depth_at(center.0, center.1).unwrap();
    assert!(
        depth < 1.0,
        "asset triangle should write depth at the frame center"
    );
    let normal = frame
        .normal_at(center.0, center.1)
        .expect("asset triangle should write a normal at the frame center");
    let dot = normal
        .iter()
        .zip(expected_normal)
        .map(|(actual, expected)| actual * expected)
        .sum::<f32>();
    assert!(
        dot >= 0.99,
        "normal must follow triangle winding: {normal:?}"
    );
    assert_eq!(frame.normal_at(0, 0), None);
}

fn triangle_normal(vertices: &[AssetVertex]) -> [f32; 3] {
    let positions = vertices
        .iter()
        .take(3)
        .map(|vertex| vertex.position.map(cogniform_protocol::FiniteF32::get))
        .collect::<Vec<_>>();
    let first = [
        positions[1][0] - positions[0][0],
        positions[1][1] - positions[0][1],
        positions[1][2] - positions[0][2],
    ];
    let second = [
        positions[2][0] - positions[0][0],
        positions[2][1] - positions[0][1],
        positions[2][2] - positions[0][2],
    ];
    let mut normal = [
        first[1] * second[2] - first[2] * second[1],
        first[2] * second[0] - first[0] * second[2],
        first[0] * second[1] - first[1] * second[0],
    ];
    let inverse_length = normal
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt()
        .recip();
    for value in &mut normal {
        *value *= inverse_length;
    }
    normal
}

fn scene_patch(
    camera: StableEntityId,
    triangle: StableEntityId,
    content_hash: cogniform_protocol::ContentHash,
) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(2).unwrap(),
        idempotency_key: IdempotencyKey::new(3).unwrap(),
        base_revision: SceneRevision::INITIAL,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![
            SceneOperation::Create(CreateEntity {
                entity_id: camera,
                components: vec![
                    ComponentValue::LocalTransform(transform(3.0)),
                    ComponentValue::Camera(CameraComponent {
                        vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                        near: positive(0.1),
                        far: positive(100.0),
                    }),
                ],
            }),
            SceneOperation::Create(CreateEntity {
                entity_id: triangle,
                components: vec![
                    ComponentValue::LocalTransform(transform(0.0)),
                    ComponentValue::AssetMesh(AssetMeshComponent {
                        content_hash,
                        mesh_index: 0,
                    }),
                ],
            }),
        ],
    }
}

fn transform(z: f32) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(0.0),
            y: finite(0.0),
            z: finite(z),
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

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}
