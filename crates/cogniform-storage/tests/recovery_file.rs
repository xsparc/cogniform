//! Controlled-adapter contract for persisted recovery-file continuation.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use core::{
    num::NonZeroU32,
    sync::atomic::{AtomicU64, Ordering},
};
use std::{
    fs,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use cogniform_engine::{
    CanonicalScenarioConfig, GatewayAdmission, GatewayResponse, LocalService, LocalServiceConfig,
    Observation, ObservationPayload, ObservationRequest, run_canonical_scenario,
};
use cogniform_protocol::{
    ApplyStatus, ComponentKind, ComponentValue, ConflictPolicy, DeliverySemantic, IdempotencyKey,
    NameComponent, ObservationId, ObservationKind, ObservationQuality, PatchBudget, SceneOperation,
    ScenePatch, SceneQuery, SceneRevision, SceneText, SchemaVersion, SetComponent, StableEntityId,
    TransactionId,
};
use cogniform_storage::RecoveryFileStore;

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn persisted_recovery_restores_and_continues_exact_causality() {
    pollster::block_on(async {
        let directory = TestDirectory::new();
        let path = directory.path().join("checkpoint.cnfr");
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let store = RecoveryFileStore::new(config.engine.replay).unwrap();
        let mut source = LocalService::new(config.clone()).await.unwrap();
        let report =
            run_canonical_scenario(&mut source, CanonicalScenarioConfig::default()).unwrap();

        let expected_status = source.status();
        let expected_hash = source.logical_hash().unwrap();
        let expected_replay = source.replay_bytes();
        let recovery = source.recovery_point().unwrap();
        let receipt = store.create_new(&path, &recovery).unwrap();
        assert_eq!(receipt.replay_bytes, expected_status.replay_bytes);
        assert_eq!(receipt.next_frame_id, recovery.next_frame_id());
        assert_eq!(receipt.envelope_bytes, fs::metadata(&path).unwrap().len());
        drop(source);

        let loaded = store.load(&path).unwrap();
        assert_eq!(loaded, recovery);
        let mut restored = LocalService::restore(config, &loaded).await.unwrap();
        let restored_status = restored.status();
        assert_eq!(
            restored_status.scene_revision,
            expected_status.scene_revision
        );
        assert_eq!(
            restored_status.renderer_revision,
            expected_status.renderer_revision
        );
        assert_eq!(restored_status.command_queue.depth, 0);
        assert_eq!(restored_status.completed_results, 0);
        assert_eq!(restored_status.outstanding_observations, 0);
        assert_eq!(restored.replay_bytes(), expected_replay);
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replayed_logical_hash().unwrap(), expected_hash);
        assert_eq!(
            restored.recovery_point().unwrap().next_frame_id(),
            recovery.next_frame_id()
        );
        assert_table_exists(&restored, report.table_id, SceneRevision::new(2));

        restored
            .request_observation(ObservationRequest {
                observation_id: ObservationId::new(800).unwrap(),
                camera_id: report.camera_id,
                kind: ObservationKind::EntityId,
                quality: ObservationQuality::Low,
            })
            .unwrap();
        let observation = wait_for_observation(&restored);
        assert_eq!(observation.metadata().scene_revision, SceneRevision::new(2));
        assert_eq!(observation.metadata().frame_id, recovery.next_frame_id());
        let ObservationPayload::EntityId(entities) = observation.payload() else {
            panic!("expected persisted entity-ID observation");
        };
        assert!(entities.contains(&Some(report.table_id)));

        let continued = process_patch(
            &mut restored,
            name_patch(
                SceneRevision::new(2),
                400,
                report.table_id,
                "persisted table",
            ),
        );
        assert_eq!(continued.status, ApplyStatus::Applied);
        assert_eq!(continued.previous_revision, SceneRevision::new(2));
        assert_eq!(continued.new_revision, SceneRevision::new(3));
        assert!(continued.estimated_visible_frame > observation.metadata().frame_id);
        assert!(restored.replay_bytes().starts_with(&expected_replay));
        assert_eq!(restored.verify_replay().unwrap().entry_count(), 3);
        assert_eq!(
            restored.logical_hash().unwrap(),
            restored.replayed_logical_hash().unwrap()
        );
    });
}

fn process_patch(
    service: &mut LocalService,
    patch: ScenePatch,
) -> cogniform_protocol::ApplyReceipt {
    assert!(matches!(
        service.submit_patch(patch).unwrap(),
        GatewayAdmission::Queued { .. }
    ));
    let GatewayResponse::PatchApplied { receipt } = service.process_next().unwrap().unwrap() else {
        panic!("expected ordinary patch response");
    };
    receipt
}

fn wait_for_observation(service: &LocalService) -> Observation {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(observation) = service.try_receive_observation().unwrap() {
            return observation;
        }
        assert!(
            Instant::now() < deadline,
            "persisted recovery observation timed out"
        );
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

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        loop {
            let candidate = std::env::temp_dir().join(format!(
                "cogniform-storage-controlled-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => return Self(candidate),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create controlled test directory: {error:?}"),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TestDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
