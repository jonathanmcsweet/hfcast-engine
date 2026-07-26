//! Validates the reliability number — the app's "chance of rain".
//!
//! The app tells a user the probability that a band works on a given day. That
//! probability comes from VOACAP's day-to-day spread deciles (`SNR LW`,
//! `SNR UP`): the engine claims 10% of days fall more than `SNR LW` dB below
//! that hour's monthly median, and 10% rise more than `SNR UP` above it. This
//! program checks those claims against what actually happened, day by day, in
//! the WSPR record.
//!
//! ## Why deviations, not absolute levels
//!
//! A path's absolute level is unknown (antennas), so probabilities against an
//! absolute threshold cannot be checked directly. Deviations from the path's
//! own monthly median are offset-free: "how often did a day fall 6 dB below
//! this hour's median" needs no knowledge of the antennas at all, and it is
//! exactly the quantity the deciles claim to describe.
//!
//! ## Censoring, handled per bin
//!
//! WSPR cannot report below its decode floor (about -29 dB), so downward
//! deviations from a low median are invisible. Every observed frequency here
//! is computed only where the deviation in question would still have been
//! comfortably decodable; bins that a path-hour cannot measure are excluded
//! for that path-hour rather than silently counted as "never happened".
//! Missing days are excluded too — a silent day can be a weak day or a
//! switched-off transmitter, and the two cannot be told apart.
//!
//! Usage: `reliability --fit <month-dir> --test <month-dir> [--test …]`

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use propcore::deck::{build_deck, DeckCase};
use propcore::listing::parse_listing;
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};
use propcore::stats::{median, quantile};
use propcore::wspr::{self, smoothed_ssn};

const VOACAP_VARIANT: &str = "O2";
const CONCURRENCY: usize = 4;

/// Matches the validated server configuration: sporadic-E on.
const SPORADIC_E: bool = true;

/// A day's median at one hour counts only with at least this many reports.
const MIN_SPOTS_PER_DAY: u32 = 4;

/// A path-hour needs this many measured days before its spread means anything.
const MIN_DAYS: usize = 20;

/// Downward deviations are only scored where the deviated value would still be
/// clearly decodable.
const CENSOR_SAFE_DB: f64 = -26.0;

/// Upward deviations are only scored well below where the reported scale
/// saturates.
const TOP_SAFE_DB: f64 = 12.0;

/// A decile is this many standard deviations of a normal distribution.
const DECILE_TO_SIGMA: f64 = 1.2816;

/// Deviation sizes scored, in dB.
const DEVIATIONS: [f64; 4] = [3.0, 6.0, 10.0, 15.0];

/// One path-hour: the engine's spread claim and the measured days.
struct SpreadRecord {
    /// Engine decile distances, dB.
    lw: f64,
    up: f64,
    /// Observed daily medians for this hour across the month.
    days: Vec<f64>,
    /// Median of `days`.
    centre: f64,
}

struct MonthData {
    name: String,
    records: Vec<SpreadRecord>,
    paths_run: usize,
    failures: usize,
}

fn phi(z: f64) -> f64 {
    // Abramowitz-Stegun 7.1.26.
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erf = 1.0 - poly * (-x * x).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// Runs the engine for every path of a month and pairs its spread claims with
/// the measured days.
fn gather(dir: &Path) -> Result<MonthData, String> {
    let data = wspr::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let daily = wspr::load_daily(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (Some(year), Some(month)) = (data.year(), data.month_number()) else {
        return Err(format!("{}: unreadable month.txt", dir.display()));
    };
    let ssn =
        smoothed_ssn(&data.month).ok_or_else(|| format!("no smoothed SSN for {}", data.month))?;
    let voacap_bin = variant_bin(VOACAP_VARIANT);

    let outcomes = map_limit(&data.paths, CONCURRENCY, |path, index| {
        let case = DeckCase {
            id: format!("r{index}"),
            from_lat: path.tx_lat,
            from_lon: path.tx_lon,
            to_lat: path.rx_lat,
            to_lon: path.rx_lon,
            month,
            year,
            ssn,
            watts: 1.0,
            required_snr_db: 24.0,
            noise_dbw: 145.0,
            freqs_mhz: vec![path.freq_mhz],
            sporadic_e: SPORADIC_E,
        };
        let deck = build_deck(&case).map_err(|e| e.to_string())?;
        let root = IsolatedRoot::create(&format!("rel-{index}")).map_err(|e| e.to_string())?;
        let listing = run_deck(&voacap_bin, root.path(), &deck).map_err(|e| e.to_string())?;
        let parsed = parse_listing(&listing);

        // Engine decile distances per hour, slot 0 (the only frequency).
        let mut lw = [None::<f64>; 24];
        let mut up = [None::<f64>; 24];
        for s in parsed.numeric.iter().filter(|s| s.slot == 0) {
            match s.row.as_str() {
                "SNR LW" if s.value > 0.0 => lw[s.hour as usize] = Some(s.value),
                "SNR UP" if s.value > 0.0 => up[s.hour as usize] = Some(s.value),
                _ => {}
            }
        }

        // Measured days per hour.
        let mut days: [Vec<f64>; 24] = std::array::from_fn(|_| Vec::new());
        if let Some(samples) = daily.get(&path.key()) {
            for s in samples {
                if s.reports >= MIN_SPOTS_PER_DAY {
                    days[s.hour as usize].push(s.snr_median);
                }
            }
        }

        let mut records = Vec::new();
        for hour in 0..24 {
            let (Some(lw), Some(up)) = (lw[hour], up[hour]) else {
                continue;
            };
            let observed = &days[hour];
            if observed.len() < MIN_DAYS {
                continue;
            }
            records.push(SpreadRecord {
                lw,
                up,
                days: observed.clone(),
                centre: median(&mut observed.clone()),
            });
        }
        Ok::<Vec<SpreadRecord>, String>(records)
    });

    let mut records = Vec::new();
    let mut failures = 0usize;
    for outcome in outcomes {
        match outcome {
            Ok(mut r) => records.append(&mut r),
            Err(_) => failures += 1,
        }
    }

    Ok(MonthData {
        name: data.month,
        records,
        paths_run: data.paths.len(),
        failures,
    })
}

/// Observed decile distances, where the floor allows them to be measured.
///
/// The lower distance is only trusted when the observed lower decile sits
/// clearly above the decode floor; otherwise it is truncated, not measured.
fn observed_deciles(r: &SpreadRecord) -> (Option<f64>, Option<f64>) {
    let q10 = quantile(&mut r.days.clone(), 0.1);
    let q90 = quantile(&mut r.days.clone(), 0.9);
    let lower = if q10 >= CENSOR_SAFE_DB {
        Some(r.centre - q10)
    } else {
        None
    };
    let upper = if r.centre + (q90 - r.centre) <= TOP_SAFE_DB {
        Some(q90 - r.centre)
    } else {
        None
    };
    (lower, upper)
}

/// Pooled least-squares ratio of observed decile distance to predicted.
fn fit_scale(records: &[SpreadRecord], lower: bool) -> (f64, usize) {
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;
    let mut n = 0usize;
    for r in records {
        let (l, u) = observed_deciles(r);
        let (observed, predicted) = if lower { (l, r.lw) } else { (u, r.up) };
        let Some(observed) = observed else { continue };
        sum_xy += observed * predicted;
        sum_xx += predicted * predicted;
        n += 1;
    }
    if sum_xx <= 0.0 {
        (1.0, 0)
    } else {
        (sum_xy / sum_xx, n)
    }
}

struct CalibrationBin {
    predicted_sum: f64,
    below: usize,
    total_days: usize,
    path_hours: usize,
}

impl CalibrationBin {
    fn new() -> Self {
        Self {
            predicted_sum: 0.0,
            below: 0,
            total_days: 0,
            path_hours: 0,
        }
    }
}

/// For each deviation size: the mean predicted probability of a day deviating
/// that far, against the measured frequency, on the side chosen.
fn calibration(
    records: &[SpreadRecord],
    lower: bool,
    scale: f64,
) -> BTreeMap<String, CalibrationBin> {
    let mut bins: BTreeMap<String, CalibrationBin> = BTreeMap::new();

    for r in records {
        for delta in DEVIATIONS {
            // Skip bins this path-hour cannot measure honestly.
            if lower && r.centre - delta < CENSOR_SAFE_DB {
                continue;
            }
            if !lower && r.centre + delta > TOP_SAFE_DB {
                continue;
            }
            let decile = if lower { r.lw } else { r.up };
            let sigma = (decile * scale).max(1e-6) / DECILE_TO_SIGMA;
            let predicted = phi(-delta / sigma);
            let beyond = r
                .days
                .iter()
                .filter(|d| {
                    if lower {
                        **d <= r.centre - delta
                    } else {
                        **d >= r.centre + delta
                    }
                })
                .count();

            let bin = bins
                .entry(format!("{delta:>2.0} dB"))
                .or_insert_with(CalibrationBin::new);
            bin.predicted_sum += predicted * r.days.len() as f64;
            bin.below += beyond;
            bin.total_days += r.days.len();
            bin.path_hours += 1;
        }
    }

    bins
}

fn print_calibration(records: &[SpreadRecord], lower: bool, raw_scale: f64, fitted_scale: f64) {
    let side = if lower { "below" } else { "above" };
    println!("| deviation | engine says | with fitted scale | actually happened | path-hours |");
    println!("| --- | --: | --: | --: | --: |");
    let raw = calibration(records, lower, raw_scale);
    let fitted = calibration(records, lower, fitted_scale);
    for (label, bin) in &raw {
        let observed = if bin.total_days == 0 {
            0.0
        } else {
            100.0 * bin.below as f64 / bin.total_days as f64
        };
        let predicted = if bin.total_days == 0 {
            0.0
        } else {
            100.0 * bin.predicted_sum / bin.total_days as f64
        };
        let scaled = fitted.get(label).map_or(0.0, |b| {
            if b.total_days == 0 {
                0.0
            } else {
                100.0 * b.predicted_sum / b.total_days as f64
            }
        });
        println!(
            "| {label} {side} | {predicted:.1}% | {scaled:.1}% | {observed:.1}% | {} |",
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

#[cfg(test)]
mod tests {
    use super::*;

    fn record(lw: f64, up: f64, days: Vec<f64>) -> SpreadRecord {
        let centre = median(&mut days.clone());
        SpreadRecord {
            lw,
            up,
            days,
            centre,
        }
    }

    /// Thirty days spread symmetrically 0..±9 dB around -5: observed decile
    /// distance is about 8 dB each side.
    fn synthetic_days() -> Vec<f64> {
        (0..30)
            .map(|i| -5.0 + 9.0 * ((i as f64 / 29.0) * 2.0 - 1.0))
            .collect()
    }

    #[test]
    fn fit_recovers_an_overstated_spread() {
        // Engine claims 16 dB deciles; reality is about 8. Scale ≈ 0.5.
        let records = vec![record(16.0, 16.0, synthetic_days())];
        let (scale_low, n) = fit_scale(&records, true);
        assert_eq!(n, 1);
        assert!((scale_low - 0.5).abs() < 0.05, "scale_low {scale_low}");
    }

    #[test]
    fn censored_lower_deciles_are_not_fitted() {
        // Median -20, spread 9: the lower decile sits below the safe line, so
        // the lower side must be excluded rather than fitted from a truncated
        // number.
        let days: Vec<f64> = synthetic_days().iter().map(|d| d - 15.0).collect();
        let records = vec![record(16.0, 16.0, days)];
        let (_, n_low) = fit_scale(&records, true);
        let (_, n_up) = fit_scale(&records, false);
        assert_eq!(n_low, 0);
        assert_eq!(n_up, 1);
    }

    #[test]
    fn calibration_counts_what_actually_happened() {
        // Ten days, one of them 10 dB below the median of 0.
        let mut days = vec![0.0; 9];
        days.push(-10.0);
        for _ in 0..2 {
            days.extend_from_slice(&[0.0; 5]);
        }
        let records = vec![record(10.0, 10.0, days)];
        let bins = calibration(&records, true, 1.0);
        let bin = bins.get(" 6 dB").expect("6 dB bin");
        assert_eq!(bin.below, 1);
        assert_eq!(bin.total_days, 20);
    }

    #[test]
    fn phi_hits_the_decile_anchor() {
        assert!((phi(-1.2816) - 0.1).abs() < 1e-3);
    }
}
