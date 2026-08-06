use std::{collections::BTreeMap, sync::Arc};

use cogniform_protocol::{FiniteF32, UnitF32};
use serde::Deserialize;

use crate::types::{
    ASSET_VERTEX_BYTES, AssetDiagnostic, AssetDiagnosticCode, AssetLimits, AssetMaterial,
    AssetVertex, DecodedAsset, DecodedMesh,
};

const GLB_MAGIC: [u8; 4] = *b"glTF";
const GLB_VERSION: u32 = 2;
const JSON_CHUNK: u32 = 0x4e4f_534a;
const BIN_CHUNK: u32 = 0x004e_4942;
const FLOAT: u32 = 5_126;
const UNSIGNED_SHORT: u32 = 5_123;
const UNSIGNED_INT: u32 = 5_125;
const TRIANGLES: u32 = 4;

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
    for field in [
        "animations",
        "cameras",
        "images",
        "nodes",
        "samplers",
        "scenes",
        "skins",
        "textures",
    ] {
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
    Ok(unsupported)
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
    let mut decoded_bytes = 0_u64;
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
    Ok(DecodedAsset {
        meshes,
        byte_len: decoded_bytes,
    })
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
    Ok(())
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
    let (position_index, positions, normals, texcoords) = vertex_layouts(root, binary, primitive)?;
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
    let vertices = decode_vertices(binary, positions, normals, texcoords, indices, output_count)?;
    let material = decode_material(root, primitive.material)?;
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
        && primitive
            .attributes
            .keys()
            .all(|attribute| matches!(attribute.as_str(), "POSITION" | "NORMAL" | "TEXCOORD_0"));
    if !attributes_supported {
        return Err(diagnostic(
            AssetDiagnosticCode::UnsupportedFeature,
            "glb.json.meshes[].primitives[].attributes",
            Some(mesh_index),
        ));
    }
    Ok(primitive)
}

fn vertex_layouts(
    root: &Root,
    binary: &[u8],
    primitive: &Primitive,
) -> Result<
    (
        u32,
        AccessorLayout,
        Option<AccessorLayout>,
        Option<AccessorLayout>,
    ),
    AssetDiagnostic,
> {
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
    Ok((
        position_index,
        positions,
        normals.map(|(_, layout)| layout),
        texcoords.map(|(_, layout)| layout),
    ))
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
        let texcoord_0 = texcoords
            .map(|layout| read_texcoord(binary, layout, position_index))
            .transpose()?
            .unwrap_or([FiniteF32::new(0.0).expect("zero is finite"); 2]);
        vertices.push(AssetVertex {
            position,
            normal,
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
    Ok(vertices)
}

fn validate_texcoords(binary: &[u8], layout: AccessorLayout) -> Result<(), AssetDiagnostic> {
    for index in 0..layout.count {
        read_texcoord(binary, layout, index)?;
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
    let (color_values, metallic_value, roughness_value) = if let Some(index) = material_index {
        let material = get(&root.materials, index, "glb.json.materials")?;
        let pbr = material.pbr_metallic_roughness.as_ref();
        (
            pbr.and_then(|value| value.base_color).unwrap_or([1.0; 4]),
            pbr.and_then(|value| value.metallic).unwrap_or(1.0),
            pbr.and_then(|value| value.roughness).unwrap_or(1.0),
        )
    } else {
        ([0.8, 0.8, 0.8, 1.0], 0.0, 0.8)
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
    Ok(AssetMaterial::new(
        color,
        material_scalar(metallic_value)?,
        material_scalar(roughness_value)?,
    ))
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
}
