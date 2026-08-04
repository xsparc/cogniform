//! Controlled-adapter contract for complete in-memory service restoration.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use core::num::NonZeroU32;
use std::time::{Duration, Instant};

use cogniform_engine::{
    CanonicalScenarioConfig, EngineError, EngineRecoveryPoint, GatewayAdmission, GatewayResponse,
    LocalService, LocalServiceConfig, LocalServiceError, Observation, ObservationPayload,
    ObservationRequest, run_canonical_scenario,
};
use cogniform_protocol::{
    ApplyStatus, ComponentKind, ComponentValue, ConflictPolicy, DeliverySemantic, FrameId,
    IdempotencyKey, NameComponent, ObservationId, ObservationKind, ObservationQuality, PatchBudget,
    SceneOperation, ScenePatch, SceneQuery, SceneRevision, SceneText, SchemaVersion, SetComponent,
    StableEntityId, TransactionId,
};
use cogniform_replay::{ReplayConfig, ReplayError, ReplayLog, ReplayTailErrorKind};
use cogniform_world::WorldConfig;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

fn name_patch(
    base_revision: SceneRevision,
    nonce: u128,
    entity_id: StableEntityId,
    name: &str,
) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(nonce * 2).unwrap(),
        idempotency_key: IdempotencyKey::new((nonce * 2) + 1).unwrap(),
        base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![SceneOperation::SetComponent(SetComponent {
            entity_id,
            component: ComponentValue::Name(NameComponent {
                value: SceneText::new(name).unwrap(),
            }),
        })],
    }
}

fn process_patch(
    service: &mut LocalService,
    patch: ScenePatch,
) -> cogniform_protocol::ApplyReceipt {
    assert!(matches!(
        service.submit_patch(patch).unwrap(),
        GatewayAdmission::Queued { .. }
    ));
    match service.process_next().unwrap().unwrap() {
        GatewayResponse::PatchApplied { receipt } => receipt,
        GatewayResponse::ImaginationProcessed { .. } => panic!("expected patch response"),
    }
}

fn wait_for_observation(service: &LocalService) -> Observation {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(observation) = service.try_receive_observation().unwrap() {
            return observation;
        }
        assert!(Instant::now() < deadline, "restored observation timed out");
        std::thread::yield_now();
    }
}

async fn assert_frame_rollback_rejected(
    config: LocalServiceConfig,
    recovery: &EngineRecoveryPoint,
    recorded_frame_id: FrameId,
) {
    let invalid_frame = FrameId::new(recovery.next_frame_id().get() - 1).unwrap();
    let invalid = EngineRecoveryPoint::from_parts(recovery.replay_bytes().to_vec(), invalid_frame);
    assert!(matches!(
        LocalService::restore(config.clone(), &invalid).await,
        Err(LocalServiceError::Engine(error))
            if matches!(
                error.as_ref(),
                EngineError::RecoveryFrameBehindReplay {
                    next_frame_id,
                    recorded_frame_id: found,
                } if *next_frame_id == invalid_frame && *found == recorded_frame_id
            )
    ));

    let mut truncated_bytes = recovery.replay_bytes().to_vec();
    truncated_bytes.pop().unwrap();
    let truncated = EngineRecoveryPoint::from_parts(truncated_bytes, recovery.next_frame_id());
    assert!(matches!(
        LocalService::restore(config, &truncated).await,
        Err(LocalServiceError::Engine(error))
            if matches!(
                error.as_ref(),
                EngineError::Replay(ReplayError::Tail(error))
                    if matches!(error.kind(), ReplayTailErrorKind::Truncated)
            )
    ));
}

fn assert_restored_query(
    service: &LocalService,
    revision: SceneRevision,
    table_id: StableEntityId,
    expected_name: &str,
) {
    let query = service
        .query(&SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: revision,
            entity_ids: vec![table_id],
            component_kinds: vec![ComponentKind::Name],
            limit: NonZeroU32::new(1).unwrap(),
        })
        .unwrap();
    assert_eq!(query.entities.len(), 1);
    assert_eq!(query.entities[0].entity_id, table_id);
    assert!(query.entities[0].components.iter().any(
        |component| matches!(component, ComponentValue::Name(name) if name.value.as_str() == expected_name)
    ));
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn complete_recovery_restores_queries_observations_and_continuation() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let mut service = LocalService::new(config.clone()).await.unwrap();
        let report =
            run_canonical_scenario(&mut service, CanonicalScenarioConfig::default()).unwrap();

        let checkpoint_receipt = process_patch(
            &mut service,
            name_patch(
                report.update_receipt.new_revision,
                100,
                report.table_id,
                "checkpointed table",
            ),
        );
        assert_eq!(checkpoint_receipt.new_revision, SceneRevision::new(3));
        assert!(checkpoint_receipt.estimated_visible_frame > report.visibility.frame_id);

        let expected_hash = service.logical_hash().unwrap();
        let recovery = service.recovery_point().unwrap();
        let envelope = recovery.to_envelope_bytes(config.engine.replay).unwrap();
        let decoded =
            EngineRecoveryPoint::from_envelope_bytes(&envelope, config.engine.replay).unwrap();
        assert_eq!(decoded, recovery);
        let expected_bytes = recovery.replay_bytes().to_vec();
        assert_eq!(
            recovery.next_frame_id(),
            checkpoint_receipt.estimated_visible_frame
        );
        let loaded = ReplayLog::load_prefix(
            recovery.replay_bytes(),
            ReplayConfig::default(),
            &WorldConfig::default().runtime_limits,
        );
        assert_eq!(loaded.tail_error(), None);
        let original_patch = loaded.log().entries()[0].patch().clone();
        drop(service);

        assert_frame_rollback_rejected(
            config.clone(),
            &recovery,
            checkpoint_receipt.estimated_visible_frame,
        )
        .await;

        let mut restored = LocalService::restore(config, &decoded).await.unwrap();
        let status = restored.status();
        assert_eq!(status.scene_revision, SceneRevision::new(3));
        assert_eq!(status.renderer_revision, SceneRevision::new(3));
        assert_eq!(status.command_queue.depth, 0);
        assert_eq!(status.completed_results, 0);
        assert_eq!(status.outstanding_observations, 0);
        assert_eq!(restored.replay_bytes(), expected_bytes);
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replayed_logical_hash().unwrap(), expected_hash);
        assert_eq!(
            restored.recovery_point().unwrap().next_frame_id(),
            recovery.next_frame_id()
        );

        assert_restored_query(
            &restored,
            SceneRevision::new(3),
            report.table_id,
            "checkpointed table",
        );

        let replayed = process_patch(&mut restored, original_patch);
        assert_eq!(replayed.status, ApplyStatus::IdempotentReplay);
        assert_eq!(restored.status().scene_revision, SceneRevision::new(3));
        assert_eq!(restored.replay_bytes(), expected_bytes);

        restored
            .request_observation(ObservationRequest {
                observation_id: ObservationId::new(500).unwrap(),
                camera_id: report.camera_id,
                kind: ObservationKind::Normal,
                quality: ObservationQuality::Low,
            })
            .unwrap();
        let observation = wait_for_observation(&restored);
        assert_eq!(observation.metadata().scene_revision, SceneRevision::new(3));
        assert_eq!(observation.metadata().frame_id, recovery.next_frame_id());
        let ObservationPayload::Normal(normals) = observation.payload() else {
            panic!("expected restored normal observation");
        };
        assert!(normals.iter().any(Option::is_some));

        let continued = process_patch(
            &mut restored,
            name_patch(
                SceneRevision::new(3),
                101,
                report.table_id,
                "continued table",
            ),
        );
        assert_eq!(continued.status, ApplyStatus::Applied);
        assert_eq!(continued.previous_revision, SceneRevision::new(3));
        assert_eq!(continued.new_revision, SceneRevision::new(4));
        assert!(continued.estimated_visible_frame > observation.metadata().frame_id);
        assert!(restored.replay_bytes().starts_with(&expected_bytes));
        assert_eq!(restored.verify_replay().unwrap().entry_count(), 4);
        assert_eq!(
            restored.logical_hash().unwrap(),
            restored.replayed_logical_hash().unwrap()
        );
    });
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn historical_recovery_fork_restores_exact_revision_and_continues() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let mut source = LocalService::new(config.clone()).await.unwrap();
        let report =
            run_canonical_scenario(&mut source, CanonicalScenarioConfig::default()).unwrap();
        let historical_hash = source.logical_hash().unwrap();
        let checkpoint_patch = name_patch(
            report.update_receipt.new_revision,
            100,
            report.table_id,
            "checkpointed table",
        );
        let checkpoint_receipt = process_patch(&mut source, checkpoint_patch.clone());
        assert_eq!(checkpoint_receipt.new_revision, SceneRevision::new(3));

        assert!(matches!(
            source.submit_patch(name_patch(
                SceneRevision::new(3),
                101,
                report.table_id,
                "queued only",
            )),
            Ok(GatewayAdmission::Queued { .. })
        ));

        let source_status = source.status();
        assert_eq!(source_status.command_queue.depth, 1);
        let source_hash = source.logical_hash().unwrap();
        let source_bytes = source.replay_bytes();
        let source_next_frame = source.recovery_point().unwrap().next_frame_id();
        let historical = source
            .recovery_point_at_revision(SceneRevision::new(2))
            .unwrap();
        assert_eq!(historical.next_frame_id(), source_next_frame);
        assert!(source_bytes.starts_with(historical.replay_bytes()));
        assert_eq!(source.status(), source_status);
        assert_eq!(source.logical_hash().unwrap(), source_hash);
        assert_eq!(source.replay_bytes(), source_bytes);

        assert!(matches!(
            source.recovery_point_at_revision(SceneRevision::new(4)),
            Err(LocalServiceError::Engine(error))
                if matches!(
                    error.as_ref(),
                    EngineError::ReplayRevision(revision_error)
                        if revision_error.requested() == SceneRevision::new(4)
                            && revision_error.latest() == SceneRevision::new(3)
                )
        ));
        drop(source);

        let mut fork = LocalService::restore(config, &historical).await.unwrap();
        let status = fork.status();
        assert_eq!(status.scene_revision, SceneRevision::new(2));
        assert_eq!(status.renderer_revision, SceneRevision::new(2));
        assert_eq!(status.command_queue.depth, 0);
        assert_eq!(status.completed_results, 0);
        assert_eq!(status.outstanding_observations, 0);
        assert_eq!(fork.replay_bytes(), historical.replay_bytes());
        assert_eq!(fork.logical_hash().unwrap(), historical_hash);
        assert_eq!(fork.replayed_logical_hash().unwrap(), historical_hash);
        assert_restored_query(&fork, SceneRevision::new(2), report.table_id, "table");

        fork.request_observation(ObservationRequest {
            observation_id: ObservationId::new(600).unwrap(),
            camera_id: report.camera_id,
            kind: ObservationKind::Normal,
            quality: ObservationQuality::Low,
        })
        .unwrap();
        let observation = wait_for_observation(&fork);
        assert_eq!(observation.metadata().scene_revision, SceneRevision::new(2));
        assert_eq!(observation.metadata().frame_id, historical.next_frame_id());

        let continued = process_patch(&mut fork, checkpoint_patch);
        assert_eq!(continued.status, ApplyStatus::Applied);
        assert_eq!(continued.previous_revision, SceneRevision::new(2));
        assert_eq!(continued.new_revision, SceneRevision::new(3));
        assert!(continued.estimated_visible_frame > observation.metadata().frame_id);
        assert!(fork.replay_bytes().starts_with(historical.replay_bytes()));
        assert_eq!(fork.verify_replay().unwrap().entry_count(), 3);
        assert_eq!(fork.logical_hash().unwrap(), source_hash);
        assert_eq!(
            fork.logical_hash().unwrap(),
            fork.replayed_logical_hash().unwrap()
        );
        assert_restored_query(
            &fork,
            SceneRevision::new(3),
            report.table_id,
            "checkpointed table",
        );
    });
}
