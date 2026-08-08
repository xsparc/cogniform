//! Contract, validation, and canonical JSON fixtures for protocol schema v1.

use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use cogniform_protocol::{
    ApplyReceipt, ApplyStatus, ApplyTiming, CameraComponent, CodecError, ColorRgba, ComponentValue,
    ConflictPolicy, CreateEntity, DeliverySemantic, Diagnostic, DiagnosticCode, DiagnosticSeverity,
    FiniteF32, FrameId, IdempotencyKey, ImageDimensions, LocalTransform, MaterialComponent,
    NameComponent, ObservationId, ObservationKind, ObservationMetadata, ObservationQuality,
    ObservationRequest, ObservationStaleness, PatchBudget, PositiveF32, PositiveVec3, Quaternion,
    QueueConfig, RuntimeLimits, SceneOperation, ScenePatch, SceneRevision, SceneText,
    SchemaVersion, SetComponent, StableEntityId, TransactionId, UnitF32, ValueErrorKind, Vec3,
};

const PATCH_FIXTURE: &[u8] = include_bytes!("fixtures/scene_patch_v1.json");
const RECEIPT_FIXTURE: &[u8] = include_bytes!("fixtures/apply_receipt_v1.json");
const OBSERVATION_FIXTURE: &[u8] = include_bytes!("fixtures/observation_metadata_v1.json");
const OBSERVATION_REQUEST_FIXTURE: &[u8] = include_bytes!("fixtures/observation_request_v1.json");

fn stable_id(value: u128) -> StableEntityId {
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

fn sample_patch() -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(0x10).unwrap(),
        idempotency_key: IdempotencyKey::new(0x20).unwrap(),
        base_revision: SceneRevision::new(7),
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::LatestWins {
            supersession_key: SceneText::new("scene/table").unwrap(),
        },
        declared_budget: PatchBudget::default(),
        operations: vec![
            SceneOperation::Create(CreateEntity {
                entity_id: stable_id(1),
                components: vec![
                    ComponentValue::Name(NameComponent {
                        value: SceneText::new("table").unwrap(),
                    }),
                    ComponentValue::LocalTransform(LocalTransform {
                        translation: Vec3 {
                            x: finite(0.0),
                            y: finite(1.0),
                            z: finite(0.0),
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
                    }),
                ],
            }),
            SceneOperation::SetComponent(SetComponent {
                entity_id: stable_id(1),
                component: ComponentValue::Material(MaterialComponent {
                    base_color: ColorRgba {
                        r: unit(0.4),
                        g: unit(0.2),
                        b: unit(0.1),
                        a: unit(1.0),
                    },
                    metallic: unit(0.0),
                    roughness: unit(0.7),
                }),
            }),
            SceneOperation::Reparent(cogniform_protocol::ReparentEntity {
                entity_id: stable_id(1),
                parent_id: Some(stable_id(2)),
            }),
        ],
    }
}

fn sample_receipt() -> ApplyReceipt {
    ApplyReceipt {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(0x10).unwrap(),
        idempotency_key: IdempotencyKey::new(0x20).unwrap(),
        status: ApplyStatus::Applied,
        previous_revision: SceneRevision::new(7),
        new_revision: SceneRevision::new(8),
        operation_count: NonZeroU32::new(3).unwrap(),
        diagnostics: Vec::new(),
        timing: ApplyTiming {
            decode_micros: 12,
            validate_micros: 34,
            commit_micros: 56,
        },
        estimated_visible_frame: FrameId::new(42).unwrap(),
    }
}

fn sample_observation() -> ObservationMetadata {
    ObservationMetadata {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(0x30).unwrap(),
        scene_revision: SceneRevision::new(8),
        frame_id: FrameId::new(42).unwrap(),
        camera_id: stable_id(3),
        kind: ObservationKind::EntityId,
        dimensions: Some(ImageDimensions {
            width: NonZeroU32::new(64).unwrap(),
            height: NonZeroU32::new(32).unwrap(),
        }),
        quality: ObservationQuality::Low,
        observed_at_unix_micros: 1_725_000_000_000_000,
        production_latency_micros: 2_500,
        staleness: ObservationStaleness {
            latest_known_revision: SceneRevision::new(10),
            revisions_behind: 2,
        },
    }
}

fn sample_observation_request() -> ObservationRequest {
    ObservationRequest {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(0x30).unwrap(),
        scene_revision: SceneRevision::new(8),
        camera_id: stable_id(3),
        kind: ObservationKind::EntityId,
        quality: ObservationQuality::Low,
    }
}

#[test]
fn canonical_fixtures_are_byte_stable_and_round_trip() {
    let limits = RuntimeLimits::default();
    let patch = sample_patch();
    let receipt = sample_receipt();
    let observation = sample_observation();

    assert_eq!(patch.to_canonical_json(&limits).unwrap(), PATCH_FIXTURE);
    assert_eq!(receipt.to_canonical_json(&limits).unwrap(), RECEIPT_FIXTURE);
    assert_eq!(
        observation.to_canonical_json(&limits).unwrap(),
        OBSERVATION_FIXTURE
    );

    assert_eq!(
        ScenePatch::from_json(PATCH_FIXTURE, &limits).unwrap(),
        patch
    );
    assert_eq!(
        ApplyReceipt::from_json(RECEIPT_FIXTURE, &limits).unwrap(),
        receipt
    );
    assert_eq!(
        ObservationMetadata::from_json(OBSERVATION_FIXTURE, &limits).unwrap(),
        observation
    );
    let observation_request = sample_observation_request();
    assert_eq!(
        observation_request.to_canonical_json(&limits).unwrap(),
        OBSERVATION_REQUEST_FIXTURE
    );
    assert_eq!(
        ObservationRequest::from_json(OBSERVATION_REQUEST_FIXTURE, &limits).unwrap(),
        observation_request
    );
}

#[test]
fn canonical_encoding_is_repeatable_and_preserves_operation_order() {
    let limits = RuntimeLimits::default();
    let patch = sample_patch();
    let first = patch.to_canonical_json(&limits).unwrap();
    let second = patch.to_canonical_json(&limits).unwrap();
    assert_eq!(first, second);

    let decoded = ScenePatch::from_json(&first, &limits).unwrap();
    assert!(matches!(decoded.operations[0], SceneOperation::Create(_)));
    assert!(matches!(
        decoded.operations[1],
        SceneOperation::SetComponent(_)
    ));
    assert!(matches!(decoded.operations[2], SceneOperation::Reparent(_)));
}

#[test]
fn unknown_and_duplicate_fields_fail_closed() {
    let limits = RuntimeLimits::default();
    let fixture = String::from_utf8(PATCH_FIXTURE.to_vec()).unwrap();
    let unknown = fixture.replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"future_field\":true",
        1,
    );
    let duplicate = fixture.replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    let nested_unknown = fixture.replacen(
        "\"entity_id\":\"00000000000000000000000000000001\",\"components\"",
        "\"entity_id\":\"00000000000000000000000000000001\",\"unexpected\":true,\"components\"",
        1,
    );

    assert!(matches!(
        ScenePatch::from_json(unknown.as_bytes(), &limits),
        Err(CodecError::MalformedJson { .. })
    ));
    assert!(matches!(
        ScenePatch::from_json(duplicate.as_bytes(), &limits),
        Err(CodecError::MalformedJson { .. })
    ));
    assert!(matches!(
        ScenePatch::from_json(nested_unknown.as_bytes(), &limits),
        Err(CodecError::MalformedJson { .. })
    ));
}

#[test]
fn encoded_size_and_nesting_are_bounded_before_decode() {
    let mut limits = RuntimeLimits {
        max_encoded_bytes: NonZeroU64::new(8).unwrap(),
        ..RuntimeLimits::default()
    };
    assert!(matches!(
        ScenePatch::from_json(PATCH_FIXTURE, &limits),
        Err(CodecError::EncodedSizeExceeded { .. })
    ));

    limits.max_encoded_bytes = RuntimeLimits::default().max_encoded_bytes;
    limits.max_json_nesting_depth = NonZeroU16::new(2).unwrap();
    assert!(matches!(
        ScenePatch::from_json(PATCH_FIXTURE, &limits),
        Err(CodecError::NestingLimitExceeded { .. })
    ));

    limits.max_json_nesting_depth = RuntimeLimits::default().max_json_nesting_depth;
    limits.max_decoded_bytes = NonZeroU64::new(8).unwrap();
    assert_eq!(
        sample_patch()
            .validate_with_limits(&limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::DecodedSizeLimitExceeded
    );
}

#[test]
fn identifiers_and_floats_reject_noncanonical_values() {
    assert_eq!(
        StableEntityId::new(0).unwrap_err().kind(),
        ValueErrorKind::ZeroIdentifier
    );
    assert_eq!(
        "0000000000000000000000000000000A"
            .parse::<StableEntityId>()
            .unwrap_err()
            .kind(),
        ValueErrorKind::InvalidIdentifierEncoding
    );
    assert_eq!(
        FiniteF32::new(f32::NAN).unwrap_err().kind(),
        ValueErrorKind::NonFiniteNumber
    );
    assert_eq!(
        FiniteF32::new(-0.0).unwrap().get().to_bits(),
        0.0_f32.to_bits()
    );

    let limits = RuntimeLimits::default();
    let fixture = String::from_utf8(PATCH_FIXTURE.to_vec()).unwrap();
    let zero_id = fixture.replacen(
        "00000000000000000000000000000001",
        "00000000000000000000000000000000",
        1,
    );
    assert!(matches!(
        ScenePatch::from_json(zero_id.as_bytes(), &limits),
        Err(CodecError::MalformedJson { .. })
    ));
}

#[test]
fn patches_enforce_schema_empty_and_declared_runtime_limits() {
    let limits = RuntimeLimits::default();
    let mut patch = sample_patch();
    patch.operations.clear();
    assert_eq!(
        patch.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::EmptyPatch
    );

    let mut patch = sample_patch();
    patch.schema_version = SchemaVersion::new(2).unwrap();
    assert_eq!(
        patch.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::UnsupportedSchema
    );

    let patch = sample_patch();
    let low_operation_limits = RuntimeLimits {
        max_operations: NonZeroU32::new(2).unwrap(),
        ..limits
    };
    assert_eq!(
        patch
            .validate_with_limits(&low_operation_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::OperationLimitExceeded
    );

    let low_text_limits = RuntimeLimits {
        max_text_bytes: NonZeroU64::new(4).unwrap(),
        ..limits
    };
    assert_eq!(
        patch
            .validate_with_limits(&low_text_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::TextLimitExceeded
    );
}

#[test]
fn patch_component_invariants_fail_with_typed_diagnostics() {
    let limits = RuntimeLimits::default();
    let mut patch = sample_patch();
    if let SceneOperation::Create(create) = &mut patch.operations[0] {
        create.components.push(create.components[0].clone());
    }
    assert_eq!(
        patch.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::DuplicateComponent
    );

    let mut patch = sample_patch();
    patch
        .operations
        .push(SceneOperation::SetComponent(SetComponent {
            entity_id: stable_id(1),
            component: ComponentValue::Camera(CameraComponent {
                vertical_fov_radians: positive(core::f32::consts::PI),
                near: positive(0.1),
                far: positive(100.0),
            }),
        }));
    assert_eq!(
        patch.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::InvalidComponentValue
    );
}

#[test]
fn component_and_diagnostic_collections_are_bounded() {
    let limits = RuntimeLimits::default();
    let patch = sample_patch();
    let low_component_limits = RuntimeLimits {
        max_components: NonZeroU32::new(2).unwrap(),
        ..limits
    };
    assert_eq!(
        patch
            .validate_with_limits(&low_component_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::ComponentLimitExceeded
    );

    let per_entity_limits = RuntimeLimits {
        max_components_per_entity: NonZeroU32::new(1).unwrap(),
        ..limits
    };
    assert_eq!(
        patch
            .validate_with_limits(&per_entity_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::ComponentLimitExceeded
    );

    let diagnostic = Diagnostic {
        code: DiagnosticCode::InvalidComponentValue,
        severity: DiagnosticSeverity::Warning,
        message: SceneText::new("bounded detail").unwrap(),
        operation_index: Some(1),
        entity_id: Some(stable_id(1)),
    };
    let mut receipt = sample_receipt();
    receipt.diagnostics = vec![diagnostic.clone(), diagnostic];
    let low_diagnostic_limits = RuntimeLimits {
        max_diagnostics: NonZeroU32::new(1).unwrap(),
        ..limits
    };
    assert_eq!(
        receipt
            .validate_with_limits(&low_diagnostic_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::DiagnosticLimitExceeded
    );

    let low_text_limits = RuntimeLimits {
        max_text_bytes: NonZeroU64::new(10).unwrap(),
        ..limits
    };
    assert_eq!(
        receipt
            .validate_with_limits(&low_text_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::TextLimitExceeded
    );
}

#[test]
fn receipts_require_one_revision_transition_and_bounded_diagnostics() {
    let limits = RuntimeLimits::default();
    let mut receipt = sample_receipt();
    receipt.new_revision = SceneRevision::new(9);
    assert_eq!(
        receipt.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::InvalidReceiptRevision
    );
    assert_eq!(
        SceneRevision::new(u64::MAX)
            .checked_next()
            .unwrap_err()
            .kind(),
        ValueErrorKind::RevisionOverflow
    );
}

#[test]
fn latest_wins_requires_a_key_and_queue_capacity_is_bounded() {
    let limits = RuntimeLimits::default();
    let fixture = String::from_utf8(PATCH_FIXTURE.to_vec()).unwrap();
    let missing_key = fixture.replacen(
        "{\"mode\":\"latest_wins\",\"supersession_key\":\"scene/table\"}",
        "{\"mode\":\"latest_wins\"}",
        1,
    );
    assert!(matches!(
        ScenePatch::from_json(missing_key.as_bytes(), &limits),
        Err(CodecError::MalformedJson { .. })
    ));

    let queue = QueueConfig {
        capacity: NonZeroU32::new(1_025).unwrap(),
        delivery: DeliverySemantic::MustApply,
    };
    let error = queue.validate_with_limits(&limits).unwrap_err();
    assert_eq!(error.code(), DiagnosticCode::QueueCapacityExceeded);
    assert_eq!(error.field(), "queue.capacity");
}

#[test]
fn observations_enforce_dimensions_pixels_and_revision_causality() {
    let limits = RuntimeLimits::default();
    let mut normal = sample_observation();
    normal.kind = ObservationKind::Normal;
    let encoded = normal.to_canonical_json(&limits).unwrap();
    let normal_kind = b"\"kind\":\"normal\"";
    assert!(
        encoded
            .windows(normal_kind.len())
            .any(|window| window == normal_kind)
    );
    assert_eq!(
        ObservationMetadata::from_json(&encoded, &limits).unwrap(),
        normal
    );

    let mut observation = sample_observation();
    observation.dimensions = None;
    assert_eq!(
        observation
            .validate_with_limits(&limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::InvalidObservationDimensions
    );

    let mut observation = sample_observation();
    observation.staleness.revisions_behind = 1;
    assert_eq!(
        observation
            .validate_with_limits(&limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::InvalidObservationStaleness
    );

    let small_pixel_limits = RuntimeLimits {
        max_observation_pixels: NonZeroU64::new(1_000).unwrap(),
        ..limits
    };
    assert_eq!(
        sample_observation()
            .validate_with_limits(&small_pixel_limits)
            .unwrap_err()
            .code(),
        DiagnosticCode::ObservationPixelLimitExceeded
    );
}
