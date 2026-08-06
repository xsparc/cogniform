//! Fail-closed content-addressed GLB store contracts.

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_assets::{
    ASSET_VERTEX_BYTES, AssetDiagnosticCode, AssetError, AssetMeshKey, AssetState, AssetStore,
    AssetStoreConfig, UnsupportedAssetPolicy, content_hash,
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

fn triangle_binary() -> Vec<u8> {
    let mut binary = Vec::with_capacity(36);
    for vertex in [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ] {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    binary
}

fn triangle_glb_with_material(pbr_fields: &str) -> Vec<u8> {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":36}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}}],"materials":[{{"pbrMetallicRoughness":{{{pbr_fields}}}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0,"mode":4}}]}}]}}"#
    );
    glb_with_json(&json, &triangle_binary())
}

fn triangle_glb_without_material() -> Vec<u8> {
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}]}"#;
    glb_with_json(json, &triangle_binary())
}

fn glb_with_normals(
    normals: [[f32; 3]; 3],
    normal_count: u32,
    normal_view_length: u32,
    normalized: bool,
) -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
    ];
    let mut binary = Vec::with_capacity(72);
    for vertex in positions.into_iter().chain(normals) {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":72}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":{normal_view_length}}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"byteOffset":0,"componentType":5126,"count":{normal_count},"type":"VEC3","normalized":{normalized}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1}},"mode":4}}]}}]}}"#
    );
    glb_with_json(&json, &binary)
}

fn process_with_proxy_policy(bytes: Vec<u8>) -> (AssetStore, cogniform_protocol::ContentHash) {
    let hash = content_hash(&bytes);
    let mut store = AssetStore::new(AssetStoreConfig {
        unsupported_policy: UnsupportedAssetPolicy::ProxyCuboid,
        ..AssetStoreConfig::default()
    });
    store.enqueue(hash, bytes).unwrap();
    store.process_next().unwrap();
    (store, hash)
}

fn indexed_glb_with_normals() -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let normals = [
        [0.0_f32, 0.0, 1.0],
        [0.0, 1.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 0.0, -1.0],
    ];
    let mut binary = Vec::with_capacity(104);
    for vertex in positions.into_iter().chain(normals) {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in [2_u16, 0, 1] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":102}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48},{"buffer":0,"byteOffset":48,"byteLength":48},{"buffer":0,"byteOffset":96,"byteLength":6}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":1,"byteOffset":0,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":2,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"NORMAL":1},"indices":2,"mode":4}]}]}"#;
    glb_with_json(json, &binary)
}

fn indexed_glb_with_texcoords(
    texcoords: [[f32; 2]; 4],
    texcoord_count: u32,
    texcoord_view_length: u32,
    component_type: u32,
    normalized: bool,
) -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let mut binary = Vec::with_capacity(88);
    for vertex in positions {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in texcoords {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in [2_u16, 0, 1] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":86}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":48,"byteLength":{texcoord_view_length}}},{{"buffer":0,"byteOffset":80,"byteLength":6}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":1,"byteOffset":0,"componentType":{component_type},"count":{texcoord_count},"type":"VEC2","normalized":{normalized}}},{{"bufferView":2,"byteOffset":0,"componentType":5123,"count":3,"type":"SCALAR"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"TEXCOORD_0":1}},"indices":2,"mode":4}}]}}]}}"#
    );
    glb_with_json(&json, &binary)
}

fn degenerate_position_only_glb() -> Vec<u8> {
    let mut binary = Vec::with_capacity(36);
    for vertex in [[0.0_f32; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]] {
        for value in vertex {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}]}"#;
    glb_with_json(json, &binary)
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
    assert_eq!(store.stats().resident_cpu_bytes, 96);

    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .expect("decoded fixture should produce an upload job");
    assert_eq!(upload.vertices().len(), 3);
    assert_eq!(upload.byte_len(), 96);
    for vertex in upload.vertices() {
        assert_normal(vertex.normal, [0.0, 0.0, 1.0]);
        assert_texcoord(vertex.texcoord_0, [0.0, 0.0]);
    }
    assert_color(upload.base_color(), [0.2, 0.6, 0.9, 1.0]);
    assert_color(upload.material().base_color(), [0.2, 0.6, 0.9, 1.0]);
    assert_eq!(
        upload.material().metallic().get().to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        upload.material().roughness().get().to_bits(),
        0.8_f32.to_bits()
    );
}

#[test]
fn explicit_material_defaults_and_no_material_fallback_are_retained() {
    let explicit = triangle_glb_with_material(r#""baseColorFactor":[0.3,0.4,0.5,0.6]"#);
    let explicit_hash = content_hash(&explicit);
    let mut explicit_store = AssetStore::default();
    explicit_store.enqueue(explicit_hash, explicit).unwrap();
    assert_eq!(
        explicit_store.process_next().unwrap().state,
        AssetState::Ready
    );
    let explicit_upload = explicit_store
        .upload_job(AssetMeshKey {
            content_hash: explicit_hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_color(
        explicit_upload.material().base_color(),
        [0.3, 0.4, 0.5, 0.6],
    );
    assert_eq!(
        explicit_upload.material().metallic().get().to_bits(),
        1.0_f32.to_bits()
    );
    assert_eq!(
        explicit_upload.material().roughness().get().to_bits(),
        1.0_f32.to_bits()
    );

    let unmaterialed = triangle_glb_without_material();
    let unmaterialed_hash = content_hash(&unmaterialed);
    let mut unmaterialed_store = AssetStore::default();
    unmaterialed_store
        .enqueue(unmaterialed_hash, unmaterialed)
        .unwrap();
    assert_eq!(
        unmaterialed_store.process_next().unwrap().state,
        AssetState::Ready
    );
    let unmaterialed_upload = unmaterialed_store
        .upload_job(AssetMeshKey {
            content_hash: unmaterialed_hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_color(
        unmaterialed_upload.material().base_color(),
        [0.8, 0.8, 0.8, 1.0],
    );
    assert_eq!(
        unmaterialed_upload.material().metallic().get().to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        unmaterialed_upload.material().roughness().get().to_bits(),
        0.8_f32.to_bits()
    );
    assert_eq!(explicit_upload.byte_len(), unmaterialed_upload.byte_len());
}

#[test]
fn out_of_range_material_factors_are_rejected() {
    for fields in [
        r#""metallicFactor":-0.1,"roughnessFactor":0.5"#,
        r#""metallicFactor":0.5,"roughnessFactor":1.1"#,
    ] {
        let bytes = triangle_glb_with_material(fields);
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }
}

#[test]
fn finite_source_normals_are_normalized_and_retained() {
    let bytes = glb_with_normals([[0.0, 2.0, 2.0]; 3], 3, 36, false);
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.byte_len(), 96);
    for vertex in upload.vertices() {
        assert_normal(
            vertex.normal,
            [
                0.0,
                core::f32::consts::FRAC_1_SQRT_2,
                core::f32::consts::FRAC_1_SQRT_2,
            ],
        );
        assert_texcoord(vertex.texcoord_0, [0.0, 0.0]);
    }
}

#[test]
fn indexed_positions_and_normals_expand_with_the_same_source_index() {
    let bytes = indexed_glb_with_normals();
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.byte_len(), 96);
    for (vertex, expected) in
        upload
            .vertices()
            .iter()
            .zip([[1.0, 0.0, 0.0], [0.0, 0.0, 1.0], [0.0, 1.0, 0.0]])
    {
        assert_normal(vertex.normal, expected);
        assert_texcoord(vertex.texcoord_0, [0.0, 0.0]);
    }
}

#[test]
fn indexed_primary_texcoords_expand_with_the_same_source_index() {
    let bytes = indexed_glb_with_texcoords(
        [[-0.25, 1.25], [2.0, -3.0], [0.5, 0.75], [9.0, 11.0]],
        4,
        32,
        5_126,
        false,
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.byte_len(), 3 * ASSET_VERTEX_BYTES);
    for (vertex, expected) in
        upload
            .vertices()
            .iter()
            .zip([[0.5, 0.75], [-0.25, 1.25], [2.0, -3.0]])
    {
        assert_texcoord(vertex.texcoord_0, expected);
    }
}

#[test]
fn invalid_primary_texcoords_never_receive_a_proxy() {
    let cases = [
        (
            indexed_glb_with_texcoords(
                [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [f32::NAN, 0.0]],
                4,
                32,
                5_126,
                false,
            ),
            AssetDiagnosticCode::InvalidTexcoord,
        ),
        (
            indexed_glb_with_texcoords([[0.0; 2]; 4], 3, 32, 5_126, false),
            AssetDiagnosticCode::InvalidTexcoord,
        ),
        (
            indexed_glb_with_texcoords([[0.0; 2]; 4], 4, 24, 5_126, false),
            AssetDiagnosticCode::InvalidBufferRange,
        ),
    ];
    for (bytes, expected) in cases {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(store.record(hash).unwrap().diagnostics[0].code, expected);
        assert!(
            store
                .upload_job(AssetMeshKey {
                    content_hash: hash,
                    mesh_index: 0,
                })
                .is_err()
        );
    }
}

#[test]
fn unsupported_primary_texcoord_encodings_obey_proxy_policy() {
    for bytes in [
        indexed_glb_with_texcoords([[0.0; 2]; 4], 4, 32, 5_126, true),
        indexed_glb_with_texcoords([[0.0; 2]; 4], 4, 32, 5_123, false),
    ] {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::UnsupportedAccessor
        );
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert_eq!(upload.byte_len(), 36 * ASSET_VERTEX_BYTES);
        assert!(
            upload
                .vertices()
                .iter()
                .all(|vertex| vertex.texcoord_0.iter().all(|value| value.get() == 0.0))
        );
    }
}

#[test]
fn invalid_normal_values_and_ranges_never_receive_a_proxy() {
    let cases = [
        (
            glb_with_normals([[0.0; 3], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]], 3, 36, false),
            AssetDiagnosticCode::InvalidNormal,
        ),
        (
            glb_with_normals(
                [[f32::NAN, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 1.0]],
                3,
                36,
                false,
            ),
            AssetDiagnosticCode::InvalidNormal,
        ),
        (
            glb_with_normals([[0.0, 0.0, 1.0]; 3], 2, 36, false),
            AssetDiagnosticCode::InvalidNormal,
        ),
        (
            glb_with_normals([[0.0, 0.0, 1.0]; 3], 3, 24, false),
            AssetDiagnosticCode::InvalidBufferRange,
        ),
    ];
    for (bytes, expected) in cases {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(store.record(hash).unwrap().diagnostics[0].code, expected);
    }
}

#[test]
fn degenerate_position_only_triangle_never_receives_a_proxy() {
    let (store, hash) = process_with_proxy_policy(degenerate_position_only_glb());
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidNormal
    );
}

#[test]
fn unsupported_normal_encoding_obeys_explicit_proxy_policy() {
    let bytes = glb_with_normals([[0.0, 0.0, 1.0]; 3], 3, 36, true);
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::UnsupportedAccessor
    );
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert_eq!(upload.byte_len(), 36 * ASSET_VERTEX_BYTES);
    for vertex in upload.vertices() {
        let length = vertex
            .normal
            .iter()
            .map(|value| value.get() * value.get())
            .sum::<f32>()
            .sqrt();
        assert!((length - 1.0).abs() <= f32::EPSILON);
        assert_texcoord(vertex.texcoord_0, [0.0, 0.0]);
    }
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
    assert_eq!(
        upload.material().metallic().get().to_bits(),
        0.0_f32.to_bits()
    );
    assert_eq!(
        upload.material().roughness().get().to_bits(),
        0.8_f32.to_bits()
    );
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
    config.limits.max_asset_decoded_bytes = NonZeroU64::new(95).unwrap();
    let mut store = AssetStore::new(config);
    store.enqueue(hash, bytes.clone()).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::ByteLimitExceeded
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

fn assert_normal(actual: [cogniform_protocol::FiniteF32; 3], expected: [f32; 3]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual.get() - expected).abs() <= 1.0e-6);
    }
}

fn assert_texcoord(actual: [cogniform_protocol::FiniteF32; 2], expected: [f32; 2]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert_eq!(actual.get().to_bits(), expected.to_bits());
    }
}
