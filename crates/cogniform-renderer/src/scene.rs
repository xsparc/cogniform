use core::num::{NonZeroU32, NonZeroU64};
use std::collections::{BTreeMap, BTreeSet};

use cogniform_assets::{
    AssetAlphaMode, AssetMaterial, AssetMeshKey, AssetShadingModel, AssetTextureTransform,
};
use cogniform_protocol::{
    CameraComponent, ColorRgba, LightKind, MaterialComponent, RenderChange, RenderEntity,
    RenderExtraction, SceneRevision, StableEntityId,
};

use crate::RendererError;

pub(crate) const MAX_DIRECTIONAL_LIGHTS: usize = 4;
pub(crate) const MAX_POINT_LIGHTS: usize = 4;
const DEFAULT_METALLIC: f32 = 0.0;
const DEFAULT_ROUGHNESS: f32 = 0.8;

/// Non-zero compact identity owned exclusively by one renderer instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct RenderEntityId(NonZeroU32);

impl RenderEntityId {
    /// Returns the compact numeric value written to the entity-ID attachment.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0.get()
    }
}

/// Result of atomically consuming one extraction packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SceneUpdateSummary {
    /// Fully consumed scene revision.
    pub scene_revision: SceneRevision,
    /// Consumed extraction generation.
    pub generation: NonZeroU64,
    /// Number of records inserted or replaced.
    pub upserts: u32,
    /// Number of removal records consumed.
    pub removals: u32,
}

/// Rejected renderer-state update; the previous extracted scene is preserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SceneUpdateError {
    /// The packet did not extend the previously consumed extraction generation.
    GenerationMismatch {
        /// Expected next generation.
        expected: u64,
        /// Supplied generation.
        supplied: u64,
    },
    /// The renderer has consumed the final representable generation.
    GenerationExhausted,
    /// The packet was prepared against another renderer revision.
    BaseRevisionMismatch {
        /// Current renderer revision.
        current: SceneRevision,
        /// Packet base revision.
        supplied: SceneRevision,
    },
    /// Applying the packet would exceed the configured renderer-state capacity.
    EntityCapacityExceeded {
        /// Configured maximum retained render records.
        limit: u32,
    },
    /// The monotonic compact identity space was exhausted.
    CompactIdentityExhausted,
}

impl std::fmt::Display for SceneUpdateError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::GenerationMismatch { expected, supplied } => write!(
                formatter,
                "expected extraction generation {expected}, received {supplied}"
            ),
            Self::GenerationExhausted => {
                formatter.write_str("renderer extraction generation is exhausted")
            }
            Self::BaseRevisionMismatch { current, supplied } => write!(
                formatter,
                "renderer revision {} does not match packet base {}",
                current.get(),
                supplied.get()
            ),
            Self::EntityCapacityExceeded { limit } => {
                write!(
                    formatter,
                    "renderer entity capacity {limit} would be exceeded"
                )
            }
            Self::CompactIdentityExhausted => {
                formatter.write_str("renderer compact identity space is exhausted")
            }
        }
    }
}

impl std::error::Error for SceneUpdateError {}

pub(crate) struct RenderScene {
    revision: SceneRevision,
    generation: u64,
    entities: BTreeMap<StableEntityId, RenderEntity>,
    compact_ids: BTreeMap<StableEntityId, RenderEntityId>,
    free_compact_ids: BTreeSet<RenderEntityId>,
    next_compact_id: Option<NonZeroU32>,
    max_entities: NonZeroU32,
}

impl RenderScene {
    pub(crate) fn new(max_entities: NonZeroU32) -> Self {
        Self {
            revision: SceneRevision::INITIAL,
            generation: 0,
            entities: BTreeMap::new(),
            compact_ids: BTreeMap::new(),
            free_compact_ids: BTreeSet::new(),
            next_compact_id: NonZeroU32::new(1),
            max_entities,
        }
    }

    pub(crate) const fn revision(&self) -> SceneRevision {
        self.revision
    }

    pub(crate) const fn generation(&self) -> u64 {
        self.generation
    }

    pub(crate) fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub(crate) fn compact_id(&self, entity_id: StableEntityId) -> Option<RenderEntityId> {
        self.compact_ids.get(&entity_id).copied()
    }

    pub(crate) fn apply(
        &mut self,
        extraction: &RenderExtraction,
    ) -> Result<SceneUpdateSummary, SceneUpdateError> {
        let supplied_generation = extraction.generation().get();
        let expected_generation = self
            .generation
            .checked_add(1)
            .ok_or(SceneUpdateError::GenerationExhausted)?;
        if supplied_generation != expected_generation {
            return Err(SceneUpdateError::GenerationMismatch {
                expected: expected_generation,
                supplied: supplied_generation,
            });
        }
        if extraction.base_revision() != self.revision {
            return Err(SceneUpdateError::BaseRevisionMismatch {
                current: self.revision,
                supplied: extraction.base_revision(),
            });
        }

        let mut projected_count = self.entities.len();
        let mut new_id_count = 0_usize;
        for change in extraction.changes() {
            match change {
                RenderChange::Upsert(entity)
                    if !self.entities.contains_key(&entity.entity_id()) =>
                {
                    projected_count = projected_count.saturating_add(1);
                    new_id_count = new_id_count.saturating_add(1);
                }
                RenderChange::Remove(entity_id) if self.entities.contains_key(entity_id) => {
                    projected_count = projected_count.saturating_sub(1);
                }
                RenderChange::Upsert(_) | RenderChange::Remove(_) => {}
            }
        }
        if projected_count
            > usize::try_from(self.max_entities.get()).expect("u32 entity capacity fits usize")
        {
            return Err(SceneUpdateError::EntityCapacityExceeded {
                limit: self.max_entities.get(),
            });
        }

        let mut assigned = BTreeMap::new();
        let mut free_compact_ids = self.free_compact_ids.clone();
        for change in extraction.changes() {
            if let RenderChange::Remove(entity_id) = change
                && let Some(compact_id) = self.compact_ids.get(entity_id)
            {
                free_compact_ids.insert(*compact_id);
            }
        }
        let mut next = self.next_compact_id;
        for change in extraction.changes() {
            let RenderChange::Upsert(entity) = change else {
                continue;
            };
            if self.entities.contains_key(&entity.entity_id()) {
                continue;
            }
            let compact_id = if let Some(value) = free_compact_ids.pop_first() {
                value
            } else {
                let value = next.ok_or(SceneUpdateError::CompactIdentityExhausted)?;
                next = value.get().checked_add(1).and_then(NonZeroU32::new);
                RenderEntityId(value)
            };
            assigned.insert(entity.entity_id(), compact_id);
        }
        debug_assert_eq!(assigned.len(), new_id_count);

        let mut upserts = 0_u32;
        let mut removals = 0_u32;
        for change in extraction.changes() {
            match change {
                RenderChange::Upsert(entity) => {
                    if let Some(compact_id) = assigned.remove(&entity.entity_id()) {
                        self.compact_ids.insert(entity.entity_id(), compact_id);
                    }
                    self.entities
                        .insert(entity.entity_id(), entity.as_ref().clone());
                    upserts = upserts.saturating_add(1);
                }
                RenderChange::Remove(entity_id) => {
                    self.entities.remove(entity_id);
                    self.compact_ids.remove(entity_id);
                    removals = removals.saturating_add(1);
                }
            }
        }
        self.next_compact_id = next;
        self.free_compact_ids = free_compact_ids;
        self.revision = extraction.scene_revision();
        self.generation = supplied_generation;
        Ok(SceneUpdateSummary {
            scene_revision: self.revision,
            generation: extraction.generation(),
            upserts,
            removals,
        })
    }

    pub(crate) fn prepare(
        &self,
        camera_id: StableEntityId,
        width: u32,
        height: u32,
        max_draws: NonZeroU32,
        mut resolve_asset: impl FnMut(AssetMeshKey) -> Option<AssetMaterial>,
    ) -> Result<PreparedScene, RendererError> {
        let camera_entity = self
            .entities
            .get(&camera_id)
            .ok_or(RendererError::CameraUnavailable { camera_id })?;
        let camera = camera_entity
            .camera()
            .ok_or(RendererError::CameraUnavailable { camera_id })?;
        let view_projection = camera_view_projection(camera_entity, camera, width, height)?;
        let camera_position = point_position(camera_entity)?;
        let directional_lights = self.prepare_directional_lights()?;
        let point_lights = self.prepare_point_lights()?;
        let mut draws = Vec::new();
        let mut id_lookup = BTreeMap::new();
        for (&entity_id, entity) in &self.entities {
            let primitive = entity.primitive();
            let (geometry, imported_material) = if let Some(asset_mesh) = entity.asset_mesh() {
                let key = AssetMeshKey {
                    content_hash: asset_mesh.content_hash,
                    mesh_index: asset_mesh.mesh_index,
                };
                if let Some(material) = resolve_asset(key) {
                    (PreparedGeometry::Asset(key), Some(material))
                } else if let Some(primitive) = primitive {
                    (primitive_geometry(primitive.shape), None)
                } else {
                    return Err(RendererError::AssetUnavailable { entity_id, key });
                }
            } else if let Some(primitive) = primitive {
                (primitive_geometry(primitive.shape), None)
            } else {
                continue;
            };
            if draws.len()
                >= usize::try_from(max_draws.get()).expect("u32 draw capacity fits usize")
            {
                return Err(RendererError::DrawCapacityExceeded {
                    limit: max_draws.get(),
                });
            }
            let compact_id = self.compact_ids[&entity_id];
            let mut model = matrix_to_f32(entity.world_transform(), entity_id)?;
            if !matches!(geometry, PreparedGeometry::Asset(_)) {
                let primitive = primitive.expect("built-in geometry has a primitive");
                let dimensions = [
                    primitive.dimensions.x.get(),
                    primitive.dimensions.y.get(),
                    primitive.dimensions.z.get(),
                ];
                for row in 0..4 {
                    model[row] *= dimensions[0];
                    model[4 + row] *= dimensions[1];
                    model[8 + row] *= dimensions[2];
                }
            }
            let (color, metallic, roughness, emissive) =
                material_values(entity.material(), imported_material);
            draws.push(
                PreparedDraw {
                    geometry,
                    model,
                    view_projection,
                    color,
                    camera_position,
                    metallic,
                    roughness,
                    emissive,
                    normal_scale: 1.0,
                    imported_texture_roles: ImportedTextureRoles::NONE,
                    imported_texture_transforms: ImportedTextureTransforms::IDENTITY,
                    imported_alpha_coverage: ImportedAlphaCoverage::Disabled,
                    imported_face_policy: ImportedFacePolicy::Disabled,
                    imported_shading_model: ImportedShadingModel::MetallicRoughness,
                    imported_vertex_color: false,
                    compact_id: compact_id.get(),
                }
                .with_imported_material(entity.material().is_none(), imported_material),
            );
            id_lookup.insert(compact_id.get(), entity_id);
        }
        Ok(PreparedScene {
            draws,
            directional_lights,
            point_lights,
            id_lookup,
        })
    }

    fn prepare_directional_lights(&self) -> Result<Vec<PreparedDirectionalLight>, RendererError> {
        let mut definition_count = 0_usize;
        let mut prepared = Vec::with_capacity(MAX_DIRECTIONAL_LIGHTS);
        for entity in self.entities.values() {
            let Some(light) = entity.light() else {
                continue;
            };
            if light.kind == LightKind::Point {
                continue;
            }
            definition_count = definition_count.saturating_add(1);
            if definition_count > MAX_DIRECTIONAL_LIGHTS {
                return Err(RendererError::DirectionalLightCapacityExceeded {
                    actual: u32::try_from(definition_count)
                        .expect("bounded directional-light count fits u32"),
                    limit: u32::try_from(MAX_DIRECTIONAL_LIGHTS)
                        .expect("fixed directional-light limit fits u32"),
                });
            }
            let intensity = light.intensity.get();
            if intensity.to_bits() == 0 {
                continue;
            }
            prepared.push(PreparedDirectionalLight {
                surface_to_light: normalized_positive_z(entity)?,
                color: [
                    light.color.r.get(),
                    light.color.g.get(),
                    light.color.b.get(),
                ],
                intensity,
            });
        }
        Ok(prepared)
    }

    fn prepare_point_lights(&self) -> Result<Vec<PreparedPointLight>, RendererError> {
        let mut definition_count = 0_usize;
        let mut prepared = Vec::with_capacity(MAX_POINT_LIGHTS);
        for entity in self.entities.values() {
            let Some(light) = entity.light() else {
                continue;
            };
            if light.kind == LightKind::Directional {
                continue;
            }
            definition_count = definition_count.saturating_add(1);
            if definition_count > MAX_POINT_LIGHTS {
                return Err(RendererError::PointLightCapacityExceeded {
                    actual: u32::try_from(definition_count)
                        .expect("bounded point-light count fits u32"),
                    limit: u32::try_from(MAX_POINT_LIGHTS)
                        .expect("fixed point-light limit fits u32"),
                });
            }
            let intensity = light.intensity.get();
            if intensity.to_bits() == 0 {
                continue;
            }
            prepared.push(PreparedPointLight {
                position: point_position(entity)?,
                color: [
                    light.color.r.get(),
                    light.color.g.get(),
                    light.color.b.get(),
                ],
                intensity,
            });
        }
        Ok(prepared)
    }
}

pub(crate) struct PreparedScene {
    pub(crate) draws: Vec<PreparedDraw>,
    pub(crate) directional_lights: Vec<PreparedDirectionalLight>,
    pub(crate) point_lights: Vec<PreparedPointLight>,
    pub(crate) id_lookup: BTreeMap<u32, StableEntityId>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PreparedDirectionalLight {
    pub(crate) surface_to_light: [f32; 3],
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct PreparedPointLight {
    pub(crate) position: [f32; 3],
    pub(crate) color: [f32; 3],
    pub(crate) intensity: f32,
}

pub(crate) struct PreparedDraw {
    pub(crate) geometry: PreparedGeometry,
    pub(crate) model: [f32; 16],
    pub(crate) view_projection: [f32; 16],
    pub(crate) color: [f32; 4],
    pub(crate) camera_position: [f32; 3],
    pub(crate) metallic: f32,
    pub(crate) roughness: f32,
    pub(crate) emissive: [f32; 3],
    pub(crate) normal_scale: f32,
    pub(crate) imported_texture_roles: ImportedTextureRoles,
    pub(crate) imported_texture_transforms: ImportedTextureTransforms,
    pub(crate) imported_alpha_coverage: ImportedAlphaCoverage,
    pub(crate) imported_face_policy: ImportedFacePolicy,
    pub(crate) imported_shading_model: ImportedShadingModel,
    pub(crate) imported_vertex_color: bool,
    pub(crate) compact_id: u32,
}

impl PreparedDraw {
    fn with_imported_material(
        mut self,
        use_imported_material: bool,
        material: Option<AssetMaterial>,
    ) -> Self {
        let (roles, transforms, normal_scale, alpha_coverage, face_policy, shading_model) =
            imported_material_selection(use_imported_material, material);
        self.imported_texture_roles = roles;
        self.imported_texture_transforms = transforms;
        self.normal_scale = normal_scale;
        self.imported_alpha_coverage = alpha_coverage;
        self.imported_face_policy = face_policy;
        self.imported_shading_model = shading_model;
        self.imported_vertex_color =
            use_imported_material && matches!(self.geometry, PreparedGeometry::Asset(_));
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ImportedTextureTransforms {
    pub(crate) base_color: AssetTextureTransform,
    pub(crate) normal: AssetTextureTransform,
    pub(crate) metallic_roughness: AssetTextureTransform,
    pub(crate) emissive: AssetTextureTransform,
}

impl ImportedTextureTransforms {
    pub(crate) const IDENTITY: Self = Self {
        base_color: AssetTextureTransform::IDENTITY,
        normal: AssetTextureTransform::IDENTITY,
        metallic_roughness: AssetTextureTransform::IDENTITY,
        emissive: AssetTextureTransform::IDENTITY,
    };
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportedShadingModel {
    MetallicRoughness,
    Unlit,
}

impl ImportedShadingModel {
    const UNLIT_FLAG: u8 = 1 << 4;

    pub(crate) const fn flags(self) -> u8 {
        match self {
            Self::MetallicRoughness => 0,
            Self::Unlit => Self::UNLIT_FLAG,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImportedFacePolicy {
    Disabled,
    SingleSided,
    DoubleSided,
}

impl ImportedFacePolicy {
    const DOUBLE_SIDED_FLAG: u8 = 1 << 3;

    pub(crate) const fn culls_back_faces(self) -> bool {
        matches!(self, Self::SingleSided)
    }

    pub(crate) const fn flags(self) -> u8 {
        match self {
            Self::Disabled | Self::SingleSided => 0,
            Self::DoubleSided => Self::DOUBLE_SIDED_FLAG,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) enum ImportedAlphaCoverage {
    Disabled,
    Opaque,
    Mask { cutoff: f32 },
}

impl ImportedAlphaCoverage {
    const ENABLED_FLAG: u8 = 1 << 1;
    const MASK_FLAG: u8 = 1 << 2;

    pub(crate) const fn flags(self) -> u8 {
        match self {
            Self::Disabled => 0,
            Self::Opaque => Self::ENABLED_FLAG,
            Self::Mask { .. } => Self::ENABLED_FLAG | Self::MASK_FLAG,
        }
    }

    pub(crate) const fn cutoff(self) -> f32 {
        match self {
            Self::Disabled | Self::Opaque => 0.0,
            Self::Mask { cutoff } => cutoff,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct ImportedTextureRoles(u8);

impl ImportedTextureRoles {
    pub(crate) const NONE: Self = Self(0);
    #[cfg(test)]
    pub(crate) const NORMAL_ONLY: Self = Self(Self::NORMAL);
    const BASE_COLOR: u8 = 1 << 0;
    const EMISSIVE: u8 = 1 << 1;
    const METALLIC_ROUGHNESS: u8 = 1 << 2;
    const NORMAL: u8 = 1 << 3;

    pub(crate) const fn base_color(self) -> bool {
        self.0 & Self::BASE_COLOR != 0
    }

    pub(crate) const fn emissive(self) -> bool {
        self.0 & Self::EMISSIVE != 0
    }

    pub(crate) const fn metallic_roughness(self) -> bool {
        self.0 & Self::METALLIC_ROUGHNESS != 0
    }

    pub(crate) const fn normal(self) -> bool {
        self.0 & Self::NORMAL != 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedGeometry {
    Cuboid,
    Plane,
    Sphere,
    Asset(AssetMeshKey),
}

fn primitive_geometry(shape: cogniform_protocol::PrimitiveShape) -> PreparedGeometry {
    match shape {
        cogniform_protocol::PrimitiveShape::Cuboid => PreparedGeometry::Cuboid,
        cogniform_protocol::PrimitiveShape::Plane => PreparedGeometry::Plane,
        cogniform_protocol::PrimitiveShape::Sphere => PreparedGeometry::Sphere,
    }
}

fn imported_material_selection(
    use_imported_material: bool,
    material: Option<AssetMaterial>,
) -> (
    ImportedTextureRoles,
    ImportedTextureTransforms,
    f32,
    ImportedAlphaCoverage,
    ImportedFacePolicy,
    ImportedShadingModel,
) {
    let shading_model = if use_imported_material
        && material
            .is_some_and(|material| matches!(material.shading_model(), AssetShadingModel::Unlit))
    {
        ImportedShadingModel::Unlit
    } else {
        ImportedShadingModel::MetallicRoughness
    };
    let use_lit_roles = matches!(shading_model, ImportedShadingModel::MetallicRoughness);
    let use_base_color =
        use_imported_material && material.is_some_and(AssetMaterial::has_base_color_texture);
    let use_normal = use_imported_material
        && use_lit_roles
        && material.is_some_and(AssetMaterial::has_normal_texture);
    let use_emissive = use_imported_material
        && use_lit_roles
        && material.is_some_and(AssetMaterial::has_emissive_texture);
    let use_metallic_roughness = use_imported_material
        && use_lit_roles
        && material.is_some_and(AssetMaterial::has_metallic_roughness_texture);
    let normal_scale = if use_normal {
        material.map_or(1.0, AssetMaterial::normal_scale)
    } else {
        1.0
    };
    let mut roles = 0;
    roles |= u8::from(use_base_color) * ImportedTextureRoles::BASE_COLOR;
    roles |= u8::from(use_emissive) * ImportedTextureRoles::EMISSIVE;
    roles |= u8::from(use_metallic_roughness) * ImportedTextureRoles::METALLIC_ROUGHNESS;
    roles |= u8::from(use_normal) * ImportedTextureRoles::NORMAL;
    let transforms = ImportedTextureTransforms {
        base_color: if use_base_color {
            material
                .and_then(AssetMaterial::base_color_texture_transform)
                .expect("selected base-color role retains a transform")
        } else {
            AssetTextureTransform::IDENTITY
        },
        normal: if use_normal {
            material
                .and_then(AssetMaterial::normal_texture_transform)
                .expect("selected normal role retains a transform")
        } else {
            AssetTextureTransform::IDENTITY
        },
        metallic_roughness: if use_metallic_roughness {
            material
                .and_then(AssetMaterial::metallic_roughness_texture_transform)
                .expect("selected metallic-roughness role retains a transform")
        } else {
            AssetTextureTransform::IDENTITY
        },
        emissive: if use_emissive {
            material
                .and_then(AssetMaterial::emissive_texture_transform)
                .expect("selected emissive role retains a transform")
        } else {
            AssetTextureTransform::IDENTITY
        },
    };
    let alpha_coverage = if use_imported_material {
        material.map_or(ImportedAlphaCoverage::Disabled, |material| {
            match material.alpha_mode() {
                AssetAlphaMode::Mask => ImportedAlphaCoverage::Mask {
                    cutoff: material
                        .alpha_cutoff()
                        .expect("mask material retains a cutoff"),
                },
                _ => ImportedAlphaCoverage::Opaque,
            }
        })
    } else {
        ImportedAlphaCoverage::Disabled
    };
    let face_policy = if use_imported_material {
        material.map_or(ImportedFacePolicy::Disabled, |material| {
            if material.double_sided() {
                ImportedFacePolicy::DoubleSided
            } else {
                ImportedFacePolicy::SingleSided
            }
        })
    } else {
        ImportedFacePolicy::Disabled
    };
    (
        ImportedTextureRoles(roles),
        transforms,
        normal_scale,
        alpha_coverage,
        face_policy,
        shading_model,
    )
}

fn color_values(color: ColorRgba) -> [f32; 4] {
    [color.r.get(), color.g.get(), color.b.get(), color.a.get()]
}

fn material_values(
    scene_material: Option<MaterialComponent>,
    imported_material: Option<AssetMaterial>,
) -> ([f32; 4], f32, f32, [f32; 3]) {
    scene_material.map_or_else(
        || {
            imported_material.map_or(
                (
                    [0.8, 0.8, 0.8, 1.0],
                    DEFAULT_METALLIC,
                    DEFAULT_ROUGHNESS,
                    [0.0; 3],
                ),
                |material| {
                    (
                        material.base_color().map(cogniform_protocol::UnitF32::get),
                        material.metallic().get(),
                        material.roughness().get(),
                        material.emissive(),
                    )
                },
            )
        },
        |material| {
            (
                color_values(material.base_color),
                material.metallic.get(),
                material.roughness.get(),
                [0.0; 3],
            )
        },
    )
}

#[allow(clippy::cast_possible_truncation)]
fn normalized_positive_z(entity: &RenderEntity) -> Result<[f32; 3], RendererError> {
    let matrix = entity.world_transform();
    let direction = [matrix[8], matrix[9], matrix[10]];
    let scale = direction
        .iter()
        .map(|value| value.abs())
        .fold(0.0, f64::max);
    if scale.to_bits() == 0 {
        return Err(RendererError::DirectionalLightDirectionInvalid {
            entity_id: entity.entity_id(),
        });
    }
    let scaled = direction.map(|value| value / scale);
    let length = scaled.iter().map(|value| value * value).sum::<f64>().sqrt();
    let normalized = scaled.map(|value| (value / length) as f32);
    if !normalized.iter().all(|value| value.is_finite()) {
        return Err(RendererError::DirectionalLightDirectionInvalid {
            entity_id: entity.entity_id(),
        });
    }
    Ok(normalized)
}

#[allow(clippy::cast_possible_truncation)]
fn point_position(entity: &RenderEntity) -> Result<[f32; 3], RendererError> {
    let matrix = entity.world_transform();
    let position = [matrix[12] as f32, matrix[13] as f32, matrix[14] as f32];
    if !position.iter().all(|value| value.is_finite()) {
        return Err(RendererError::GpuTransformOutOfRange {
            entity_id: entity.entity_id(),
        });
    }
    Ok(position)
}

fn camera_view_projection(
    entity: &RenderEntity,
    camera: CameraComponent,
    width: u32,
    height: u32,
) -> Result<[f32; 16], RendererError> {
    let world = entity.world_transform();
    let view = invert_affine(world).ok_or(RendererError::CameraTransformNotInvertible {
        camera_id: entity.entity_id(),
    })?;
    let aspect = f64::from(width) / f64::from(height);
    let f = (f64::from(camera.vertical_fov_radians.get()) * 0.5)
        .tan()
        .recip();
    let near = f64::from(camera.near.get());
    let far = f64::from(camera.far.get());
    let projection = [
        f / aspect,
        0.0,
        0.0,
        0.0,
        0.0,
        f,
        0.0,
        0.0,
        0.0,
        0.0,
        far / (near - far),
        -1.0,
        0.0,
        0.0,
        (far * near) / (near - far),
        0.0,
    ];
    let combined = multiply_matrices(&projection, &view);
    matrix_to_f32(&combined, entity.entity_id())
}

#[allow(clippy::cast_possible_truncation)]
fn matrix_to_f32(
    matrix: &[f64; 16],
    entity_id: StableEntityId,
) -> Result<[f32; 16], RendererError> {
    let mut result = [0.0_f32; 16];
    for (target, &value) in result.iter_mut().zip(matrix) {
        let converted = value as f32;
        if !converted.is_finite() {
            return Err(RendererError::GpuTransformOutOfRange { entity_id });
        }
        *target = converted;
    }
    Ok(result)
}

fn invert_affine(matrix: &[f64; 16]) -> Option<[f64; 16]> {
    let epsilon = f64::EPSILON * 16.0;
    if matrix[3].abs() > epsilon
        || matrix[7].abs() > epsilon
        || matrix[11].abs() > epsilon
        || (matrix[15] - 1.0).abs() > epsilon
    {
        return None;
    }
    let a00 = matrix[0];
    let a01 = matrix[4];
    let a02 = matrix[8];
    let a10 = matrix[1];
    let a11 = matrix[5];
    let a12 = matrix[9];
    let a20 = matrix[2];
    let a21 = matrix[6];
    let a22 = matrix[10];
    let determinant = a00 * (a11 * a22 - a12 * a21) - a01 * (a10 * a22 - a12 * a20)
        + a02 * (a10 * a21 - a11 * a20);
    if !determinant.is_finite() || determinant.abs() <= f64::EPSILON {
        return None;
    }
    let inverse = determinant.recip();
    let b00 = (a11 * a22 - a12 * a21) * inverse;
    let b01 = (a02 * a21 - a01 * a22) * inverse;
    let b02 = (a01 * a12 - a02 * a11) * inverse;
    let b10 = (a12 * a20 - a10 * a22) * inverse;
    let b11 = (a00 * a22 - a02 * a20) * inverse;
    let b12 = (a02 * a10 - a00 * a12) * inverse;
    let b20 = (a10 * a21 - a11 * a20) * inverse;
    let b21 = (a01 * a20 - a00 * a21) * inverse;
    let b22 = (a00 * a11 - a01 * a10) * inverse;
    let tx = matrix[12];
    let ty = matrix[13];
    let tz = matrix[14];
    let result = [
        b00,
        b10,
        b20,
        0.0,
        b01,
        b11,
        b21,
        0.0,
        b02,
        b12,
        b22,
        0.0,
        -(b00 * tx + b01 * ty + b02 * tz),
        -(b10 * tx + b11 * ty + b12 * tz),
        -(b20 * tx + b21 * ty + b22 * tz),
        1.0,
    ];
    result
        .iter()
        .all(|value| value.is_finite())
        .then_some(result)
}

fn multiply_matrices(left: &[f64; 16], right: &[f64; 16]) -> [f64; 16] {
    let mut output = [0.0; 16];
    for column in 0..4 {
        for row in 0..4 {
            output[column * 4 + row] = (0..4)
                .map(|index| left[index * 4 + row] * right[column * 4 + index])
                .sum();
        }
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use cogniform_protocol::{
        AssetMeshComponent, CameraComponent, ColorRgb, ColorRgba, ContentHash, LightComponent,
        MaterialComponent, NonNegativeF32, PositiveF32, PositiveVec3, PrimitiveComponent,
        PrimitiveShape, RenderComponents, UnitF32,
    };

    fn id(value: u128) -> StableEntityId {
        StableEntityId::new(value).unwrap()
    }

    fn assert_exact_f32(actual: f32, expected: f32) {
        assert_eq!(actual.to_bits(), expected.to_bits());
    }

    fn asset_material(color: [f32; 4], metallic: f32, roughness: f32) -> AssetMaterial {
        AssetMaterial::new(
            color.map(|value| UnitF32::new(value).unwrap()),
            UnitF32::new(metallic).unwrap(),
            UnitF32::new(roughness).unwrap(),
        )
    }

    fn entity(entity_id: StableEntityId) -> RenderEntity {
        primitive_entity(entity_id, PrimitiveShape::Cuboid, [1.0; 3])
    }

    fn primitive_entity(
        entity_id: StableEntityId,
        shape: PrimitiveShape,
        dimensions: [f32; 3],
    ) -> RenderEntity {
        let positive = |value| PositiveF32::new(value).unwrap();
        let unit = |value| UnitF32::new(value).unwrap();
        RenderEntity::new(
            entity_id,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            1,
            RenderComponents {
                primitive: Some(PrimitiveComponent {
                    shape,
                    dimensions: PositiveVec3 {
                        x: positive(dimensions[0]),
                        y: positive(dimensions[1]),
                        z: positive(dimensions[2]),
                    },
                }),
                material: Some(MaterialComponent {
                    base_color: ColorRgba {
                        r: unit(0.2),
                        g: unit(0.4),
                        b: unit(0.6),
                        a: unit(1.0),
                    },
                    metallic: unit(0.0),
                    roughness: unit(0.5),
                }),
                ..RenderComponents::default()
            },
        )
        .unwrap()
    }

    fn camera(entity_id: StableEntityId) -> RenderEntity {
        RenderEntity::new(
            entity_id,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 3.0, 1.0,
            ],
            1,
            RenderComponents {
                camera: Some(CameraComponent {
                    vertical_fov_radians: PositiveF32::new(core::f32::consts::FRAC_PI_2).unwrap(),
                    near: PositiveF32::new(0.1).unwrap(),
                    far: PositiveF32::new(100.0).unwrap(),
                }),
                ..RenderComponents::default()
            },
        )
        .unwrap()
    }

    fn light_entity(
        entity_id: StableEntityId,
        kind: LightKind,
        world_transform: [f64; 16],
        color: [f32; 3],
        intensity: f32,
    ) -> RenderEntity {
        let unit = |value| UnitF32::new(value).unwrap();
        RenderEntity::new(
            entity_id,
            world_transform,
            1,
            RenderComponents {
                light: Some(LightComponent {
                    kind,
                    color: ColorRgb {
                        r: unit(color[0]),
                        g: unit(color[1]),
                        b: unit(color[2]),
                    },
                    intensity: NonNegativeF32::new(intensity).unwrap(),
                }),
                ..RenderComponents::default()
            },
        )
        .unwrap()
    }

    fn asset_entity(entity_id: StableEntityId, key: AssetMeshKey) -> RenderEntity {
        asset_entity_with_primitive(entity_id, key, None)
    }

    fn asset_entity_with_primitive(
        entity_id: StableEntityId,
        key: AssetMeshKey,
        primitive: Option<PrimitiveComponent>,
    ) -> RenderEntity {
        RenderEntity::new(
            entity_id,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            1,
            RenderComponents {
                primitive,
                asset_mesh: Some(AssetMeshComponent {
                    content_hash: key.content_hash,
                    mesh_index: key.mesh_index,
                }),
                ..RenderComponents::default()
            },
        )
        .unwrap()
    }

    fn asset_entity_with_material(
        entity_id: StableEntityId,
        key: AssetMeshKey,
        material: MaterialComponent,
        generation: u64,
    ) -> RenderEntity {
        RenderEntity::new(
            entity_id,
            [
                1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
            ],
            generation,
            RenderComponents {
                asset_mesh: Some(AssetMeshComponent {
                    content_hash: key.content_hash,
                    mesh_index: key.mesh_index,
                }),
                material: Some(material),
                ..RenderComponents::default()
            },
        )
        .unwrap()
    }

    fn extraction(
        generation: u64,
        base: u64,
        target: u64,
        changes: Vec<RenderChange>,
    ) -> RenderExtraction {
        RenderExtraction::new(
            NonZeroU64::new(generation).unwrap(),
            SceneRevision::new(base),
            SceneRevision::new(target),
            changes,
        )
        .unwrap()
    }

    #[test]
    fn updates_are_atomic_and_compact_ids_are_safely_recycled() {
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![RenderChange::upsert(entity(id(1)))],
            ))
            .unwrap();
        let first = scene.compact_id(id(1)).unwrap();

        let mismatch = extraction(3, 1, 2, vec![RenderChange::upsert(entity(id(2)))]);
        assert!(matches!(
            scene.apply(&mismatch),
            Err(SceneUpdateError::GenerationMismatch { .. })
        ));
        assert_eq!(scene.entity_count(), 1);
        assert_eq!(scene.revision(), SceneRevision::new(1));

        scene
            .apply(&extraction(2, 1, 2, vec![RenderChange::remove(id(1))]))
            .unwrap();
        scene
            .apply(&extraction(
                3,
                2,
                3,
                vec![RenderChange::upsert(entity(id(1)))],
            ))
            .unwrap();
        assert_eq!(scene.compact_id(id(1)).unwrap(), first);
    }

    #[test]
    fn draw_capacity_rejects_before_gpu_preparation() {
        let mut scene = RenderScene::new(NonZeroU32::new(3).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(entity(id(2))),
                    RenderChange::upsert(entity(id(3))),
                ],
            ))
            .unwrap();
        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::DrawCapacityExceeded { limit: 1 })
        ));
    }

    #[test]
    fn prepared_draw_carries_exact_camera_and_material_inputs() {
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(entity(id(2))),
                ],
            ))
            .unwrap();

        let prepared = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None)
            .unwrap();
        let draw = &prepared.draws[0];
        assert_eq!(
            draw.camera_position.map(f32::to_bits),
            [0.0, 0.0, 3.0].map(f32::to_bits)
        );
        assert_eq!(draw.metallic.to_bits(), 0.0_f32.to_bits());
        assert_eq!(draw.roughness.to_bits(), 0.5_f32.to_bits());
    }

    #[test]
    fn directional_and_point_lights_are_prepared_in_stable_order() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut plus_x_plus_y = identity;
        plus_x_plus_y[8] = 3.0;
        plus_x_plus_y[9] = 4.0;
        plus_x_plus_y[10] = 0.0;
        let mut degenerate = identity;
        degenerate[8] = 0.0;
        degenerate[9] = 0.0;
        degenerate[10] = 0.0;
        let mut first_point = identity;
        first_point[12] = 1.0;
        first_point[13] = 2.0;
        first_point[14] = 3.0;
        let mut second_point = identity;
        second_point[12] = -4.0;
        second_point[13] = 5.0;
        second_point[14] = 6.0;
        let mut scene = RenderScene::new(NonZeroU32::new(7).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(entity(id(2))),
                    RenderChange::upsert(light_entity(
                        id(3),
                        LightKind::Point,
                        first_point,
                        [1.0, 0.0, 0.5],
                        1.0,
                    )),
                    RenderChange::upsert(light_entity(
                        id(4),
                        LightKind::Directional,
                        degenerate,
                        [1.0; 3],
                        0.0,
                    )),
                    RenderChange::upsert(light_entity(
                        id(5),
                        LightKind::Directional,
                        plus_x_plus_y,
                        [0.0, 1.0, 0.0],
                        0.5,
                    )),
                    RenderChange::upsert(light_entity(
                        id(8),
                        LightKind::Point,
                        second_point,
                        [0.25, 0.5, 0.75],
                        0.5,
                    )),
                    RenderChange::upsert(light_entity(
                        id(9),
                        LightKind::Directional,
                        identity,
                        [1.0, 0.0, 0.0],
                        0.25,
                    )),
                ],
            ))
            .unwrap();

        let prepared = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None)
            .unwrap();
        assert_eq!(
            prepared.directional_lights,
            vec![
                PreparedDirectionalLight {
                    surface_to_light: [0.6, 0.8, 0.0],
                    color: [0.0, 1.0, 0.0],
                    intensity: 0.5,
                },
                PreparedDirectionalLight {
                    surface_to_light: [0.0, 0.0, 1.0],
                    color: [1.0, 0.0, 0.0],
                    intensity: 0.25,
                },
            ]
        );
        assert_eq!(
            prepared.point_lights,
            vec![
                PreparedPointLight {
                    position: [1.0, 2.0, 3.0],
                    color: [1.0, 0.0, 0.5],
                    intensity: 1.0,
                },
                PreparedPointLight {
                    position: [-4.0, 5.0, 6.0],
                    color: [0.25, 0.5, 0.75],
                    intensity: 0.5,
                },
            ]
        );
    }

    #[test]
    fn fifth_directional_light_definition_is_rejected_before_gpu_submission() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut changes = vec![
            RenderChange::upsert(camera(id(1))),
            RenderChange::upsert(entity(id(2))),
        ];
        for entity_id in 3..=7 {
            changes.push(RenderChange::upsert(light_entity(
                id(entity_id),
                LightKind::Directional,
                identity,
                [1.0; 3],
                if entity_id < 7 { 0.0 } else { 1.0 },
            )));
        }
        let mut scene = RenderScene::new(NonZeroU32::new(7).unwrap());
        scene.apply(&extraction(1, 0, 1, changes)).unwrap();

        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::DirectionalLightCapacityExceeded {
                actual: 5,
                limit: 4,
            })
        ));
    }

    #[test]
    fn fifth_point_light_definition_is_rejected_before_gpu_submission() {
        let identity = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut changes = vec![
            RenderChange::upsert(camera(id(1))),
            RenderChange::upsert(entity(id(2))),
        ];
        for entity_id in 3..=7 {
            changes.push(RenderChange::upsert(light_entity(
                id(entity_id),
                LightKind::Point,
                identity,
                [1.0; 3],
                if entity_id < 7 { 0.0 } else { 1.0 },
            )));
        }
        let mut scene = RenderScene::new(NonZeroU32::new(7).unwrap());
        scene.apply(&extraction(1, 0, 1, changes)).unwrap();

        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::PointLightCapacityExceeded {
                actual: 5,
                limit: 4,
            })
        ));
    }

    #[test]
    fn active_point_light_requires_a_finite_gpu_position() {
        let mut out_of_range = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        out_of_range[12] = f64::MAX;
        let mut scene = RenderScene::new(NonZeroU32::new(3).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(entity(id(2))),
                    RenderChange::upsert(light_entity(
                        id(3),
                        LightKind::Point,
                        out_of_range,
                        [1.0; 3],
                        1.0,
                    )),
                ],
            ))
            .unwrap();

        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::GpuTransformOutOfRange { entity_id })
                if entity_id == id(3)
        ));
    }

    #[test]
    fn active_directional_light_requires_a_non_degenerate_positive_z_axis() {
        let degenerate = [
            1.0, 0.0, 0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 1.0,
        ];
        let mut scene = RenderScene::new(NonZeroU32::new(3).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(entity(id(2))),
                    RenderChange::upsert(light_entity(
                        id(3),
                        LightKind::Directional,
                        degenerate,
                        [1.0; 3],
                        1.0,
                    )),
                ],
            ))
            .unwrap();

        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::DirectionalLightDirectionInvalid { entity_id })
                if entity_id == id(3)
        ));
    }

    #[test]
    fn unavailable_asset_without_an_explicit_primitive_proxy_is_typed() {
        let key = AssetMeshKey {
            content_hash: ContentHash::from_bytes([7; 32]),
            mesh_index: 3,
        };
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(asset_entity(id(2), key)),
                ],
            ))
            .unwrap();
        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::AssetUnavailable {
                entity_id,
                key: missing,
            }) if entity_id == id(2) && missing == key
        ));
    }

    #[test]
    fn plane_geometry_preserves_all_primitive_dimensions_in_the_model() {
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(primitive_entity(
                        id(2),
                        PrimitiveShape::Plane,
                        [2.0, 3.0, 4.0],
                    )),
                ],
            ))
            .unwrap();

        let prepared = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None)
            .unwrap();
        assert_eq!(prepared.draws.len(), 1);
        let draw = &prepared.draws[0];
        assert_eq!(draw.geometry, PreparedGeometry::Plane);
        assert_exact_f32(draw.model[0], 2.0);
        assert_exact_f32(draw.model[5], 3.0);
        assert_exact_f32(draw.model[10], 4.0);
    }

    #[test]
    fn sphere_geometry_preserves_all_bounding_diameters_in_the_model() {
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(primitive_entity(
                        id(2),
                        PrimitiveShape::Sphere,
                        [2.0, 3.0, 4.0],
                    )),
                ],
            ))
            .unwrap();

        let prepared = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None)
            .unwrap();
        assert_eq!(prepared.draws.len(), 1);
        let draw = &prepared.draws[0];
        assert_eq!(draw.geometry, PreparedGeometry::Sphere);
        assert_exact_f32(draw.model[0], 2.0);
        assert_exact_f32(draw.model[5], 3.0);
        assert_exact_f32(draw.model[10], 4.0);
    }

    #[test]
    fn unavailable_asset_uses_its_exact_explicit_sphere_fallback() {
        let key = AssetMeshKey {
            content_hash: ContentHash::from_bytes([9; 32]),
            mesh_index: 5,
        };
        let primitive = PrimitiveComponent {
            shape: PrimitiveShape::Sphere,
            dimensions: PositiveVec3 {
                x: PositiveF32::new(2.0).unwrap(),
                y: PositiveF32::new(3.0).unwrap(),
                z: PositiveF32::new(4.0).unwrap(),
            },
        };
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(asset_entity_with_primitive(id(2), key, Some(primitive))),
                ],
            ))
            .unwrap();

        let fallback = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None)
            .unwrap();
        assert_eq!(fallback.draws[0].geometry, PreparedGeometry::Sphere);
        assert_eq!(
            fallback.draws[0].imported_face_policy,
            ImportedFacePolicy::Disabled
        );
        assert_exact_f32(fallback.draws[0].model[0], 2.0);
        assert_exact_f32(fallback.draws[0].model[5], 3.0);
        assert_exact_f32(fallback.draws[0].model[10], 4.0);

        let resident = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |resolved| {
                (resolved == key).then_some(asset_material([0.1, 0.2, 0.3, 1.0], 0.4, 0.6))
            })
            .unwrap();
        assert_eq!(resident.draws[0].geometry, PreparedGeometry::Asset(key));
        assert_eq!(
            resident.draws[0].imported_face_policy,
            ImportedFacePolicy::SingleSided
        );
        assert_exact_f32(resident.draws[0].model[0], 1.0);
        assert_exact_f32(resident.draws[0].model[5], 1.0);
        assert_exact_f32(resident.draws[0].model[10], 1.0);
    }

    #[test]
    fn unavailable_asset_uses_its_exact_explicit_plane_fallback() {
        let key = AssetMeshKey {
            content_hash: ContentHash::from_bytes([8; 32]),
            mesh_index: 4,
        };
        let primitive = PrimitiveComponent {
            shape: PrimitiveShape::Plane,
            dimensions: PositiveVec3 {
                x: PositiveF32::new(2.0).unwrap(),
                y: PositiveF32::new(3.0).unwrap(),
                z: PositiveF32::new(4.0).unwrap(),
            },
        };
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(asset_entity_with_primitive(id(2), key, Some(primitive))),
                ],
            ))
            .unwrap();

        let fallback = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None)
            .unwrap();
        assert_eq!(fallback.draws[0].geometry, PreparedGeometry::Plane);
        assert_eq!(
            fallback.draws[0].imported_face_policy,
            ImportedFacePolicy::Disabled
        );
        assert_eq!(
            fallback.draws[0].color.map(f32::to_bits),
            [0.8, 0.8, 0.8, 1.0].map(f32::to_bits)
        );
        assert_eq!(fallback.draws[0].metallic.to_bits(), 0.0_f32.to_bits());
        assert_eq!(fallback.draws[0].roughness.to_bits(), 0.8_f32.to_bits());
        assert_eq!(fallback.draws[0].emissive.map(f32::to_bits), [0; 3]);
        assert!(!fallback.draws[0].imported_vertex_color);
        assert_exact_f32(fallback.draws[0].model[0], 2.0);
        assert_exact_f32(fallback.draws[0].model[5], 3.0);
        assert_exact_f32(fallback.draws[0].model[10], 4.0);

        let resident = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |resolved| {
                (resolved == key).then_some(asset_material([0.1, 0.2, 0.3, 1.0], 0.4, 0.6))
            })
            .unwrap();
        assert_eq!(resident.draws[0].geometry, PreparedGeometry::Asset(key));
        assert!(resident.draws[0].imported_vertex_color);
        assert_exact_f32(resident.draws[0].model[0], 1.0);
        assert_exact_f32(resident.draws[0].model[5], 1.0);
        assert_exact_f32(resident.draws[0].model[10], 1.0);
    }

    #[test]
    fn resident_asset_material_is_used_until_scene_material_overrides_all_values() {
        let key = AssetMeshKey {
            content_hash: ContentHash::from_bytes([10; 32]),
            mesh_index: 6,
        };
        let imported = asset_material([0.1, 0.2, 0.3, 0.4], 0.75, 0.25);
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(asset_entity(id(2), key)),
                ],
            ))
            .unwrap();

        let prepared = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |resolved| {
                (resolved == key).then_some(imported)
            })
            .unwrap();
        assert_eq!(
            prepared.draws[0].color.map(f32::to_bits),
            [0.1, 0.2, 0.3, 0.4].map(f32::to_bits)
        );
        assert_exact_f32(prepared.draws[0].metallic, 0.75);
        assert_exact_f32(prepared.draws[0].roughness, 0.25);
        assert_eq!(prepared.draws[0].emissive.map(f32::to_bits), [0; 3]);
        assert_eq!(
            prepared.draws[0].imported_alpha_coverage,
            ImportedAlphaCoverage::Opaque
        );
        assert_eq!(
            prepared.draws[0].imported_face_policy,
            ImportedFacePolicy::SingleSided
        );
        assert!(prepared.draws[0].imported_vertex_color);

        let unit = |value| UnitF32::new(value).unwrap();
        let scene_material = MaterialComponent {
            base_color: ColorRgba {
                r: unit(0.8),
                g: unit(0.4),
                b: unit(0.2),
                a: unit(0.5),
            },
            metallic: unit(0.0),
            roughness: unit(0.9),
        };
        scene
            .apply(&extraction(
                2,
                1,
                2,
                vec![RenderChange::upsert(asset_entity_with_material(
                    id(2),
                    key,
                    scene_material,
                    2,
                ))],
            ))
            .unwrap();
        let overridden = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| {
                Some(imported)
            })
            .unwrap();
        assert_eq!(
            overridden.draws[0].color.map(f32::to_bits),
            [0.8, 0.4, 0.2, 0.5].map(f32::to_bits)
        );
        assert_exact_f32(overridden.draws[0].metallic, 0.0);
        assert_exact_f32(overridden.draws[0].roughness, 0.9);
        assert_eq!(overridden.draws[0].emissive.map(f32::to_bits), [0; 3]);
        assert_eq!(
            overridden.draws[0].imported_alpha_coverage,
            ImportedAlphaCoverage::Disabled
        );
        assert_eq!(
            overridden.draws[0].imported_face_policy,
            ImportedFacePolicy::Disabled
        );
        assert!(!overridden.draws[0].imported_vertex_color);
    }

    #[test]
    fn imported_face_policy_keeps_pipeline_and_shader_selection_coherent() {
        assert!(ImportedFacePolicy::SingleSided.culls_back_faces());
        assert!(!ImportedFacePolicy::Disabled.culls_back_faces());
        assert!(!ImportedFacePolicy::DoubleSided.culls_back_faces());
        assert_eq!(ImportedFacePolicy::Disabled.flags(), 0);
        assert_eq!(ImportedFacePolicy::SingleSided.flags(), 0);
        assert_eq!(ImportedFacePolicy::DoubleSided.flags(), 8);

        let material = asset_material([0.1, 0.2, 0.3, 1.0], 0.0, 0.8);
        assert_eq!(
            imported_material_selection(true, Some(material)).4,
            ImportedFacePolicy::SingleSided
        );
        assert_eq!(
            imported_material_selection(false, Some(material)).4,
            ImportedFacePolicy::Disabled
        );
        assert_eq!(
            imported_material_selection(true, None).4,
            ImportedFacePolicy::Disabled
        );
        assert_eq!(ImportedShadingModel::MetallicRoughness.flags(), 0);
        assert_eq!(ImportedShadingModel::Unlit.flags(), 16);
        assert_eq!(
            imported_material_selection(true, Some(material)).5,
            ImportedShadingModel::MetallicRoughness
        );
        assert_eq!(
            imported_material_selection(false, Some(material)).5,
            ImportedShadingModel::MetallicRoughness
        );
    }
}
