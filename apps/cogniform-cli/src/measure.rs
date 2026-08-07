use std::io::{self, Write};

use cogniform_engine::{TimingDistribution, WorldMeasurement, measure_controlled_world_fixture};
use serde::Serialize;

const SCHEMA_VERSION: u32 = 1;
const UNIT: &str = "nanoseconds";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MeasureOutput {
    Human,
    Json,
}

pub(crate) fn run(output: MeasureOutput) -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        eprintln!("warning: measure-world evidence should be collected with --release");
    }
    let report = measure_controlled_world_fixture()?;

    match output {
        MeasureOutput::Human => print_human(&report),
        MeasureOutput::Json => print_json(&report)?,
    }
    Ok(())
}

fn print_human(report: &WorldMeasurement) {
    println!("Cogniform controlled world measurement");
    println!("fixture: {}", report.fixture_name());
    println!("profile: {}", report.profile());
    println!("operations per sample: {}", report.operations());
    println!("warmup samples: {}", report.warmup_samples());
    println!("measured samples: {}", report.measured_samples());
    print_distribution("apply total", report.apply());
    print_distribution("validate and preflight", report.validate());
    print_distribution("atomic commit", report.commit());
    print_distribution("render extraction", report.extraction());
    print_distribution("logical hash", report.logical_hash());
    println!("threshold: informational only; no release or merge gate");
}

fn print_distribution(label: &str, distribution: TimingDistribution) {
    println!(
        "{label} (microseconds): min={} median={} p95={} max={}",
        micros(distribution.min_nanos()),
        micros(distribution.median_nanos()),
        micros(distribution.p95_nanos()),
        micros(distribution.max_nanos()),
    );
}

fn micros(nanos: u128) -> String {
    format!("{}.{:03}", nanos / 1_000, nanos % 1_000)
}

fn print_json(report: &WorldMeasurement) -> Result<(), Box<dyn std::error::Error>> {
    let report = MeasurementReport {
        schema_version: SCHEMA_VERSION,
        fixture: report.fixture_name(),
        profile: report.profile().to_string(),
        operations_per_sample: report.operations(),
        warmup_samples: report.warmup_samples(),
        measured_samples: report.measured_samples(),
        unit: UNIT,
        informational_only: true,
        apply_total: distribution_report(report.apply())?,
        validate_and_preflight: distribution_report(report.validate())?,
        atomic_commit: distribution_report(report.commit())?,
        render_extraction: distribution_report(report.extraction())?,
        logical_hash: distribution_report(report.logical_hash())?,
    };
    let mut encoded = Vec::new();
    serde_json::to_writer(&mut encoded, &report)?;
    encoded.push(b'\n');
    io::stdout().lock().write_all(&encoded)?;
    Ok(())
}

fn distribution_report(distribution: TimingDistribution) -> io::Result<DistributionReport> {
    Ok(DistributionReport {
        min: json_nanos(distribution.min_nanos())?,
        median: json_nanos(distribution.median_nanos())?,
        p95: json_nanos(distribution.p95_nanos())?,
        max: json_nanos(distribution.max_nanos())?,
    })
}

fn json_nanos(nanos: u128) -> io::Result<u64> {
    u64::try_from(nanos).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "measurement timing exceeds JSON numeric range",
        )
    })
}

#[derive(Serialize)]
struct MeasurementReport<'a> {
    schema_version: u32,
    fixture: &'a str,
    profile: String,
    operations_per_sample: u32,
    warmup_samples: u32,
    measured_samples: u32,
    unit: &'static str,
    informational_only: bool,
    apply_total: DistributionReport,
    validate_and_preflight: DistributionReport,
    atomic_commit: DistributionReport,
    render_extraction: DistributionReport,
    logical_hash: DistributionReport,
}

#[derive(Serialize)]
struct DistributionReport {
    min: u64,
    median: u64,
    p95: u64,
    max: u64,
}

#[cfg(test)]
mod tests {
    use super::json_nanos;

    #[test]
    fn json_nanoseconds_are_checked_at_the_u64_boundary() {
        assert_eq!(json_nanos(u128::from(u64::MAX)).unwrap(), u64::MAX);
        let error = json_nanos(u128::from(u64::MAX) + 1).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        assert_eq!(
            error.to_string(),
            "measurement timing exceeds JSON numeric range"
        );
    }
}
