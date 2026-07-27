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
use propcore::stats::{correlation, fit_line, median, rms};
use propcore::wspr::{self, smoothed_ssn, WsprPath, WSPR_BANDWIDTH_HZ, WSPR_BANDWIDTH_OFFSET_DB};

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

/// Below this a predicted received power is a dead-path sentinel.
const IMPLAUSIBLE_SIGNAL_DBW: f64 = -250.0;

/// One usable hour on one path: the observation and both engines' predictions.
///
/// The signal-only fields separate the two halves of a prediction. Both
/// engines predict the received signal and the background noise and subtract.
/// A typical WSPR receiver's noise is set by local interference, which barely
/// changes through the day, while the models' noise swings strongly between
/// day and night. Scoring the signal alone scores the engine as if the noise
/// were constant, which shows how much of any exaggerated daily swing comes
/// from the noise half of the prediction.
struct HourSample {
    observed: f64,
    voacap_snr: f64,
    itu_snr: f64,
    /// VOACAP's predicted received signal power (`S DBW`), noise left out.
    voacap_signal: f64,
    /// P.533's predicted median receiver power (`Pr`), noise left out.
    itu_signal: f64,
}

struct PathOutcome {
    label: String,
    km: f64,
    band: i32,
    /// Hours where the observation and all four predictions exist and the
    /// observation is above the censoring floor.
    hours: Vec<HourSample>,
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
            eprintln!("add it to SMOOTHED_SSN in src/wspr.rs");
            return ExitCode::FAILURE;
        }
    };

    let itu = ItuPaths::from_checkout(&itu_root);
    let voacap_bin = variant_bin(VOACAP_VARIANT);
    if !itu.is_built() || !voacap_bin.is_file() {
        eprintln!("both engines must be built; see propcore/README.md");
        return ExitCode::FAILURE;
    }

    // Enables VOACAP's sporadic-E term, which standard practice keeps off.
    // Used by the summer-mechanism experiment; affects VOACAP only.
    let sporadic_e = std::env::args().any(|a| a == "--es");

    eprintln!(
        "{} paths from {}, smoothed sunspot number {ssn}, sporadic-E {}",
        data.paths.len(),
        data.month,
        if sporadic_e { "on" } else { "off" }
    );
    let started = Instant::now();

    let outcomes = map_limit(&data.paths, CONCURRENCY, |path, index| {
        run_path(
            path,
            index,
            year,
            month,
            ssn,
            sporadic_e,
            &data,
            &itu,
            &voacap_bin,
        )
    });

    eprintln!("finished in {:.1}s", started.elapsed().as_secs_f64());

    report(&data.month, ssn, sporadic_e, &outcomes, &data_dir);

    // The calibration step consumes raw hours rather than summaries, so
    // percentile and fitting decisions stay in one place downstream.
    if let Some(dump) = arg("--dump") {
        match dump_hours(&outcomes, &dump) {
            Ok(()) => eprintln!("wrote {}", dump.display()),
            Err(e) => {
                eprintln!("could not write {}: {e}", dump.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}

/// Writes one row per scored path-hour, for the calibration step.
///
/// Only paths that meet [`MIN_HOURS`] appear, so downstream consumers see
/// exactly the population the report describes.
fn dump_hours(outcomes: &[PathOutcome], to: &Path) -> std::io::Result<()> {
    let mut text =
        String::from("label,band,km,observed,voacap_snr,itu_snr,voacap_signal,itu_signal\n");
    for o in outcomes {
        if o.hours.len() < MIN_HOURS {
            continue;
        }
        for h in &o.hours {
            text.push_str(&format!(
                "{},{},{:.0},{},{},{},{},{}\n",
                o.label,
                o.band,
                o.km,
                h.observed,
                h.voacap_snr,
                h.itu_snr,
                h.voacap_signal,
                h.itu_signal
            ));
        }
    }
    fs::write(to, text)
}

#[allow(clippy::too_many_arguments)]
fn run_path(
    path: &WsprPath,
    index: usize,
    year: u32,
    month: u32,
    ssn: f64,
    sporadic_e: bool,
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
        method: 30,
        ursi: false,
        fprob: None,
        botlines: None,
        toplines: None,
        outgraph: None,
            integrate: None,
            comment: None,
            extra_cards: Vec::new(),
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
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        sporadic_e,
    };

    let deck = match build_deck(&case) {
        Ok(d) => d,
        Err(e) => {
            outcome.failure = Some(format!("deck: {e}"));
            return outcome;
        }
    };

    // VOACAP prints its ratio in a 1 Hz bandwidth; WSPR reports in 2500 Hz.
    // `S DBW` is kept alongside so the signal can be scored with the noise
    // held out; the per-path offset makes its absolute unit irrelevant.
    let voacap: Option<[Option<(f64, f64)>; 24]> =
        match IsolatedRoot::create(&format!("val-{index}")) {
            Ok(root) => match run_deck(voacap_bin, root.path(), &deck) {
                Ok(text) => {
                    let listing = parse_listing(&text);
                    let mut snr = [None; 24];
                    let mut signal = [None; 24];
                    for s in listing.numeric.iter().filter(|s| s.slot == 0) {
                        match s.row.as_str() {
                            "SNR" if s.value > IMPLAUSIBLE_SNR_DB => {
                                snr[s.hour as usize] = Some(s.value - WSPR_BANDWIDTH_OFFSET_DB);
                            }
                            "S DBW" if s.value > IMPLAUSIBLE_SIGNAL_DBW => {
                                signal[s.hour as usize] = Some(s.value);
                            }
                            _ => {}
                        }
                    }
                    let mut day = [None; 24];
                    for (hour, slot) in day.iter_mut().enumerate() {
                        *slot = snr[hour].zip(signal[hour]);
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
    let itu_day: Option<[Option<(f64, f64)>; 24]> = match fs::create_dir_all(&work)
        .map_err(|e| e.to_string())
        .and_then(|()| run_case(itu, &case, &work, WSPR_BANDWIDTH_HZ).map_err(|e| e.to_string()))
    {
        Ok(text) => {
            let mut day = [None; 24];
            for r in parse_report(&text) {
                // `NONE` means the model found no propagating mode, which is a
                // prediction of no signal rather than a number to compare.
                if !r.mode.trim().eq_ignore_ascii_case("NONE")
                    && r.snr > IMPLAUSIBLE_SNR_DB
                    && r.receiver_power > IMPLAUSIBLE_SIGNAL_DBW
                {
                    day[r.hour as usize] = Some((r.snr, r.receiver_power));
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
        let (Some(obs), Some((v_snr, v_sig)), Some((i_snr, i_sig))) =
            (observed[hour], voacap[hour], itu_day[hour])
        else {
            continue;
        };
        if obs < OBSERVED_FLOOR_DB {
            continue;
        }
        outcome.hours.push(HourSample {
            observed: obs,
            voacap_snr: v_snr,
            itu_snr: i_snr,
            voacap_signal: v_sig,
            itu_signal: i_sig,
        });
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

#[derive(Default)]
struct Scores {
    voacap_snr: EngineScore,
    itu_snr: EngineScore,
    voacap_signal: EngineScore,
    itu_signal: EngineScore,
    flat: EngineScore,
    used: usize,
}

fn score(outcomes: &[PathOutcome], uncensored_only: bool) -> Scores {
    let mut s = Scores::default();

    for o in outcomes {
        if o.hours.len() < MIN_HOURS {
            continue;
        }
        let observed: Vec<f64> = o.hours.iter().map(|h| h.observed).collect();
        if uncensored_only && observed.iter().any(|v| *v < UNCENSORED_FLOOR_DB) {
            continue;
        }
        s.used += 1;

        let level = median(&mut observed.clone());
        let flat: Vec<f64> = vec![level; observed.len()];
        let take = |f: fn(&HourSample) -> f64| -> Vec<f64> { o.hours.iter().map(f).collect() };

        s.voacap_snr.add_path(&observed, &take(|h| h.voacap_snr));
        s.itu_snr.add_path(&observed, &take(|h| h.itu_snr));
        s.voacap_signal
            .add_path(&observed, &take(|h| h.voacap_signal));
        s.itu_signal.add_path(&observed, &take(|h| h.itu_signal));
        s.flat.add_path(&observed, &flat);
    }

    s
}

fn report(month: &str, ssn: f64, sporadic_e: bool, outcomes: &[PathOutcome], data_dir: &Path) {
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
        let observed: Vec<f64> = o.hours.iter().map(|h| h.observed).collect();
        let v: Vec<f64> = o.hours.iter().map(|h| h.voacap_snr).collect();
        let i: Vec<f64> = o.hours.iter().map(|h| h.itu_snr).collect();
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
        "{month}, smoothed sunspot number {ssn}, VOACAP sporadic-E {}. \
         {used} paths used of {} fetched.\n",
        if sporadic_e { "on" } else { "off" },
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

    let all = score(outcomes, false);
    println!("## All paths\n");
    println!("{TABLE_HEADER}");
    println!("{}", all.voacap_snr.line("VOACAP"));
    println!("{}", all.itu_snr.line("ITU-R P.533"));
    println!("{}", all.voacap_signal.line("VOACAP, signal only"));
    println!("{}", all.itu_signal.line("P.533, signal only"));
    println!("{}", all.flat.line("flat baseline"));
    println!(
        "\nErrors are in dB. Correlation and slope come from fitting \
         `observed = a + b * predicted` per path: correlation says whether the \
         peaks and troughs land in the right places, slope says whether the \
         model swings by the right amount, and the last column is what is left \
         once both are fitted.\n"
    );
    println!(
        "The signal-only rows score each engine's predicted received signal \
         with its noise prediction left out, as if the receiver's noise were \
         constant through the day. The models' noise swings strongly between \
         day and night, while a typical WSPR receiver's noise is set by local \
         interference and barely moves. The gap between an engine's row and \
         its signal-only row is the part of the exaggerated swing that comes \
         from the noise half of the prediction.\n"
    );

    let unc = score(outcomes, true);
    println!("## Paths that never approach the decoder's floor\n");
    println!(
        "{} paths whose weakest hour stays above {UNCENSORED_FLOOR_DB:.0} dB. \
         WSPR cannot report what it fails to decode, so weak hours read higher \
         than they were or vanish, which flattens the measured daily swing. On \
         these paths that cannot be happening, so anything that survives here is \
         the models rather than the measurement.\n",
        unc.used
    );
    println!("{TABLE_HEADER}");
    println!("{}", unc.voacap_snr.line("VOACAP"));
    println!("{}", unc.itu_snr.line("ITU-R P.533"));
    println!("{}", unc.voacap_signal.line("VOACAP, signal only"));
    println!("{}", unc.itu_signal.line("P.533, signal only"));
    println!("{}", unc.flat.line("flat baseline"));
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
