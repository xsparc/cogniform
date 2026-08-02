//! Canonical and bounded contracts for imagination and logical scene queries.

use core::num::NonZeroU32;

use cogniform_protocol::{
    ComponentKind, ComponentValue, DeliverySemantic, DiagnosticCode, IdempotencyKey,
    ImaginationBudget, ImaginationConstraint, ImaginationEnvelope, ImaginationId,
    ImaginationRelation, ImaginedEntity, NameComponent, NonNegativeF32, PositiveF32, PositiveVec3,
    PrimitiveComponent, PrimitiveShape, RuntimeLimits, SceneEntityView, SceneQuery,
    SceneQueryResult, SceneRevision, SceneText, SchemaVersion, StableEntityId, TransactionId,
};

const IMAGINATION_FIXTURE: &[u8] = include_bytes!("fixtures/imagination_v1.json");
const QUERY_FIXTURE: &[u8] = include_bytes!("fixtures/scene_query_v1.json");
const QUERY_RESULT_FIXTURE: &[u8] = include_bytes!("fixtures/scene_query_result_v1.json");

fn stable_id(value: u128) -> StableEntityId {
    StableEntityId::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

fn imagination() -> ImaginationEnvelope {
    ImaginationEnvelope {
        schema_version: SchemaVersion::V1,
        imagination_id: ImaginationId::new(0x40).unwrap(),
        transaction_id: TransactionId::new(0x10).unwrap(),
        idempotency_key: IdempotencyKey::new(0x20).unwrap(),
        base_revision: SceneRevision::new(7),
        delivery: DeliverySemantic::LatestWins {
            supersession_key: SceneText::new("draft/table").unwrap(),
        },
        seed: 42,
        declared_budget: ImaginationBudget::default(),
        entities: vec![ImaginedEntity {
            key: SceneText::new("table").unwrap(),
            preferred_id: None,
            name: None,
            primitive: PrimitiveComponent {
                shape: PrimitiveShape::Cuboid,
                dimensions: PositiveVec3 {
                    x: positive(2.0),
                    y: positive(1.0),
                    z: positive(1.0),
                },
            },
            transform: None,
            material: None,
        }],
        relations: Vec::new(),
        constraints: vec![ImaginationConstraint::EntityAbsent {
            entity_id: stable_id(1),
        }],
    }
}

fn query() -> SceneQuery {
    SceneQuery {
        schema_version: SchemaVersion::V1,
        scene_revision: SceneRevision::new(7),
        entity_ids: vec![stable_id(1)],
        component_kinds: vec![ComponentKind::Name, ComponentKind::Primitive],
        limit: NonZeroU32::new(4).unwrap(),
    }
}

fn query_result() -> SceneQueryResult {
    SceneQueryResult {
        schema_version: SchemaVersion::V1,
        scene_revision: SceneRevision::new(7),
        entities: vec![SceneEntityView {
            entity_id: stable_id(1),
            parent_id: None,
            components: vec![
                ComponentValue::Name(NameComponent {
                    value: SceneText::new("table").unwrap(),
                }),
                ComponentValue::Primitive(PrimitiveComponent {
                    shape: PrimitiveShape::Cuboid,
                    dimensions: PositiveVec3 {
                        x: positive(2.0),
                        y: positive(1.0),
                        z: positive(1.0),
                    },
                }),
            ],
        }],
    }
}

#[test]
fn imagination_and_query_fixtures_are_canonical_and_round_trip() {
    let limits = RuntimeLimits::default();
    let imagination = imagination();
    let query = query();
    let result = query_result();
    assert_eq!(
        imagination.to_canonical_json(&limits).unwrap(),
        IMAGINATION_FIXTURE
    );
    assert_eq!(query.to_canonical_json(&limits).unwrap(), QUERY_FIXTURE);
    assert_eq!(
        result.to_canonical_json(&limits).unwrap(),
        QUERY_RESULT_FIXTURE
    );
    assert_eq!(
        ImaginationEnvelope::from_json(IMAGINATION_FIXTURE, &limits).unwrap(),
        imagination
    );
    assert_eq!(
        SceneQuery::from_json(QUERY_FIXTURE, &limits).unwrap(),
        query
    );
    assert_eq!(
        SceneQueryResult::from_json(QUERY_RESULT_FIXTURE, &limits).unwrap(),
        result
    );
}

#[test]
fn imagination_collections_and_query_filters_fail_closed() {
    let limits = RuntimeLimits {
        max_imagination_entities: NonZeroU32::new(1).unwrap(),
        ..RuntimeLimits::default()
    };
    let mut oversized = imagination();
    oversized.entities.push(oversized.entities[0].clone());
    assert_eq!(
        oversized.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::ImaginationEntityLimitExceeded
    );

    let mut duplicate_filter = query();
    duplicate_filter.entity_ids.push(stable_id(1));
    assert_eq!(
        duplicate_filter
            .validate_with_limits(&RuntimeLimits::default())
            .unwrap_err()
            .code(),
        DiagnosticCode::DuplicateQueryFilter
    );
}

#[test]
fn query_results_require_stable_entity_and_component_order() {
    let limits = RuntimeLimits::default();
    let mut result = query_result();
    result.entities.push(result.entities[0].clone());
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::NonCanonicalQueryResult
    );

    let mut result = query_result();
    result.entities[0].components.reverse();
    assert_eq!(
        result.validate_with_limits(&limits).unwrap_err().code(),
        DiagnosticCode::NonCanonicalQueryResult
    );
}

#[test]
fn supported_relations_round_trip_through_the_bounded_decoder() {
    let limits = RuntimeLimits::default();
    let mut value = imagination();
    value.entities.push(ImaginedEntity {
        key: SceneText::new("lamp").unwrap(),
        preferred_id: None,
        name: None,
        primitive: PrimitiveComponent {
            shape: PrimitiveShape::Cuboid,
            dimensions: PositiveVec3 {
                x: positive(1.0),
                y: positive(2.0),
                z: positive(1.0),
            },
        },
        transform: None,
        material: None,
    });
    value.relations.push(ImaginationRelation::Above {
        subject: SceneText::new("lamp").unwrap(),
        anchor: SceneText::new("table").unwrap(),
        clearance: NonNegativeF32::new(0.25).unwrap(),
    });
    let encoded = value.to_canonical_json(&limits).unwrap();
    assert_eq!(
        ImaginationEnvelope::from_json(&encoded, &limits).unwrap(),
        value
    );
}
