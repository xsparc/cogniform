use cogniform_engine::{TimingDistribution, measure_controlled_world_fixture};

pub(crate) fn run() -> Result<(), Box<dyn std::error::Error>> {
    if cfg!(debug_assertions) {
        eprintln!("warning: measure-world evidence should be collected with --release");
    }
    let report = measure_controlled_world_fixture()?;
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
    Ok(())
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
