//! Controlled-adapter integration for the CF041 production service adapter.

use core::num::{NonZeroU32, NonZeroU64};
use std::time::{Duration, Instant};

use cogniform_engine::{LocalService, LocalServiceConfig};
use cogniform_local_executor::{LocalExecutorConfig, LocalExecutorPhase, LocalSessionExecutor};
use cogniform_local_session::{
    ClientHello, LOCAL_SESSION_SCHEMA_VERSION, LocalSessionClientKind, LocalSessionClientMessage,
    LocalSessionLimits, LocalSessionServerKind, QueryRequest, SessionClose, SubmitPatch,
    client_control_frame, decode_server_control_frame,
};
use cogniform_local_transport::{LocalFrame, LocalFrameConfig};
use cogniform_protocol::{
    CameraComponent, ComponentValue, ConflictPolicy, CreateEntity, DeliverySemantic, FiniteF32,
    IdempotencyKey, LocalTransform, ObservationId, ObservationKind, ObservationQuality,
    ObservationRequest, PatchBudget, PositiveF32, PositiveVec3, Quaternion, SceneOperation,
    ScenePatch, SceneQuery, SceneRevision, SchemaVersion, StableEntityId, TransactionId, Vec3,
};

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn production_executor_runs_hello_patch_query_and_orderly_close() {
    pollster::block_on(async {
        let service = LocalService::new(LocalServiceConfig::new(8, 8))
            .await
            .unwrap();
        let config = LocalFrameConfig::default();
        let mut executor =
            LocalSessionExecutor::new(service, LocalExecutorConfig::default()).unwrap();
        let entity_id = StableEntityId::new(5).unwrap();
        let camera_id = StableEntityId::new(6).unwrap();

        open_session(&mut executor, &config);
        apply_scene(&mut executor, &config, entity_id, camera_id);
        query_entity(&mut executor, &config, entity_id);
        observe_scene(&mut executor, &config, camera_id);
        close_session(&mut executor, &config);
    });
}

fn open_session(executor: &mut LocalSessionExecutor, config: &LocalFrameConfig) {
    let hello = executor
        .handle_frame(&client(
            1,
            LocalSessionClientKind::Hello(ClientHello {
                receive_limits: LocalSessionLimits::from_config(config).unwrap(),
                compilation_receive_limits: None,
            }),
            config,
        ))
        .unwrap();
    assert!(matches!(
        server_kind(&hello[0], config),
        LocalSessionServerKind::Hello(_)
    ));
}

fn apply_scene(
    executor: &mut LocalSessionExecutor,
    config: &LocalFrameConfig,
    entity_id: StableEntityId,
    camera_id: StableEntityId,
) {
    let patch = ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(2).unwrap(),
        idempotency_key: IdempotencyKey::new(3).unwrap(),
        base_revision: SceneRevision::INITIAL,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![
            SceneOperation::Create(CreateEntity {
                entity_id,
                components: Vec::new(),
            }),
            SceneOperation::Create(CreateEntity {
                entity_id: camera_id,
                components: vec![
                    ComponentValue::LocalTransform(transform(3.0)),
                    ComponentValue::Camera(CameraComponent {
                        vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                        near: positive(0.1),
                        far: positive(100.0),
                    }),
                ],
            }),
        ],
    };
    let admitted = executor
        .handle_frame(&client(
            2,
            LocalSessionClientKind::SubmitPatch(SubmitPatch { patch }),
            config,
        ))
        .unwrap();
    assert!(matches!(
        server_kind(&admitted[0], config),
        LocalSessionServerKind::PatchAdmission(_)
    ));
    let completed = executor.advance().unwrap();
    assert!(matches!(
        server_kind(&completed[0], config),
        LocalSessionServerKind::PatchCompleted(_)
    ));
}

fn query_entity(
    executor: &mut LocalSessionExecutor,
    config: &LocalFrameConfig,
    entity_id: StableEntityId,
) {
    let queried = executor
        .handle_frame(&client(
            3,
            LocalSessionClientKind::Query(QueryRequest {
                query: SceneQuery {
                    schema_version: SchemaVersion::V1,
                    scene_revision: SceneRevision::new(1),
                    entity_ids: vec![entity_id],
                    component_kinds: Vec::new(),
                    limit: NonZeroU32::new(1).unwrap(),
                },
            }),
            config,
        ))
        .unwrap();
    match server_kind(&queried[0], config) {
        LocalSessionServerKind::QueryResult(response) => {
            assert_eq!(response.result.entities.len(), 1);
            assert_eq!(response.result.entities[0].entity_id, entity_id);
        }
        other => panic!("expected query result, received {other:?}"),
    }
}

fn observe_scene(
    executor: &mut LocalSessionExecutor,
    config: &LocalFrameConfig,
    camera_id: StableEntityId,
) {
    let observation_id = ObservationId::new(7).unwrap();
    let accepted = executor
        .handle_frame(&client(
            4,
            LocalSessionClientKind::RequestObservation(
                cogniform_local_session::RequestObservation {
                    request: ObservationRequest {
                        schema_version: SchemaVersion::V1,
                        observation_id,
                        scene_revision: SceneRevision::new(1),
                        camera_id,
                        kind: ObservationKind::Visibility,
                        quality: ObservationQuality::Low,
                    },
                },
            ),
            config,
        ))
        .unwrap();
    assert!(matches!(
        server_kind(&accepted[0], config),
        LocalSessionServerKind::ObservationAccepted(_)
    ));
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        let output = executor.advance().unwrap();
        if let Some(LocalFrame::Observation { metadata, .. }) = output
            .iter()
            .find(|frame| matches!(frame, LocalFrame::Observation { .. }))
        {
            assert_eq!(metadata.observation_id, observation_id);
            assert_eq!(metadata.scene_revision, SceneRevision::new(1));
            break;
        }
        assert!(Instant::now() < deadline, "observation timed out");
        std::thread::yield_now();
    }
}

fn close_session(executor: &mut LocalSessionExecutor, config: &LocalFrameConfig) {
    let closed = executor
        .handle_frame(&client(
            5,
            LocalSessionClientKind::Close(SessionClose {}),
            config,
        ))
        .unwrap();
    assert!(matches!(
        server_kind(&closed[0], config),
        LocalSessionServerKind::Closed(_)
    ));
    assert_eq!(executor.status().phase, LocalExecutorPhase::Closed);
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
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

fn client(
    correlation_id: u64,
    message: LocalSessionClientKind,
    config: &LocalFrameConfig,
) -> LocalFrame {
    client_control_frame(
        NonZeroU64::new(correlation_id).unwrap(),
        &LocalSessionClientMessage {
            schema_version: LOCAL_SESSION_SCHEMA_VERSION,
            message,
        },
        config,
    )
    .unwrap()
}

fn server_kind(frame: &LocalFrame, config: &LocalFrameConfig) -> LocalSessionServerKind {
    decode_server_control_frame(frame, config)
        .unwrap()
        .1
        .message
}
