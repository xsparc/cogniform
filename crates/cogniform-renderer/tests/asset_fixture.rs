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
            &scene_patch(camera, triangle, content_hash, [1.0; 3]),
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

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn imported_normals_are_inverse_transformed_and_observable() {
    let bytes = smooth_fixture();
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let upload = assets.upload_job(key).unwrap();
    let source_normal = upload.vertices()[0]
        .normal
        .map(cogniform_protocol::FiniteF32::get);
    assert!(source_normal[0] > 0.9 && source_normal[2] < 0.3);

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.enqueue_asset_upload(upload).unwrap();
    renderer.process_next_asset_upload().unwrap();

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &scene_patch(camera, triangle, content_hash, [2.0, 1.0, 1.0]),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let frame = renderer.submit_scene(camera).unwrap().read().unwrap();
    let normal = frame
        .normal_at(WIDTH / 2, HEIGHT / 2)
        .expect("smooth triangle should write a normal at the frame center");
    let inverse_sqrt_six = 6.0_f32.sqrt().recip();
    let expected = [inverse_sqrt_six, 2.0 * inverse_sqrt_six, inverse_sqrt_six];
    let dot = normal
        .iter()
        .zip(expected)
        .map(|(actual, expected)| actual * expected)
        .sum::<f32>();
    assert!(
        dot >= 0.99,
        "normal must use the model inverse-transpose: {normal:?}"
    );
    assert!(
        normal[2] < 0.6,
        "imported normal must differ from the +Z face normal"
    );
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
    triangle_scale: [f32; 3],
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
                    ComponentValue::LocalTransform(transform(3.0, [1.0; 3])),
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
                    ComponentValue::LocalTransform(transform(0.0, triangle_scale)),
                    ComponentValue::AssetMesh(AssetMeshComponent {
                        content_hash,
                        mesh_index: 0,
                    }),
                ],
            }),
        ],
    }
}

fn transform(z: f32, scale: [f32; 3]) -> LocalTransform {
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
            x: positive(scale[0]),
            y: positive(scale[1]),
            z: positive(scale[2]),
        },
    }
}

fn smooth_fixture() -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ];
    let normals = [[4.0_f32, 0.0, 1.0], [4.0, 0.0, 1.0], [0.0, 4.0, 1.0]];
    let mut binary = Vec::with_capacity(72);
    for vertex in positions.into_iter().chain(normals) {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":72}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"mode":4}]}]}"#;
    glb_with_json(json, &binary)
}

fn glb_with_json(json: &str, binary: &[u8]) -> Vec<u8> {
    let mut json = json.as_bytes().to_vec();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut binary = binary.to_vec();
    binary.resize(binary.len().next_multiple_of(4), 0);
    let length = 12 + 8 + json.len() + 8 + binary.len();
    let mut output = Vec::with_capacity(length);
    output.extend_from_slice(b"glTF");
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&u32::try_from(length).unwrap().to_le_bytes());
    output.extend_from_slice(&u32::try_from(json.len()).unwrap().to_le_bytes());
    output.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(&u32::try_from(binary.len()).unwrap().to_le_bytes());
    output.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    output.extend_from_slice(&binary);
    output
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
