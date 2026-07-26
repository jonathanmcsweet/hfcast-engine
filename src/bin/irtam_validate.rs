//! Does real-time ionosphere data beat monthly climatology? Measured.
//!
//! Monthly climatology cannot know which days are good or bad — every day of
//! a month gets the same prediction. IRTAM refits the foF2 map every 15
//! minutes from ionosonde soundings, and its archive covers the validation
//! months. This program runs VOACAP twice per path: once as shipped
//! (climatology), and once per day with that day's IRTAM foF2 map written
//! into the run's private tree (see [`propcore::irtam`]). Both are scored
//! against the per-day WSPR medians.
//!
//! The decisive metric is the day-to-day one: correlation between predicted
//! and observed deviations from each path-hour's monthly median. Climatology
//! scores exactly zero here by construction, so any positive correlation is
//! value the real-time input added. Absolute error is reported too, offset-
//! adjusted per path as in the rest of the validation (antennas unknown).
//!
//! Usage: `irtam_validate --kp <kp-file> <month-dir> [<month-dir> …]`
//! Results are cached per month; delete `data/cache/<month>.irtam.csv` to
//! force a re-run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use propcore::deck::{build_deck, DeckCase};
use propcore::geomag::{self, GeomagTable};
use propcore::irtam;
use propcore::listing::parse_listing;
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};
use propcore::stats::{correlation, fit_line, median};
use propcore::wspr::{self, smoothed_ssn};

const VOACAP_VARIANT: &str = "O2";
const CONCURRENCY: usize = 4;
const MIN_SPOTS_PER_DAY: u32 = 4;
/// Below this a predicted ratio is a dead-path sentinel, not a prediction.
const IMPLAUSIBLE_SNR_DB: f64 = -200.0;

/// One scored triple: what happened and what each model said.
#[derive(Debug, Clone)]
struct Sample {
    /// Path identity, as an index into the month's path list.
    path: usize,
    day: u8,
    hour: u8,
    observed: f64,
    climatology: f64,
    irtam: f64,
}

/// Runs the engine for one deck inside a fresh private tree, optionally with
/// an IRTAM foF2 file patched in, and returns SNR per hour.
fn run_snr(
    bin: &Path,
    case: &DeckCase,
    tag: &str,
    fof2: Option<&[u8]>,
) -> Result<[Option<f64>; 24], String> {
    let deck = build_deck(case).map_err(|e| e.to_string())?;
    let root = IsolatedRoot::create(tag).map_err(|e| e.to_string())?;
    if let Some(bytes) = fof2 {
        root.replace_file("coeffs/fof2CCIR.daw", bytes)
            .map_err(|e| e.to_string())?;
    }
    let listing = run_deck(bin, root.path(), &deck).map_err(|e| e.to_string())?;
    let parsed = parse_listing(&listing);
    let mut snr = [None::<f64>; 24];
    for s in parsed.numeric.iter().filter(|s| s.slot == 0) {
        if s.row == "SNR" && s.value > IMPLAUSIBLE_SNR_DB {
            snr[s.hour as usize] = Some(s.value);
        }
    }
    Ok(snr)
}

/// Day numbers that have an IRTAM file, with the file's parsed map.
fn irtam_days(dir: &Path) -> Vec<(u8, irtam::IrtamMap)> {
    let mut days = Vec::new();
    let irtam_dir = dir.join("irtam");
    let Ok(entries) = std::fs::read_dir(&irtam_dir) else {
        return days;
    };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        // IRTAM_foF2_COEFFS_YYYYMMDD_HHMMSS.ASC — day is characters 24-25.
        let Some(day) = name
            .strip_prefix("IRTAM_foF2_COEFFS_")
            .and_then(|r| r.get(6..8))
            .and_then(|d| d.parse::<u8>().ok())
        else {
            continue;
        };
        match irtam::load_asc(&entry.path()) {
            Ok(Ok(map)) => days.push((day, map)),
            Ok(Err(e)) => eprintln!("{name}: {e}"),
            Err(e) => eprintln!("{name}: {e}"),
        }
    }
    days.sort_by_key(|(day, _)| *day);
    days
}

/// Runs both models for a month, or loads the cached result. The third
/// element is each path's frequency in MHz, indexed like `Sample::path`.
fn gather(dir: &Path, cache_dir: &Path) -> Result<(String, Vec<Sample>, Vec<f64>), String> {
    let data = wspr::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let freqs: Vec<f64> = data.paths.iter().map(|p| p.freq_mhz).collect();
    let cache_file = cache_dir.join(format!("{}.irtam.csv", data.month));
    if let Some(samples) = load_cache(&cache_file) {
        eprintln!("{}: {} samples (cached)", data.month, samples.len());
        return Ok((data.month, samples, freqs));
    }

    let daily = wspr::load_daily(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (Some(year), Some(month)) = (data.year(), data.month_number()) else {
        return Err(format!("{}: unreadable month.txt", dir.display()));
    };
    let ssn =
        smoothed_ssn(&data.month).ok_or_else(|| format!("no smoothed SSN for {}", data.month))?;
    let days = irtam_days(dir);
    if days.is_empty() {
        return Err(format!(
            "{}: no IRTAM files; run tools/fetch-irtam.sh {}",
            dir.display(),
            data.month
        ));
    }
    let fof2_by_day: Vec<(u8, Vec<u8>)> = days
        .iter()
        .map(|(day, map)| (*day, irtam::daw_file(map)))
        .collect();
    let bin = variant_bin(VOACAP_VARIANT);

    // One work item per path: the climatology run plus one run per IRTAM day.
    // Parallelism across paths keeps each private tree single-threaded.
    let outcomes = map_limit(&data.paths, CONCURRENCY, |path, index| {
        let case = DeckCase {
            id: format!("iv{index}"),
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
            sporadic_e: true,
        };
        let climatology = run_snr(&bin, &case, &format!("iv-{index}-c"), None)?;

        // Observed daily medians for this path, keyed by (day, hour).
        let mut observed: HashMap<(u8, u8), f64> = HashMap::new();
        if let Some(samples) = daily.get(&path.key()) {
            for s in samples {
                if s.reports >= MIN_SPOTS_PER_DAY {
                    observed.insert((s.day, s.hour), s.snr_median);
                }
            }
        }

        let mut samples = Vec::new();
        for (day, fof2) in &fof2_by_day {
            // Skip days with no observations at all: nothing to score.
            if !observed.keys().any(|(d, _)| d == day) {
                continue;
            }
            let irtam_snr = run_snr(&bin, &case, &format!("iv-{index}-d{day}"), Some(fof2))?;
            for hour in 0..24u8 {
                let (Some(&obs), Some(clim), Some(irt)) = (
                    observed.get(&(*day, hour)),
                    climatology[hour as usize],
                    irtam_snr[hour as usize],
                ) else {
                    continue;
                };
                samples.push(Sample {
                    path: index,
                    day: *day,
                    hour,
                    observed: obs,
                    climatology: clim,
                    irtam: irt,
                });
            }
        }
        Ok::<Vec<Sample>, String>(samples)
    });

    let mut samples = Vec::new();
    let mut failures = 0usize;
    for outcome in outcomes {
        match outcome {
            Ok(mut s) => samples.append(&mut s),
            Err(_) => failures += 1,
        }
    }
    eprintln!(
        "{}: {} samples from {} paths ({} failed)",
        data.month,
        samples.len(),
        data.paths.len(),
        failures
    );
    save_cache(&cache_file, &samples);
    Ok((data.month, samples, freqs))
}

fn save_cache(path: &Path, samples: &[Sample]) {
    let mut out = String::from("path,day,hour,observed,climatology,irtam\n");
    for s in samples {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            s.path, s.day, s.hour, s.observed, s.climatology, s.irtam
        ));
    }
    if let Err(e) = std::fs::write(path, out) {
        eprintln!("cache write failed ({}): {e}", path.display());
    }
}

fn load_cache(path: &Path) -> Option<Vec<Sample>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "path,day,hour,observed,climatology,irtam" {
        return None;
    }
    let mut samples = Vec::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 6 {
            return None;
        }
        samples.push(Sample {
            path: fields[0].parse().ok()?,
            day: fields[1].parse().ok()?,
            hour: fields[2].parse().ok()?,
            observed: fields[3].parse().ok()?,
            climatology: fields[4].parse().ok()?,
            irtam: fields[5].parse().ok()?,
        });
    }
    Some(samples)
}

/// Median absolute error after removing one offset per path.
fn offset_adjusted_mae(samples: &[Sample], pick: &dyn Fn(&Sample) -> f64) -> f64 {
    let mut by_path: HashMap<usize, Vec<f64>> = HashMap::new();
    for s in samples {
        by_path
            .entry(s.path)
            .or_default()
            .push(s.observed - pick(s));
    }
    let offsets: HashMap<usize, f64> = by_path
        .into_iter()
        .map(|(p, mut residuals)| (p, median(&mut residuals)))
        .collect();
    let mut errors: Vec<f64> = samples
        .iter()
        .map(|s| (s.observed - pick(s) - offsets[&s.path]).abs())
        .collect();
    median(&mut errors)
}

/// One deviation pair: how far the day sat from its path-hour's monthly
/// median, observed and as the model predicted, plus the day's Kp history
/// and the path's frequency.
struct DeviationPair {
    observed: f64,
    predicted: f64,
    kp_max_24h: Option<f64>,
    freq_mhz: f64,
}

/// Deviations of observation and of a model from their own per-path-hour
/// monthly medians. This is where climatology is zero by construction.
fn deviations(
    samples: &[Sample],
    pick: &dyn Fn(&Sample) -> f64,
    kp: impl Fn(&Sample) -> Option<f64>,
    freqs: &[f64],
) -> Vec<DeviationPair> {
    let mut obs_by_hour: HashMap<(usize, u8), Vec<f64>> = HashMap::new();
    let mut pred_by_hour: HashMap<(usize, u8), Vec<f64>> = HashMap::new();
    for s in samples {
        obs_by_hour
            .entry((s.path, s.hour))
            .or_default()
            .push(s.observed);
        pred_by_hour
            .entry((s.path, s.hour))
            .or_default()
            .push(pick(s));
    }
    let centre = |m: &HashMap<(usize, u8), Vec<f64>>| -> HashMap<(usize, u8), f64> {
        m.iter()
            .filter(|(_, v)| v.len() >= 5)
            .map(|(k, v)| (*k, median(&mut v.clone())))
            .collect()
    };
    let obs_centre = centre(&obs_by_hour);
    let pred_centre = centre(&pred_by_hour);

    let mut pairs = Vec::new();
    for s in samples {
        let key = (s.path, s.hour);
        let (Some(oc), Some(pc)) = (obs_centre.get(&key), pred_centre.get(&key)) else {
            continue;
        };
        pairs.push(DeviationPair {
            observed: s.observed - oc,
            predicted: pick(s) - pc,
            kp_max_24h: kp(s),
            freq_mhz: freqs.get(s.path).copied().unwrap_or(0.0),
        });
    }
    pairs
}

fn deviation_row(label: &str, pairs: &[&DeviationPair]) {
    let obs: Vec<f64> = pairs.iter().map(|p| p.observed).collect();
    let pred: Vec<f64> = pairs.iter().map(|p| p.predicted).collect();
    let corr = correlation(&obs, &pred).map_or("n/a".to_string(), |c| format!("{c:+.3}"));
    let slope = fit_line(&obs, &pred).map_or("n/a".to_string(), |(_, b)| format!("{b:.3}"));
    let mut sizes: Vec<f64> = pred.iter().map(|d| d.abs()).collect();
    let mut observed_sizes: Vec<f64> = obs.iter().map(|d| d.abs()).collect();
    println!(
        "| {label} | {} | {corr} | {slope} | {:.2} | {:.2} |",
        pairs.len(),
        median(&mut sizes),
        median(&mut observed_sizes),
    );
}

fn report(month: &str, samples: &[Sample], freqs: &[f64], table: Option<&GeomagTable>) {
    println!("## {month} ({} path-day-hours)\n", samples.len());

    let clim = |s: &Sample| s.climatology;
    let irt = |s: &Sample| s.irtam;

    println!("Absolute error, one offset per path (median absolute error, dB):\n");
    println!("| model | error |");
    println!("| --- | --: |");
    println!(
        "| climatology | {:.2} |",
        offset_adjusted_mae(samples, &clim)
    );
    println!("| IRTAM foF2 | {:.2} |", offset_adjusted_mae(samples, &irt));

    let (year, month_no) = month
        .split_once('-')
        .and_then(|(y, m)| Some((y.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or((0, 0));
    let kp_of =
        |s: &Sample| table.and_then(|t| t.kp_max_lookback(year, month_no, s.day, s.hour, 24));
    let pairs = deviations(samples, &irt, kp_of, freqs);

    println!("\nDay-to-day deviations from each path-hour's monthly median.");
    println!("Climatology predicts zero deviation for every day, so any");
    println!("positive correlation is information climatology cannot have:\n");
    println!(
        "| condition | day-hours | correlation | slope | predicted size (dB) | observed size (dB) |"
    );
    println!("| --- | --: | --: | --: | --: | --: |");
    deviation_row("all days", &pairs.iter().collect::<Vec<_>>());
    if table.is_some() {
        let group = |lo: f64, hi: f64| {
            pairs
                .iter()
                .filter(|p| p.kp_max_24h.is_some_and(|k| k >= lo && k < hi))
                .collect::<Vec<_>>()
        };
        deviation_row("quiet (Kp < 3)", &group(0.0, 3.0));
        deviation_row("unsettled (3-5)", &group(3.0, 5.0));
        deviation_row("storm (Kp >= 5)", &group(5.0, 10.0));
    }
    // Higher bands run closer to the MUF, where foF2 decides everything;
    // low bands are ruled by absorption and noise instead. If the real-time
    // map helps anywhere, it is at the top.
    let band = |lo: f64, hi: f64| {
        pairs
            .iter()
            .filter(|p| p.freq_mhz >= lo && p.freq_mhz < hi)
            .collect::<Vec<_>>()
    };
    deviation_row("bands up to 8 MHz", &band(0.0, 8.0));
    deviation_row("bands 8-15 MHz", &band(8.0, 15.0));
    deviation_row("bands above 15 MHz", &band(15.0, 60.0));
    println!();
}

fn main() -> ExitCode {
    let dirs: Vec<PathBuf> = std::env::args()
        .skip(1)
        .filter(|a| !a.starts_with("--"))
        .map(PathBuf::from)
        .collect();
    if dirs.is_empty() {
        eprintln!("usage: irtam_validate <month-dir> [<month-dir> …]");
        return ExitCode::FAILURE;
    }
    if !variant_bin(VOACAP_VARIANT).is_file() {
        eprintln!("no voacapl variant binary; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }
    let cache_dir = PathBuf::from("data/cache");
    if let Err(e) = std::fs::create_dir_all(&cache_dir) {
        eprintln!("cannot create {}: {e}", cache_dir.display());
        return ExitCode::FAILURE;
    }

    let table = geomag::load(&PathBuf::from("data/kp_daily.txt")).ok();
    if table.is_none() {
        eprintln!("no data/kp_daily.txt; storm split skipped (tools/fetch-kp.sh)");
    }

    println!("# Real-time foF2 against monthly climatology\n");
    println!(
        "Same engine, same configuration, one change: the foF2 coefficient \
         file is replaced per day with the IRTAM map for that day. Scored \
         against per-day WSPR medians.\n"
    );
    for dir in &dirs {
        match gather(dir, &cache_dir) {
            Ok((month, samples, freqs)) => report(&month, &samples, &freqs, table.as_ref()),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
