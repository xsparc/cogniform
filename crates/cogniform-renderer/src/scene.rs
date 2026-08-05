use core::num::{NonZeroU32, NonZeroU64};
use std::collections::{BTreeMap, BTreeSet};

use cogniform_assets::AssetMeshKey;
use cogniform_protocol::{
    CameraComponent, ColorRgba, RenderChange, RenderEntity, RenderExtraction, SceneRevision,
    StableEntityId,
};

use crate::RendererError;

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
        mut resolve_asset: impl FnMut(AssetMeshKey) -> Option<[f32; 4]>,
    ) -> Result<PreparedScene, RendererError> {
        let camera_entity = self
            .entities
            .get(&camera_id)
            .ok_or(RendererError::CameraUnavailable { camera_id })?;
        let camera = camera_entity
            .camera()
            .ok_or(RendererError::CameraUnavailable { camera_id })?;
        let view_projection = camera_view_projection(camera_entity, camera, width, height)?;
        let mut draws = Vec::new();
        let mut id_lookup = BTreeMap::new();
        for (&entity_id, entity) in &self.entities {
            let primitive = entity.primitive();
            let (geometry, imported_color) = if let Some(asset_mesh) = entity.asset_mesh() {
                let key = AssetMeshKey {
                    content_hash: asset_mesh.content_hash,
                    mesh_index: asset_mesh.mesh_index,
                };
                if let Some(color) = resolve_asset(key) {
                    (Ok(PreparedGeometry::Asset(key)), Some(color))
                } else if let Some(primitive) = primitive {
                    (primitive_geometry(entity_id, primitive.shape), None)
                } else {
                    return Err(RendererError::AssetUnavailable { entity_id, key });
                }
            } else if let Some(primitive) = primitive {
                (primitive_geometry(entity_id, primitive.shape), None)
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
            let geometry = geometry?;
            let compact_id = self.compact_ids[&entity_id];
            let mut model = matrix_to_f32(entity.world_transform(), entity_id)?;
            if matches!(geometry, PreparedGeometry::Cuboid | PreparedGeometry::Plane) {
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
            let color = entity.material().map_or_else(
                || imported_color.unwrap_or([0.8, 0.8, 0.8, 1.0]),
                |material| color_values(material.base_color),
            );
            draws.push(PreparedDraw {
                geometry,
                model,
                view_projection,
                color,
                compact_id: compact_id.get(),
            });
            id_lookup.insert(compact_id.get(), entity_id);
        }
        Ok(PreparedScene { draws, id_lookup })
    }
}

pub(crate) struct PreparedScene {
    pub(crate) draws: Vec<PreparedDraw>,
    pub(crate) id_lookup: BTreeMap<u32, StableEntityId>,
}

pub(crate) struct PreparedDraw {
    pub(crate) geometry: PreparedGeometry,
    pub(crate) model: [f32; 16],
    pub(crate) view_projection: [f32; 16],
    pub(crate) color: [f32; 4],
    pub(crate) compact_id: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PreparedGeometry {
    Cuboid,
    Plane,
    Asset(AssetMeshKey),
}

fn primitive_geometry(
    entity_id: StableEntityId,
    shape: cogniform_protocol::PrimitiveShape,
) -> Result<PreparedGeometry, RendererError> {
    match shape {
        cogniform_protocol::PrimitiveShape::Cuboid => Ok(PreparedGeometry::Cuboid),
        cogniform_protocol::PrimitiveShape::Plane => Ok(PreparedGeometry::Plane),
        cogniform_protocol::PrimitiveShape::Sphere => {
            Err(RendererError::UnsupportedPrimitive { entity_id, shape })
        }
    }
}

fn color_values(color: ColorRgba) -> [f32; 4] {
    [color.r.get(), color.g.get(), color.b.get(), color.a.get()]
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
        AssetMeshComponent, CameraComponent, ColorRgba, ContentHash, MaterialComponent,
        PositiveF32, PositiveVec3, PrimitiveComponent, PrimitiveShape, RenderComponents, UnitF32,
    };

    fn id(value: u128) -> StableEntityId {
        StableEntityId::new(value).unwrap()
    }

    fn assert_exact_f32(actual: f32, expected: f32) {
        assert_eq!(actual.to_bits(), expected.to_bits());
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
    fn sphere_remains_a_typed_unsupported_primitive() {
        let mut scene = RenderScene::new(NonZeroU32::new(2).unwrap());
        scene
            .apply(&extraction(
                1,
                0,
                1,
                vec![
                    RenderChange::upsert(camera(id(1))),
                    RenderChange::upsert(primitive_entity(id(2), PrimitiveShape::Sphere, [1.0; 3])),
                ],
            ))
            .unwrap();

        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::UnsupportedPrimitive {
                entity_id,
                shape: PrimitiveShape::Sphere,
            }) if entity_id == id(2)
        ));
    }

    #[test]
    fn unavailable_asset_sphere_fallback_remains_typed() {
        let key = AssetMeshKey {
            content_hash: ContentHash::from_bytes([9; 32]),
            mesh_index: 5,
        };
        let primitive = PrimitiveComponent {
            shape: PrimitiveShape::Sphere,
            dimensions: PositiveVec3 {
                x: PositiveF32::new(1.0).unwrap(),
                y: PositiveF32::new(1.0).unwrap(),
                z: PositiveF32::new(1.0).unwrap(),
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

        assert!(matches!(
            scene.prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |_| None),
            Err(RendererError::UnsupportedPrimitive {
                entity_id,
                shape: PrimitiveShape::Sphere,
            }) if entity_id == id(2)
        ));
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
        assert_exact_f32(fallback.draws[0].model[0], 2.0);
        assert_exact_f32(fallback.draws[0].model[5], 3.0);
        assert_exact_f32(fallback.draws[0].model[10], 4.0);

        let resident = scene
            .prepare(id(1), 64, 64, NonZeroU32::new(1).unwrap(), |resolved| {
                (resolved == key).then_some([0.1, 0.2, 0.3, 1.0])
            })
            .unwrap();
        assert_eq!(resident.draws[0].geometry, PreparedGeometry::Asset(key));
        assert_exact_f32(resident.draws[0].model[0], 1.0);
        assert_exact_f32(resident.draws[0].model[5], 1.0);
        assert_exact_f32(resident.draws[0].model[10], 1.0);
    }
}
