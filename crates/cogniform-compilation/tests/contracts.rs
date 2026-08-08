//! Canonical schema, invariant, and bounded-decoding contracts.

use core::num::{NonZeroU16, NonZeroU32, NonZeroU64};

use cogniform_compilation::{
    COMPILATION_SCHEMA_VERSION, CompilationCodecError, CompilationDecision,
    CompilationDecisionCode, CompilationLimits, CompilationResult, CompilationValidationKind,
    UnresolvedConstraint, UnresolvedConstraintCode,
};
use cogniform_protocol::{
    ComponentValue, ConflictPolicy, CreateEntity, DeliverySemantic, IdempotencyKey, ImaginationId,
    NameComponent, PatchBudget, SceneOperation, ScenePatch, SceneRevision, SceneText,
    SchemaVersion, StableEntityId, TransactionId,
};

const COMPILED_FIXTURE: &[u8] = include_bytes!("fixtures/compiled_result_v1.json");
const UNRESOLVED_FIXTURE: &[u8] = include_bytes!("fixtures/unresolved_result_v1.json");

fn stable_id(value: u128) -> StableEntityId {
    StableEntityId::new(value).unwrap()
}

fn text(value: &str) -> SceneText {
    SceneText::new(value).unwrap()
}

fn sample_patch() -> ScenePatch {
    ScenePatch {
        schema_version: COMPILATION_SCHEMA_VERSION,
        transaction_id: TransactionId::new(2).unwrap(),
        idempotency_key: IdempotencyKey::new(3).unwrap(),
        base_revision: SceneRevision::new(7),
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::Create(CreateEntity {
            entity_id: stable_id(4),
            components: vec![ComponentValue::Name(NameComponent {
                value: text("table"),
            })],
        })],
    }
}

fn generated_decision(key: &str, entity_id: u128) -> CompilationDecision {
    CompilationDecision {
        code: CompilationDecisionCode::GeneratedEntityId,
        entity_key: text(key),
        relation_index: None,
        entity_id: Some(stable_id(entity_id)),
    }
}

fn relation_issue(index: u32, key: &str, related: &str) -> UnresolvedConstraint {
    UnresolvedConstraint {
        code: UnresolvedConstraintCode::UnknownEntityReference,
        relation_index: Some(index),
        constraint_index: None,
        entity_key: Some(text(key)),
        related_key: Some(text(related)),
        entity_id: None,
    }
}

fn compiled_result() -> CompilationResult {
    CompilationResult {
        schema_version: COMPILATION_SCHEMA_VERSION,
        imagination_id: ImaginationId::new(1).unwrap(),
        scene_revision: SceneRevision::new(7),
        patch: Some(sample_patch()),
        decisions: vec![generated_decision("table", 4)],
        unresolved: Vec::new(),
    }
}

fn unresolved_result() -> CompilationResult {
    CompilationResult {
        schema_version: COMPILATION_SCHEMA_VERSION,
        imagination_id: ImaginationId::new(1).unwrap(),
        scene_revision: SceneRevision::new(7),
        patch: None,
        decisions: Vec::new(),
        unresolved: vec![relation_issue(0, "lamp", "table")],
    }
}

fn validation_kind(error: CompilationCodecError) -> CompilationValidationKind {
    match error {
        CompilationCodecError::InvalidResult(error) => error.kind(),
        other => panic!("expected validation error, got {other:?}"),
    }
}

#[test]
fn exact_compiled_and_unresolved_fixtures_round_trip() {
    let limits = CompilationLimits::default();
    let compiled = compiled_result();
    let unresolved = unresolved_result();

    assert_eq!(
        compiled.to_canonical_json(&limits).unwrap(),
        COMPILED_FIXTURE
    );
    assert_eq!(
        unresolved.to_canonical_json(&limits).unwrap(),
        UNRESOLVED_FIXTURE
    );
    assert_eq!(
        CompilationResult::from_canonical_json(COMPILED_FIXTURE, &limits).unwrap(),
        compiled
    );
    assert_eq!(
        CompilationResult::from_canonical_json(UNRESOLVED_FIXTURE, &limits).unwrap(),
        unresolved
    );
}

#[test]
fn schema_and_exact_canonical_bytes_fail_closed() {
    let limits = CompilationLimits::default();
    let fixture = String::from_utf8(COMPILED_FIXTURE.to_vec()).unwrap();

    let unsupported = fixture.replacen("\"schema_version\":1", "\"schema_version\":2", 1);
    assert_eq!(
        validation_kind(
            CompilationResult::from_canonical_json(unsupported.as_bytes(), &limits).unwrap_err()
        ),
        CompilationValidationKind::UnsupportedSchema
    );

    let unknown = fixture.replacen(
        "\"scene_revision\":7",
        "\"scene_revision\":7,\"future_field\":true",
        1,
    );
    assert!(matches!(
        CompilationResult::from_canonical_json(unknown.as_bytes(), &limits),
        Err(CompilationCodecError::MalformedJson { .. })
    ));

    let duplicate = fixture.replacen(
        "\"schema_version\":1",
        "\"schema_version\":1,\"schema_version\":1",
        1,
    );
    assert!(matches!(
        CompilationResult::from_canonical_json(duplicate.as_bytes(), &limits),
        Err(CompilationCodecError::MalformedJson { .. })
    ));

    let invalid_code = fixture.replacen("generated_entity_id", "future_choice", 1);
    assert!(matches!(
        CompilationResult::from_canonical_json(invalid_code.as_bytes(), &limits),
        Err(CompilationCodecError::MalformedJson { .. })
    ));
    let invalid_unresolved_code = String::from_utf8(UNRESOLVED_FIXTURE.to_vec())
        .unwrap()
        .replacen("unknown_entity_reference", "future_constraint", 1);
    assert!(matches!(
        CompilationResult::from_canonical_json(invalid_unresolved_code.as_bytes(), &limits),
        Err(CompilationCodecError::MalformedJson { .. })
    ));

    let nested_unknown = fixture.replacen(
        "\"entity_key\":\"table\"",
        "\"entity_key\":\"table\",\"private_value\":\"do-not-retain\"",
        1,
    );
    assert!(matches!(
        CompilationResult::from_canonical_json(nested_unknown.as_bytes(), &limits),
        Err(CompilationCodecError::MalformedJson { .. })
    ));

    let spaced = fixture.replacen('{', "{ ", 1);
    assert_eq!(
        CompilationResult::from_canonical_json(spaced.as_bytes(), &limits),
        Err(CompilationCodecError::NonCanonicalResult)
    );

    let reordered = fixture.replacen(
        "{\"schema_version\":1,\"imagination_id\":\"00000000000000000000000000000001\"",
        "{\"imagination_id\":\"00000000000000000000000000000001\",\"schema_version\":1",
        1,
    );
    assert_eq!(
        CompilationResult::from_canonical_json(reordered.as_bytes(), &limits),
        Err(CompilationCodecError::NonCanonicalResult)
    );

    let mut trailing = COMPILED_FIXTURE.to_vec();
    trailing.extend_from_slice(b"{}");
    assert!(matches!(
        CompilationResult::from_canonical_json(&trailing, &limits),
        Err(CompilationCodecError::MalformedJson { .. })
    ));
    for end in 0..COMPILED_FIXTURE.len() {
        assert!(
            CompilationResult::from_canonical_json(&COMPILED_FIXTURE[..end], &limits).is_err(),
            "truncated prefix {end} must reject"
        );
    }
}

#[test]
fn encoded_nesting_and_typed_resource_limits_precede_return() {
    let result = compiled_result();
    let encoded = result
        .to_canonical_json(&CompilationLimits::default())
        .unwrap();

    let exact_encoded_limits = CompilationLimits {
        max_encoded_bytes: NonZeroU64::new(encoded.len() as u64).unwrap(),
        ..CompilationLimits::default()
    };
    assert_eq!(
        result.to_canonical_json(&exact_encoded_limits).unwrap(),
        encoded
    );

    let limits = CompilationLimits {
        max_encoded_bytes: NonZeroU64::new(encoded.len() as u64 - 1).unwrap(),
        ..CompilationLimits::default()
    };
    assert!(matches!(
        CompilationResult::from_canonical_json(&encoded, &limits),
        Err(CompilationCodecError::EncodedSizeExceeded { .. })
    ));
    assert!(matches!(
        result.to_canonical_json(&limits),
        Err(CompilationCodecError::EncodedSizeExceeded { .. })
    ));

    let limits = CompilationLimits {
        max_json_nesting_depth: NonZeroU16::new(2).unwrap(),
        ..CompilationLimits::default()
    };
    assert!(matches!(
        CompilationResult::from_canonical_json(&encoded, &limits),
        Err(CompilationCodecError::NestingLimitExceeded { .. })
    ));
    assert!(matches!(
        result.to_canonical_json(&limits),
        Err(CompilationCodecError::NestingLimitExceeded { .. })
    ));

    let exact_text_bytes = result.patch.as_ref().unwrap().text_bytes()
        + result.decisions[0].entity_key.len_bytes() as u64;
    let exact_text_limits = CompilationLimits {
        max_text_bytes: NonZeroU64::new(exact_text_bytes).unwrap(),
        ..CompilationLimits::default()
    };
    result.validate_with_limits(&exact_text_limits).unwrap();
    let limits = CompilationLimits {
        max_text_bytes: NonZeroU64::new(exact_text_bytes - 1).unwrap(),
        ..exact_text_limits
    };
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::TextLimitExceeded
    );

    let exact_logical_bytes = result.patch.as_ref().unwrap().logical_size_bytes()
        + 35
        + 8
        + result.decisions[0].entity_key.len_bytes() as u64
        + 16;
    let exact_logical_limits = CompilationLimits {
        max_decoded_bytes: NonZeroU64::new(exact_logical_bytes).unwrap(),
        ..CompilationLimits::default()
    };
    result.validate_with_limits(&exact_logical_limits).unwrap();
    let limits = CompilationLimits {
        max_decoded_bytes: NonZeroU64::new(exact_logical_bytes - 1).unwrap(),
        ..exact_logical_limits
    };
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::DecodedSizeLimitExceeded
    );

    let limits = CompilationLimits {
        patch_limits: cogniform_protocol::RuntimeLimits {
            max_operations: NonZeroU32::new(1).unwrap(),
            ..cogniform_protocol::RuntimeLimits::default()
        },
        ..CompilationLimits::default()
    };
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::InvalidPatch
    );
}

#[test]
fn derived_limits_preserve_the_fixed_result_nesting_floor() {
    let runtime = cogniform_protocol::RuntimeLimits {
        max_json_nesting_depth: NonZeroU16::new(1).unwrap(),
        ..cogniform_protocol::RuntimeLimits::default()
    };
    let limits = CompilationLimits::for_runtime_limits(runtime);
    let encoded = compiled_result().to_canonical_json(&limits).unwrap();

    assert_eq!(limits.max_json_nesting_depth.get(), 9);
    assert_eq!(
        CompilationResult::from_canonical_json(&encoded, &limits).unwrap(),
        compiled_result()
    );
}

#[test]
fn decision_and_unresolved_counts_are_independently_bounded() {
    let mut compiled = compiled_result();
    compiled.decisions.push(generated_decision("tabletop", 5));
    let limits = CompilationLimits {
        max_decisions: NonZeroU32::new(1).unwrap(),
        ..CompilationLimits::default()
    };
    assert_eq!(
        compiled.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::DecisionLimitExceeded
    );

    let mut unresolved = unresolved_result();
    unresolved
        .unresolved
        .push(relation_issue(1, "shade", "lamp"));
    let limits = CompilationLimits {
        max_unresolved_constraints: NonZeroU32::new(1).unwrap(),
        ..CompilationLimits::default()
    };
    assert_eq!(
        unresolved.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::UnresolvedLimitExceeded
    );
}

#[test]
fn outcome_revision_and_code_specific_shapes_are_enforced() {
    let limits = CompilationLimits::default();
    let mut result = compiled_result();
    result.patch = None;
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::InvalidOutcome
    );

    result = compiled_result();
    result.unresolved.push(relation_issue(0, "lamp", "table"));
    assert!(!result.is_compiled());
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::InvalidOutcome
    );

    result = compiled_result();
    result.patch.as_mut().unwrap().base_revision = SceneRevision::new(8);
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::PatchRevisionMismatch
    );

    result = compiled_result();
    result.patch.as_mut().unwrap().schema_version = SchemaVersion::new(2).unwrap();
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::PatchSchemaMismatch
    );

    result = compiled_result();
    result.decisions[0].entity_id = None;
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::InvalidDecisionShape
    );

    let mut unresolved = unresolved_result();
    unresolved.unresolved[0].constraint_index = Some(0);
    assert_eq!(
        unresolved.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::InvalidUnresolvedShape
    );
}

#[test]
fn every_decision_code_accepts_only_its_field_role() {
    let limits = CompilationLimits::default();
    let valid = [
        (CompilationDecisionCode::GeneratedEntityId, None, Some(10)),
        (
            CompilationDecisionCode::PreferredEntityIdSubstituted,
            None,
            Some(10),
        ),
        (CompilationDecisionCode::DefaultName, None, None),
        (CompilationDecisionCode::DefaultTransform, None, None),
        (CompilationDecisionCode::DefaultMaterial, None, None),
        (
            CompilationDecisionCode::ParentRelationApplied,
            Some(0),
            None,
        ),
        (CompilationDecisionCode::AboveRelationApplied, Some(0), None),
        (
            CompilationDecisionCode::RightOfRelationApplied,
            Some(0),
            None,
        ),
    ];
    for (code, relation_index, entity_id) in valid {
        let mut result = compiled_result();
        result.decisions = vec![CompilationDecision {
            code,
            entity_key: text("a"),
            relation_index,
            entity_id: entity_id.map(stable_id),
        }];
        result.validate_with_limits(&limits).unwrap();
    }

    let invalid = [
        (CompilationDecisionCode::GeneratedEntityId, None, None),
        (
            CompilationDecisionCode::PreferredEntityIdSubstituted,
            Some(0),
            Some(10),
        ),
        (CompilationDecisionCode::DefaultName, None, Some(10)),
        (CompilationDecisionCode::DefaultTransform, Some(0), None),
        (CompilationDecisionCode::DefaultMaterial, None, Some(10)),
        (CompilationDecisionCode::ParentRelationApplied, None, None),
        (
            CompilationDecisionCode::AboveRelationApplied,
            Some(0),
            Some(10),
        ),
        (CompilationDecisionCode::RightOfRelationApplied, None, None),
    ];
    for (code, relation_index, entity_id) in invalid {
        let mut result = compiled_result();
        result.decisions = vec![CompilationDecision {
            code,
            entity_key: text("a"),
            relation_index,
            entity_id: entity_id.map(stable_id),
        }];
        assert_eq!(
            result.validate_with_limits(&limits).unwrap_err().kind(),
            CompilationValidationKind::InvalidDecisionShape
        );
    }
}

#[test]
fn every_unresolved_code_accepts_only_its_field_role() {
    let limits = CompilationLimits::default();
    let relation_codes = [
        UnresolvedConstraintCode::UnknownEntityReference,
        UnresolvedConstraintCode::SelfRelation,
        UnresolvedConstraintCode::ConflictingRelation,
        UnresolvedConstraintCode::HierarchyCycle,
        UnresolvedConstraintCode::PlacementCycle,
        UnresolvedConstraintCode::NonFinitePlacement,
        UnresolvedConstraintCode::UnsupportedSpatialRotation,
    ];
    let constraint_codes = [
        UnresolvedConstraintCode::RequiredEntityMissing,
        UnresolvedConstraintCode::RequiredEntityPresent,
    ];
    for code in relation_codes.iter().copied() {
        let mut result = unresolved_result();
        result.unresolved[0].code = code;
        result.validate_with_limits(&limits).unwrap();
    }
    for code in constraint_codes.iter().copied() {
        let mut result = unresolved_result();
        result.unresolved[0] = UnresolvedConstraint {
            code,
            relation_index: None,
            constraint_index: Some(0),
            entity_key: None,
            related_key: None,
            entity_id: Some(stable_id(10)),
        };
        result.validate_with_limits(&limits).unwrap();
    }

    for code in relation_codes {
        let mut result = unresolved_result();
        result.unresolved[0].code = code;
        result.unresolved[0].related_key = None;
        assert_eq!(
            result.validate_with_limits(&limits).unwrap_err().kind(),
            CompilationValidationKind::InvalidUnresolvedShape
        );
    }
    for code in constraint_codes {
        let mut result = unresolved_result();
        result.unresolved[0].code = code;
        assert_eq!(
            result.validate_with_limits(&limits).unwrap_err().kind(),
            CompilationValidationKind::InvalidUnresolvedShape
        );
    }
}

#[test]
fn canonical_comparators_pin_code_optional_text_numeric_and_identity_order() {
    let limits = CompilationLimits::default();
    let decision_codes = [
        CompilationDecisionCode::GeneratedEntityId,
        CompilationDecisionCode::PreferredEntityIdSubstituted,
        CompilationDecisionCode::DefaultName,
        CompilationDecisionCode::DefaultTransform,
        CompilationDecisionCode::DefaultMaterial,
        CompilationDecisionCode::ParentRelationApplied,
        CompilationDecisionCode::AboveRelationApplied,
        CompilationDecisionCode::RightOfRelationApplied,
    ];
    let mut decisions: Vec<_> = decision_codes
        .into_iter()
        .rev()
        .map(|code| {
            let (relation_index, entity_id) = match code {
                CompilationDecisionCode::GeneratedEntityId
                | CompilationDecisionCode::PreferredEntityIdSubstituted => {
                    (None, Some(stable_id(10)))
                }
                CompilationDecisionCode::DefaultName
                | CompilationDecisionCode::DefaultTransform
                | CompilationDecisionCode::DefaultMaterial => (None, None),
                CompilationDecisionCode::ParentRelationApplied
                | CompilationDecisionCode::AboveRelationApplied
                | CompilationDecisionCode::RightOfRelationApplied => (Some(0), None),
                _ => unreachable!("version-one test enumerates every decision code"),
            };
            CompilationDecision {
                code,
                entity_key: text("entity"),
                relation_index,
                entity_id,
            }
        })
        .collect();
    decisions.sort_by(CompilationDecision::canonical_cmp);
    assert_eq!(
        decisions.iter().map(|entry| entry.code).collect::<Vec<_>>(),
        decision_codes
    );
    let mut compiled = compiled_result();
    compiled.decisions = decisions;
    compiled.validate_with_limits(&limits).unwrap();

    let constraint = |code, entity_id| UnresolvedConstraint {
        code,
        relation_index: None,
        constraint_index: Some(0),
        entity_key: None,
        related_key: None,
        entity_id: Some(stable_id(entity_id)),
    };
    let relation = |code, relation_index, key| UnresolvedConstraint {
        code,
        relation_index: Some(relation_index),
        constraint_index: None,
        entity_key: Some(text(key)),
        related_key: Some(text("anchor")),
        entity_id: None,
    };
    let mut unresolved = vec![
        relation(UnresolvedConstraintCode::UnknownEntityReference, 1, "a"),
        relation(UnresolvedConstraintCode::UnsupportedSpatialRotation, 0, "a"),
        relation(UnresolvedConstraintCode::NonFinitePlacement, 0, "a"),
        relation(UnresolvedConstraintCode::PlacementCycle, 0, "a"),
        relation(UnresolvedConstraintCode::HierarchyCycle, 0, "a"),
        relation(UnresolvedConstraintCode::ConflictingRelation, 0, "a"),
        relation(UnresolvedConstraintCode::SelfRelation, 0, "a"),
        relation(UnresolvedConstraintCode::UnknownEntityReference, 0, "b"),
        relation(UnresolvedConstraintCode::UnknownEntityReference, 0, "a"),
        constraint(UnresolvedConstraintCode::RequiredEntityPresent, 1),
        constraint(UnresolvedConstraintCode::RequiredEntityMissing, 2),
        constraint(UnresolvedConstraintCode::RequiredEntityMissing, 1),
    ];
    unresolved.sort_by(UnresolvedConstraint::canonical_cmp);
    assert_eq!(
        unresolved
            .iter()
            .map(|entry| entry.code)
            .collect::<Vec<_>>(),
        vec![
            UnresolvedConstraintCode::RequiredEntityMissing,
            UnresolvedConstraintCode::RequiredEntityMissing,
            UnresolvedConstraintCode::RequiredEntityPresent,
            UnresolvedConstraintCode::UnknownEntityReference,
            UnresolvedConstraintCode::UnknownEntityReference,
            UnresolvedConstraintCode::SelfRelation,
            UnresolvedConstraintCode::ConflictingRelation,
            UnresolvedConstraintCode::HierarchyCycle,
            UnresolvedConstraintCode::PlacementCycle,
            UnresolvedConstraintCode::NonFinitePlacement,
            UnresolvedConstraintCode::UnsupportedSpatialRotation,
            UnresolvedConstraintCode::UnknownEntityReference,
        ]
    );
    let mut unresolved_result = unresolved_result();
    unresolved_result.unresolved = unresolved;
    unresolved_result.validate_with_limits(&limits).unwrap();
}

#[test]
fn decision_and_unresolved_order_and_uniqueness_are_canonical() {
    let limits = CompilationLimits::default();
    let mut result = compiled_result();
    result.decisions.push(CompilationDecision {
        code: CompilationDecisionCode::DefaultName,
        entity_key: text("table"),
        relation_index: None,
        entity_id: None,
    });
    result.decisions.reverse();
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::NonCanonicalDecisionOrder
    );

    result = compiled_result();
    result.decisions.push(result.decisions[0].clone());
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::DuplicateDecision
    );

    result = compiled_result();
    result.decisions.push(generated_decision("table", 5));
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::DuplicateDecision
    );

    let mut unresolved = unresolved_result();
    unresolved
        .unresolved
        .push(relation_issue(1, "shade", "lamp"));
    unresolved.unresolved.reverse();
    assert_eq!(
        unresolved.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::NonCanonicalUnresolvedOrder
    );

    unresolved = unresolved_result();
    unresolved.unresolved.push(unresolved.unresolved[0].clone());
    assert_eq!(
        unresolved.validate_with_limits(&limits).unwrap_err().kind(),
        CompilationValidationKind::DuplicateUnresolved
    );
}
