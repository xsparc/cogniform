//! Controlled contract for persisted recovery plus exact-hash asset rehydration.

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
    AssetAdmission, AssetMeshKey, AssetState, AssetUploadAdmission, EngineError, GatewayAdmission,
    GatewayResponse, LocalService, LocalServiceConfig, LocalServiceError, Observation,
    ObservationPayload, ObservationRequest, RendererError, content_hash,
};
use cogniform_protocol::{
    AssetMeshComponent, CameraComponent, ComponentKind, ComponentValue, ConflictPolicy,
    CreateEntity, DeliverySemantic, FiniteF32, IdempotencyKey, LocalTransform, ObservationId,
    ObservationKind, ObservationQuality, PatchBudget, PositiveF32, PositiveVec3, Quaternion,
    SceneOperation, ScenePatch, SceneQuery, SceneRevision, SchemaVersion, StableEntityId,
    TransactionId, Vec3,
};
use cogniform_storage::{AssetFileStore, RecoveryFileStore};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(1);

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn persisted_recovery_and_asset_sources_restore_renderable_state() {
    pollster::block_on(async {
        let directory = TestDirectory::new();
        let recovery_path = directory.path().join("checkpoint.cnfr");
        let asset_path = directory.path().join("triangle.glb");
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let recovery_files = RecoveryFileStore::new(config.engine.replay).unwrap();
        let asset_files = AssetFileStore::new(config.asset_store.limits.max_source_bytes);
        let source = decode_hex(include_str!("../../../tests/assets/triangle.glb.hex"));
        let hash = content_hash(&source);
        let key = AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        };
        let camera = StableEntityId::new(1).unwrap();
        let triangle = StableEntityId::new(2).unwrap();

        let asset_receipt = asset_files.create_new(&asset_path, hash, &source).unwrap();
        assert_eq!(asset_receipt.content_hash, hash);
        assert_eq!(asset_receipt.source_bytes, source.len() as u64);

        let mut service = LocalService::new(config.clone()).await.unwrap();
        assert_eq!(
            service.enqueue_asset_source(hash, source).unwrap(),
            AssetAdmission::Queued { content_hash: hash }
        );
        assert_eq!(
            service.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        assert_eq!(
            service.enqueue_asset_upload(key).unwrap(),
            AssetUploadAdmission::Queued { key }
        );
        assert_eq!(service.process_next_asset_upload().unwrap().key, key);
        assert!(matches!(
            service
                .submit_patch(scene_patch(camera, triangle, hash))
                .unwrap(),
            GatewayAdmission::Queued { .. }
        ));
        let GatewayResponse::PatchApplied { receipt } = service.process_next().unwrap().unwrap()
        else {
            panic!("expected persisted asset patch response");
        };
        assert_eq!(receipt.new_revision, SceneRevision::new(1));
        service.request_observation(request(1, camera)).unwrap();
        assert_center_entity(&wait_for_observation(&service), triangle);

        let expected_status = service.status();
        let expected_hash = service.logical_hash().unwrap();
        let expected_replay = service.replay_bytes();
        let recovery = service.recovery_point().unwrap();
        let recovery_receipt = recovery_files
            .create_new(&recovery_path, &recovery)
            .unwrap();
        assert_eq!(recovery_receipt.replay_bytes, expected_status.replay_bytes);
        drop(service);

        let loaded_recovery = recovery_files.load(&recovery_path).unwrap();
        let loaded_source = asset_files.load(&asset_path, hash).unwrap();
        assert_eq!(content_hash(&loaded_source), hash);
        let mut restored = LocalService::restore(config, &loaded_recovery)
            .await
            .unwrap();
        assert_eq!(
            restored.status().scene_revision,
            expected_status.scene_revision
        );
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replayed_logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replay_bytes(), expected_replay);
        assert_asset_reference(&restored, triangle, hash);

        assert!(matches!(
            restored.request_observation(request(2, camera)),
            Err(LocalServiceError::Engine(error))
                if matches!(
                    error.as_ref(),
                    EngineError::Renderer(RendererError::AssetUnavailable {
                        entity_id,
                        key: missing,
                    }) if *entity_id == triangle && *missing == key
                )
        ));
        assert_eq!(restored.status().outstanding_observations, 0);

        restored.enqueue_asset_source(hash, loaded_source).unwrap();
        assert_eq!(
            restored.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        restored.enqueue_asset_upload(key).unwrap();
        assert_eq!(restored.process_next_asset_upload().unwrap().key, key);
        assert_eq!(
            restored.status().scene_revision,
            expected_status.scene_revision
        );
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replay_bytes(), expected_replay);

        restored.request_observation(request(3, camera)).unwrap();
        let rehydrated = wait_for_observation(&restored);
        assert_center_entity(&rehydrated, triangle);
        assert_eq!(rehydrated.metadata().scene_revision, SceneRevision::new(1));
    });
}

fn assert_asset_reference(
    service: &LocalService,
    triangle: StableEntityId,
    hash: cogniform_protocol::ContentHash,
) {
    let query = service
        .query(&SceneQuery {
            schema_version: SchemaVersion::V1,
            scene_revision: SceneRevision::new(1),
            entity_ids: vec![triangle],
            component_kinds: vec![ComponentKind::AssetMesh],
            limit: NonZeroU32::new(1).unwrap(),
        })
        .unwrap();
    assert_eq!(query.entities.len(), 1);
    assert!(query.entities[0].components.iter().any(|component| {
        matches!(
            component,
            ComponentValue::AssetMesh(asset)
                if asset.content_hash == hash && asset.mesh_index == 0
        )
    }));
}

fn request(nonce: u128, camera_id: StableEntityId) -> ObservationRequest {
    ObservationRequest {
        schema_version: SchemaVersion::V1,
        observation_id: ObservationId::new(nonce).unwrap(),
        scene_revision: SceneRevision::new(1),
        camera_id,
        kind: ObservationKind::EntityId,
        quality: ObservationQuality::Low,
    }
}

fn wait_for_observation(service: &LocalService) -> Observation {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if let Some(observation) = service.try_receive_observation().unwrap() {
            return observation;
        }
        assert!(
            Instant::now() < deadline,
            "persisted asset observation timed out"
        );
        std::thread::yield_now();
    }
}

fn assert_center_entity(observation: &Observation, expected: StableEntityId) {
    let ObservationPayload::EntityId(pixels) = observation.payload() else {
        panic!("expected persisted asset entity-ID observation");
    };
    let center = usize::try_from((HEIGHT / 2) * WIDTH + (WIDTH / 2)).unwrap();
    assert_eq!(pixels[center], Some(expected));
}

fn scene_patch(
    camera: StableEntityId,
    triangle: StableEntityId,
    content_hash: cogniform_protocol::ContentHash,
) -> ScenePatch {
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(2).unwrap(),
        idempotency_key: IdempotencyKey::new(3).unwrap(),
        base_revision: SceneRevision::INITIAL,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::default(),
        operations: vec![
            SceneOperation::Create(CreateEntity {
                entity_id: camera,
                components: vec![
                    ComponentValue::LocalTransform(transform(3.0)),
                    ComponentValue::Camera(CameraComponent {
                        vertical_fov_radians: positive(core::f32::consts::FRAC_PI_2),
                        near: positive(0.1),
                        far: positive(100.0),
                    }),
                ],
            }),
            SceneOperation::Create(CreateEntity {
                entity_id: triangle,
                components: vec![
                    ComponentValue::LocalTransform(transform(0.0)),
                    ComponentValue::AssetMesh(AssetMeshComponent {
                        content_hash,
                        mesh_index: 0,
                    }),
                ],
            }),
        ],
    }
}

fn transform(z: f32) -> LocalTransform {
    LocalTransform {
        translation: Vec3 {
            x: finite(0.0),
            y: finite(0.0),
            z: finite(z),
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

fn decode_hex(value: &str) -> Vec<u8> {
    value
        .trim()
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| {
            let pair = core::str::from_utf8(pair).unwrap();
            u8::from_str_radix(pair, 16).unwrap()
        })
        .collect()
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

struct TestDirectory(PathBuf);

impl TestDirectory {
    fn new() -> Self {
        loop {
            let candidate = std::env::temp_dir().join(format!(
                "cogniform-persisted-asset-controlled-{}-{}",
                std::process::id(),
                NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
            ));
            match fs::create_dir(&candidate) {
                Ok(()) => return Self(candidate),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("failed to create controlled asset test directory: {error:?}"),
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
