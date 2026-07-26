//! Compares both engines against measured WSPR reception reports.
//!
//! This is the only test here that involves reality. The other two programs ask
//! whether a port matches the engine, and whether the two engines match each
//! other. This one asks whether either engine matches the ionosphere.
//!
//! ## What is being measured, and what is not
//!
//! A path is a fixed pair of stations on a fixed band. Two things about it are
//! unknown and unknowable from the data: the antennas at each end, and the
//! man-made noise at the receiver. Within one path both are constant, so they
//! add one fixed offset to every hour of that path rather than scattering it.
//!
//! So the method is: predict all 24 hours, fit one constant offset per path by
//! taking the median difference, and report what is left. What is left is how
//! well the model tracks the **daily shape** of the circuit — when it opens,
//! when it peaks, when it closes. Absolute signal level is not tested, and
//! cannot be without knowing the antennas.
//!
//! Fitting the offset also absorbs any error in the reference bandwidth
//! conversion, which is a useful safety property: a constant mistake there
//! cannot change the result.
//!
//! ## The control
//!
//! A model that tracks nothing still scores well if the circuit barely varies.
//! So a flat baseline is measured alongside: predicting that every hour equals
//! the path's own median. An engine that cannot beat that line is adding
//! nothing over knowing the band's average and no physics at all.
//!
//! ## Known biases, not fixed here
//!
//! - **Censoring.** WSPR only records what was decoded, roughly above -29 dB.
//!   Hours where the real median sits below that appear as a higher median, or
//!   as no data. Path-hours whose observed median is within a few dB of the
//!   floor are dropped, which reduces the bias without removing it.
//! - **One month, one solar level.** June 2025 only, at a smoothed sunspot
//!   number of 125. Nothing here says how either engine behaves at solar
//!   minimum or in winter.
//! - **Receiver population.** WSPR receivers are self-selected and cluster in
//!   North America and Europe.
//!
//! Usage: `cargo run --release --bin validate -- [--data <dir>] [--itu <dir>]`

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Instant;

use propcore::deck::{build_deck, DeckCase};
use propcore::itu::{parse_report, run_case, ItuPaths};
use propcore::listing::parse_listing;
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};
use propcore::wspr::{self, WsprPath, WSPR_BANDWIDTH_HZ, WSPR_BANDWIDTH_OFFSET_DB};

const VOACAP_VARIANT: &str = "O2";
const CONCURRENCY: usize = 4;

/// Only affects the reliability figure, not the signal-to-noise ratio this
/// program reads, so its value does not matter here.
const REQUIRED_SNR_DB: f64 = 24.0;

/// A neutral rural noise assumption. The receiver's real noise environment is
/// unknown, but it is constant within a path, so the fitted offset absorbs it.
const NOISE_DBW: f64 = 145.0;

/// Both engines are run at one watt regardless of what the station actually
/// used.
///
/// Signal-to-noise ratio is linear in transmit power in both models, so power
/// only shifts a path's whole day by a constant, and the fitted offset removes
/// it. Using a fixed value also sidesteps a hard limit in the P.533 reference
/// implementation, which rejects anything below one watt with `RTN_ERRTXPOWER`
/// (`ValidatePath.c`) — that covers most WSPR beacons, which commonly run
/// 200 mW.
const REFERENCE_WATTS: f64 = 1.0;

/// Observed medians this close to the decoder's floor are truncated rather than
/// measured, so they are left out.
const OBSERVED_FLOOR_DB: f64 = -25.0;

/// Below this a predicted ratio is a dead-path sentinel, not a prediction.
const IMPLAUSIBLE_SNR_DB: f64 = -200.0;

/// A path needs this many usable hours before its shape means anything.
const MIN_HOURS: usize = 8;

struct PathOutcome {
    label: String,
    km: f64,
    band: i32,
    /// Observed, VOACAP and P.533 signal-to-noise ratios for the hours where
    /// all three exist and the observation is above the censoring floor.
    hours: Vec<(f64, f64, f64)>,
    failure: Option<String>,
}

fn main() -> ExitCode {
    let data_dir = arg("--data").unwrap_or_else(|| PathBuf::from("data"));
    let itu_root = arg("--itu").unwrap_or_else(|| {
        std::env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_default()
            .join("workspace/vendor/itu-r-hf")
    });

    let data = match wspr::load(&data_dir) {
        Ok(d) => d,
        Err(e) => {
            eprintln!("could not read WSPR data from {}: {e}", data_dir.display());
            eprintln!("run tools/fetch-wspr.sh first");
            return ExitCode::FAILURE;
        }
    };
    let (Some(year), Some(month)) = (data.year(), data.month_number()) else {
        eprintln!(
            "could not read the month from {}/month.txt",
            data_dir.display()
        );
        return ExitCode::FAILURE;
    };

    let ssn = match smoothed_ssn(&data.month) {
        Some(v) => v,
        None => {
            eprintln!("no smoothed sunspot number known for {}", data.month);
            eprintln!("add it to SMOOTHED_SSN in this file");
            return ExitCode::FAILURE;
        }
    };

    let itu = ItuPaths::from_checkout(&itu_root);
    let voacap_bin = variant_bin(VOACAP_VARIANT);
    if !itu.is_built() || !voacap_bin.is_file() {
        eprintln!("both engines must be built; see propcore/README.md");
        return ExitCode::FAILURE;
    }

    eprintln!(
        "{} paths from {}, smoothed sunspot number {ssn}",
        data.paths.len(),
        data.month
    );
    let started = Instant::now();

    let outcomes = map_limit(&data.paths, CONCURRENCY, |path, index| {
        run_path(path, index, year, month, ssn, &data, &itu, &voacap_bin)
    });

    eprintln!("finished in {:.1}s", started.elapsed().as_secs_f64());

    report(&data.month, ssn, &outcomes, &data_dir);
    ExitCode::SUCCESS
}

#[allow(clippy::too_many_arguments)]
fn run_path(
    path: &WsprPath,
    index: usize,
    year: u32,
    month: u32,
    ssn: f64,
    data: &wspr::WsprData,
    itu: &ItuPaths,
    voacap_bin: &Path,
) -> PathOutcome {
    let mut outcome = PathOutcome {
        label: path.label(),
        km: path.km,
        band: path.band,
        hours: Vec::new(),
        failure: None,
    };

    let case = DeckCase {
        id: format!("w{index}"),
        from_lat: path.tx_lat,
        from_lon: path.tx_lon,
        to_lat: path.rx_lat,
        to_lon: path.rx_lon,
        month,
        year,
        ssn,
        watts: REFERENCE_WATTS,
        required_snr_db: REQUIRED_SNR_DB,
        noise_dbw: NOISE_DBW,
        freqs_mhz: vec![path.freq_mhz],
    };

    let deck = match build_deck(&case) {
        Ok(d) => d,
        Err(e) => {
            outcome.failure = Some(format!("deck: {e}"));
            return outcome;
        }
    };

    // VOACAP prints its ratio in a 1 Hz bandwidth; WSPR reports in 2500 Hz.
    let voacap: Option<[Option<f64>; 24]> = match IsolatedRoot::create(&format!("val-{index}")) {
        Ok(root) => match run_deck(voacap_bin, root.path(), &deck) {
            Ok(text) => {
                let listing = parse_listing(&text);
                let mut day = [None; 24];
                for s in listing
                    .numeric
                    .iter()
                    .filter(|s| s.row == "SNR" && s.slot == 0)
                {
                    if s.value > IMPLAUSIBLE_SNR_DB {
                        day[s.hour as usize] = Some(s.value - WSPR_BANDWIDTH_OFFSET_DB);
                    }
                }
                Some(day)
            }
            Err(e) => {
                outcome.failure = Some(format!("voacap: {e}"));
                None
            }
        },
        Err(e) => {
            outcome.failure = Some(format!("isolate: {e}"));
            None
        }
    };

    let work = scratch(&format!("propcore-val-{index}"));
    let itu_day: Option<[Option<f64>; 24]> = match fs::create_dir_all(&work)
        .map_err(|e| e.to_string())
        .and_then(|()| run_case(itu, &case, &work, WSPR_BANDWIDTH_HZ).map_err(|e| e.to_string()))
    {
        Ok(text) => {
            let mut day = [None; 24];
            for r in parse_report(&text) {
                // `NONE` means the model found no propagating mode, which is a
                // prediction of no signal rather than a number to compare.
                if !r.mode.trim().eq_ignore_ascii_case("NONE") && r.snr > IMPLAUSIBLE_SNR_DB {
                    day[r.hour as usize] = Some(r.snr);
                }
            }
            Some(day)
        }
        Err(e) => {
            outcome.failure.get_or_insert(format!("itu: {e}"));
            None
        }
    };
    let _ = fs::remove_dir_all(&work);

    let (Some(voacap), Some(itu_day)) = (voacap, itu_day) else {
        return outcome;
    };
    let Some(observed) = data.hourly.get(&path.key()) else {
        outcome.failure = Some("no hourly reports".to_string());
        return outcome;
    };

    for hour in 0..24 {
        let (Some(obs), Some(v), Some(i)) = (observed[hour], voacap[hour], itu_day[hour]) else {
            continue;
        };
        if obs < OBSERVED_FLOOR_DB {
            continue;
        }
        outcome.hours.push((obs, v, i));
    }

    outcome
}

fn arg(name: &str) -> Option<PathBuf> {
    let argv: Vec<String> = std::env::args().collect();
    let i = argv.iter().position(|a| a == name)?;
    argv.get(i + 1).map(PathBuf::from)
}

fn scratch(name: &str) -> PathBuf {
    std::env::var_os("PROPCORE_SCRATCH")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(name)
}

/// Smoothed sunspot number (R12) per month, from NOAA SWPC's observed solar
/// cycle indices.
///
/// This is deliberately a table rather than a fetch: R12 for a past month never
/// changes once published, and a validation run should not depend on a network
/// service being up or on which day it was run.
const SMOOTHED_SSN: &[(&str, f64)] = &[
    ("2025-04", 133.4),
    ("2025-05", 128.6),
    ("2025-06", 124.7),
    ("2025-07", 122.5),
    ("2025-08", 118.4),
];

fn smoothed_ssn(month: &str) -> Option<f64> {
    SMOOTHED_SSN
        .iter()
        .find(|(m, _)| *m == month)
        .map(|(_, v)| *v)
}

fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

fn rms(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|v| v * v).sum::<f64>() / values.len() as f64).sqrt()
}

/// Pearson correlation, or `None` if either side does not vary.
fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() != b.len() || a.len() < 3 {
        return None;
    }
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for (x, y) in a.iter().zip(b) {
        num += (x - mean_a) * (y - mean_b);
        da += (x - mean_a).powi(2);
        db += (y - mean_b).powi(2);
    }
    if da <= 0.0 || db <= 0.0 {
        return None;
    }
    Some(num / (da * db).sqrt())
}

/// Least-squares fit of `observed = a + b * predicted`.
///
/// The slope matters as much as the fit. A model can put the peaks and troughs
/// in the right places and still swing far too hard between them; correlation
/// cannot see that, because it ignores scale, but the slope shows it directly.
/// A slope near 1 means the model predicts the right amount of variation, and
/// well under 1 means it predicts too much.
fn fit_line(observed: &[f64], predicted: &[f64]) -> Option<(f64, f64)> {
    if observed.len() != predicted.len() || observed.len() < 3 {
        return None;
    }
    let n = observed.len() as f64;
    let mean_p = predicted.iter().sum::<f64>() / n;
    let mean_o = observed.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (p, o) in predicted.iter().zip(observed) {
        num += (p - mean_p) * (o - mean_o);
        den += (p - mean_p).powi(2);
    }
    if den <= 0.0 {
        return None;
    }
    let slope = num / den;
    Some((mean_o - slope * mean_p, slope))
}

#[derive(Default)]
struct EngineScore {
    errors: Vec<f64>,
    /// Errors after fitting a gain as well as an offset.
    scaled_errors: Vec<f64>,
    correlations: Vec<f64>,
    offsets: Vec<f64>,
    slopes: Vec<f64>,
}

impl EngineScore {
    fn add_path(&mut self, observed: &[f64], predicted: &[f64]) {
        let mut residuals: Vec<f64> = predicted.iter().zip(observed).map(|(p, o)| p - o).collect();
        // One offset per path absorbs the unknown antennas, the unknown local
        // noise, and any constant error in the bandwidth conversion.
        let offset = median(&mut residuals.clone());
        self.offsets.push(offset);
        for r in &mut residuals {
            *r -= offset;
        }
        self.errors.extend(residuals.iter().map(|r| r.abs()));

        if let Some(c) = correlation(observed, predicted) {
            self.correlations.push(c);
        }
        if let Some((a, b)) = fit_line(observed, predicted) {
            self.slopes.push(b);
            self.scaled_errors.extend(
                predicted
                    .iter()
                    .zip(observed)
                    .map(|(p, o)| (a + b * p - o).abs()),
            );
        }
    }

    fn line(&self, name: &str) -> String {
        let mut errors = self.errors.clone();
        let mut scaled = self.scaled_errors.clone();
        let mut correlations = self.correlations.clone();
        let mut slopes = self.slopes.clone();
        let optional = |v: &mut Vec<f64>, width: usize| -> String {
            if v.is_empty() {
                "—".to_string()
            } else {
                format!("{:+.*}", width, median(v))
            }
        };
        // A constant predictor has no slope to fit, so the last two columns are
        // undefined for the baseline rather than perfect.
        format!(
            "| {name} | {} | {:.1} | {:.1} | {} | {} | {} |",
            self.errors.len(),
            median(&mut errors),
            rms(&self.errors),
            optional(&mut correlations, 2),
            optional(&mut slopes, 2),
            if scaled.is_empty() {
                "—".to_string()
            } else {
                format!("{:.1}", median(&mut scaled))
            },
        )
    }
}

const TABLE_HEADER: &str = concat!(
    "| predictor | path-hours | median error | RMS error | correlation | slope | error after gain fit |\n",
    "| --- | --: | --: | --: | --: | --: | --: |"
);

/// Paths whose weakest hour is comfortably clear of the decoder's floor.
///
/// Censoring compresses the observed daily swing: hours that were really very
/// weak either read higher than they were or vanish. That alone could make a
/// model look like it exaggerates variation. Restricting to paths that never
/// approach the floor removes the effect, so if the same pattern survives here
/// it is the models, not the measurement.
const UNCENSORED_FLOOR_DB: f64 = -15.0;

fn score(
    outcomes: &[PathOutcome],
    uncensored_only: bool,
) -> (EngineScore, EngineScore, EngineScore, usize) {
    let mut voacap = EngineScore::default();
    let mut itu = EngineScore::default();
    let mut flat = EngineScore::default();
    let mut used = 0usize;

    for o in outcomes {
        if o.hours.len() < MIN_HOURS {
            continue;
        }
        let observed: Vec<f64> = o.hours.iter().map(|h| h.0).collect();
        if uncensored_only && observed.iter().any(|v| *v < UNCENSORED_FLOOR_DB) {
            continue;
        }
        used += 1;

        let v: Vec<f64> = o.hours.iter().map(|h| h.1).collect();
        let i: Vec<f64> = o.hours.iter().map(|h| h.2).collect();
        let level = median(&mut observed.clone());
        let f: Vec<f64> = vec![level; observed.len()];

        voacap.add_path(&observed, &v);
        itu.add_path(&observed, &i);
        flat.add_path(&observed, &f);
    }

    (voacap, itu, flat, used)
}

fn report(month: &str, ssn: f64, outcomes: &[PathOutcome], data_dir: &Path) {
    let mut used = 0usize;
    let mut skipped_short = 0usize;
    let failures: Vec<&PathOutcome> = outcomes.iter().filter(|o| o.failure.is_some()).collect();

    let mut per_path = String::from("path,km,band,hours,voacap_mad,itu_mad,flat_mad\n");

    for o in outcomes {
        if o.hours.len() < MIN_HOURS {
            if o.failure.is_none() {
                skipped_short += 1;
            }
            continue;
        }
        used += 1;
        let observed: Vec<f64> = o.hours.iter().map(|h| h.0).collect();
        let v: Vec<f64> = o.hours.iter().map(|h| h.1).collect();
        let i: Vec<f64> = o.hours.iter().map(|h| h.2).collect();
        // The control: predict every hour as the path's own median.
        let level = median(&mut observed.clone());
        let f: Vec<f64> = vec![level; observed.len()];

        let mad = |pred: &[f64]| -> f64 {
            let r: Vec<f64> = pred.iter().zip(&observed).map(|(p, o)| p - o).collect();
            let off = median(&mut r.clone());
            let mut abs: Vec<f64> = r.iter().map(|x| (x - off).abs()).collect();
            median(&mut abs)
        };
        per_path.push_str(&format!(
            "{},{:.0},{},{},{:.2},{:.2},{:.2}\n",
            o.label,
            o.km,
            o.band,
            o.hours.len(),
            mad(&v),
            mad(&i),
            mad(&f)
        ));
    }

    println!("# Both engines against measured WSPR reports\n");
    println!(
        "{month}, smoothed sunspot number {ssn}. {used} paths used of {} fetched.\n",
        outcomes.len()
    );
    println!(
        "Each path is a fixed pair of stations on a fixed band, so its antennas \
         and its local noise are unknown but constant. One offset per path is \
         fitted and removed, which is why this measures how well a model tracks \
         the **daily shape** of a circuit rather than its absolute level.\n"
    );
    println!(
        "The flat baseline predicts every hour as that path's own median. It \
         contains no physics. An engine that does not beat it is adding \
         nothing.\n"
    );

    let (voacap, itu, flat, _) = score(outcomes, false);
    println!("## All paths\n");
    println!("{TABLE_HEADER}");
    println!("{}", voacap.line("VOACAP"));
    println!("{}", itu.line("ITU-R P.533"));
    println!("{}", flat.line("flat baseline"));
    println!(
        "\nErrors are in dB. Correlation and slope come from fitting \
         `observed = a + b * predicted` per path: correlation says whether the \
         peaks and troughs land in the right places, slope says whether the \
         model swings by the right amount, and the last column is what is left \
         once both are fitted.\n"
    );

    let (unc_v, unc_i, unc_f, unc_used) = score(outcomes, true);
    println!("## Paths that never approach the decoder's floor\n");
    println!(
        "{unc_used} paths whose weakest hour stays above {UNCENSORED_FLOOR_DB:.0} dB. \
         WSPR cannot report what it fails to decode, so weak hours read higher \
         than they were or vanish, which flattens the measured daily swing. On \
         these paths that cannot be happening, so anything that survives here is \
         the models rather than the measurement.\n"
    );
    println!("{TABLE_HEADER}");
    println!("{}", unc_v.line("VOACAP"));
    println!("{}", unc_i.line("ITU-R P.533"));
    println!("{}", unc_f.line("flat baseline"));
    println!();

    println!("## What was left out\n");
    println!(
        "- {skipped_short} paths had fewer than {MIN_HOURS} usable hours after \
         dropping observations within {:.0} dB of the decoder's floor.",
        OBSERVED_FLOOR_DB.abs() - 29.0_f64.abs()
    );
    println!("- {} paths failed to run.", failures.len());
    for f in failures.iter().take(5) {
        println!(
            "  - `{}`: {}",
            f.label,
            f.failure.as_deref().unwrap_or("unknown")
        );
    }

    let per_path_file = data_dir.join("validation-per-path.csv");
    match fs::write(&per_path_file, per_path) {
        Ok(()) => println!(
            "\nPer-path detail written to `{}`.",
            per_path_file.display()
        ),
        Err(e) => eprintln!("could not write {}: {e}", per_path_file.display()),
    }
}
