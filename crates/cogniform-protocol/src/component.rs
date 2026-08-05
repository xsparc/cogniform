use serde::{Deserialize, Serialize};

use crate::{
    AssetMeshComponent, DiagnosticCode, FiniteF32, NonNegativeF32, PositiveF32, SceneText, UnitF32,
    ValidationError,
};

/// A finite three-dimensional vector.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Vec3 {
    /// X coordinate.
    pub x: FiniteF32,
    /// Y coordinate.
    pub y: FiniteF32,
    /// Z coordinate.
    pub z: FiniteF32,
}

/// A three-dimensional vector whose coordinates are all positive.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositiveVec3 {
    /// Positive X coordinate.
    pub x: PositiveF32,
    /// Positive Y coordinate.
    pub y: PositiveF32,
    /// Positive Z coordinate.
    pub z: PositiveF32,
}

/// Finite quaternion stored in explicit XYZW order.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quaternion {
    /// X imaginary component.
    pub x: FiniteF32,
    /// Y imaginary component.
    pub y: FiniteF32,
    /// Z imaginary component.
    pub z: FiniteF32,
    /// W real component.
    pub w: FiniteF32,
}

impl Quaternion {
    fn is_non_zero(self) -> bool {
        let values = [self.x.get(), self.y.get(), self.z.get(), self.w.get()];
        values
            .into_iter()
            .map(f64::from)
            .map(|value| value * value)
            .sum::<f64>()
            > 0.0
    }
}

/// Linear red, green, and blue color channels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorRgb {
    /// Red channel.
    pub r: UnitF32,
    /// Green channel.
    pub g: UnitF32,
    /// Blue channel.
    pub b: UnitF32,
}

/// Linear red, green, blue, and alpha color channels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorRgba {
    /// Red channel.
    pub r: UnitF32,
    /// Green channel.
    pub g: UnitF32,
    /// Blue channel.
    pub b: UnitF32,
    /// Alpha channel.
    pub a: UnitF32,
}

/// Human-readable entity name component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NameComponent {
    /// Validated scene text.
    pub value: SceneText,
}

/// Authoritative local transform component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalTransform {
    /// Translation in world units relative to the parent.
    pub translation: Vec3,
    /// Non-zero local rotation quaternion in XYZW order.
    pub rotation: Quaternion,
    /// Positive local scale.
    pub scale: PositiveVec3,
}

/// Built-in primitive shapes available before asset loading lands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrimitiveShape {
    /// Unit cuboid scaled by explicit positive dimensions.
    Cuboid,
    /// Centered unit XY plane with a positive-Z front, scaled by explicit
    /// positive XYZ dimensions while remaining at local Z = 0.
    Plane,
    /// Unit UV sphere scaled by explicit positive dimensions.
    Sphere,
}

/// Built-in primitive geometry component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimitiveComponent {
    /// Primitive topology.
    pub shape: PrimitiveShape,
    /// Positive XYZ dimensions in world units.
    pub dimensions: PositiveVec3,
}

/// Physically based material inputs used by the primitive MVP.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialComponent {
    /// Linear base color and opacity.
    pub base_color: ColorRgba,
    /// Metallic response in the inclusive unit interval.
    pub metallic: UnitF32,
    /// Perceptual roughness in the inclusive unit interval.
    pub roughness: UnitF32,
}

/// Perspective camera projection component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CameraComponent {
    /// Vertical field of view in radians, strictly between zero and pi.
    pub vertical_fov_radians: PositiveF32,
    /// Positive near clipping distance.
    pub near: PositiveF32,
    /// Positive far clipping distance, greater than `near`.
    pub far: PositiveF32,
}

/// Baseline light types supported by the public scene contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LightKind {
    /// Directional light whose transform supplies its direction.
    Directional,
    /// Omnidirectional point light.
    Point,
}

/// Baseline light component.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LightComponent {
    /// Light emission model.
    pub kind: LightKind,
    /// Linear light color.
    pub color: ColorRgb,
    /// Non-negative implementation-defined luminous intensity.
    pub intensity: NonNegativeF32,
}

/// Stable version-one component discriminants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentKind {
    /// Human-readable name.
    Name,
    /// Authoritative local transform.
    LocalTransform,
    /// Built-in primitive geometry.
    Primitive,
    /// Primitive material.
    Material,
    /// Perspective camera.
    Camera,
    /// Baseline light.
    Light,
    /// Immutable hash-addressed decoded mesh selection.
    AssetMesh,
}

/// Version-one component values carried by scene operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "component",
    content = "value",
    rename_all = "snake_case",
    deny_unknown_fields
)]
pub enum ComponentValue {
    /// Human-readable name value.
    Name(NameComponent),
    /// Local transform value.
    LocalTransform(LocalTransform),
    /// Primitive geometry value.
    Primitive(PrimitiveComponent),
    /// Material value.
    Material(MaterialComponent),
    /// Camera value.
    Camera(CameraComponent),
    /// Light value.
    Light(LightComponent),
    /// Immutable hash-addressed decoded mesh selection.
    AssetMesh(AssetMeshComponent),
}

impl ComponentValue {
    /// Returns the stable component discriminant.
    #[must_use]
    pub const fn kind(&self) -> ComponentKind {
        match self {
            Self::Name(_) => ComponentKind::Name,
            Self::LocalTransform(_) => ComponentKind::LocalTransform,
            Self::Primitive(_) => ComponentKind::Primitive,
            Self::Material(_) => ComponentKind::Material,
            Self::Camera(_) => ComponentKind::Camera,
            Self::Light(_) => ComponentKind::Light,
            Self::AssetMesh(_) => ComponentKind::AssetMesh,
        }
    }

    pub(crate) fn text_bytes(&self) -> usize {
        match self {
            Self::Name(name) => name.value.len_bytes(),
            _ => 0,
        }
    }

    pub(crate) fn logical_size_bytes(&self) -> u64 {
        const TAG_BYTES: u64 = 1;
        match self {
            Self::Name(name) => TAG_BYTES
                .saturating_add(4)
                .saturating_add(u64::try_from(name.value.len_bytes()).unwrap_or(u64::MAX)),
            Self::LocalTransform(_) => TAG_BYTES + (10 * 4),
            Self::Primitive(_) => TAG_BYTES + 1 + (3 * 4),
            Self::Material(_) => TAG_BYTES + (6 * 4),
            Self::Camera(_) => TAG_BYTES + (3 * 4),
            Self::Light(_) => TAG_BYTES + 1 + (4 * 4),
            Self::AssetMesh(_) => TAG_BYTES + 32 + 4,
        }
    }

    pub(crate) fn validate(&self) -> Result<(), ValidationError> {
        match self {
            Self::LocalTransform(transform) if !transform.rotation.is_non_zero() => Err(
                ValidationError::new(DiagnosticCode::InvalidComponentValue, "component.rotation"),
            ),
            Self::Camera(camera)
                if camera.vertical_fov_radians.get() >= core::f32::consts::PI
                    || camera.far.get() <= camera.near.get() =>
            {
                Err(ValidationError::new(
                    DiagnosticCode::InvalidComponentValue,
                    "component.camera",
                ))
            }
            _ => Ok(()),
        }
    }
}
