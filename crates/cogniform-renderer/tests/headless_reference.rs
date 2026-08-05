//! Headless adapter integration contract for the built-in reference scene.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_protocol::{
    CameraComponent, ColorRgba, MaterialComponent, PositiveF32, PositiveVec3, PrimitiveComponent,
    PrimitiveShape, RenderChange, RenderComponents, RenderEntity, RenderExtraction, SceneRevision,
    StableEntityId, UnitF32,
};
use cogniform_renderer::{
    HeadlessRenderer, REFERENCE_COLOR, REFERENCE_ENTITY_ID, RendererConfig, RendererError,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn reference_cube_produces_exact_ids_and_tolerant_color_depth_normals() {
    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    let adapter = renderer.adapter().clone();
    let frame = renderer
        .submit_reference_scene()
        .expect("reference submission must be admitted")
        .read()
        .expect("reference readback must complete");

    assert_eq!(frame.width(), WIDTH);
    assert_eq!(frame.height(), HEIGHT);
    assert_eq!(frame.adapter(), &adapter);
    assert!(matches!(adapter.backend.as_str(), "dx12" | "vulkan"));

    let center = (WIDTH / 2, HEIGHT / 2);
    assert_eq!(
        frame.entity_id_at(center.0, center.1),
        Some(REFERENCE_ENTITY_ID)
    );
    assert_eq!(frame.entity_id_at(0, 0), Some(0));
    assert!(
        frame
            .entity_ids()
            .iter()
            .all(|entity_id| matches!(*entity_id, 0 | REFERENCE_ENTITY_ID))
    );

    let center_color = frame
        .color_at(center.0, center.1)
        .expect("center color should exist");
    for (actual, expected) in center_color.into_iter().zip(REFERENCE_COLOR) {
        assert!(
            actual.abs_diff(expected) <= 2,
            "color channel {actual} differs from {expected}"
        );
    }

    let center_depth = frame
        .depth_at(center.0, center.1)
        .expect("center depth should exist");
    assert!(
        (center_depth - 0.3).abs() <= 0.02,
        "unexpected center depth {center_depth}"
    );
    assert_eq!(frame.depth_at(0, 0), Some(1.0));

    let center_normal = frame
        .normal_at(center.0, center.1)
        .expect("cube center should have a world-space normal");
    let normal_length = center_normal
        .iter()
        .map(|value| value * value)
        .sum::<f32>()
        .sqrt();
    assert!((normal_length - 1.0).abs() <= 1.0e-5);
    assert!(center_normal[2].abs() >= 0.99);
    assert_eq!(frame.normal_at(0, 0), None);

    assert_eq!(frame.color().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.depth().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.normals().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.entity_ids().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.color_at(WIDTH, 0), None);
    assert_eq!(frame.depth_at(0, HEIGHT), None);
    assert_eq!(frame.normal_at(WIDTH, HEIGHT), None);
    assert_eq!(frame.entity_id_at(WIDTH, HEIGHT), None);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn pending_readback_survives_renderer_drop_after_submission() {
    let pending = pollster::block_on(async {
        let mut renderer = HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT))
            .await
            .expect("the declared reference adapter must initialize");
        renderer
            .submit_reference_scene()
            .expect("reference submission must be admitted")
    });

    let frame = pending
        .read()
        .expect("readback must remain valid after the renderer is dropped");
    assert_eq!(
        frame.entity_id_at(WIDTH / 2, HEIGHT / 2),
        Some(REFERENCE_ENTITY_ID)
    );
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn readback_pool_pressure_is_explicit_and_reusable() {
    let config =
        RendererConfig::new(WIDTH, HEIGHT).with_readback_capacity(NonZeroU32::new(1).unwrap());
    let mut renderer = pollster::block_on(HeadlessRenderer::new(config))
        .expect("the declared reference adapter must initialize");
    let first = renderer
        .submit_reference_scene()
        .expect("the first readback lease must be available");
    assert!(matches!(
        renderer.submit_reference_scene(),
        Err(RendererError::ReadbackPoolExhausted { capacity: 1 })
    ));
    drop(first);
    renderer
        .submit_reference_scene()
        .expect("dropping a pending frame must return its lease")
        .read()
        .expect("the recycled readback lease must remain valid");
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn extracted_plane_produces_color_depth_identity_and_plus_z_normal() {
    let camera_id = StableEntityId::new(1).unwrap();
    let plane_id = StableEntityId::new(2).unwrap();
    let positive = |value| PositiveF32::new(value).unwrap();
    let unit = |value| UnitF32::new(value).unwrap();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut camera_world = identity;
    camera_world[14] = 3.0;
    let camera = RenderEntity::new(
        camera_id,
        camera_world,
        1,
        RenderComponents {
            camera: Some(CameraComponent {
                vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                near: positive(0.1),
                far: positive(100.0),
            }),
            ..RenderComponents::default()
        },
    )
    .unwrap();
    let plane = RenderEntity::new(
        plane_id,
        identity,
        1,
        RenderComponents {
            primitive: Some(PrimitiveComponent {
                shape: PrimitiveShape::Plane,
                dimensions: PositiveVec3 {
                    x: positive(1.5),
                    y: positive(0.75),
                    z: positive(2.0),
                },
            }),
            material: Some(MaterialComponent {
                base_color: ColorRgba {
                    r: unit(0.2),
                    g: unit(0.6),
                    b: unit(0.9),
                    a: unit(1.0),
                },
                metallic: unit(0.0),
                roughness: unit(0.5),
            }),
            ..RenderComponents::default()
        },
    )
    .unwrap();
    let extraction = RenderExtraction::new(
        NonZeroU64::new(1).unwrap(),
        SceneRevision::INITIAL,
        SceneRevision::new(1),
        vec![RenderChange::upsert(camera), RenderChange::upsert(plane)],
    )
    .unwrap();

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.apply_extraction(&extraction).unwrap();
    let frame = renderer.submit_scene(camera_id).unwrap().read().unwrap();
    let center = (WIDTH / 2, HEIGHT / 2);

    assert_eq!(
        frame.stable_entity_id_at(center.0, center.1),
        Some(plane_id)
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
    assert!(depth.is_finite() && depth < 1.0);
    let normal = frame.normal_at(center.0, center.1).unwrap();
    assert!(normal[0].abs() <= 0.01);
    assert!(normal[1].abs() <= 0.01);
    assert!(normal[2] >= 0.99);
    assert_eq!(frame.stable_entity_id_at(0, 0), None);
    assert_eq!(frame.normal_at(0, 0), None);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn extracted_sphere_produces_curved_depth_identity_and_radial_normals() {
    let camera_id = StableEntityId::new(1).unwrap();
    let sphere_id = StableEntityId::new(2).unwrap();
    let positive = |value| PositiveF32::new(value).unwrap();
    let unit = |value| UnitF32::new(value).unwrap();
    let identity = [
        1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
    ];
    let mut camera_world = identity;
    camera_world[14] = 3.0;
    let camera = RenderEntity::new(
        camera_id,
        camera_world,
        1,
        RenderComponents {
            camera: Some(CameraComponent {
                vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                near: positive(0.1),
                far: positive(100.0),
            }),
            ..RenderComponents::default()
        },
    )
    .unwrap();
    let sphere = RenderEntity::new(
        sphere_id,
        identity,
        1,
        RenderComponents {
            primitive: Some(PrimitiveComponent {
                shape: PrimitiveShape::Sphere,
                dimensions: PositiveVec3 {
                    x: positive(1.5),
                    y: positive(1.5),
                    z: positive(1.5),
                },
            }),
            material: Some(MaterialComponent {
                base_color: ColorRgba {
                    r: unit(0.9),
                    g: unit(0.3),
                    b: unit(0.2),
                    a: unit(1.0),
                },
                metallic: unit(0.0),
                roughness: unit(0.5),
            }),
            ..RenderComponents::default()
        },
    )
    .unwrap();
    let extraction = RenderExtraction::new(
        NonZeroU64::new(1).unwrap(),
        SceneRevision::INITIAL,
        SceneRevision::new(1),
        vec![RenderChange::upsert(camera), RenderChange::upsert(sphere)],
    )
    .unwrap();

    let mut renderer =
        pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
            .expect("the declared reference adapter must initialize");
    renderer.apply_extraction(&extraction).unwrap();
    let frame = renderer.submit_scene(camera_id).unwrap().read().unwrap();
    let center = (WIDTH / 2, HEIGHT / 2);

    assert_eq!(
        frame.stable_entity_id_at(center.0, center.1),
        Some(sphere_id)
    );
    for (actual, expected) in frame
        .color_at(center.0, center.1)
        .unwrap()
        .into_iter()
        .zip([230, 77, 51, 255])
    {
        assert!(actual.abs_diff(expected) <= 2);
    }
    let center_depth = frame.depth_at(center.0, center.1).unwrap();
    assert!(center_depth.is_finite() && center_depth < 1.0);
    let center_normal = frame.normal_at(center.0, center.1).unwrap();
    assert!(center_normal[0].abs() <= 0.2);
    assert!(center_normal[1].abs() <= 0.2);
    assert!(center_normal[2] >= 0.95);

    let right_foreground = ((center.0 + 1)..WIDTH)
        .filter(|&x| frame.stable_entity_id_at(x, center.1) == Some(sphere_id))
        .collect::<Vec<_>>();
    assert!(right_foreground.len() >= 3);
    let curved_x = right_foreground[right_foreground.len() / 2];
    let curved_depth = frame.depth_at(curved_x, center.1).unwrap();
    let curved_normal = frame.normal_at(curved_x, center.1).unwrap();
    assert!(curved_depth > center_depth);
    assert!(curved_normal[0] > center_normal[0] + 0.1);
    assert!(curved_normal[2] > 0.1);
    assert_eq!(frame.stable_entity_id_at(0, 0), None);
    assert_eq!(frame.normal_at(0, 0), None);
}
