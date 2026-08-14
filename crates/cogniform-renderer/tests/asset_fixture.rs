//! Controlled adapter contract for verified GLB decode, upload, and rendering.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use cogniform_assets::{
    ASSET_VERTEX_BYTES, AssetMeshKey, AssetState, AssetStore, AssetVertex, content_hash,
};
use cogniform_protocol::{
    AssetMeshComponent, CameraComponent, ColorRgb, ColorRgba, ComponentValue, ConflictPolicy,
    CreateEntity, DeliverySemantic, FiniteF32, FrameId, IdempotencyKey, LightComponent, LightKind,
    LocalTransform, MaterialComponent, NonNegativeF32, PatchBudget, PositiveF32, PositiveVec3,
    PrimitiveComponent, PrimitiveShape, Quaternion, SceneOperation, ScenePatch, SceneRevision,
    SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32, Vec3,
};
use cogniform_renderer::{AssetUploadAdmission, HeadlessRenderer, RenderedFrame, RendererConfig};
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
    let repeated_upload = upload.clone();
    let expected_normal = triangle_normal(upload.vertices());

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    assert_eq!(
        renderer.enqueue_asset_upload(upload).unwrap(),
        AssetUploadAdmission::Queued { key }
    );
    assert_eq!(renderer.asset_stats().pending_uploads, 1);
    assert!(
        renderer
            .asset_stats()
            .oldest_pending_upload_age_micros
            .is_some()
    );
    assert_eq!(renderer.asset_stats().resident_meshes, 0);
    let uploaded = renderer
        .process_next_asset_upload()
        .expect("one upload should be processed");
    assert_eq!(uploaded.key, key);
    assert_eq!(uploaded.vertex_count, 3);
    assert_eq!(renderer.asset_stats().pending_uploads, 0);
    assert_eq!(
        renderer.asset_stats().oldest_pending_upload_age_micros,
        None
    );
    assert_eq!(renderer.asset_stats().resident_meshes, 1);
    assert_eq!(
        renderer.enqueue_asset_upload(repeated_upload).unwrap(),
        AssetUploadAdmission::AlreadyResident { key }
    );
    assert_eq!(
        renderer.asset_stats().oldest_pending_upload_age_micros,
        None
    );

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
fn primary_texcoords_are_retained_without_changing_rendered_observations() {
    let baseline = imported_frame(decode_hex(include_str!(
        "../../../tests/assets/triangle.glb.hex"
    )));
    let bytes = primary_uv_fixture();
    let content_hash = content_hash(&bytes);
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes.clone()).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let upload = assets
        .upload_job(AssetMeshKey {
            content_hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.byte_len(), 3 * ASSET_VERTEX_BYTES);
    for (vertex, expected) in
        upload
            .vertices()
            .iter()
            .zip([[-0.25, 1.25], [2.0, -3.0], [0.5, 0.75]])
    {
        assert_eq!(
            vertex.texcoord_0.map(FiniteF32::get).map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }

    let with_texcoords = imported_frame(bytes);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(with_texcoords.color_at(x, y), baseline.color_at(x, y));
            assert_eq!(with_texcoords.depth_at(x, y), baseline.depth_at(x, y));
            assert_eq!(
                with_texcoords.stable_entity_id_at(x, y),
                baseline.stable_entity_id_at(x, y)
            );
            assert_eq!(with_texcoords.normal_at(x, y), baseline.normal_at(x, y));
        }
    }
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn imported_material_factors_drive_direct_light_and_scene_override() {
    let bytes = metallic_fixture();
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let upload = assets.upload_job(key).unwrap();
    assert_eq!(
        upload.material().metallic().get().to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        upload.material().roughness().get().to_bits(),
        0.5_f32.to_bits()
    );

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.enqueue_asset_upload(upload).unwrap();
    renderer.process_next_asset_upload().unwrap();

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let light = StableEntityId::new(3).unwrap();
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &scene_patch(camera, triangle, content_hash, [1.0; 3]),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let unlit = renderer.submit_scene(camera).unwrap().read().unwrap();
    assert_eq!(renderer.scene_revision(), SceneRevision::new(1));

    world
        .apply_patch(
            &add_directional_light_patch(SceneRevision::new(1), light),
            FrameId::new(2).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let imported = renderer.submit_scene(camera).unwrap().read().unwrap();
    assert_eq!(renderer.scene_revision(), SceneRevision::new(2));

    world
        .apply_patch(
            &override_material_patch(SceneRevision::new(2), triangle),
            FrameId::new(3).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let overridden = renderer.submit_scene(camera).unwrap().read().unwrap();
    assert_eq!(renderer.scene_revision(), SceneRevision::new(3));

    let center = (WIDTH / 2, HEIGHT / 2);
    assert_eq!(
        unlit.color_at(center.0, center.1),
        Some([204, 102, 51, 255])
    );
    assert_color_near(&imported, center, [129, 65, 32, 255]);
    assert_color_near(&overridden, center, [38, 22, 14, 255]);
    for frame in [&unlit, &imported, &overridden] {
        assert_eq!(
            frame.stable_entity_id_at(center.0, center.1),
            Some(triangle)
        );
        assert_eq!(frame.stable_entity_id_at(0, 0), None);
        assert_eq!(frame.normal_at(0, 0), None);
    }
    assert_eq!(
        imported.depth_at(center.0, center.1),
        unlit.depth_at(center.0, center.1)
    );
    assert_eq!(
        overridden.depth_at(center.0, center.1),
        unlit.depth_at(center.0, center.1)
    );
    assert_eq!(
        imported.normal_at(center.0, center.1),
        unlit.normal_at(center.0, center.1)
    );
    assert_eq!(
        overridden.normal_at(center.0, center.1),
        unlit.normal_at(center.0, center.1)
    );
    assert_eq!(imported.color_at(0, 0), unlit.color_at(0, 0));
    assert_eq!(overridden.color_at(0, 0), unlit.color_at(0, 0));
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn embedded_base_color_texture_preserves_orientation_factor_override_and_residency() {
    let bytes = textured_two_mesh_fixture();
    let content_hash = content_hash(&bytes);
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    upload_textured_meshes(&mut renderer, &assets, content_hash);

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let light = StableEntityId::new(3).unwrap();
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &scene_patch(camera, triangle, content_hash, [1.0; 3]),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let top_left = renderer.submit_scene(camera).unwrap().read().unwrap();

    world
        .apply_patch(
            &set_asset_mesh_patch(SceneRevision::new(1), triangle, content_hash, 1),
            FrameId::new(2).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let bottom_left = renderer.submit_scene(camera).unwrap().read().unwrap();

    world
        .apply_patch(
            &add_directional_light_patch(SceneRevision::new(2), light),
            FrameId::new(3).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let lit = renderer.submit_scene(camera).unwrap().read().unwrap();

    world
        .apply_patch(
            &override_material_patch(SceneRevision::new(3), triangle),
            FrameId::new(4).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let overridden = renderer.submit_scene(camera).unwrap().read().unwrap();

    let center = (WIDTH / 2, HEIGHT / 2);
    assert_color_near(&top_left, center, [28, 3, 4, 64]);
    assert_color_near(&bottom_left, center, [2, 14, 255, 32]);
    assert_ne!(
        lit.color_at(center.0, center.1),
        bottom_left.color_at(center.0, center.1)
    );
    assert_eq!(lit.color_at(center.0, center.1).unwrap()[3], 32);
    assert_color_near(&overridden, center, [38, 22, 14, 255]);
    for frame in [&top_left, &bottom_left, &lit, &overridden] {
        assert_eq!(
            frame.stable_entity_id_at(center.0, center.1),
            Some(triangle)
        );
        assert_eq!(frame.stable_entity_id_at(0, 0), None);
        assert_eq!(frame.normal_at(0, 0), None);
        assert_eq!(frame.color_at(0, 0), Some([5, 8, 13, 255]));
    }
    assert_eq!(
        top_left.depth_at(center.0, center.1),
        bottom_left.depth_at(center.0, center.1)
    );
    assert_eq!(
        top_left.depth_at(center.0, center.1),
        lit.depth_at(center.0, center.1)
    );
    assert_eq!(
        top_left.depth_at(center.0, center.1),
        overridden.depth_at(center.0, center.1)
    );
    assert_eq!(
        top_left.normal_at(center.0, center.1),
        bottom_left.normal_at(center.0, center.1)
    );
    assert_eq!(
        top_left.normal_at(center.0, center.1),
        lit.normal_at(center.0, center.1)
    );
    assert_eq!(
        top_left.normal_at(center.0, center.1),
        overridden.normal_at(center.0, center.1)
    );
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn normal_texture_changes_direct_lighting_not_geometric_normal_observation() {
    let neutral = lit_normal_textured_frame(normal_texture_fixture([128, 128, 255, 0], 1.0), false);
    let tilted =
        lit_normal_textured_frame(normal_texture_fixture([255, 128, 128, 255], 1.0), false);
    let tilted_zero_alpha =
        lit_normal_textured_frame(normal_texture_fixture([255, 128, 128, 0], 1.0), false);
    let scaled_out =
        lit_normal_textured_frame(normal_texture_fixture([255, 128, 128, 7], 0.0), false);
    let maximum_scale =
        lit_normal_textured_frame(normal_texture_fixture([255, 128, 128, 11], f32::MAX), false);
    let overridden =
        lit_normal_textured_frame(normal_texture_fixture([255, 128, 128, 31], 1.0), true);
    let center = (WIDTH / 2, HEIGHT / 2);

    assert_ne!(
        neutral.color_at(center.0, center.1),
        tilted.color_at(center.0, center.1),
        "a source-tangent normal map must perturb direct-light response"
    );
    assert_eq!(
        neutral.color_at(center.0, center.1),
        scaled_out.color_at(center.0, center.1),
        "zero normal scale must suppress encoded XY perturbation"
    );
    assert_eq!(
        neutral.depth_at(center.0, center.1),
        tilted.depth_at(center.0, center.1)
    );
    assert_eq!(
        tilted.color_at(center.0, center.1),
        tilted_zero_alpha.color_at(center.0, center.1),
        "normal-texture alpha must be ignored"
    );
    assert_eq!(
        neutral.normal_at(center.0, center.1),
        tilted.normal_at(center.0, center.1),
        "normal observation remains the geometric transformed normal"
    );
    assert_eq!(
        neutral.normal_at(center.0, center.1),
        scaled_out.normal_at(center.0, center.1)
    );
    assert!(maximum_scale.color_at(center.0, center.1).is_some());
    assert_eq!(
        maximum_scale.stable_entity_id_at(center.0, center.1),
        Some(StableEntityId::new(2).unwrap()),
        "finite maximum scale must preserve a renderable normalized basis"
    );
    assert_eq!(
        neutral.normal_at(center.0, center.1),
        maximum_scale.normal_at(center.0, center.1)
    );
    assert_eq!(
        overridden.color_at(center.0, center.1),
        scaled_out.color_at(center.0, center.1),
        "a scene material override must disable the imported normal role"
    );
    assert_eq!(
        overridden.normal_at(center.0, center.1),
        neutral.normal_at(center.0, center.1)
    );
    assert_eq!(neutral.stable_entity_id_at(0, 0), None);
    assert_eq!(tilted.normal_at(0, 0), None);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn content_hash_eviction_cancels_partial_uploads_and_preserves_submitted_work() {
    let bytes = textured_two_mesh_fixture();
    let content_hash = content_hash(&bytes);
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    for mesh_index in 0..2 {
        renderer
            .enqueue_asset_upload(
                assets
                    .upload_job(AssetMeshKey {
                        content_hash,
                        mesh_index,
                    })
                    .unwrap(),
            )
            .unwrap();
    }
    let uploaded = renderer.process_next_asset_upload().unwrap();
    assert!(uploaded.texture_uploaded);
    assert_eq!(renderer.asset_stats().pending_uploads, 1);
    assert_eq!(renderer.asset_stats().resident_meshes, 1);
    assert_eq!(renderer.asset_stats().resident_textures, 1);

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &scene_patch_with_cuboid_fallback(camera, triangle, content_hash),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let pending_frame = renderer.submit_scene(camera).unwrap();

    let eviction = renderer.evict_asset(content_hash);
    assert_eq!(eviction.removed_pending_uploads, 1);
    assert_eq!(eviction.released_pending_bytes, 144);
    assert_eq!(eviction.removed_resident_meshes, 1);
    assert_eq!(eviction.released_resident_bytes, 144);
    assert_eq!(eviction.removed_pending_textures, 0);
    assert_eq!(eviction.released_pending_texture_bytes, 0);
    assert_eq!(eviction.removed_resident_textures, 1);
    assert_eq!(eviction.released_resident_texture_bytes, 16);
    assert!(renderer.evict_asset(content_hash).is_already_absent());
    let empty = renderer.asset_stats();
    assert_eq!(empty.pending_uploads, 0);
    assert_eq!(empty.oldest_pending_upload_age_micros, None);
    assert_eq!(empty.pending_bytes, 0);
    assert_eq!(empty.resident_meshes, 0);
    assert_eq!(empty.resident_bytes, 0);
    assert_eq!(empty.pending_textures, 0);
    assert_eq!(empty.pending_texture_bytes, 0);
    assert_eq!(empty.resident_textures, 0);
    assert_eq!(empty.resident_texture_bytes, 0);

    let submitted = pending_frame.read().unwrap();
    assert_eq!(
        submitted.stable_entity_id_at(WIDTH / 2, HEIGHT / 2),
        Some(triangle)
    );
    let fallback = renderer.submit_scene(camera).unwrap().read().unwrap();
    assert_eq!(
        fallback.stable_entity_id_at(WIDTH / 2, HEIGHT / 2),
        Some(triangle)
    );
    assert_ne!(
        fallback.color_at(WIDTH / 2, HEIGHT / 2),
        submitted.color_at(WIDTH / 2, HEIGHT / 2)
    );

    upload_textured_meshes(&mut renderer, &assets, content_hash);
    let rehydrated = renderer.submit_scene(camera).unwrap().read().unwrap();
    assert_eq!(
        rehydrated.stable_entity_id_at(WIDTH / 2, HEIGHT / 2),
        Some(triangle)
    );
    assert_eq!(
        rehydrated.color_at(WIDTH / 2, HEIGHT / 2),
        submitted.color_at(WIDTH / 2, HEIGHT / 2)
    );
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

fn imported_frame(bytes: Vec<u8>) -> RenderedFrame {
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer
        .enqueue_asset_upload(assets.upload_job(key).unwrap())
        .unwrap();
    renderer.process_next_asset_upload().unwrap();

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let mut world = AuthoritativeWorld::default();
    world
        .apply_patch(
            &scene_patch(camera, triangle, content_hash, [1.0; 3]),
            FrameId::new(1).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    renderer.submit_scene(camera).unwrap().read().unwrap()
}

fn lit_normal_textured_frame(bytes: Vec<u8>, override_material: bool) -> RenderedFrame {
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let upload = assets.upload_job(key).unwrap();
    assert!(upload.material().has_base_color_texture());
    assert_eq!(upload.base_color_texture().unwrap().byte_len(), 4);
    assert!(upload.material().has_normal_texture());
    assert_eq!(upload.normal_texture().unwrap().byte_len(), 4);
    let rehydration_upload = upload.clone();

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.enqueue_asset_upload(upload).unwrap();
    assert_eq!(renderer.asset_stats().pending_textures, 2);
    let uploaded = renderer.process_next_asset_upload().unwrap();
    assert!(uploaded.texture_uploaded);
    assert_eq!(uploaded.texture_byte_len, 8);
    assert_eq!(renderer.asset_stats().resident_textures, 2);
    let eviction = renderer.evict_asset(content_hash);
    assert_eq!(eviction.removed_resident_meshes, 1);
    assert_eq!(eviction.removed_resident_textures, 2);
    assert_eq!(eviction.released_resident_texture_bytes, 8);
    renderer.enqueue_asset_upload(rehydration_upload).unwrap();
    let rehydrated = renderer.process_next_asset_upload().unwrap();
    assert!(rehydrated.texture_uploaded);
    assert_eq!(rehydrated.texture_byte_len, 8);
    assert_eq!(renderer.asset_stats().resident_textures, 2);

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let light = StableEntityId::new(3).unwrap();
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
    world
        .apply_patch(
            &add_directional_light_patch(SceneRevision::new(1), light),
            FrameId::new(2).unwrap(),
        )
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    if override_material {
        world
            .apply_patch(
                &override_material_patch(SceneRevision::new(2), triangle),
                FrameId::new(3).unwrap(),
            )
            .unwrap();
        renderer
            .apply_extraction(&world.take_render_extraction().unwrap())
            .unwrap();
    }
    renderer.submit_scene(camera).unwrap().read().unwrap()
}

fn upload_textured_meshes(
    renderer: &mut HeadlessRenderer,
    assets: &AssetStore,
    content_hash: cogniform_protocol::ContentHash,
) {
    for mesh_index in 0..2 {
        let key = AssetMeshKey {
            content_hash,
            mesh_index,
        };
        assert_eq!(
            renderer
                .enqueue_asset_upload(assets.upload_job(key).unwrap())
                .unwrap(),
            AssetUploadAdmission::Queued { key }
        );
    }
    assert_eq!(renderer.asset_stats().pending_textures, 1);
    assert_eq!(renderer.asset_stats().pending_texture_bytes, 16);
    let first = renderer.process_next_asset_upload().unwrap();
    assert!(first.texture_uploaded);
    assert_eq!(first.texture_byte_len, 16);
    let second = renderer.process_next_asset_upload().unwrap();
    assert!(!second.texture_uploaded);
    assert_eq!(second.texture_byte_len, 0);
    assert_eq!(renderer.asset_stats().resident_meshes, 2);
    assert_eq!(renderer.asset_stats().resident_textures, 1);
    assert_eq!(renderer.asset_stats().resident_texture_bytes, 16);
}

fn assert_color_near(frame: &RenderedFrame, at: (u32, u32), expected_color: [u8; 4]) {
    let actual_color = frame.color_at(at.0, at.1).unwrap();
    for (actual, expected) in actual_color.into_iter().zip(expected_color) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "color {actual_color:?} differs from {expected_color:?}"
        );
    }
}

fn add_directional_light_patch(base_revision: SceneRevision, light: StableEntityId) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(4).unwrap(),
        idempotency_key: IdempotencyKey::new(5).unwrap(),
        base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::Create(CreateEntity {
            entity_id: light,
            components: vec![
                ComponentValue::LocalTransform(transform(0.0, [1.0; 3])),
                ComponentValue::Light(LightComponent {
                    kind: LightKind::Directional,
                    color: ColorRgb {
                        r: unit(1.0),
                        g: unit(1.0),
                        b: unit(1.0),
                    },
                    intensity: NonNegativeF32::new(0.5).unwrap(),
                }),
            ],
        })],
    }
}

fn override_material_patch(base_revision: SceneRevision, triangle: StableEntityId) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(6).unwrap(),
        idempotency_key: IdempotencyKey::new(7).unwrap(),
        base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::SetComponent(SetComponent {
            entity_id: triangle,
            component: ComponentValue::Material(MaterialComponent {
                base_color: ColorRgba {
                    r: unit(0.8),
                    g: unit(0.4),
                    b: unit(0.2),
                    a: unit(1.0),
                },
                metallic: unit(0.0),
                roughness: unit(0.5),
            }),
        })],
    }
}

fn set_asset_mesh_patch(
    base_revision: SceneRevision,
    triangle: StableEntityId,
    content_hash: cogniform_protocol::ContentHash,
    mesh_index: u32,
) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(8).unwrap(),
        idempotency_key: IdempotencyKey::new(9).unwrap(),
        base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::SetComponent(SetComponent {
            entity_id: triangle,
            component: ComponentValue::AssetMesh(AssetMeshComponent {
                content_hash,
                mesh_index,
            }),
        })],
    }
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

fn scene_patch_with_cuboid_fallback(
    camera: StableEntityId,
    triangle: StableEntityId,
    content_hash: cogniform_protocol::ContentHash,
) -> ScenePatch {
    let mut patch = scene_patch(camera, triangle, content_hash, [1.0; 3]);
    let SceneOperation::Create(entity) = &mut patch.operations[1] else {
        unreachable!("fixture entity creation remains the second operation")
    };
    entity
        .components
        .push(ComponentValue::Primitive(PrimitiveComponent {
            shape: PrimitiveShape::Cuboid,
            dimensions: PositiveVec3 {
                x: positive(1.0),
                y: positive(1.0),
                z: positive(1.0),
            },
        }));
    patch
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

fn metallic_fixture() -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ];
    let mut binary = Vec::with_capacity(36);
    for vertex in positions {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.4,0.2,1.0],"metallicFactor":1.0,"roughnessFactor":0.5}}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0,"mode":4}]}]}"#;
    glb_with_json(json, &binary)
}

fn primary_uv_fixture() -> Vec<u8> {
    let positions = [[-0.75_f32, -0.5, 0.0], [0.75, -0.5, 0.0], [0.0, 0.75, 0.0]];
    let texcoords = [[-0.25_f32, 1.25], [2.0, -3.0], [0.5, 0.75]];
    let mut binary = Vec::with_capacity(60);
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in texcoords {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":60}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36},{"buffer":0,"byteOffset":36,"byteLength":24}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"},{"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC2"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.6,0.9,1.0],"metallicFactor":0.0,"roughnessFactor":0.8}}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"TEXCOORD_0":1},"material":0,"mode":4}]}]}"#;
    glb_with_json(json, &binary)
}

fn textured_two_mesh_fixture() -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ];
    let mut binary = Vec::new();
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.25_f32, 0.25]; 3]
        .into_iter()
        .chain([[0.25_f32, 0.75]; 3])
    {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let png = encode_png(
        2,
        2,
        &[
            128, 64, 32, 128, 255, 0, 255, 255, 32, 128, 255, 64, 0, 0, 0, 255,
        ],
    );
    let image_offset = binary.len();
    binary.extend_from_slice(&png);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":60,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC2"}},{{"bufferView":2,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.5,0.25,1.0,0.5],"metallicFactor":0.0,"roughnessFactor":0.8,"baseColorTexture":{{"index":0}}}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":3,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0,"mode":4}}]}},{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":2}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_length = png.len(),
    );
    glb_with_json(&json, &binary)
}

fn normal_texture_fixture(texel: [u8; 4], scale: f32) -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ];
    let mut binary = Vec::new();
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for normal in [[0.0_f32, 0.0, 1.0]; 3] {
        for value in normal {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for tangent in [[1.0_f32, 0.0, 0.0, 1.0]; 3] {
        for value in tangent {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.5_f32, 0.5]; 3] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let base_png = encode_png(1, 1, &[255, 255, 255, 255]);
    let normal_png = encode_png(1, 1, &texel);
    let base_image_offset = binary.len();
    binary.extend_from_slice(&base_png);
    let normal_image_offset = binary.len();
    binary.extend_from_slice(&normal_png);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{base_image_offset},"byteLength":{base_image_length}}},{{"buffer":0,"byteOffset":{normal_image_offset},"byteLength":{normal_image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.8,0.4,0.2,1.0],"metallicFactor":0.0,"roughnessFactor":0.5,"baseColorTexture":{{"index":0}}}},"normalTexture":{{"index":1,"scale":{scale}}}}}],"textures":[{{"source":0}},{{"source":1}}],"images":[{{"bufferView":4,"mimeType":"image/png"}},{{"bufferView":5,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        base_image_length = base_png.len(),
        normal_image_length = normal_png.len(),
    );
    glb_with_json(&json, &binary)
}

fn encode_png(width: u32, height: u32, pixels: &[u8]) -> Vec<u8> {
    let mut png_bytes = Vec::new();
    {
        let mut png_encoder = png::Encoder::new(&mut png_bytes, width, height);
        png_encoder.set_color(png::ColorType::Rgba);
        png_encoder.set_depth(png::BitDepth::Eight);
        let mut writer = png_encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    png_bytes
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

fn unit(value: f32) -> UnitF32 {
    UnitF32::new(value).unwrap()
}
