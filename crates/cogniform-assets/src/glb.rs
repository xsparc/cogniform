use core::num::NonZeroU32;
use std::{
    collections::{BTreeMap, BTreeSet},
    io::Cursor,
    sync::Arc,
};

use bevy_mikktspace::{Geometry, TangentSpace, generate_tangents};
use cogniform_protocol::{FiniteF32, UnitF32};
use serde::Deserialize;

use crate::types::{
    ASSET_VERTEX_BYTES, AssetDiagnostic, AssetDiagnosticCode, AssetLimits, AssetMaterial,
    AssetSampler, AssetSamplerFilter, AssetSamplerMinFilter, AssetSamplerWrap, AssetTexture,
    AssetTextureTransform, AssetVertex, DecodedAsset, DecodedMesh,
};

const GLB_MAGIC: [u8; 4] = *b"glTF";
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4e4f_534a;
const BIN_CHUNK: u32 = 0x004e_4942;
const FLOAT: u32 = 5_126;
const UNSIGNED_BYTE: u32 = 5_121;
const UNSIGNED_SHORT: u32 = 5_123;
const UNSIGNED_INT: u32 = 5_125;
const TRIANGLES: u32 = 4;
const MAX_PRIMITIVE_ATTRIBUTES: usize = 16;
const PNG_SIGNATURE: [u8; 8] = [137, 80, 78, 71, 13, 10, 26, 10];
const PNG_IHDR_LENGTH: u32 = 13;
const PNG_IEND: [u8; 12] = [0, 0, 0, 0, b'I', b'E', b'N', b'D', 0xae, 0x42, 0x60, 0x82];
const PNG_RGB: u8 = 2;
const PNG_RGBA: u8 = 6;
const KHR_MATERIALS_UNLIT: &str = "KHR_materials_unlit";
const KHR_TEXTURE_TRANSFORM: &str = "KHR_texture_transform";
const GENERATED_TANGENT_GROUP_WORK_LIMIT: u64 = 268_435_456;
const GENERATED_TANGENT_DEGENERATE_SEARCH_LIMIT: u64 = 16_777_216;

struct ExtensionPreflight {
    unsupported: Option<AssetDiagnostic>,
    unlit_materials: Vec<bool>,
    texture_transforms: Vec<MaterialTextureTransforms>,
}

#[derive(Clone, Copy, Default)]
struct MaterialTextureTransforms {
    base_color: Option<AssetTextureTransform>,
    emissive: Option<AssetTextureTransform>,
    metallic_roughness: Option<AssetTextureTransform>,
    normal: Option<AssetTextureTransform>,
}

pub(crate) fn decode_glb(
    bytes: &[u8],
    limits: AssetLimits,
) -> Result<DecodedAsset, AssetDiagnostic> {
    let (json, binary) = split_glb(bytes, limits)?;
    let mut value: serde_json::Value = serde_json::from_slice(json)
        .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, "glb.json", None))?;
    let extensions = remove_declared_extensions_and_features(&mut value)?;
    let root: Root = serde_json::from_value(value)
        .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, "glb.json.schema", None))?;
    let decoded = validate_root(&root, binary, limits, &extensions)?;
    extensions.unsupported.map_or(Ok(decoded), Err)
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
) -> Result<ExtensionPreflight, AssetDiagnostic> {
    let Some(root) = value.as_object_mut() else {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.root",
            None,
        ));
    };
    let used = remove_extension_names(root, "extensionsUsed")?.unwrap_or_default();
    let required = remove_extension_names(root, "extensionsRequired")?.unwrap_or_default();
    if !required.is_subset(&used) {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.extensionsRequired",
            None,
        ));
    }
    let mut unsupported = used
        .iter()
        .any(|name| !matches!(name.as_str(), KHR_MATERIALS_UNLIT | KHR_TEXTURE_TRANSFORM))
        .then(|| {
            diagnostic(
                AssetDiagnosticCode::UnsupportedExtension,
                "glb.json.extensions",
                None,
            )
        });
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
    let unlit_materials = remove_material_unlit_extensions(value, &used, &mut unsupported)?;
    let texture_transforms =
        remove_material_texture_transform_extensions(value, &used, &mut unsupported)?;
    remove_nested_extensions(value, &used, &mut unsupported)?;
    remove_unsupported_material_features(value, &mut unsupported)?;
    Ok(ExtensionPreflight {
        unsupported,
        unlit_materials,
        texture_transforms,
    })
}

fn remove_extension_names(
    root: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
) -> Result<Option<BTreeSet<String>>, AssetDiagnostic> {
    let Some(value) = root.remove(field) else {
        return Ok(None);
    };
    let Some(values) = value.as_array().filter(|values| !values.is_empty()) else {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.extensions",
            None,
        ));
    };
    let mut names = BTreeSet::new();
    for value in values {
        let Some(name) = value.as_str().filter(|name| !name.is_empty()) else {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.extensions",
                None,
            ));
        };
        if !names.insert(name.to_owned()) {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.extensions",
                None,
            ));
        }
    }
    Ok(Some(names))
}

fn remove_material_unlit_extensions(
    value: &mut serde_json::Value,
    used: &BTreeSet<String>,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<Vec<bool>, AssetDiagnostic> {
    let Some(materials) = value
        .as_object_mut()
        .and_then(|root| root.get_mut("materials"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(Vec::new());
    };
    let mut unlit_materials = Vec::with_capacity(materials.len());
    for material in materials {
        let Some(material) = material.as_object_mut() else {
            unlit_materials.push(false);
            continue;
        };
        let Some(mut extensions) = material.remove("extensions") else {
            unlit_materials.push(false);
            continue;
        };
        let Some(extensions) = extensions.as_object_mut() else {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.extensions",
                None,
            ));
        };
        let mut unlit = false;
        if let Some(marker) = extensions.remove(KHR_MATERIALS_UNLIT) {
            if !used.contains(KHR_MATERIALS_UNLIT) {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.materials.extensions.KHR_materials_unlit",
                    None,
                ));
            }
            let Some(marker) = marker.as_object() else {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.materials.extensions.KHR_materials_unlit",
                    None,
                ));
            };
            if marker.is_empty() {
                unlit = true;
            } else {
                unsupported.get_or_insert_with(|| {
                    diagnostic(
                        AssetDiagnosticCode::UnsupportedExtension,
                        "glb.json.materials.extensions.KHR_materials_unlit",
                        None,
                    )
                });
            }
        }
        if !extensions.is_empty() {
            material.insert(
                "extensions".to_owned(),
                serde_json::Value::Object(core::mem::take(extensions)),
            );
        }
        unlit_materials.push(unlit);
    }
    Ok(unlit_materials)
}

fn remove_material_texture_transform_extensions(
    value: &mut serde_json::Value,
    used: &BTreeSet<String>,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<Vec<MaterialTextureTransforms>, AssetDiagnostic> {
    let Some(materials) = value
        .as_object_mut()
        .and_then(|root| root.get_mut("materials"))
        .and_then(serde_json::Value::as_array_mut)
    else {
        return Ok(Vec::new());
    };
    let mut retained = Vec::with_capacity(materials.len());
    for material in materials {
        let Some(material) = material.as_object_mut() else {
            retained.push(MaterialTextureTransforms::default());
            continue;
        };
        let mut transforms = MaterialTextureTransforms::default();
        if let Some(pbr) = material
            .get_mut("pbrMetallicRoughness")
            .and_then(serde_json::Value::as_object_mut)
        {
            transforms.base_color = remove_texture_transform_from_info(
                pbr.get_mut("baseColorTexture"),
                used,
                unsupported,
                "glb.json.materials.baseColorTexture.extensions.KHR_texture_transform",
            )?;
            transforms.metallic_roughness = remove_texture_transform_from_info(
                pbr.get_mut("metallicRoughnessTexture"),
                used,
                unsupported,
                "glb.json.materials.metallicRoughnessTexture.extensions.KHR_texture_transform",
            )?;
        }
        transforms.normal = remove_texture_transform_from_info(
            material.get_mut("normalTexture"),
            used,
            unsupported,
            "glb.json.materials.normalTexture.extensions.KHR_texture_transform",
        )?;
        transforms.emissive = remove_texture_transform_from_info(
            material.get_mut("emissiveTexture"),
            used,
            unsupported,
            "glb.json.materials.emissiveTexture.extensions.KHR_texture_transform",
        )?;
        retained.push(transforms);
    }
    Ok(retained)
}

fn remove_texture_transform_from_info(
    texture_info: Option<&mut serde_json::Value>,
    used: &BTreeSet<String>,
    unsupported: &mut Option<AssetDiagnostic>,
    location: &'static str,
) -> Result<Option<AssetTextureTransform>, AssetDiagnostic> {
    let Some(texture_info) = texture_info.and_then(serde_json::Value::as_object_mut) else {
        return Ok(None);
    };
    let Some(mut extensions) = texture_info.remove("extensions") else {
        return Ok(None);
    };
    let Some(extensions) = extensions.as_object_mut() else {
        return Err(diagnostic(AssetDiagnosticCode::InvalidJson, location, None));
    };
    let transform = extensions
        .remove(KHR_TEXTURE_TRANSFORM)
        .map(|payload| {
            if !used.contains(KHR_TEXTURE_TRANSFORM) {
                return Err(diagnostic(AssetDiagnosticCode::InvalidJson, location, None));
            }
            decode_texture_transform(payload, used, unsupported, location)
        })
        .transpose()?
        .flatten();
    if !extensions.is_empty() {
        texture_info.insert(
            "extensions".to_owned(),
            serde_json::Value::Object(core::mem::take(extensions)),
        );
    }
    Ok(transform)
}

fn decode_texture_transform(
    mut payload: serde_json::Value,
    used: &BTreeSet<String>,
    unsupported: &mut Option<AssetDiagnostic>,
    location: &'static str,
) -> Result<Option<AssetTextureTransform>, AssetDiagnostic> {
    let Some(payload) = payload.as_object_mut() else {
        return Err(diagnostic(AssetDiagnosticCode::InvalidJson, location, None));
    };
    let offset = remove_finite_pair(payload, "offset", [0.0, 0.0], location)?;
    let scale = remove_finite_pair(payload, "scale", [1.0, 1.0], location)?;
    let rotation = remove_finite_scalar(payload, "rotation", 0.0, location)?;
    let tex_coord = payload
        .remove("texCoord")
        .map(|value| {
            serde_json::from_value::<u32>(value)
                .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, location, None))
        })
        .transpose()?;
    let mut wider = tex_coord.is_some_and(|value| value != 0);
    if !payload.is_empty() {
        let mut remainder = serde_json::Value::Object(core::mem::take(payload));
        remove_nested_extensions(&mut remainder, used, unsupported)?;
        wider = true;
    }
    if wider {
        unsupported.get_or_insert_with(|| {
            diagnostic(AssetDiagnosticCode::UnsupportedExtension, location, None)
        });
        Ok(None)
    } else {
        Ok(Some(AssetTextureTransform::new(offset, rotation, scale)))
    }
}

fn remove_finite_pair(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: [f32; 2],
    location: &'static str,
) -> Result<[FiniteF32; 2], AssetDiagnostic> {
    let values = object
        .remove(field)
        .map(|value| {
            serde_json::from_value::<[f32; 2]>(value)
                .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, location, None))
        })
        .transpose()?
        .unwrap_or(default);
    let mut finite = [FiniteF32::new(0.0).expect("zero is finite"); 2];
    for (target, value) in finite.iter_mut().zip(values) {
        *target = FiniteF32::new(value)
            .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, location, None))?;
    }
    Ok(finite)
}

fn remove_finite_scalar(
    object: &mut serde_json::Map<String, serde_json::Value>,
    field: &str,
    default: f32,
    location: &'static str,
) -> Result<FiniteF32, AssetDiagnostic> {
    let value = object
        .remove(field)
        .map(|value| {
            serde_json::from_value::<f32>(value)
                .map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, location, None))
        })
        .transpose()?
        .unwrap_or(default);
    FiniteF32::new(value).map_err(|_| diagnostic(AssetDiagnosticCode::InvalidJson, location, None))
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
    used: &BTreeSet<String>,
    unsupported: &mut Option<AssetDiagnostic>,
) -> Result<(), AssetDiagnostic> {
    match value {
        serde_json::Value::Object(object) => {
            if let Some(extensions) = object.remove("extensions") {
                let Some(extensions) = extensions.as_object() else {
                    return Err(diagnostic(
                        AssetDiagnosticCode::InvalidJson,
                        "glb.json.extensions",
                        None,
                    ));
                };
                if extensions
                    .iter()
                    .any(|(name, payload)| !used.contains(name) || !payload.is_object())
                {
                    return Err(diagnostic(
                        AssetDiagnosticCode::InvalidJson,
                        "glb.json.extensions",
                        None,
                    ));
                }
                unsupported.get_or_insert_with(|| {
                    diagnostic(
                        AssetDiagnosticCode::UnsupportedExtension,
                        "glb.json.extensions",
                        None,
                    )
                });
            }
            for nested in object.values_mut() {
                remove_nested_extensions(nested, used, unsupported)?;
            }
        }
        serde_json::Value::Array(values) => {
            for nested in values {
                remove_nested_extensions(nested, used, unsupported)?;
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
    extensions: &ExtensionPreflight,
) -> Result<DecodedAsset, AssetDiagnostic> {
    validate_sampler_resources(root)?;
    validate_root_header(root, binary, limits)?;
    validate_material_values(root)?;
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
            extensions,
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

fn validate_material_values(root: &Root) -> Result<(), AssetDiagnostic> {
    for (material_index, material) in root.materials.iter().enumerate() {
        let index = Some(stable_index(material_index));
        if let Some(pbr) = material.pbr_metallic_roughness.as_ref() {
            if let Some(values) = pbr.base_color {
                for value in values {
                    UnitF32::new(value).map_err(|_| {
                        diagnostic(
                            AssetDiagnosticCode::InvalidJson,
                            "glb.json.materials.baseColorFactor",
                            index,
                        )
                    })?;
                }
            }
            for value in [pbr.metallic, pbr.roughness].into_iter().flatten() {
                UnitF32::new(value).map_err(|_| {
                    diagnostic(
                        AssetDiagnosticCode::InvalidJson,
                        "glb.json.materials.pbrMetallicRoughness",
                        index,
                    )
                })?;
            }
        }
        if let Some(scale) = material
            .normal_texture
            .as_ref()
            .and_then(|texture| texture.scale)
        {
            FiniteF32::new(scale).map_err(|_| {
                diagnostic(
                    AssetDiagnosticCode::InvalidJson,
                    "glb.json.materials.normalTexture.scale",
                    index,
                )
            })?;
        }
        if let Some(values) = material.emissive {
            for value in values {
                UnitF32::new(value).map_err(|_| {
                    diagnostic(
                        AssetDiagnosticCode::InvalidJson,
                        "glb.json.materials.emissiveFactor",
                        index,
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
    for (actual, location) in [
        (root.images.len(), "glb.json.images"),
        (root.samplers.len(), "glb.json.samplers"),
        (root.textures.len(), "glb.json.textures"),
    ] {
        if actual > 4 {
            return Err(diagnostic(
                AssetDiagnosticCode::CollectionLimitExceeded,
                location,
                None,
            ));
        }
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
    let texture_sources = validate_texture_sources(root)?;
    let referenced_samplers = root
        .textures
        .iter()
        .filter_map(|texture| texture.sampler)
        .collect::<BTreeSet<_>>();
    if referenced_samplers.len() != root.samplers.len() {
        remember_unsupported(
            &mut unsupported,
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.samplers",
            None,
        );
    }
    let (decoded_images, byte_len) = validate_root_images(root, binary, limits, &mut unsupported)?;
    Ok(ValidatedTextureResources {
        texture_sources,
        decoded_images,
        byte_len,
        unsupported,
    })
}

fn validate_root_samplers(root: &Root) -> Result<(), AssetDiagnostic> {
    for (sampler_index, sampler) in root.samplers.iter().enumerate() {
        decode_sampler(stable_index(sampler_index), sampler)?;
    }
    Ok(())
}

fn validate_sampler_resources(root: &Root) -> Result<(), AssetDiagnostic> {
    if root.samplers.len() > 4 {
        return Err(diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            "glb.json.samplers",
            None,
        ));
    }
    validate_root_samplers(root)?;
    for (texture_index, texture) in root.textures.iter().enumerate() {
        if let Some(sampler_index) = texture.sampler {
            get(&root.samplers, sampler_index, "glb.json.textures[].sampler").map_err(
                |mut error| {
                    error.index = Some(stable_index(texture_index));
                    error
                },
            )?;
        }
    }
    Ok(())
}

fn decode_sampler(index: u32, sampler: &Sampler) -> Result<AssetSampler, AssetDiagnostic> {
    let index = Some(index);
    let mag_filter = match sampler.mag_filter.unwrap_or(9_729) {
        9_728 => AssetSamplerFilter::Nearest,
        9_729 => AssetSamplerFilter::Linear,
        _ => {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.samplers[].magFilter",
                index,
            ));
        }
    };
    let min_filter = match sampler.min_filter.unwrap_or(9_729) {
        9_728 => AssetSamplerMinFilter::Nearest,
        9_729 => AssetSamplerMinFilter::Linear,
        9_984 => AssetSamplerMinFilter::NearestMipmapNearest,
        9_985 => AssetSamplerMinFilter::LinearMipmapNearest,
        9_986 => AssetSamplerMinFilter::NearestMipmapLinear,
        9_987 => AssetSamplerMinFilter::LinearMipmapLinear,
        _ => {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.samplers[].minFilter",
                index,
            ));
        }
    };
    let decode_wrap = |value: Option<u32>, location| match value.unwrap_or(10_497) {
        33_071 => Ok(AssetSamplerWrap::ClampToEdge),
        33_648 => Ok(AssetSamplerWrap::MirroredRepeat),
        10_497 => Ok(AssetSamplerWrap::Repeat),
        _ => Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            location,
            index,
        )),
    };
    Ok(AssetSampler::new(
        mag_filter,
        min_filter,
        decode_wrap(sampler.wrap_s, "glb.json.samplers[].wrapS")?,
        decode_wrap(sampler.wrap_t, "glb.json.samplers[].wrapT")?,
    ))
}

fn texture_sampler(root: &Root, texture_index: u32) -> Result<AssetSampler, AssetDiagnostic> {
    let texture = get(
        &root.textures,
        texture_index,
        "glb.json.materials.texture.index",
    )?;
    let Some(sampler_index) = texture.sampler else {
        return Ok(AssetSampler::LINEAR_REPEAT);
    };
    let sampler = get(&root.samplers, sampler_index, "glb.json.textures[].sampler")?;
    decode_sampler(sampler_index, sampler)
}

fn validate_texture_sources(root: &Root) -> Result<BTreeMap<u32, u32>, AssetDiagnostic> {
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
        if let Some(sampler) = texture.sampler {
            get(&root.samplers, sampler, "glb.json.textures[].sampler")?;
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
    extensions: &ExtensionPreflight,
) -> Result<DecodedMesh, AssetDiagnostic> {
    let primitive = validated_primitive(mesh, limits, mesh_index)?;
    let layouts = vertex_layouts(root, binary, primitive)?;
    if primitive.mode.unwrap_or(TRIANGLES) != TRIANGLES {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedPrimitiveMode,
            "glb.json.meshes[].primitives[].mode",
            Some(mesh_index),
        ));
    }
    let (indices, output_count) = output_layout(
        root,
        binary,
        primitive,
        layouts.positions,
        layouts.position_index,
        limits,
    )?;
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
    if let Some(texcoords) = layouts.texcoords {
        validate_texcoords(binary, texcoords)?;
    }
    if let Some(tangents) = layouts.tangents {
        validate_tangents(binary, tangents)?;
    }
    let mut vertices = decode_vertices(binary, &layouts, indices, output_count)?;
    let material = decode_material(root, primitive.material, extensions)?;
    if (material.has_base_color_texture()
        || material.has_emissive_texture()
        || material.has_metallic_roughness_texture()
        || material.has_normal_texture())
        && layouts.texcoords.is_none()
    {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidTexcoord,
            "glb.json.meshes[].primitives[].attributes.TEXCOORD_0",
            Some(mesh_index),
        ));
    }
    validate_transformed_texture_coordinates(&vertices, material, mesh_index)?;
    if layouts.has_unsupported_attributes {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.meshes[].primitives[].attributes",
            Some(mesh_index),
        ));
    }
    if material.has_normal_texture() && (layouts.normals.is_none() || layouts.tangents.is_none()) {
        generate_missing_tangents(
            &mut vertices,
            material
                .normal_texture_transform()
                .expect("normal texture retains a transform"),
            mesh_index,
        )?;
    }
    Ok(DecodedMesh {
        vertices: Arc::from(vertices),
        material,
    })
}

fn validate_transformed_texture_coordinates(
    vertices: &[AssetVertex],
    material: AssetMaterial,
    mesh_index: u32,
) -> Result<(), AssetDiagnostic> {
    let transforms = [
        material.base_color_texture_transform(),
        material.emissive_texture_transform(),
        material.metallic_roughness_texture_transform(),
        material.normal_texture_transform(),
    ];
    for transform in transforms.into_iter().flatten() {
        for vertex in vertices {
            let coordinate = vertex.texcoord_0.map(FiniteF32::get);
            if transform.transform(coordinate).is_none() {
                return Err(diagnostic(
                    AssetDiagnosticCode::InvalidTexcoord,
                    "glb.decoded.texture_transform",
                    Some(mesh_index),
                ));
            }
        }
    }
    Ok(())
}

fn validated_primitive(
    mesh: &Mesh,
    limits: AssetLimits,
    mesh_index: u32,
) -> Result<&Primitive, AssetDiagnostic> {
    if mesh.primitives.is_empty() {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.meshes[].primitives",
            Some(mesh_index),
        ));
    }
    if mesh.primitives.len() != 1
        || count(mesh.primitives.len()) > limits.max_primitives_per_mesh.get()
    {
        return Err(diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            "glb.json.meshes[].primitives",
            Some(mesh_index),
        ));
    }
    let primitive = &mesh.primitives[0];
    if primitive.attributes.len() > MAX_PRIMITIVE_ATTRIBUTES {
        return Err(diagnostic(
            AssetDiagnosticCode::CollectionLimitExceeded,
            "glb.json.meshes[].primitives[].attributes",
            Some(mesh_index),
        ));
    }
    if !primitive.attributes.contains_key("POSITION") {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidJson,
            "glb.json.meshes[].primitives[].attributes.POSITION",
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
    colors: Option<AccessorLayout>,
    has_unsupported_attributes: bool,
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
    let (colors, has_wider_colors) = color_layouts(root, binary, primitive, positions)?;
    let has_other_unsupported_attributes = primitive.attributes.keys().any(|attribute| {
        !matches!(
            attribute.as_str(),
            "POSITION" | "NORMAL" | "TANGENT" | "TEXCOORD_0"
        ) && !attribute.starts_with("COLOR_")
    });
    Ok(VertexLayouts {
        position_index,
        positions,
        normals: normals.map(|(_, layout)| layout),
        tangents: tangents.map(|(_, layout)| layout),
        texcoords: texcoords.map(|(_, layout)| layout),
        colors,
        has_unsupported_attributes: has_wider_colors || has_other_unsupported_attributes,
    })
}

fn color_layouts(
    root: &Root,
    binary: &[u8],
    primitive: &Primitive,
    positions: AccessorLayout,
) -> Result<(Option<AccessorLayout>, bool), AssetDiagnostic> {
    let mut sets = BTreeMap::new();
    for (attribute, &accessor_index) in &primitive.attributes {
        let Some(suffix) = attribute.strip_prefix("COLOR_") else {
            continue;
        };
        if suffix.is_empty()
            || (suffix.len() > 1 && suffix.starts_with('0'))
            || !suffix.bytes().all(|value| value.is_ascii_digit())
        {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidColor,
                "glb.json.meshes[].primitives[].attributes.COLOR_n",
                Some(accessor_index),
            ));
        }
        let set_index = suffix.parse::<u32>().map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidColor,
                "glb.json.meshes[].primitives[].attributes.COLOR_n",
                Some(accessor_index),
            )
        })?;
        sets.insert(set_index, accessor_index);
    }

    let mut color_0 = None;
    let mut validated_accessors = BTreeMap::new();
    for (expected_set, (&set_index, &accessor_index)) in (0_u32..).zip(&sets) {
        if set_index != expected_set {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidColor,
                "glb.json.meshes[].primitives[].attributes.COLOR_n",
                Some(accessor_index),
            ));
        }
        let layout = if let Some(layout) = validated_accessors.get(&accessor_index) {
            *layout
        } else {
            let layout = accessor_layout(root, binary, accessor_index, AccessorExpectation::Colors)
                .map_err(|error| {
                    if matches!(error.code, AssetDiagnosticCode::UnsupportedAccessor) {
                        diagnostic(
                            AssetDiagnosticCode::InvalidColor,
                            "glb.json.accessors.color_0",
                            Some(accessor_index),
                        )
                    } else {
                        error
                    }
                })?;
            validate_colors(binary, layout)?;
            validated_accessors.insert(accessor_index, layout);
            layout
        };
        if layout.count != positions.count {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidColor,
                "glb.json.accessors.color_0.count",
                Some(accessor_index),
            ));
        }
        if set_index == 0 {
            color_0 = Some(layout);
        }
    }
    Ok((color_0, sets.len() > 1))
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
    Colors,
    Indices,
}

#[derive(Clone, Copy)]
struct AccessorLayout {
    start: usize,
    stride: usize,
    count: u32,
    component_type: u32,
    component_count: u8,
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
    if view.buffer != 0 {
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
    let (element_bytes, component_alignment, component_count) =
        accessor_format(accessor, expectation, accessor_index)?;
    let mut layout = accessor_range(
        binary,
        accessor,
        view,
        accessor_index,
        element_bytes,
        component_alignment,
    )
    .map_err(|error| {
        if matches!(expectation, AccessorExpectation::Colors)
            && matches!(error.code, AssetDiagnosticCode::UnsupportedAccessor)
        {
            diagnostic(
                AssetDiagnosticCode::InvalidColor,
                "glb.json.accessors.color_0",
                Some(accessor_index),
            )
        } else {
            error
        }
    })?;
    layout.component_count = component_count;
    Ok(layout)
}

fn accessor_format(
    accessor: &Accessor,
    expectation: AccessorExpectation,
    accessor_index: u32,
) -> Result<(usize, usize, u8), AssetDiagnostic> {
    match expectation {
        AccessorExpectation::Positions | AccessorExpectation::Normals
            if accessor.component_type == FLOAT
                && accessor.kind == "VEC3"
                && !accessor.normalized =>
        {
            Ok((12, 4, 3))
        }
        AccessorExpectation::Tangents
            if accessor.component_type == FLOAT
                && accessor.kind == "VEC4"
                && !accessor.normalized =>
        {
            Ok((16, 4, 4))
        }
        AccessorExpectation::Texcoords
            if accessor.component_type == FLOAT
                && accessor.kind == "VEC2"
                && !accessor.normalized =>
        {
            Ok((8, 4, 2))
        }
        AccessorExpectation::Colors
            if matches!(accessor.kind.as_str(), "VEC3" | "VEC4")
                && ((accessor.component_type == FLOAT && !accessor.normalized)
                    || (matches!(accessor.component_type, UNSIGNED_BYTE | UNSIGNED_SHORT)
                        && accessor.normalized)) =>
        {
            let component_count: u8 = if accessor.kind == "VEC3" { 3 } else { 4 };
            let component_bytes = match accessor.component_type {
                UNSIGNED_BYTE => 1,
                UNSIGNED_SHORT => 2,
                FLOAT => 4,
                _ => unreachable!("guard admits only core color component types"),
            };
            Ok((
                usize::from(component_count) * component_bytes,
                4,
                component_count,
            ))
        }
        AccessorExpectation::Indices
            if accessor.kind == "SCALAR"
                && !accessor.normalized
                && matches!(accessor.component_type, UNSIGNED_SHORT | UNSIGNED_INT) =>
        {
            let width = if accessor.component_type == UNSIGNED_SHORT {
                2
            } else {
                4
            };
            Ok((width, width, 1))
        }
        AccessorExpectation::Colors => Err(diagnostic(
            AssetDiagnosticCode::InvalidColor,
            "glb.json.accessors.color_0",
            Some(accessor_index),
        )),
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
    if accessor_offset % component_alignment != 0 || start % component_alignment != 0 {
        return Err(diagnostic(
            AssetDiagnosticCode::InvalidBufferRange,
            "glb.json.accessors.alignment",
            Some(accessor_index),
        ));
    }
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
        component_count: 0,
    })
}

fn decode_vertices(
    binary: &[u8],
    layouts: &VertexLayouts,
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
        if position_index >= layouts.positions.count {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidIndex,
                "glb.binary.indices",
                Some(output_index),
            ));
        }
        let position = read_position(binary, layouts.positions, position_index)?;
        let normal = layouts
            .normals
            .map(|layout| read_normal(binary, layout, position_index))
            .transpose()?
            .unwrap_or([FiniteF32::new(0.0).expect("zero is finite"); 3]);
        let tangent = layouts
            .tangents
            .map(|layout| read_tangent(binary, layout, position_index))
            .transpose()?
            .unwrap_or([
                FiniteF32::new(1.0).expect("one is finite"),
                FiniteF32::new(0.0).expect("zero is finite"),
                FiniteF32::new(0.0).expect("zero is finite"),
                FiniteF32::new(1.0).expect("one is finite"),
            ]);
        let texcoord_0 = layouts
            .texcoords
            .map(|layout| read_texcoord(binary, layout, position_index))
            .transpose()?
            .unwrap_or([FiniteF32::new(0.0).expect("zero is finite"); 2]);
        let color_0 = layouts
            .colors
            .map(|layout| read_color(binary, layout, position_index))
            .transpose()?
            .unwrap_or([UnitF32::new(1.0).expect("one is in range"); 4]);
        vertices.push(AssetVertex {
            position,
            normal,
            tangent,
            texcoord_0,
            color_0,
        });
    }
    if layouts.normals.is_none() {
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
    if layouts.tangents.is_some() {
        validate_triangle_tangent_handedness(&vertices, "glb.decoded.triangle_tangent_handedness")?;
    }
    Ok(vertices)
}

fn generate_missing_tangents(
    vertices: &mut [AssetVertex],
    texture_transform: AssetTextureTransform,
    mesh_index: u32,
) -> Result<(), AssetDiagnostic> {
    preflight_generated_tangent_work(vertices, texture_transform, mesh_index)?;

    let zero = FiniteF32::new(0.0).expect("zero is finite");
    for vertex in &mut *vertices {
        vertex.tangent = [zero; 4];
    }

    generate_tangents(&mut GeneratedTangentGeometry {
        vertices,
        texture_transform,
    })
    .map_err(|_| {
        diagnostic(
            AssetDiagnosticCode::InvalidTangent,
            "glb.decoded.generated_tangents",
            Some(mesh_index),
        )
    })?;

    for (corner_index, vertex) in vertices.iter_mut().enumerate() {
        if !matches!(vertex.tangent[3].get(), -1.0 | 1.0) {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidTangent,
                "glb.decoded.generated_tangents",
                Some(stable_index(corner_index)),
            ));
        }
        let xyz = normalize_tangent_vector(
            [vertex.tangent[0], vertex.tangent[1], vertex.tangent[2]],
            "glb.decoded.generated_tangents",
            Some(stable_index(corner_index)),
        )?;
        vertex.tangent[..3].copy_from_slice(&xyz);
    }
    validate_triangle_tangent_handedness(vertices, "glb.decoded.generated_tangents")
}

fn preflight_generated_tangent_work(
    vertices: &[AssetVertex],
    texture_transform: AssetTextureTransform,
    mesh_index: u32,
) -> Result<(), AssetDiagnostic> {
    {
        let mut multiplicities = BTreeMap::<[u32; 8], u64>::new();
        let mut group_work = 0_u64;
        for vertex in vertices {
            let key = generated_tangent_weld_key(vertex, texture_transform);
            let count = multiplicities.entry(key).or_default();
            let next = count
                .checked_add(1)
                .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
            let previous_cube = count
                .checked_mul(*count)
                .and_then(|square| square.checked_mul(*count))
                .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
            let next_cube = next
                .checked_mul(next)
                .and_then(|square| square.checked_mul(next))
                .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
            group_work = group_work
                .checked_add(next_cube - previous_cube)
                .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
            if group_work > GENERATED_TANGENT_GROUP_WORK_LIMIT {
                return Err(generated_tangent_work_diagnostic(mesh_index));
            }
            *count = next;
        }
    }

    let mut degenerate_faces = 0_u64;
    let mut good_faces = 0_u64;
    for triangle in vertices.chunks_exact(3) {
        let first = generated_tangent_position_key(&triangle[0]);
        let second = generated_tangent_position_key(&triangle[1]);
        let third = generated_tangent_position_key(&triangle[2]);
        if first == second || second == third || third == first {
            degenerate_faces = degenerate_faces
                .checked_add(1)
                .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
        } else {
            good_faces = good_faces
                .checked_add(1)
                .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
        }
    }
    let degenerate_search_work = degenerate_faces
        .checked_mul(good_faces)
        .and_then(|pairs| pairs.checked_mul(9))
        .ok_or_else(|| generated_tangent_work_diagnostic(mesh_index))?;
    if degenerate_search_work > GENERATED_TANGENT_DEGENERATE_SEARCH_LIMIT {
        return Err(generated_tangent_work_diagnostic(mesh_index));
    }
    Ok(())
}

fn generated_tangent_weld_key(
    vertex: &AssetVertex,
    texture_transform: AssetTextureTransform,
) -> [u32; 8] {
    let texcoord = texture_transform
        .transform(vertex.texcoord_0.map(FiniteF32::get))
        .expect("texture transforms are validated before tangent generation");
    [
        vertex.position[0].get().to_bits(),
        vertex.position[1].get().to_bits(),
        vertex.position[2].get().to_bits(),
        vertex.normal[0].get().to_bits(),
        vertex.normal[1].get().to_bits(),
        vertex.normal[2].get().to_bits(),
        texcoord[0].to_bits(),
        texcoord[1].to_bits(),
    ]
}

fn generated_tangent_position_key(vertex: &AssetVertex) -> [u32; 3] {
    vertex.position.map(|value| {
        let value = value.get();
        if value == 0.0 {
            0.0_f32.to_bits()
        } else {
            value.to_bits()
        }
    })
}

const fn generated_tangent_work_diagnostic(mesh_index: u32) -> AssetDiagnostic {
    diagnostic(
        AssetDiagnosticCode::CollectionLimitExceeded,
        "glb.decoded.generated_tangent_work",
        Some(mesh_index),
    )
}

fn validate_triangle_tangent_handedness(
    vertices: &[AssetVertex],
    location: &'static str,
) -> Result<(), AssetDiagnostic> {
    for (triangle_index, triangle) in vertices.chunks_exact(3).enumerate() {
        let handedness = triangle[0].tangent[3].get();
        if triangle[1..]
            .iter()
            .any(|vertex| vertex.tangent[3].get().to_bits() != handedness.to_bits())
        {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidTangent,
                location,
                Some(stable_index(triangle_index)),
            ));
        }
    }
    Ok(())
}

struct GeneratedTangentGeometry<'a> {
    vertices: &'a mut [AssetVertex],
    texture_transform: AssetTextureTransform,
}

impl Geometry for GeneratedTangentGeometry<'_> {
    fn num_faces(&self) -> usize {
        self.vertices.len() / 3
    }

    fn num_vertices_of_face(&self, _face: usize) -> usize {
        3
    }

    fn position(&self, face: usize, vert: usize) -> [f32; 3] {
        self.vertex(face, vert).position.map(FiniteF32::get)
    }

    fn normal(&self, face: usize, vert: usize) -> [f32; 3] {
        self.vertex(face, vert).normal.map(FiniteF32::get)
    }

    fn tex_coord(&self, face: usize, vert: usize) -> [f32; 2] {
        self.texture_transform
            .transform(self.vertex(face, vert).texcoord_0.map(FiniteF32::get))
            .expect("texture transforms are validated before tangent generation")
    }

    fn set_tangent(&mut self, tangent_space: Option<TangentSpace>, face: usize, vert: usize) {
        let Some(tangent_space) = tangent_space else {
            return;
        };
        let tangent = tangent_space
            .tangent_encoded()
            .map(|value| FiniteF32::new(value).ok());
        let [Some(x), Some(y), Some(z), Some(w)] = tangent else {
            return;
        };
        self.vertex_mut(face, vert).tangent = [x, y, z, w];
    }
}

impl GeneratedTangentGeometry<'_> {
    fn vertex(&self, face: usize, vert: usize) -> &AssetVertex {
        &self.vertices[face * 3 + vert]
    }

    fn vertex_mut(&mut self, face: usize, vert: usize) -> &mut AssetVertex {
        &mut self.vertices[face * 3 + vert]
    }
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

fn validate_colors(binary: &[u8], layout: AccessorLayout) -> Result<(), AssetDiagnostic> {
    for index in 0..layout.count {
        read_color(binary, layout, index)?;
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

fn read_color(
    binary: &[u8],
    layout: AccessorLayout,
    index: u32,
) -> Result<[UnitF32; 4], AssetDiagnostic> {
    let offset = element_offset(layout, index, "glb.binary.color_0")?;
    let mut color = [UnitF32::new(1.0).expect("one is in range"); 4];
    let component_width = match layout.component_type {
        UNSIGNED_BYTE => 1,
        UNSIGNED_SHORT => 2,
        FLOAT => 4,
        _ => {
            return Err(diagnostic(
                AssetDiagnosticCode::InvalidColor,
                "glb.binary.color_0",
                Some(index),
            ));
        }
    };
    for (component, target) in color
        .iter_mut()
        .take(usize::from(layout.component_count))
        .enumerate()
    {
        let start = offset + component * component_width;
        let value = match layout.component_type {
            UNSIGNED_BYTE => {
                f32::from(*binary.get(start).ok_or_else(|| {
                    diagnostic(
                        AssetDiagnosticCode::InvalidBufferRange,
                        "glb.binary.color_0",
                        Some(index),
                    )
                })?) / f32::from(u8::MAX)
            }
            UNSIGNED_SHORT => {
                let encoded: [u8; 2] = binary
                    .get(start..start + 2)
                    .and_then(|value| value.try_into().ok())
                    .ok_or_else(|| {
                        diagnostic(
                            AssetDiagnosticCode::InvalidBufferRange,
                            "glb.binary.color_0",
                            Some(index),
                        )
                    })?;
                f32::from(u16::from_le_bytes(encoded)) / f32::from(u16::MAX)
            }
            FLOAT => {
                let encoded: [u8; 4] = binary
                    .get(start..start + 4)
                    .and_then(|value| value.try_into().ok())
                    .ok_or_else(|| {
                        diagnostic(
                            AssetDiagnosticCode::InvalidBufferRange,
                            "glb.binary.color_0",
                            Some(index),
                        )
                    })?;
                FiniteF32::new(f32::from_le_bytes(encoded))
                    .map_err(|_| {
                        diagnostic(
                            AssetDiagnosticCode::InvalidColor,
                            "glb.binary.color_0",
                            Some(index),
                        )
                    })?
                    .get()
                    .clamp(0.0, 1.0)
            }
            _ => unreachable!("component type checked above"),
        };
        *target = UnitF32::new(value).expect("normalized or clamped color is in range");
    }
    Ok(color)
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
    extensions: &ExtensionPreflight,
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
    let material = apply_unlit(material_index, &extensions.unlit_materials, material);
    let material = apply_double_sided(root, material_index, material);
    let material = apply_alpha_coverage(root, material_index, material)?;
    let transforms = material_index
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| extensions.texture_transforms.get(index))
        .copied()
        .unwrap_or_default();
    apply_material_textures(root, material_index, transforms, material)
}

fn apply_material_textures(
    root: &Root,
    material_index: Option<u32>,
    transforms: MaterialTextureTransforms,
    mut material: AssetMaterial,
) -> Result<AssetMaterial, AssetDiagnostic> {
    let source_material = material_index.and_then(|index| {
        root.materials
            .get(usize::try_from(index).unwrap_or(usize::MAX))
    });
    if let Some(info) = source_material
        .and_then(|material| material.pbr_metallic_roughness.as_ref())
        .and_then(|pbr| pbr.base_color_texture.as_ref())
    {
        material = material.with_base_color_texture(
            texture_sampler(root, info.index)?,
            transforms
                .base_color
                .unwrap_or(AssetTextureTransform::IDENTITY),
        );
    }
    if let Some(info) = source_material.and_then(|material| material.emissive_texture.as_ref()) {
        material = material.with_emissive_texture(
            texture_sampler(root, info.index)?,
            transforms
                .emissive
                .unwrap_or(AssetTextureTransform::IDENTITY),
        );
    }
    if let Some(info) = source_material
        .and_then(|material| material.pbr_metallic_roughness.as_ref())
        .and_then(|pbr| pbr.metallic_roughness_texture.as_ref())
    {
        material = material.with_metallic_roughness_texture(
            texture_sampler(root, info.index)?,
            transforms
                .metallic_roughness
                .unwrap_or(AssetTextureTransform::IDENTITY),
        );
    }
    if let Some(normal_texture) =
        source_material.and_then(|material| material.normal_texture.as_ref())
    {
        let scale = FiniteF32::new(normal_texture.scale.unwrap_or(1.0)).map_err(|_| {
            diagnostic(
                AssetDiagnosticCode::InvalidJson,
                "glb.json.materials.normalTexture.scale",
                material_index,
            )
        })?;
        material = material.with_normal_texture(
            scale,
            texture_sampler(root, normal_texture.index)?,
            transforms.normal.unwrap_or(AssetTextureTransform::IDENTITY),
        );
    }
    Ok(material)
}

fn apply_unlit(
    material_index: Option<u32>,
    unlit_materials: &[bool],
    material: AssetMaterial,
) -> AssetMaterial {
    let enabled = material_index
        .and_then(|index| usize::try_from(index).ok())
        .and_then(|index| unlit_materials.get(index))
        .copied()
        .unwrap_or(false);
    if enabled {
        material.with_unlit()
    } else {
        material
    }
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
                color_0: [UnitF32::new(1.0).expect("one is in range"); 4],
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
    #[serde(default, deserialize_with = "deserialize_sampler_records")]
    samplers: Vec<Sampler>,
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
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    sampler: Option<u32>,
    #[serde(default)]
    source: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Sampler {
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    mag_filter: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    min_filter: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    wrap_s: Option<u32>,
    #[serde(default, deserialize_with = "deserialize_optional_u32")]
    wrap_t: Option<u32>,
}

fn deserialize_sampler_records<'de, D>(deserializer: D) -> Result<Vec<Sampler>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Vec::<serde_json::Value>::deserialize(deserializer)?
        .into_iter()
        .map(|value| {
            if !value.is_object() {
                return Err(serde::de::Error::custom(
                    "each glTF sampler must be an object",
                ));
            }
            serde_json::from_value(value).map_err(serde::de::Error::custom)
        })
        .collect()
}

fn deserialize_optional_u32<'de, D>(deserializer: D) -> Result<Option<u32>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    u32::deserialize(deserializer).map(Some)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_vertex(position: [f32; 3], texcoord_0: [f32; 2]) -> AssetVertex {
        AssetVertex {
            position: position.map(|value| FiniteF32::new(value).unwrap()),
            normal: [0.0, 0.0, 1.0].map(|value| FiniteF32::new(value).unwrap()),
            tangent: [1.0, 0.0, 0.0, 1.0].map(|value| FiniteF32::new(value).unwrap()),
            texcoord_0: texcoord_0.map(|value| FiniteF32::new(value).unwrap()),
            color_0: [UnitF32::new(1.0).unwrap(); 4],
        }
    }

    #[allow(clippy::cast_precision_loss)]
    fn exact_budget_vertices(degenerate_faces: usize, good_faces: usize) -> Vec<AssetVertex> {
        let mut vertices = Vec::with_capacity((degenerate_faces + good_faces) * 3);
        for face in 0..degenerate_faces {
            let coordinate = (face * 2) as f32;
            for corner in 0..3 {
                vertices.push(test_vertex(
                    [coordinate, 0.0, 0.0],
                    [corner as f32, coordinate + corner as f32],
                ));
            }
        }
        for face in 0..good_faces {
            let coordinate = ((degenerate_faces + face) * 2) as f32;
            vertices.extend([
                test_vertex([coordinate, 0.0, 0.0], [0.0, coordinate]),
                test_vertex([coordinate + 1.0, 0.0, 0.0], [1.0, coordinate]),
                test_vertex([coordinate, 1.0, 0.0], [0.0, coordinate + 1.0]),
            ]);
        }
        vertices
    }

    #[test]
    fn material_preflight_rejects_non_finite_unused_normal_scale() {
        for scale in [f32::INFINITY, f32::NEG_INFINITY, f32::NAN] {
            let root = Root {
                asset: AssetHeader {
                    version: "2.0".to_owned(),
                },
                buffers: Vec::new(),
                buffer_views: Vec::new(),
                accessors: Vec::new(),
                meshes: Vec::new(),
                materials: vec![Material {
                    pbr_metallic_roughness: None,
                    normal_texture: Some(NormalTextureInfo {
                        index: 0,
                        tex_coord: None,
                        scale: Some(scale),
                    }),
                    emissive_texture: None,
                    emissive: None,
                    alpha_mode: None,
                    alpha_cutoff: None,
                    double_sided: None,
                }],
                images: Vec::new(),
                samplers: Vec::new(),
                textures: Vec::new(),
            };

            let error = validate_material_values(&root).unwrap_err();
            assert_eq!(error.code, AssetDiagnosticCode::InvalidJson);
            assert_eq!(error.index, Some(0));
        }
    }

    #[test]
    fn generated_tangent_weld_budget_accepts_exact_limit_and_rejects_one_more() {
        let first = test_vertex([0.0, 0.0, 0.0], [0.0, 0.0]);
        let second = test_vertex([0.0, 0.0, 0.0], [1.0, 0.0]);
        let third = test_vertex([0.0, 0.0, 0.0], [2.0, 0.0]);
        let mut vertices = vec![first; 512];
        vertices.extend(vec![second; 512]);

        preflight_generated_tangent_work(&vertices, AssetTextureTransform::IDENTITY, 7).unwrap();
        vertices.push(third);
        let error = preflight_generated_tangent_work(&vertices, AssetTextureTransform::IDENTITY, 7)
            .unwrap_err();
        assert_eq!(error.code, AssetDiagnosticCode::CollectionLimitExceeded);
        assert_eq!(error.location, "glb.decoded.generated_tangent_work");
        assert_eq!(error.index, Some(7));
    }

    #[test]
    fn generated_tangent_degenerate_search_budget_has_exact_boundary() {
        preflight_generated_tangent_work(
            &exact_budget_vertices(1_365, 1_365),
            AssetTextureTransform::IDENTITY,
            3,
        )
        .unwrap();
        let error = preflight_generated_tangent_work(
            &exact_budget_vertices(1_365, 1_366),
            AssetTextureTransform::IDENTITY,
            3,
        )
        .unwrap_err();
        assert_eq!(error.code, AssetDiagnosticCode::CollectionLimitExceeded);
        assert_eq!(error.location, "glb.decoded.generated_tangent_work");
        assert_eq!(error.index, Some(3));
    }

    #[test]
    fn generated_tangent_degenerate_budget_matches_signed_zero_equality() {
        let mut vertices = exact_budget_vertices(0, 1_365);
        for face in 0_u16..1_366 {
            let coordinate = f32::from(face + 3_000);
            vertices.extend([
                test_vertex([-0.0, coordinate, 0.0], [0.0, coordinate]),
                test_vertex([0.0, coordinate, 0.0], [1.0, coordinate]),
                test_vertex([1.0, coordinate, 0.0], [2.0, coordinate]),
            ]);
        }

        let error = preflight_generated_tangent_work(&vertices, AssetTextureTransform::IDENTITY, 5)
            .unwrap_err();
        assert_eq!(error.code, AssetDiagnosticCode::CollectionLimitExceeded);
        assert_eq!(error.location, "glb.decoded.generated_tangent_work");
        assert_eq!(error.index, Some(5));
    }

    #[test]
    fn generated_tangents_cover_mirrors_seams_and_neighboring_degenerates() {
        let mut mirrored_seam = vec![
            test_vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            test_vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            test_vertex([0.0, 1.0, 0.0], [0.0, 1.0]),
            test_vertex([0.0, 0.0, 0.0], [0.0, 0.0]),
            test_vertex([1.0, 0.0, 0.0], [0.0, 1.0]),
            test_vertex([0.0, 1.0, 0.0], [1.0, 0.0]),
        ];
        generate_missing_tangents(&mut mirrored_seam, AssetTextureTransform::IDENTITY, 0).unwrap();
        assert!(
            mirrored_seam[..3]
                .iter()
                .all(|vertex| vertex.tangent[3].get().to_bits() == 1.0_f32.to_bits())
        );
        assert!(
            mirrored_seam[3..]
                .iter()
                .all(|vertex| vertex.tangent[3].get().to_bits() == (-1.0_f32).to_bits())
        );

        let shared = test_vertex([0.0, 0.0, 0.0], [0.0, 0.0]);
        let mut neighboring_degenerate = vec![
            shared,
            test_vertex([1.0, 0.0, 0.0], [1.0, 0.0]),
            test_vertex([0.0, 1.0, 0.0], [0.0, 1.0]),
            shared,
            shared,
            shared,
        ];
        generate_missing_tangents(
            &mut neighboring_degenerate,
            AssetTextureTransform::IDENTITY,
            0,
        )
        .unwrap();
        let expected = [1.0_f32, 0.0, 0.0, 1.0].map(f32::to_bits);
        assert!(
            neighboring_degenerate
                .iter()
                .all(|vertex| { vertex.tangent.map(|value| value.get().to_bits()) == expected })
        );
    }

    #[test]
    fn isolated_degenerate_generated_tangent_fails_without_a_default() {
        let mut vertices = vec![test_vertex([0.0, 0.0, 0.0], [0.0, 0.0]); 3];
        let error = generate_missing_tangents(&mut vertices, AssetTextureTransform::IDENTITY, 11)
            .unwrap_err();
        assert_eq!(error.code, AssetDiagnosticCode::InvalidTangent);
        assert_eq!(error.location, "glb.decoded.generated_tangents");
        assert_eq!(error.index, Some(0));
    }
}
