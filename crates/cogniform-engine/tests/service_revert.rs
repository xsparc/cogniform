//! Controlled-adapter contract for quiescent in-place historical revert.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use core::num::NonZeroU32;
use std::time::{Duration, Instant};

use cogniform_engine::{
    AssetAdmission, AssetMeshKey, AssetState, AssetUploadAdmission, CanonicalScenarioConfig,
    CanonicalScenarioReport, EngineError, EngineRecoveryPoint, GatewayAdmission, GatewayResponse,
    LocalAssetStatus, LocalRevertError, LocalRevertReceipt, LocalService, LocalServiceConfig,
    LocalServiceError, LocalServiceStatus, Observation, ObservationRequest, content_hash,
    run_canonical_scenario,
};
use cogniform_protocol::{
    ApplyStatus, ComponentKind, ComponentValue, ConflictPolicy, DeliverySemantic, FrameId,
    IdempotencyKey, NameComponent, ObservationId, ObservationKind, ObservationQuality, PatchBudget,
    SceneOperation, ScenePatch, SceneQuery, SceneRevision, SceneText, SchemaVersion, SetComponent,
    StableEntityId, TransactionId,
};
use cogniform_replay::ReplayLog;
use cogniform_world::LogicalSceneHash;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn local_service_revert_is_quiescent_atomic_and_branch_continuable() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let mut service = LocalService::new(config.clone()).await.unwrap();
        let report =
            run_canonical_scenario(&mut service, CanonicalScenarioConfig::default()).unwrap();
        let revision_two_hash = service.logical_hash().unwrap();
        let (retained_patch, removed_tail_patch) = replay_patches(&service, &config);

        let pre_revert_observation =
            prepare_source_and_assert_blockers(&mut service, &report).await;
        assert_non_mutating_revision_errors(&mut service).await;

        let target = service
            .recovery_point_at_revision(SceneRevision::new(1))
            .unwrap();
        let reverted = assert_successful_revert(&mut service, &report, &target).await;
        assert_post_revert_observation(&mut service, &report, reverted, &pre_revert_observation);
        assert_idempotency_and_branch_continuation(
            &mut service,
            retained_patch,
            removed_tail_patch,
            revision_two_hash,
        );
    });
}

fn replay_patches(service: &LocalService, config: &LocalServiceConfig) -> (ScenePatch, ScenePatch) {
    let complete = service.recovery_point().unwrap();
    let loaded = ReplayLog::load_prefix(
        complete.replay_bytes(),
        config.engine.replay,
        &config.engine.world.runtime_limits,
    );
    assert_eq!(loaded.tail_error(), None);
    (
        loaded.log().entries()[0].patch().clone(),
        loaded.log().entries()[1].patch().clone(),
    )
}

async fn prepare_source_and_assert_blockers(
    service: &mut LocalService,
    report: &CanonicalScenarioReport,
) -> Observation {
    let queued_patch = name_patch(
        SceneRevision::new(2),
        100,
        report.table_id,
        "temporary branch tail",
    );
    assert!(matches!(
        service.submit_patch(queued_patch).unwrap(),
        GatewayAdmission::Queued { .. }
    ));
    let queued_proof = StateProof::capture(service);
    assert_not_quiescent(
        service.revert_to_revision(SceneRevision::new(1)).await,
        1,
        0,
        0,
        0,
    );
    queued_proof.assert_unchanged(service);
    assert_eq!(process_patch(service).new_revision, SceneRevision::new(3));

    request_entity_observation(service, report.camera_id, 700);
    let observation_proof = StateProof::capture(service);
    assert_not_quiescent(
        service.revert_to_revision(SceneRevision::new(1)).await,
        0,
        1,
        0,
        0,
    );
    observation_proof.assert_unchanged(service);
    let pre_revert_observation = wait_for_observation(service);

    let bytes = decode_hex(include_str!("../../../tests/assets/triangle.glb.hex"));
    let hash = content_hash(&bytes);
    let key = AssetMeshKey {
        content_hash: hash,
        mesh_index: 0,
    };
    assert_eq!(
        service.enqueue_asset_source(hash, bytes).unwrap(),
        AssetAdmission::Queued { content_hash: hash }
    );
    let import_proof = StateProof::capture(service);
    assert_not_quiescent(
        service.revert_to_revision(SceneRevision::new(1)).await,
        0,
        0,
        1,
        0,
    );
    import_proof.assert_unchanged(service);
    assert_eq!(
        service.process_next_asset_import().unwrap().state,
        AssetState::Ready
    );

    assert_eq!(
        service.enqueue_asset_upload(key).unwrap(),
        AssetUploadAdmission::Queued { key }
    );
    let upload_proof = StateProof::capture(service);
    assert_not_quiescent(
        service.revert_to_revision(SceneRevision::new(1)).await,
        0,
        0,
        0,
        1,
    );
    upload_proof.assert_unchanged(service);
    assert_eq!(service.process_next_asset_upload().unwrap().key, key);
    pre_revert_observation
}

async fn assert_successful_revert(
    service: &mut LocalService,
    report: &CanonicalScenarioReport,
    target: &EngineRecoveryPoint,
) -> LocalRevertReceipt {
    let before = service.status();
    let before_assets = service.asset_status();
    assert_eq!(before.scene_revision, SceneRevision::new(3));
    assert_eq!(before.command_queue.depth, 0);
    assert_eq!(before.command_queue.oldest_pending_age_micros, None);
    assert_eq!(before.outstanding_observations, 0);
    assert_eq!(before.oldest_outstanding_observation_age_micros, None);
    assert_eq!(before_assets.store.pending_imports, 0);
    assert_eq!(before_assets.store.oldest_pending_import_age_micros, None);
    assert_eq!(before_assets.renderer.pending_uploads, 0);
    assert_eq!(
        before_assets.renderer.oldest_pending_upload_age_micros,
        None
    );
    assert_eq!(before_assets.store.records, 1);
    assert_eq!(before_assets.renderer.resident_meshes, 1);

    let reverted = service
        .revert_to_revision(SceneRevision::new(1))
        .await
        .unwrap();
    assert_eq!(reverted.previous_revision, SceneRevision::new(3));
    assert_eq!(reverted.target_revision, SceneRevision::new(1));
    assert_eq!(reverted.removed_replay_entries, 2);
    assert_eq!(reverted.next_frame_id, target.next_frame_id());
    assert_eq!(reverted.cleared_completed_results, before.completed_results);
    assert_eq!(reverted.cleared_asset_records, before_assets.store.records);
    assert_eq!(
        reverted.cleared_cpu_asset_bytes,
        before_assets.store.resident_cpu_bytes
    );
    assert_eq!(
        reverted.cleared_resident_asset_meshes,
        before_assets.renderer.resident_meshes
    );
    assert_eq!(
        reverted.cleared_gpu_asset_bytes,
        before_assets.renderer.resident_bytes
    );

    let status = service.status();
    assert_eq!(status.scene_revision, SceneRevision::new(1));
    assert_eq!(status.renderer_revision, SceneRevision::new(1));
    assert_eq!(status.command_queue.depth, 0);
    assert_eq!(status.command_queue.oldest_pending_age_micros, None);
    assert_eq!(status.completed_results, 0);
    assert_eq!(status.outstanding_observations, 0);
    assert_eq!(status.oldest_outstanding_observation_age_micros, None);
    assert_eq!(status.replay_entries, 1);
    assert_empty_assets(&service.asset_status());
    assert_eq!(service.replay_bytes(), target.replay_bytes());
    assert_eq!(
        service.logical_hash().unwrap(),
        service.replayed_logical_hash().unwrap()
    );
    assert_table_exists(service, report.table_id, SceneRevision::new(1));
    assert_eq!(
        service.recovery_point().unwrap().next_frame_id(),
        reverted.next_frame_id
    );
    reverted
}

fn assert_post_revert_observation(
    service: &mut LocalService,
    report: &CanonicalScenarioReport,
    reverted: LocalRevertReceipt,
    pre_revert_observation: &Observation,
) {
    request_entity_observation(service, report.camera_id, 701);
    let post_revert_observation = wait_for_observation(service);
    assert_eq!(
        post_revert_observation.metadata().frame_id,
        reverted.next_frame_id
    );
    assert!(
        post_revert_observation.metadata().frame_id > pre_revert_observation.metadata().frame_id
    );
    assert_eq!(
        post_revert_observation.metadata().scene_revision,
        SceneRevision::new(1)
    );
}

fn assert_idempotency_and_branch_continuation(
    service: &mut LocalService,
    retained_patch: ScenePatch,
    removed_tail_patch: ScenePatch,
    revision_two_hash: LogicalSceneHash,
) {
    let target_replay = service.replay_bytes();
    let retained_receipt = submit_and_process(service, retained_patch);
    assert_eq!(retained_receipt.status, ApplyStatus::IdempotentReplay);
    assert_eq!(service.status().scene_revision, SceneRevision::new(1));
    assert_eq!(service.replay_bytes(), target_replay);

    let reapplied = submit_and_process(service, removed_tail_patch);
    assert_eq!(reapplied.status, ApplyStatus::Applied);
    assert_eq!(reapplied.new_revision, SceneRevision::new(2));
    assert_eq!(service.status().replay_entries, 2);
    assert_eq!(service.logical_hash().unwrap(), revision_two_hash);
    assert_eq!(service.replayed_logical_hash().unwrap(), revision_two_hash);
}

async fn assert_non_mutating_revision_errors(service: &mut LocalService) {
    let equal_proof = StateProof::capture(service);
    assert!(matches!(
        service.revert_to_revision(SceneRevision::new(3)).await,
        Err(LocalServiceError::Revert(error))
            if matches!(
                error.as_ref(),
                LocalRevertError::TargetIsCurrent { revision }
                    if *revision == SceneRevision::new(3)
            )
    ));
    equal_proof.assert_unchanged(service);

    let future_proof = StateProof::capture(service);
    assert!(matches!(
        service.revert_to_revision(SceneRevision::new(4)).await,
        Err(LocalServiceError::Engine(error))
            if matches!(
                error.as_ref(),
                EngineError::ReplayRevision(error)
                    if error.requested() == SceneRevision::new(4)
                        && error.latest() == SceneRevision::new(3)
            )
    ));
    future_proof.assert_unchanged(service);
}

fn assert_not_quiescent(
    result: Result<cogniform_engine::LocalRevertReceipt, LocalServiceError>,
    command_depth: u32,
    outstanding_observations: u32,
    pending_asset_imports: u32,
    pending_asset_uploads: u32,
) {
    assert!(matches!(
        result,
        Err(LocalServiceError::Revert(error))
            if matches!(
                error.as_ref(),
                LocalRevertError::NotQuiescent {
                    command_depth: found_commands,
                    outstanding_observations: found_observations,
                    pending_asset_imports: found_imports,
                    pending_asset_uploads: found_uploads,
                } if *found_commands == command_depth
                    && *found_observations == outstanding_observations
                    && *found_imports == pending_asset_imports
                    && *found_uploads == pending_asset_uploads
            )
    ));
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateProof {
    status: LocalServiceStatus,
    assets: LocalAssetStatus,
    logical_hash: LogicalSceneHash,
    replay: Vec<u8>,
    next_frame_id: FrameId,
}

impl StateProof {
    fn capture(service: &LocalService) -> Self {
        Self {
            status: service.status(),
            assets: service.asset_status(),
            logical_hash: service.logical_hash().unwrap(),
            replay: service.replay_bytes(),
            next_frame_id: service.recovery_point().unwrap().next_frame_id(),
        }
    }

    fn assert_unchanged(&self, service: &LocalService) {
        assert_status_state_unchanged(&self.status, service.status());
        assert_asset_state_unchanged(&self.assets, service.asset_status());
        assert_eq!(service.logical_hash().unwrap(), self.logical_hash);
        assert_eq!(service.replay_bytes(), self.replay);
        assert_eq!(
            service.recovery_point().unwrap().next_frame_id(),
            self.next_frame_id
        );
    }
}

fn assert_status_state_unchanged(expected: &LocalServiceStatus, mut actual: LocalServiceStatus) {
    assert_monotonic_age(
        expected.command_queue.oldest_pending_age_micros,
        actual.command_queue.oldest_pending_age_micros,
    );
    assert_monotonic_age(
        expected.oldest_outstanding_observation_age_micros,
        actual.oldest_outstanding_observation_age_micros,
    );
    actual.command_queue.oldest_pending_age_micros =
        expected.command_queue.oldest_pending_age_micros;
    actual.oldest_outstanding_observation_age_micros =
        expected.oldest_outstanding_observation_age_micros;
    assert_eq!(&actual, expected);
}

fn assert_asset_state_unchanged(expected: &LocalAssetStatus, mut actual: LocalAssetStatus) {
    assert_monotonic_age(
        expected.store.oldest_pending_import_age_micros,
        actual.store.oldest_pending_import_age_micros,
    );
    assert_monotonic_age(
        expected.renderer.oldest_pending_upload_age_micros,
        actual.renderer.oldest_pending_upload_age_micros,
    );
    actual.store.oldest_pending_import_age_micros = expected.store.oldest_pending_import_age_micros;
    actual.renderer.oldest_pending_upload_age_micros =
        expected.renderer.oldest_pending_upload_age_micros;
    assert_eq!(&actual, expected);
}

fn assert_monotonic_age(expected: Option<u64>, actual: Option<u64>) {
    match (expected, actual) {
        (None, None) => {}
        (Some(expected), Some(actual)) => assert!(actual >= expected),
        _ => panic!("unchanged pending lifecycle must preserve age presence"),
    }
}

fn process_patch(service: &mut LocalService) -> cogniform_protocol::ApplyReceipt {
    let response = service.process_next().unwrap().unwrap();
    let GatewayResponse::PatchApplied { receipt } = response else {
        panic!("expected an ordinary patch response");
    };
    receipt
}

fn submit_and_process(
    service: &mut LocalService,
    patch: ScenePatch,
) -> cogniform_protocol::ApplyReceipt {
    assert!(matches!(
        service.submit_patch(patch).unwrap(),
        GatewayAdmission::Queued { .. }
    ));
    process_patch(service)
}

fn request_entity_observation(service: &mut LocalService, camera_id: StableEntityId, nonce: u128) {
    service
        .request_observation(ObservationRequest {
            observation_id: ObservationId::new(nonce).unwrap(),
            camera_id,
            kind: ObservationKind::EntityId,
            quality: ObservationQuality::Low,
        })
        .unwrap();
}

fn wait_for_observation(service: &LocalService) -> Observation {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(observation) = service.try_receive_observation().unwrap() {
            return observation;
        }
        assert!(Instant::now() < deadline, "revert observation timed out");
        std::thread::yield_now();
    }
}

fn assert_table_exists(service: &LocalService, table_id: StableEntityId, revision: SceneRevision) {
    let query = service
        .query(&SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: revision,
            entity_ids: vec![table_id],
            component_kinds: vec![ComponentKind::LocalTransform],
            limit: NonZeroU32::new(1).unwrap(),
        })
        .unwrap();
    assert_eq!(query.entities.len(), 1);
    assert_eq!(query.entities[0].entity_id, table_id);
    assert!(
        query.entities[0]
            .components
            .iter()
            .any(|component| matches!(component, ComponentValue::LocalTransform(_)))
    );
}

fn assert_empty_assets(status: &LocalAssetStatus) {
    assert_eq!(status.store.records, 0);
    assert_eq!(status.store.pending_imports, 0);
    assert_eq!(status.store.oldest_pending_import_age_micros, None);
    assert_eq!(status.store.pending_source_bytes, 0);
    assert_eq!(status.store.resident_cpu_bytes, 0);
    assert_eq!(status.renderer.pending_uploads, 0);
    assert_eq!(status.renderer.oldest_pending_upload_age_micros, None);
    assert_eq!(status.renderer.pending_bytes, 0);
    assert_eq!(status.renderer.resident_meshes, 0);
    assert_eq!(status.renderer.resident_bytes, 0);
}

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

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
