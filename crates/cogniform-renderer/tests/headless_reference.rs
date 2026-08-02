//! Headless adapter integration contract for the built-in reference scene.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use cogniform_renderer::{HeadlessRenderer, REFERENCE_COLOR, REFERENCE_ENTITY_ID, RendererConfig};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn reference_cube_produces_exact_ids_and_tolerant_color_depth() {
    let renderer = pollster::block_on(HeadlessRenderer::new(RendererConfig::new(WIDTH, HEIGHT)))
        .expect("the declared reference adapter must initialize");
    let adapter = renderer.adapter().clone();
    let frame = renderer
        .submit_reference_scene()
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

    assert_eq!(frame.color().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.depth().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.entity_ids().len(), (WIDTH * HEIGHT) as usize);
    assert_eq!(frame.color_at(WIDTH, 0), None);
    assert_eq!(frame.depth_at(0, HEIGHT), None);
    assert_eq!(frame.entity_id_at(WIDTH, HEIGHT), None);
}
