//! Exact deterministic built-in procedure contracts.

use core::num::NonZeroU32;

use cogniform_procedural::{
    BuiltinProcedure, CuboidGrid, ProcedureError, ProcedureLimits, ProcedureRequest, execute,
};
use cogniform_protocol::{
    ColorRgba, DeliverySemantic, FiniteF32, IdempotencyKey, MaterialComponent, PatchBudget,
    PositiveF32, PositiveVec3, ProcedureId, RuntimeLimits, SceneRevision, TransactionId, UnitF32,
    Vec3,
};

fn request(seed: u64) -> ProcedureRequest {
    ProcedureRequest {
        procedure_id: ProcedureId::new(11).unwrap(),
        seed,
        transaction_id: TransactionId::new(12).unwrap(),
        idempotency_key: IdempotencyKey::new(13).unwrap(),
        base_revision: SceneRevision::INITIAL,
        delivery: DeliverySemantic::MustApply,
        patch_budget: PatchBudget::default(),
        procedure_limits: ProcedureLimits::default(),
        procedure: BuiltinProcedure::CuboidGrid(CuboidGrid {
            rows: NonZeroU32::new(2).unwrap(),
            columns: NonZeroU32::new(3).unwrap(),
            origin: Vec3 {
                x: finite(-2.0),
                y: finite(0.5),
                z: finite(-1.0),
            },
            spacing_x: positive(1.5),
            spacing_z: positive(2.0),
            dimensions: PositiveVec3 {
                x: positive(1.0),
                y: positive(1.0),
                z: positive(1.0),
            },
            material: MaterialComponent {
                base_color: ColorRgba {
                    r: unit(0.2),
                    g: unit(0.4),
                    b: unit(0.8),
                    a: unit(1.0),
                },
                metallic: unit(0.0),
                roughness: unit(0.7),
            },
        }),
    }
}

#[test]
fn identical_parameters_and_seed_produce_exact_canonical_patch_bytes() {
    let limits = RuntimeLimits::default();
    let first = execute(&request(99), &limits).unwrap();
    let second = execute(&request(99), &limits).unwrap();
    assert_eq!(first.entity_ids, second.entity_ids);
    assert_eq!(first.patch, second.patch);
    assert_eq!(
        first.patch.to_canonical_json(&limits).unwrap(),
        second.patch.to_canonical_json(&limits).unwrap()
    );
    assert_eq!(first.entity_ids.len(), 6);
}

#[test]
fn seed_changes_derived_identity_without_changing_output_shape() {
    let limits = RuntimeLimits::default();
    let first = execute(&request(99), &limits).unwrap();
    let second = execute(&request(100), &limits).unwrap();
    assert_ne!(first.entity_ids, second.entity_ids);
    assert_eq!(first.patch.operations.len(), second.patch.operations.len());
}

#[test]
fn entity_budget_is_checked_before_output_allocation() {
    let limits = RuntimeLimits::default();
    let mut request = request(99);
    request.procedure_limits.max_entities = NonZeroU32::new(5).unwrap();
    assert!(matches!(
        execute(&request, &limits),
        Err(ProcedureError::EntityLimitExceeded {
            actual: 6,
            limit: 5
        })
    ));
}

fn finite(value: f32) -> FiniteF32 {
    FiniteF32::new(value).unwrap()
}

fn positive(value: f32) -> PositiveF32 {
    PositiveF32::new(value).unwrap()
}

fn unit(value: f32) -> UnitF32 {
    UnitF32::new(value).unwrap()
}
