use core::num::NonZeroU32;
use std::collections::BTreeSet;

use cogniform_protocol::{
    ComponentValue, ConflictPolicy, CreateEntity, DeliverySemantic, FiniteF32, IdempotencyKey,
    LocalTransform, MaterialComponent, PatchBudget, PositiveF32, PositiveVec3, PrimitiveComponent,
    ProcedureId, Quaternion, RuntimeLimits, SceneOperation, ScenePatch, SceneRevision,
    SchemaVersion, StableEntityId, TransactionId, Vec3,
};
use sha2::{Digest, Sha256};

use crate::ProcedureError;

const ID_DOMAIN: &[u8] = b"cogniform.builtin-procedure.entity-id\0";

/// Explicit output bounds for a built-in procedure invocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProcedureLimits {
    /// Maximum entities one invocation may emit.
    pub max_entities: NonZeroU32,
    /// Maximum deterministic collision-resolution attempts per entity.
    pub max_id_derivation_attempts: NonZeroU32,
}

impl Default for ProcedureLimits {
    fn default() -> Self {
        Self {
            max_entities: NonZeroU32::new(256).expect("constant is non-zero"),
            max_id_derivation_attempts: NonZeroU32::new(16).expect("constant is non-zero"),
        }
    }
}

/// Parameters for one row-major grid of cuboids.
#[derive(Debug, Clone, PartialEq)]
pub struct CuboidGrid {
    /// Number of rows along positive Z.
    pub rows: NonZeroU32,
    /// Number of columns along positive X.
    pub columns: NonZeroU32,
    /// Position of the first grid cell.
    pub origin: Vec3,
    /// Positive center-to-center X and Z spacing.
    pub spacing_x: PositiveF32,
    /// Positive center-to-center X and Z spacing.
    pub spacing_z: PositiveF32,
    /// Positive cuboid dimensions copied into every emitted entity.
    pub dimensions: PositiveVec3,
    /// Material copied into every emitted entity.
    pub material: MaterialComponent,
}

/// Approved pure built-in procedure kinds.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub enum BuiltinProcedure {
    /// Emit a deterministic row-major grid of cuboids.
    CuboidGrid(CuboidGrid),
}

/// Complete explicit input to one pure procedure invocation.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureRequest {
    /// Stable identity of this invocation.
    pub procedure_id: ProcedureId,
    /// Explicit deterministic seed; no ambient entropy is consulted.
    pub seed: u64,
    /// Transaction identity for the resulting ordinary patch.
    pub transaction_id: TransactionId,
    /// Idempotency identity for the resulting ordinary patch.
    pub idempotency_key: IdempotencyKey,
    /// Authoritative revision against which the patch is prepared.
    pub base_revision: SceneRevision,
    /// Queue semantics carried into the resulting patch.
    pub delivery: DeliverySemantic,
    /// Caller-declared ordinary patch budget.
    pub patch_budget: PatchBudget,
    /// Procedure-specific resource limits.
    pub procedure_limits: ProcedureLimits,
    /// Selected built-in and its complete parameters.
    pub procedure: BuiltinProcedure,
}

/// Deterministic procedure output ready for normal validation and application.
#[derive(Debug, Clone, PartialEq)]
pub struct ProcedureArtifact {
    /// Ordinary atomic scene patch produced by the procedure.
    pub patch: ScenePatch,
    /// Entity identities in deterministic row-major emission order.
    pub entity_ids: Vec<StableEntityId>,
}

/// Executes one pure built-in procedure under explicit public runtime bounds.
pub fn execute(
    request: &ProcedureRequest,
    runtime_limits: &RuntimeLimits,
) -> Result<ProcedureArtifact, ProcedureError> {
    match &request.procedure {
        BuiltinProcedure::CuboidGrid(grid) => execute_grid(request, grid, runtime_limits),
    }
}

fn execute_grid(
    request: &ProcedureRequest,
    grid: &CuboidGrid,
    runtime_limits: &RuntimeLimits,
) -> Result<ProcedureArtifact, ProcedureError> {
    let entity_count = u64::from(grid.rows.get()).saturating_mul(u64::from(grid.columns.get()));
    if entity_count > u64::from(request.procedure_limits.max_entities.get()) {
        return Err(ProcedureError::EntityLimitExceeded {
            actual: entity_count,
            limit: request.procedure_limits.max_entities.get(),
        });
    }
    let operations = entity_count;
    let components = entity_count.saturating_mul(3);
    if operations > u64::from(request.patch_budget.max_operations.get())
        || operations > u64::from(runtime_limits.max_operations.get())
        || components > u64::from(request.patch_budget.max_components.get())
        || components > u64::from(runtime_limits.max_components.get())
        || 3 > runtime_limits.max_components_per_entity.get()
    {
        return Err(ProcedureError::PatchCapacityExceeded {
            operations,
            components,
        });
    }

    let capacity = usize::try_from(entity_count).expect("bounded u32 entity count fits usize");
    let mut emitted = BTreeSet::new();
    let mut entity_ids = Vec::with_capacity(capacity);
    let mut patch_operations = Vec::with_capacity(capacity);
    for row in 0..grid.rows.get() {
        for column in 0..grid.columns.get() {
            let entity_index = row
                .checked_mul(grid.columns.get())
                .and_then(|value| value.checked_add(column))
                .expect("bounded grid index fits u32");
            let entity_id = derive_entity_id(request, entity_index, &mut emitted)?;
            let translation = Vec3 {
                x: grid_coordinate(grid.origin.x, grid.spacing_x, column, entity_index)?,
                y: grid.origin.y,
                z: grid_coordinate(grid.origin.z, grid.spacing_z, row, entity_index)?,
            };
            patch_operations.push(SceneOperation::Create(CreateEntity {
                entity_id,
                components: vec![
                    ComponentValue::LocalTransform(LocalTransform {
                        translation,
                        rotation: identity_rotation(),
                        scale: unit_scale(),
                    }),
                    ComponentValue::Primitive(PrimitiveComponent {
                        shape: cogniform_protocol::PrimitiveShape::Cuboid,
                        dimensions: grid.dimensions,
                    }),
                    ComponentValue::Material(grid.material),
                ],
            }));
            entity_ids.push(entity_id);
        }
    }

    let patch = ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: request.transaction_id,
        idempotency_key: request.idempotency_key,
        base_revision: request.base_revision,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: request.delivery.clone(),
        declared_budget: request.patch_budget,
        operations: patch_operations,
    };
    patch
        .validate_with_limits(runtime_limits)
        .map_err(ProcedureError::InvalidPatch)?;
    Ok(ProcedureArtifact { patch, entity_ids })
}

fn grid_coordinate(
    origin: FiniteF32,
    spacing: PositiveF32,
    offset: u32,
    entity_index: u32,
) -> Result<FiniteF32, ProcedureError> {
    let value = (0..offset).fold(origin.get(), |value, _| value + spacing.get());
    FiniteF32::new(value).map_err(|_| ProcedureError::NonFiniteTransform { entity_index })
}

fn derive_entity_id(
    request: &ProcedureRequest,
    entity_index: u32,
    emitted: &mut BTreeSet<StableEntityId>,
) -> Result<StableEntityId, ProcedureError> {
    for attempt in 0..request.procedure_limits.max_id_derivation_attempts.get() {
        let mut hasher = Sha256::new();
        hasher.update(ID_DOMAIN);
        hasher.update(request.procedure_id.get().to_be_bytes());
        hasher.update(request.seed.to_be_bytes());
        hasher.update(entity_index.to_be_bytes());
        hasher.update(attempt.to_be_bytes());
        let digest: [u8; 32] = hasher.finalize().into();
        let value = u128::from_be_bytes(digest[..16].try_into().expect("slice length is fixed"));
        if let Ok(entity_id) = StableEntityId::new(value)
            && emitted.insert(entity_id)
        {
            return Ok(entity_id);
        }
    }
    Err(ProcedureError::EntityIdDerivationExhausted {
        entity_index,
        attempts: request.procedure_limits.max_id_derivation_attempts.get(),
    })
}

fn identity_rotation() -> Quaternion {
    Quaternion {
        x: finite(0.0),
        y: finite(0.0),
        z: finite(0.0),
        w: finite(1.0),
    }
}

fn unit_scale() -> PositiveVec3 {
    PositiveVec3 {
        x: positive(1.0),
        y: positive(1.0),
        z: positive(1.0),
    }
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).expect("constant is finite")
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).expect("constant is positive")
}
