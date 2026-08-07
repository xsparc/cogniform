//! Controlled-adapter contract for service-owned asset import and rehydration.

#![cfg(any(target_os = "windows", target_os = "linux"))]

use std::time::{Duration, Instant};

use cogniform_engine::{
    AssetAdmission, AssetError, AssetMeshKey, AssetState, AssetUploadAdmission, EngineError,
    GatewayAdmission, GatewayResponse, LocalService, LocalServiceConfig, LocalServiceError,
    Observation, ObservationPayload, ObservationRequest, RendererError, content_hash,
};
use cogniform_protocol::{
    AssetMeshComponent, CameraComponent, ComponentKind, ComponentValue, ConflictPolicy,
    CreateEntity, DeliverySemantic, FiniteF32, IdempotencyKey, LocalTransform, ObservationId,
    ObservationKind, ObservationQuality, PatchBudget, PositiveF32, PositiveVec3, Quaternion,
    SceneOperation, ScenePatch, SceneQuery, SceneRevision, SchemaVersion, StableEntityId,
    TransactionId, Vec3,
};

const WIDTH: u32 = 64;
const HEIGHT: u32 = 64;

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn local_service_imports_renders_and_explicitly_rehydrates_one_glb_asset() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let bytes = decode_hex(include_str!("../../../tests/assets/triangle.glb.hex"));
        let hash = content_hash(&bytes);
        let key = AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        };
        let camera = StableEntityId::new(1).unwrap();
        let triangle = StableEntityId::new(2).unwrap();

        let mut service = LocalService::new(config.clone()).await.unwrap();
        assert_empty_asset_status(&service);

        assert_hash_mismatch_is_capacity_neutral(&mut service, hash, &bytes);

        assert_eq!(
            service.enqueue_asset_source(hash, bytes.clone()).unwrap(),
            AssetAdmission::Queued { content_hash: hash }
        );
        let queued = service.asset_status();
        assert_eq!(queued.store.records, 1);
        assert_eq!(queued.store.pending_imports, 1);
        assert_eq!(queued.store.pending_source_bytes, bytes.len() as u64);
        assert_eq!(queued.renderer.pending_uploads, 0);

        let imported = service
            .process_next_asset_import()
            .expect("one admitted import should be processed");
        assert_eq!(imported.content_hash, hash);
        assert_eq!(imported.state, AssetState::Ready);
        assert_eq!(imported.mesh_count, 1);
        let record = service.asset_record(hash).unwrap();
        assert_eq!(record.state, AssetState::Ready);
        assert_eq!(record.source_bytes, bytes.len() as u64);
        assert!(record.decoded_bytes > 0);
        assert_eq!(service.asset_status().store.pending_source_bytes, 0);

        assert_eq!(
            service.enqueue_asset_upload(key).unwrap(),
            AssetUploadAdmission::Queued { key }
        );
        assert_eq!(service.asset_status().renderer.pending_uploads, 1);
        let uploaded = service
            .process_next_asset_upload()
            .expect("one admitted GPU upload should be processed");
        assert_eq!(uploaded.key, key);
        assert_eq!(uploaded.vertex_count, 3);
        assert_eq!(service.asset_status().renderer.resident_meshes, 1);

        assert!(matches!(
            service
                .submit_patch(scene_patch(camera, triangle, hash))
                .unwrap(),
            GatewayAdmission::Queued { .. }
        ));
        let response = service.process_next().unwrap().unwrap();
        let GatewayResponse::PatchApplied { receipt } = response else {
            panic!("expected patch response");
        };
        assert_eq!(receipt.new_revision, SceneRevision::new(1));

        service.request_observation(request(1, camera)).unwrap();
        let observation = wait_for_observation(&service);
        assert_center_entity(&observation, triangle);
        assert_eq!(observation.metadata().scene_revision, SceneRevision::new(1));

        let expected_hash = service.logical_hash().unwrap();
        let expected_replay = service.replay_bytes();
        let recovery = service.recovery_point().unwrap();
        drop(service);

        let mut restored = LocalService::restore(config, &recovery).await.unwrap();
        assert_empty_asset_status(&restored);
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
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replay_bytes(), expected_replay);

        restored.enqueue_asset_source(hash, bytes).unwrap();
        assert_eq!(
            restored.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        restored.enqueue_asset_upload(key).unwrap();
        assert_eq!(restored.process_next_asset_upload().unwrap().key, key);
        assert_eq!(restored.status().scene_revision, SceneRevision::new(1));
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replay_bytes(), expected_replay);

        restored.request_observation(request(3, camera)).unwrap();
        let rehydrated = wait_for_observation(&restored);
        assert_center_entity(&rehydrated, triangle);
        assert_eq!(rehydrated.metadata().scene_revision, SceneRevision::new(1));
    });
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn exact_hash_rehydration_restores_a_textured_asset_only_after_explicit_work() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let bytes = textured_fixture();
        let hash = content_hash(&bytes);
        let key = AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        };
        let camera = StableEntityId::new(1).unwrap();
        let triangle = StableEntityId::new(2).unwrap();
        let mut service = LocalService::new(config.clone()).await.unwrap();
        service.enqueue_asset_source(hash, bytes.clone()).unwrap();
        assert_eq!(
            service.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        service.enqueue_asset_upload(key).unwrap();
        let uploaded = service.process_next_asset_upload().unwrap();
        assert!(uploaded.texture_uploaded);
        assert_eq!(uploaded.texture_byte_len, 4);
        assert_eq!(service.asset_status().renderer.resident_textures, 1);
        assert_eq!(service.asset_status().renderer.resident_texture_bytes, 4);
        service
            .submit_patch(scene_patch(camera, triangle, hash))
            .unwrap();
        service.process_next().unwrap().unwrap();
        let expected_hash = service.logical_hash().unwrap();
        let expected_replay = service.replay_bytes();
        let recovery = service.recovery_point().unwrap();
        drop(service);

        let mut restored = LocalService::restore(config, &recovery).await.unwrap();
        assert_empty_asset_status(&restored);
        assert!(matches!(
            restored.request_observation(request(10, camera)),
            Err(LocalServiceError::Engine(error))
                if matches!(
                    error.as_ref(),
                    EngineError::Renderer(RendererError::AssetUnavailable { key: missing, .. })
                        if *missing == key
                )
        ));
        restored.enqueue_asset_source(hash, bytes).unwrap();
        assert_eq!(
            restored.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        restored.enqueue_asset_upload(key).unwrap();
        let rehydrated_upload = restored.process_next_asset_upload().unwrap();
        assert!(rehydrated_upload.texture_uploaded);
        assert_eq!(restored.asset_status().renderer.resident_textures, 1);
        assert_eq!(restored.logical_hash().unwrap(), expected_hash);
        assert_eq!(restored.replay_bytes(), expected_replay);
        restored.request_observation(request(11, camera)).unwrap();
        assert_center_entity(&wait_for_observation(&restored), triangle);
    });
}

#[test]
#[ignore = "requires an approved DX12 or Vulkan conformance adapter"]
fn explicit_eviction_is_capacity_exact_and_logically_neutral_before_rehydration() {
    pollster::block_on(async {
        let config = LocalServiceConfig::new(WIDTH, HEIGHT);
        let bytes = two_mesh_fixture();
        let hash = content_hash(&bytes);
        let first_key = AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        };
        let second_key = AssetMeshKey {
            content_hash: hash,
            mesh_index: 1,
        };
        let camera = StableEntityId::new(1).unwrap();
        let triangle = StableEntityId::new(2).unwrap();
        let mut service = LocalService::new(config).await.unwrap();

        service.enqueue_asset_source(hash, bytes.clone()).unwrap();
        let queued = service.evict_asset(hash);
        assert_eq!(queued.content_hash, hash);
        assert_eq!(queued.store.previous_state, Some(AssetState::Queued));
        assert_eq!(queued.store.removed_pending_imports, 1);
        assert_eq!(
            queued.store.released_pending_source_bytes,
            u64::try_from(bytes.len()).unwrap()
        );
        assert!(queued.renderer.is_already_absent());
        assert_empty_asset_status(&service);
        assert!(service.evict_asset(hash).is_already_absent());

        service.enqueue_asset_source(hash, bytes.clone()).unwrap();
        assert_eq!(
            service.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        let decoded_bytes = service.asset_record(hash).unwrap().decoded_bytes;
        assert_eq!(service.asset_record(hash).unwrap().mesh_count, 2);
        service.enqueue_asset_upload(first_key).unwrap();
        service.enqueue_asset_upload(second_key).unwrap();
        assert_eq!(service.process_next_asset_upload().unwrap().key, first_key);
        assert_eq!(service.asset_status().renderer.pending_uploads, 1);
        assert_eq!(service.asset_status().renderer.resident_meshes, 1);

        service
            .submit_patch(scene_patch(camera, triangle, hash))
            .unwrap();
        service.process_next().unwrap().unwrap();
        service.request_observation(request(20, camera)).unwrap();
        assert_center_entity(&wait_for_observation(&service), triangle);
        let expected_revision = service.status().scene_revision;
        let expected_hash = service.logical_hash().unwrap();
        let expected_replay = service.replay_bytes();
        let expected_next_frame = service.recovery_point().unwrap().next_frame_id();

        let eviction = service.evict_asset(hash);
        assert_eq!(eviction.store.previous_state, Some(AssetState::Ready));
        assert_eq!(eviction.store.released_resident_cpu_bytes, decoded_bytes);
        assert_eq!(eviction.store.removed_meshes, 2);
        assert_eq!(eviction.store.removed_textures, 0);
        assert_eq!(eviction.renderer.removed_pending_uploads, 1);
        assert_eq!(eviction.renderer.released_pending_bytes, 96);
        assert_eq!(eviction.renderer.removed_resident_meshes, 1);
        assert_eq!(eviction.renderer.released_resident_bytes, 96);
        assert_eq!(eviction.renderer.removed_pending_textures, 0);
        assert_eq!(eviction.renderer.removed_resident_textures, 0);
        assert_empty_asset_status(&service);
        assert!(service.asset_record(hash).is_none());
        assert_asset_reference(&service, triangle, hash);
        assert_eq!(service.status().scene_revision, expected_revision);
        assert_eq!(service.logical_hash().unwrap(), expected_hash);
        assert_eq!(service.replay_bytes(), expected_replay);
        assert_eq!(
            service.recovery_point().unwrap().next_frame_id(),
            expected_next_frame
        );
        assert!(matches!(
            service.request_observation(request(21, camera)),
            Err(LocalServiceError::Engine(error))
                if matches!(
                    error.as_ref(),
                    EngineError::Renderer(RendererError::AssetUnavailable { key, .. })
                        if *key == first_key
                )
        ));
        assert_eq!(
            service.recovery_point().unwrap().next_frame_id(),
            expected_next_frame
        );
        assert!(service.evict_asset(hash).is_already_absent());

        service.enqueue_asset_source(hash, bytes).unwrap();
        assert_eq!(
            service.process_next_asset_import().unwrap().state,
            AssetState::Ready
        );
        service.enqueue_asset_upload(first_key).unwrap();
        assert_eq!(service.process_next_asset_upload().unwrap().key, first_key);
        assert_eq!(service.status().scene_revision, expected_revision);
        assert_eq!(service.logical_hash().unwrap(), expected_hash);
        assert_eq!(service.replay_bytes(), expected_replay);
        service.request_observation(request(22, camera)).unwrap();
        let rehydrated = wait_for_observation(&service);
        assert_center_entity(&rehydrated, triangle);
        assert_eq!(rehydrated.metadata().frame_id, expected_next_frame);
    });
}

fn assert_hash_mismatch_is_capacity_neutral(
    service: &mut LocalService,
    hash: cogniform_protocol::ContentHash,
    bytes: &[u8],
) {
    let mut mismatched = bytes.to_vec();
    mismatched.push(0);
    assert!(matches!(
        service.enqueue_asset_source(hash, mismatched),
        Err(LocalServiceError::Asset(error))
            if matches!(error.as_ref(), AssetError::ContentHashMismatch { expected, .. } if *expected == hash)
    ));
    assert_empty_asset_status(service);
}

fn assert_empty_asset_status(service: &LocalService) {
    let status = service.asset_status();
    assert_eq!(status.store.records, 0);
    assert_eq!(status.store.pending_imports, 0);
    assert_eq!(status.store.pending_source_bytes, 0);
    assert_eq!(status.store.resident_cpu_bytes, 0);
    assert_eq!(status.renderer.pending_uploads, 0);
    assert_eq!(status.renderer.pending_bytes, 0);
    assert_eq!(status.renderer.resident_meshes, 0);
    assert_eq!(status.renderer.resident_bytes, 0);
    assert_eq!(status.renderer.pending_textures, 0);
    assert_eq!(status.renderer.pending_texture_bytes, 0);
    assert_eq!(status.renderer.resident_textures, 0);
    assert_eq!(status.renderer.resident_texture_bytes, 0);
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
            limit: core::num::NonZeroU32::new(1).unwrap(),
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
        observation_id: ObservationId::new(nonce).unwrap(),
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
        assert!(Instant::now() < deadline, "asset observation timed out");
        std::thread::yield_now();
    }
}

fn assert_center_entity(observation: &Observation, expected: StableEntityId) {
    let ObservationPayload::EntityId(pixels) = observation.payload() else {
        panic!("expected entity-ID observation");
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
        .map(|pair| u8::from_str_radix(core::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

fn textured_fixture() -> Vec<u8> {
    let mut binary = Vec::new();
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.5_f32, 0.5]; 3] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let mut png_bytes = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png_bytes, 1, 1);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().unwrap();
        writer.write_image_data(&[128, 64, 32, 255]).unwrap();
    }
    let image_offset = binary.len();
    binary.extend_from_slice(&png_bytes);
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":2,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"material":0}}]}}]}}"#,
        binary_length = binary.len(),
        image_length = png_bytes.len(),
    );
    glb_with_json(&json, &binary)
}

fn two_mesh_fixture() -> Vec<u8> {
    let mut binary = Vec::with_capacity(36);
    for position in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0}}]},{"primitives":[{"attributes":{"POSITION":0}}]}]}"#;
    glb_with_json(json, &binary)
}

fn glb_with_json(json: &str, binary: &[u8]) -> Vec<u8> {
    let mut json = json.as_bytes().to_vec();
    json.resize(json.len().next_multiple_of(4), b' ');
    let mut binary = binary.to_vec();
    binary.resize(binary.len().next_multiple_of(4), 0);
    let length = 12 + 8 + json.len() + 8 + binary.len();
    let mut output = Vec::with_capacity(length);
    output.extend_from_slice(b"glTF");
    output.extend_from_slice(&2_u32.to_le_bytes());
    output.extend_from_slice(&u32::try_from(length).unwrap().to_le_bytes());
    output.extend_from_slice(&u32::try_from(json.len()).unwrap().to_le_bytes());
    output.extend_from_slice(&0x4e4f_534a_u32.to_le_bytes());
    output.extend_from_slice(&json);
    output.extend_from_slice(&u32::try_from(binary.len()).unwrap().to_le_bytes());
    output.extend_from_slice(&0x004e_4942_u32.to_le_bytes());
    output.extend_from_slice(&binary);
    output
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}
