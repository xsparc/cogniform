use core::fmt;

use cogniform_protocol::{ComponentValue, LightKind, PrimitiveShape, StableEntityId};
use sha2::{Digest, Sha256};

use crate::WorldSnapshot;

const LOGICAL_HASH_DOMAIN: &[u8] = b"cogniform.logical-scene\0";
const LOGICAL_HASH_FORMAT_VERSION: u16 = 1;

/// A SHA-256 digest of versioned, canonical logical scene state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LogicalSceneHash([u8; 32]);

impl LogicalSceneHash {
    /// Hash value used for the empty predecessor of an append-only chain.
    pub const ZERO: Self = Self([0; 32]);

    /// Constructs a digest from its exact bytes.
    #[must_use]
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for LogicalSceneHash {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

impl WorldSnapshot {
    /// Computes the version-one canonical logical scene hash.
    ///
    /// Entities are already ordered by stable ID and component values by their
    /// versioned type key. Integers and raw normalized `f32` bits are encoded
    /// big-endian. Revision, ECS handles, cached transforms, generations,
    /// timing, and other operational state are excluded.
    #[must_use]
    pub fn logical_hash(&self) -> LogicalSceneHash {
        let mut hasher = Sha256::new();
        hasher.update(LOGICAL_HASH_DOMAIN);
        hasher.update(LOGICAL_HASH_FORMAT_VERSION.to_be_bytes());
        update_u32(
            &mut hasher,
            u32::try_from(self.entities().len()).expect("world entity bound fits u32"),
        );
        for entity in self.entities() {
            update_entity_id(&mut hasher, entity.entity_id());
            match entity.parent_id() {
                Some(parent_id) => {
                    hasher.update([1]);
                    update_entity_id(&mut hasher, parent_id);
                }
                None => hasher.update([0]),
            }
            update_u32(
                &mut hasher,
                u32::try_from(entity.components().len()).expect("component bound fits u32"),
            );
            for component in entity.components() {
                update_component(&mut hasher, component);
            }
        }
        LogicalSceneHash(hasher.finalize().into())
    }
}

fn update_entity_id(hasher: &mut Sha256, entity_id: StableEntityId) {
    hasher.update(entity_id.get().to_be_bytes());
}

fn update_u32(hasher: &mut Sha256, value: u32) {
    hasher.update(value.to_be_bytes());
}

fn update_f32(hasher: &mut Sha256, value: f32) {
    hasher.update(value.to_bits().to_be_bytes());
}

fn update_component(hasher: &mut Sha256, component: &ComponentValue) {
    match component {
        ComponentValue::Name(value) => {
            hasher.update([1]);
            let bytes = value.value.as_str().as_bytes();
            update_u32(
                hasher,
                u32::try_from(bytes.len()).expect("validated scene text length fits u32"),
            );
            hasher.update(bytes);
        }
        ComponentValue::LocalTransform(value) => {
            hasher.update([2]);
            for number in [
                value.translation.x.get(),
                value.translation.y.get(),
                value.translation.z.get(),
                value.rotation.x.get(),
                value.rotation.y.get(),
                value.rotation.z.get(),
                value.rotation.w.get(),
                value.scale.x.get(),
                value.scale.y.get(),
                value.scale.z.get(),
            ] {
                update_f32(hasher, number);
            }
        }
        ComponentValue::Primitive(value) => {
            hasher.update([3]);
            hasher.update([match value.shape {
                PrimitiveShape::Cuboid => 1,
                PrimitiveShape::Plane => 2,
                PrimitiveShape::Sphere => 3,
            }]);
            for number in [
                value.dimensions.x.get(),
                value.dimensions.y.get(),
                value.dimensions.z.get(),
            ] {
                update_f32(hasher, number);
            }
        }
        ComponentValue::Material(value) => {
            hasher.update([4]);
            for number in [
                value.base_color.r.get(),
                value.base_color.g.get(),
                value.base_color.b.get(),
                value.base_color.a.get(),
                value.metallic.get(),
                value.roughness.get(),
            ] {
                update_f32(hasher, number);
            }
        }
        ComponentValue::Camera(value) => {
            hasher.update([5]);
            for number in [
                value.vertical_fov_radians.get(),
                value.near.get(),
                value.far.get(),
            ] {
                update_f32(hasher, number);
            }
        }
        ComponentValue::Light(value) => {
            hasher.update([6]);
            hasher.update([match value.kind {
                LightKind::Directional => 1,
                LightKind::Point => 2,
            }]);
            for number in [
                value.color.r.get(),
                value.color.g.get(),
                value.color.b.get(),
                value.intensity.get(),
            ] {
                update_f32(hasher, number);
            }
        }
        ComponentValue::AssetMesh(value) => {
            hasher.update([7]);
            hasher.update(value.content_hash.as_bytes());
            update_u32(hasher, value.mesh_index);
        }
    }
}
