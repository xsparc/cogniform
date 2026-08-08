//! Controlled adapter contract for extraction, rendering, and observation causality.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use core::num::NonZeroU32;
use std::time::{Duration, Instant};

use cogniform_engine::{
    CogniformEngine, EngineConfig, EngineError, Observation, ObservationError, ObservationPayload,
    ObservationRequest,
};
use cogniform_protocol::{
    ApplyStatus, CameraComponent, ColorRgba, ComponentValue, ConflictPolicy, CreateEntity,
    DeliverySemantic, FiniteF32, IdempotencyKey, LocalTransform, MaterialComponent, NameComponent,
    ObservationId, ObservationKind, ObservationQuality, PatchBudget, PositiveF32, PositiveVec3,
    PrimitiveComponent, PrimitiveShape, Quaternion, SceneOperation, ScenePatch, SceneRevision,
    SceneText, SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32, Vec3,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

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

fn initial_patch(camera: StableEntityId, cube: StableEntityId) -> ScenePatch {
    patch(
        SceneRevision::INITIAL,
        1,
        vec![
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
                entity_id: cube,
                components: vec![
                    ComponentValue::LocalTransform(transform(0.0)),
                    ComponentValue::Primitive(PrimitiveComponent {
                        shape: PrimitiveShape::Cuboid,
                        dimensions: PositiveVec3 {
                            x: positive(1.0),
                            y: positive(1.0),
                            z: positive(1.0),
                        },
                    }),
                    ComponentValue::Material(MaterialComponent {
                        base_color: ColorRgba {
                            r: unit(0.2),
                            g: unit(0.6),
                            b: unit(0.9),
                            a: unit(1.0),
                        },
                        metallic: unit(0.0),
                        roughness: unit(0.5),
                    }),
                ],
            }),
        ],
    )
}

fn request(
    nonce: u128,
    scene_revision: SceneRevision,
    camera_id: StableEntityId,
    kind: ObservationKind,
) -> ObservationRequest {
    ObservationRequest {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(nonce).unwrap(),
        scene_revision,
        camera_id,
        kind,
        quality: ObservationQuality::Low,
    }
}

fn wait_for(engine: &CogniformEngine) -> Observation {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(observation) = engine.try_receive_observation().unwrap() {
            return observation;
        }
        assert!(Instant::now() < deadline, "observation timed out");
        std::thread::yield_now();
    }
}

fn assert_revision_rejects_before_capacity_and_renderer(engine: &mut CogniformEngine) {
    assert!(matches!(
        engine.request_observation(request(
            12,
            SceneRevision::INITIAL,
            id(999),
            ObservationKind::Depth,
        )),
        Err(EngineError::Observation(
            ObservationError::RequestRevisionMismatch {
                requested: SceneRevision::INITIAL,
                current,
            }
        )) if current == SceneRevision::new(1)
    ));
    assert!(matches!(
        engine.request_observation(request(
            13,
            SceneRevision::new(2),
            id(999),
            ObservationKind::Depth,
        )),
        Err(EngineError::Observation(
            ObservationError::RequestRevisionMismatch { requested, current }
        )) if requested == SceneRevision::new(2) && current == SceneRevision::new(1)
    ));
    assert_eq!(engine.outstanding_observations(), 1);
}

fn assert_capacity_remains_full(engine: &mut CogniformEngine, camera: StableEntityId) {
    assert!(matches!(
        engine.request_observation(request(
            11,
            SceneRevision::new(1),
            camera,
            ObservationKind::Depth,
        )),
        Err(EngineError::Observation(
            ObservationError::CapacityExceeded { capacity: 1 }
        ))
    ));
}

fn assert_observation_metadata(
    observation: &Observation,
    nonce: u128,
    camera: StableEntityId,
    kind: ObservationKind,
) {
    let metadata = observation.metadata();
    assert_eq!(metadata.observation_id, ObservationId::new(nonce).unwrap());
    assert_eq!(metadata.scene_revision, SceneRevision::new(2));
    assert_eq!(metadata.camera_id, camera);
    assert_eq!(metadata.kind, kind);
    assert_eq!(metadata.quality, ObservationQuality::Low);
    assert!(metadata.observed_at_unix_micros > 0);
    assert!(
        metadata.production_latency_micros
            <= u64::try_from(Duration::from_secs(10).as_micros()).unwrap()
    );
    assert_eq!(
        metadata.staleness.latest_known_revision,
        SceneRevision::new(2)
    );
    assert_eq!(metadata.staleness.revisions_behind, 0);
    if kind == ObservationKind::Visibility {
        assert_eq!(metadata.dimensions, None);
    } else {
        let dimensions = metadata
            .dimensions
            .expect("image observation must carry dimensions");
        assert_eq!(dimensions.width.get(), WIDTH);
        assert_eq!(dimensions.height.get(), HEIGHT);
    }
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn extracted_frames_and_bounded_observations_preserve_revision_causality() {
    let camera = id(1);
    let cube = id(2);
    let mut config =
        EngineConfig::new(WIDTH, HEIGHT).with_observation_capacity(NonZeroU32::new(1).unwrap());
    config.renderer = config
        .renderer
        .with_readback_capacity(NonZeroU32::new(1).unwrap());
    let mut engine = pollster::block_on(CogniformEngine::new(config)).unwrap();
    let receipt = engine.apply_patch(&initial_patch(camera, cube)).unwrap();
    assert_eq!(receipt.new_revision, SceneRevision::new(1));
    assert_eq!(engine.renderer().scene_revision(), SceneRevision::new(1));
    assert_eq!(engine.renderer().extracted_entity_count(), 2);
    let generation = engine.renderer().extraction_generation();
    let replayed = engine.apply_patch(&initial_patch(camera, cube)).unwrap();
    assert_eq!(replayed.status, ApplyStatus::IdempotentReplay);
    assert_eq!(engine.revision(), SceneRevision::new(1));
    assert_eq!(engine.renderer().extraction_generation(), generation);

    engine
        .request_observation(request(
            10,
            SceneRevision::new(1),
            camera,
            ObservationKind::EntityId,
        ))
        .unwrap();
    assert_eq!(engine.outstanding_observations(), 1);
    assert!(engine.oldest_outstanding_observation_age_micros().is_some());
    assert_revision_rejects_before_capacity_and_renderer(&mut engine);
    assert_capacity_remains_full(&mut engine, camera);

    engine
        .apply_patch(&patch(
            engine.revision(),
            2,
            vec![SceneOperation::SetComponent(SetComponent {
                entity_id: cube,
                component: ComponentValue::Name(NameComponent {
                    value: SceneText::new("logical-only revision").unwrap(),
                }),
            })],
        ))
        .unwrap();
    assert_eq!(engine.renderer().scene_revision(), SceneRevision::new(2));

    let ids = wait_for(&engine);
    assert_eq!(engine.oldest_outstanding_observation_age_micros(), None);
    assert_eq!(ids.metadata().scene_revision, SceneRevision::new(1));
    assert_eq!(ids.metadata().frame_id, receipt.estimated_visible_frame);
    assert_eq!(ids.metadata().staleness.revisions_behind, 1);
    let ObservationPayload::EntityId(pixels) = ids.payload() else {
        panic!("expected entity-ID payload");
    };
    assert_eq!(
        pixels[((HEIGHT / 2 * WIDTH) + WIDTH / 2) as usize],
        Some(cube)
    );

    for (nonce, kind) in [
        (20, ObservationKind::Visibility),
        (21, ObservationKind::Color),
        (22, ObservationKind::Depth),
        (23, ObservationKind::Normal),
    ] {
        engine
            .request_observation(request(nonce, SceneRevision::new(2), camera, kind))
            .unwrap();
        let observation = wait_for(&engine);
        assert_observation_metadata(&observation, nonce, camera, kind);
        match observation.payload() {
            ObservationPayload::Visibility(visible) => {
                assert_eq!(visible.len(), 1);
                assert_eq!(visible[0].entity_id, cube);
                assert!(visible[0].visible_pixels > 0);
            }
            ObservationPayload::Color(pixels) => {
                let center = pixels[((HEIGHT / 2 * WIDTH) + WIDTH / 2) as usize];
                for (actual, expected) in center.into_iter().zip([51, 153, 230, 255]) {
                    assert!(actual.abs_diff(expected) <= 2);
                }
            }
            ObservationPayload::Depth(pixels) => {
                assert!(pixels.iter().any(|depth| *depth < 1.0));
            }
            ObservationPayload::Normal(pixels) => {
                let center = pixels[((HEIGHT / 2 * WIDTH) + WIDTH / 2) as usize]
                    .expect("rendered cube center must have a normal");
                let length = center.iter().map(|value| value * value).sum::<f32>().sqrt();
                assert!((length - 1.0).abs() <= 1.0e-5);
                assert!(pixels[0].is_none(), "background normal must be absent");
            }
            ObservationPayload::EntityId(_) => panic!("unexpected entity-ID payload"),
        }
    }
    assert_eq!(engine.outstanding_observations(), 0);
}
