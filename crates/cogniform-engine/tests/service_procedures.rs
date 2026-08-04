//! Controlled-adapter contract for pure built-in procedures through `LocalService`.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use core::num::NonZeroU32;

use cogniform_engine::{
    BuiltinProcedure, CuboidGrid, GatewayAdmission, GatewayError, GatewayResponse, LocalService,
    LocalServiceConfig, LocalServiceError, ProcedureError, ProcedureLimits, ProcedureRequest,
};
use cogniform_protocol::{
    ApplyStatus, ColorRgba, ComponentKind, ComponentValue, DeliverySemantic, FiniteF32,
    IdempotencyKey, MaterialComponent, PatchBudget, PositiveF32, PositiveVec3, ProcedureId,
    SceneQuery, SceneRevision, SchemaVersion, StableEntityId, TransactionId, UnitF32, Vec3,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn local_service_procedure_preserves_queue_query_replay_and_restore_idempotency() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let mut service = LocalService::new(config.clone()).await.unwrap();
        let request = procedure_request(99);

        assert_invalid_procedure_is_not_admitted(&mut service, &request);

        let submitted = service.submit_procedure(&request).unwrap();
        assert!(matches!(
            submitted.admission,
            GatewayAdmission::Queued {
                idempotency_key
            } if idempotency_key == request.idempotency_key
        ));
        assert_eq!(submitted.entity_ids, expected_grid_ids());
        assert_eq!(service.status().scene_revision, SceneRevision::INITIAL);
        assert_eq!(service.status().command_queue.depth, 1);
        assert_eq!(service.verify_replay().unwrap().entry_count(), 0);

        let already_queued = service.submit_procedure(&request).unwrap();
        assert!(matches!(
            already_queued.admission,
            GatewayAdmission::AlreadyQueued { .. }
        ));
        assert_eq!(already_queued.entity_ids, submitted.entity_ids);
        assert_eq!(service.status().command_queue.depth, 1);

        let conflicting = procedure_request(100);
        assert!(matches!(
            service.submit_procedure(&conflicting),
            Err(LocalServiceError::Gateway(error))
                if matches!(error.as_ref(), GatewayError::IdempotencyConflict { idempotency_key }
                    if *idempotency_key == request.idempotency_key)
        ));
        assert_eq!(service.status().scene_revision, SceneRevision::INITIAL);
        assert_eq!(service.status().command_queue.depth, 1);

        let receipt = process_patch_response(&mut service);
        assert_eq!(receipt.status, ApplyStatus::Applied);
        assert_eq!(receipt.new_revision, SceneRevision::new(1));
        assert_grid_query(&service, &submitted.entity_ids);

        let expected_replay = service.replay_bytes();
        let expected_hash = service.logical_hash().unwrap();
        assert_eq!(service.replayed_logical_hash().unwrap(), expected_hash);
        assert_eq!(service.verify_replay().unwrap().entry_count(), 1);

        let replayed = service.submit_procedure(&request).unwrap();
        assert_eq!(replayed.entity_ids, submitted.entity_ids);
        let GatewayAdmission::Replayed { response } = replayed.admission else {
            panic!("expected completed gateway replay");
        };
        let GatewayResponse::PatchApplied { receipt } = *response else {
            panic!("procedure output must remain an ordinary patch response");
        };
        assert_eq!(receipt.status, ApplyStatus::IdempotentReplay);
        assert_unchanged(&service, expected_hash, &expected_replay);

        let recovery = service.recovery_point().unwrap();
        drop(service);
        let mut restored = LocalService::restore(config, &recovery).await.unwrap();
        assert_unchanged(&restored, expected_hash, &expected_replay);
        assert_grid_query(&restored, &submitted.entity_ids);

        let restored_submission = restored.submit_procedure(&request).unwrap();
        assert!(matches!(
            restored_submission.admission,
            GatewayAdmission::Queued { .. }
        ));
        assert_eq!(restored_submission.entity_ids, submitted.entity_ids);
        assert_eq!(restored.status().command_queue.depth, 1);
        let restored_receipt = process_patch_response(&mut restored);
        assert_eq!(restored_receipt.status, ApplyStatus::IdempotentReplay);
        assert_unchanged(&restored, expected_hash, &expected_replay);
    });
}

fn assert_invalid_procedure_is_not_admitted(
    service: &mut LocalService,
    request: &ProcedureRequest,
) {
    let mut invalid = request.clone();
    invalid.procedure_limits.max_entities = NonZeroU32::new(5).unwrap();
    assert!(matches!(
        service.submit_procedure(&invalid),
        Err(LocalServiceError::Procedure(error))
            if matches!(
                error.as_ref(),
                ProcedureError::EntityLimitExceeded {
                    actual: 6,
                    limit: 5
                }
            )
    ));
    assert_eq!(service.status().scene_revision, SceneRevision::INITIAL);
    assert_eq!(service.status().command_queue.depth, 0);
    assert_eq!(service.status().completed_results, 0);
    assert_eq!(service.verify_replay().unwrap().entry_count(), 0);
}

fn process_patch_response(service: &mut LocalService) -> cogniform_protocol::ApplyReceipt {
    let response = service.process_next().unwrap().unwrap();
    let GatewayResponse::PatchApplied { receipt } = response else {
        panic!("procedure output must use the ordinary patch path");
    };
    receipt
}

fn assert_grid_query(service: &LocalService, entity_ids: &[StableEntityId]) {
    let mut expected_ids = entity_ids.to_vec();
    expected_ids.sort_unstable();
    let result = service
        .query(&SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: SceneRevision::new(1),
            entity_ids: entity_ids.to_vec(),
            component_kinds: vec![
                ComponentKind::LocalTransform,
                ComponentKind::Primitive,
                ComponentKind::Material,
            ],
            limit: NonZeroU32::new(6).unwrap(),
        })
        .unwrap();
    assert_eq!(
        result
            .entities
            .iter()
            .map(|entity| entity.entity_id)
            .collect::<Vec<_>>(),
        expected_ids
    );
    for entity in result.entities {
        assert_eq!(entity.components.len(), 3);
        assert!(
            entity
                .components
                .iter()
                .any(|value| matches!(value, ComponentValue::LocalTransform(_)))
        );
        assert!(
            entity
                .components
                .iter()
                .any(|value| matches!(value, ComponentValue::Primitive(_)))
        );
        assert!(
            entity
                .components
                .iter()
                .any(|value| matches!(value, ComponentValue::Material(_)))
        );
    }
}

fn assert_unchanged(
    service: &LocalService,
    expected_hash: cogniform_world::LogicalSceneHash,
    expected_replay: &[u8],
) {
    assert_eq!(service.status().scene_revision, SceneRevision::new(1));
    assert_eq!(service.status().command_queue.depth, 0);
    assert_eq!(service.replay_bytes(), expected_replay);
    assert_eq!(service.logical_hash().unwrap(), expected_hash);
    assert_eq!(service.replayed_logical_hash().unwrap(), expected_hash);
    assert_eq!(service.verify_replay().unwrap().entry_count(), 1);
}

fn procedure_request(seed: u64) -> ProcedureRequest {
    ProcedureRequest {
        procedure_id: ProcedureId::new(11).unwrap(),
        seed,
        transaction_id: TransactionId::new(12).unwrap(),
        idempotency_key: IdempotencyKey::new(13).unwrap(),
        base_revision: SceneRevision::INITIAL,
        delivery: DeliverySemantic::MustApply,
        patch_budget: PatchBudget::default(),
        procedure_limits: ProcedureLimits::default(),
        procedure: BuiltinProcedure::CuboidGrid(CuboidGrid {
            rows: NonZeroU32::new(2).unwrap(),
            columns: NonZeroU32::new(3).unwrap(),
            origin: Vec3 {
                x: finite(-2.0),
                y: finite(0.5),
                z: finite(-1.0),
            },
            spacing_x: positive(1.5),
            spacing_z: positive(2.0),
            dimensions: PositiveVec3 {
                x: positive(1.0),
                y: positive(1.0),
                z: positive(1.0),
            },
            material: MaterialComponent {
                base_color: ColorRgba {
                    r: unit(0.2),
                    g: unit(0.4),
                    b: unit(0.8),
                    a: unit(1.0),
                },
                metallic: unit(0.0),
                roughness: unit(0.7),
            },
        }),
    }
}

fn expected_grid_ids() -> Vec<StableEntityId> {
    [
        0xd80f_a111_d699_90a0_71ad_0c6f_0278_a6b4,
        0x49d6_99cd_aa6d_2c73_8c57_063c_faef_5228,
        0xef15_519d_b970_56d2_9f98_6056_6c12_4713,
        0xfdf5_6fea_a2d2_4a35_3250_c6b8_474b_fb0d,
        0x90aa_4196_f287_d659_42db_5322_4021_df89,
        0xb962_804d_7f6e_b372_018f_bfe8_1c26_7eeb,
    ]
    .into_iter()
    .map(|value| StableEntityId::new(value).unwrap())
    .collect()
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
