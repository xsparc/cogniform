use cogniform_protocol::LocalTransform;

/// Cached derived transform from entity-local space into world space.
///
/// Matrices are column-major and use 64-bit elements so composing validated
/// 32-bit protocol values has useful overflow headroom. This value is derived
/// state and is intentionally excluded from the canonical logical scene hash.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldTransform {
    matrix: [f64; 16],
    generation: u64,
}

impl WorldTransform {
    pub(crate) const IDENTITY_MATRIX: [f64; 16] = [
        1.0, 0.0, 0.0, 0.0, // column 0
        0.0, 1.0, 0.0, 0.0, // column 1
        0.0, 0.0, 1.0, 0.0, // column 2
        0.0, 0.0, 0.0, 1.0, // column 3
    ];

    pub(crate) const fn identity(generation: u64) -> Self {
        Self {
            matrix: Self::IDENTITY_MATRIX,
            generation,
        }
    }

    pub(crate) fn from_local(local: Option<LocalTransform>, generation: u64) -> Option<Self> {
        local.map_or_else(
            || Some(Self::identity(generation)),
            |local| {
                let x = f64::from(local.rotation.x.get());
                let y = f64::from(local.rotation.y.get());
                let z = f64::from(local.rotation.z.get());
                let w = f64::from(local.rotation.w.get());
                let inverse_norm = (x * x + y * y + z * z + w * w).sqrt().recip();
                if !inverse_norm.is_finite() {
                    return None;
                }
                let x = x * inverse_norm;
                let y = y * inverse_norm;
                let z = z * inverse_norm;
                let w = w * inverse_norm;
                let sx = f64::from(local.scale.x.get());
                let sy = f64::from(local.scale.y.get());
                let sz = f64::from(local.scale.z.get());

                let matrix = [
                    (1.0 - 2.0 * (y * y + z * z)) * sx,
                    2.0 * (x * y + z * w) * sx,
                    2.0 * (x * z - y * w) * sx,
                    0.0,
                    2.0 * (x * y - z * w) * sy,
                    (1.0 - 2.0 * (x * x + z * z)) * sy,
                    2.0 * (y * z + x * w) * sy,
                    0.0,
                    2.0 * (x * z + y * w) * sz,
                    2.0 * (y * z - x * w) * sz,
                    (1.0 - 2.0 * (x * x + y * y)) * sz,
                    0.0,
                    f64::from(local.translation.x.get()),
                    f64::from(local.translation.y.get()),
                    f64::from(local.translation.z.get()),
                    1.0,
                ];
                matrix
                    .iter()
                    .all(|value| value.is_finite())
                    .then_some(Self { matrix, generation })
            },
        )
    }

    pub(crate) fn compose(self, local: Self, generation: u64) -> Option<Self> {
        let mut matrix = [0.0; 16];
        for column in 0..4 {
            for row in 0..4 {
                let value = (0..4).fold(0.0, |sum, index| {
                    sum + self.matrix[index * 4 + row] * local.matrix[column * 4 + index]
                });
                if !value.is_finite() {
                    return None;
                }
                matrix[column * 4 + row] = value;
            }
        }
        Some(Self { matrix, generation })
    }

    /// Returns the column-major world matrix.
    #[must_use]
    pub const fn matrix(&self) -> &[f64; 16] {
        &self.matrix
    }

    /// Returns the propagation generation that last recomputed this entity.
    #[must_use]
    pub const fn generation(self) -> u64 {
        self.generation
    }
}
