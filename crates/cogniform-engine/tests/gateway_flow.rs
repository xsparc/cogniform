//! Controlled-adapter integration for gateway compilation, apply, query, and replay.

use core::num::NonZeroU32;

use cogniform_engine::{
    CogniformEngine, EngineConfig, EngineError, GatewayAdmission, GatewayConfig, GatewayError,
    GatewayResponse, LocalGateway,
};
use cogniform_protocol::{
    ApplyStatus, ConflictPolicy, DeliverySemantic, IdempotencyKey, ImaginationBudget,
    ImaginationEnvelope, ImaginationId, ImaginedEntity, PatchBudget, PositiveF32, PositiveVec3,
    PrimitiveComponent, PrimitiveShape, SceneOperation, ScenePatch, SceneQuery, SceneRevision,
    SceneText, SchemaVersion, StableEntityId, TransactionId,
};
use cogniform_world::WorldApplyError;

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

fn request() -> ImaginationEnvelope {
    ImaginationEnvelope {
        schema_version: SchemaVersion::V1,
        imagination_id: ImaginationId::new(10).unwrap(),
        transaction_id: TransactionId::new(20).unwrap(),
        idempotency_key: IdempotencyKey::new(30).unwrap(),
        base_revision: SceneRevision::INITIAL,
        delivery: DeliverySemantic::MustApply,
        seed: 40,
        declared_budget: ImaginationBudget::default(),
        entities: vec![ImaginedEntity {
            key: SceneText::new("gateway-cube").unwrap(),
            preferred_id: None,
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

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn gateway_compiles_applies_queries_and_replays_without_duplicate_mutation() {
    pollster::block_on(async {
        let engine = CogniformEngine::new(EngineConfig::new(64, 64))
            .await
            .unwrap();
        let mut gateway = LocalGateway::new(
            engine,
            GatewayConfig {
                command_capacity: NonZeroU32::new(2).unwrap(),
                idempotency_capacity: NonZeroU32::new(4).unwrap(),
            },
        )
        .unwrap();
        let imagination = request();
        assert!(matches!(
            gateway.submit_imagination(imagination.clone()).unwrap(),
            GatewayAdmission::Queued { .. }
        ));
        let response = gateway.process_next().unwrap().unwrap();
        let created_revision = match response {
            GatewayResponse::ImaginationProcessed {
                compilation,
                receipt: Some(receipt),
            } => {
                assert!(compilation.is_compiled());
                assert_eq!(receipt.status, ApplyStatus::Applied);
                receipt.new_revision
            }
            _ => panic!("expected an applied imagination"),
        };
        assert_eq!(created_revision, SceneRevision::new(1));

        let query = SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: created_revision,
            entity_ids: Vec::new(),
            component_kinds: Vec::new(),
            limit: NonZeroU32::new(4).unwrap(),
        };
        let result = gateway.query(&query).unwrap();
        assert_eq!(result.entities.len(), 1);
        assert_eq!(result.entities[0].components.len(), 4);

        match gateway.submit_imagination(imagination.clone()).unwrap() {
            GatewayAdmission::Replayed { response } => match *response {
                GatewayResponse::ImaginationProcessed {
                    receipt: Some(receipt),
                    ..
                } => assert_eq!(receipt.status, ApplyStatus::IdempotentReplay),
                _ => panic!("expected a replayed imagination receipt"),
            },
            _ => panic!("expected an immediate idempotent replay"),
        }
        assert_eq!(gateway.engine().revision(), created_revision);

        let mut conflicting = imagination;
        conflicting.transaction_id = TransactionId::new(41).unwrap();
        assert!(matches!(
            gateway.submit_imagination(conflicting),
            Err(GatewayError::IdempotencyConflict { .. })
        ));

        let stale = ScenePatch {
            schema_version: SchemaVersion::V1,
            transaction_id: TransactionId::new(50).unwrap(),
            idempotency_key: IdempotencyKey::new(60).unwrap(),
            base_revision: SceneRevision::INITIAL,
            conflict_policy: ConflictPolicy::RequireExactBase,
            delivery: DeliverySemantic::MustApply,
            declared_budget: PatchBudget::default(),
            operations: vec![SceneOperation::Delete(cogniform_protocol::DeleteEntity {
                entity_id: StableEntityId::new(70).unwrap(),
            })],
        };
        gateway.submit_patch(stale).unwrap();
        assert!(matches!(
            gateway.process_next(),
            Err(GatewayError::Engine(error))
                if matches!(
                    error.as_ref(),
                    EngineError::WorldApply(WorldApplyError::BaseRevisionMismatch { .. })
                )
        ));
        assert_eq!(gateway.engine().revision(), created_revision);
    });
}
