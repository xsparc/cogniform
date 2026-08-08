//! Determinism, explanation, constraint, and budget contracts for the compiler.

use cogniform_compiler::{
    COMPILATION_SCHEMA_VERSION, CompilationCodecError, CompilationDecisionCode, CompilationLimits,
    CompilationSceneView, CompileError, CompilerConfig, DeterministicCompiler,
    UnresolvedConstraintCode,
};
use cogniform_protocol::{
    ComponentValue, DeliverySemantic, FiniteF32, IdempotencyKey, ImaginationBudget,
    ImaginationConstraint, ImaginationEnvelope, ImaginationId, ImaginationRelation, ImaginedEntity,
    LocalTransform, NonNegativeF32, PositiveF32, PositiveVec3, PrimitiveComponent, PrimitiveShape,
    Quaternion, RuntimeLimits, SceneOperation, SceneRevision, SceneText, SchemaVersion,
    StableEntityId, TransactionId, Vec3,
};

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

fn entity(key: &str, dimensions: [f32; 3]) -> ImaginedEntity {
    ImaginedEntity {
        key: SceneText::new(key).unwrap(),
        preferred_id: None,
        name: None,
        primitive: PrimitiveComponent {
            shape: PrimitiveShape::Cuboid,
            dimensions: PositiveVec3 {
                x: positive(dimensions[0]),
                y: positive(dimensions[1]),
                z: positive(dimensions[2]),
            },
        },
        transform: None,
        material: None,
    }
}

fn translated(position: [f32; 3]) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(position[0]),
            y: finite(position[1]),
            z: finite(position[2]),
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

fn imagination(entities: Vec<ImaginedEntity>) -> ImaginationEnvelope {
    ImaginationEnvelope {
        schema_version: SchemaVersion::V1,
        imagination_id: ImaginationId::new(0x100).unwrap(),
        transaction_id: TransactionId::new(0x200).unwrap(),
        idempotency_key: IdempotencyKey::new(0x300).unwrap(),
        base_revision: SceneRevision::new(7),
        delivery: DeliverySemantic::LatestWins {
            supersession_key: SceneText::new("draft/furniture").unwrap(),
        },
        seed: 42,
        declared_budget: ImaginationBudget::default(),
        entities,
        relations: Vec::new(),
        constraints: Vec::new(),
    }
}

#[test]
fn normalized_patch_is_byte_stable_across_entity_input_order() {
    let compiler = DeterministicCompiler::new(CompilerConfig::default());
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let mut table = entity("table", [2.0, 1.0, 1.0]);
    let mut table_transform = translated([3.0, 4.0, 5.0]);
    table_transform.scale.y = positive(2.0);
    table.transform = Some(table_transform);
    let mut first = imagination(vec![table, entity("lamp", [1.0, 2.0, 1.0])]);
    first.relations.push(ImaginationRelation::Above {
        subject: SceneText::new("lamp").unwrap(),
        anchor: SceneText::new("table").unwrap(),
        clearance: NonNegativeF32::new(0.25).unwrap(),
    });
    let mut second = first.clone();
    second.entities.reverse();

    let first_result = compiler.compile(&first, &scene).unwrap();
    let second_result = compiler.compile(&second, &scene).unwrap();
    let limits = RuntimeLimits::default();
    let first_bytes = first_result
        .patch
        .as_ref()
        .unwrap()
        .to_canonical_json(&limits)
        .unwrap();
    let second_bytes = second_result
        .patch
        .as_ref()
        .unwrap()
        .to_canonical_json(&limits)
        .unwrap();
    assert_eq!(first_bytes, second_bytes);

    let lamp_create = first_result
        .patch
        .as_ref()
        .unwrap()
        .operations
        .iter()
        .find_map(|operation| match operation {
            SceneOperation::Create(create)
                if create.components.iter().any(|component| {
                    matches!(
                        component,
                        ComponentValue::Name(name) if name.value.as_str() == "lamp"
                    )
                }) =>
            {
                Some(create)
            }
            _ => None,
        })
        .unwrap();
    let lamp_transform = lamp_create
        .components
        .iter()
        .find_map(|component| match component {
            ComponentValue::LocalTransform(transform) => Some(transform),
            _ => None,
        })
        .unwrap();
    assert!((lamp_transform.translation.x.get() - 3.0).abs() <= f32::EPSILON);
    assert!((lamp_transform.translation.y.get() - 6.25).abs() <= f32::EPSILON);
    assert!((lamp_transform.translation.z.get() - 5.0).abs() <= f32::EPSILON);
    assert!(
        first_result
            .decisions
            .iter()
            .any(|decision| { decision.code == CompilationDecisionCode::AboveRelationApplied })
    );
}

#[test]
fn compilation_result_reexport_and_canonical_contract_are_compatible() {
    let request = imagination(vec![entity("table", [2.0, 1.0, 1.0])]);
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let reexported: cogniform_compiler::CompilationResult =
        DeterministicCompiler::new(CompilerConfig::default())
            .compile(&request, &scene)
            .unwrap();
    assert_eq!(reexported.schema_version, COMPILATION_SCHEMA_VERSION);

    let direct: cogniform_compilation::CompilationResult = reexported.clone();
    let bytes = direct
        .to_canonical_json(&CompilationLimits::default())
        .unwrap();
    assert_eq!(
        cogniform_compilation::CompilationResult::from_canonical_json(
            &bytes,
            &CompilationLimits::default()
        )
        .unwrap(),
        reexported
    );
}

#[test]
fn explicit_report_limits_fail_before_a_result_is_returned() {
    let runtime_limits = RuntimeLimits::default();
    let config = CompilerConfig {
        compilation_limits: CompilationLimits {
            max_decisions: core::num::NonZeroU32::new(1).unwrap(),
            ..CompilationLimits::for_runtime_limits(runtime_limits)
        },
        ..CompilerConfig::new(runtime_limits)
    };
    let request = imagination(vec![entity("table", [2.0, 1.0, 1.0])]);
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);

    assert!(matches!(
        DeterministicCompiler::new(config).compile(&request, &scene),
        Err(CompileError::InvalidCompilationResult(error))
            if error.kind() == cogniform_compiler::CompilationValidationKind::DecisionLimitExceeded
    ));
}

#[test]
fn encoded_and_nesting_report_limits_fail_before_a_result_is_returned() {
    let runtime_limits = RuntimeLimits::default();
    let request = imagination(vec![entity("table", [2.0, 1.0, 1.0])]);
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let default_config = CompilerConfig::new(runtime_limits);
    let baseline = DeterministicCompiler::new(default_config)
        .compile(&request, &scene)
        .unwrap();
    let encoded = baseline
        .to_canonical_json(&default_config.compilation_limits)
        .unwrap();

    let encoded_limited = CompilerConfig {
        compilation_limits: CompilationLimits {
            max_encoded_bytes: core::num::NonZeroU64::new(
                u64::try_from(encoded.len()).unwrap() - 1,
            )
            .unwrap(),
            ..default_config.compilation_limits
        },
        ..default_config
    };
    assert!(matches!(
        DeterministicCompiler::new(encoded_limited).compile(&request, &scene),
        Err(CompileError::InvalidCompilationEncoding(
            CompilationCodecError::EncodedSizeExceeded { .. }
        ))
    ));

    let nesting_limited = CompilerConfig {
        compilation_limits: CompilationLimits {
            max_json_nesting_depth: core::num::NonZeroU16::new(1).unwrap(),
            ..default_config.compilation_limits
        },
        ..default_config
    };
    assert!(matches!(
        DeterministicCompiler::new(nesting_limited).compile(&request, &scene),
        Err(CompileError::InvalidCompilationEncoding(
            CompilationCodecError::NestingLimitExceeded { .. }
        ))
    ));
}

#[test]
fn derived_report_limits_admit_defaulted_compiler_nesting() {
    let runtime_limits = RuntimeLimits {
        max_json_nesting_depth: core::num::NonZeroU16::new(1).unwrap(),
        ..RuntimeLimits::default()
    };
    let config = CompilerConfig::new(runtime_limits);
    let report_limits = config.compilation_limits;
    let request = imagination(vec![entity("table", [2.0, 1.0, 1.0])]);
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let result = DeterministicCompiler::new(config)
        .compile(&request, &scene)
        .unwrap();
    let encoded = result.to_canonical_json(&report_limits).unwrap();

    assert_eq!(report_limits.max_json_nesting_depth.get(), 9);
    assert_eq!(
        cogniform_compilation::CompilationResult::from_canonical_json(&encoded, &report_limits)
            .unwrap(),
        result
    );
}

#[test]
fn unsupported_spatial_transforms_fail_closed_with_exact_relation_context() {
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let mut rotated_anchor = entity("anchor", [1.0; 3]);
    let mut anchor_transform = translated([0.0; 3]);
    anchor_transform.rotation.x = finite(1.0);
    anchor_transform.rotation.w = finite(1.0);
    rotated_anchor.transform = Some(anchor_transform);
    let mut request = imagination(vec![rotated_anchor, entity("subject", [1.0; 3])]);
    request.relations.push(ImaginationRelation::Above {
        subject: SceneText::new("subject").unwrap(),
        anchor: SceneText::new("anchor").unwrap(),
        clearance: NonNegativeF32::new(0.0).unwrap(),
    });

    let result = DeterministicCompiler::new(CompilerConfig::default())
        .compile(&request, &scene)
        .unwrap();
    assert!(!result.is_compiled());
    assert_eq!(result.unresolved.len(), 1);
    assert_eq!(
        result.unresolved[0].code,
        UnresolvedConstraintCode::UnsupportedSpatialRotation
    );
    assert_eq!(result.unresolved[0].relation_index, Some(0));
    assert_eq!(
        result.unresolved[0].entity_key.as_ref().unwrap().as_str(),
        "subject"
    );
    assert_eq!(
        result.unresolved[0].related_key.as_ref().unwrap().as_str(),
        "anchor"
    );
}

#[test]
fn a_parented_spatial_anchor_is_an_explicit_conflict() {
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let mut request = imagination(vec![
        entity("lamp", [1.0; 3]),
        entity("room", [4.0; 3]),
        entity("table", [2.0, 1.0, 1.0]),
    ]);
    request.relations.extend([
        ImaginationRelation::Above {
            subject: SceneText::new("lamp").unwrap(),
            anchor: SceneText::new("table").unwrap(),
            clearance: NonNegativeF32::new(0.0).unwrap(),
        },
        ImaginationRelation::Parent {
            child: SceneText::new("table").unwrap(),
            parent: SceneText::new("room").unwrap(),
        },
    ]);

    let result = DeterministicCompiler::new(CompilerConfig::default())
        .compile(&request, &scene)
        .unwrap();
    assert!(!result.is_compiled());
    assert!(result.unresolved.iter().any(|issue| {
        issue.code == UnresolvedConstraintCode::ConflictingRelation
            && issue.relation_index == Some(0)
    }));
}

#[test]
fn defaults_and_preferred_id_substitution_are_explained() {
    let occupied = StableEntityId::new(9).unwrap();
    let scene = CompilationSceneView::new(SceneRevision::new(7), [occupied]);
    let mut request = imagination(vec![entity("cube", [1.0, 1.0, 1.0])]);
    request.entities[0].preferred_id = Some(occupied);

    let result = DeterministicCompiler::new(CompilerConfig::default())
        .compile(&request, &scene)
        .unwrap();
    assert!(result.is_compiled());
    for code in [
        CompilationDecisionCode::PreferredEntityIdSubstituted,
        CompilationDecisionCode::DefaultName,
        CompilationDecisionCode::DefaultTransform,
        CompilationDecisionCode::DefaultMaterial,
    ] {
        assert!(
            result
                .decisions
                .iter()
                .any(|decision| decision.code == code)
        );
    }
}

#[test]
fn unresolved_relations_and_scene_constraints_never_emit_a_patch() {
    let required = StableEntityId::new(0xfeed).unwrap();
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let mut request = imagination(vec![entity("table", [1.0, 1.0, 1.0])]);
    request.relations.push(ImaginationRelation::RightOf {
        subject: SceneText::new("table").unwrap(),
        anchor: SceneText::new("missing").unwrap(),
        gap: NonNegativeF32::new(0.0).unwrap(),
    });
    request
        .constraints
        .push(ImaginationConstraint::EntityExists {
            entity_id: required,
        });

    let result = DeterministicCompiler::new(CompilerConfig::default())
        .compile(&request, &scene)
        .unwrap();
    assert!(!result.is_compiled());
    assert!(
        result
            .unresolved
            .iter()
            .any(|issue| { issue.code == UnresolvedConstraintCode::UnknownEntityReference })
    );
    assert!(
        result
            .unresolved
            .iter()
            .any(|issue| { issue.code == UnresolvedConstraintCode::RequiredEntityMissing })
    );
}

#[test]
fn relation_cycles_and_stale_views_fail_structurally() {
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    let mut request = imagination(vec![entity("a", [1.0; 3]), entity("b", [1.0; 3])]);
    request.relations.extend([
        ImaginationRelation::Parent {
            child: SceneText::new("a").unwrap(),
            parent: SceneText::new("b").unwrap(),
        },
        ImaginationRelation::Parent {
            child: SceneText::new("b").unwrap(),
            parent: SceneText::new("a").unwrap(),
        },
    ]);
    let result = DeterministicCompiler::new(CompilerConfig::default())
        .compile(&request, &scene)
        .unwrap();
    assert!(
        result
            .unresolved
            .iter()
            .all(|issue| { issue.code == UnresolvedConstraintCode::HierarchyCycle })
    );

    let stale = CompilationSceneView::new(SceneRevision::new(8), []);
    assert!(matches!(
        DeterministicCompiler::new(CompilerConfig::default()).compile(&request, &stale),
        Err(CompileError::SceneRevisionMismatch { .. })
    ));
}

#[test]
fn declared_compilation_limits_fail_before_hashing_or_allocation_growth() {
    let limits = RuntimeLimits {
        max_imagination_entities: core::num::NonZeroU32::new(1).unwrap(),
        ..RuntimeLimits::default()
    };
    let compiler = DeterministicCompiler::new(CompilerConfig::new(limits));
    let request = imagination(vec![entity("a", [1.0; 3]), entity("b", [1.0; 3])]);
    let scene = CompilationSceneView::new(SceneRevision::new(7), []);
    assert!(matches!(
        compiler.compile(&request, &scene),
        Err(CompileError::InvalidImagination(_))
    ));
}
