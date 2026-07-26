//! Validates the reliability number — the app's "chance of rain".
//!
//! The app tells a user the probability that a band works on a given day. That
//! probability comes from VOACAP's day-to-day spread deciles (`SNR LW`,
//! `SNR UP`). This program checks those claims against what actually happened,
//! day by day, in the WSPR record. The measurement method — offset-free
//! deviations, censor-safe counting — lives in [`propcore::spread`], shared
//! with the `storm` binary.
//!
//! Usage: `reliability --fit <month-dir> --test <month-dir> [--test …]`

use std::path::PathBuf;
use std::process::ExitCode;

use propcore::runner::variant_bin;
use propcore::spread::{calibration, fit_scale, gather, SpreadRecord, VOACAP_VARIANT};

fn print_calibration(records: &[SpreadRecord], lower: bool, raw_scale: f64, fitted_scale: f64) {
    let side = if lower { "below" } else { "above" };
    println!("| deviation | engine says | with fitted scale | actually happened | path-hours |");
    println!("| --- | --: | --: | --: | --: |");
    let keep = |_: &SpreadRecord, _: &propcore::spread::DaySample| true;
    let raw = calibration(records, lower, raw_scale, &keep);
    let fitted = calibration(records, lower, fitted_scale, &keep);
    for (label, bin) in &raw {
        let scaled = fitted.get(label).map_or(0.0, |b| b.predicted_percent());
        println!(
            "| {label} {side} | {:.1}% | {scaled:.1}% | {:.1}% | {} |",
            bin.predicted_percent(),
            bin.observed_percent(),
            bin.path_hours
        );
    }
}

fn args_of(name: &str) -> Vec<PathBuf> {
    let argv: Vec<String> = std::env::args().collect();
    argv.iter()
        .enumerate()
        .filter(|(_, a)| *a == name)
        .filter_map(|(i, _)| argv.get(i + 1).map(PathBuf::from))
        .collect()
}

fn main() -> ExitCode {
    let fit_dirs = args_of("--fit");
    let test_dirs = args_of("--test");
    if fit_dirs.is_empty() || test_dirs.is_empty() {
        eprintln!("usage: reliability --fit <month-dir> [--fit …] --test <month-dir> [--test …]");
        return ExitCode::FAILURE;
    }
    if !variant_bin(VOACAP_VARIANT).is_file() {
        eprintln!("no voacapl variant binary; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }

    let mut fit_records = Vec::new();
    let mut fit_names = Vec::new();
    for dir in &fit_dirs {
        match gather(dir) {
            Ok(mut m) => {
                eprintln!(
                    "{}: {} spread records from {} paths ({} failed)",
                    m.name,
                    m.records.len(),
                    m.paths_run,
                    m.failures
                );
                fit_names.push(m.name.clone());
                fit_records.append(&mut m.records);
            }
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }

    let (scale_low, n_low) = fit_scale(&fit_records, true);
    let (scale_up, n_up) = fit_scale(&fit_records, false);

    println!("# Is the reliability number honest?\n");
    println!(
        "VOACAP claims a day-to-day spread for every hour: 10% of days fall \
         more than `SNR LW` dB below the hour's monthly median, 10% rise more \
         than `SNR UP` above it. The app's \"chance of rain\" is computed from \
         those claims, so this checks them against the WSPR record, day by \
         day. All comparisons are deviations from each path-hour's own median, \
         which no unknown antenna can shift.\n"
    );
    println!(
        "Fitted on {}: the engine's lower decile is {:.2} times too wide \
         ({n_low} path-hours), the upper {:.2} times ({n_up} path-hours). \
         Scale factors below {:.0}% mean the engine overstates how much days \
         differ from each other.\n",
        fit_names.join(", "),
        1.0 / scale_low.max(1e-9),
        1.0 / scale_up.max(1e-9),
        100.0 * scale_low.min(scale_up),
    );
    println!("Fitted spread scales: lower {scale_low:.3}, upper {scale_up:.3}.\n");

    for dir in &test_dirs {
        let m = match gather(dir) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        println!(
            "## Tested on {} ({} spread records)\n",
            m.name,
            m.records.len()
        );
        println!("Days falling BELOW the hour's median:\n");
        print_calibration(&m.records, true, 1.0, scale_low);
        println!("\nDays rising ABOVE the hour's median:\n");
        print_calibration(&m.records, false, 1.0, scale_up);
        println!();
    }

    ExitCode::SUCCESS
}
