use core::{fmt, num::NonZeroU64};

use crate::{
    AssetMeshComponent, CameraComponent, LightComponent, MaterialComponent, PrimitiveComponent,
    SceneRevision, StableEntityId,
};

/// Backend-neutral render record extracted from one authoritative entity.
///
/// The matrix is column-major derived world state. GPU handles and compact
/// renderer-local identities are deliberately absent.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderEntity {
    entity_id: StableEntityId,
    world_transform: [f64; 16],
    world_transform_generation: u64,
    components: RenderComponents,
}

/// Compact render-relevant component bundle for one extracted entity.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct RenderComponents {
    /// Built-in primitive geometry, including an explicit asset fallback proxy.
    pub primitive: Option<PrimitiveComponent>,
    /// Optional scene-level material override.
    pub material: Option<MaterialComponent>,
    /// Optional perspective camera.
    pub camera: Option<CameraComponent>,
    /// Optional baseline light.
    pub light: Option<LightComponent>,
    /// Optional immutable hash-addressed mesh selection.
    pub asset_mesh: Option<AssetMeshComponent>,
}

impl RenderEntity {
    /// Builds a render record after checking its derived matrix and payload.
    pub fn new(
        entity_id: StableEntityId,
        world_transform: [f64; 16],
        world_transform_generation: u64,
        components: RenderComponents,
    ) -> Result<Self, RenderContractError> {
        if !world_transform.iter().all(|value| value.is_finite()) {
            return Err(RenderContractError::NonFiniteWorldTransform { entity_id });
        }
        if components.primitive.is_none()
            && components.material.is_none()
            && components.camera.is_none()
            && components.light.is_none()
            && components.asset_mesh.is_none()
        {
            return Err(RenderContractError::EmptyRenderEntity { entity_id });
        }
        Ok(Self {
            entity_id,
            world_transform,
            world_transform_generation,
            components,
        })
    }

    /// Returns the stable world identity.
    #[must_use]
    pub const fn entity_id(&self) -> StableEntityId {
        self.entity_id
    }

    /// Returns the column-major derived world matrix.
    #[must_use]
    pub const fn world_transform(&self) -> &[f64; 16] {
        &self.world_transform
    }

    /// Returns the world propagation generation that produced the matrix.
    #[must_use]
    pub const fn world_transform_generation(&self) -> u64 {
        self.world_transform_generation
    }

    /// Returns the built-in primitive, when present.
    #[must_use]
    pub const fn primitive(&self) -> Option<PrimitiveComponent> {
        self.components.primitive
    }

    /// Returns the material, when present.
    #[must_use]
    pub const fn material(&self) -> Option<MaterialComponent> {
        self.components.material
    }

    /// Returns the perspective camera, when present.
    #[must_use]
    pub const fn camera(&self) -> Option<CameraComponent> {
        self.components.camera
    }

    /// Returns the light, when present.
    #[must_use]
    pub const fn light(&self) -> Option<LightComponent> {
        self.components.light
    }

    /// Returns the hash-addressed mesh selection, when present.
    #[must_use]
    pub const fn asset_mesh(&self) -> Option<AssetMeshComponent> {
        self.components.asset_mesh
    }
}

/// One compact mutation to renderer-owned extracted state.
#[derive(Debug, Clone, PartialEq)]
pub enum RenderChange {
    /// Insert or completely replace one current render record.
    Upsert(Box<RenderEntity>),
    /// Remove any renderer-owned state for one stable entity.
    Remove(StableEntityId),
}

impl RenderChange {
    /// Creates a complete record replacement.
    #[must_use]
    pub fn upsert(entity: RenderEntity) -> Self {
        Self::Upsert(Box::new(entity))
    }

    /// Creates a stable-identity removal.
    #[must_use]
    pub const fn remove(entity_id: StableEntityId) -> Self {
        Self::Remove(entity_id)
    }

    /// Returns the stable identity affected by this change.
    #[must_use]
    pub const fn entity_id(&self) -> StableEntityId {
        match self {
            Self::Upsert(entity) => entity.entity_id(),
            Self::Remove(entity_id) => *entity_id,
        }
    }
}

/// Immutable, ordered packet connecting world state to renderer state.
#[derive(Debug, Clone, PartialEq)]
pub struct RenderExtraction {
    generation: NonZeroU64,
    base_revision: SceneRevision,
    scene_revision: SceneRevision,
    changes: Vec<RenderChange>,
}

impl RenderExtraction {
    /// Builds a packet whose changes are strictly ordered by stable identity.
    pub fn new(
        generation: NonZeroU64,
        base_revision: SceneRevision,
        scene_revision: SceneRevision,
        changes: Vec<RenderChange>,
    ) -> Result<Self, RenderContractError> {
        if scene_revision < base_revision {
            return Err(RenderContractError::RevisionRegressed {
                base_revision,
                scene_revision,
            });
        }
        for pair in changes.windows(2) {
            if pair[0].entity_id() >= pair[1].entity_id() {
                return Err(RenderContractError::ChangesNotStrictlyOrdered {
                    previous: pair[0].entity_id(),
                    current: pair[1].entity_id(),
                });
            }
        }
        Ok(Self {
            generation,
            base_revision,
            scene_revision,
            changes,
        })
    }

    /// Returns the monotonic extraction generation.
    #[must_use]
    pub const fn generation(&self) -> NonZeroU64 {
        self.generation
    }

    /// Returns the renderer revision this packet must extend.
    #[must_use]
    pub const fn base_revision(&self) -> SceneRevision {
        self.base_revision
    }

    /// Returns the exact fully extracted world revision.
    #[must_use]
    pub const fn scene_revision(&self) -> SceneRevision {
        self.scene_revision
    }

    /// Returns compact changes in stable-identity order.
    #[must_use]
    pub fn changes(&self) -> &[RenderChange] {
        &self.changes
    }
}

/// Invalid backend-neutral render data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RenderContractError {
    /// A derived transform contained NaN or infinity.
    NonFiniteWorldTransform {
        /// Affected stable entity.
        entity_id: StableEntityId,
    },
    /// A record contained no render-domain component.
    EmptyRenderEntity {
        /// Affected stable entity.
        entity_id: StableEntityId,
    },
    /// A packet attempted to move its scene revision backwards.
    RevisionRegressed {
        /// Required renderer base revision.
        base_revision: SceneRevision,
        /// Packet target revision.
        scene_revision: SceneRevision,
    },
    /// Packet changes were duplicated or not in stable-identity order.
    ChangesNotStrictlyOrdered {
        /// Previous identity in the packet.
        previous: StableEntityId,
        /// Current identity in the packet.
        current: StableEntityId,
    },
}

impl fmt::Display for RenderContractError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonFiniteWorldTransform { entity_id } => {
                write!(
                    formatter,
                    "render entity {entity_id} has a non-finite world transform"
                )
            }
            Self::EmptyRenderEntity { entity_id } => {
                write!(
                    formatter,
                    "render entity {entity_id} has no render components"
                )
            }
            Self::RevisionRegressed {
                base_revision,
                scene_revision,
            } => write!(
                formatter,
                "render extraction regressed from revision {} to {}",
                base_revision.get(),
                scene_revision.get()
            ),
            Self::ChangesNotStrictlyOrdered { previous, current } => write!(
                formatter,
                "render changes are not strictly ordered: {previous} then {current}"
            ),
        }
    }
}

impl std::error::Error for RenderContractError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u128) -> StableEntityId {
        StableEntityId::new(value).unwrap()
    }

    #[test]
    fn extraction_rejects_duplicate_or_regressing_changes() {
        let generation = NonZeroU64::new(1).unwrap();
        assert!(matches!(
            RenderExtraction::new(
                generation,
                SceneRevision::new(2),
                SceneRevision::new(1),
                Vec::new(),
            ),
            Err(RenderContractError::RevisionRegressed { .. })
        ));
        assert!(matches!(
            RenderExtraction::new(
                generation,
                SceneRevision::INITIAL,
                SceneRevision::new(1),
                vec![RenderChange::remove(id(1)), RenderChange::remove(id(1))],
            ),
            Err(RenderContractError::ChangesNotStrictlyOrdered { .. })
        ));
    }
}
