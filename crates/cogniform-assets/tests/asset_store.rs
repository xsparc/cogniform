//! Fail-closed content-addressed GLB store contracts.

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_assets::{
    AssetDiagnosticCode, AssetError, AssetMeshKey, AssetState, AssetStore, AssetStoreConfig,
    UnsupportedAssetPolicy, content_hash,
};

fn fixture() -> Vec<u8> {
    decode_hex(include_str!("../../../tests/assets/triangle.glb.hex"))
}

fn decode_hex(value: &str) -> Vec<u8> {
    let encoded = value.trim().as_bytes();
    assert_eq!(encoded.len() % 2, 0);
    encoded
        .chunks_exact(2)
        .map(|pair| {
            let pair = core::str::from_utf8(pair).expect("fixture is ASCII");
            u8::from_str_radix(pair, 16).expect("fixture is hexadecimal")
        })
        .collect()
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

#[test]
fn verified_fixture_decodes_only_when_explicitly_processed() {
    let bytes = fixture();
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();

    store.enqueue(hash, bytes).expect("fixture should queue");
    assert_eq!(store.record(hash).unwrap().state, AssetState::Queued);
    assert_eq!(store.stats().pending_imports, 1);
    assert_eq!(store.stats().resident_cpu_bytes, 0);

    let outcome = store.process_next().expect("one import should process");
    assert_eq!(outcome.state, AssetState::Ready);
    assert_eq!(outcome.mesh_count, 1);
    assert_eq!(store.stats().pending_imports, 0);
    assert_eq!(store.stats().resident_cpu_bytes, 36);

    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .expect("decoded fixture should produce an upload job");
    assert_eq!(upload.vertices().len(), 3);
    assert_eq!(upload.byte_len(), 36);
    assert_color(upload.base_color(), [0.2, 0.6, 0.9, 1.0]);
}

#[test]
fn content_hash_mismatch_fails_before_record_or_queue_mutation() {
    let bytes = fixture();
    let wrong = content_hash(b"different immutable bytes");
    let mut store = AssetStore::default();
    assert!(matches!(
        store.enqueue(wrong, bytes),
        Err(AssetError::ContentHashMismatch { .. })
    ));
    assert_eq!(store.stats().records, 0);
    assert_eq!(store.stats().pending_imports, 0);
}

#[test]
fn every_truncated_fixture_prefix_is_rejected_without_a_panic() {
    let bytes = fixture();
    for end in 0..bytes.len() {
        let source = bytes[..end].to_vec();
        let hash = content_hash(&source);
        let mut store = AssetStore::default();
        store
            .enqueue(hash, source)
            .expect("bounded prefix should queue");
        let outcome = store.process_next().expect("prefix should be processed");
        assert_eq!(outcome.state, AssetState::Rejected, "prefix length {end}");
        assert_eq!(store.record(hash).unwrap().diagnostics.len(), 1);
    }
}

#[test]
fn unsupported_extensions_are_typed_and_proxy_policy_is_explicit() {
    let source = fixture();
    let json = r#"{"asset":{"version":"2.0"},"extensionsUsed":["EXT_example"],"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}]}"#;
    let bytes = glb_with_json(json, &source[source.len() - 36..]);
    let hash = content_hash(&bytes);
    let mut rejecting = AssetStore::default();
    rejecting.enqueue(hash, bytes.clone()).unwrap();
    assert_eq!(
        rejecting.process_next().unwrap().state,
        AssetState::Rejected
    );
    assert_eq!(
        rejecting.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::UnsupportedExtension
    );

    let mut proxying = AssetStore::new(AssetStoreConfig {
        unsupported_policy: UnsupportedAssetPolicy::ProxyCuboid,
        ..AssetStoreConfig::default()
    });
    proxying.enqueue(hash, bytes).unwrap();
    assert_eq!(
        proxying.process_next().unwrap().state,
        AssetState::ProxyReady
    );
    let upload = proxying
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.vertices().len(), 36);
    assert_color(upload.base_color(), [1.0, 0.0, 1.0, 1.0]);
}

#[test]
fn malformed_extension_declarations_never_receive_a_proxy() {
    let bytes = glb_with_json(
        r#"{"asset":{"version":"2.0"},"extensionsUsed":"not-an-array","buffers":[],"bufferViews":[],"accessors":[],"meshes":[]}"#,
        &[],
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::new(AssetStoreConfig {
        unsupported_policy: UnsupportedAssetPolicy::ProxyCuboid,
        ..AssetStoreConfig::default()
    });
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );
}

#[test]
fn standard_scene_selector_is_typed_but_malformed_selector_never_proxies() {
    let source = fixture();
    let base = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}]}"#;
    let base = base.strip_suffix('}').unwrap();
    let binary = &source[source.len() - 36..];
    let valid = glb_with_json(&format!(r#"{base},"scene":0,"scenes":[]}}"#), binary);
    let valid_hash = content_hash(&valid);
    let mut store = AssetStore::new(AssetStoreConfig {
        unsupported_policy: UnsupportedAssetPolicy::ProxyCuboid,
        ..AssetStoreConfig::default()
    });
    store.enqueue(valid_hash, valid).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::ProxyReady);
    assert_eq!(
        store.record(valid_hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::UnsupportedFeature
    );

    let malformed = glb_with_json(&format!(r#"{base},"scene":"zero"}}"#), binary);
    let malformed_hash = content_hash(&malformed);
    store.enqueue(malformed_hash, malformed).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(malformed_hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );
}

#[test]
fn decoded_vertex_and_pending_source_limits_fail_closed() {
    let bytes = fixture();
    let hash = content_hash(&bytes);
    let mut config = AssetStoreConfig::default();
    config.limits.max_vertices_per_mesh = NonZeroU32::new(2).unwrap();
    let mut store = AssetStore::new(config);
    store.enqueue(hash, bytes.clone()).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::CollectionLimitExceeded
    );

    let mut config = AssetStoreConfig::default();
    config.limits.max_pending_imports = NonZeroU32::new(2).unwrap();
    config.limits.max_pending_source_bytes =
        NonZeroU64::new(u64::try_from(bytes.len()).unwrap()).unwrap();
    let mut store = AssetStore::new(config);
    store.enqueue(hash, bytes).unwrap();
    let second = vec![0_u8];
    assert!(matches!(
        store.enqueue(content_hash(&second), second),
        Err(AssetError::PendingSourceBytesExceeded { .. })
    ));
}

fn assert_color(actual: [cogniform_protocol::UnitF32; 4], expected: [f32; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual.get() - expected).abs() <= f32::EPSILON);
    }
}
