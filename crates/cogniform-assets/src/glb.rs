use core::num::NonZeroU32;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    sync::Arc,
};

use cogniform_protocol::{FiniteF32, UnitF32};
use serde::Deserialize;

use crate::types::{
    ASSET_VERTEX_BYTES, AssetDiagnostic, AssetDiagnosticCode, AssetLimits, AssetMaterial,
    AssetTexture, AssetVertex, DecodedAsset, DecodedMesh,
};

const GLB_MAGIC: [u8; 4] = *b"glTF";
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4e4f_534a;
const BIN_CHUNK: u32 = 0x004e_4942;
const FLOAT: u32 = 5_126;
const UNSIGNED_SHORT: u32 = 5_123;
const UNSIGNED_INT: u32 = 5_125;
const TRIANGLES: u32 = 4;
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const PNG_IHDR_LENGTH: u32 = 13;
const PNG_IEND: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82];
const PNG_RGB: u8 = 2;
const PNG_RGBA: u8 = 6;

pub(crate) fn decode_glb(
    bytes: &[u8],
    limits: AssetLimits,
) -> Result<DecodedAsset, AssetDiagnostic> {
    let (json, binary) = split_glb(bytes, limits)?;
    let mut value: serde_json::Value = serde_json::from_slice(json)
        .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, "glb.json", None))?;
    let unsupported = remove_declared_extensions_and_features(&mut value)?;
    let root: Root = serde_json::from_value(value)
        .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, "glb.json.schema", None))?;
    let decoded = validate_root(&root, binary, limits)?;
    unsupported.map_or(Ok(decoded), Err)
}

fn split_glb(bytes: &[u8], limits: AssetLimits) -> Result<(&[u8], &[u8]), AssetDiagnostic> {
    if bytes.len() < 20 || bytes.get(..4) != Some(&GLB_MAGIC) {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidGlb,
            "glb.header.magic",
            None,
        ));
    }
    if read_u32(bytes, 4) != Some(GLB_VERSION) {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidGlb,
            "glb.header.version",
            None,
        ));
    }
    let declared = read_u32(bytes, 8)
        .map(usize::try_from)
        .transpose()
        .ok()
        .flatten();
    if declared != Some(bytes.len()) {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidLength,
            "glb.header.length",
            None,
        ));
    }

    let (json, json_type, next) = read_chunk(bytes, 12)?;
    if json_type != JSON_CHUNK {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidGlb,
            "glb.chunks[0].type",
            Some(0),
        ));
    }
    if u64::try_from(json.len()).unwrap_or(u64::MAX) > limits.max_json_bytes.get() {
        return Err(diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            "glb.chunks[0].length",
            Some(0),
        ));
    }
    let (binary, binary_type, end) = read_chunk(bytes, next)?;
    if binary_type != BIN_CHUNK || end != bytes.len() {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidGlb,
            "glb.chunks[1]",
            Some(1),
        ));
    }
    if u64::try_from(binary.len()).unwrap_or(u64::MAX) > limits.max_bin_bytes.get() {
        return Err(diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            "glb.chunks[1].length",
            Some(1),
        ));
    }
    Ok((json, binary))
}

fn read_chunk(bytes: &[u8], offset: usize) -> Result<(&[u8], u32, usize), AssetDiagnostic> {
    let length = read_u32(bytes, offset)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| diagnostic(AssetDiagnosticCode::InvalidLength, "glb.chunk.length", None))?;
    if length % 4 != 0 {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidLength,
            "glb.chunk.alignment",
            None,
        ));
    }
    let chunk_type = read_u32(
        bytes,
        offset.checked_add(4).ok_or_else(|| {
            diagnostic(AssetDiagnosticCode::InvalidLength, "glb.chunk.type", None)
        })?,
    )
    .ok_or_else(|| diagnostic(AssetDiagnosticCode::InvalidLength, "glb.chunk.type", None))?;
    let start = offset
        .checked_add(8)
        .ok_or_else(|| diagnostic(AssetDiagnosticCode::InvalidLength, "glb.chunk.start", None))?;
    let end = start
        .checked_add(length)
        .ok_or_else(|| diagnostic(AssetDiagnosticCode::InvalidLength, "glb.chunk.end", None))?;
    let chunk = bytes
        .get(start..end)
        .ok_or_else(|| diagnostic(AssetDiagnosticCode::InvalidLength, "glb.chunk.bytes", None))?;
    Ok((chunk, chunk_type, end))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let encoded: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_le_bytes(encoded))
}

fn remove_declared_extensions_and_features(
    value: &mut serde_json::Value,
) -> Result<Option<AssetDiagnostic>, AssetDiagnostic> {
    let Some(root) = value.as_object_mut() else {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.root",
            None,
        ));
    };
    let mut unsupported = None;
    for field in ["extensionsUsed", "extensionsRequired"] {
        if let Some(declared) = root.remove(field) {
            if !declared
                .as_array()
                .is_some_and(|values| values.iter().all(serde_json::Value::is_string))
            {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.extensions",
                    None,
                ));
            }
            unsupported = Some(diagnostic(
                AssetDiagnosticCode::UnsupportedExtension,
                "glb.json.extensions",
                None,
            ));
        }
    }
    for field in ["animations", "cameras", "nodes", "scenes", "skins"] {
        if let Some(feature) = root.remove(field) {
            if !feature.is_array() {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.root",
                    None,
                ));
            }
            unsupported.get_or_insert_with(|| {
                diagnostic(
                    AssetDiagnosticCode::UnsupportedFeature,
                    "glb.json.root",
                    None,
                )
            });
        }
    }
    if let Some(scene) = root.remove("scene") {
        if !scene.is_u64() {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.scene",
                None,
            ));
        }
        unsupported.get_or_insert_with(|| {
            diagnostic(
                AssetDiagnosticCode::UnsupportedFeature,
                "glb.json.scene",
                None,
            )
        });
    }
    remove_nested_extensions(value, &mut unsupported)?;
    remove_unsupported_material_features(value, &mut unsupported)?;
    Ok(unsupported)
}

fn remove_unsupported_material_features(
    value: &mut serde_json::Value,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<(), AssetDiagnostic> {
    let Some(materials) = value
        .as_object_mut()
        .and_then(|root| root.get_mut("materials"))
    else {
        return Ok(());
    };
    let Some(materials) = materials.as_array_mut() else {
        return Ok(());
    };
    for material in materials {
        let Some(material) = material.as_object_mut() else {
            continue;
        };
        remove_typed_unsupported(material, "occlusionTexture", JsonKind::Object, unsupported)?;
    }
    Ok(())
}

#[derive(Clone, Copy)]
enum JsonKind {
    Object,
}

fn remove_typed_unsupported(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    expected: JsonKind,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<(), AssetDiagnostic> {
    let Some(value) = object.remove(field) else {
        return Ok(());
    };
    let valid = match expected {
        JsonKind::Object => value.is_object(),
    };
    if !valid {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.materials",
            None,
        ));
    }
    unsupported.get_or_insert_with(|| {
        diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.materials",
            None,
        )
    });
    Ok(())
}

fn remove_nested_extensions(
    value: &mut serde_json::Value,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<(), AssetDiagnostic> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(extensions) = object.remove("extensions") {
                if !extensions.is_object() {
                    return Err(diagnostic(
                        AssetDiagnosticCode::InvalidJson,
                        "glb.json.extensions",
                        None,
                    ));
                }
                *unsupported = Some(diagnostic(
                    AssetDiagnosticCode::UnsupportedExtension,
                    "glb.json.extensions",
                    None,
                ));
            }
            for nested in object.values_mut() {
                remove_nested_extensions(nested, unsupported)?;
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                remove_nested_extensions(nested, unsupported)?;
            }
        }
        serde_json::Value::Null
        | serde_json::Value::Bool(_)
        | serde_json::Value::Number(_)
        | serde_json::Value::String(_) => {}
    }
    Ok(())
}

fn validate_root(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
) -> Result<DecodedAsset, AssetDiagnostic> {
    validate_root_header(root, binary, limits)?;
    validate_emissive_factors(root)?;
    let alpha_unsupported = validate_alpha_coverage(root)?;
    let textures = decode_textures(root, binary, limits)?;
    let mut decoded_bytes = textures.byte_len;
    let mut meshes = Vec::with_capacity(root.meshes.len());
    for (mesh_index, mesh) in root.meshes.iter().enumerate() {
        meshes.push(decode_mesh(
            root,
            binary,
            limits,
            stable_index(mesh_index),
            mesh,
            &mut decoded_bytes,
        )?);
    }
    if meshes.is_empty() {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.meshes",
            None,
        ));
    }
    if let Some(unsupported) = textures.unsupported {
        return Err(unsupported);
    }
    if textures.base_color.is_some()
        && !meshes
            .iter()
            .any(|mesh| mesh.material.has_base_color_texture())
    {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.materials.baseColorTexture",
            None,
        ));
    }
    if textures.normal.is_some() && !meshes.iter().any(|mesh| mesh.material.has_normal_texture()) {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.materials.normalTexture",
            None,
        ));
    }
    if textures.emissive.is_some()
        && !meshes
            .iter()
            .any(|mesh| mesh.material.has_emissive_texture())
    {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.materials.emissiveTexture",
            None,
        ));
    }
    if textures.metallic_roughness.is_some()
        && !meshes
            .iter()
            .any(|mesh| mesh.material.has_metallic_roughness_texture())
    {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.materials.metallicRoughnessTexture",
            None,
        ));
    }
    if let Some(unsupported) = alpha_unsupported {
        return Err(unsupported);
    }
    Ok(DecodedAsset {
        meshes,
        base_color_texture: textures.base_color,
        emissive_texture: textures.emissive,
        metallic_roughness_texture: textures.metallic_roughness,
        normal_texture: textures.normal,
        byte_len: decoded_bytes,
    })
}

fn validate_alpha_coverage(root: &Root) -> Result<Option<AssetDiagnostic>, AssetDiagnostic> {
    let mut unsupported = None;
    for (material_index, material) in root.materials.iter().enumerate() {
        let index = Some(stable_index(material_index));
        if material.alpha_cutoff.is_some() && material.alpha_mode.is_none() {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.alphaCutoff",
                index,
            ));
        }
        if let Some(cutoff) = material.alpha_cutoff {
            let cutoff = FiniteF32::new(cutoff).map_err(|_| {
                diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.materials.alphaCutoff",
                    index,
                )
            })?;
            if cutoff.get() < 0.0 {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.materials.alphaCutoff",
                    index,
                ));
            }
        }
        if material
            .alpha_mode
            .as_deref()
            .is_some_and(|mode| !matches!(mode, "OPAQUE" | "MASK"))
        {
            unsupported.get_or_insert_with(|| {
                diagnostic(
                    AssetDiagnosticCode::UnsupportedFeature,
                    "glb.json.materials.alphaMode",
                    index,
                )
            });
        }
    }
    Ok(unsupported)
}

fn validate_emissive_factors(root: &Root) -> Result<(), AssetDiagnostic> {
    for (material_index, material) in root.materials.iter().enumerate() {
        if let Some(values) = material.emissive {
            for value in values {
                UnitF32::new(value).map_err(|_| {
                    diagnostic(
                        AssetDiagnosticCode::InvalidJson,
                        "glb.json.materials.emissiveFactor",
                        Some(stable_index(material_index)),
                    )
                })?;
            }
        }
    }
    Ok(())
}

fn validate_root_header(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
) -> Result<(), AssetDiagnostic> {
    if root.asset.version != "2.0" {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.asset.version",
            None,
        ));
    }
    if root.buffers.is_empty() {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.buffers",
            None,
        ));
    }
    if root.buffers.len() != 1 || root.buffers[0].uri.is_some() {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.buffers",
            None,
        ));
    }
    let declared_binary = usize::try_from(root.buffers[0].byte_length).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidLength,
            "glb.json.buffers[0].byteLength",
            Some(0),
        )
    })?;
    if declared_binary > binary.len() || binary.len().saturating_sub(declared_binary) > 3 {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidLength,
            "glb.json.buffers[0].byteLength",
            Some(0),
        ));
    }
    if count(root.meshes.len()) > limits.max_meshes.get() {
        return Err(diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            "glb.json.meshes",
            None,
        ));
    }
    for (actual, limit, location) in [
        (
            count(root.buffer_views.len()),
            limits.max_buffer_views.get(),
            "glb.json.bufferViews",
        ),
        (
            count(root.accessors.len()),
            limits.max_accessors.get(),
            "glb.json.accessors",
        ),
        (
            count(root.materials.len()),
            limits.max_materials.get(),
            "glb.json.materials",
        ),
    ] {
        if actual > limit {
            return Err(diagnostic(
                AssetDiagnosticCode::CollectionLimitExceeded,
                location,
                None,
            ));
        }
    }
    if root.images.len() > 4 || root.textures.len() > 4 {
        return Err(diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            "glb.json.textures",
            None,
        ));
    }
    Ok(())
}

struct DecodedTextures {
    base_color: Option<AssetTexture>,
    emissive: Option<AssetTexture>,
    metallic_roughness: Option<AssetTexture>,
    normal: Option<AssetTexture>,
    byte_len: u64,
    unsupported: Option<AssetDiagnostic>,
}

fn decode_textures(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
) -> Result<DecodedTextures, AssetDiagnostic> {
    let resources = validate_texture_resources(root, binary, limits)?;
    let mut unsupported = resources.unsupported;
    let [
        base_color_index,
        emissive_index,
        metallic_roughness_index,
        normal_index,
    ] = texture_role_indices(root, &mut unsupported)?;
    let role_indices = [
        base_color_index,
        emissive_index,
        metallic_roughness_index,
        normal_index,
    ];
    if role_indices.iter().all(Option::is_none) {
        if root.textures.is_empty() && root.images.is_empty() {
            return Ok(DecodedTextures {
                base_color: None,
                emissive: None,
                metallic_roughness: None,
                normal: None,
                byte_len: 0,
                unsupported,
            });
        }
        remember_unsupported(
            &mut unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.textures",
            None,
        );
    }
    validate_texture_coordinates(root, &mut unsupported);
    let referenced_textures: BTreeSet<_> = role_indices.into_iter().flatten().collect();
    if referenced_textures.len() != root.textures.len() {
        remember_unsupported(
            &mut unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.textures",
            None,
        );
    }
    let referenced_images = referenced_textures
        .iter()
        .map(|texture_index| resources.texture_sources[texture_index])
        .collect::<BTreeSet<_>>();
    if referenced_images.len() != root.images.len() {
        remember_unsupported(
            &mut unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.images",
            None,
        );
    }
    let role_texture = |texture_index: Option<u32>| {
        texture_index
            .and_then(|texture_index| resources.texture_sources.get(&texture_index))
            .and_then(|source| resources.decoded_images.get(source))
            .cloned()
    };
    Ok(DecodedTextures {
        base_color: role_texture(base_color_index),
        emissive: role_texture(emissive_index),
        metallic_roughness: role_texture(metallic_roughness_index),
        normal: role_texture(normal_index),
        byte_len: resources.byte_len,
        unsupported,
    })
}

fn texture_role_indices(
    root: &Root,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<[Option<u32>; 4], AssetDiagnostic> {
    let base_color_index = shared_texture_index(
        root.materials.iter().filter_map(|material| {
            material
                .pbr_metallic_roughness
                .as_ref()
                .and_then(|pbr| pbr.base_color_texture.as_ref())
                .map(|info| info.index)
        }),
        root.textures.len(),
        "glb.json.materials.baseColorTexture.index",
        unsupported,
    )?;
    let emissive_index = shared_texture_index(
        root.materials
            .iter()
            .filter_map(|material| material.emissive_texture.as_ref().map(|info| info.index)),
        root.textures.len(),
        "glb.json.materials.emissiveTexture.index",
        unsupported,
    )?;
    let normal_index = shared_texture_index(
        root.materials
            .iter()
            .filter_map(|material| material.normal_texture.as_ref().map(|info| info.index)),
        root.textures.len(),
        "glb.json.materials.normalTexture.index",
        unsupported,
    )?;
    let metallic_roughness_index = shared_texture_index(
        root.materials.iter().filter_map(|material| {
            material
                .pbr_metallic_roughness
                .as_ref()
                .and_then(|pbr| pbr.metallic_roughness_texture.as_ref())
                .map(|info| info.index)
        }),
        root.textures.len(),
        "glb.json.materials.metallicRoughnessTexture.index",
        unsupported,
    )?;
    Ok([
        base_color_index,
        emissive_index,
        metallic_roughness_index,
        normal_index,
    ])
}

struct ValidatedTextureResources {
    texture_sources: BTreeMap<u32, u32>,
    decoded_images: BTreeMap<u32, AssetTexture>,
    byte_len: u64,
    unsupported: Option<AssetDiagnostic>,
}

fn validate_texture_resources(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
) -> Result<ValidatedTextureResources, AssetDiagnostic> {
    let mut unsupported = None;
    if !root.samplers.is_empty() {
        remember_unsupported(
            &mut unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.samplers",
            None,
        );
    }
    let texture_sources = validate_texture_sources(root, &mut unsupported)?;
    let (decoded_images, byte_len) = validate_root_images(root, binary, limits, &mut unsupported)?;
    Ok(ValidatedTextureResources {
        texture_sources,
        decoded_images,
        byte_len,
        unsupported,
    })
}

fn validate_texture_sources(
    root: &Root,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<BTreeMap<u32, u32>, AssetDiagnostic> {
    let mut texture_sources = BTreeMap::new();
    for (texture_index, texture) in root.textures.iter().enumerate() {
        let texture_index = u32::try_from(texture_index).expect("texture count is bounded");
        let source = texture.source.ok_or_else(|| {
            diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.textures[].source",
                Some(texture_index),
            )
        })?;
        get(&root.images, source, "glb.json.images")?;
        texture_sources.insert(texture_index, source);
        if texture.sampler.is_some() {
            remember_unsupported(
                unsupported,
                AssetDiagnosticCode::UnsupportedFeature,
                "glb.json.textures[].sampler",
                Some(texture_index),
            );
        }
    }
    Ok(texture_sources)
}

fn validate_root_images(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<(BTreeMap<u32, AssetTexture>, u64), AssetDiagnostic> {
    let mut decoded_images = BTreeMap::new();
    let mut byte_len = 0_u64;
    for (image_index, image) in root.images.iter().enumerate() {
        let image_index = u32::try_from(image_index).expect("image count is bounded");
        if let Some(decoded) =
            validate_root_image(root, binary, limits, image_index, image, unsupported)?
        {
            byte_len = checked_texture_bytes(byte_len, decoded.byte_len(), limits)?;
            decoded_images.insert(image_index, decoded);
        }
    }
    Ok((decoded_images, byte_len))
}

fn validate_root_image(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
    image_index: u32,
    image: &Image,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<Option<AssetTexture>, AssetDiagnostic> {
    if image.uri.is_some() {
        if image.buffer_view.is_some() {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.images[]",
                Some(image_index),
            ));
        }
        remember_unsupported(
            unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.images[]",
            Some(image_index),
        );
        return Ok(None);
    }
    let view_index = image.buffer_view.ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.images[].bufferView",
            Some(image_index),
        )
    })?;
    let (bytes, has_stride) = image_view_bytes(root, binary, view_index)?;
    if has_stride {
        remember_unsupported(
            unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.images[].bufferView.byteStride",
            Some(view_index),
        );
    }
    let mime_type = image.mime_type.as_deref().ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.images[].mimeType",
            Some(image_index),
        )
    })?;
    if mime_type != "image/png" {
        remember_unsupported(
            unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.images[]",
            Some(image_index),
        );
        return Ok(None);
    }
    match decode_png(bytes, limits, image_index) {
        Ok(decoded) => Ok(Some(decoded)),
        Err(diagnostic) if diagnostic.code.permits_proxy() => {
            if unsupported.is_none() {
                *unsupported = Some(diagnostic);
            }
            Ok(None)
        }
        Err(diagnostic) => Err(diagnostic),
    }
}

fn image_view_bytes<'a>(
    root: &Root,
    binary: &'a [u8],
    view_index: u32,
) -> Result<(&'a [u8], bool), AssetDiagnostic> {
    let view = get(&root.buffer_views, view_index, "glb.json.bufferViews")?;
    if view.buffer != 0 {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.images[].bufferView",
            Some(view_index),
        ));
    }
    let start = usize::try_from(view.byte_offset).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.images[].bufferView",
            Some(view_index),
        )
    })?;
    let length = usize::try_from(view.byte_length).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.images[].bufferView",
            Some(view_index),
        )
    })?;
    let end = start.checked_add(length).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.images[].bufferView",
            Some(view_index),
        )
    })?;
    let bytes = binary.get(start..end).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.images[].bufferView",
            Some(view_index),
        )
    })?;
    Ok((bytes, view.byte_stride.is_some()))
}

fn checked_texture_bytes(
    total: u64,
    decoded: u64,
    limits: AssetLimits,
) -> Result<u64, AssetDiagnostic> {
    let total = total.checked_add(decoded).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            "glb.decoded.asset_bytes",
            None,
        )
    })?;
    if total > limits.max_asset_decoded_bytes.get() {
        return Err(diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            "glb.decoded.asset_bytes",
            None,
        ));
    }
    Ok(total)
}

fn remember_unsupported(
    unsupported: &mut Option<AssetDiagnostic>,
    code: AssetDiagnosticCode,
    location: &'static str,
    index: Option<u32>,
) {
    if unsupported.is_none() {
        *unsupported = Some(diagnostic(code, location, index));
    }
}

fn shared_texture_index(
    references: impl Iterator<Item = u32>,
    texture_count: usize,
    location: &'static str,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<Option<u32>, AssetDiagnostic> {
    let indices: BTreeSet<_> = references.collect();
    if let Some(&invalid) = indices
        .iter()
        .find(|&&index| usize::try_from(index).map_or(true, |index| index >= texture_count))
    {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            location,
            Some(invalid),
        ));
    }
    if indices.len() > 1 {
        remember_unsupported(
            unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            location,
            None,
        );
    }
    Ok(indices.into_iter().next())
}

fn validate_texture_coordinates(root: &Root, unsupported: &mut Option<AssetDiagnostic>) {
    for material in &root.materials {
        if let Some(info) = material
            .pbr_metallic_roughness
            .as_ref()
            .and_then(|pbr| pbr.base_color_texture.as_ref())
            && info.tex_coord.unwrap_or(0) != 0
        {
            remember_unsupported(
                unsupported,
                AssetDiagnosticCode::UnsupportedFeature,
                "glb.json.materials.baseColorTexture.texCoord",
                None,
            );
        }
        if let Some(info) = material.normal_texture.as_ref()
            && info.tex_coord.unwrap_or(0) != 0
        {
            remember_unsupported(
                unsupported,
                AssetDiagnosticCode::UnsupportedFeature,
                "glb.json.materials.normalTexture.texCoord",
                None,
            );
        }
        if let Some(info) = material.emissive_texture.as_ref()
            && info.tex_coord.unwrap_or(0) != 0
        {
            remember_unsupported(
                unsupported,
                AssetDiagnosticCode::UnsupportedFeature,
                "glb.json.materials.emissiveTexture.texCoord",
                None,
            );
        }
        if let Some(info) = material
            .pbr_metallic_roughness
            .as_ref()
            .and_then(|pbr| pbr.metallic_roughness_texture.as_ref())
            && info.tex_coord.unwrap_or(0) != 0
        {
            remember_unsupported(
                unsupported,
                AssetDiagnosticCode::UnsupportedFeature,
                "glb.json.materials.metallicRoughnessTexture.texCoord",
                None,
            );
        }
    }
}

fn decode_png(
    bytes: &[u8],
    limits: AssetLimits,
    image_index: u32,
) -> Result<AssetTexture, AssetDiagnostic> {
    validate_png_framing(bytes, image_index)?;
    let width = read_be_u32(bytes, 16)
        .and_then(NonZeroU32::new)
        .ok_or_else(|| image_diagnostic(AssetDiagnosticCode::InvalidImage, image_index))?;
    let height = read_be_u32(bytes, 20)
        .and_then(NonZeroU32::new)
        .ok_or_else(|| image_diagnostic(AssetDiagnosticCode::InvalidImage, image_index))?;
    decode_png_image(bytes, width, height, limits, image_index)
}

fn validate_png_framing(bytes: &[u8], image_index: u32) -> Result<(), AssetDiagnostic> {
    if bytes.len() < 33
        || bytes.get(..8) != Some(&PNG_SIGNATURE)
        || !bytes.ends_with(&PNG_IEND)
        || read_be_u32(bytes, 8) != Some(PNG_IHDR_LENGTH)
        || bytes.get(12..16) != Some(b"IHDR")
    {
        return Err(image_diagnostic(
            AssetDiagnosticCode::InvalidImage,
            image_index,
        ));
    }
    Ok(())
}

fn decode_png_image(
    bytes: &[u8],
    width: NonZeroU32,
    height: NonZeroU32,
    limits: AssetLimits,
    image_index: u32,
) -> Result<AssetTexture, AssetDiagnostic> {
    let (pixels, rgba_bytes) = validated_texture_size(width, height, limits, image_index)?;
    let color_type = validated_png_color(bytes, image_index)?;
    decode_png_pixels(
        bytes,
        PngLayout {
            width,
            height,
            pixels,
            rgba_bytes,
            color_type,
        },
        limits,
        image_index,
    )
}

#[derive(Clone, Copy)]
struct PngLayout {
    width: NonZeroU32,
    height: NonZeroU32,
    pixels: u64,
    rgba_bytes: u64,
    color_type: u8,
}

fn validated_texture_size(
    width: NonZeroU32,
    height: NonZeroU32,
    limits: AssetLimits,
    image_index: u32,
) -> Result<(u64, u64), AssetDiagnostic> {
    if width.get() > limits.max_texture_dimension_2d.get()
        || height.get() > limits.max_texture_dimension_2d.get()
    {
        return Err(image_diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            image_index,
        ));
    }
    let pixels = u64::from(width.get())
        .checked_mul(u64::from(height.get()))
        .ok_or_else(|| image_diagnostic(AssetDiagnosticCode::ByteLimitExceeded, image_index))?;
    if pixels > limits.max_texture_pixels.get() {
        return Err(image_diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            image_index,
        ));
    }
    let rgba_bytes = pixels
        .checked_mul(4)
        .ok_or_else(|| image_diagnostic(AssetDiagnosticCode::ByteLimitExceeded, image_index))?;
    if rgba_bytes > limits.max_texture_decoded_bytes.get()
        || rgba_bytes > limits.max_asset_decoded_bytes.get()
    {
        return Err(image_diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            image_index,
        ));
    }
    Ok((pixels, rgba_bytes))
}

fn validated_png_color(bytes: &[u8], image_index: u32) -> Result<u8, AssetDiagnostic> {
    let bit_depth = bytes[24];
    let color_type = bytes[25];
    if bit_depth != 8 || !matches!(color_type, PNG_RGB | PNG_RGBA) || bytes[26..29] != [0, 0, 0] {
        return Err(image_diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            image_index,
        ));
    }
    Ok(color_type)
}

fn decode_png_pixels(
    bytes: &[u8],
    layout: PngLayout,
    limits: AssetLimits,
    image_index: u32,
) -> Result<AssetTexture, AssetDiagnostic> {
    let PngLayout {
        width,
        height,
        pixels,
        rgba_bytes,
        color_type,
    } = layout;
    let decoder_limit =
        usize::try_from(limits.max_texture_decoder_bytes.get()).unwrap_or(usize::MAX);
    let mut decoder = png::Decoder::new_with_limits(
        Cursor::new(bytes),
        png::Limits {
            bytes: decoder_limit,
        },
    );
    decoder.set_transformations(png::Transformations::IDENTITY);
    decoder.set_ignore_text_chunk(true);
    decoder.set_ignore_iccp_chunk(true);
    let mut reader = decoder
        .read_info()
        .map_err(|error| png_diagnostic(&error, image_index))?;
    if reader.info().animation_control.is_some() {
        return Err(image_diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            image_index,
        ));
    }
    if reader.info().trns.is_some() {
        return Err(image_diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            image_index,
        ));
    }
    let channels = if color_type == PNG_RGB { 3_u64 } else { 4_u64 };
    let raw_bytes = pixels
        .checked_mul(channels)
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| image_diagnostic(AssetDiagnosticCode::ByteLimitExceeded, image_index))?;
    if reader.output_buffer_size() != Some(raw_bytes) {
        return Err(image_diagnostic(
            AssetDiagnosticCode::InvalidImage,
            image_index,
        ));
    }
    let mut raw = vec![0_u8; raw_bytes];
    let output = reader
        .next_frame(&mut raw)
        .map_err(|error| png_diagnostic(&error, image_index))?;
    let expected_color = if color_type == PNG_RGB {
        png::ColorType::Rgb
    } else {
        png::ColorType::Rgba
    };
    if output.width != width.get()
        || output.height != height.get()
        || output.bit_depth != png::BitDepth::Eight
        || output.color_type != expected_color
        || output.buffer_size() != raw_bytes
    {
        return Err(image_diagnostic(
            AssetDiagnosticCode::InvalidImage,
            image_index,
        ));
    }
    reader
        .finish()
        .map_err(|error| png_diagnostic(&error, image_index))?;
    let rgba8 = if color_type == PNG_RGBA {
        raw
    } else {
        let capacity = usize::try_from(rgba_bytes)
            .map_err(|_| image_diagnostic(AssetDiagnosticCode::ByteLimitExceeded, image_index))?;
        let mut rgba8 = Vec::with_capacity(capacity);
        for rgb in raw.chunks_exact(3) {
            rgba8.extend_from_slice(rgb);
            rgba8.push(255);
        }
        rgba8
    };
    Ok(AssetTexture::new(width, height, Arc::from(rgba8)))
}

fn read_be_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let encoded: [u8; 4] = bytes.get(offset..offset.checked_add(4)?)?.try_into().ok()?;
    Some(u32::from_be_bytes(encoded))
}

const fn image_diagnostic(code: AssetDiagnosticCode, image_index: u32) -> AssetDiagnostic {
    diagnostic(code, "glb.binary.image.png", Some(image_index))
}

fn png_diagnostic(error: &png::DecodingError, image_index: u32) -> AssetDiagnostic {
    if matches!(error, png::DecodingError::LimitsExceeded) {
        image_diagnostic(AssetDiagnosticCode::ByteLimitExceeded, image_index)
    } else {
        image_diagnostic(AssetDiagnosticCode::InvalidImage, image_index)
    }
}

fn decode_mesh(
    root: &Root,
    binary: &[u8],
    limits: AssetLimits,
    mesh_index: u32,
    mesh: &Mesh,
    decoded_bytes: &mut u64,
) -> Result<DecodedMesh, AssetDiagnostic> {
    let primitive = validated_primitive(mesh, limits, mesh_index)?;
    let VertexLayouts {
        position_index,
        positions,
        normals,
        tangents,
        texcoords,
    } = vertex_layouts(root, binary, primitive)?;
    let (indices, output_count) =
        output_layout(root, binary, primitive, positions, position_index, limits)?;
    if output_count > limits.max_vertices_per_mesh.get() {
        return Err(diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            "glb.decoded.vertices",
            Some(mesh_index),
        ));
    }
    let mesh_bytes = u64::from(output_count)
        .checked_mul(ASSET_VERTEX_BYTES)
        .ok_or_else(|| {
            diagnostic(
                AssetDiagnosticCode::ByteLimitExceeded,
                "glb.decoded.mesh_bytes",
                Some(mesh_index),
            )
        })?;
    *decoded_bytes = decoded_bytes.checked_add(mesh_bytes).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            "glb.decoded.asset_bytes",
            None,
        )
    })?;
    if *decoded_bytes > limits.max_asset_decoded_bytes.get() {
        return Err(diagnostic(
            AssetDiagnosticCode::ByteLimitExceeded,
            "glb.decoded.asset_bytes",
            None,
        ));
    }
    if let Some(texcoords) = texcoords {
        validate_texcoords(binary, texcoords)?;
    }
    if let Some(tangents) = tangents {
        validate_tangents(binary, tangents)?;
    }
    let vertices = decode_vertices(
        binary,
        positions,
        normals,
        tangents,
        texcoords,
        indices,
        output_count,
    )?;
    let material = decode_material(root, primitive.material)?;
    if (material.has_base_color_texture()
        || material.has_emissive_texture()
        || material.has_metallic_roughness_texture()
        || material.has_normal_texture())
        && texcoords.is_none()
    {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidTexcoord,
            "glb.json.meshes[].primitives[].attributes.TEXCOORD_0",
            Some(mesh_index),
        ));
    }
    if material.has_normal_texture() && (normals.is_none() || tangents.is_none()) {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.meshes[].primitives[].attributes.TANGENT",
            Some(mesh_index),
        ));
    }
    Ok(DecodedMesh {
        vertices: Arc::from(vertices),
        material,
    })
}

fn validated_primitive(
    mesh: &Mesh,
    limits: AssetLimits,
    mesh_index: u32,
) -> Result<&Primitive, AssetDiagnostic> {
    if mesh.primitives.len() != 1
        || count(mesh.primitives.len()) > limits.max_primitives_per_mesh.get()
    {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.meshes[].primitives",
            Some(mesh_index),
        ));
    }
    let primitive = &mesh.primitives[0];
    if primitive.mode.unwrap_or(TRIANGLES) != TRIANGLES {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedPrimitiveMode,
            "glb.json.meshes[].primitives[].mode",
            Some(mesh_index),
        ));
    }
    let attributes_supported = primitive.attributes.contains_key("POSITION")
        && primitive.attributes.keys().all(|attribute| {
            matches!(
                attribute.as_str(),
                "POSITION" | "NORMAL" | "TANGENT" | "TEXCOORD_0"
            )
        });
    if !attributes_supported {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.meshes[].primitives[].attributes",
            Some(mesh_index),
        ));
    }
    Ok(primitive)
}

struct VertexLayouts {
    position_index: u32,
    positions: AccessorLayout,
    normals: Option<AccessorLayout>,
    tangents: Option<AccessorLayout>,
    texcoords: Option<AccessorLayout>,
}

fn vertex_layouts(
    root: &Root,
    binary: &[u8],
    primitive: &Primitive,
) -> Result<VertexLayouts, AssetDiagnostic> {
    let position_index = primitive.attributes["POSITION"];
    let positions = accessor_layout(root, binary, position_index, AccessorExpectation::Positions)?;
    let normals = primitive
        .attributes
        .get("NORMAL")
        .copied()
        .map(|normal_index| {
            accessor_layout(root, binary, normal_index, AccessorExpectation::Normals)
                .map(|layout| (normal_index, layout))
        })
        .transpose()?;
    if let Some((normal_index, normals)) = normals
        && normals.count != positions.count
    {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidNormal,
            "glb.json.accessors.normal.count",
            Some(normal_index),
        ));
    }
    let tangents = primitive
        .attributes
        .get("TANGENT")
        .copied()
        .map(|tangent_index| {
            accessor_layout(root, binary, tangent_index, AccessorExpectation::Tangents)
                .map(|layout| (tangent_index, layout))
        })
        .transpose()?;
    if let Some((tangent_index, tangents)) = tangents
        && tangents.count != positions.count
    {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidTangent,
            "glb.json.accessors.tangent.count",
            Some(tangent_index),
        ));
    }
    let texcoords = primitive
        .attributes
        .get("TEXCOORD_0")
        .copied()
        .map(|texcoord_index| {
            accessor_layout(root, binary, texcoord_index, AccessorExpectation::Texcoords)
                .map(|layout| (texcoord_index, layout))
        })
        .transpose()?;
    if let Some((texcoord_index, texcoords)) = texcoords
        && texcoords.count != positions.count
    {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidTexcoord,
            "glb.json.accessors.texcoord_0.count",
            Some(texcoord_index),
        ));
    }
    Ok(VertexLayouts {
        position_index,
        positions,
        normals: normals.map(|(_, layout)| layout),
        tangents: tangents.map(|(_, layout)| layout),
        texcoords: texcoords.map(|(_, layout)| layout),
    })
}

fn output_layout(
    root: &Root,
    binary: &[u8],
    primitive: &Primitive,
    positions: AccessorLayout,
    position_index: u32,
    limits: AssetLimits,
) -> Result<(Option<AccessorLayout>, u32), AssetDiagnostic> {
    let indices = primitive
        .indices
        .map(|index_accessor| {
            accessor_layout(root, binary, index_accessor, AccessorExpectation::Indices)
                .map(|layout| (index_accessor, layout))
        })
        .transpose()?;
    let output_count = if let Some((index_accessor, indices)) = indices {
        if indices.count > limits.max_indices_per_mesh.get() || !indices.count.is_multiple_of(3) {
            return Err(diagnostic(
                AssetDiagnosticCode::CollectionLimitExceeded,
                "glb.json.accessors.indices.count",
                Some(index_accessor),
            ));
        }
        indices.count
    } else {
        if !positions.count.is_multiple_of(3) {
            return Err(diagnostic(
                AssetDiagnosticCode::UnsupportedPrimitiveMode,
                "glb.json.accessors.position.count",
                Some(position_index),
            ));
        }
        positions.count
    };
    Ok((indices.map(|(_, layout)| layout), output_count))
}

#[derive(Clone, Copy)]
enum AccessorExpectation {
    Positions,
    Normals,
    Tangents,
    Texcoords,
    Indices,
}

#[derive(Clone, Copy)]
struct AccessorLayout {
    start: usize,
    stride: usize,
    count: u32,
    component_type: u32,
}

fn accessor_layout(
    root: &Root,
    binary: &[u8],
    accessor_index: u32,
    expectation: AccessorExpectation,
) -> Result<AccessorLayout, AssetDiagnostic> {
    let accessor = get(&root.accessors, accessor_index, "glb.json.accessors")?;
    let view = get(
        &root.buffer_views,
        accessor.buffer_view,
        "glb.json.bufferViews",
    )?;
    if view.buffer != 0 || accessor.normalized {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedAccessor,
            "glb.json.accessors",
            Some(accessor_index),
        ));
    }
    if accessor.count == 0 {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedAccessor,
            "glb.json.accessors.count",
            Some(accessor_index),
        ));
    }
    let (element_bytes, component_alignment) =
        accessor_format(accessor, expectation, accessor_index)?;
    accessor_range(
        binary,
        accessor,
        view,
        accessor_index,
        element_bytes,
        component_alignment,
    )
}

fn accessor_format(
    accessor: &Accessor,
    expectation: AccessorExpectation,
    accessor_index: u32,
) -> Result<(usize, usize), AssetDiagnostic> {
    match expectation {
        AccessorExpectation::Positions | AccessorExpectation::Normals
            if accessor.component_type == FLOAT && accessor.kind == "VEC3" =>
        {
            Ok((12, 4))
        }
        AccessorExpectation::Tangents
            if accessor.component_type == FLOAT && accessor.kind == "VEC4" =>
        {
            Ok((16, 4))
        }
        AccessorExpectation::Texcoords
            if accessor.component_type == FLOAT && accessor.kind == "VEC2" =>
        {
            Ok((8, 4))
        }
        AccessorExpectation::Indices
            if accessor.kind == "SCALAR"
                && matches!(accessor.component_type, UNSIGNED_SHORT | UNSIGNED_INT) =>
        {
            let width = if accessor.component_type == UNSIGNED_SHORT {
                2
            } else {
                4
            };
            Ok((width, width))
        }
        AccessorExpectation::Positions
        | AccessorExpectation::Normals
        | AccessorExpectation::Tangents
        | AccessorExpectation::Texcoords
        | AccessorExpectation::Indices => Err(diagnostic(
            AssetDiagnosticCode::UnsupportedAccessor,
            "glb.json.accessors",
            Some(accessor_index),
        )),
    }
}

fn accessor_range(
    binary: &[u8],
    accessor: &Accessor,
    view: &BufferView,
    accessor_index: u32,
    element_bytes: usize,
    component_alignment: usize,
) -> Result<AccessorLayout, AssetDiagnostic> {
    let stride = usize::try_from(
        view.byte_stride
            .unwrap_or(u32::try_from(element_bytes).unwrap()),
    )
    .map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.bufferViews.byteStride",
            Some(accessor.buffer_view),
        )
    })?;
    if stride < element_bytes || stride % component_alignment != 0 || stride > 252 {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedAccessor,
            "glb.json.bufferViews.byteStride",
            Some(accessor.buffer_view),
        ));
    }
    let view_start = usize::try_from(view.byte_offset).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.bufferViews.byteOffset",
            Some(accessor.buffer_view),
        )
    })?;
    let view_length = usize::try_from(view.byte_length).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.bufferViews.byteLength",
            Some(accessor.buffer_view),
        )
    })?;
    let accessor_offset = usize::try_from(accessor.byte_offset).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.accessors.byteOffset",
            Some(accessor_index),
        )
    })?;
    let view_end = view_start.checked_add(view_length).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.bufferViews.range",
            Some(accessor.buffer_view),
        )
    })?;
    if view_end > binary.len() {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.bufferViews.range",
            Some(accessor.buffer_view),
        ));
    }
    let start = view_start.checked_add(accessor_offset).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.accessors.range",
            Some(accessor_index),
        )
    })?;
    let occupied = usize::try_from(accessor.count - 1)
        .ok()
        .and_then(|count| count.checked_mul(stride))
        .and_then(|bytes| bytes.checked_add(element_bytes))
        .ok_or_else(|| {
            diagnostic(
                AssetDiagnosticCode::InvalidBufferRange,
                "glb.json.accessors.range",
                Some(accessor_index),
            )
        })?;
    let end = start.checked_add(occupied).ok_or_else(|| {
        diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.accessors.range",
            Some(accessor_index),
        )
    })?;
    if start < view_start || end > view_end {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.accessors.range",
            Some(accessor_index),
        ));
    }
    Ok(AccessorLayout {
        start,
        stride,
        count: accessor.count,
        component_type: accessor.component_type,
    })
}

fn decode_vertices(
    binary: &[u8],
    positions: AccessorLayout,
    normals: Option<AccessorLayout>,
    tangents: Option<AccessorLayout>,
    texcoords: Option<AccessorLayout>,
    indices: Option<AccessorLayout>,
    output_count: u32,
) -> Result<Vec<AssetVertex>, AssetDiagnostic> {
    let mut vertices = Vec::with_capacity(usize::try_from(output_count).unwrap_or(usize::MAX));
    for output_index in 0..output_count {
        let position_index = if let Some(indices) = indices {
            read_index(binary, indices, output_index)?
        } else {
            output_index
        };
        if position_index >= positions.count {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidIndex,
                "glb.binary.indices",
                Some(output_index),
            ));
        }
        let position = read_position(binary, positions, position_index)?;
        let normal = normals
            .map(|layout| read_normal(binary, layout, position_index))
            .transpose()?
            .unwrap_or([FiniteF32::new(0.0).expect("zero is finite"); 3]);
        let tangent = tangents
            .map(|layout| read_tangent(binary, layout, position_index))
            .transpose()?
            .unwrap_or([
                FiniteF32::new(1.0).expect("one is finite"),
                FiniteF32::new(0.0).expect("zero is finite"),
                FiniteF32::new(0.0).expect("zero is finite"),
                FiniteF32::new(1.0).expect("one is finite"),
            ]);
        let texcoord_0 = texcoords
            .map(|layout| read_texcoord(binary, layout, position_index))
            .transpose()?
            .unwrap_or([FiniteF32::new(0.0).expect("zero is finite"); 2]);
        vertices.push(AssetVertex {
            position,
            normal,
            tangent,
            texcoord_0,
        });
    }
    if normals.is_none() {
        for triangle in vertices.chunks_exact_mut(3) {
            let normal = face_normal(
                triangle[0].position,
                triangle[1].position,
                triangle[2].position,
            )?;
            for vertex in triangle {
                vertex.normal = normal;
            }
        }
    }
    if tangents.is_some() {
        for (triangle_index, triangle) in vertices.chunks_exact(3).enumerate() {
            let handedness = triangle[0].tangent[3].get();
            if triangle[1..]
                .iter()
                .any(|vertex| vertex.tangent[3].get().to_bits() != handedness.to_bits())
            {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidTangent,
                    "glb.decoded.triangle_tangent_handedness",
                    Some(stable_index(triangle_index)),
                ));
            }
        }
    }
    Ok(vertices)
}

fn validate_texcoords(binary: &[u8], layout: AccessorLayout) -> Result<(), AssetDiagnostic> {
    for index in 0..layout.count {
        read_texcoord(binary, layout, index)?;
    }
    Ok(())
}

fn validate_tangents(binary: &[u8], layout: AccessorLayout) -> Result<(), AssetDiagnostic> {
    for index in 0..layout.count {
        read_tangent(binary, layout, index)?;
    }
    Ok(())
}

fn read_index(binary: &[u8], layout: AccessorLayout, index: u32) -> Result<u32, AssetDiagnostic> {
    let offset = element_offset(layout, index, "glb.binary.indices")?;
    match layout.component_type {
        UNSIGNED_SHORT => {
            let encoded: [u8; 2] = binary
                .get(offset..offset + 2)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| {
                    diagnostic(
                        AssetDiagnosticCode::InvalidBufferRange,
                        "glb.binary.indices",
                        Some(index),
                    )
                })?;
            Ok(u32::from(u16::from_le_bytes(encoded)))
        }
        UNSIGNED_INT => {
            let encoded: [u8; 4] = binary
                .get(offset..offset + 4)
                .and_then(|value| value.try_into().ok())
                .ok_or_else(|| {
                    diagnostic(
                        AssetDiagnosticCode::InvalidBufferRange,
                        "glb.binary.indices",
                        Some(index),
                    )
                })?;
            Ok(u32::from_le_bytes(encoded))
        }
        _ => Err(diagnostic(
            AssetDiagnosticCode::UnsupportedAccessor,
            "glb.binary.indices",
            Some(index),
        )),
    }
}

fn read_position(
    binary: &[u8],
    layout: AccessorLayout,
    index: u32,
) -> Result<[FiniteF32; 3], AssetDiagnostic> {
    let offset = element_offset(layout, index, "glb.binary.positions")?;
    let mut position = [FiniteF32::new(0.0).expect("zero is finite"); 3];
    for (component, target) in position.iter_mut().enumerate() {
        let start = offset + component * 4;
        let encoded: [u8; 4] = binary
            .get(start..start + 4)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| {
                diagnostic(
                    AssetDiagnosticCode::InvalidBufferRange,
                    "glb.binary.positions",
                    Some(index),
                )
            })?;
        *target = FiniteF32::new(f32::from_le_bytes(encoded)).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::NonFiniteVertex,
                "glb.binary.positions",
                Some(index),
            )
        })?;
    }
    Ok(position)
}

fn read_normal(
    binary: &[u8],
    layout: AccessorLayout,
    index: u32,
) -> Result<[FiniteF32; 3], AssetDiagnostic> {
    let offset = element_offset(layout, index, "glb.binary.normals")?;
    let mut normal = [FiniteF32::new(0.0).expect("zero is finite"); 3];
    for (component, target) in normal.iter_mut().enumerate() {
        let start = offset + component * 4;
        let encoded: [u8; 4] = binary
            .get(start..start + 4)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| {
                diagnostic(
                    AssetDiagnosticCode::InvalidBufferRange,
                    "glb.binary.normals",
                    Some(index),
                )
            })?;
        *target = FiniteF32::new(f32::from_le_bytes(encoded)).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidNormal,
                "glb.binary.normals",
                Some(index),
            )
        })?;
    }
    normalize_vector(normal, "glb.binary.normals", Some(index))
}

fn read_tangent(
    binary: &[u8],
    layout: AccessorLayout,
    index: u32,
) -> Result<[FiniteF32; 4], AssetDiagnostic> {
    let offset = element_offset(layout, index, "glb.binary.tangents")?;
    let mut tangent = [FiniteF32::new(0.0).expect("zero is finite"); 4];
    for (component, target) in tangent.iter_mut().enumerate() {
        let start = offset + component * 4;
        let encoded: [u8; 4] = binary
            .get(start..start + 4)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| {
                diagnostic(
                    AssetDiagnosticCode::InvalidBufferRange,
                    "glb.binary.tangents",
                    Some(index),
                )
            })?;
        *target = FiniteF32::new(f32::from_le_bytes(encoded)).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidTangent,
                "glb.binary.tangents",
                Some(index),
            )
        })?;
    }
    if !matches!(tangent[3].get(), -1.0 | 1.0) {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidTangent,
            "glb.binary.tangents.handedness",
            Some(index),
        ));
    }
    let xyz = normalize_tangent_vector(
        [tangent[0], tangent[1], tangent[2]],
        "glb.binary.tangents",
        Some(index),
    )?;
    Ok([xyz[0], xyz[1], xyz[2], tangent[3]])
}

fn read_texcoord(
    binary: &[u8],
    layout: AccessorLayout,
    index: u32,
) -> Result<[FiniteF32; 2], AssetDiagnostic> {
    let offset = element_offset(layout, index, "glb.binary.texcoord_0")?;
    let mut texcoord = [FiniteF32::new(0.0).expect("zero is finite"); 2];
    for (component, target) in texcoord.iter_mut().enumerate() {
        let start = offset + component * 4;
        let encoded: [u8; 4] = binary
            .get(start..start + 4)
            .and_then(|value| value.try_into().ok())
            .ok_or_else(|| {
                diagnostic(
                    AssetDiagnosticCode::InvalidBufferRange,
                    "glb.binary.texcoord_0",
                    Some(index),
                )
            })?;
        *target = FiniteF32::new(f32::from_le_bytes(encoded)).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidTexcoord,
                "glb.binary.texcoord_0",
                Some(index),
            )
        })?;
    }
    Ok(texcoord)
}

fn face_normal(
    first: [FiniteF32; 3],
    second: [FiniteF32; 3],
    third: [FiniteF32; 3],
) -> Result<[FiniteF32; 3], AssetDiagnostic> {
    let first = first.map(|value| f64::from(value.get()));
    let second = second.map(|value| f64::from(value.get()));
    let third = third.map(|value| f64::from(value.get()));
    let edge_a = [
        second[0] - first[0],
        second[1] - first[1],
        second[2] - first[2],
    ];
    let edge_b = [
        third[0] - first[0],
        third[1] - first[1],
        third[2] - first[2],
    ];
    normalize_f64_vector(
        [
            edge_a[1] * edge_b[2] - edge_a[2] * edge_b[1],
            edge_a[2] * edge_b[0] - edge_a[0] * edge_b[2],
            edge_a[0] * edge_b[1] - edge_a[1] * edge_b[0],
        ],
        "glb.decoded.triangle_normal",
        None,
    )
}

fn normalize_vector(
    vector: [FiniteF32; 3],
    location: &'static str,
    index: Option<u32>,
) -> Result<[FiniteF32; 3], AssetDiagnostic> {
    normalize_f64_vector(vector.map(|value| f64::from(value.get())), location, index)
}

#[allow(clippy::cast_possible_truncation)]
fn normalize_tangent_vector(
    vector: [FiniteF32; 3],
    location: &'static str,
    index: Option<u32>,
) -> Result<[FiniteF32; 3], AssetDiagnostic> {
    let vector = vector.map(|value| f64::from(value.get()));
    let length_squared = vector.iter().map(|value| value * value).sum::<f64>();
    if !length_squared.is_finite() || length_squared == 0.0 {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidTangent,
            location,
            index,
        ));
    }
    let inverse_length = length_squared.sqrt().recip();
    let mut normalized = [FiniteF32::new(0.0).expect("zero is finite"); 3];
    for (target, value) in normalized.iter_mut().zip(vector) {
        *target = FiniteF32::new((value * inverse_length) as f32)
            .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidTangent, location, index))?;
    }
    Ok(normalized)
}

#[allow(clippy::cast_possible_truncation)]
fn normalize_f64_vector(
    vector: [f64; 3],
    location: &'static str,
    index: Option<u32>,
) -> Result<[FiniteF32; 3], AssetDiagnostic> {
    let length_squared = vector.iter().map(|value| value * value).sum::<f64>();
    if !length_squared.is_finite() || length_squared == 0.0 {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidNormal,
            location,
            index,
        ));
    }
    let inverse_length = length_squared.sqrt().recip();
    let mut normalized = [FiniteF32::new(0.0).expect("zero is finite"); 3];
    for (target, value) in normalized.iter_mut().zip(vector) {
        *target = FiniteF32::new((value * inverse_length) as f32)
            .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidNormal, location, index))?;
    }
    Ok(normalized)
}

fn element_offset(
    layout: AccessorLayout,
    index: u32,
    location: &'static str,
) -> Result<usize, AssetDiagnostic> {
    usize::try_from(index)
        .ok()
        .and_then(|index| index.checked_mul(layout.stride))
        .and_then(|relative| layout.start.checked_add(relative))
        .ok_or_else(|| {
            diagnostic(
                AssetDiagnosticCode::InvalidBufferRange,
                location,
                Some(index),
            )
        })
}

fn decode_material(
    root: &Root,
    material_index: Option<u32>,
) -> Result<AssetMaterial, AssetDiagnostic> {
    let (color_values, metallic_value, roughness_value, emissive_values) =
        if let Some(index) = material_index {
            let material = get(&root.materials, index, "glb.json.materials")?;
            let pbr = material.pbr_metallic_roughness.as_ref();
            (
                pbr.and_then(|value| value.base_color).unwrap_or([1.0; 4]),
                pbr.and_then(|value| value.metallic).unwrap_or(1.0),
                pbr.and_then(|value| value.roughness).unwrap_or(1.0),
                material.emissive.unwrap_or([0.0; 3]),
            )
        } else {
            ([0.8, 0.8, 0.8, 1.0], 0.0, 0.8, [0.0; 3])
        };
    let mut color = [UnitF32::new(0.0).expect("zero is in range"); 4];
    for (target, value) in color.iter_mut().zip(color_values) {
        *target = UnitF32::new(value).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.baseColorFactor",
                material_index,
            )
        })?;
    }
    let material_scalar = |value| {
        UnitF32::new(value).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.pbrMetallicRoughness",
                material_index,
            )
        })
    };
    let mut emissive = [UnitF32::new(0.0).expect("zero is in range"); 3];
    for (target, value) in emissive.iter_mut().zip(emissive_values) {
        *target = UnitF32::new(value).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.emissiveFactor",
                material_index,
            )
        })?;
    }
    let material = AssetMaterial::new(
        color,
        material_scalar(metallic_value)?,
        material_scalar(roughness_value)?,
    )
    .with_emissive(emissive);
    let material = apply_double_sided(root, material_index, material);
    let material = apply_alpha_coverage(root, material_index, material)?;
    let has_base_color_texture = material_index.is_some_and(|index| {
        root.materials
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .and_then(|material| material.pbr_metallic_roughness.as_ref())
            .and_then(|pbr| pbr.base_color_texture.as_ref())
            .is_some()
    });
    let mut material = if has_base_color_texture {
        material.with_base_color_texture()
    } else {
        material
    };
    if material_index.is_some_and(|index| {
        root.materials
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .and_then(|material| material.emissive_texture.as_ref())
            .is_some()
    }) {
        material = material.with_emissive_texture();
    }
    if material_index.is_some_and(|index| {
        root.materials
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .and_then(|material| material.pbr_metallic_roughness.as_ref())
            .and_then(|pbr| pbr.metallic_roughness_texture.as_ref())
            .is_some()
    }) {
        material = material.with_metallic_roughness_texture();
    }
    if let Some(normal_texture) = material_index
        .and_then(|index| {
            root.materials
                .get(usize::try_from(index).unwrap_or(usize::MAX))
        })
        .and_then(|material| material.normal_texture.as_ref())
    {
        let scale = FiniteF32::new(normal_texture.scale.unwrap_or(1.0)).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.normalTexture.scale",
                material_index,
            )
        })?;
        material = material.with_normal_texture(scale);
    }
    Ok(material)
}

fn apply_double_sided(
    root: &Root,
    material_index: Option<u32>,
    material: AssetMaterial,
) -> AssetMaterial {
    let enabled = material_index
        .and_then(|index| {
            root.materials
                .get(usize::try_from(index).unwrap_or(usize::MAX))
        })
        .and_then(|material| material.double_sided)
        .unwrap_or(false);
    if enabled {
        material.with_double_sided()
    } else {
        material
    }
}

fn apply_alpha_coverage(
    root: &Root,
    material_index: Option<u32>,
    material: AssetMaterial,
) -> Result<AssetMaterial, AssetDiagnostic> {
    let Some(source) = material_index
        .and_then(|index| {
            root.materials
                .get(usize::try_from(index).unwrap_or(usize::MAX))
        })
        .filter(|material| material.alpha_mode.as_deref() == Some("MASK"))
    else {
        return Ok(material);
    };
    let cutoff = FiniteF32::new(source.alpha_cutoff.unwrap_or(0.5)).map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.materials.alphaCutoff",
            material_index,
        )
    })?;
    Ok(material.with_alpha_mask(cutoff))
}

pub(crate) fn proxy_asset() -> DecodedAsset {
    const POSITIONS: [[f32; 3]; 36] = [
        [-0.5, -0.5, -0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, -0.5, -0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [0.5, -0.5, 0.5],
        [-0.5, -0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [0.5, -0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [0.5, 0.5, 0.5],
        [-0.5, -0.5, 0.5],
        [-0.5, -0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [-0.5, 0.5, -0.5],
        [-0.5, 0.5, 0.5],
        [0.5, -0.5, -0.5],
        [0.5, -0.5, 0.5],
        [0.5, 0.5, 0.5],
        [0.5, -0.5, -0.5],
        [0.5, 0.5, 0.5],
        [0.5, 0.5, -0.5],
        [-0.5, 0.5, -0.5],
        [0.5, 0.5, -0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, -0.5],
        [0.5, 0.5, 0.5],
        [-0.5, 0.5, 0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, 0.5],
        [0.5, -0.5, -0.5],
        [-0.5, -0.5, 0.5],
        [0.5, -0.5, -0.5],
        [-0.5, -0.5, -0.5],
    ];
    let vertices: Vec<_> = POSITIONS
        .chunks_exact(3)
        .flat_map(|triangle| {
            let positions = [triangle[0], triangle[1], triangle[2]].map(|position| {
                position.map(|value| FiniteF32::new(value).expect("proxy is finite"))
            });
            let normal = face_normal(positions[0], positions[1], positions[2])
                .expect("proxy triangles are non-degenerate");
            positions.into_iter().map(move |position| AssetVertex {
                position,
                normal,
                tangent: [
                    FiniteF32::new(1.0).expect("one is finite"),
                    FiniteF32::new(0.0).expect("zero is finite"),
                    FiniteF32::new(0.0).expect("zero is finite"),
                    FiniteF32::new(1.0).expect("one is finite"),
                ],
                texcoord_0: [FiniteF32::new(0.0).expect("zero is finite"); 2],
            })
        })
        .collect();
    DecodedAsset {
        meshes: vec![DecodedMesh {
            vertices: Arc::from(vertices),
            material: AssetMaterial::new(
                [
                    UnitF32::new(1.0).expect("constant is in range"),
                    UnitF32::new(0.0).expect("constant is in range"),
                    UnitF32::new(1.0).expect("constant is in range"),
                    UnitF32::new(1.0).expect("constant is in range"),
                ],
                UnitF32::new(0.0).expect("constant is in range"),
                UnitF32::new(0.8).expect("constant is in range"),
            ),
        }],
        base_color_texture: None,
        emissive_texture: None,
        metallic_roughness_texture: None,
        normal_texture: None,
        byte_len: 36 * ASSET_VERTEX_BYTES,
    }
}

fn get<'a, T>(
    values: &'a [T],
    index: u32,
    location: &'static str,
) -> Result<&'a T, AssetDiagnostic> {
    values
        .get(usize::try_from(index).unwrap_or(usize::MAX))
        .ok_or_else(|| {
            diagnostic(
                AssetDiagnosticCode::InvalidBufferRange,
                location,
                Some(index),
            )
        })
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn stable_index(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

const fn diagnostic(
    code: AssetDiagnosticCode,
    location: &'static str,
    index: Option<u32>,
) -> AssetDiagnostic {
    AssetDiagnostic::new(code, location, index)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Root {
    asset: AssetHeader,
    buffers: Vec<Buffer>,
    buffer_views: Vec<BufferView>,
    accessors: Vec<Accessor>,
    meshes: Vec<Mesh>,
    #[serde(default)]
    materials: Vec<Material>,
    #[serde(default)]
    images: Vec<Image>,
    #[serde(default)]
    samplers: Vec<serde_json::Value>,
    #[serde(default)]
    textures: Vec<Texture>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct AssetHeader {
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Buffer {
    byte_length: u64,
    #[serde(default)]
    uri: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct BufferView {
    buffer: u32,
    #[serde(default)]
    byte_offset: u64,
    byte_length: u64,
    #[serde(default)]
    byte_stride: Option<u32>,
    #[serde(default)]
    #[serde(rename = "target")]
    _target: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Accessor {
    buffer_view: u32,
    #[serde(default)]
    byte_offset: u64,
    component_type: u32,
    count: u32,
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    normalized: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Mesh {
    primitives: Vec<Primitive>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Primitive {
    attributes: BTreeMap<String, u32>,
    #[serde(default)]
    indices: Option<u32>,
    #[serde(default)]
    material: Option<u32>,
    #[serde(default)]
    mode: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Material {
    #[serde(default)]
    pbr_metallic_roughness: Option<PbrMetallicRoughness>,
    #[serde(default)]
    normal_texture: Option<NormalTextureInfo>,
    #[serde(
        rename = "emissiveTexture",
        default,
        deserialize_with = "deserialize_texture_info"
    )]
    emissive_texture: Option<TextureInfo>,
    #[serde(
        rename = "emissiveFactor",
        default,
        deserialize_with = "deserialize_emissive_factor"
    )]
    emissive: Option<[f32; 3]>,
    #[serde(
        rename = "alphaMode",
        default,
        deserialize_with = "deserialize_alpha_mode"
    )]
    alpha_mode: Option<String>,
    #[serde(
        rename = "alphaCutoff",
        default,
        deserialize_with = "deserialize_alpha_cutoff"
    )]
    alpha_cutoff: Option<f32>,
    #[serde(
        rename = "doubleSided",
        default,
        deserialize_with = "deserialize_double_sided"
    )]
    double_sided: Option<bool>,
}

fn deserialize_double_sided<'de, D>(deserializer: D) -> Result<Option<bool>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    bool::deserialize(deserializer).map(Some)
}

fn deserialize_alpha_mode<'de, D>(deserializer: D) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    String::deserialize(deserializer).map(Some)
}

fn deserialize_alpha_cutoff<'de, D>(deserializer: D) -> Result<Option<f32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    f32::deserialize(deserializer).map(Some)
}

fn deserialize_emissive_factor<'de, D>(deserializer: D) -> Result<Option<[f32; 3]>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    <[f32; 3]>::deserialize(deserializer).map(Some)
}

fn deserialize_texture_info<'de, D>(deserializer: D) -> Result<Option<TextureInfo>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    TextureInfo::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct PbrMetallicRoughness {
    #[serde(rename = "baseColorFactor", default)]
    base_color: Option<[f32; 4]>,
    #[serde(rename = "metallicFactor", default)]
    metallic: Option<f32>,
    #[serde(rename = "roughnessFactor", default)]
    roughness: Option<f32>,
    #[serde(rename = "baseColorTexture", default)]
    base_color_texture: Option<TextureInfo>,
    #[serde(rename = "metallicRoughnessTexture", default)]
    metallic_roughness_texture: Option<TextureInfo>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TextureInfo {
    index: u32,
    #[serde(default)]
    tex_coord: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct NormalTextureInfo {
    index: u32,
    #[serde(default)]
    tex_coord: Option<u32>,
    #[serde(default)]
    scale: Option<f32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Texture {
    #[serde(default)]
    sampler: Option<u32>,
    #[serde(default)]
    source: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Image {
    #[serde(default)]
    buffer_view: Option<u32>,
    #[serde(default)]
    mime_type: Option<String>,
    #[serde(default)]
    uri: Option<String>,
}
