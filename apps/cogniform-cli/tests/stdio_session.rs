//! Black-box coverage for the fixed-profile binary standard-stream session.

use core::num::{NonZeroU32, NonZeroU64};
use std::{
    io::Write,
    process::{Child, Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use cogniform_compilation::CompilationLimits;
use cogniform_engine::ObservationPayload;
use cogniform_local_session::{
    ClientHello, ImaginationAdmissionStatus, LOCAL_SESSION_SCHEMA_VERSION,
    LOCAL_SESSION_SCHEMA_VERSION_V2, LocalSessionClientKind, LocalSessionClientMessage,
    LocalSessionLimits, LocalSessionServerKind, PatchAdmissionStatus, QueryRequest,
    RequestObservation, SessionClose, SubmitImagination, SubmitPatch, client_control_frame,
    decode_server_control_frame, decode_server_control_frame_with_limits,
};
use cogniform_local_transport::{LocalFrame, LocalFrameConfig, encode_frame, read_frame};
use cogniform_protocol::{
    ApplyStatus, CameraComponent, ComponentValue, ConflictPolicy, CreateEntity, DeliverySemantic,
    FiniteF32, IdempotencyKey, ImaginationBudget, ImaginationEnvelope, ImaginationId,
    ImaginedEntity, LocalTransform, ObservationId, ObservationKind, ObservationQuality,
    ObservationRequest, PatchBudget, PositiveF32, PositiveVec3, PrimitiveComponent, PrimitiveShape,
    Quaternion, SceneOperation, ScenePatch, SceneQuery, SceneRevision, SceneText, SchemaVersion,
    StableEntityId, TransactionId, Vec3,
};

const CHILD_TIMEOUT: Duration = Duration::from_secs(30);

#[test]
fn arguments_are_exact_and_help_does_not_enter_protocol_mode() {
    for arguments in [
        &["serve-stdio", "unexpected"][..],
        &["serve-stdio", "--help"][..],
        &["serve-stdio", "--"][..],
        &["serve-stdio", "serve-stdio"][..],
    ] {
        let output = command().args(arguments).output().unwrap();
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            normalize(output.stderr),
            "error: serve-stdio accepts no arguments\n"
        );
    }
}

#[test]
fn immediate_piped_eof_is_clean_and_needs_no_adapter() {
    let output = command().arg("serve-stdio").output().unwrap();
    assert!(output.status.success());
    assert!(output.stdout.is_empty());
    assert!(output.stderr.is_empty());
}

#[test]
fn malformed_first_frame_is_redacted_and_leaves_stdout_empty() {
    for input in [vec![0_u8], vec![0xff_u8; 68]] {
        let output = run_with_input(&input, Duration::from_secs(5));
        assert!(!output.status.success());
        assert!(output.stdout.is_empty());
        assert_eq!(
            normalize(output.stderr),
            "error: serve-stdio input frame rejected\n"
        );
    }
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn controlled_child_completes_hello_patch_query_observation_and_close() {
    let config = LocalFrameConfig::default();
    let ids = SessionIds {
        entity: StableEntityId::new(5).unwrap(),
        camera: StableEntityId::new(6).unwrap(),
        observation: ObservationId::new(7).unwrap(),
    };
    let input = encode_session_input(&config, ids);

    let output = run_with_input(&input, CHILD_TIMEOUT);
    assert!(
        output.status.success(),
        "{}",
        normalize(output.stderr.clone())
    );
    assert!(output.stderr.is_empty());
    assert_session_output(&output.stdout, &config, ids);
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn controlled_child_completes_v2_imagination_query_observation_replay_and_close() {
    let config = LocalFrameConfig::default();
    let ids = SessionIds {
        entity: StableEntityId::new(15).unwrap(),
        camera: StableEntityId::new(16).unwrap(),
        observation: ObservationId::new(17).unwrap(),
    };
    let input = encode_v2_session_input(&config, ids);

    let output = run_with_input(&input, CHILD_TIMEOUT);
    assert!(
        output.status.success(),
        "{}",
        normalize(output.stderr.clone())
    );
    assert!(output.stderr.is_empty());
    assert_v2_session_output(&output.stdout, &config, ids);
}

#[derive(Clone, Copy)]
struct SessionIds {
    entity: StableEntityId,
    camera: StableEntityId,
    observation: ObservationId,
}

fn encode_session_input(config: &LocalFrameConfig, ids: SessionIds) -> Vec<u8> {
    let frames = [
        client(
            1,
            LocalSessionClientKind::Hello(ClientHello {
                receive_limits: LocalSessionLimits::from_config(config).unwrap(),
                compilation_receive_limits: None,
            }),
            config,
        ),
        client(
            2,
            LocalSessionClientKind::SubmitPatch(SubmitPatch {
                patch: scene_patch(ids.entity, ids.camera),
            }),
            config,
        ),
        client(
            3,
            LocalSessionClientKind::Query(QueryRequest {
                query: SceneQuery {
                    schema_version: SchemaVersion::V1,
                    scene_revision: SceneRevision::new(1),
                    entity_ids: vec![ids.entity],
                    component_kinds: Vec::new(),
                    limit: NonZeroU32::new(1).unwrap(),
                },
            }),
            config,
        ),
        client(
            4,
            LocalSessionClientKind::RequestObservation(RequestObservation {
                request: ObservationRequest {
                    schema_version: SchemaVersion::V1,
                    observation_id: ids.observation,
                    scene_revision: SceneRevision::new(1),
                    camera_id: ids.camera,
                    kind: ObservationKind::Visibility,
                    quality: ObservationQuality::Low,
                },
            }),
            config,
        ),
        client(5, LocalSessionClientKind::Close(SessionClose {}), config),
    ];
    frames
        .iter()
        .flat_map(|frame| encode_frame(frame, config).unwrap())
        .collect()
}

fn encode_v2_session_input(config: &LocalFrameConfig, ids: SessionIds) -> Vec<u8> {
    let request = imagination(ids.entity);
    let frames = [
        client_v2(
            1,
            LocalSessionClientKind::Hello(ClientHello {
                receive_limits: LocalSessionLimits::from_config(config).unwrap(),
                compilation_receive_limits: Some(CompilationLimits::default()),
            }),
            config,
        ),
        client_v2(
            2,
            LocalSessionClientKind::SubmitImagination(SubmitImagination {
                imagination: request.clone(),
            }),
            config,
        ),
        client_v2(
            3,
            LocalSessionClientKind::SubmitPatch(SubmitPatch {
                patch: camera_patch(ids.camera),
            }),
            config,
        ),
        client_v2(
            4,
            LocalSessionClientKind::Query(QueryRequest {
                query: SceneQuery {
                    schema_version: SchemaVersion::V1,
                    scene_revision: SceneRevision::new(2),
                    entity_ids: vec![ids.entity],
                    component_kinds: Vec::new(),
                    limit: NonZeroU32::new(1).unwrap(),
                },
            }),
            config,
        ),
        client_v2(
            5,
            LocalSessionClientKind::RequestObservation(RequestObservation {
                request: ObservationRequest {
                    schema_version: SchemaVersion::V1,
                    observation_id: ids.observation,
                    scene_revision: SceneRevision::new(2),
                    camera_id: ids.camera,
                    kind: ObservationKind::Visibility,
                    quality: ObservationQuality::Low,
                },
            }),
            config,
        ),
        client_v2(
            6,
            LocalSessionClientKind::SubmitImagination(SubmitImagination {
                imagination: request,
            }),
            config,
        ),
        client_v2(7, LocalSessionClientKind::Close(SessionClose {}), config),
    ];
    frames
        .iter()
        .flat_map(|frame| encode_frame(frame, config).unwrap())
        .collect()
}

fn assert_v2_session_output(output: &[u8], config: &LocalFrameConfig, ids: SessionIds) {
    let limits = CompilationLimits::default();
    let mut bytes = output;
    let hello_frame = read_frame(&mut bytes, config).unwrap().unwrap();
    assert_eq!(hello_frame.correlation_id().get(), 1);
    let (_, hello) =
        decode_server_control_frame_with_limits(&hello_frame, config, &limits).unwrap();
    let LocalSessionServerKind::Hello(hello) = hello.message else {
        panic!("expected version-two server hello");
    };
    assert_eq!(hello.effective_compilation_limits, Some(limits));
    let effective_config = hello.effective_limits.to_frame_config().unwrap();

    let imagination_admission = read_frame(&mut bytes, &effective_config).unwrap().unwrap();
    assert_eq!(imagination_admission.correlation_id().get(), 2);
    let (_, imagination_admission) =
        decode_server_control_frame_with_limits(&imagination_admission, &effective_config, &limits)
            .unwrap();
    assert!(matches!(
        imagination_admission.message,
        LocalSessionServerKind::ImaginationAdmission(admission)
            if admission.status == ImaginationAdmissionStatus::Queued
    ));
    let imagination_completion = read_frame(&mut bytes, &effective_config).unwrap().unwrap();
    assert_eq!(imagination_completion.correlation_id().get(), 2);
    let (_, imagination_completion) = decode_server_control_frame_with_limits(
        &imagination_completion,
        &effective_config,
        &limits,
    )
    .unwrap();
    let LocalSessionServerKind::ImaginationCompleted(first_completion) =
        imagination_completion.message
    else {
        panic!("expected imagination completion");
    };
    assert_eq!(
        first_completion.compilation.imagination_id,
        ImaginationId::new(101).unwrap()
    );
    assert!(first_completion.compilation.unresolved.is_empty());
    assert!(matches!(
        first_completion.receipt,
        Some(ref receipt)
            if receipt.status == ApplyStatus::Applied
                && receipt.new_revision == SceneRevision::new(1)
    ));

    assert_patch_v2(&mut bytes, &effective_config, &limits);
    assert_query_v2(&mut bytes, &effective_config, &limits, ids.entity);
    assert_observation_v2(&mut bytes, &effective_config, &limits, ids);

    let replay_frame = read_frame(&mut bytes, &effective_config).unwrap().unwrap();
    assert_eq!(replay_frame.correlation_id().get(), 6);
    let (_, replay) =
        decode_server_control_frame_with_limits(&replay_frame, &effective_config, &limits).unwrap();
    let LocalSessionServerKind::ImaginationAdmission(admission) = replay.message else {
        panic!("expected replay admission");
    };
    let ImaginationAdmissionStatus::Replayed { completion } = admission.status else {
        panic!("expected retained imagination replay");
    };
    assert_eq!(completion.compilation, first_completion.compilation);
    assert!(matches!(
        completion.receipt,
        Some(receipt) if receipt.status == ApplyStatus::IdempotentReplay
    ));

    let close_frame = read_frame(&mut bytes, &effective_config).unwrap().unwrap();
    assert_eq!(close_frame.correlation_id().get(), 7);
    let (_, close) =
        decode_server_control_frame_with_limits(&close_frame, &effective_config, &limits).unwrap();
    assert!(matches!(close.message, LocalSessionServerKind::Closed(_)));
    assert!(read_frame(&mut bytes, &effective_config).unwrap().is_none());
}

fn assert_patch_v2(bytes: &mut &[u8], config: &LocalFrameConfig, limits: &CompilationLimits) {
    let admission = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(admission.correlation_id().get(), 3);
    let (_, admission) =
        decode_server_control_frame_with_limits(&admission, config, limits).unwrap();
    assert!(matches!(
        admission.message,
        LocalSessionServerKind::PatchAdmission(admission)
            if admission.status == PatchAdmissionStatus::Queued
    ));
    let completion = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(completion.correlation_id().get(), 3);
    let (_, completion) =
        decode_server_control_frame_with_limits(&completion, config, limits).unwrap();
    assert!(matches!(
        completion.message,
        LocalSessionServerKind::PatchCompleted(completion)
            if completion.receipt.status == ApplyStatus::Applied
                && completion.receipt.new_revision == SceneRevision::new(2)
    ));
}

fn assert_query_v2(
    bytes: &mut &[u8],
    config: &LocalFrameConfig,
    limits: &CompilationLimits,
    entity_id: StableEntityId,
) {
    let frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(frame.correlation_id().get(), 4);
    let (_, result) = decode_server_control_frame_with_limits(&frame, config, limits).unwrap();
    assert!(matches!(
        result.message,
        LocalSessionServerKind::QueryResult(result)
            if result.result.scene_revision == SceneRevision::new(2)
                && result.result.entities.len() == 1
                && result.result.entities[0].entity_id == entity_id
    ));
}

fn assert_observation_v2(
    bytes: &mut &[u8],
    config: &LocalFrameConfig,
    limits: &CompilationLimits,
    ids: SessionIds,
) {
    let accepted = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(accepted.correlation_id().get(), 5);
    let (_, accepted) = decode_server_control_frame_with_limits(&accepted, config, limits).unwrap();
    assert!(matches!(
        accepted.message,
        LocalSessionServerKind::ObservationAccepted(reference)
            if reference.observation_id == ids.observation
                && reference.scene_revision == SceneRevision::new(2)
    ));
    let mut pending = 0;
    loop {
        let frame = read_frame(bytes, config).unwrap().unwrap();
        assert_eq!(frame.correlation_id().get(), 5);
        match frame {
            LocalFrame::Observation {
                metadata, payload, ..
            } => {
                assert_eq!(metadata.scene_revision, SceneRevision::new(2));
                assert!(matches!(payload, ObservationPayload::Visibility(_)));
                break;
            }
            frame @ LocalFrame::Control { .. } => {
                let (_, message) =
                    decode_server_control_frame_with_limits(&frame, config, limits).unwrap();
                assert!(matches!(
                    message.message,
                    LocalSessionServerKind::ObservationPending(_)
                ));
                pending += 1;
                assert_eq!(pending, 1);
            }
        }
    }
}

fn assert_session_output(output: &[u8], config: &LocalFrameConfig, ids: SessionIds) {
    let mut bytes = output;
    let effective_config = assert_hello(&mut bytes, config);
    assert_patch(&mut bytes, &effective_config);
    assert_query(&mut bytes, &effective_config, ids.entity);
    assert_observation(&mut bytes, &effective_config, ids);
    assert_close(&mut bytes, &effective_config);
    assert!(read_frame(&mut bytes, &effective_config).unwrap().is_none());
}

fn assert_hello(bytes: &mut &[u8], config: &LocalFrameConfig) -> LocalFrameConfig {
    let hello_frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(hello_frame.correlation_id().get(), 1);
    let (_, hello) = decode_server_control_frame(&hello_frame, config).unwrap();
    let LocalSessionServerKind::Hello(hello) = hello.message else {
        panic!("expected server hello");
    };
    hello.effective_limits.to_frame_config().unwrap()
}

fn assert_patch(bytes: &mut &[u8], config: &LocalFrameConfig) {
    let admission_frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(admission_frame.correlation_id().get(), 2);
    let (_, admission) = decode_server_control_frame(&admission_frame, config).unwrap();
    assert!(matches!(
        admission.message,
        LocalSessionServerKind::PatchAdmission(admission)
            if admission.status == PatchAdmissionStatus::Queued
    ));

    let completion_frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(completion_frame.correlation_id().get(), 2);
    let (_, completion) = decode_server_control_frame(&completion_frame, config).unwrap();
    assert!(matches!(
        completion.message,
        LocalSessionServerKind::PatchCompleted(completion)
            if completion.receipt.status == ApplyStatus::Applied
                && completion.receipt.new_revision == SceneRevision::new(1)
    ));
}

fn assert_query(bytes: &mut &[u8], config: &LocalFrameConfig, entity_id: StableEntityId) {
    let query_frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(query_frame.correlation_id().get(), 3);
    let (_, query) = decode_server_control_frame(&query_frame, config).unwrap();
    assert!(matches!(
        query.message,
        LocalSessionServerKind::QueryResult(query)
            if query.result.scene_revision == SceneRevision::new(1)
                && query.result.entities.len() == 1
                && query.result.entities[0].entity_id == entity_id
    ));
}

fn assert_observation(bytes: &mut &[u8], config: &LocalFrameConfig, ids: SessionIds) {
    let accepted_frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(accepted_frame.correlation_id().get(), 4);
    let (_, accepted) = decode_server_control_frame(&accepted_frame, config).unwrap();
    assert!(matches!(
        accepted.message,
        LocalSessionServerKind::ObservationAccepted(reference)
            if reference.observation_id == ids.observation
                && reference.scene_revision == SceneRevision::new(1)
    ));

    let mut pending_count = 0;
    let observation = loop {
        let frame = read_frame(bytes, config).unwrap().unwrap();
        assert_eq!(frame.correlation_id().get(), 4);
        match frame {
            LocalFrame::Observation {
                metadata, payload, ..
            } => break (metadata, payload),
            frame @ LocalFrame::Control { .. } => {
                let (_, message) = decode_server_control_frame(&frame, config).unwrap();
                assert!(matches!(
                    message.message,
                    LocalSessionServerKind::ObservationPending(reference)
                        if reference.observation_id == ids.observation
                            && reference.scene_revision == SceneRevision::new(1)
                ));
                pending_count += 1;
                assert_eq!(pending_count, 1);
            }
        }
    };
    assert_eq!(observation.0.observation_id, ids.observation);
    assert_eq!(observation.0.scene_revision, SceneRevision::new(1));
    assert_eq!(observation.0.camera_id, ids.camera);
    assert_eq!(observation.0.kind, ObservationKind::Visibility);
    assert!(matches!(observation.1, ObservationPayload::Visibility(_)));
}

fn assert_close(bytes: &mut &[u8], config: &LocalFrameConfig) {
    let closed_frame = read_frame(bytes, config).unwrap().unwrap();
    assert_eq!(closed_frame.correlation_id().get(), 5);
    let (_, closed) = decode_server_control_frame(&closed_frame, config).unwrap();
    assert!(matches!(closed.message, LocalSessionServerKind::Closed(_)));
}

fn run_with_input(input: &[u8], timeout: Duration) -> Output {
    let mut child = command()
        .arg("serve-stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input).unwrap();
    wait_with_timeout(child, timeout)
}

fn wait_with_timeout(mut child: Child, timeout: Duration) -> Output {
    let deadline = Instant::now() + timeout;
    loop {
        if child.try_wait().unwrap().is_some() {
            return child.wait_with_output().unwrap();
        }
        if Instant::now() >= deadline {
            child.kill().unwrap();
            let output = child.wait_with_output().unwrap();
            panic!("serve-stdio child timed out: {}", normalize(output.stderr));
        }
        thread::sleep(Duration::from_millis(5));
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

fn client_v2(
    correlation_id: u64,
    message: LocalSessionClientKind,
    config: &LocalFrameConfig,
) -> LocalFrame {
    client_control_frame(
        NonZeroU64::new(correlation_id).unwrap(),
        &LocalSessionClientMessage {
            schema_version: LOCAL_SESSION_SCHEMA_VERSION_V2,
            message,
        },
        config,
    )
    .unwrap()
}

fn imagination(entity_id: StableEntityId) -> ImaginationEnvelope {
    ImaginationEnvelope {
        schema_version: SchemaVersion::V1,
        imagination_id: ImaginationId::new(101).unwrap(),
        transaction_id: TransactionId::new(102).unwrap(),
        idempotency_key: IdempotencyKey::new(103).unwrap(),
        base_revision: SceneRevision::INITIAL,
        delivery: DeliverySemantic::MustApply,
        seed: 104,
        declared_budget: ImaginationBudget::default(),
        entities: vec![ImaginedEntity {
            key: SceneText::new("table").unwrap(),
            preferred_id: Some(entity_id),
            name: None,
            primitive: PrimitiveComponent {
                shape: PrimitiveShape::Cuboid,
                dimensions: PositiveVec3 {
                    x: positive(1.0),
                    y: positive(1.0),
                    z: positive(1.0),
                },
            },
            transform: None,
            material: None,
        }],
        relations: Vec::new(),
        constraints: Vec::new(),
    }
}

fn camera_patch(camera_id: StableEntityId) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(112).unwrap(),
        idempotency_key: IdempotencyKey::new(113).unwrap(),
        base_revision: SceneRevision::new(1),
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::Create(CreateEntity {
            entity_id: camera_id,
            components: vec![
                ComponentValue::LocalTransform(transform(3.0)),
                ComponentValue::Camera(CameraComponent {
                    vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                    near: positive(0.1),
                    far: positive(100.0),
                }),
            ],
        })],
    }
}

fn scene_patch(entity_id: StableEntityId, camera_id: StableEntityId) -> ScenePatch {
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

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

fn normalize(bytes: Vec<u8>) -> String {
    String::from_utf8(bytes).unwrap().replace("\r\n", "\n")
}

fn command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_cogniform-cli"))
}
