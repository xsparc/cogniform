//! Controlled adapter contract for verified GLB decode, upload, and rendering.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use cogniform_assets::{
    ASSET_VERTEX_BYTES, AssetMeshKey, AssetShadingModel, AssetState, AssetStore, AssetVertex,
    content_hash,
};
use cogniform_protocol::{
    ApplyStatus, AssetMeshComponent, CameraComponent, ColorRgb, ColorRgba, ComponentValue,
    ConflictPolicy, CreateEntity, DeliverySemantic, FiniteF32, FrameId, IdempotencyKey,
    LightComponent, LightKind, LocalTransform, MaterialComponent, NonNegativeF32, PatchBudget,
    PositiveF32, PositiveVec3, PrimitiveComponent, PrimitiveShape, Quaternion, SceneOperation,
    ScenePatch, SceneRevision, SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32,
    Vec3,
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
fn double_sided_draws_switch_pipelines_without_reordering_or_causality_changes() {
    let bytes = mixed_double_sided_fixture();
    let content_hash = content_hash(&bytes);
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let uploads: Vec<_> = (0..3)
        .map(|mesh_index| {
            assets
                .upload_job(AssetMeshKey {
                    content_hash,
                    mesh_index,
                })
                .unwrap()
        })
        .collect();
    assert!(!uploads[0].material().double_sided());
    assert!(uploads[1].material().double_sided());
    assert!(!uploads[2].material().double_sided());

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    for upload in uploads.clone() {
        renderer.enqueue_asset_upload(upload).unwrap();
        renderer.process_next_asset_upload().unwrap();
    }

    let camera = StableEntityId::new(1).unwrap();
    let front = StableEntityId::new(2).unwrap();
    let double_back = StableEntityId::new(3).unwrap();
    let single_back = StableEntityId::new(4).unwrap();
    let light = StableEntityId::new(5).unwrap();
    let mut world = AuthoritativeWorld::default();
    let initial_patch =
        mixed_double_sided_scene_patch(camera, [front, double_back, single_back], content_hash);
    world
        .apply_patch(&initial_patch, FrameId::new(1).unwrap())
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

    let revision_before_eviction = world.revision();
    let hash_before_eviction = world.logical_hash().unwrap();
    let eviction = renderer.evict_asset(content_hash);
    assert_eq!(eviction.removed_resident_meshes, 3);
    assert_eq!(world.revision(), revision_before_eviction);
    assert_eq!(world.logical_hash().unwrap(), hash_before_eviction);
    for upload in uploads {
        renderer.enqueue_asset_upload(upload).unwrap();
        renderer.process_next_asset_upload().unwrap();
    }
    let replay = world
        .apply_patch(&initial_patch, FrameId::new(3).unwrap())
        .unwrap();
    assert_eq!(replay.status, ApplyStatus::IdempotentReplay);
    assert_eq!(world.revision(), revision_before_eviction);
    assert_eq!(world.logical_hash().unwrap(), hash_before_eviction);

    let frame = renderer.submit_scene(camera).unwrap().read().unwrap();
    assert!(visible_pixel_count(&frame, front) > 0);
    assert!(visible_pixel_count(&frame, double_back) > 0);
    assert_eq!(visible_pixel_count(&frame, single_back), 0);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            if frame.stable_entity_id_at(x, y) == Some(double_back) {
                let normal = frame.normal_at(x, y).unwrap();
                assert!(
                    normal[2] >= 0.99,
                    "double-sided back-face normal must face +Z: {normal:?}"
                );
            }
        }
    }
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
fn unlit_base_texture_is_exact_across_lights_and_scene_override_restores_lighting() {
    let bytes = unlit_four_role_texture_fixture();
    let hash = content_hash(&bytes);
    let mut assets = AssetStore::default();
    assets.enqueue(hash, bytes.clone()).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(assets.record(hash).unwrap().decoded_bytes, 208);
    let upload = assets
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.material().shading_model(), AssetShadingModel::Unlit);
    assert!(upload.base_color_texture().is_some());
    assert!(upload.emissive_texture().is_some());
    assert!(upload.metallic_roughness_texture().is_some());
    assert!(upload.normal_texture().is_some());

    let no_light = material_frame(bytes.clone(), None, false);
    let directional = material_frame(bytes.clone(), Some(LightKind::Directional), false);
    let point = material_frame(bytes.clone(), Some(LightKind::Point), false);
    let combined = material_frame_with_combined_lights(bytes.clone());
    let overridden = material_frame(bytes, Some(LightKind::Directional), true);
    let center = (WIDTH / 2, HEIGHT / 2);
    assert_color_near(&no_light, center, [28, 3, 3, 255]);
    for frame in [&directional, &point, &combined] {
        for y in 0..HEIGHT {
            for x in 0..WIDTH {
                assert_eq!(frame.color_at(x, y), no_light.color_at(x, y));
            }
        }
        assert_non_color_observations_equal(frame, &no_light);
    }
    assert_color_near(&overridden, center, [38, 22, 14, 255]);
    assert_ne!(
        overridden.color_at(center.0, center.1),
        no_light.color_at(center.0, center.1)
    );
    assert_non_color_observations_equal(&overridden, &no_light);
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
    assert_color_near(&top_left, center, [28, 3, 4, 255]);
    assert_color_near(&bottom_left, center, [2, 14, 255, 255]);
    assert_ne!(
        lit.color_at(center.0, center.1),
        bottom_left.color_at(center.0, center.1)
    );
    assert_eq!(lit.color_at(center.0, center.1).unwrap()[3], 255);
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
fn core_sampler_wrap_and_magnification_modes_are_pixel_observable() {
    let default_pixels = [0, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255];
    let render_default = |sampler_fields| {
        material_frame(
            sampled_unlit_base_fixture(2, 2, &default_pixels, [[0.6, 0.6]; 3], sampler_fields),
            None,
            false,
        )
    };
    let omitted_default = render_default(None);
    for explicit_default in [
        render_default(Some("")),
        render_default(Some(
            r#""magFilter":9729,"minFilter":9729,"wrapS":10497,"wrapT":10497"#,
        )),
    ] {
        assert_frames_equal(&explicit_default, &omitted_default);
    }

    let mut wrap_pixels = Vec::with_capacity(4 * 4 * 4);
    for green in [64_u8, 128, 255, 0] {
        for red in [0_u8, 64, 128, 255] {
            wrap_pixels.extend_from_slice(&[red, green, 0, 255]);
        }
    }
    let center = (WIDTH / 2, HEIGHT / 2);
    for (wrap_s, column) in [(10497, 1_usize), (33648, 2), (33071, 3)] {
        for (wrap_t, row) in [(10497, 2_usize), (33648, 1), (33071, 0)] {
            let bytes = sampled_unlit_base_fixture(
                4,
                4,
                &wrap_pixels,
                [[1.375, -0.375]; 3],
                Some(&format!(
                    r#""magFilter":9728,"minFilter":9728,"wrapS":{wrap_s},"wrapT":{wrap_t}"#
                )),
            );
            let frame = material_frame(bytes, None, false);
            assert_color_near(
                &frame,
                center,
                [
                    [0_u8, 13, 55, 255][column],
                    [13_u8, 55, 255, 0][row],
                    0,
                    255,
                ],
            );
        }
    }

    let magnification_pixels = [0, 0, 0, 255, 255, 0, 0, 255, 0, 255, 0, 255, 0, 0, 255, 255];
    let nearest = material_frame(
        sampled_unlit_base_fixture(
            2,
            2,
            &magnification_pixels,
            [[0.6, 0.6]; 3],
            Some(r#""magFilter":9728,"minFilter":9728"#),
        ),
        None,
        false,
    );
    let linear = material_frame(
        sampled_unlit_base_fixture(
            2,
            2,
            &magnification_pixels,
            [[0.6, 0.6]; 3],
            Some(r#""magFilter":9729,"minFilter":9728"#),
        ),
        None,
        false,
    );
    assert_color_near(&nearest, center, [0, 0, 255, 255]);
    assert_color_near(&linear, center, [54, 54, 125, 255]);
    assert_non_color_observations_equal(&nearest, &linear);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn mipmapped_minification_modes_use_the_documented_one_mip_fallback() {
    let mut checker = Vec::with_capacity(64 * 64 * 4);
    for row in 0..64 {
        for column in 0..64 {
            let value = if (row + column) % 2 == 0 { 0 } else { 255 };
            checker.extend_from_slice(&[value, value, value, 255]);
        }
    }
    let render = |min_filter| {
        material_frame(
            sampled_unlit_base_fixture(
                64,
                64,
                &checker,
                [[0.0, 0.0], [16.0, 0.0], [0.0, 16.0]],
                Some(&format!(r#""magFilter":9728,"minFilter":{min_filter}"#)),
            ),
            None,
            false,
        )
    };
    let nearest = render(9728);
    for frame in [render(9984), render(9986)] {
        assert_frames_equal(&frame, &nearest);
    }
    let linear = render(9729);
    for frame in [render(9985), render(9987)] {
        assert_frames_equal(&frame, &linear);
    }
    assert!(
        (0..HEIGHT).any(|y| (0..WIDTH).any(|x| nearest.color_at(x, y) != linear.color_at(x, y))),
        "nearest and linear minification families must produce different frames"
    );
    assert_non_color_observations_equal(&nearest, &linear);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn four_texture_roles_bind_independent_samplers_for_one_shared_image() {
    let shared = material_frame(
        four_role_sampler_fixture(true, false),
        Some(LightKind::Directional),
        false,
    );
    let one_texel_references = material_frame(
        four_role_sampler_fixture(false, false),
        Some(LightKind::Directional),
        false,
    );
    assert_frames_equal(&shared, &one_texel_references);
    assert_eq!(
        shared.stable_entity_id_at(WIDTH / 2, HEIGHT / 2),
        Some(StableEntityId::new(2).unwrap())
    );

    let shared_sampler_and_image = material_frame(
        four_role_sampler_fixture(true, true),
        Some(LightKind::Directional),
        false,
    );
    let shared_sampler_distinct_images = material_frame(
        four_role_sampler_fixture(false, true),
        Some(LightKind::Directional),
        false,
    );
    assert_frames_equal(&shared_sampler_and_image, &shared_sampler_distinct_images);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn texture_transforms_apply_independently_to_all_four_roles() {
    let transformed = material_frame(
        transformed_four_role_fixture(true),
        Some(LightKind::Directional),
        false,
    );
    let one_texel_references = material_frame(
        transformed_four_role_fixture(false),
        Some(LightKind::Directional),
        false,
    );

    assert_frames_equal(&transformed, &one_texel_references);
    assert_eq!(
        transformed.stable_entity_id_at(WIDTH / 2, HEIGHT / 2),
        Some(StableEntityId::new(2).unwrap())
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
fn generated_tangent_normal_texture_matches_explicit_render_output() {
    let texel = [255, 128, 128, 255];
    let explicit = lit_normal_textured_frame(normal_texture_fixture(texel, 1.0), false);
    let generated = lit_normal_textured_frame(generated_normal_texture_fixture(texel, 1.0), false);

    assert_frames_equal(&explicit, &generated);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn double_sided_back_face_composes_with_normal_maps_and_scene_override() {
    let neutral = oriented_material_frame(
        double_sided_normal_texture_fixture([128, 128, 255, 0], 1.0),
        Some(LightKind::Directional),
        None,
        true,
    );
    let tilted = oriented_material_frame(
        double_sided_normal_texture_fixture([255, 128, 128, 255], 1.0),
        Some(LightKind::Directional),
        None,
        true,
    );
    let point_lit = oriented_material_frame(
        double_sided_normal_texture_fixture([128, 128, 255, 0], 1.0),
        Some(LightKind::Point),
        None,
        true,
    );
    let overridden = oriented_material_frame(
        normal_texture_fixture([255, 128, 128, 31], 1.0),
        Some(LightKind::Directional),
        Some(1.0),
        true,
    );
    let center = (WIDTH / 2, HEIGHT / 2);
    let triangle = StableEntityId::new(2).unwrap();

    assert_eq!(
        neutral.stable_entity_id_at(center.0, center.1),
        Some(triangle)
    );
    assert_eq!(
        tilted.stable_entity_id_at(center.0, center.1),
        Some(triangle)
    );
    assert_eq!(
        point_lit.stable_entity_id_at(center.0, center.1),
        Some(triangle)
    );
    assert_ne!(
        neutral.color_at(center.0, center.1),
        tilted.color_at(center.0, center.1),
        "the tilted tangent-space normal must change back-face direct lighting"
    );
    assert_eq!(
        neutral.depth_at(center.0, center.1),
        tilted.depth_at(center.0, center.1)
    );
    assert_eq!(
        neutral.normal_at(center.0, center.1),
        tilted.normal_at(center.0, center.1)
    );
    for frame in [&neutral, &tilted, &point_lit] {
        let normal = frame.normal_at(center.0, center.1).unwrap();
        assert!(
            normal[2] >= 0.99,
            "back-face geometric normal must face +Z: {normal:?}"
        );
    }
    assert_eq!(
        overridden.stable_entity_id_at(center.0, center.1),
        Some(triangle),
        "a scene material override must preserve legacy unculled rendering"
    );
    let overridden_normal = overridden.normal_at(center.0, center.1).unwrap();
    assert!(
        overridden_normal[2] <= -0.99,
        "scene override must disable imported face correction: {overridden_normal:?}"
    );
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn metallic_roughness_texture_multiplies_factors_for_direct_lights_only() {
    let texture = metallic_roughness_fixture(Some([11, 128, 64, 17]), 1.0, 0.5);
    let ignored_channels = metallic_roughness_fixture(Some([233, 128, 64, 251]), 1.0, 0.5);
    let neutral = metallic_roughness_fixture(Some([0, 255, 255, 0]), 1.0, 0.5);
    let multiplied_factors = metallic_roughness_fixture(
        None,
        f32::from(64_u8) / 255.0,
        0.5 * f32::from(128_u8) / 255.0,
    );

    let unlit_textured = material_frame(texture.clone(), None, false);
    let unlit_neutral = material_frame(neutral.clone(), None, false);
    assert_eq!(
        unlit_textured.color_at(WIDTH / 2, HEIGHT / 2),
        unlit_neutral.color_at(WIDTH / 2, HEIGHT / 2),
        "metallic-roughness values must not alter an unlit base color"
    );

    for light_kind in [LightKind::Directional, LightKind::Point] {
        let textured = material_frame(texture.clone(), Some(light_kind), false);
        let ignored = material_frame(ignored_channels.clone(), Some(light_kind), false);
        let scalar = material_frame(multiplied_factors.clone(), Some(light_kind), false);
        let neutral = material_frame(neutral.clone(), Some(light_kind), false);
        let center = (WIDTH / 2, HEIGHT / 2);
        assert_eq!(
            textured.color_at(center.0, center.1),
            ignored.color_at(center.0, center.1),
            "metallic-roughness red and alpha channels must be ignored"
        );
        assert_color_near(
            &textured,
            center,
            scalar.color_at(center.0, center.1).unwrap(),
        );
        assert_ne!(
            textured.color_at(center.0, center.1),
            neutral.color_at(center.0, center.1),
            "green roughness and blue metallic channels must alter direct response"
        );
        assert_non_color_observations_equal(&textured, &neutral);
    }

    let overridden = material_frame(texture, Some(LightKind::Directional), true);
    let overridden_neutral = material_frame(neutral, Some(LightKind::Directional), true);
    assert_eq!(
        overridden.color_at(WIDTH / 2, HEIGHT / 2),
        overridden_neutral.color_at(WIDTH / 2, HEIGHT / 2),
        "an explicit scene material must disable the imported metallic-roughness role"
    );
    assert_non_color_observations_equal(&overridden, &overridden_neutral);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn emissive_factor_adds_after_unlit_or_direct_response_and_preserves_other_outputs() {
    let baseline = emissive_fixture(None);
    let explicit_zero = emissive_fixture(Some([0.0; 3]));
    let emissive = emissive_fixture(Some([0.1, 0.2, 0.3]));
    let center = (WIDTH / 2, HEIGHT / 2);

    let unlit_baseline = material_frame(baseline.clone(), None, false);
    let unlit_explicit_zero = material_frame(explicit_zero, None, false);
    assert_eq!(
        unlit_explicit_zero.color_at(center.0, center.1),
        unlit_baseline.color_at(center.0, center.1)
    );
    assert_non_color_observations_equal(&unlit_explicit_zero, &unlit_baseline);
    let unlit = material_frame(emissive.clone(), None, false);
    assert_color_near(&unlit, center, [77, 77, 89, 255]);
    assert_emissive_addition(&unlit, &unlit_baseline, center, [26, 51, 77]);
    assert_non_color_observations_equal(&unlit, &unlit_baseline);

    for light_kind in [LightKind::Directional, LightKind::Point] {
        let lit_baseline = material_frame(baseline.clone(), Some(light_kind), false);
        let lit = material_frame(emissive.clone(), Some(light_kind), false);
        assert_emissive_addition(&lit, &lit_baseline, center, [26, 51, 77]);
        assert_non_color_observations_equal(&lit, &lit_baseline);
    }

    let overridden = material_frame(emissive, Some(LightKind::Directional), true);
    let overridden_baseline = material_frame(baseline, Some(LightKind::Directional), true);
    assert_eq!(
        overridden.color_at(center.0, center.1),
        overridden_baseline.color_at(center.0, center.1),
        "an explicit scene material must disable imported emission"
    );
    assert_non_color_observations_equal(&overridden, &overridden_baseline);

    let saturated = material_frame(emissive_fixture(Some([1.0; 3])), None, false);
    assert_color_near(&saturated, center, [255, 255, 255, 255]);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn emissive_texture_decodes_srgb_ignores_alpha_and_uses_white_fallback() {
    let center = (WIDTH / 2, HEIGHT / 2);
    let baseline = material_frame(emissive_texture_fixture(None, [0.0; 3]), None, false);
    let zero = material_frame(
        emissive_texture_fixture(Some([128, 64, 32, 0]), [0.0; 3]),
        None,
        false,
    );
    assert_eq!(
        zero.color_at(center.0, center.1),
        baseline.color_at(center.0, center.1)
    );
    assert_non_color_observations_equal(&zero, &baseline);

    let transparent = material_frame(
        emissive_texture_fixture(Some([128, 64, 32, 0]), [1.0; 3]),
        None,
        false,
    );
    let opaque = material_frame(
        emissive_texture_fixture(Some([128, 64, 32, 255]), [1.0; 3]),
        None,
        false,
    );
    assert_eq!(
        transparent.color_at(center.0, center.1),
        opaque.color_at(center.0, center.1)
    );
    assert_emissive_addition(&transparent, &baseline, center, [55, 13, 4]);
    assert_non_color_observations_equal(&transparent, &baseline);

    let multiplied = material_frame(
        emissive_texture_fixture(Some([128, 64, 32, 7]), [0.5, 0.25, 0.75]),
        None,
        false,
    );
    assert_emissive_addition(&multiplied, &baseline, center, [28, 3, 3]);
    assert_non_color_observations_equal(&multiplied, &baseline);

    let factor = [0.1, 0.2, 0.3];
    let omitted = material_frame(emissive_texture_fixture(None, factor), None, false);
    let white = material_frame(
        emissive_texture_fixture(Some([255; 4]), factor),
        None,
        false,
    );
    assert_eq!(
        white.color_at(center.0, center.1),
        omitted.color_at(center.0, center.1)
    );
    assert_non_color_observations_equal(&white, &omitted);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn emissive_texture_adds_after_direct_light_and_scene_override_disables_it() {
    let center = (WIDTH / 2, HEIGHT / 2);
    for light_kind in [LightKind::Directional, LightKind::Point] {
        let baseline = material_frame(
            emissive_texture_fixture(None, [0.0; 3]),
            Some(light_kind),
            false,
        );
        let textured = material_frame(
            emissive_texture_fixture(Some([128, 64, 32, 7]), [1.0; 3]),
            Some(light_kind),
            false,
        );
        assert_emissive_addition(&textured, &baseline, center, [55, 13, 4]);
        assert_non_color_observations_equal(&textured, &baseline);
    }

    let textured = material_frame(
        emissive_texture_fixture(Some([128, 64, 32, 7]), [1.0; 3]),
        Some(LightKind::Directional),
        true,
    );
    let baseline = material_frame(
        emissive_texture_fixture(None, [0.0; 3]),
        Some(LightKind::Directional),
        true,
    );
    assert_eq!(
        textured.color_at(center.0, center.1),
        baseline.color_at(center.0, center.1)
    );
    assert_non_color_observations_equal(&textured, &baseline);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn vertex_colors_interpolate_and_preserve_non_color_observations() {
    let gradient = material_frame(
        vertex_color_fixture(VertexColorFixture::gradient()),
        None,
        false,
    );
    let white = material_frame(
        vertex_color_fixture(VertexColorFixture::white()),
        None,
        false,
    );
    let triangle = StableEntityId::new(2).unwrap();
    let visible_colors = (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| gradient.stable_entity_id_at(x, y) == Some(triangle))
        .map(|(x, y)| gradient.color_at(x, y).unwrap())
        .collect::<Vec<_>>();

    assert!(
        visible_colors
            .iter()
            .any(|color| color[0] > color[1] + 40 && color[0] > color[2] + 40)
    );
    assert!(
        visible_colors
            .iter()
            .any(|color| color[1] > color[0] + 40 && color[1] > color[2] + 40)
    );
    assert!(
        visible_colors
            .iter()
            .any(|color| color[2] > color[0] + 40 && color[2] > color[1] + 40)
    );
    let center = gradient.color_at(WIDTH / 2, HEIGHT / 2).unwrap();
    assert!(
        center[..3]
            .iter()
            .all(|channel| (32..=192).contains(channel))
    );
    assert_eq!(center[3], 255);
    assert_eq!(white.color_at(WIDTH / 2, HEIGHT / 2), Some([255; 4]));
    assert_non_color_observations_equal(&gradient, &white);

    let lit_spec = VertexColorFixture {
        colors: Some([[0.25, 0.5, 0.75, 1.0]; 3]),
        unlit: false,
        ..VertexColorFixture::white()
    };
    let lit_color = material_frame(
        vertex_color_fixture(lit_spec),
        Some(LightKind::Directional),
        false,
    );
    let lit_white = material_frame(
        vertex_color_fixture(VertexColorFixture {
            unlit: false,
            ..VertexColorFixture::white()
        }),
        Some(LightKind::Directional),
        false,
    );
    let colored = lit_color.color_at(WIDTH / 2, HEIGHT / 2).unwrap();
    let plain = lit_white.color_at(WIDTH / 2, HEIGHT / 2).unwrap();
    assert_color_near(&lit_color, (WIDTH / 2, HEIGHT / 2), [10, 20, 30, 255]);
    assert_color_near(&lit_white, (WIDTH / 2, HEIGHT / 2), [40, 40, 40, 255]);
    assert!(colored[0] < colored[1] && colored[1] < colored[2]);
    assert!(
        colored[..3]
            .iter()
            .zip(plain)
            .all(|(color, white)| color < &white)
    );
    assert_non_color_observations_equal(&lit_color, &lit_white);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn vertex_color_multiplies_factor_texture_and_scene_override() {
    let composed_spec = VertexColorFixture {
        colors: Some([[0.5, 0.25, 0.75, 0.5]; 3]),
        factor: [0.8, 0.4, 0.2, 0.75],
        texture: Some([128, 64, 255, 128]),
        ..VertexColorFixture::white()
    };
    let composed = material_frame(vertex_color_fixture(composed_spec), None, false);
    assert_color_near(&composed, (WIDTH / 2, HEIGHT / 2), [22, 1, 38, 255]);

    let colored_override = material_frame(vertex_color_fixture(composed_spec), None, true);
    let plain_override = material_frame(
        vertex_color_fixture(VertexColorFixture::white()),
        None,
        true,
    );
    assert_frames_equal(&colored_override, &plain_override);

    let emissive_spec = VertexColorFixture {
        colors: Some([[0.1, 0.2, 0.3, 0.4]; 3]),
        factor: [0.0, 0.0, 0.0, 1.0],
        emissive: [0.1, 0.2, 0.3],
        unlit: false,
        ..VertexColorFixture::white()
    };
    let emissive = material_frame(vertex_color_fixture(emissive_spec), None, false);
    assert_color_near(&emissive, (WIDTH / 2, HEIGHT / 2), [26, 51, 77, 255]);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn vertex_color_alpha_default_material_and_double_sided_back_face_are_exact() {
    let masked = |alpha, mode, cutoff, double_sided| VertexColorFixture {
        colors: Some([[0.8, 0.4, 0.2, alpha]; 3]),
        alpha_mode: mode,
        alpha_cutoff: cutoff,
        double_sided,
        ..VertexColorFixture::white()
    };
    let below = material_frame(
        vertex_color_fixture(masked(0.49, "MASK", Some(0.5), false)),
        None,
        false,
    );
    let equal = oriented_material_frame(
        vertex_color_fixture(masked(0.5, "MASK", Some(0.5), true)),
        None,
        None,
        true,
    );
    let opaque = material_frame(
        vertex_color_fixture(masked(0.0, "OPAQUE", None, false)),
        None,
        false,
    );
    let no_material = material_frame(
        vertex_color_fixture(VertexColorFixture {
            colors: Some([[0.25, 0.5, 0.75, 0.0]; 3]),
            include_material: false,
            ..VertexColorFixture::white()
        }),
        None,
        false,
    );

    assert_fully_discarded(&below);
    assert_opaque_center(&equal, 255);
    assert!(equal.normal_at(WIDTH / 2, HEIGHT / 2).unwrap()[2] > 0.99);
    assert_opaque_center(&opaque, 255);
    assert_color_near(&no_material, (WIDTH / 2, HEIGHT / 2), [51, 102, 153, 255]);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn alpha_mask_factor_boundaries_control_every_fragment_output() {
    let below = alpha_material_frame(alpha_fixture("MASK", 0.25, None, Some(0.5)), None);
    let equal = alpha_material_frame(alpha_fixture("MASK", 0.5, None, Some(0.5)), None);
    let above_one = alpha_material_frame(alpha_fixture("MASK", 1.0, None, Some(1.25)), None);

    assert_fully_discarded(&below);
    assert_fully_discarded(&above_one);
    assert_opaque_center(&equal, 255);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn double_sided_back_face_preserves_mask_discard_and_equality() {
    let below = oriented_material_frame(
        double_sided_alpha_fixture("MASK", 0.25, None, Some(0.5)),
        None,
        None,
        true,
    );
    let equal = oriented_material_frame(
        double_sided_alpha_fixture("MASK", 0.5, None, Some(0.5)),
        None,
        None,
        true,
    );

    assert_fully_discarded(&below);
    assert_opaque_center(&equal, 255);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn unlit_double_sided_back_face_preserves_opaque_and_mask_coverage() {
    let below = oriented_material_frame(
        unlit_double_sided_alpha_fixture("MASK", 0.25, None, Some(0.5)),
        None,
        None,
        true,
    );
    let equality = oriented_material_frame(
        unlit_double_sided_alpha_fixture("MASK", 0.5, None, Some(0.5)),
        None,
        None,
        true,
    );
    let opaque = oriented_material_frame(
        unlit_double_sided_alpha_fixture("OPAQUE", 0.0, None, None),
        Some(LightKind::Point),
        None,
        true,
    );
    assert_fully_discarded(&below);
    assert_opaque_center(&equality, 255);
    assert_opaque_center(&opaque, 255);
    let center = (WIDTH / 2, HEIGHT / 2);
    let equality_normal = equality.normal_at(center.0, center.1).unwrap();
    let opaque_normal = opaque.normal_at(center.0, center.1).unwrap();
    assert!(equality_normal[2] > 0.99);
    assert!(opaque_normal[2] > 0.99);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn alpha_texture_product_opaque_mode_and_scene_override_are_exact() {
    let texture_only = alpha_material_frame(alpha_fixture("MASK", 1.0, Some(64), Some(0.3)), None);
    let multiplied = alpha_material_frame(alpha_fixture("MASK", 0.5, Some(128), Some(0.3)), None);
    let opaque = alpha_material_frame(alpha_fixture("OPAQUE", 0.0, Some(0), None), None);
    let overridden =
        alpha_material_frame(alpha_fixture("MASK", 0.0, Some(0), Some(1.0)), Some(0.25));

    assert_fully_discarded(&texture_only);
    assert_fully_discarded(&multiplied);
    assert_opaque_center(&opaque, 255);
    assert_opaque_center(&overridden, 64);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn four_texture_roles_upload_evict_and_rehydrate_exactly() {
    let bytes = four_role_texture_fixture();
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(assets.record(content_hash).unwrap().decoded_bytes, 208);
    let upload = assets.upload_job(key).unwrap();
    assert!(upload.base_color_texture().is_some());
    assert!(upload.emissive_texture().is_some());
    assert!(upload.metallic_roughness_texture().is_some());
    assert!(upload.normal_texture().is_some());

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.enqueue_asset_upload(upload.clone()).unwrap();
    assert_eq!(renderer.asset_stats().pending_textures, 4);
    assert_eq!(renderer.asset_stats().pending_texture_bytes, 16);
    let uploaded = renderer.process_next_asset_upload().unwrap();
    assert_eq!(uploaded.texture_byte_len, 16);
    assert_eq!(renderer.asset_stats().resident_textures, 4);
    assert_eq!(renderer.asset_stats().resident_texture_bytes, 16);
    let eviction = renderer.evict_asset(content_hash);
    assert_eq!(eviction.removed_resident_textures, 4);
    assert_eq!(eviction.released_resident_texture_bytes, 16);
    renderer.enqueue_asset_upload(upload).unwrap();
    assert_eq!(
        renderer
            .process_next_asset_upload()
            .unwrap()
            .texture_byte_len,
        16
    );
    assert_eq!(renderer.asset_stats().resident_textures, 4);
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
    assert_eq!(eviction.released_pending_bytes, 192);
    assert_eq!(eviction.removed_resident_meshes, 1);
    assert_eq!(eviction.released_resident_bytes, 192);
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

fn material_frame(
    bytes: Vec<u8>,
    light_kind: Option<LightKind>,
    override_material: bool,
) -> RenderedFrame {
    material_frame_with_override_alpha(bytes, light_kind, override_material.then_some(1.0))
}

fn material_frame_with_combined_lights(bytes: Vec<u8>) -> RenderedFrame {
    oriented_material_frame_with_lights(bytes, FixtureLights::Combined, None, false)
}

fn alpha_material_frame(bytes: Vec<u8>, override_alpha: Option<f32>) -> RenderedFrame {
    material_frame_with_override_alpha(bytes, None, override_alpha)
}

fn material_frame_with_override_alpha(
    bytes: Vec<u8>,
    light_kind: Option<LightKind>,
    override_alpha: Option<f32>,
) -> RenderedFrame {
    oriented_material_frame(bytes, light_kind, override_alpha, false)
}

fn oriented_material_frame(
    bytes: Vec<u8>,
    light_kind: Option<LightKind>,
    override_alpha: Option<f32>,
    back_facing: bool,
) -> RenderedFrame {
    let lights = light_kind.map_or(FixtureLights::None, FixtureLights::One);
    oriented_material_frame_with_lights(bytes, lights, override_alpha, back_facing)
}

#[derive(Clone, Copy)]
enum FixtureLights {
    None,
    One(LightKind),
    Combined,
}

fn oriented_material_frame_with_lights(
    bytes: Vec<u8>,
    lights: FixtureLights,
    override_alpha: Option<f32>,
    back_facing: bool,
) -> RenderedFrame {
    let content_hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash,
        mesh_index: 0,
    };
    let mut assets = AssetStore::default();
    assets.enqueue(content_hash, bytes).unwrap();
    assert_eq!(assets.process_next().unwrap().state, AssetState::Ready);
    let upload = assets.upload_job(key).unwrap();
    let texture_count = [
        upload.base_color_texture(),
        upload.emissive_texture(),
        upload.metallic_roughness_texture(),
        upload.normal_texture(),
    ]
    .into_iter()
    .flatten()
    .count();
    let texture_count = u32::try_from(texture_count).unwrap();
    let texture_bytes = [
        upload.base_color_texture(),
        upload.emissive_texture(),
        upload.metallic_roughness_texture(),
        upload.normal_texture(),
    ]
    .into_iter()
    .flatten()
    .map(cogniform_assets::AssetTexture::byte_len)
    .sum::<u64>();
    let rehydration_upload = upload.clone();

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.enqueue_asset_upload(upload).unwrap();
    assert_eq!(renderer.asset_stats().pending_textures, texture_count);
    assert_eq!(renderer.asset_stats().pending_texture_bytes, texture_bytes);
    let uploaded = renderer.process_next_asset_upload().unwrap();
    assert_eq!(uploaded.texture_byte_len, texture_bytes);
    if texture_count > 0 {
        let eviction = renderer.evict_asset(content_hash);
        assert_eq!(eviction.removed_resident_textures, texture_count);
        assert_eq!(eviction.released_resident_texture_bytes, texture_bytes);
        renderer.enqueue_asset_upload(rehydration_upload).unwrap();
        assert_eq!(
            renderer
                .process_next_asset_upload()
                .unwrap()
                .texture_byte_len,
            texture_bytes
        );
    }

    let camera = StableEntityId::new(1).unwrap();
    let triangle = StableEntityId::new(2).unwrap();
    let light = StableEntityId::new(3).unwrap();
    let mut world = AuthoritativeWorld::default();
    let initial_patch = oriented_scene_patch(camera, triangle, content_hash, [1.0; 3], back_facing);
    world
        .apply_patch(&initial_patch, FrameId::new(1).unwrap())
        .unwrap();
    renderer
        .apply_extraction(&world.take_render_extraction().unwrap())
        .unwrap();
    let mut revision = SceneRevision::new(1);
    if let Some(light_patch) = fixture_light_patch(lights, revision, light) {
        world
            .apply_patch(&light_patch, FrameId::new(2).unwrap())
            .unwrap();
        renderer
            .apply_extraction(&world.take_render_extraction().unwrap())
            .unwrap();
        revision = SceneRevision::new(2);
    }
    if let Some(alpha) = override_alpha {
        world
            .apply_patch(
                &override_material_patch_with_alpha(revision, triangle, alpha),
                FrameId::new(3).unwrap(),
            )
            .unwrap();
        renderer
            .apply_extraction(&world.take_render_extraction().unwrap())
            .unwrap();
    }
    let revision_before_replay = world.revision();
    let hash_before_replay = world.logical_hash().unwrap();
    let replay = world
        .apply_patch(&initial_patch, FrameId::new(4).unwrap())
        .unwrap();
    assert_eq!(replay.status, ApplyStatus::IdempotentReplay);
    assert_eq!(world.revision(), revision_before_replay);
    assert_eq!(world.logical_hash().unwrap(), hash_before_replay);
    renderer.submit_scene(camera).unwrap().read().unwrap()
}

fn fixture_light_patch(
    lights: FixtureLights,
    base_revision: SceneRevision,
    light: StableEntityId,
) -> Option<ScenePatch> {
    match lights {
        FixtureLights::None => None,
        FixtureLights::One(kind) => Some(add_light_patch(base_revision, light, kind)),
        FixtureLights::Combined => Some(add_combined_lights_patch(
            base_revision,
            light,
            StableEntityId::new(4).unwrap(),
        )),
    }
}

fn assert_fully_discarded(frame: &RenderedFrame) {
    let background_color = frame.color_at(0, 0);
    let background_depth = frame.depth_at(0, 0);
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(frame.color_at(x, y), background_color);
            assert_eq!(frame.depth_at(x, y), background_depth);
            assert_eq!(frame.stable_entity_id_at(x, y), None);
            assert_eq!(frame.normal_at(x, y), None);
        }
    }
}

fn assert_opaque_center(frame: &RenderedFrame, expected_alpha: u8) {
    let center = (WIDTH / 2, HEIGHT / 2);
    assert_eq!(
        frame.stable_entity_id_at(center.0, center.1),
        Some(StableEntityId::new(2).unwrap())
    );
    assert!(frame.depth_at(center.0, center.1).unwrap() < 1.0);
    assert!(frame.normal_at(center.0, center.1).is_some());
    assert_eq!(
        frame.color_at(center.0, center.1).unwrap()[3],
        expected_alpha
    );
}

fn visible_pixel_count(frame: &RenderedFrame, entity_id: StableEntityId) -> usize {
    (0..HEIGHT)
        .flat_map(|y| (0..WIDTH).map(move |x| (x, y)))
        .filter(|&(x, y)| frame.stable_entity_id_at(x, y) == Some(entity_id))
        .count()
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

fn assert_emissive_addition(
    actual: &RenderedFrame,
    baseline: &RenderedFrame,
    at: (u32, u32),
    emissive_u8: [u8; 3],
) {
    let baseline_color = baseline.color_at(at.0, at.1).unwrap();
    let mut expected = baseline_color;
    for index in 0..3 {
        expected[index] = baseline_color[index].saturating_add(emissive_u8[index]);
    }
    assert_color_near(actual, at, expected);
    assert_eq!(
        actual.color_at(at.0, at.1).unwrap()[3],
        baseline_color[3],
        "emission must preserve material alpha"
    );
}

fn assert_non_color_observations_equal(actual: &RenderedFrame, expected: &RenderedFrame) {
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(actual.depth_at(x, y), expected.depth_at(x, y));
            assert_eq!(
                actual.stable_entity_id_at(x, y),
                expected.stable_entity_id_at(x, y)
            );
            assert_eq!(actual.normal_at(x, y), expected.normal_at(x, y));
            if actual.stable_entity_id_at(x, y).is_none() {
                assert_eq!(actual.color_at(x, y), expected.color_at(x, y));
            }
        }
    }
}

fn assert_frames_equal(actual: &RenderedFrame, expected: &RenderedFrame) {
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            assert_eq!(actual.color_at(x, y), expected.color_at(x, y));
            assert_eq!(actual.depth_at(x, y), expected.depth_at(x, y));
            assert_eq!(
                actual.stable_entity_id_at(x, y),
                expected.stable_entity_id_at(x, y)
            );
            assert_eq!(actual.normal_at(x, y), expected.normal_at(x, y));
        }
    }
}

fn add_directional_light_patch(base_revision: SceneRevision, light: StableEntityId) -> ScenePatch {
    add_light_patch(base_revision, light, LightKind::Directional)
}

fn add_light_patch(
    base_revision: SceneRevision,
    light: StableEntityId,
    kind: LightKind,
) -> ScenePatch {
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
                ComponentValue::LocalTransform(transform(
                    if kind == LightKind::Point { 2.0 } else { 0.0 },
                    [1.0; 3],
                )),
                ComponentValue::Light(LightComponent {
                    kind,
                    color: ColorRgb {
                        r: unit(1.0),
                        g: unit(1.0),
                        b: unit(1.0),
                    },
                    intensity: NonNegativeF32::new(if kind == LightKind::Point {
                        2.0
                    } else {
                        0.5
                    })
                    .unwrap(),
                }),
            ],
        })],
    }
}

fn add_combined_lights_patch(
    base_revision: SceneRevision,
    directional: StableEntityId,
    point: StableEntityId,
) -> ScenePatch {
    let mut patch = add_light_patch(base_revision, directional, LightKind::Directional);
    patch.transaction_id = TransactionId::new(40).unwrap();
    patch.idempotency_key = IdempotencyKey::new(41).unwrap();
    patch.operations.push(SceneOperation::Create(CreateEntity {
        entity_id: point,
        components: vec![
            ComponentValue::LocalTransform(transform(2.0, [1.0; 3])),
            ComponentValue::Light(LightComponent {
                kind: LightKind::Point,
                color: ColorRgb {
                    r: unit(1.0),
                    g: unit(1.0),
                    b: unit(1.0),
                },
                intensity: NonNegativeF32::new(2.0).unwrap(),
            }),
        ],
    }));
    patch
}

fn override_material_patch(base_revision: SceneRevision, triangle: StableEntityId) -> ScenePatch {
    override_material_patch_with_alpha(base_revision, triangle, 1.0)
}

fn override_material_patch_with_alpha(
    base_revision: SceneRevision,
    triangle: StableEntityId,
    alpha: f32,
) -> ScenePatch {
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
                    a: unit(alpha),
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
    oriented_scene_patch(camera, triangle, content_hash, triangle_scale, false)
}

fn oriented_scene_patch(
    camera: StableEntityId,
    triangle: StableEntityId,
    content_hash: cogniform_protocol::ContentHash,
    triangle_scale: [f32; 3],
    back_facing: bool,
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
                    ComponentValue::LocalTransform(oriented_transform(
                        0.0,
                        triangle_scale,
                        back_facing,
                    )),
                    ComponentValue::AssetMesh(AssetMeshComponent {
                        content_hash,
                        mesh_index: 0,
                    }),
                ],
            }),
        ],
    }
}

fn mixed_double_sided_scene_patch(
    camera: StableEntityId,
    triangles: [StableEntityId; 3],
    content_hash: cogniform_protocol::ContentHash,
) -> ScenePatch {
    let mut operations = vec![SceneOperation::Create(CreateEntity {
        entity_id: camera,
        components: vec![
            ComponentValue::LocalTransform(transform(3.0, [1.0; 3])),
            ComponentValue::Camera(CameraComponent {
                vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                near: positive(0.1),
                far: positive(100.0),
            }),
        ],
    })];
    for (mesh_index, (entity_id, (x, back_facing))) in triangles
        .into_iter()
        .zip([(-0.9, false), (0.0, true), (0.9, true)])
        .enumerate()
    {
        let mut local_transform = oriented_transform(0.0, [0.65; 3], back_facing);
        local_transform.translation.x = finite(x);
        operations.push(SceneOperation::Create(CreateEntity {
            entity_id,
            components: vec![
                ComponentValue::LocalTransform(local_transform),
                ComponentValue::AssetMesh(AssetMeshComponent {
                    content_hash,
                    mesh_index: u32::try_from(mesh_index).unwrap(),
                }),
            ],
        }));
    }
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(20).unwrap(),
        idempotency_key: IdempotencyKey::new(21).unwrap(),
        base_revision: SceneRevision::INITIAL,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations,
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
    oriented_transform(z, scale, false)
}

fn oriented_transform(z: f32, scale: [f32; 3], back_facing: bool) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(0.0),
            y: finite(0.0),
            z: finite(z),
        },
        rotation: Quaternion {
            x: finite(0.0),
            y: finite(if back_facing { 1.0 } else { 0.0 }),
            z: finite(0.0),
            w: finite(if back_facing { 0.0 } else { 1.0 }),
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

fn mixed_double_sided_fixture() -> Vec<u8> {
    let binary = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ]
    .into_iter()
    .flatten()
    .flat_map(f32::to_le_bytes)
    .collect::<Vec<_>>();
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"materials":[{"pbrMetallicRoughness":{"baseColorFactor":[0.8,0.4,0.2,1.0]}},{"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.8,0.4,1.0]},"doubleSided":true},{"pbrMetallicRoughness":{"baseColorFactor":[0.2,0.4,0.8,1.0]},"doubleSided":false}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":0,"mode":4}]},{"primitives":[{"attributes":{"POSITION":0},"material":1,"mode":4}]},{"primitives":[{"attributes":{"POSITION":0},"material":2,"mode":4}]}]}"#;
    glb_with_json(json, &binary)
}

fn metallic_fixture() -> Vec<u8> {
    metallic_roughness_fixture(None, 1.0, 0.5)
}

fn emissive_fixture(emissive: Option<[f32; 3]>) -> Vec<u8> {
    let mut binary = Vec::with_capacity(36);
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let emissive_field = emissive.map_or_else(String::new, |value| {
        format!(
            r#", "emissiveFactor":[{},{},{}]"#,
            value[0], value[1], value[2]
        )
    });
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":36}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.2,0.1,0.05,0.4],"metallicFactor":0.0,"roughnessFactor":0.5}}{emissive_field}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0,"mode":4}}]}}]}}"#,
    );
    glb_with_json(&json, &binary)
}

fn alpha_fixture(
    alpha_mode: &str,
    factor_alpha: f32,
    texture_alpha: Option<u8>,
    cutoff: Option<f32>,
) -> Vec<u8> {
    alpha_fixture_with_double_sided(
        alpha_mode,
        factor_alpha,
        texture_alpha,
        cutoff,
        false,
        false,
    )
}

#[derive(Clone, Copy)]
struct VertexColorFixture {
    colors: Option<[[f32; 4]; 3]>,
    factor: [f32; 4],
    texture: Option<[u8; 4]>,
    emissive: [f32; 3],
    alpha_mode: &'static str,
    alpha_cutoff: Option<f32>,
    double_sided: bool,
    unlit: bool,
    include_material: bool,
}

impl VertexColorFixture {
    const fn white() -> Self {
        Self {
            colors: Some([[1.0; 4]; 3]),
            factor: [1.0; 4],
            texture: None,
            emissive: [0.0; 3],
            alpha_mode: "OPAQUE",
            alpha_cutoff: None,
            double_sided: false,
            unlit: true,
            include_material: true,
        }
    }

    const fn gradient() -> Self {
        Self {
            colors: Some([
                [1.0, 0.0, 0.0, 0.25],
                [0.0, 1.0, 0.0, 0.5],
                [0.0, 0.0, 1.0, 0.75],
            ]),
            ..Self::white()
        }
    }
}

fn vertex_color_fixture(fixture: VertexColorFixture) -> Vec<u8> {
    let mut binary = Vec::new();
    append_f32_vectors(
        &mut binary,
        &[[-0.75, -0.75, 0.0], [0.75, -0.75, 0.0], [0.0, 0.75, 0.0]],
    );
    let mut views = vec![r#"{"buffer":0,"byteOffset":0,"byteLength":36}"#.to_owned()];
    let mut accessors =
        vec![r#"{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}"#.to_owned()];
    let mut attributes = vec![r#""POSITION":0"#.to_owned()];
    if let Some(colors) = fixture.colors {
        let offset = binary.len();
        append_f32_vectors(&mut binary, &colors);
        let view = views.len();
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":48}}"#
        ));
        let accessor = accessors.len();
        accessors.push(format!(
            r#"{{"bufferView":{view},"componentType":5126,"count":3,"type":"VEC4"}}"#
        ));
        attributes.push(format!(r#""COLOR_0":{accessor}"#));
    }
    let texture_resources = append_vertex_color_texture(
        fixture.texture,
        &mut binary,
        &mut views,
        &mut accessors,
        &mut attributes,
    );
    let (materials, material_reference, extensions_used) = vertex_color_material(&fixture);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}}{extensions_used},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{views}],"accessors":[{accessors}]{texture_resources}{materials},"meshes":[{{"primitives":[{{"attributes":{{{attributes}}}{material_reference},"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        views = views.join(","),
        accessors = accessors.join(","),
        attributes = attributes.join(","),
    );
    glb_with_json(&json, &binary)
}

fn append_vertex_color_texture(
    texture: Option<[u8; 4]>,
    binary: &mut Vec<u8>,
    views: &mut Vec<String>,
    accessors: &mut Vec<String>,
    attributes: &mut Vec<String>,
) -> String {
    let Some(texel) = texture else {
        return String::new();
    };
    let texcoord_offset = binary.len();
    append_f32_vectors(binary, &[[0.5, 0.5]; 3]);
    let texcoord_view = views.len();
    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{texcoord_offset},"byteLength":24}}"#
    ));
    let texcoord_accessor = accessors.len();
    accessors.push(format!(
        r#"{{"bufferView":{texcoord_view},"componentType":5126,"count":3,"type":"VEC2"}}"#
    ));
    attributes.push(format!(r#""TEXCOORD_0":{texcoord_accessor}"#));
    let png = encode_png(1, 1, &texel);
    let image_offset = binary.len();
    binary.extend_from_slice(&png);
    let image_view = views.len();
    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{image_offset},"byteLength":{}}}"#,
        png.len()
    ));
    format!(
        r#", "textures":[{{"source":0}}],"images":[{{"bufferView":{image_view},"mimeType":"image/png"}}]"#
    )
}

fn vertex_color_material(fixture: &VertexColorFixture) -> (String, &'static str, &'static str) {
    if !fixture.include_material {
        return (String::new(), "", "");
    }
    let base_color_texture = fixture
        .texture
        .map_or("", |_| r#", "baseColorTexture":{"index":0}"#);
    let alpha_cutoff = fixture
        .alpha_cutoff
        .map_or_else(String::new, |value| format!(r#", "alphaCutoff":{value}"#));
    let double_sided = if fixture.double_sided {
        r#", "doubleSided":true"#
    } else {
        ""
    };
    let (extension, extensions_used) = if fixture.unlit {
        (
            r#", "extensions":{"KHR_materials_unlit":{}}"#,
            r#", "extensionsUsed":["KHR_materials_unlit"]"#,
        )
    } else {
        ("", "")
    };
    let material = format!(
        r#", "materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[{},{},{},{}],"metallicFactor":0,"roughnessFactor":0.8{base_color_texture}}},"emissiveFactor":[{},{},{}],"alphaMode":"{}"{alpha_cutoff}{double_sided}{extension}}}]"#,
        fixture.factor[0],
        fixture.factor[1],
        fixture.factor[2],
        fixture.factor[3],
        fixture.emissive[0],
        fixture.emissive[1],
        fixture.emissive[2],
        fixture.alpha_mode,
    );
    (material, r#", "material":0"#, extensions_used)
}

fn append_f32_vectors<const WIDTH: usize>(binary: &mut Vec<u8>, values: &[[f32; WIDTH]; 3]) {
    for vector in values {
        for value in vector {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
}

fn double_sided_alpha_fixture(
    alpha_mode: &str,
    factor_alpha: f32,
    texture_alpha: Option<u8>,
    cutoff: Option<f32>,
) -> Vec<u8> {
    alpha_fixture_with_double_sided(alpha_mode, factor_alpha, texture_alpha, cutoff, true, false)
}

fn unlit_double_sided_alpha_fixture(
    alpha_mode: &str,
    factor_alpha: f32,
    texture_alpha: Option<u8>,
    cutoff: Option<f32>,
) -> Vec<u8> {
    alpha_fixture_with_double_sided(alpha_mode, factor_alpha, texture_alpha, cutoff, true, true)
}

fn alpha_fixture_with_double_sided(
    alpha_mode: &str,
    factor_alpha: f32,
    texture_alpha: Option<u8>,
    cutoff: Option<f32>,
    double_sided: bool,
    unlit: bool,
) -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.5_f32, 0.5]; 3] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let (image_view, resources, texture_role) = texture_alpha.map_or_else(
        || (String::new(), String::new(), String::new()),
        |alpha| {
            let png = encode_png(1, 1, &[255, 255, 255, alpha]);
            let image_offset = binary.len();
            binary.extend_from_slice(&png);
            (
                format!(
                    r#",{{"buffer":0,"byteOffset":{image_offset},"byteLength":{}}}"#,
                    png.len()
                ),
                r#", "textures":[{"source":0}],"images":[{"bufferView":2,"mimeType":"image/png"}]"#
                    .to_owned(),
                r#", "baseColorTexture":{"index":0}"#.to_owned(),
            )
        },
    );
    let cutoff = cutoff.map_or_else(String::new, |value| format!(r#", "alphaCutoff":{value}"#));
    let double_sided = if double_sided {
        r#", "doubleSided":true"#
    } else {
        ""
    };
    let (extension_declaration, extension_marker) = if unlit {
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#", "extensions":{"KHR_materials_unlit":{}}"#,
        )
    } else {
        ("", "")
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},{extension_declaration}"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}}{image_view}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.8,0.4,0.2,{factor_alpha}]{texture_role}}},"alphaMode":"{alpha_mode}"{cutoff}{double_sided}{extension_marker}}}]{resources},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
    );
    glb_with_json(&json, &binary)
}

fn emissive_texture_fixture(texel: Option<[u8; 4]>, factor: [f32; 3]) -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.5_f32, 0.5]; 3] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let (image_view, resources, emissive_role) = texel.map_or_else(
        || (String::new(), String::new(), String::new()),
        |texel| {
            let png = encode_png(1, 1, &texel);
            let image_offset = binary.len();
            binary.extend_from_slice(&png);
            (
                format!(
                    r#",{{"buffer":0,"byteOffset":{image_offset},"byteLength":{}}}"#,
                    png.len()
                ),
                r#", "textures":[{"source":0}],"images":[{"bufferView":2,"mimeType":"image/png"}]"#
                    .to_owned(),
                r#", "emissiveTexture":{"index":0}"#.to_owned(),
            )
        },
    );
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}}{image_view}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.2,0.1,0.05,0.4],"metallicFactor":0.0,"roughnessFactor":0.5}},"emissiveFactor":[{red},{green},{blue}]{emissive_role}}}]{resources},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        red = factor[0],
        green = factor[1],
        blue = factor[2],
    );
    glb_with_json(&json, &binary)
}

fn metallic_roughness_fixture(
    texel: Option<[u8; 4]>,
    metallic_factor: f32,
    roughness_factor: f32,
) -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ];
    let mut binary = Vec::new();
    for vertex in positions {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.5_f32, 0.5]; 3] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let (image_view, texture_fields, role) = if let Some(texel) = texel {
        let png = encode_png(1, 1, &texel);
        let image_offset = binary.len();
        binary.extend_from_slice(&png);
        (
            format!(
                r#",{{"buffer":0,"byteOffset":{image_offset},"byteLength":{}}}"#,
                png.len()
            ),
            r#","textures":[{"source":0}],"images":[{"bufferView":2,"mimeType":"image/png"}]"#,
            r#","metallicRoughnessTexture":{"index":0}"#,
        )
    } else {
        (String::new(), "", "")
    };
    let pbr_fields = format!(
        r#""baseColorFactor":[0.8,0.4,0.2,1.0],"metallicFactor":{metallic_factor},"roughnessFactor":{roughness_factor}{role}"#
    );
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}}{image_view}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{{pbr_fields}}}}}]{texture_fields},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
    );
    glb_with_json(&json, &binary)
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
    normal_texture_fixture_with_options(texel, scale, false, true)
}

fn generated_normal_texture_fixture(texel: [u8; 4], scale: f32) -> Vec<u8> {
    normal_texture_fixture_with_options(texel, scale, false, false)
}

fn double_sided_normal_texture_fixture(texel: [u8; 4], scale: f32) -> Vec<u8> {
    normal_texture_fixture_with_options(texel, scale, true, true)
}

fn normal_texture_fixture_with_options(
    texel: [u8; 4],
    scale: f32,
    double_sided: bool,
    include_tangents: bool,
) -> Vec<u8> {
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
    for texcoord in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
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
    let double_sided = if double_sided {
        r#", "doubleSided":true"#
    } else {
        ""
    };
    let tangent_attribute = if include_tangents {
        r#","TANGENT":2"#
    } else {
        ""
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{base_image_offset},"byteLength":{base_image_length}}},{{"buffer":0,"byteOffset":{normal_image_offset},"byteLength":{normal_image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.8,0.4,0.2,1.0],"metallicFactor":0.0,"roughnessFactor":0.5,"baseColorTexture":{{"index":0}}}},"normalTexture":{{"index":1,"scale":{scale}}}{double_sided}}}],"textures":[{{"source":0}},{{"source":1}}],"images":[{{"bufferView":4,"mimeType":"image/png"}},{{"bufferView":5,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1{tangent_attribute},"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        base_image_length = base_png.len(),
        normal_image_length = normal_png.len(),
    );
    glb_with_json(&json, &binary)
}

fn four_role_texture_fixture() -> Vec<u8> {
    four_role_texture_fixture_with_unlit(false)
}

fn unlit_four_role_texture_fixture() -> Vec<u8> {
    four_role_texture_fixture_with_unlit(true)
}

fn sampled_unlit_base_fixture(
    width: u32,
    height: u32,
    pixels: &[u8],
    texcoords: [[f32; 2]; 3],
    sampler_fields: Option<&str>,
) -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in texcoords {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let png = encode_png(width, height, pixels);
    let image_offset = binary.len();
    binary.extend_from_slice(&png);
    let (samplers, texture_sampler) = sampler_fields.map_or_else(
        || (String::new(), String::new()),
        |fields| {
            (
                format!(r#", "samplers":[{{{fields}}}]"#),
                r#""sampler":0,"#.to_owned(),
            )
        },
    );
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"extensionsUsed":["KHR_materials_unlit"],"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}]{samplers},"textures":[{{{texture_sampler}"source":0}}],"images":[{{"bufferView":2,"mimeType":"image/png"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"extensions":{{"KHR_materials_unlit":{{}}}}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_length = png.len(),
    );
    glb_with_json(&json, &binary)
}

fn four_role_sampler_fixture(shared_image: bool, shared_sampler: bool) -> Vec<u8> {
    four_role_sampler_fixture_with_transforms(shared_image, shared_sampler, false)
}

fn transformed_four_role_fixture(shared_image: bool) -> Vec<u8> {
    four_role_sampler_fixture_with_transforms(shared_image, true, true)
}

fn four_role_sampler_fixture_with_transforms(
    shared_image: bool,
    shared_sampler: bool,
    texture_transforms: bool,
) -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
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
    let texcoord = if texture_transforms {
        [0.125_f32, 0.125]
    } else {
        [1.375_f32, -0.375]
    };
    for texcoord in [texcoord; 3] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }

    let (image_views, image_records) = append_four_role_fixture_images(
        &mut binary,
        shared_image,
        shared_sampler,
        texture_transforms,
    );
    let (samplers, textures) = if texture_transforms {
        (
            r#", "samplers":[{"magFilter":9728,"minFilter":9728,"wrapS":33071,"wrapT":33071}]"#
                .to_owned(),
            (0..4)
                .map(|source| {
                    let source = if shared_image { 0 } else { source };
                    format!(r#"{{"sampler":0,"source":{source}}}"#)
                })
                .collect::<Vec<_>>()
                .join(","),
        )
    } else {
        four_role_sampler_records(shared_image, shared_sampler)
    };
    let (extension_declaration, transforms) = if texture_transforms {
        (
            r#", "extensionsUsed":["KHR_texture_transform"]"#,
            [
                r#", "extensions":{"KHR_texture_transform":{"offset":[0.3125,0.375],"scale":[0.5,2.0]}}"#,
                r#", "extensions":{"KHR_texture_transform":{"offset":[0.8125,0.875],"rotation":-1.5707963267948966,"scale":[2.0,0.5]}}"#,
                r#", "extensions":{"KHR_texture_transform":{"offset":[0.75,0.5],"rotation":1.5707963267948966}}"#,
                r#", "extensions":{"KHR_texture_transform":{"offset":[0.25,0.0]}}"#,
            ],
        )
    } else {
        ("", [""; 4])
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}}{extension_declaration},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{image_views}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.5,0.25,0.75,1.0],"baseColorTexture":{{"index":0{base_color_transform}}},"metallicRoughnessTexture":{{"index":1{metallic_roughness_transform}}},"metallicFactor":0.75,"roughnessFactor":0.5}},"normalTexture":{{"index":2,"scale":0.5{normal_transform}}},"emissiveFactor":[0.25,0.5,0.75],"emissiveTexture":{{"index":3{emissive_transform}}}}}],"textures":[{textures}],"images":[{image_records}]{samplers},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_views = image_views.join(","),
        image_records = image_records.join(","),
        base_color_transform = transforms[0],
        metallic_roughness_transform = transforms[1],
        normal_transform = transforms[2],
        emissive_transform = transforms[3],
    );
    glb_with_json(&json, &binary)
}

fn append_four_role_fixture_images(
    binary: &mut Vec<u8>,
    shared_image: bool,
    shared_sampler: bool,
    texture_transforms: bool,
) -> (Vec<String>, Vec<String>) {
    let selected = if shared_sampler && !texture_transforms {
        [[128_u8, 128, 255, 255]; 4]
    } else {
        [
            [128_u8, 64, 32, 255],
            [0, 128, 64, 255],
            [128, 128, 255, 255],
            [32, 64, 128, 255],
        ]
    };
    let images = if shared_image {
        let mut pixels = vec![255_u8; 4 * 4 * 4];
        for (column, row, texel) in [
            (1_usize, 2_usize, selected[0]),
            (3, 2, selected[1]),
            (2, 2, selected[2]),
            (1, 0, selected[3]),
        ] {
            let offset = (row * 4 + column) * 4;
            pixels[offset..offset + 4].copy_from_slice(&texel);
        }
        vec![encode_png(4, 4, &pixels)]
    } else {
        selected
            .into_iter()
            .map(|texel| encode_png(1, 1, &texel))
            .collect()
    };
    let offsets = images
        .iter()
        .map(|image| {
            let offset = binary.len();
            binary.extend_from_slice(image);
            offset
        })
        .collect::<Vec<_>>();
    let views = images
        .iter()
        .zip(&offsets)
        .map(|(image, offset)| {
            format!(
                r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{}}}"#,
                image.len()
            )
        })
        .collect();
    let records = images
        .iter()
        .enumerate()
        .map(|(index, _)| format!(r#"{{"bufferView":{},"mimeType":"image/png"}}"#, index + 4))
        .collect();
    (views, records)
}

fn four_role_sampler_records(shared_image: bool, shared_sampler: bool) -> (String, String) {
    if shared_sampler {
        (
            r#", "samplers":[{"magFilter":9728,"minFilter":9728,"wrapS":10497,"wrapT":10497}]"#
                .to_owned(),
            (0..4)
                .map(|source| {
                    let source = if shared_image { 0 } else { source };
                    format!(r#"{{"sampler":0,"source":{source}}}"#)
                })
                .collect::<Vec<_>>()
                .join(","),
        )
    } else if shared_image {
        (
            r#", "samplers":[{"magFilter":9728,"minFilter":9728,"wrapS":10497,"wrapT":10497},{"magFilter":9728,"minFilter":9728,"wrapS":33071,"wrapT":10497},{"magFilter":9728,"minFilter":9728,"wrapS":33648,"wrapT":10497},{"magFilter":9728,"minFilter":9728,"wrapS":10497,"wrapT":33071}]"#.to_owned(),
            (0..4)
                .map(|sampler| format!(r#"{{"sampler":{sampler},"source":0}}"#))
                .collect::<Vec<_>>()
                .join(","),
        )
    } else {
        (
            String::new(),
            (0..4)
                .map(|source| format!(r#"{{"source":{source}}}"#))
                .collect::<Vec<_>>()
                .join(","),
        )
    }
}

fn four_role_texture_fixture_with_unlit(unlit: bool) -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
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
    let images = [
        encode_png(
            1,
            1,
            if unlit {
                &[128, 64, 32, 255]
            } else {
                &[255, 255, 255, 255]
            },
        ),
        encode_png(1, 1, &[7, 128, 64, 9]),
        encode_png(1, 1, &[128, 128, 255, 255]),
        encode_png(1, 1, &[32, 64, 128, 3]),
    ];
    let offsets = images.each_ref().map(|image| {
        let offset = binary.len();
        binary.extend_from_slice(image);
        offset
    });
    let (extension_declaration, material_fields) = if unlit {
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#", "emissiveFactor":[1.0,0.5,0.25], "extensions":{"KHR_materials_unlit":{}}"#,
        )
    } else {
        ("", "")
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},{extension_declaration}"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{base_offset},"byteLength":{base_length}}},{{"buffer":0,"byteOffset":{material_offset},"byteLength":{material_length}}},{{"buffer":0,"byteOffset":{normal_offset},"byteLength":{normal_length}}},{{"buffer":0,"byteOffset":{emissive_offset},"byteLength":{emissive_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorFactor":[0.5,0.25,0.75,1.0],"baseColorTexture":{{"index":0}},"metallicRoughnessTexture":{{"index":1}}}},"normalTexture":{{"index":2}},"emissiveTexture":{{"index":3}}{material_fields}}}],"textures":[{{"source":0}},{{"source":1}},{{"source":2}},{{"source":3}}],"images":[{{"bufferView":4,"mimeType":"image/png"}},{{"bufferView":5,"mimeType":"image/png"}},{{"bufferView":6,"mimeType":"image/png"}},{{"bufferView":7,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        base_offset = offsets[0],
        base_length = images[0].len(),
        material_offset = offsets[1],
        material_length = images[1].len(),
        normal_offset = offsets[2],
        normal_length = images[2].len(),
        emissive_offset = offsets[3],
        emissive_length = images[3].len(),
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
