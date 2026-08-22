//! Fail-closed content-addressed GLB store contracts.

use core::num::{NonZeroU32, NonZeroU64};

use cogniform_assets::{
    ASSET_VERTEX_BYTES, AssetAlphaMode, AssetDiagnosticCode, AssetError, AssetMeshKey,
    AssetSampler, AssetSamplerFilter, AssetSamplerMinFilter, AssetSamplerWrap, AssetShadingModel,
    AssetState, AssetStore, AssetStoreConfig, AssetTextureTransform, UnsupportedAssetPolicy,
    content_hash,
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
    triangle_glb_with_material_fields(&format!(r#""pbrMetallicRoughness":{{{pbr_fields}}}"#))
}

fn triangle_glb_with_material_fields(material_fields: &str) -> Vec<u8> {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":36}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}}],"materials":[{{{material_fields}}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":0,"mode":4}}]}}]}}"#
    );
    glb_with_json(&json, &triangle_binary())
}

fn triangle_glb_without_material() -> Vec<u8> {
    let json = r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"mode":4}]}]}"#;
    glb_with_json(json, &triangle_binary())
}

fn triangle_glb_with_extension_materials(
    root_extension_fields: &str,
    materials: &str,
    selected_material: u32,
) -> Vec<u8> {
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},{root_extension_fields}"buffers":[{{"byteLength":36}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}}],"materials":{materials},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":{selected_material},"mode":4}}]}}]}}"#,
    );
    glb_with_json(&json, &triangle_binary())
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
    material_textured_triangle_glb(
        png,
        &format!(r#""pbrMetallicRoughness":{{{pbr_fields}}}"#),
        texture_items,
        image_items,
        root_fields,
        include_texcoords,
    )
}

fn material_textured_triangle_glb(
    png: &[u8],
    material_fields: &str,
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
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{{material_fields}}}],"textures":[{texture_items}],"images":[{image_items}]{root_fields},"meshes":[{{"primitives":[{{"attributes":{{{attributes}}},"material":0,"mode":4}}]}}]}}"#,
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

fn emissive_texture_glb(png: &[u8], emissive_texture: &str, include_texcoords: bool) -> Vec<u8> {
    material_textured_triangle_glb(
        png,
        &format!(
            r#""pbrMetallicRoughness":{{"baseColorFactor":[0.2,0.1,0.05,0.4]}},"emissiveFactor":[0.25,0.5,0.75],"emissiveTexture":{emissive_texture}"#
        ),
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        include_texcoords,
    )
}

#[derive(Clone, Copy)]
struct NormalTexturedFixture<'a> {
    positions: [[f32; 3]; 3],
    tangents: [[f32; 4]; 3],
    texcoords: [[f32; 2]; 3],
    tangent_count: u32,
    tangent_kind: &'a str,
    tangent_normalized: bool,
    include_normals: bool,
    include_tangents: bool,
    texcoord_attribute: &'a str,
    normal_texture_fields: &'a str,
    root_fields: &'a str,
}

impl Default for NormalTexturedFixture<'_> {
    fn default() -> Self {
        Self {
            positions: [[-0.75, -0.75, 0.0], [0.75, -0.75, 0.0], [0.0, 0.75, 0.0]],
            tangents: [[1.0, 0.0, 0.0, 1.0]; 3],
            texcoords: [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]],
            tangent_count: 3,
            tangent_kind: "VEC4",
            tangent_normalized: false,
            include_normals: true,
            include_tangents: true,
            texcoord_attribute: r#""TEXCOORD_0":3"#,
            normal_texture_fields: r#""index":0"#,
            root_fields: "",
        }
    }
}

fn normal_textured_triangle_glb(png: &[u8], fixture: NormalTexturedFixture<'_>) -> Vec<u8> {
    let NormalTexturedFixture {
        positions,
        tangents,
        texcoords,
        tangent_count,
        tangent_kind,
        tangent_normalized,
        include_normals,
        include_tangents,
        texcoord_attribute,
        normal_texture_fields,
        root_fields,
    } = fixture;
    let mut binary = Vec::with_capacity(144 + png.len());
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
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
    for texcoord in texcoords {
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
        r#"{{"asset":{{"version":"2.0"}}{root_fields},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{{"buffer":0,"byteOffset":{image_offset},"byteLength":{image_length}}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":{tangent_count},"type":"{tangent_kind}","normalized":{tangent_normalized}}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"normalTexture":{{{normal_texture_fields}}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":4,"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{{attributes}}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_length = png.len(),
    );
    glb_with_json(&json, &binary)
}

fn generated_normal_textured_glb(
    png: &[u8],
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    texcoords: &[[f32; 2]],
    indices: Option<&[u32]>,
) -> Vec<u8> {
    generated_normal_textured_glb_with_transform(
        png, positions, normals, texcoords, indices, "", "",
    )
}

fn generated_normal_textured_glb_with_transform(
    png: &[u8],
    positions: &[[f32; 3]],
    normals: &[[f32; 3]],
    texcoords: &[[f32; 2]],
    indices: Option<&[u32]>,
    root_fields: &str,
    normal_texture_extension: &str,
) -> Vec<u8> {
    assert_eq!(positions.len(), normals.len());
    assert_eq!(positions.len(), texcoords.len());
    let mut binary = Vec::new();
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let normal_offset = binary.len();
    for normal in normals {
        for value in normal {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let texcoord_offset = binary.len();
    for texcoord in texcoords {
        for value in texcoord {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    let index_offset = binary.len();
    if let Some(indices) = indices {
        for index in indices {
            binary.extend_from_slice(&index.to_le_bytes());
        }
    }
    let image_offset = binary.len();
    binary.extend_from_slice(png);

    let position_bytes = positions.len() * 12;
    let normal_bytes = normals.len() * 12;
    let texcoord_bytes = texcoords.len() * 8;
    let mut views = vec![
        format!(r#"{{"buffer":0,"byteOffset":0,"byteLength":{position_bytes}}}"#),
        format!(r#"{{"buffer":0,"byteOffset":{normal_offset},"byteLength":{normal_bytes}}}"#),
        format!(r#"{{"buffer":0,"byteOffset":{texcoord_offset},"byteLength":{texcoord_bytes}}}"#),
    ];
    let indices_field = if let Some(indices) = indices {
        let index_bytes = indices.len() * 4;
        views.push(format!(
            r#"{{"buffer":0,"byteOffset":{index_offset},"byteLength":{index_bytes}}}"#
        ));
        r#","indices":3"#
    } else {
        ""
    };
    let image_view = views.len();
    views.push(format!(
        r#"{{"buffer":0,"byteOffset":{image_offset},"byteLength":{}}}"#,
        png.len()
    ));
    let count = positions.len();
    let mut accessors = vec![
        format!(r#"{{"bufferView":0,"componentType":5126,"count":{count},"type":"VEC3"}}"#),
        format!(r#"{{"bufferView":1,"componentType":5126,"count":{count},"type":"VEC3"}}"#),
        format!(r#"{{"bufferView":2,"componentType":5126,"count":{count},"type":"VEC2"}}"#),
    ];
    if let Some(indices) = indices {
        let index_count = indices.len();
        accessors.push(format!(
            r#"{{"bufferView":3,"componentType":5125,"count":{index_count},"type":"SCALAR"}}"#
        ));
    }
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}}{root_fields},"buffers":[{{"byteLength":{}}}],"bufferViews":[{}],"accessors":[{}],"materials":[{{"normalTexture":{{"index":0{normal_texture_extension}}}}}],"textures":[{{"source":0}}],"images":[{{"bufferView":{image_view},"mimeType":"image/png"}}],"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TEXCOORD_0":2}}{indices_field},"material":0,"mode":4}}]}}]}}"#,
        binary.len(),
        views.join(","),
        accessors.join(","),
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

fn four_textured_triangle_glb(images: [&[u8]; 4], shared_image: bool) -> Vec<u8> {
    four_textured_triangle_glb_with_samplers(images, shared_image, [None; 4], "")
}

fn four_textured_triangle_glb_with_samplers(
    images: [&[u8]; 4],
    shared_image: bool,
    sampler_indices: [Option<u32>; 4],
    sampler_records: &str,
) -> Vec<u8> {
    four_textured_triangle_glb_with_options(
        images,
        shared_image,
        sampler_indices,
        sampler_records,
        [""; 4],
        "",
    )
}

fn four_textured_triangle_glb_with_options(
    images: [&[u8]; 4],
    shared_image: bool,
    sampler_indices: [Option<u32>; 4],
    sampler_records: &str,
    texture_info_extensions: [&str; 4],
    root_fields: &str,
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
    let selected = if shared_image { &images[..1] } else { &images };
    let mut image_views = Vec::with_capacity(selected.len());
    let mut image_defs = Vec::with_capacity(selected.len());
    for (index, image) in selected.iter().enumerate() {
        let offset = binary.len();
        binary.extend_from_slice(image);
        image_views.push(format!(
            r#"{{"buffer":0,"byteOffset":{offset},"byteLength":{}}}"#,
            image.len()
        ));
        image_defs.push(format!(
            r#"{{"bufferView":{},"mimeType":"image/png"}}"#,
            index + 4
        ));
    }
    let sources = if shared_image { [0; 4] } else { [0, 1, 2, 3] };
    let textures = sources
        .into_iter()
        .zip(sampler_indices)
        .map(|(source, sampler)| {
            sampler.map_or_else(
                || format!(r#"{{"source":{source}}}"#),
                |sampler| format!(r#"{{"sampler":{sampler},"source":{source}}}"#),
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let samplers = if sampler_records.is_empty() {
        String::new()
    } else {
        format!(r#", "samplers":[{sampler_records}]"#)
    };
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}}{root_fields},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}},{{"buffer":0,"byteOffset":36,"byteLength":36}},{{"buffer":0,"byteOffset":72,"byteLength":48}},{{"buffer":0,"byteOffset":120,"byteLength":24}},{image_views}],"accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":1,"componentType":5126,"count":3,"type":"VEC3"}},{{"bufferView":2,"componentType":5126,"count":3,"type":"VEC4"}},{{"bufferView":3,"componentType":5126,"count":3,"type":"VEC2"}}],"materials":[{{"pbrMetallicRoughness":{{"baseColorTexture":{{"index":0{base_extension}}},"metallicRoughnessTexture":{{"index":1{metallic_roughness_extension}}},"metallicFactor":0.75,"roughnessFactor":0.5}},"normalTexture":{{"index":2,"scale":0.5{normal_extension}}},"emissiveFactor":[0.25,0.5,0.75],"emissiveTexture":{{"index":3{emissive_extension}}}}}],"textures":[{textures}],"images":[{image_defs}]{samplers},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0,"NORMAL":1,"TANGENT":2,"TEXCOORD_0":3}},"material":0,"mode":4}}]}}]}}"#,
        binary_length = binary.len(),
        image_views = image_views.join(","),
        image_defs = image_defs.join(","),
        base_extension = texture_info_extensions[0],
        normal_extension = texture_info_extensions[1],
        metallic_roughness_extension = texture_info_extensions[2],
        emissive_extension = texture_info_extensions[3],
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

fn ready_material(bytes: Vec<u8>) -> cogniform_assets::AssetMaterial {
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Ready);
    store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap()
        .material()
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

fn indexed_glb_with_color_bytes(
    color_bytes: &[u8],
    color_count: u32,
    component_type: u32,
    kind: &str,
    normalized: bool,
    byte_stride: u32,
    color_attributes: &str,
) -> Vec<u8> {
    indexed_glb_with_color_spec(
        color_bytes,
        ColorGlbSpec {
            color_count,
            component_type,
            kind,
            normalized,
            byte_stride,
            color_attributes,
            primitive_mode: 4,
            canceling_offsets: false,
            include_position: true,
            primitive_count: 1,
        },
    )
}

#[derive(Clone, Copy)]
struct ColorGlbSpec<'a> {
    color_count: u32,
    component_type: u32,
    kind: &'a str,
    normalized: bool,
    byte_stride: u32,
    color_attributes: &'a str,
    primitive_mode: u32,
    canceling_offsets: bool,
    include_position: bool,
    primitive_count: usize,
}

fn indexed_glb_with_color_spec(color_bytes: &[u8], spec: ColorGlbSpec<'_>) -> Vec<u8> {
    let ColorGlbSpec {
        color_count,
        component_type,
        kind,
        normalized,
        byte_stride,
        color_attributes,
        primitive_mode,
        canceling_offsets,
        include_position,
        primitive_count,
    } = spec;
    let positions = [
        [-0.75_f32, -0.75, 0.0],
        [0.75, -0.75, 0.0],
        [0.0, 0.75, 0.0],
        [0.0, 0.0, 1.0],
    ];
    let mut binary = Vec::with_capacity(48 + color_bytes.len() + 6);
    for position in positions {
        for value in position {
            binary.extend_from_slice(&value.to_le_bytes());
        }
    }
    binary.extend_from_slice(color_bytes);
    let index_offset = binary.len();
    for index in [2_u16, 0, 1] {
        binary.extend_from_slice(&index.to_le_bytes());
    }
    let (color_view_offset, color_accessor_offset, color_view_length) = if canceling_offsets {
        (46, 2, color_bytes.len() + 2)
    } else {
        (48, 0, color_bytes.len())
    };
    let attributes = if include_position {
        format!(r#""POSITION":0,{color_attributes}"#)
    } else {
        color_attributes.to_owned()
    };
    let primitive =
        format!(r#"{{"attributes":{{{attributes}}},"indices":2,"mode":{primitive_mode}}}"#);
    let primitives = vec![primitive; primitive_count].join(",");
    let json = format!(
        r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":{binary_length}}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":48}},{{"buffer":0,"byteOffset":{color_view_offset},"byteLength":{color_view_length},"byteStride":{byte_stride}}},{{"buffer":0,"byteOffset":{index_offset},"byteLength":6}}],"accessors":[{{"bufferView":0,"componentType":5126,"count":4,"type":"VEC3"}},{{"bufferView":1,"byteOffset":{color_accessor_offset},"componentType":{component_type},"count":{color_count},"type":"{kind}","normalized":{normalized}}},{{"bufferView":2,"componentType":5123,"count":3,"type":"SCALAR"}}],"meshes":[{{"primitives":[{primitives}]}}]}}"#,
        binary_length = binary.len(),
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
    assert_eq!(store.stats().resident_cpu_bytes, 192);

    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .expect("decoded fixture should produce an upload job");
    assert_eq!(upload.vertices().len(), 3);
    assert_eq!(upload.byte_len(), 192);
    for vertex in upload.vertices() {
        assert_normal(vertex.normal, [0.0, 0.0, 1.0]);
        assert_texcoord(vertex.texcoord_0, [0.0, 0.0]);
        assert_color(vertex.color_0, [1.0; 4]);
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
    assert_emissive(explicit_upload.material().emissive(), [0.0; 3]);
    assert_eq!(
        explicit_upload.material().shading_model(),
        AssetShadingModel::MetallicRoughness
    );
    assert!(!explicit_upload.material().double_sided());

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
    assert_emissive(unmaterialed_upload.material().emissive(), [0.0; 3]);
    assert_eq!(
        unmaterialed_upload.material().shading_model(),
        AssetShadingModel::MetallicRoughness
    );
    assert!(!unmaterialed_upload.material().double_sided());
    assert_eq!(explicit_upload.byte_len(), unmaterialed_upload.byte_len());
}

#[test]
fn double_sided_defaults_and_explicit_values_are_retained_without_accounting_growth() {
    let cases = [
        ("", false),
        (r#""doubleSided":false"#, false),
        (r#""doubleSided":true"#, true),
    ];
    let mut decoded_bytes = Vec::new();
    let mut upload_bytes = Vec::new();
    for (fields, expected) in cases {
        let bytes = triangle_glb_with_material_fields(fields);
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        decoded_bytes.push(store.record(hash).unwrap().decoded_bytes);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert_eq!(upload.material().double_sided(), expected);
        upload_bytes.push(upload.byte_len());
    }
    assert!(decoded_bytes.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(upload_bytes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn unlit_declarations_and_selected_or_unused_markers_are_typed_and_bounded() {
    let cases = [
        ("", r"[{}]", 0, AssetShadingModel::MetallicRoughness),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#"[{"extensions":{"KHR_materials_unlit":{}}}]"#,
            0,
            AssetShadingModel::Unlit,
        ),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"extensionsRequired":["KHR_materials_unlit"],"#,
            r#"[{}, {"extensions":{"KHR_materials_unlit":{}}}]"#,
            1,
            AssetShadingModel::Unlit,
        ),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#"[{}, {"extensions":{"KHR_materials_unlit":{}}}]"#,
            0,
            AssetShadingModel::MetallicRoughness,
        ),
    ];
    let mut decoded_bytes = Vec::new();
    let mut upload_bytes = Vec::new();
    for (root_fields, materials, selected, expected) in cases {
        let bytes = triangle_glb_with_extension_materials(root_fields, materials, selected);
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        decoded_bytes.push(store.record(hash).unwrap().decoded_bytes);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert_eq!(upload.material().shading_model(), expected);
        upload_bytes.push(upload.byte_len());
    }
    assert!(decoded_bytes.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(upload_bytes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn malformed_unlit_declarations_and_markers_never_receive_a_proxy() {
    let cases = [
        (r#""extensionsUsed":[],"#, r"[{}]"),
        (
            r#""extensionsUsed":["KHR_materials_unlit","KHR_materials_unlit"],"#,
            r"[{}]",
        ),
        (r#""extensionsUsed":[""],"#, r"[{}]"),
        (r#""extensionsUsed":[1],"#, r"[{}]"),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"extensionsRequired":[],"#,
            r"[{}]",
        ),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"extensionsRequired":["KHR_materials_unlit","KHR_materials_unlit"],"#,
            r"[{}]",
        ),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"extensionsRequired":[1],"#,
            r"[{}]",
        ),
        (r#""extensionsRequired":["KHR_materials_unlit"],"#, r"[{}]"),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"extensionsRequired":["EXT_other"],"#,
            r"[{}]",
        ),
        ("", r#"[{"extensions":{"KHR_materials_unlit":{}}}]"#),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#"[{"extensions":{"KHR_materials_unlit":null}}]"#,
        ),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#"[{"extensions":{"KHR_materials_unlit":[]}}]"#,
        ),
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#"[{}, {"extensions":{"KHR_materials_unlit":1}}]"#,
        ),
    ];
    for (root_fields, materials) in cases {
        let bytes = triangle_glb_with_extension_materials(root_fields, materials, 0);
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }
}

#[test]
fn wider_extensions_proxy_only_after_malformed_material_peers_are_excluded() {
    for (root_fields, materials) in [
        (
            r#""extensionsUsed":["KHR_materials_unlit"],"#,
            r#"[{"extensions":{"KHR_materials_unlit":{"future":true}}}]"#,
        ),
        (r#""extensionsUsed":["EXT_other"],"#, r"[{}]"),
    ] {
        let bytes = triangle_glb_with_extension_materials(root_fields, materials, 0);
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::UnsupportedExtension
        );
    }

    let bytes = triangle_glb_with_extension_materials(
        r#""extensionsUsed":["KHR_materials_unlit","EXT_other"],"#,
        r#"[{"extensions":{"KHR_materials_unlit":{}},"doubleSided":null}, {"extensions":{"EXT_other":{}}}]"#,
        0,
    );
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );

    for payload in ["null", "true", "1", r#""value""#, "[]"] {
        let materials = format!(r#"[{{"extensions":{{"EXT_other":{payload}}}}}]"#);
        let bytes = triangle_glb_with_extension_materials(
            r#""extensionsUsed":["EXT_other"],"#,
            &materials,
            0,
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }
    let bytes =
        triangle_glb_with_extension_materials("", r#"[{"extensions":{"EXT_other":{}}}]"#, 0);
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );
    let bytes = triangle_glb_with_extension_materials(
        r#""extensionsUsed":["KHR_materials_unlit","EXT_other"],"#,
        r#"[{"extensions":{"KHR_materials_unlit":{"future":true},"EXT_other":null}}]"#,
        0,
    );
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );

    for unused_material in [
        r#"{"pbrMetallicRoughness":{"baseColorFactor":[1.1,0.0,0.0,1.0]}}"#,
        r#"{"pbrMetallicRoughness":{"metallicFactor":1.1}}"#,
        r#"{"pbrMetallicRoughness":{"roughnessFactor":-0.1}}"#,
    ] {
        let materials = format!(r"[{{}}, {unused_material}]");
        let bytes = triangle_glb_with_extension_materials(
            r#""extensionsUsed":["EXT_other"],"#,
            &materials,
            0,
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }
}

#[test]
fn malformed_double_sided_is_rejected_without_proxy_substitution() {
    for fields in [
        r#""doubleSided":null"#,
        r#""doubleSided":1"#,
        r#""doubleSided":"true""#,
        r#""doubleSided":{}"#,
        r#""doubleSided":[]"#,
        r#""occlusionTexture":{},"doubleSided":null"#,
        r#""alphaMode":"BLEND","doubleSided":1"#,
    ] {
        let bytes = triangle_glb_with_material_fields(fields);
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }

    for materials in [
        r#"[{"doubleSided":null},{}]"#,
        r#"[{"alphaMode":"BLEND"},{"doubleSided":1}]"#,
        r#"[{"doubleSided":"false"},{"occlusionTexture":{}}]"#,
    ] {
        let json = format!(
            r#"{{"asset":{{"version":"2.0"}},"buffers":[{{"byteLength":36}}],"bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],"accessors":[{{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}}],"materials":{materials},"meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}},"material":1,"mode":4}}]}}]}}"#,
        );
        let (store, hash) = process_with_proxy_policy(glb_with_json(&json, &triangle_binary()));
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }
}

#[test]
fn alpha_coverage_defaults_and_mask_cutoffs_are_retained_without_accounting_growth() {
    let cases = [
        ("", AssetAlphaMode::Opaque, None),
        (r#""alphaMode":"OPAQUE""#, AssetAlphaMode::Opaque, None),
        (
            r#""alphaMode":"OPAQUE","alphaCutoff":0.75"#,
            AssetAlphaMode::Opaque,
            None,
        ),
        (r#""alphaMode":"MASK""#, AssetAlphaMode::Mask, Some(0.5)),
        (
            r#""alphaMode":"MASK","alphaCutoff":1.25"#,
            AssetAlphaMode::Mask,
            Some(1.25),
        ),
    ];
    let mut decoded_bytes = Vec::new();
    let mut upload_bytes = Vec::new();
    for (fields, expected_mode, expected_cutoff) in cases {
        let bytes = triangle_glb_with_material_fields(fields);
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        decoded_bytes.push(store.record(hash).unwrap().decoded_bytes);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert_eq!(upload.material().alpha_mode(), expected_mode);
        assert_eq!(
            upload.material().alpha_cutoff().map(f32::to_bits),
            expected_cutoff.map(f32::to_bits)
        );
        upload_bytes.push(upload.byte_len());
    }
    assert!(decoded_bytes.windows(2).all(|pair| pair[0] == pair[1]));
    assert!(upload_bytes.windows(2).all(|pair| pair[0] == pair[1]));
}

#[test]
fn malformed_alpha_coverage_is_rejected_without_proxy_substitution() {
    for fields in [
        r#""alphaMode":null"#,
        r#""alphaMode":1"#,
        r#""alphaMode":"MASK","alphaCutoff":null"#,
        r#""alphaMode":"MASK","alphaCutoff":"0.5""#,
        r#""alphaMode":"MASK","alphaCutoff":-0.1"#,
        r#""alphaMode":"MASK","alphaCutoff":1e999"#,
        r#""alphaCutoff":0.5"#,
    ] {
        let bytes = triangle_glb_with_material_fields(fields);
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }

    let unused_invalid = glb_with_json(
        r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"materials":[{"alphaMode":"MASK","alphaCutoff":-1.0},{}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":1,"mode":4}]}]}"#,
        &triangle_binary(),
    );
    let (store, hash) = process_with_proxy_policy(unused_invalid);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(store.record(hash).unwrap().diagnostics[0].index, Some(0));
}

#[test]
fn wider_alpha_modes_proxy_only_after_malformed_peer_data_is_excluded() {
    for mode in ["BLEND", "FUTURE_MODE"] {
        let bytes = triangle_glb_with_material_fields(&format!(r#""alphaMode":"{mode}""#));
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::UnsupportedFeature
        );
    }

    let malformed_peer = glb_with_json(
        r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"materials":[{"alphaMode":"BLEND"}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":999,"mode":4}]}]}"#,
        &triangle_binary(),
    );
    let (store, hash) = process_with_proxy_policy(malformed_peer);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );
}

#[test]
fn emissive_factors_are_bounded_and_do_not_change_asset_accounting() {
    let emissive = triangle_glb_with_material_fields(
        r#""pbrMetallicRoughness":{"baseColorFactor":[0.3,0.4,0.5,0.6]},"emissiveFactor":[0.25,0.5,0.75]"#,
    );
    let baseline = triangle_glb_with_material_fields(
        r#""pbrMetallicRoughness":{"baseColorFactor":[0.3,0.4,0.5,0.6]}"#,
    );
    let mut decoded_bytes = Vec::new();
    let mut upload_bytes = Vec::new();
    for (bytes, expected) in [(emissive, [0.25, 0.5, 0.75]), (baseline, [0.0; 3])] {
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        decoded_bytes.push(store.record(hash).unwrap().decoded_bytes);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        assert_emissive(upload.material().emissive(), expected);
        upload_bytes.push(upload.byte_len());
    }
    assert_eq!(decoded_bytes[0], decoded_bytes[1]);
    assert_eq!(upload_bytes[0], upload_bytes[1]);
}

#[test]
fn emissive_texture_is_typed_bounded_and_retained_with_exact_accounting() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 64, 32, 7],
    );
    let bytes = emissive_texture_glb(&png, r#"{"index":0,"texCoord":0}"#, true);
    let hash = content_hash(&bytes);
    let mut config = AssetStoreConfig::default();
    config.limits.max_asset_decoded_bytes = NonZeroU64::new(196).unwrap();
    config.limits.max_resident_cpu_bytes = NonZeroU64::new(196).unwrap();
    let mut store = AssetStore::new(config);
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 196);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    assert!(upload.material().has_emissive_texture());
    assert!(!upload.material().has_base_color_texture());
    assert!(!upload.material().has_metallic_roughness_texture());
    assert!(!upload.material().has_normal_texture());
    assert_eq!(upload.emissive_texture().unwrap().rgba8(), [128, 64, 32, 7]);
    assert_eq!(upload.byte_len(), 3 * ASSET_VERTEX_BYTES);
    let eviction = store.evict(hash);
    assert_eq!(eviction.removed_textures, 1);
    assert_eq!(eviction.released_resident_cpu_bytes, 196);
}

#[test]
fn malformed_emissive_texture_roles_reject_without_proxy() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let cases = [
        (
            emissive_texture_glb(&png, "null", true),
            AssetDiagnosticCode::InvalidJson,
        ),
        (
            emissive_texture_glb(&png, r#"{"index":"zero"}"#, true),
            AssetDiagnosticCode::InvalidJson,
        ),
        (
            emissive_texture_glb(&png, r#"{"index":0,"texCoord":"zero"}"#, true),
            AssetDiagnosticCode::InvalidJson,
        ),
        (
            emissive_texture_glb(&png, r#"{"index":999}"#, true),
            AssetDiagnosticCode::InvalidBufferRange,
        ),
        (
            emissive_texture_glb(&png, r#"{"index":0}"#, false),
            AssetDiagnosticCode::InvalidTexcoord,
        ),
    ];
    for (bytes, expected) in cases {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(store.record(hash).unwrap().diagnostics[0].code, expected);
    }
}

#[test]
fn valid_but_unsupported_emissive_texture_shapes_obey_proxy_policy() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let cases = [
        emissive_texture_glb(&png, r#"{"index":0,"texCoord":1}"#, true),
        material_textured_triangle_glb(
            &png,
            r#""pbrMetallicRoughness":{},"emissiveTexture":{"index":0}"#,
            r#"{"source":0},{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        ),
        material_textured_triangle_glb(
            &png,
            r#""pbrMetallicRoughness":{},"emissiveTexture":{"index":0}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"}"#,
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
                .has_emissive_texture()
        );
    }
}

#[test]
fn malformed_unused_or_mixed_emissive_resources_never_proxy() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let material = r#""pbrMetallicRoughness":{},"emissiveTexture":{"index":0}"#;
    let cases = [
        (
            material_textured_triangle_glb(
                &png,
                material,
                r#"{"source":0},{}"#,
                r#"{"bufferView":2,"mimeType":"image/png"}"#,
                "",
                true,
            ),
            AssetDiagnosticCode::InvalidJson,
        ),
        (
            material_textured_triangle_glb(
                &png,
                material,
                r#"{"source":0},{"source":999}"#,
                r#"{"bufferView":2,"mimeType":"image/png"}"#,
                "",
                true,
            ),
            AssetDiagnosticCode::InvalidBufferRange,
        ),
        (
            material_textured_triangle_glb(
                &png,
                material,
                r#"{"source":0}"#,
                r#"{"bufferView":2,"mimeType":"image/png"},{"bufferView":999,"mimeType":"image/png"}"#,
                "",
                true,
            ),
            AssetDiagnosticCode::InvalidBufferRange,
        ),
        (
            material_textured_triangle_glb(
                &png,
                material,
                r#"{"source":0}"#,
                r#"{"bufferView":2,"mimeType":"image/png"},{"bufferView":0,"mimeType":"image/png"}"#,
                "",
                true,
            ),
            AssetDiagnosticCode::InvalidImage,
        ),
        (
            emissive_texture_glb(b"not a png", r#"{"index":0,"texCoord":1}"#, true),
            AssetDiagnosticCode::InvalidImage,
        ),
    ];
    for (bytes, expected) in cases {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(store.record(hash).unwrap().diagnostics[0].code, expected);
    }
}

#[test]
fn unsupported_resources_do_not_mask_malformed_emissive_roles() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let cases = [
        (
            material_textured_triangle_glb(
                &png,
                r#""pbrMetallicRoughness":{},"emissiveTexture":{"index":999}"#,
                r#"{"source":0}"#,
                r#"{"uri":"texture.png"}"#,
                "",
                true,
            ),
            AssetDiagnosticCode::InvalidBufferRange,
        ),
        (
            material_textured_triangle_glb(
                &png,
                r#""pbrMetallicRoughness":{},"emissiveTexture":{"index":0}"#,
                r#"{"source":0}"#,
                r#"{"uri":"texture.png"}"#,
                "",
                false,
            ),
            AssetDiagnosticCode::InvalidTexcoord,
        ),
        (
            material_textured_triangle_glb(
                &png,
                r#""pbrMetallicRoughness":{},"emissiveTexture":{"index":999}"#,
                r#"{"source":0}"#,
                r#"{"bufferView":2,"mimeType":"image/png"}"#,
                r#","samplers":[{}]"#,
                true,
            ),
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
    assert_eq!(rgba_store.record(rgba_hash).unwrap().decoded_bytes, 208);
    assert_eq!(rgba_store.stats().resident_cpu_bytes, 208);
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
    assert_eq!(rgba_eviction.released_resident_cpu_bytes, 208);
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
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 196);
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
    assert_eq!(eviction.released_resident_cpu_bytes, 196);
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
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 196);
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
    for (shared_image, expected_bytes) in [(true, 196), (false, 200)] {
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
    for (shared_image, expected_bytes) in [(true, 196), (false, 204)] {
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
fn four_texture_roles_count_shared_cpu_images_once_and_roles_exactly() {
    let images = [
        encode_png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[255, 0, 0, 255],
        ),
        encode_png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[7, 64, 192, 9],
        ),
        encode_png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[128, 128, 255, 255],
        ),
        encode_png(
            1,
            1,
            png::ColorType::Rgba,
            png::BitDepth::Eight,
            &[32, 64, 128, 3],
        ),
    ];
    for (shared_image, expected_bytes) in [(true, 196), (false, 208)] {
        let bytes = four_textured_triangle_glb(images.each_ref().map(Vec::as_slice), shared_image);
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
        assert!(upload.emissive_texture().is_some());
        assert!(upload.metallic_roughness_texture().is_some());
        assert!(upload.normal_texture().is_some());
        if shared_image {
            assert_eq!(
                upload.base_color_texture().unwrap().rgba8(),
                upload.emissive_texture().unwrap().rgba8()
            );
        } else {
            assert_eq!(upload.emissive_texture().unwrap().rgba8(), [32, 64, 128, 3]);
        }
        let eviction = store.evict(hash);
        assert_eq!(eviction.removed_textures, 4);
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
fn texture_transforms_retain_exact_defaults_and_independent_affine_rows() {
    let image = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let images = [image.as_slice(); 4];
    let bytes = four_textured_triangle_glb_with_options(
        images,
        true,
        [None; 4],
        "",
        [
            r#", "extensions":{"KHR_texture_transform":{}}"#,
            r#", "extensions":{"KHR_texture_transform":{"offset":[0.25,-0.5],"rotation":1.5707963267948966,"scale":[2.0,3.0]}}"#,
            r#", "extensions":{"KHR_texture_transform":{"scale":[2.0,-3.0]}}"#,
            r#", "extensions":{"KHR_texture_transform":{"offset":[-0.25,0.75]}}"#,
        ],
        r#", "extensionsUsed":["KHR_texture_transform"],"extensionsRequired":["KHR_texture_transform"]"#,
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 196);
    let material = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap()
        .material();

    assert_eq!(
        material.base_color_texture_transform(),
        Some(AssetTextureTransform::IDENTITY)
    );
    assert_eq!(
        material
            .metallic_roughness_texture_transform()
            .unwrap()
            .affine_rows(),
        [[2.0, 0.0, 0.0, 0.0], [0.0, -3.0, 0.0, 0.0]]
    );
    assert_eq!(
        material.emissive_texture_transform().unwrap().affine_rows(),
        [[1.0, 0.0, -0.25, 0.0], [0.0, 1.0, 0.75, 0.0]]
    );
    let normal = material.normal_texture_transform().unwrap().affine_rows();
    for (actual, expected) in normal
        .into_iter()
        .flatten()
        .zip([0.0, -3.0, 0.25, 0.0, 2.0, 0.0, -0.5, 0.0])
    {
        assert!((actual - expected).abs() <= 1e-6, "{actual} != {expected}");
    }
}

#[test]
fn texture_transform_malformed_paths_fail_closed() {
    let image = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let images = [image.as_slice(); 4];
    let declared = r#", "extensionsUsed":["KHR_texture_transform"]"#;
    for payload in [
        "null",
        "[]",
        r#"{"offset":[0.0]}"#,
        r#"{"offset":[0.0,1.0,2.0]}"#,
        r#"{"offset":[0.0,"one"]}"#,
        r#"{"rotation":"zero"}"#,
        r#"{"scale":[1.0]}"#,
        r#"{"texCoord":null}"#,
        r#"{"texCoord":-1}"#,
    ] {
        let extension = format!(r#", "extensions":{{"KHR_texture_transform":{payload}}}"#);
        let bytes = four_textured_triangle_glb_with_options(
            images,
            true,
            [None; 4],
            "",
            [extension.as_str(), "", "", ""],
            declared,
        );
        let (store, hash) = process_with_proxy_policy(bytes);
        let record = store.record(hash).unwrap();
        assert_eq!(record.state, AssetState::Rejected, "{payload}");
        assert_eq!(
            record.diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson,
            "{payload}"
        );
    }

    let undeclared = r#", "extensions":{"KHR_texture_transform":{}}"#;
    let (store, hash) = process_with_proxy_policy(four_textured_triangle_glb_with_options(
        images,
        true,
        [None; 4],
        "",
        [undeclared, "", "", ""],
        "",
    ));
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );
}

#[test]
fn texture_transform_wider_and_overflow_paths_fail_closed() {
    let image = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let images = [image.as_slice(); 4];
    let declared = r#", "extensionsUsed":["KHR_texture_transform"]"#;

    for payload in [r#"{"texCoord":1}"#, r#"{"future":true}"#] {
        let extension = format!(r#", "extensions":{{"KHR_texture_transform":{payload}}}"#);
        let (store, hash) = process_with_proxy_policy(four_textured_triangle_glb_with_options(
            images,
            true,
            [None; 4],
            "",
            [extension.as_str(), "", "", ""],
            declared,
        ));
        let record = store.record(hash).unwrap();
        assert_eq!(record.state, AssetState::ProxyReady, "{payload}");
        assert_eq!(
            record.diagnostics[0].code,
            AssetDiagnosticCode::UnsupportedExtension,
            "{payload}"
        );
    }

    let wider = r#", "extensions":{"KHR_texture_transform":{"future":true}}"#;
    let (store, hash) = process_with_proxy_policy(four_textured_triangle_glb_with_options(
        images,
        true,
        [Some(999), None, None, None],
        r"{}",
        [wider, "", "", ""],
        declared,
    ));
    let record = store.record(hash).unwrap();
    assert_eq!(record.state, AssetState::Rejected);
    assert_eq!(
        record.diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );

    let overflow = r#", "extensions":{"KHR_texture_transform":{"offset":[3.4028235e38,0.0],"scale":[3.4028235e38,1.0]}}"#;
    let (store, hash) = process_with_proxy_policy(four_textured_triangle_glb_with_options(
        images,
        true,
        [None; 4],
        "",
        [overflow, "", "", ""],
        declared,
    ));
    let record = store.record(hash).unwrap();
    assert_eq!(record.state, AssetState::Rejected);
    assert_eq!(
        record.diagnostics[0].code,
        AssetDiagnosticCode::InvalidTexcoord
    );
    assert_eq!(
        record.diagnostics[0].location,
        "glb.decoded.texture_transform"
    );
}

#[test]
fn generated_tangents_use_transformed_normal_coordinates_without_mutating_uvs() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[255, 128, 128, 255],
    );
    let positions = [[-0.75, -0.75, 0.0], [0.75, -0.75, 0.0], [0.0, 0.75, 0.0]];
    let normals = [[0.0, 0.0, 1.0]; 3];
    let texcoords = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]];
    let load = |extension: &str, root_fields: &str| {
        let bytes = generated_normal_textured_glb_with_transform(
            &png,
            &positions,
            &normals,
            &texcoords,
            None,
            root_fields,
            extension,
        );
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap()
            .vertices()
            .to_vec()
    };
    let identity = load("", "");
    let root = r#", "extensionsUsed":["KHR_texture_transform"],"extensionsRequired":["KHR_texture_transform"]"#;
    let rotated = load(
        r#", "extensions":{"KHR_texture_transform":{"rotation":1.5707963267948966}}"#,
        root,
    );
    let mirrored = load(
        r#", "extensions":{"KHR_texture_transform":{"scale":[-1.0,1.0]}}"#,
        root,
    );

    for ((source, rotated), mirrored) in identity.iter().zip(&rotated).zip(&mirrored) {
        assert_eq!(source.texcoord_0, rotated.texcoord_0);
        assert_eq!(source.texcoord_0, mirrored.texcoord_0);
        assert!((source.tangent[0].get() - 1.0).abs() <= 1e-6);
        assert!((rotated.tangent[0].get() + 0.447_213_6).abs() <= 1e-5);
        assert!((rotated.tangent[1].get() + 0.894_427_2).abs() <= 1e-5);
        assert_eq!(source.tangent[3].get().to_bits(), 1.0_f32.to_bits());
        assert_eq!(rotated.tangent[3].get().to_bits(), 1.0_f32.to_bits());
        assert!((mirrored.tangent[0].get() + 1.0).abs() <= 1e-6);
        assert_eq!(mirrored.tangent[3].get().to_bits(), (-1.0_f32).to_bits());
    }

    let explicit_source = [[0.0, 1.0, 0.0, -1.0]; 3];
    let explicit = normal_textured_triangle_glb(
        &png,
        NormalTexturedFixture {
            tangents: explicit_source,
            normal_texture_fields: r#""index":0,"extensions":{"KHR_texture_transform":{"rotation":1.5707963267948966}}"#,
            root_fields: root,
            ..NormalTexturedFixture::default()
        },
    );
    let hash = content_hash(&explicit);
    let mut store = AssetStore::default();
    store.enqueue(hash, explicit).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    let upload = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap();
    for (vertex, expected) in upload.vertices().iter().zip(explicit_source) {
        assert_eq!(
            vertex.tangent.map(|value| value.get().to_bits()),
            expected.map(f32::to_bits)
        );
    }
}

#[test]
fn four_texture_roles_retain_independent_samplers_without_accounting_growth() {
    let image = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let images = [image.as_slice(); 4];
    let bytes = four_textured_triangle_glb_with_samplers(
        images,
        true,
        [Some(0), Some(1), Some(2), Some(3)],
        r#"{"magFilter":9728,"minFilter":9728,"wrapS":10497,"wrapT":10497},{"magFilter":9729,"minFilter":9729,"wrapS":33071,"wrapT":10497},{"magFilter":9728,"minFilter":9986,"wrapS":33648,"wrapT":33071},{"magFilter":9729,"minFilter":9987,"wrapS":10497,"wrapT":33648}"#,
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(store.record(hash).unwrap().decoded_bytes, 196);
    assert_eq!(store.stats().resident_cpu_bytes, 196);

    let material = store
        .upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0,
        })
        .unwrap()
        .material();
    let sampler_key = |sampler: AssetSampler| {
        (
            sampler.mag_filter(),
            sampler.min_filter(),
            sampler.effective_min_filter(),
            sampler.wrap_s(),
            sampler.wrap_t(),
        )
    };
    assert_eq!(
        sampler_key(material.base_color_sampler().unwrap()),
        (
            AssetSamplerFilter::Nearest,
            AssetSamplerMinFilter::Nearest,
            AssetSamplerFilter::Nearest,
            AssetSamplerWrap::Repeat,
            AssetSamplerWrap::Repeat,
        )
    );
    assert_eq!(
        sampler_key(material.metallic_roughness_sampler().unwrap()),
        (
            AssetSamplerFilter::Linear,
            AssetSamplerMinFilter::Linear,
            AssetSamplerFilter::Linear,
            AssetSamplerWrap::ClampToEdge,
            AssetSamplerWrap::Repeat,
        )
    );
    assert_eq!(
        sampler_key(material.normal_sampler().unwrap()),
        (
            AssetSamplerFilter::Nearest,
            AssetSamplerMinFilter::NearestMipmapLinear,
            AssetSamplerFilter::Nearest,
            AssetSamplerWrap::MirroredRepeat,
            AssetSamplerWrap::ClampToEdge,
        )
    );
    assert_eq!(
        sampler_key(material.emissive_sampler().unwrap()),
        (
            AssetSamplerFilter::Linear,
            AssetSamplerMinFilter::LinearMipmapLinear,
            AssetSamplerFilter::Linear,
            AssetSamplerWrap::Repeat,
            AssetSamplerWrap::MirroredRepeat,
        )
    );

    let eviction = store.evict(hash);
    assert_eq!(eviction.removed_textures, 4);
    assert_eq!(eviction.released_resident_cpu_bytes, 196);

    assert_shared_sampler_accounting(images);
}

fn assert_shared_sampler_accounting(images: [&[u8]; 4]) {
    let shared_bytes = four_textured_triangle_glb_with_samplers(images, true, [Some(0); 4], r"{}");
    let shared_hash = content_hash(&shared_bytes);
    let mut shared_store = AssetStore::default();
    shared_store.enqueue(shared_hash, shared_bytes).unwrap();
    assert_eq!(
        shared_store.process_next().unwrap().state,
        AssetState::Ready
    );
    assert_eq!(shared_store.record(shared_hash).unwrap().decoded_bytes, 196);
    assert_eq!(shared_store.stats().resident_cpu_bytes, 196);
    let shared_material = shared_store
        .upload_job(AssetMeshKey {
            content_hash: shared_hash,
            mesh_index: 0,
        })
        .unwrap()
        .material();
    assert_eq!(
        [
            shared_material.base_color_sampler(),
            shared_material.metallic_roughness_sampler(),
            shared_material.normal_sampler(),
            shared_material.emissive_sampler(),
        ],
        [Some(AssetSampler::LINEAR_REPEAT); 4]
    );
    let shared_eviction = shared_store.evict(shared_hash);
    assert_eq!(shared_eviction.removed_textures, 4);
    assert_eq!(shared_eviction.released_resident_cpu_bytes, 196);
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
        for include_normals in [true, false] {
            let bytes = normal_textured_triangle_glb(
                &png,
                NormalTexturedFixture {
                    tangents,
                    include_normals,
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
}

#[test]
fn missing_normal_or_tangent_generates_default_mikktspace_values() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    for fixture in [
        NormalTexturedFixture {
            include_tangents: false,
            ..NormalTexturedFixture::default()
        },
        NormalTexturedFixture {
            tangents: [[0.0, 1.0, 0.0, -1.0]; 3],
            include_normals: false,
            ..NormalTexturedFixture::default()
        },
        NormalTexturedFixture {
            include_normals: false,
            include_tangents: false,
            ..NormalTexturedFixture::default()
        },
    ] {
        let bytes = normal_textured_triangle_glb(&png, fixture);
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Ready);
        let upload = store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap();
        for vertex in upload.vertices() {
            assert_eq!(
                vertex.tangent.map(|value| value.get().to_bits()),
                [1.0_f32, 0.0, 0.0, 1.0].map(f32::to_bits)
            );
        }
    }
}

#[test]
fn indexed_missing_tangents_expand_and_generate_per_corner() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let positions = [
        [0.0, 0.0, 0.0],
        [1.0, 0.0, 0.0],
        [0.0, 1.0, 0.0],
        [9.0, 9.0, 9.0],
    ];
    let normals = [[0.0, 0.0, 1.0]; 4];
    let texcoords = [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0], [0.5, 0.5]];
    let bytes =
        generated_normal_textured_glb(&png, &positions, &normals, &texcoords, Some(&[0, 1, 2]));
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
    let expected = [1.0_f32, 0.0, 0.0, 1.0].map(f32::to_bits);
    assert_eq!(upload.vertices().len(), 3);
    assert!(
        upload
            .vertices()
            .iter()
            .all(|vertex| { vertex.tangent.map(|value| value.get().to_bits()) == expected })
    );
}

#[test]
fn generated_tangent_work_guard_accepts_447_overlaps_and_rejects_448() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    for (faces, expected_state) in [(447, AssetState::Ready), (448, AssetState::Rejected)] {
        let mut positions = Vec::with_capacity(faces * 3);
        let mut normals = Vec::with_capacity(faces * 3);
        let mut texcoords = Vec::with_capacity(faces * 3);
        for _ in 0..faces {
            positions.extend([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
            normals.extend([[0.0, 0.0, 1.0]; 3]);
            texcoords.extend([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
        }
        let bytes = generated_normal_textured_glb(&png, &positions, &normals, &texcoords, None);
        let hash = content_hash(&bytes);
        let mut store = AssetStore::default();
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, expected_state);
        if expected_state == AssetState::Rejected {
            let diagnostic = &store.record(hash).unwrap().diagnostics[0];
            assert_eq!(
                diagnostic.code,
                AssetDiagnosticCode::CollectionLimitExceeded
            );
            assert_eq!(diagnostic.location, "glb.decoded.generated_tangent_work");
            assert_eq!(diagnostic.index, Some(0));
        }
    }
}

#[test]
#[allow(clippy::cast_precision_loss)]
fn unique_weld_keys_do_not_bypass_the_degenerate_search_guard() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let mut positions = Vec::with_capacity((1_365 + 1_366) * 3);
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut texcoords = Vec::with_capacity(positions.capacity());
    for face in 0..1_365 {
        let coordinate = (face * 2) as f32;
        for corner in 0..3 {
            positions.push([coordinate, 0.0, 0.0]);
            normals.push([0.0, 0.0, 1.0]);
            let key = (face * 3 + corner) as f32;
            texcoords.push([key, key]);
        }
    }
    for face in 0..1_366 {
        let coordinate = ((1_365 + face) * 2) as f32;
        positions.extend([
            [coordinate, 0.0, 0.0],
            [coordinate + 1.0, 0.0, 0.0],
            [coordinate, 1.0, 0.0],
        ]);
        normals.extend([[0.0, 0.0, 1.0]; 3]);
        let key = ((1_365 + face) * 3) as f32;
        texcoords.extend([[key, 0.0], [key + 1.0, 0.0], [key, 1.0]]);
    }
    let bytes = generated_normal_textured_glb(&png, &positions, &normals, &texcoords, None);
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    let diagnostic = &store.record(hash).unwrap().diagnostics[0];
    assert_eq!(
        diagnostic.code,
        AssetDiagnosticCode::CollectionLimitExceeded
    );
    assert_eq!(diagnostic.location, "glb.decoded.generated_tangent_work");
    assert_eq!(diagnostic.index, Some(0));
}

#[test]
fn isolated_degenerate_generated_tangent_rejects_without_partial_admission() {
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let bytes = normal_textured_triangle_glb(
        &png,
        NormalTexturedFixture {
            positions: [[0.0, 0.0, 0.0]; 3],
            include_tangents: false,
            ..NormalTexturedFixture::default()
        },
    );
    let hash = content_hash(&bytes);
    let mut store = AssetStore::default();
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    assert!(matches!(
        store.upload_job(AssetMeshKey {
            content_hash: hash,
            mesh_index: 0
        }),
        Err(AssetError::AssetNotReady { .. })
    ));
    let diagnostic = &store.record(hash).unwrap().diagnostics[0];
    assert_eq!(diagnostic.code, AssetDiagnosticCode::InvalidTangent);
    assert_eq!(diagnostic.location, "glb.decoded.generated_tangents");
}

#[test]
#[ignore = "resource-bound whole-import measurement"]
#[allow(clippy::cast_precision_loss)]
fn generated_tangent_maximum_resource_boundaries() {
    const CORNERS: usize = 262_143;
    const FACES: usize = CORNERS / 3;
    let png = encode_png(
        1,
        1,
        png::ColorType::Rgba,
        png::BitDepth::Eight,
        &[128, 128, 255, 255],
    );
    let mut config = AssetStoreConfig::default();
    config.limits.max_asset_decoded_bytes =
        NonZeroU64::new(ASSET_VERTEX_BYTES * u64::try_from(CORNERS).unwrap() + 4).unwrap();

    for shared_positions in [false, true] {
        let mut positions = Vec::with_capacity(CORNERS);
        let mut normals = Vec::with_capacity(CORNERS);
        let mut texcoords = Vec::with_capacity(CORNERS);
        for face in 0..FACES {
            let position_offset = if shared_positions {
                0.0
            } else {
                (face * 2) as f32
            };
            let texcoord_offset = if shared_positions {
                (face * 2) as f32
            } else {
                0.0
            };
            positions.extend([
                [position_offset, 0.0, 0.0],
                [position_offset + 1.0, 0.0, 0.0],
                [position_offset, 1.0, 0.0],
            ]);
            normals.extend([[0.0, 0.0, 1.0]; 3]);
            texcoords.extend([
                [texcoord_offset, 0.0],
                [texcoord_offset + 1.0, 0.0],
                [texcoord_offset, 1.0],
            ]);
        }
        let bytes = generated_normal_textured_glb(&png, &positions, &normals, &texcoords, None);
        let hash = content_hash(&bytes);
        let mut store = AssetStore::new(config);
        store.enqueue(hash, bytes).unwrap();
        assert_eq!(store.process_next().unwrap().state, AssetState::Ready);
        assert_eq!(
            store
                .upload_job(AssetMeshKey {
                    content_hash: hash,
                    mesh_index: 0
                })
                .unwrap()
                .vertices()
                .len(),
            CORNERS
        );
    }

    let mut positions = Vec::with_capacity(CORNERS);
    let mut normals = Vec::with_capacity(CORNERS);
    let mut texcoords = Vec::with_capacity(CORNERS);
    for _ in 0..FACES {
        positions.extend([[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
        normals.extend([[0.0, 0.0, 1.0]; 3]);
        texcoords.extend([[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
    }
    let bytes = generated_normal_textured_glb(&png, &positions, &normals, &texcoords, None);
    let hash = content_hash(&bytes);
    let mut store = AssetStore::new(config);
    store.enqueue(hash, bytes).unwrap();
    assert_eq!(store.process_next().unwrap().state, AssetState::Rejected);
    let diagnostic = &store.record(hash).unwrap().diagnostics[0];
    assert_eq!(
        diagnostic.code,
        AssetDiagnosticCode::CollectionLimitExceeded
    );
    assert_eq!(diagnostic.location, "glb.decoded.generated_tangent_work");
}

#[test]
fn unsupported_tangent_encodings_fail_closed_before_generated_fallback() {
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
fn core_sampler_enums_are_retained_with_the_specified_one_mip_fallback() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let magnification = [
        (9_728, AssetSamplerFilter::Nearest, 0_usize),
        (9_729, AssetSamplerFilter::Linear, 1),
    ];
    let minification = [
        (
            9_728,
            AssetSamplerMinFilter::Nearest,
            AssetSamplerFilter::Nearest,
            0_usize,
        ),
        (
            9_729,
            AssetSamplerMinFilter::Linear,
            AssetSamplerFilter::Linear,
            1,
        ),
        (
            9_984,
            AssetSamplerMinFilter::NearestMipmapNearest,
            AssetSamplerFilter::Nearest,
            0,
        ),
        (
            9_985,
            AssetSamplerMinFilter::LinearMipmapNearest,
            AssetSamplerFilter::Linear,
            1,
        ),
        (
            9_986,
            AssetSamplerMinFilter::NearestMipmapLinear,
            AssetSamplerFilter::Nearest,
            0,
        ),
        (
            9_987,
            AssetSamplerMinFilter::LinearMipmapLinear,
            AssetSamplerFilter::Linear,
            1,
        ),
    ];
    let wrapping = [
        (33_071, AssetSamplerWrap::ClampToEdge),
        (33_648, AssetSamplerWrap::MirroredRepeat),
        (10_497, AssetSamplerWrap::Repeat),
    ];
    let mut effective = std::collections::BTreeSet::new();
    let mut authored = 0_usize;
    for &(mag_value, mag_filter, mag_index) in &magnification {
        for &(min_value, min_filter, effective_min, min_index) in &minification {
            for (wrap_s_index, &(wrap_s_value, wrap_s)) in wrapping.iter().enumerate() {
                for (wrap_t_index, &(wrap_t_value, wrap_t)) in wrapping.iter().enumerate() {
                    let root_fields = format!(
                        r#","samplers":[{{"magFilter":{mag_value},"minFilter":{min_value},"wrapS":{wrap_s_value},"wrapT":{wrap_t_value}}}]"#,
                    );
                    let material = ready_material(textured_triangle_glb(
                        &png,
                        r#""baseColorTexture":{"index":0}"#,
                        r#"{"sampler":0,"source":0}"#,
                        r#"{"bufferView":2,"mimeType":"image/png"}"#,
                        &root_fields,
                        true,
                    ));
                    let sampler = material.base_color_sampler().unwrap();
                    assert_eq!(sampler.mag_filter(), mag_filter);
                    assert_eq!(sampler.min_filter(), min_filter);
                    assert_eq!(sampler.effective_min_filter(), effective_min);
                    assert_eq!(sampler.wrap_s(), wrap_s);
                    assert_eq!(sampler.wrap_t(), wrap_t);
                    effective.insert((wrap_s_index, wrap_t_index, mag_index, min_index));
                    authored += 1;
                }
            }
        }
    }
    assert_eq!(authored, 108);
    assert_eq!(effective.len(), 36);
}

#[test]
fn omitted_and_partial_sampler_defaults_preserve_linear_repeat() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let omitted = ready_material(textured_triangle_glb(
        &png,
        r#""baseColorTexture":{"index":0}"#,
        r#"{"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        "",
        true,
    ));
    assert_eq!(
        omitted.base_color_sampler(),
        Some(AssetSampler::LINEAR_REPEAT)
    );
    for sampler_fields in [
        "",
        r#""magFilter":9729"#,
        r#""minFilter":9729"#,
        r#""wrapS":10497"#,
        r#""wrapT":10497"#,
        r#""magFilter":9729,"minFilter":9729,"wrapS":10497,"wrapT":10497"#,
    ] {
        let root_fields = format!(r#","samplers":[{{{sampler_fields}}}]"#);
        let material = ready_material(textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"sampler":0,"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            &root_fields,
            true,
        ));
        assert_eq!(
            material.base_color_sampler(),
            Some(AssetSampler::LINEAR_REPEAT)
        );
    }
}

#[test]
fn malformed_sampler_record_shapes_and_fields_fail_closed_before_proxy() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    for record in [
        "null",
        "true",
        "0",
        r#""sampler""#,
        "[]",
        r#"{"unknown":0}"#,
    ] {
        for records in [record.to_owned(), format!(r"{{}},{record}")] {
            let root_fields = format!(r#","samplers":[{records}]"#);
            let bytes = textured_triangle_glb(
                &png,
                r#""baseColorTexture":{"index":0}"#,
                r#"{"sampler":0,"source":0}"#,
                r#"{"bufferView":2,"mimeType":"image/png"}"#,
                &root_fields,
                true,
            );
            let (store, hash) = process_with_proxy_policy(bytes);
            assert_eq!(
                store.record(hash).unwrap().state,
                AssetState::Rejected,
                "sampler records {records}"
            );
            assert_eq!(
                store.record(hash).unwrap().diagnostics[0].code,
                AssetDiagnosticCode::InvalidJson
            );
        }
    }

    for field in ["magFilter", "minFilter", "wrapS", "wrapT"] {
        let invalid_enum = if field.starts_with("wrap") {
            "9728"
        } else {
            "33071"
        };
        for value in [
            "null",
            "true",
            r#""nearest""#,
            "{}",
            "[]",
            "1.5",
            "-1",
            "4294967296",
            invalid_enum,
        ] {
            for records in [
                format!(r#"{{"{field}":{value}}}"#),
                format!(r#"{{}},{{"{field}":{value}}}"#),
            ] {
                let root_fields = format!(r#","samplers":[{records}]"#);
                let bytes = textured_triangle_glb(
                    &png,
                    r#""baseColorTexture":{"index":0}"#,
                    r#"{"sampler":0,"source":0}"#,
                    r#"{"bufferView":2,"mimeType":"image/png"}"#,
                    &root_fields,
                    true,
                );
                let (store, hash) = process_with_proxy_policy(bytes);
                assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
                assert_eq!(
                    store.record(hash).unwrap().diagnostics[0].code,
                    AssetDiagnosticCode::InvalidJson
                );
            }
        }
    }
}

#[test]
fn malformed_sampler_indices_counts_and_precedence_fail_closed() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    for sampler_value in [
        "null",
        "true",
        r#""0""#,
        "{}",
        "[]",
        "-1",
        "4294967296",
        "1.5",
        "1",
    ] {
        for unused in [false, true] {
            let root_fields = if unused || sampler_value == "1" {
                r#","samplers":[{}]"#
            } else {
                ""
            };
            let texture = if unused {
                format!(r#"{{"sampler":0,"source":0}},{{"sampler":{sampler_value},"source":0}}"#)
            } else {
                format!(r#"{{"sampler":{sampler_value},"source":0}}"#)
            };
            let bytes = textured_triangle_glb(
                &png,
                r#""baseColorTexture":{"index":0}"#,
                &texture,
                r#"{"bufferView":2,"mimeType":"image/png"}"#,
                root_fields,
                true,
            );
            let (store, hash) = process_with_proxy_policy(bytes);
            assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
            assert!(matches!(
                store.record(hash).unwrap().diagnostics[0].code,
                AssetDiagnosticCode::InvalidJson | AssetDiagnosticCode::InvalidBufferRange
            ));
        }
    }

    let five_records = textured_triangle_glb(
        &png,
        r#""baseColorTexture":{"index":0}"#,
        r#"{"sampler":0,"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        r#","samplers":[{},{},{},{},{}]"#,
        true,
    );
    let (store, hash) = process_with_proxy_policy(five_records);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::CollectionLimitExceeded
    );

    let valid_unused = textured_triangle_glb(
        &png,
        r#""baseColorTexture":{"index":0}"#,
        r#"{"sampler":0,"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        r#","samplers":[{},{}]"#,
        true,
    );
    let (store, hash) = process_with_proxy_policy(valid_unused);
    assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::UnsupportedFeature
    );

    let malformed_before_uri = textured_triangle_glb(
        &png,
        r#""baseColorTexture":{"index":0}"#,
        r#"{"sampler":0,"source":0}"#,
        r#"{"uri":"external.png","mimeType":"image/png"}"#,
        r#","samplers":[{"minFilter":null}]"#,
        true,
    );
    let (store, hash) = process_with_proxy_policy(malformed_before_uri);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
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
fn explicit_default_sampler_is_supported_and_retained() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    let bytes = textured_triangle_glb(
        &png,
        r#""baseColorTexture":{"index":0}"#,
        r#"{"sampler":0,"source":0}"#,
        r#"{"bufferView":2,"mimeType":"image/png"}"#,
        r#","samplers":[{}]"#,
        true,
    );
    let (store, hash) = process_with_proxy_policy(bytes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Ready);
    assert_eq!(
        store
            .upload_job(AssetMeshKey {
                content_hash: hash,
                mesh_index: 0,
            })
            .unwrap()
            .material()
            .base_color_sampler(),
        Some(AssetSampler::LINEAR_REPEAT)
    );
}

#[test]
fn more_than_four_texture_or_image_resources_fail_closed() {
    let png = encode_png(1, 1, png::ColorType::Rgba, png::BitDepth::Eight, &[255; 4]);
    for bytes in [
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0},{"source":0},{"source":0},{"source":0},{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"}"#,
            "",
            true,
        ),
        textured_triangle_glb(
            &png,
            r#""baseColorTexture":{"index":0}"#,
            r#"{"source":0}"#,
            r#"{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"},{"bufferView":2,"mimeType":"image/png"}"#,
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
fn malformed_emissive_factors_are_rejected_without_a_proxy() {
    for emissive in [
        "[0.0,0.0]",
        "[0.0,0.0,0.0,0.0]",
        "null",
        "{}",
        r#"[0.0,"green",0.0]"#,
        "[-0.1,0.0,0.0]",
        "[0.0,1.1,0.0]",
        "[0.0,1e999,0.0]",
    ] {
        let bytes = triangle_glb_with_material_fields(&format!(r#""emissiveFactor":{emissive}"#));
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidJson
        );
    }

    let unused_invalid = glb_with_json(
        r#"{"asset":{"version":"2.0"},"buffers":[{"byteLength":36}],"bufferViews":[{"buffer":0,"byteOffset":0,"byteLength":36}],"accessors":[{"bufferView":0,"byteOffset":0,"componentType":5126,"count":3,"type":"VEC3"}],"materials":[{"emissiveFactor":[2.0,0.0,0.0]},{}],"meshes":[{"primitives":[{"attributes":{"POSITION":0},"material":1,"mode":4}]}]}"#,
        &triangle_binary(),
    );
    let (store, hash) = process_with_proxy_policy(unused_invalid);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );
    assert_eq!(store.record(hash).unwrap().diagnostics[0].index, Some(0));
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
    assert_eq!(upload.byte_len(), 192);
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
    assert_eq!(upload.byte_len(), 192);
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
fn float_vertex_colors_clamp_expand_and_synthesize_vec3_alpha() {
    let vec3_source = [
        [-0.25_f32, 0.25, 1.5, 0.0],
        [0.5, 0.75, 0.25, 0.0],
        [1.0, 0.0, 0.5, 0.0],
        [0.25, 1.0, 0.0, 0.0],
    ];
    let vec3_bytes = vec3_source
        .into_iter()
        .flat_map(|color| {
            color[..3]
                .iter()
                .copied()
                .flat_map(f32::to_le_bytes)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();
    assert_indexed_vertex_colors(
        indexed_glb_with_color_bytes(&vec3_bytes, 4, 5_126, "VEC3", false, 12, r#""COLOR_0":1"#),
        [
            [0.0, 0.25, 1.0, 1.0],
            [0.5, 0.75, 0.25, 1.0],
            [1.0, 0.0, 0.5, 1.0],
            [0.25, 1.0, 0.0, 1.0],
        ],
    );

    let vec4_source = [
        [0.0_f32, 0.25, 0.5, 0.75],
        [1.0, 0.75, 0.5, 0.25],
        [0.2, 0.4, 0.6, 0.8],
        [1.25, -0.25, 0.5, 2.0],
    ];
    let vec4_bytes = vec4_source
        .into_iter()
        .flatten()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    assert_indexed_vertex_colors(
        indexed_glb_with_color_bytes(&vec4_bytes, 4, 5_126, "VEC4", false, 16, r#""COLOR_0":1"#),
        [
            [0.0, 0.25, 0.5, 0.75],
            [1.0, 0.75, 0.5, 0.25],
            [0.2, 0.4, 0.6, 0.8],
            [1.0, 0.0, 0.5, 1.0],
        ],
    );
}

#[test]
fn normalized_integer_vertex_colors_decode_all_core_shapes() {
    let u8_vec3 = [
        [0_u8, 64, 255, 0],
        [128, 255, 0, 0],
        [255, 0, 128, 0],
        [32, 16, 8, 0],
    ];
    assert_indexed_vertex_colors(
        indexed_glb_with_color_bytes(
            &u8_vec3.into_iter().flatten().collect::<Vec<_>>(),
            4,
            5_121,
            "VEC3",
            true,
            4,
            r#""COLOR_0":1"#,
        ),
        u8_vec3.map(|value| {
            [
                f32::from(value[0]) / 255.0,
                f32::from(value[1]) / 255.0,
                f32::from(value[2]) / 255.0,
                1.0,
            ]
        }),
    );

    let u8_vec4 = [
        [0_u8, 64, 128, 255],
        [255, 128, 64, 0],
        [32, 96, 160, 224],
        [1; 4],
    ];
    assert_indexed_vertex_colors(
        indexed_glb_with_color_bytes(
            &u8_vec4.into_iter().flatten().collect::<Vec<_>>(),
            4,
            5_121,
            "VEC4",
            true,
            4,
            r#""COLOR_0":1"#,
        ),
        u8_vec4.map(|value| value.map(|component| f32::from(component) / 255.0)),
    );

    assert_normalized_u16_vertex_colors("VEC3");
    assert_normalized_u16_vertex_colors("VEC4");
}

#[test]
fn malformed_vertex_colors_never_receive_a_proxy() {
    let float_bytes = [0.5_f32; 16]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let mut non_finite = float_bytes.clone();
    non_finite[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    let cases = [
        indexed_glb_with_color_bytes(&float_bytes, 4, 5_126, "VEC2", false, 16, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&[128; 16], 4, 5_121, "VEC4", false, 4, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&float_bytes, 4, 5_126, "VEC4", true, 16, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&float_bytes, 3, 5_126, "VEC4", false, 16, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&[], 0, 5_126, "VEC4", false, 16, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&non_finite, 4, 5_126, "VEC4", false, 16, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&[128; 12], 4, 5_121, "VEC3", true, 3, r#""COLOR_0":1"#),
        indexed_glb_with_color_bytes(&float_bytes, 4, 5_126, "VEC4", false, 16, r#""COLOR_1":1"#),
        indexed_glb_with_color_bytes(
            &float_bytes,
            4,
            5_126,
            "VEC4",
            false,
            16,
            r#""COLOR_0":1,"COLOR_1":2"#,
        ),
        indexed_glb_with_color_bytes(
            &float_bytes,
            4,
            5_126,
            "VEC4",
            false,
            16,
            r#""COLOR_0":1,"COLOR_2":1"#,
        ),
        indexed_glb_with_color_bytes(&float_bytes, 4, 5_126, "VEC4", false, 16, r#""COLOR_01":1"#),
    ];
    for bytes in cases {
        let (store, hash) = process_with_proxy_policy(bytes);
        assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
        assert_eq!(
            store.record(hash).unwrap().diagnostics[0].code,
            AssetDiagnosticCode::InvalidColor
        );
    }

    let truncated =
        indexed_glb_with_color_bytes(&[128; 8], 4, 5_121, "VEC4", true, 4, r#""COLOR_0":1"#);
    let (store, hash) = process_with_proxy_policy(truncated);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );

    let invalid_index =
        indexed_glb_with_color_bytes(&float_bytes, 4, 5_126, "VEC4", false, 16, r#""COLOR_0":99"#);
    let (store, hash) = process_with_proxy_policy(invalid_index);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );

    let canceling_offsets = indexed_glb_with_color_spec(
        &float_bytes,
        ColorGlbSpec {
            color_count: 4,
            component_type: 5_126,
            kind: "VEC4",
            normalized: false,
            byte_stride: 16,
            color_attributes: r#""COLOR_0":1"#,
            primitive_mode: 4,
            canceling_offsets: true,
            include_position: true,
            primitive_count: 1,
        },
    );
    let (store, hash) = process_with_proxy_policy(canceling_offsets);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidBufferRange
    );
}

#[test]
fn vertex_color_preflight_bounds_attributes_and_never_proxies_invalid_peers() {
    let mut invalid = [0.5_f32; 16]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    invalid[..4].copy_from_slice(&f32::NAN.to_le_bytes());
    let missing_position = indexed_glb_with_color_spec(
        &invalid,
        ColorGlbSpec {
            color_count: 4,
            component_type: 5_126,
            kind: "VEC4",
            normalized: false,
            byte_stride: 16,
            color_attributes: r#""COLOR_0":1"#,
            primitive_mode: 4,
            canceling_offsets: false,
            include_position: false,
            primitive_count: 1,
        },
    );
    let (store, hash) = process_with_proxy_policy(missing_position);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidJson
    );

    let multiple_primitives = indexed_glb_with_color_spec(
        &invalid,
        ColorGlbSpec {
            color_count: 4,
            component_type: 5_126,
            kind: "VEC4",
            normalized: false,
            byte_stride: 16,
            color_attributes: r#""COLOR_0":1"#,
            primitive_mode: 4,
            canceling_offsets: false,
            include_position: true,
            primitive_count: 2,
        },
    );
    let (store, hash) = process_with_proxy_policy(multiple_primitives);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::CollectionLimitExceeded
    );

    let attributes = (0..16)
        .map(|set| format!(r#""COLOR_{set}":1"#))
        .collect::<Vec<_>>()
        .join(",");
    let too_many_attributes =
        indexed_glb_with_color_bytes(&[128; 16], 4, 5_121, "VEC4", true, 4, &attributes);
    let (store, hash) = process_with_proxy_policy(too_many_attributes);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::CollectionLimitExceeded
    );
}

#[test]
fn wider_color_sets_proxy_only_after_primary_color_validation() {
    let valid = [0.5_f32; 16]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let wider = indexed_glb_with_color_bytes(
        &valid,
        4,
        5_126,
        "VEC4",
        false,
        16,
        r#""COLOR_0":1,"COLOR_1":1"#,
    );
    let (store, hash) = process_with_proxy_policy(wider);
    assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::UnsupportedFeature
    );

    let mut invalid = valid;
    invalid[..4].copy_from_slice(&f32::INFINITY.to_le_bytes());
    let mixed = indexed_glb_with_color_bytes(
        &invalid,
        4,
        5_126,
        "VEC4",
        false,
        16,
        r#""COLOR_0":1,"COLOR_1":1,"JOINTS_0":1"#,
    );
    let (store, hash) = process_with_proxy_policy(mixed);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidColor
    );

    let unsupported_mode = indexed_glb_with_color_spec(
        &invalid,
        ColorGlbSpec {
            color_count: 4,
            component_type: 5_126,
            kind: "VEC4",
            normalized: false,
            byte_stride: 16,
            color_attributes: r#""COLOR_0":1"#,
            primitive_mode: 1,
            canceling_offsets: false,
            include_position: true,
            primitive_count: 1,
        },
    );
    let (store, hash) = process_with_proxy_policy(unsupported_mode);
    assert_eq!(store.record(hash).unwrap().state, AssetState::Rejected);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::InvalidColor
    );

    let valid_unsupported_mode = indexed_glb_with_color_spec(
        &[0.5_f32; 16]
            .into_iter()
            .flat_map(f32::to_le_bytes)
            .collect::<Vec<_>>(),
        ColorGlbSpec {
            color_count: 4,
            component_type: 5_126,
            kind: "VEC4",
            normalized: false,
            byte_stride: 16,
            color_attributes: r#""COLOR_0":1"#,
            primitive_mode: 1,
            canceling_offsets: false,
            include_position: true,
            primitive_count: 1,
        },
    );
    let (store, hash) = process_with_proxy_policy(valid_unsupported_mode);
    assert_eq!(store.record(hash).unwrap().state, AssetState::ProxyReady);
    assert_eq!(
        store.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::UnsupportedPrimitiveMode
    );
}

#[test]
fn vertex_color_bytes_hold_exact_decoded_limits() {
    let colors = [0.5_f32; 16]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let bytes =
        indexed_glb_with_color_bytes(&colors, 4, 5_126, "VEC4", false, 16, r#""COLOR_0":1"#);
    let hash = content_hash(&bytes);
    let mut exact_config = AssetStoreConfig::default();
    exact_config.limits.max_asset_decoded_bytes = NonZeroU64::new(192).unwrap();
    exact_config.limits.max_resident_cpu_bytes = NonZeroU64::new(192).unwrap();
    let mut exact = AssetStore::new(exact_config);
    exact.enqueue(hash, bytes.clone()).unwrap();
    assert_eq!(exact.process_next().unwrap().state, AssetState::Ready);
    assert_eq!(exact.record(hash).unwrap().decoded_bytes, 192);

    let mut narrow_config = exact_config;
    narrow_config.limits.max_asset_decoded_bytes = NonZeroU64::new(191).unwrap();
    let mut narrow = AssetStore::new(narrow_config);
    narrow.enqueue(hash, bytes).unwrap();
    assert_eq!(narrow.process_next().unwrap().state, AssetState::Rejected);
    assert_eq!(
        narrow.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::ByteLimitExceeded
    );
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
    assert_emissive(upload.material().emissive(), [0.0; 3]);
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

    let proxy_bytes = 36 * ASSET_VERTEX_BYTES;
    let mut exact_config = AssetStoreConfig {
        unsupported_policy: UnsupportedAssetPolicy::ProxyCuboid,
        ..AssetStoreConfig::default()
    };
    exact_config.limits.max_asset_decoded_bytes = NonZeroU64::new(proxy_bytes).unwrap();
    let mut proxying = AssetStore::new(exact_config);
    proxying.enqueue(hash, bytes.clone()).unwrap();
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
    assert!(!upload.material().double_sided());
    let decoded_bytes = proxying.record(hash).unwrap().decoded_bytes;
    let eviction = proxying.evict(hash);
    assert_eq!(eviction.previous_state, Some(AssetState::ProxyReady));
    assert_eq!(eviction.released_resident_cpu_bytes, decoded_bytes);
    assert_eq!(eviction.removed_meshes, 1);
    assert_eq!(eviction.removed_textures, 0);
    assert_eq!(proxying.stats().records, 0);

    let mut narrow_config = exact_config;
    narrow_config.limits.max_asset_decoded_bytes = NonZeroU64::new(proxy_bytes - 1).unwrap();
    let mut narrow = AssetStore::new(narrow_config);
    narrow.enqueue(hash, bytes).unwrap();
    assert_eq!(narrow.process_next().unwrap().state, AssetState::Rejected);
    assert_eq!(narrow.record(hash).unwrap().decoded_bytes, 0);
    assert_eq!(narrow.stats().resident_cpu_bytes, 0);
    assert_eq!(
        narrow.record(hash).unwrap().diagnostics[0].code,
        AssetDiagnosticCode::ByteLimitExceeded
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

fn assert_indexed_vertex_colors(bytes: Vec<u8>, source_colors: [[f32; 4]; 4]) {
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
    for (vertex, source_index) in upload.vertices().iter().zip([2, 0, 1]) {
        assert_color(vertex.color_0, source_colors[source_index]);
    }
}

fn assert_normalized_u16_vertex_colors(kind: &str) {
    let source = [
        [0_u16, 16_384, 65_535, 32_768],
        [65_535, 32_768, 16_384, 0],
        [8_192, 24_576, 40_960, 57_344],
        [1, 2, 3, 4],
    ];
    let component_count = if kind == "VEC3" { 3 } else { 4 };
    let mut bytes = Vec::with_capacity(32);
    for color in source {
        let start = bytes.len();
        for component in color.into_iter().take(component_count) {
            bytes.extend_from_slice(&component.to_le_bytes());
        }
        bytes.resize(start + 8, 0);
    }
    let expected = source.map(|value| {
        let mut decoded = value.map(|component| f32::from(component) / 65_535.0);
        if component_count == 3 {
            decoded[3] = 1.0;
        }
        decoded
    });
    assert_indexed_vertex_colors(
        indexed_glb_with_color_bytes(&bytes, 4, 5_123, kind, true, 8, r#""COLOR_0":1"#),
        expected,
    );
}

fn assert_color(actual: [cogniform_protocol::UnitF32; 4], expected: [f32; 4]) {
    for (actual, expected) in actual.into_iter().zip(expected) {
        assert!((actual.get() - expected).abs() <= f32::EPSILON);
    }
}

fn assert_emissive(actual: [f32; 3], expected: [f32; 3]) {
    assert_eq!(actual.map(f32::to_bits), expected.map(f32::to_bits));
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
