use core::num::{NonZeroU32, NonZeroU64};
use std::{hint::black_box, time::Instant};

use cogniform_protocol::{
    ConflictPolicy, CreateEntity, DeliverySemantic, FrameId, IdempotencyKey, PatchBudget,
    SceneOperation, ScenePatch, SceneRevision, SchemaVersion, StableEntityId, TransactionId,
};
use cogniform_world::{
    AuthoritativeWorld, WorldApplyError, WorldConfig, WorldExtractionError, WorldInvariantError,
};

const FIXTURE_NAME: &str = "world-create-empty-v1";
const FIXTURE_OPERATIONS: u32 = 1_000;
const WARMUP_SAMPLES: u32 = 5;
const MEASURED_SAMPLES: u32 = 30;

/// Build profile used to collect a controlled measurement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MeasurementProfile {
    /// Development profile with debug assertions.
    Debug,
    /// Optimized release profile.
    Release,
}

impl std::fmt::Display for MeasurementProfile {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Debug => formatter.write_str("debug"),
            Self::Release => formatter.write_str("release"),
        }
    }
}

/// Sorted nearest-rank timing distribution stored in nanoseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimingDistribution {
    min: u128,
    median: u128,
    p95: u128,
    max: u128,
}

impl TimingDistribution {
    fn from_samples(mut samples: Vec<u128>) -> Self {
        debug_assert!(!samples.is_empty());
        samples.sort_unstable();
        let median_index = samples.len() / 2;
        let p95_rank = samples.len().saturating_mul(95).div_ceil(100);
        let p95_index = p95_rank.saturating_sub(1);
        Self {
            min: samples[0],
            median: samples[median_index],
            p95: samples[p95_index],
            max: samples[samples.len() - 1],
        }
    }

    /// Returns the minimum sample in nanoseconds.
    #[must_use]
    pub const fn min_nanos(self) -> u128 {
        self.min
    }

    /// Returns the upper middle sample in nanoseconds.
    #[must_use]
    pub const fn median_nanos(self) -> u128 {
        self.median
    }

    /// Returns the nearest-rank 95th percentile in nanoseconds.
    #[must_use]
    pub const fn p95_nanos(self) -> u128 {
        self.p95
    }

    /// Returns the maximum sample in nanoseconds.
    #[must_use]
    pub const fn max_nanos(self) -> u128 {
        self.max
    }
}

#[derive(Debug, Clone, Copy)]
struct NanosecondSample {
    apply: u128,
    validate: u128,
    commit: u128,
    extraction: u128,
    logical_hash: u128,
}

/// Complete result of the fixed controlled CPU world fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldMeasurement {
    apply: TimingDistribution,
    validate: TimingDistribution,
    commit: TimingDistribution,
    extraction: TimingDistribution,
    logical_hash: TimingDistribution,
}

impl WorldMeasurement {
    /// Returns the versioned fixture name.
    #[must_use]
    pub const fn fixture_name(self) -> &'static str {
        FIXTURE_NAME
    }

    /// Returns the profile compiled into the measuring process.
    #[must_use]
    pub const fn profile(self) -> MeasurementProfile {
        if cfg!(debug_assertions) {
            MeasurementProfile::Debug
        } else {
            MeasurementProfile::Release
        }
    }

    /// Returns entity-create operations applied per sample.
    #[must_use]
    pub const fn operations(self) -> u32 {
        FIXTURE_OPERATIONS
    }

    /// Returns discarded warmup sample count.
    #[must_use]
    pub const fn warmup_samples(self) -> u32 {
        WARMUP_SAMPLES
    }

    /// Returns retained measured sample count.
    #[must_use]
    pub const fn measured_samples(self) -> u32 {
        MEASURED_SAMPLES
    }

    /// Returns the complete decoded patch-apply distribution.
    #[must_use]
    pub const fn apply(self) -> TimingDistribution {
        self.apply
    }

    /// Returns protocol-validation and atomic-preflight receipt timing.
    #[must_use]
    pub const fn validate(self) -> TimingDistribution {
        self.validate
    }

    /// Returns atomic world-commit receipt timing.
    #[must_use]
    pub const fn commit(self) -> TimingDistribution {
        self.commit
    }

    /// Returns compact render-extraction timing.
    #[must_use]
    pub const fn extraction(self) -> TimingDistribution {
        self.extraction
    }

    /// Returns canonical logical-hash timing.
    #[must_use]
    pub const fn logical_hash(self) -> TimingDistribution {
        self.logical_hash
    }
}

/// Failure while running the fixed controlled CPU world fixture.
#[derive(Debug)]
pub enum MeasurementError {
    /// The authoritative world rejected the fixed patch.
    WorldApply(WorldApplyError),
    /// Compact render extraction failed.
    WorldExtraction(WorldExtractionError),
    /// Canonical logical hashing found a world invariant failure.
    WorldInvariant(WorldInvariantError),
    /// Fixed fixture output did not match its declared operation count.
    Contract,
}

impl std::fmt::Display for MeasurementError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WorldApply(error) => write!(formatter, "measurement patch failed: {error}"),
            Self::WorldExtraction(error) => {
                write!(formatter, "measurement extraction failed: {error}")
            }
            Self::WorldInvariant(error) => {
                write!(formatter, "measurement logical hash failed: {error}")
            }
            Self::Contract => formatter.write_str("measurement fixture output count mismatch"),
        }
    }
}

impl std::error::Error for MeasurementError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::WorldApply(error) => Some(error),
            Self::WorldExtraction(error) => Some(error),
            Self::WorldInvariant(error) => Some(error),
            Self::Contract => None,
        }
    }
}

impl From<WorldApplyError> for MeasurementError {
    fn from(value: WorldApplyError) -> Self {
        Self::WorldApply(value)
    }
}

impl From<WorldExtractionError> for MeasurementError {
    fn from(value: WorldExtractionError) -> Self {
        Self::WorldExtraction(value)
    }
}

impl From<WorldInvariantError> for MeasurementError {
    fn from(value: WorldInvariantError) -> Self {
        Self::WorldInvariant(value)
    }
}

/// Measures the fixed 1,000-operation CPU world fixture without a GPU or I/O.
///
/// Five warmups are discarded before 30 samples are summarized. The fixture
/// and caller are responsible for recording hardware, operating system, and
/// isolation limitations; this function never enforces a performance gate.
pub fn measure_controlled_world_fixture() -> Result<WorldMeasurement, MeasurementError> {
    let patch = fixture_patch();
    for _ in 0..WARMUP_SAMPLES {
        black_box(measure_once(&patch)?);
    }

    let mut samples = Vec::with_capacity(MEASURED_SAMPLES as usize);
    for _ in 0..MEASURED_SAMPLES {
        samples.push(measure_once(&patch)?);
    }
    Ok(WorldMeasurement {
        apply: distribution(&samples, |sample| sample.apply),
        validate: distribution(&samples, |sample| sample.validate),
        commit: distribution(&samples, |sample| sample.commit),
        extraction: distribution(&samples, |sample| sample.extraction),
        logical_hash: distribution(&samples, |sample| sample.logical_hash),
    })
}

fn measure_once(patch: &ScenePatch) -> Result<NanosecondSample, MeasurementError> {
    let mut world = AuthoritativeWorld::new(WorldConfig::default());
    let apply_started = Instant::now();
    let receipt = world.apply_patch(
        patch,
        FrameId::new(1).expect("controlled frame identity is non-zero"),
    )?;
    let apply = apply_started.elapsed().as_nanos();

    let extraction_started = Instant::now();
    let extraction = world.take_render_extraction()?;
    let extraction_elapsed = extraction_started.elapsed().as_nanos();
    if extraction.changes().len() != FIXTURE_OPERATIONS as usize {
        return Err(MeasurementError::Contract);
    }

    let hash_started = Instant::now();
    let logical_hash = world.logical_hash()?;
    let logical_hash_elapsed = hash_started.elapsed().as_nanos();
    black_box((extraction.generation(), logical_hash));

    Ok(NanosecondSample {
        apply,
        validate: u128::from(receipt.timing.validate_micros) * 1_000,
        commit: u128::from(receipt.timing.commit_micros) * 1_000,
        extraction: extraction_elapsed,
        logical_hash: logical_hash_elapsed,
    })
}

fn fixture_patch() -> ScenePatch {
    let operations = (1..=FIXTURE_OPERATIONS)
        .map(|value| {
            SceneOperation::Create(CreateEntity {
                entity_id: StableEntityId::new(u128::from(value))
                    .expect("controlled entity identity is non-zero"),
                components: Vec::new(),
            })
        })
        .collect();
    ScenePatch {
        schema_version: SchemaVersion::V1,
        transaction_id: TransactionId::new(1).expect("controlled transaction identity is non-zero"),
        idempotency_key: IdempotencyKey::new(2).expect("controlled idempotency key is non-zero"),
        base_revision: SceneRevision::INITIAL,
        conflict_policy: ConflictPolicy::RequireExactBase,
        delivery: DeliverySemantic::MustApply,
        declared_budget: PatchBudget::new(
            NonZeroU32::new(FIXTURE_OPERATIONS).expect("fixture operation count is non-zero"),
            NonZeroU32::new(1).expect("constant is non-zero"),
            NonZeroU64::new(1).expect("constant is non-zero"),
            NonZeroU64::new(4_194_304).expect("constant is non-zero"),
        ),
        operations,
    }
}

fn distribution(
    samples: &[NanosecondSample],
    select: impl Fn(NanosecondSample) -> u128,
) -> TimingDistribution {
    TimingDistribution::from_samples(samples.iter().copied().map(select).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_uses_sorted_nearest_rank_percentiles() {
        let report = TimingDistribution::from_samples((1..=20).rev().collect());
        assert_eq!(report.min_nanos(), 1);
        assert_eq!(report.median_nanos(), 11);
        assert_eq!(report.p95_nanos(), 19);
        assert_eq!(report.max_nanos(), 20);
    }

    #[test]
    fn one_controlled_sample_preserves_fixture_contract() {
        let sample = measure_once(&fixture_patch()).unwrap();
        assert!(sample.apply > 0);
        assert!(sample.extraction > 0);
        assert!(sample.logical_hash > 0);
    }
}
