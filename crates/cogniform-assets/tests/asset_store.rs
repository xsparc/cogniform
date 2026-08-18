//! Fail-closed content-addressed GLB store contracts.

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_assets::{
    ASSET_VERTEX_BYTES, AssetDiagnosticCode, AssetError, AssetMeshKey, AssetState, AssetStore,
    AssetStoreConfig, UnsupportedAssetPolicy, content_hash,
};
use cogniform_protocol::FiniteF32;

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

fn encode_png(
    width: u32,
    height: u32,
    color: png::ColorType,
    depth: png::BitDepth,
    pixels: &[u8],
) -> Vec<u8> {
    let mut png_bytes = Vec::new();
    {
        let mut png_encoder = png::Encoder::new(&mut png_bytes, width, height);
        png_encoder.set_color(color);
        png_encoder.set_depth(depth);
        let mut writer = png_encoder.write_header().unwrap();
        writer.write_image_data(pixels).unwrap();
    }
    png_bytes
}

fn textured_triangle_glb(
    png: &[u8],
    pbr_fields: &str,
    texture_items: &str,
    image_items: &str,
    root_fields: &str,
    include_texcoords: bool,
) -> Vec<u8> {
    let mut binary = triangle_binary();
    for texcoord in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let image_offset = binary.len();
    binary.extend_from_slice(png);
    let attributes = if include_texcoords {
        r#""POSITION":0,"TEXCOORD_0":1"#
    } else {
        r#""POSITION":0"#
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{{pbr_fields}}}}}],"textures":[{texture_items}],"images":[{image_items}]{root_fields},"meshes":[{{"primitives":[{{"attributes":{{{attributes}}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_length = png.len(),
    );
    glb_with_json(&json, &binary)
}

fn rgba_texture_glb(pixels: &[[u8; 4]], width: u32, height: u32) -> Vec<u8> {
    let rgba = pixels.iter().flatten().copied().collect::<Vec<_>>();
    let png = encode_png(
        width,
        height,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &rgba,
    );
    textured_triangle_glb(
        &png,
        r#""baseColorFactor":[0.5,0.25,1.0,0.75],"baseColorTexture":{"index":0}"#,
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        true,
    )
}

#[derive(Clone, Copy)]
struct NormalTexturedFixture<'a> {
    tangents: [[f32; 4]; 3],
    tangent_count: u32,
    tangent_kind: &'a str,
    tangent_normalized: bool,
    include_normals: bool,
    include_tangents: bool,
    texcoord_attribute: &'a str,
    normal_texture_fields: &'a str,
}

impl Default for NormalTexturedFixture<'_> {
    fn default() -> Self {
        Self {
            tangents: [[1.0, 0.0, 0.0, 1.0]; 3],
            tangent_count: 3,
            tangent_kind: "VEC4",
            tangent_normalized: false,
            include_normals: true,
            include_tangents: true,
            texcoord_attribute: r#""TEXCOORD_0":3"#,
            normal_texture_fields: r#""index":0"#,
        }
    }
}

fn normal_textured_triangle_glb(png: &[u8], fixture: NormalTexturedFixture<'_>) -> Vec<u8> {
    let NormalTexturedFixture {
        tangents,
        tangent_count,
        tangent_kind,
        tangent_normalized,
        include_normals,
        include_tangents,
        texcoord_attribute,
        normal_texture_fields,
    } = fixture;
    let mut binary = triangle_binary();
    for normal in [[0.0_f32, 0.0, 1.0]; 3] {
        for value in normal {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for tangent in tangents {
        for value in tangent {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let image_offset = binary.len();
    binary.extend_from_slice(png);
    let mut attributes = vec![r#""POSITION":0"#];
    if include_normals {
        attributes.push(r#""NORMAL":1"#);
    }
    if include_tangents {
        attributes.push(r#""TANGENT":2"#);
    }
    if !texcoord_attribute.is_empty() {
        attributes.push(texcoord_attribute);
    }
    let attributes = attributes.join(",");
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":{tangent_count},"type":"{tangent_kind}","normalized":{tangent_normalized}}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"normalTexture":{{{normal_texture_fields}}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":4,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{{attributes}}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_length = png.len(),
    );
    glb_with_json(&json, &binary)
}

fn dual_textured_triangle_glb(base_png: &[u8], normal_png: &[u8], shared_image: bool) -> Vec<u8> {
    let mut binary = triangle_binary();
    for normal in [[0.0_f32, 0.0, 1.0]; 3] {
        for value in normal {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for tangent in [[1.0_f32, 0.0, 0.0, 1.0]; 3] {
        for value in tangent {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let base_offset = binary.len();
    binary.extend_from_slice(base_png);
    let normal_offset = binary.len();
    if !shared_image {
        binary.extend_from_slice(normal_png);
    }
    let (images, normal_source, image_views) = if shared_image {
        (
            r#"{"bufferView":4,"mimeType":"image/png"}"#.to_owned(),
            0,
            String::new(),
        )
    } else {
        (
            r#"{"bufferView":4,"mimeType":"image/png"},{"bufferView":5,"mimeType":"image/png"}"#
                .to_owned(),
            1,
            format!(
                r#",{{"buffer":0,"byteOffset":{normal_offset},"byteLength":{}}}"#,
                normal_png.len()
            ),
        )
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{base_offset},"byteLength":{base_length}}}{image_views}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}}}},"normalTexture":{{"index":1,"scale":0.5}}}}],"textures":[{{"source":0}},{{"source":{normal_source}}}],"images":[{images}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        base_length = base_png.len(),
    );
    glb_with_json(&json, &binary)
}

fn triple_textured_triangle_glb(
    base_png: &[u8],
    metallic_roughness_png: &[u8],
    normal_png: &[u8],
    shared_image: bool,
) -> Vec<u8> {
    let mut binary = triangle_binary();
    for normal in [[0.0_f32, 0.0, 1.0]; 3] {
        for value in normal {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for tangent in [[1.0_f32, 0.0, 0.0, 1.0]; 3] {
        for value in tangent {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for texcoord in [[0.0_f32, 0.0], [1.0, 0.0], [0.0, 1.0]] {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let base_offset = binary.len();
    binary.extend_from_slice(base_png);
    let metallic_roughness_offset = binary.len();
    let normal_offset;
    if shared_image {
        normal_offset = base_offset;
    } else {
        binary.extend_from_slice(metallic_roughness_png);
        normal_offset = binary.len();
        binary.extend_from_slice(normal_png);
    }
    let (image_views, images, metallic_roughness_source, normal_source) = if shared_image {
        (
            String::new(),
            r#"{"bufferView":4,"mimeType":"image/png"}"#.to_owned(),
            0,
            0,
        )
    } else {
        (
            format!(
                r#",{{"buffer":0,"byteOffset":{metallic_roughness_offset},"byteLength":{}}},{{"buffer":0,"byteOffset":{normal_offset},"byteLength":{}}}"#,
                metallic_roughness_png.len(),
                normal_png.len()
            ),
            r#"{"bufferView":4,"mimeType":"image/png"},{"bufferView":5,"mimeType":"image/png"},{"bufferView":6,"mimeType":"image/png"}"#.to_owned(),
            1,
            2,
        )
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{base_offset},"byteLength":{base_length}}}{image_views}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0}},"metallicRoughnessTexture":{{"index":1}},"metallicFactor":0.75,"roughnessFactor":0.5}},"normalTexture":{{"index":2,"scale":0.5}}}}],"textures":[{{"source":0}},{{"source":{metallic_roughness_source}}},{{"source":{normal_source}}}],"images":[{images}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        base_length = base_png.len(),
    );
    glb_with_json(&json, &binary)
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

fn indexed_glb_with_tangents(tangents: [[f32; 4]; 4]) -> Vec<u8> {
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let mut binary = Vec::with_capacity(120);
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for tangent in tangents {
        for value in tangent {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    for index in [2_u16, 0, 1] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":118}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":48},{"buffer":0,"byteOffset":48,"byteLength":64},{"buffer":0,"byteOffset":112,"byteLength":6}],"accessors":[{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"},{"bufferView":1,"componentType":5126,"count":4,"type":"VEC4"},{"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0,"TANGENT":1},"indices":2,"mode":4}]}]}"#;
    glb_with_json(json, &binary)
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
    assert!(store.stats().oldest_pending_import_age_micros.is_some());
    assert_eq!(store.stats().resident_cpu_bytes, 0);

    let outcome = store.process_next().expect("one import should process");
    assert_eq!(outcome.state, AssetState::Ready);
    assert_eq!(outcome.mesh_count, 1);
    assert_eq!(store.stats().pending_imports, 0);
    assert_eq!(store.stats().oldest_pending_import_age_micros, None);
    assert_eq!(store.stats().resident_cpu_bytes, 144);

    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .expect("decoded fixture should produce an upload job");
    assert_eq!(upload.vertices().len(), 3);
    assert_eq!(upload.byte_len(), 144);
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
fn queued_eviction_releases_exact_capacity_and_preserves_unrelated_fifo_order() {
    let first = fixture();
    let first_hash = content_hash(&first);
    let second = triangle_glb_without_material();
    let second_hash = content_hash(&second);
    let mut store = AssetStore::default();
    store.enqueue(first_hash, first.clone()).unwrap();
    store.enqueue(second_hash, second.clone()).unwrap();

    let eviction = store.evict(first_hash);
    assert_eq!(eviction.content_hash, first_hash);
    assert_eq!(eviction.previous_state, Some(AssetState::Queued));
    assert_eq!(eviction.removed_pending_imports, 1);
    assert_eq!(
        eviction.released_pending_source_bytes,
        u64::try_from(first.len()).unwrap()
    );
    assert_eq!(eviction.released_resident_cpu_bytes, 0);
    assert_eq!(eviction.removed_meshes, 0);
    assert_eq!(eviction.removed_textures, 0);
    assert_eq!(store.stats().records, 1);
    assert_eq!(store.stats().pending_imports, 1);
    assert_eq!(
        store.stats().pending_source_bytes,
        u64::try_from(second.len()).unwrap()
    );
    assert_eq!(store.process_next().unwrap().content_hash, second_hash);

    let absent = store.evict(first_hash);
    assert!(absent.is_already_absent());
    assert_eq!(absent.released_pending_source_bytes, 0);

    assert_eq!(
        store.enqueue(first_hash, first).unwrap(),
        cogniform_assets::AssetAdmission::Queued {
            content_hash: first_hash
        }
    );
}

#[test]
fn ready_and_rejected_eviction_release_only_their_retained_state() {
    let ready = fixture();
    let ready_hash = content_hash(&ready);
    let rejected = Vec::new();
    let rejected_hash = content_hash(&rejected);
    let mut store = AssetStore::default();
    store.enqueue(ready_hash, ready).unwrap();
    store.enqueue(rejected_hash, rejected).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    let ready_bytes = store.record(ready_hash).unwrap().decoded_bytes;
    assert!(ready_bytes > 0);

    let rejected_eviction = store.evict(rejected_hash);
    assert_eq!(rejected_eviction.previous_state, Some(AssetState::Rejected));
    assert_eq!(rejected_eviction.released_resident_cpu_bytes, 0);
    assert_eq!(rejected_eviction.removed_meshes, 0);
    assert_eq!(rejected_eviction.removed_textures, 0);
    assert_eq!(store.stats().resident_cpu_bytes, ready_bytes);

    let ready_eviction = store.evict(ready_hash);
    assert_eq!(ready_eviction.previous_state, Some(AssetState::Ready));
    assert_eq!(ready_eviction.released_resident_cpu_bytes, ready_bytes);
    assert_eq!(ready_eviction.removed_meshes, 1);
    assert_eq!(ready_eviction.removed_textures, 0);
    assert_eq!(store.stats().records, 0);
    assert_eq!(store.stats().resident_cpu_bytes, 0);
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
fn embedded_rgba_and_rgb_base_color_textures_are_bounded_and_retained() {
    let rgba = rgba_texture_glb(
        &[
            [255, 0, 0, 255],
            [0, 255, 0, 128],
            [0, 0, 255, 64],
            [255, 255, 255, 0],
        ],
        2,
        2,
    );
    let rgba_hash = content_hash(&rgba);
    let mut rgba_store = AssetStore::default();
    rgba_store.enqueue(rgba_hash, rgba).unwrap();
    assert_eq!(rgba_store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(rgba_store.record(rgba_hash).unwrap().decoded_bytes, 160);
    assert_eq!(rgba_store.stats().resident_cpu_bytes, 160);
    let upload = rgba_store
        .upload_job(AssetMeshKey {
            content_hash: rgba_hash,
            mesh_index: 0,
        })
        .unwrap();
    assert!(upload.material().has_base_color_texture());
    let texture = upload.base_color_texture().unwrap();
    assert_eq!(
        (texture.width(), texture.height(), texture.byte_len()),
        (2, 2, 16)
    );
    assert_eq!(
        texture.rgba8(),
        [
            255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 64, 255, 255, 255, 0,
        ]
    );
    let rgba_eviction = rgba_store.evict(rgba_hash);
    assert_eq!(rgba_eviction.removed_meshes, 1);
    assert_eq!(rgba_eviction.removed_textures, 1);
    assert_eq!(rgba_eviction.released_resident_cpu_bytes, 160);
    assert_eq!(rgba_store.stats().resident_cpu_bytes, 0);

    let rgb_png = encode_png(
        2,
        1,
        png::ColorType::Rgb,
        png::BitDepth::Eight,
        &[10, 20, 30, 40, 50, 60],
    );
    let expanded = textured_triangle_glb(
        &rgb_png,
        r#""baseColorTexture":{"index":0,"texCoord":0}"#,
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        true,
    );
    let expanded_hash = content_hash(&expanded);
    let mut expanded_store = AssetStore::default();
    expanded_store.enqueue(expanded_hash, expanded).unwrap();
    assert_eq!(
        expanded_store.process_next().unwrap().state,
        AssetState::Ready
    );
    assert_eq!(
        expanded_store
            .upload_job(AssetMeshKey {
                content_hash: expanded_hash,
                mesh_index: 0,
            })
            .unwrap()
            .base_color_texture()
            .unwrap()
            .rgba8(),
        [10, 20, 30, 255, 40, 50, 60, 255]
    );
}

#[test]
fn source_tangent_normal_texture_is_typed_normalized_and_retained() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 255, 255, 7],
    );
    let bytes = normal_textured_triangle_glb(
        &png,
        NormalTexturedFixture {
            tangents: [[2.0, 0.0, 0.0, -1.0]; 3],
            normal_texture_fields: r#""index":0,"texCoord":0,"scale":0.25"#,
            ..NormalTexturedFixture::default()
        },
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 148);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert!(!upload.material().has_base_color_texture());
    assert!(upload.base_color_texture().is_none());
    assert!(upload.material().has_normal_texture());
    assert_eq!(
        upload.material().normal_scale().to_bits(),
        0.25_f32.to_bits()
    );
    assert_eq!(upload.normal_texture().unwrap().rgba8(), [128, 255, 255, 7]);
    for vertex in upload.vertices() {
        assert_eq!(
            vertex.tangent.map(|value| value.get().to_bits()),
            [1.0_f32, 0.0, 0.0, -1.0].map(f32::to_bits)
        );
    }
    let eviction = store.evict(hash);
    assert_eq!(eviction.removed_textures, 1);
    assert_eq!(eviction.released_resident_cpu_bytes, 148);
}

#[test]
fn metallic_roughness_texture_is_linear_role_metadata_with_exact_texels() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[17, 64, 192, 231],
    );
    let bytes = textured_triangle_glb(
        &png,
        r#""metallicFactor":0.75,"roughnessFactor":0.5,"metallicRoughnessTexture":{"index":0,"texCoord":0}"#,
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        true,
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 148);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert!(!upload.material().has_base_color_texture());
    assert!(upload.material().has_metallic_roughness_texture());
    assert!(!upload.material().has_normal_texture());
    assert_eq!(
        upload.material().roughness().get().to_bits(),
        0.5_f32.to_bits()
    );
    assert_eq!(
        upload.material().metallic().get().to_bits(),
        0.75_f32.to_bits()
    );
    assert_eq!(
        upload.metallic_roughness_texture().unwrap().rgba8(),
        [17, 64, 192, 231]
    );
}

#[test]
fn dual_texture_roles_account_shared_and_distinct_images_exactly() {
    let base_png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[255, 0, 0, 255],
    );
    let normal_png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    for (shared_image, expected_bytes) in [(true, 148), (false, 152)] {
        let bytes = dual_textured_triangle_glb(&base_png, &normal_png, shared_image);
        let hash = content_hash(&bytes);
        let mut exact_config = AssetStoreConfig::default();
        exact_config.limits.max_asset_decoded_bytes = NonZeroU64::new(expected_bytes).unwrap();
        exact_config.limits.max_resident_cpu_bytes = NonZeroU64::new(expected_bytes).unwrap();
        let mut store = AssetStore::new(exact_config);
        store.enqueue(hash, bytes.clone()).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        assert_eq!(store.record(hash).unwrap().decoded_bytes, expected_bytes);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert!(upload.base_color_texture().is_some());
        assert!(upload.normal_texture().is_some());
        if shared_image {
            assert_eq!(
                upload.base_color_texture().unwrap().rgba8(),
                upload.normal_texture().unwrap().rgba8()
            );
        } else {
            assert_ne!(
                upload.base_color_texture().unwrap().rgba8(),
                upload.normal_texture().unwrap().rgba8()
            );
        }
        let eviction = store.evict(hash);
        assert_eq!(eviction.removed_textures, 2);
        assert_eq!(eviction.released_resident_cpu_bytes, expected_bytes);

        let mut narrow_config = exact_config;
        narrow_config.limits.max_asset_decoded_bytes = NonZeroU64::new(expected_bytes - 1).unwrap();
        let mut narrow = AssetStore::new(narrow_config);
        narrow.enqueue(hash, bytes).unwrap();
        assert_eq!(narrow.process_next().unwrap().state, AssetState::Rejected);
        assert_eq!(narrow.stats().resident_cpu_bytes, 0);
        assert_eq!(
            narrow.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::ByteLimitExceeded
        );
    }
}

#[test]
fn three_texture_roles_count_shared_cpu_images_once_and_roles_exactly() {
    let base_png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[255, 0, 0, 255],
    );
    let metallic_roughness_png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[7, 64, 192, 9],
    );
    let normal_png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    for (shared_image, expected_bytes) in [(true, 148), (false, 156)] {
        let bytes = triple_textured_triangle_glb(
            &base_png,
            &metallic_roughness_png,
            &normal_png,
            shared_image,
        );
        let hash = content_hash(&bytes);
        let mut exact_config = AssetStoreConfig::default();
        exact_config.limits.max_asset_decoded_bytes = NonZeroU64::new(expected_bytes).unwrap();
        exact_config.limits.max_resident_cpu_bytes = NonZeroU64::new(expected_bytes).unwrap();
        let mut store = AssetStore::new(exact_config);
        store.enqueue(hash, bytes.clone()).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert!(upload.base_color_texture().is_some());
        assert!(upload.metallic_roughness_texture().is_some());
        assert!(upload.normal_texture().is_some());
        if shared_image {
            assert_eq!(
                upload.base_color_texture().unwrap().rgba8(),
                upload.metallic_roughness_texture().unwrap().rgba8()
            );
        }
        let eviction = store.evict(hash);
        assert_eq!(eviction.removed_textures, 3);
        assert_eq!(eviction.released_resident_cpu_bytes, expected_bytes);

        let mut narrow_config = exact_config;
        narrow_config.limits.max_asset_decoded_bytes = NonZeroU64::new(expected_bytes - 1).unwrap();
        let mut narrow = AssetStore::new(narrow_config);
        narrow.enqueue(hash, bytes).unwrap();
        assert_eq!(narrow.process_next().unwrap().state, AssetState::Rejected);
        assert_eq!(narrow.stats().resident_cpu_bytes, 0);
    }
}

#[test]
fn invalid_source_tangent_values_fail_closed() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let invalid_tangents = [
        [[0.0, 0.0, 0.0, 1.0]; 3],
        [[f32::NAN, 0.0, 0.0, 1.0]; 3],
        [[1.0, 0.0, 0.0, 0.0]; 3],
        [
            [1.0, 0.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, -1.0],
            [1.0, 0.0, 0.0, 1.0],
        ],
    ];
    for tangents in invalid_tangents {
        let bytes = normal_textured_triangle_glb(
            &png,
            NormalTexturedFixture {
                tangents,
                ..NormalTexturedFixture::default()
            },
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidTangent
        );
    }
}

#[test]
fn unsupported_tangent_encodings_and_normal_texture_roles_fail_closed() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    for bytes in [
        normal_textured_triangle_glb(
            &png,
            NormalTexturedFixture {
                tangent_count: 2,
                ..NormalTexturedFixture::default()
            },
        ),
        normal_textured_triangle_glb(
            &png,
            NormalTexturedFixture {
                tangent_kind: "VEC3",
                ..NormalTexturedFixture::default()
            },
        ),
        normal_textured_triangle_glb(
            &png,
            NormalTexturedFixture {
                tangent_normalized: true,
                ..NormalTexturedFixture::default()
            },
        ),
    ] {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert!(matches!(
            store.record(hash).unwrap().state,
            AssetState::Rejected | AssetState::ProxyReady
        ));
        assert!(matches!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidTangent | AssetDiagnosticCode::UnsupportedAccessor
        ));
    }

    for (include_normals, include_tangents) in [(false, true), (true, false)] {
        let bytes = normal_textured_triangle_glb(
            &png,
            NormalTexturedFixture {
                include_normals,
                include_tangents,
                ..NormalTexturedFixture::default()
            },
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::UnsupportedFeature
        );
    }

    for fields in [r#""index":0,"texCoord":1"#, r#""index":0,"scale":1e999"#] {
        let bytes = normal_textured_triangle_glb(
            &png,
            NormalTexturedFixture {
                normal_texture_fields: fields,
                ..NormalTexturedFixture::default()
            },
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_ne!(store.record(hash).unwrap().state, AssetState::Ready);
    }

    let invalid_index = normal_textured_triangle_glb(
        &png,
        NormalTexturedFixture {
            normal_texture_fields: r#""index":999"#,
            ..NormalTexturedFixture::default()
        },
    );
    let (store, hash) = process_with_proxy_policy(invalid_index);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );

    let missing_coordinates = normal_textured_triangle_glb(
        &png,
        NormalTexturedFixture {
            texcoord_attribute: "",
            ..NormalTexturedFixture::default()
        },
    );
    let (store, hash) = process_with_proxy_policy(missing_coordinates);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidTexcoord
    );
}

#[test]
fn texture_resource_shape_and_coordinate_contract_is_typed() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let cases = [
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0,"texCoord":1}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        ),
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"sampler":0,"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            r#","samplers":[{}]"#,
            true,
        ),
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0}"#,
            r#"{"uri":"data:image/png;base64,AA==","mimeType":"image/png"}"#,
            "",
            true,
        ),
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/jpeg"}"#,
            "",
            true,
        ),
        textured_triangle_glb(
            &png,
            r#""metallicRoughnessTexture":{"index":0,"texCoord":1}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        ),
    ];
    for bytes in cases {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::UnsupportedFeature
        );
        assert!(
            !store
                .upload_job(AssetMeshKey {
                    content_hash: hash,
                    mesh_index: 0,
                })
                .unwrap()
                .material()
                .has_base_color_texture()
        );
    }

    for pbr_fields in [
        r#""baseColorTexture":{"index":0}"#,
        r#""metallicRoughnessTexture":{"index":0}"#,
    ] {
        let missing_coordinates = textured_triangle_glb(
            &png,
            pbr_fields,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            false,
        );
        let (store, hash) = process_with_proxy_policy(missing_coordinates);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidTexcoord
        );
    }

    let invalid_metallic_roughness_index = textured_triangle_glb(
        &png,
        r#""metallicRoughnessTexture":{"index":999}"#,
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        true,
    );
    let (store, hash) = process_with_proxy_policy(invalid_metallic_roughness_index);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );
}

#[test]
fn more_than_three_texture_or_image_resources_fail_closed() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    for bytes in [
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0},{"source":0},{"source":0},{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        ),
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        ),
    ] {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::CollectionLimitExceeded
        );
    }
}

#[test]
fn malformed_and_truncated_png_never_receive_a_proxy() {
    let png = encode_png(2, 2, png::ColorType::Rgba, png::BitDepth::Eight, &[7; 16]);
    for end in [0, 7, 28, png.len() - 1] {
        let bytes = textured_triangle_glb(
            &png[..end],
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidImage
        );
    }
    let mut trailing = png.clone();
    trailing.extend_from_slice(b"junk");
    let bytes = textured_triangle_glb(
        &trailing,
        r#""baseColorTexture":{"index":0}"#,
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        true,
    );
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidImage
    );
}

#[test]
fn texture_dimension_pixel_and_decoded_byte_limits_fail_before_adoption() {
    let bytes = rgba_texture_glb(&[[9; 4]; 4], 2, 2);
    let hash = content_hash(&bytes);
    let cases = [
        ("dimension", 1_u64),
        ("pixels", 3_u64),
        ("texture_bytes", 15_u64),
        ("decoder_bytes", 1_u64),
        ("asset_bytes", 159_u64),
        ("resident_bytes", 159_u64),
    ];
    for (kind, limit) in cases {
        let mut config = AssetStoreConfig::default();
        match kind {
            "dimension" => {
                config.limits.max_texture_dimension_2d =
                    NonZeroU32::new(u32::try_from(limit).unwrap()).unwrap();
            }
            "pixels" => config.limits.max_texture_pixels = NonZeroU64::new(limit).unwrap(),
            "texture_bytes" => {
                config.limits.max_texture_decoded_bytes = NonZeroU64::new(limit).unwrap();
            }
            "decoder_bytes" => {
                config.limits.max_texture_decoder_bytes = NonZeroU64::new(limit).unwrap();
            }
            "asset_bytes" => {
                config.limits.max_asset_decoded_bytes = NonZeroU64::new(limit).unwrap();
            }
            "resident_bytes" => {
                config.limits.max_resident_cpu_bytes = NonZeroU64::new(limit).unwrap();
            }
            _ => unreachable!(),
        }
        let mut store = AssetStore::new(config);
        store.enqueue(hash, bytes.clone()).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
        assert_eq!(store.stats().resident_cpu_bytes, 0);
        assert!(matches!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::CollectionLimitExceeded | AssetDiagnosticCode::ByteLimitExceeded
        ));
    }
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
    assert_eq!(upload.byte_len(), 144);
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
    assert_eq!(upload.byte_len(), 144);
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
fn indexed_source_tangents_expand_and_validate_unused_values() {
    let bytes = indexed_glb_with_tangents([
        [2.0, 0.0, 0.0, 1.0],
        [0.0, 3.0, 0.0, 1.0],
        [0.0, 0.0, 4.0, 1.0],
        [1.0, 1.0, 0.0, -1.0],
    ]);
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
    for (vertex, expected) in upload.vertices().iter().zip([
        [0.0, 0.0, 1.0, 1.0],
        [1.0, 0.0, 0.0, 1.0],
        [0.0, 1.0, 0.0, 1.0],
    ]) {
        assert_eq!(
            vertex.tangent.map(FiniteF32::get).map(f32::to_bits),
            expected.map(f32::to_bits)
        );
    }

    let invalid_unused = indexed_glb_with_tangents([
        [1.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, 1.0],
        [1.0, 0.0, 0.0, 1.0],
        [f32::NAN, 0.0, 0.0, 1.0],
    ]);
    let (store, hash) = process_with_proxy_policy(invalid_unused);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidTangent
    );
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
    assert_eq!(store.stats().oldest_pending_import_age_micros, None);
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
    let decoded_bytes = proxying.record(hash).unwrap().decoded_bytes;
    let eviction = proxying.evict(hash);
    assert_eq!(eviction.previous_state, Some(AssetState::ProxyReady));
    assert_eq!(eviction.released_resident_cpu_bytes, decoded_bytes);
    assert_eq!(eviction.removed_meshes, 1);
    assert_eq!(eviction.removed_textures, 0);
    assert_eq!(proxying.stats().records, 0);
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
