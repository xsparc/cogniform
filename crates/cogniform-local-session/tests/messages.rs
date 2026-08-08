//! Direction, canonical-byte, bound, and nested-value contracts for CF040.

use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use cogniform_local_session::{
    ClientHello, LOCAL_SESSION_SCHEMA_VERSION, LocalSessionClientKind, LocalSessionClientMessage,
    LocalSessionError, LocalSessionLimits, LocalSessionServerKind, LocalSessionServerMessage,
    LocalSessionValidationKind, ObservationReference, PatchAdmission, PatchAdmissionStatus,
    PatchCompletion, QueryRequest, QueryResponse, RequestObservation, ServerHello, SessionClose,
    SessionClosed, SessionFailure, SessionFailureCode, SubmitPatch, client_control_frame,
    decode_client_control_frame, decode_client_message, decode_server_control_frame,
    decode_server_message, encode_client_message, encode_server_message, server_control_frame,
};
use cogniform_local_transport::{
    LOCAL_FRAME_HEADER_BYTES, LocalFrame, LocalFrameConfig, LocalFrameLimits,
};
use cogniform_observation::ObservationPayload;
use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ApplyTiming, ConflictPolicy, DeleteEntity, DeliverySemantic,
    FrameId, IdempotencyKey, ObservationId, ObservationKind, ObservationMetadata,
    ObservationQuality, ObservationRequest, ObservationStaleness, PatchBudget, RuntimeLimits,
    SceneOperation, ScenePatch, SceneQuery, SceneQueryResult, SceneRevision, SchemaVersion,
    StableEntityId, TransactionId,
};

const CLIENT_HELLO_FIXTURE: &[u8] = include_bytes!("fixtures/client_hello_v1.json");
const CLIENT_SUBMIT_PATCH_FIXTURE: &[u8] = include_bytes!("fixtures/client_submit_patch_v1.json");
const CLIENT_QUERY_FIXTURE: &[u8] = include_bytes!("fixtures/client_query_v1.json");
const CLIENT_REQUEST_OBSERVATION_FIXTURE: &[u8] =
    include_bytes!("fixtures/client_request_observation_v1.json");
const CLIENT_CLOSE_FIXTURE: &[u8] = include_bytes!("fixtures/client_close_v1.json");
const SERVER_HELLO_FIXTURE: &[u8] = include_bytes!("fixtures/server_hello_v1.json");
const SERVER_PATCH_ADMISSION_FIXTURE: &[u8] =
    include_bytes!("fixtures/server_patch_admission_v1.json");
const SERVER_PATCH_COMPLETED_FIXTURE: &[u8] =
    include_bytes!("fixtures/server_patch_completed_v1.json");
const SERVER_QUERY_RESULT_FIXTURE: &[u8] = include_bytes!("fixtures/server_query_result_v1.json");
const SERVER_OBSERVATION_ACCEPTED_FIXTURE: &[u8] =
    include_bytes!("fixtures/server_observation_accepted_v1.json");
const SERVER_OBSERVATION_PENDING_FIXTURE: &[u8] =
    include_bytes!("fixtures/server_observation_pending_v1.json");
const SERVER_FAILURE_FIXTURE: &[u8] = include_bytes!("fixtures/server_failure_v1.json");
const SERVER_CLOSED_FIXTURE: &[u8] = include_bytes!("fixtures/server_closed_v1.json");

fn id(value: u128) -> StableEntityId {
    StableEntityId::new(value).unwrap()
}

fn config() -> LocalFrameConfig {
    LocalFrameConfig::default()
}

fn session_limits() -> LocalSessionLimits {
    LocalSessionLimits::from_config(&config()).unwrap()
}

fn patch() -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(1).unwrap(),
        idempotency_key: IdempotencyKey::new(2).unwrap(),
        base_revision: SceneRevision::new(7),
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::Delete(DeleteEntity { entity_id: id(3) })],
    }
}

fn query() -> SceneQuery {
    SceneQuery {
        schema_version: SchemaVersion::V1,
        scene_revision: SceneRevision::new(7),
        entity_ids: vec![id(3)],
        component_kinds: Vec::new(),
        limit: NonZeroU32::new(1).unwrap(),
    }
}

fn observation_request() -> ObservationRequest {
    ObservationRequest {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(4).unwrap(),
        scene_revision: SceneRevision::new(7),
        camera_id: id(5),
        kind: ObservationKind::Visibility,
        quality: ObservationQuality::Low,
    }
}

fn receipt(status: ApplyStatus) -> ApplyReceipt {
    ApplyReceipt {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(1).unwrap(),
        idempotency_key: IdempotencyKey::new(2).unwrap(),
        status,
        previous_revision: SceneRevision::new(7),
        new_revision: SceneRevision::new(8),
        operation_count: NonZeroU32::new(1).unwrap(),
        diagnostics: Vec::new(),
        timing: ApplyTiming {
            decode_micros: 1,
            validate_micros: 2,
            commit_micros: 3,
        },
        estimated_visible_frame: FrameId::new(9).unwrap(),
    }
}

fn client(message: LocalSessionClientKind) -> LocalSessionClientMessage {
    LocalSessionClientMessage {
        schema_version: LOCAL_SESSION_SCHEMA_VERSION,
        message,
    }
}

fn server(message: LocalSessionServerKind) -> LocalSessionServerMessage {
    LocalSessionServerMessage {
        schema_version: LOCAL_SESSION_SCHEMA_VERSION,
        message,
    }
}

fn assert_client_fixture(
    message: &LocalSessionClientMessage,
    fixture: &[u8],
    config: &LocalFrameConfig,
) {
    assert_eq!(encode_client_message(message, config).unwrap(), fixture);
    assert_eq!(
        decode_client_message(fixture, config).unwrap(),
        message.clone()
    );
    let correlation_id = NonZeroU64::new(97).unwrap();
    let frame = client_control_frame(correlation_id, message, config).unwrap();
    assert_eq!(
        decode_client_control_frame(&frame, config).unwrap(),
        (correlation_id, message.clone())
    );
    assert!(matches!(frame, LocalFrame::Control { bytes, .. } if bytes == fixture));
    assert_eq!(fixture.last(), Some(&b'\n'));
}

fn assert_server_fixture(
    message: &LocalSessionServerMessage,
    fixture: &[u8],
    config: &LocalFrameConfig,
) {
    assert_eq!(encode_server_message(message, config).unwrap(), fixture);
    assert_eq!(
        decode_server_message(fixture, config).unwrap(),
        message.clone()
    );
    let correlation_id = NonZeroU64::new(98).unwrap();
    let frame = server_control_frame(correlation_id, message, config).unwrap();
    assert_eq!(
        decode_server_control_frame(&frame, config).unwrap(),
        (correlation_id, message.clone())
    );
    assert!(matches!(frame, LocalFrame::Control { bytes, .. } if bytes == fixture));
    assert_eq!(fixture.last(), Some(&b'\n'));
}

#[test]
fn exact_schema_v1_fixtures_are_stable_and_lf_terminated() {
    let config = config();
    let client_fixtures = [
        (
            client(LocalSessionClientKind::Hello(ClientHello {
                receive_limits: session_limits(),
            })),
            CLIENT_HELLO_FIXTURE,
        ),
        (
            client(LocalSessionClientKind::SubmitPatch(SubmitPatch {
                patch: patch(),
            })),
            CLIENT_SUBMIT_PATCH_FIXTURE,
        ),
        (
            client(LocalSessionClientKind::Query(QueryRequest {
                query: query(),
            })),
            CLIENT_QUERY_FIXTURE,
        ),
        (
            client(LocalSessionClientKind::RequestObservation(
                RequestObservation {
                    request: observation_request(),
                },
            )),
            CLIENT_REQUEST_OBSERVATION_FIXTURE,
        ),
        (
            client(LocalSessionClientKind::Close(SessionClose {})),
            CLIENT_CLOSE_FIXTURE,
        ),
    ];
    for (message, fixture) in client_fixtures {
        assert_client_fixture(&message, fixture, &config);
    }

    let reference = ObservationReference {
        observation_id: ObservationId::new(4).unwrap(),
        scene_revision: SceneRevision::new(7),
    };
    let server_fixtures = [
        (
            server(LocalSessionServerKind::Hello(ServerHello {
                effective_limits: session_limits(),
            })),
            SERVER_HELLO_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
                idempotency_key: IdempotencyKey::new(2).unwrap(),
                status: PatchAdmissionStatus::Queued,
            })),
            SERVER_PATCH_ADMISSION_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::PatchCompleted(PatchCompletion {
                receipt: receipt(ApplyStatus::Applied),
            })),
            SERVER_PATCH_COMPLETED_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::QueryResult(QueryResponse {
                result: SceneQueryResult {
                    schema_version: SchemaVersion::V1,
                    scene_revision: SceneRevision::new(7),
                    entities: Vec::new(),
                },
            })),
            SERVER_QUERY_RESULT_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::ObservationAccepted(reference)),
            SERVER_OBSERVATION_ACCEPTED_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::ObservationPending(reference)),
            SERVER_OBSERVATION_PENDING_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::Failure(SessionFailure {
                code: SessionFailureCode::RevisionMismatch,
            })),
            SERVER_FAILURE_FIXTURE,
        ),
        (
            server(LocalSessionServerKind::Closed(SessionClosed {})),
            SERVER_CLOSED_FIXTURE,
        ),
    ];
    for (message, fixture) in server_fixtures {
        assert_server_fixture(&message, fixture, &config);
    }
}

#[test]
fn every_client_message_round_trips_under_core_limits() {
    let config = config();
    let messages = [
        client(LocalSessionClientKind::Hello(ClientHello {
            receive_limits: session_limits(),
        })),
        client(LocalSessionClientKind::SubmitPatch(SubmitPatch {
            patch: patch(),
        })),
        client(LocalSessionClientKind::Query(QueryRequest {
            query: query(),
        })),
        client(LocalSessionClientKind::RequestObservation(
            RequestObservation {
                request: observation_request(),
            },
        )),
        client(LocalSessionClientKind::Close(SessionClose {})),
    ];
    for (index, message) in messages.into_iter().enumerate() {
        let encoded = encode_client_message(&message, &config).unwrap();
        assert_eq!(decode_client_message(&encoded, &config).unwrap(), message);
        let correlation_id = NonZeroU64::new(u64::try_from(index + 1).unwrap()).unwrap();
        let frame = client_control_frame(correlation_id, &message, &config).unwrap();
        assert_eq!(
            decode_client_control_frame(&frame, &config).unwrap(),
            (correlation_id, message)
        );
    }
}

#[test]
fn every_server_message_round_trips_under_core_limits() {
    let config = config();
    let reference = ObservationReference {
        observation_id: ObservationId::new(4).unwrap(),
        scene_revision: SceneRevision::new(7),
    };
    let replay = receipt(ApplyStatus::IdempotentReplay);
    let messages = [
        server(LocalSessionServerKind::Hello(ServerHello {
            effective_limits: session_limits(),
        })),
        server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key: IdempotencyKey::new(2).unwrap(),
            status: PatchAdmissionStatus::Queued,
        })),
        server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key: IdempotencyKey::new(2).unwrap(),
            status: PatchAdmissionStatus::AlreadyQueued,
        })),
        server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key: IdempotencyKey::new(2).unwrap(),
            status: PatchAdmissionStatus::Superseded {
                superseded_idempotency_key: IdempotencyKey::new(6).unwrap(),
            },
        })),
        server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key: IdempotencyKey::new(2).unwrap(),
            status: PatchAdmissionStatus::Dropped,
        })),
        server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
            idempotency_key: IdempotencyKey::new(2).unwrap(),
            status: PatchAdmissionStatus::Replayed { receipt: replay },
        })),
        server(LocalSessionServerKind::PatchCompleted(PatchCompletion {
            receipt: receipt(ApplyStatus::Applied),
        })),
        server(LocalSessionServerKind::QueryResult(QueryResponse {
            result: SceneQueryResult {
                schema_version: SchemaVersion::V1,
                scene_revision: SceneRevision::new(7),
                entities: Vec::new(),
            },
        })),
        server(LocalSessionServerKind::ObservationAccepted(reference)),
        server(LocalSessionServerKind::ObservationPending(reference)),
        server(LocalSessionServerKind::Failure(SessionFailure {
            code: SessionFailureCode::Internal,
        })),
        server(LocalSessionServerKind::Closed(SessionClosed {})),
    ];
    for (index, message) in messages.into_iter().enumerate() {
        let encoded = encode_server_message(&message, &config).unwrap();
        assert_eq!(decode_server_message(&encoded, &config).unwrap(), message);
        let correlation_id = NonZeroU64::new(u64::try_from(index + 1).unwrap()).unwrap();
        let frame = server_control_frame(correlation_id, &message, &config).unwrap();
        assert_eq!(
            decode_server_control_frame(&frame, &config).unwrap(),
            (correlation_id, message)
        );
    }
}

#[test]
fn direction_version_unknown_and_canonical_substitutions_are_rejected() {
    let config = config();
    assert!(matches!(
        decode_client_message(SERVER_FAILURE_FIXTURE, &config),
        Err(LocalSessionError::WrongDirection)
    ));

    let unsupported = b"{\"schema_version\":2,\"message\":{\"close\":{}}}\n";
    assert!(matches!(
        decode_client_message(unsupported, &config),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::UnsupportedVersion
    ));

    let unknown = b"{\"schema_version\":1,\"message\":{\"close\":{\"extra\":1}}}\n";
    assert!(matches!(
        decode_client_message(unknown, &config),
        Err(LocalSessionError::MalformedJson { .. })
    ));
    let unsupported_tag = b"{\"schema_version\":1,\"message\":{\"execute\":{}}}\n";
    assert!(matches!(
        decode_client_message(unsupported_tag, &config),
        Err(LocalSessionError::MalformedJson { .. })
    ));

    let no_lf = &CLIENT_CLOSE_FIXTURE[..CLIENT_CLOSE_FIXTURE.len() - 1];
    assert!(matches!(
        decode_client_message(no_lf, &config),
        Err(LocalSessionError::NonCanonicalMessage)
    ));
    let spaced = b"{ \"schema_version\":1,\"message\":{\"close\":{}}}\n";
    assert!(matches!(
        decode_client_message(spaced, &config),
        Err(LocalSessionError::NonCanonicalMessage)
    ));
    let truncated = &CLIENT_CLOSE_FIXTURE[..CLIENT_CLOSE_FIXTURE.len() - 4];
    assert!(matches!(
        decode_client_message(truncated, &config),
        Err(LocalSessionError::MalformedJson { .. })
    ));
    let trailing = b"{\"schema_version\":1,\"message\":{\"close\":{}}}\n{}";
    assert!(matches!(
        decode_client_message(trailing, &config),
        Err(LocalSessionError::MalformedJson { .. })
    ));
}

#[test]
fn decode_errors_do_not_retain_or_render_control_payloads() {
    let config = config();
    let encoded =
        b"{\"schema_version\":1,\"message\":{\"close\":{\"private_value\":\"do-not-retain\"}}}\n";
    let error = decode_client_message(encoded, &config).unwrap_err();
    let rendered = format!("{error:?} {error}");
    assert!(!rendered.contains("private_value"));
    assert!(!rendered.contains("do-not-retain"));
}

#[test]
fn nesting_and_effective_control_limits_precede_decoding_and_allocation() {
    let mut shallow = config();
    shallow.runtime_limits.max_json_nesting_depth = NonZeroU16::new(2).unwrap();
    assert!(matches!(
        decode_client_message(CLIENT_CLOSE_FIXTURE, &shallow),
        Err(LocalSessionError::NestingLimitExceeded {
            actual: 3,
            limit: 2
        })
    ));

    let mut small = config();
    small.frame_limits = LocalFrameLimits::new(
        small.frame_limits.max_frame_bytes,
        NonZeroU64::new(16).unwrap(),
        small.frame_limits.max_bulk_bytes,
    );
    assert!(matches!(
        decode_client_message(CLIENT_CLOSE_FIXTURE, &small),
        Err(LocalSessionError::MessageLimitExceeded { limit: 16, .. })
    ));
    assert!(matches!(
        encode_client_message(
            &client(LocalSessionClientKind::Close(SessionClose {})),
            &small
        ),
        Err(LocalSessionError::MessageLimitExceeded { limit: 16, .. })
    ));
}

#[test]
fn nested_core_values_and_receipt_roles_are_validated() {
    let config = config();
    let mut invalid_request = observation_request();
    invalid_request.schema_version = SchemaVersion::new(2).unwrap();
    let invalid_client = client(LocalSessionClientKind::RequestObservation(
        RequestObservation {
            request: invalid_request,
        },
    ));
    assert!(matches!(
        encode_client_message(&invalid_client, &config),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidProtocolValue
    ));

    let mut decoded_too_small = config.clone();
    decoded_too_small.runtime_limits.max_decoded_bytes = NonZeroU64::new(1).unwrap();
    let valid_request = client(LocalSessionClientKind::RequestObservation(
        RequestObservation {
            request: observation_request(),
        },
    ));
    assert!(matches!(
        encode_client_message(&valid_request, &decoded_too_small),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidProtocolValue
    ));

    let wrong_completion = server(LocalSessionServerKind::PatchCompleted(PatchCompletion {
        receipt: receipt(ApplyStatus::IdempotentReplay),
    }));
    assert!(matches!(
        encode_server_message(&wrong_completion, &config),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidPatchCompletion
    ));

    let mismatched_replay = server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
        idempotency_key: IdempotencyKey::new(10).unwrap(),
        status: PatchAdmissionStatus::Replayed {
            receipt: receipt(ApplyStatus::IdempotentReplay),
        },
    }));
    assert!(matches!(
        encode_server_message(&mismatched_replay, &config),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidPatchAdmission
    ));

    let self_supersession = server(LocalSessionServerKind::PatchAdmission(PatchAdmission {
        idempotency_key: IdempotencyKey::new(2).unwrap(),
        status: PatchAdmissionStatus::Superseded {
            superseded_idempotency_key: IdempotencyKey::new(2).unwrap(),
        },
    }));
    assert!(matches!(
        encode_server_message(&self_supersession, &config),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidPatchAdmission
    ));
}

#[test]
fn outer_correlation_is_preserved_without_entering_json() {
    let config = config();
    let correlation = NonZeroU64::new(77).unwrap();
    let client_message = client(LocalSessionClientKind::Close(SessionClose {}));
    let frame = client_control_frame(correlation, &client_message, &config).unwrap();
    let (decoded_correlation, decoded_message) =
        decode_client_control_frame(&frame, &config).unwrap();
    assert_eq!(decoded_correlation, correlation);
    assert_eq!(decoded_message, client_message);
    let LocalFrame::Control { bytes, .. } = frame else {
        panic!("client helper must emit a control frame");
    };
    assert!(!bytes.windows(2).any(|window| window == b"77"));

    let server_message = server(LocalSessionServerKind::Closed(SessionClosed {}));
    let frame = server_control_frame(correlation, &server_message, &config).unwrap();
    assert_eq!(
        decode_server_control_frame(&frame, &config).unwrap(),
        (correlation, server_message)
    );
}

#[test]
fn completed_observation_frames_cannot_be_substituted_for_control() {
    let config = config();
    let correlation = NonZeroU64::new(1).unwrap();
    let frame = LocalFrame::Observation {
        correlation_id: correlation,
        metadata: ObservationMetadata {
            schema_version: SchemaVersion::V1,
            observation_id: ObservationId::new(4).unwrap(),
            scene_revision: SceneRevision::new(7),
            frame_id: FrameId::new(9).unwrap(),
            camera_id: id(5),
            kind: ObservationKind::Visibility,
            dimensions: None,
            quality: ObservationQuality::Low,
            observed_at_unix_micros: 10,
            production_latency_micros: 2,
            staleness: ObservationStaleness {
                latest_known_revision: SceneRevision::new(7),
                revisions_behind: 0,
            },
        },
        payload: ObservationPayload::Visibility(Vec::new()),
    };
    assert!(matches!(
        decode_server_control_frame(&frame, &config),
        Err(LocalSessionError::WrongFrameKind)
    ));
}

#[test]
fn advertised_limits_must_be_self_consistent() {
    let mut limits = session_limits();
    limits.max_control_message_bytes = limits.max_frame_bytes;
    let hello = client(LocalSessionClientKind::Hello(ClientHello {
        receive_limits: limits,
    }));
    assert!(matches!(
        encode_client_message(&hello, &config()),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidLimits
    ));
}

#[test]
fn server_effective_limits_cannot_exceed_the_active_receive_configuration() {
    let config = config();
    let mut limits = session_limits();
    limits.max_visibility_entries =
        NonZeroU32::new(limits.max_visibility_entries.get().checked_add(1).unwrap()).unwrap();
    let hello = server(LocalSessionServerKind::Hello(ServerHello {
        effective_limits: limits,
    }));
    assert!(matches!(
        encode_server_message(&hello, &config),
        Err(LocalSessionError::InvalidMessage(error))
            if error.kind() == LocalSessionValidationKind::InvalidLimits
    ));
}

#[test]
fn derived_bulk_and_envelope_limits_are_effective_post_header_bounds() {
    let mut config = config();
    let body_bytes = 128_u64;
    config.frame_limits = LocalFrameLimits::new(
        NonZeroU64::new(u64::try_from(LOCAL_FRAME_HEADER_BYTES).unwrap() + body_bytes).unwrap(),
        NonZeroU64::new(64).unwrap(),
        NonZeroU64::new(1_024).unwrap(),
    );
    config.payload_limits.max_envelope_bytes = NonZeroU64::new(2_048).unwrap();
    let limits = LocalSessionLimits::from_config(&config).unwrap();
    assert_eq!(limits.max_bulk_bytes.get(), body_bytes);
    assert_eq!(limits.max_observation_envelope_bytes.get(), body_bytes);
}

#[test]
fn runtime_encoded_limit_is_part_of_the_effective_message_cap() {
    let mut config = config();
    config.runtime_limits = RuntimeLimits {
        max_encoded_bytes: NonZeroU64::new(32).unwrap(),
        ..config.runtime_limits
    };
    assert!(matches!(
        decode_client_message(CLIENT_CLOSE_FIXTURE, &config),
        Err(LocalSessionError::MessageLimitExceeded { limit: 32, .. })
    ));
}
