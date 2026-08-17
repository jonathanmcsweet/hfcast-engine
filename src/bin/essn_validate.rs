//! Does the fitted daily index beat monthly climatology on real links?
//!
//! `docs/ionosonde.md` scores the daily conditioning against ionosonde
//! truth. This program asks the user-level question with the ruler
//! `docs/irtam.md` used: predicted SNR against per-day WSPR medians on
//! real paths. Two engine runs per path-day question, both through the
//! Rust engine's own API: as shipped (the month's smoothed sunspot
//! number), and conditioned on the day's fitted index from the same
//! GIRO soundings a deployed device would read (`sonde::essn_series`).
//! The WSPR paths are independent of the fit: the index comes from
//! ionosondes, the truth from radio links.
//!
//! The daily run applies the same floor `Conditioning::Daily` does: an
//! index below zero runs the engine at zero — below the map's lower
//! plane there is no measured state for the other channels to
//! extrapolate into — with a synthesized coefficient overlay
//! (`irtam::ccir_at`) pinning foF2 alone to the fitted line.
//!
//! The decisive metric is day-to-day: correlation between predicted and
//! observed deviations from each path-hour's monthly median, where
//! climatology scores exactly zero by construction. Absolute error is
//! offset-adjusted per path (antennas and local noise are unknown but
//! constant).
//!
//! Needs the embedded coefficients:
//! `cargo run --release --all-features --bin essn_validate --
//!  [--kp data/kp_daily.txt] [--stations tools/giro-stations.tsv]
//!  data/YYYY-MM ...`
//! Results are cached per month; delete `data/cache/<month>.essnv.csv`
//! to force a re-run.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hfcast::api::{predict, FoF2Model, Ionosphere, Model, Report, Request, Site, Task};
use hfcast::geomag::{self, GeomagTable};
use hfcast::stats::{correlation, median_in_place};
use hfcast::voacap::data;
use hfcast::wspr::{self, deviations, offset_adjusted_mae, Scored, WsprPath};
use hfcast::{irtam, sonde};

const MIN_SPOTS_PER_DAY: u32 = 4;
/// Below this a predicted ratio is a dead-path sentinel, not a prediction.
const IMPLAUSIBLE_SNR_DB: f64 = -200.0;

/// One scored triple: what happened and what each model said.
#[derive(Debug, Clone)]
struct Sample {
    path: usize,
    day: u8,
    hour: u8,
    observed: f64,
    climatology: f64,
    essn: f64,
}

/// The engine request for one WSPR path at one sunspot number.
fn path_request(path: &WsprPath, month: u32, ssn: f64) -> Request {
    Request {
        tx: Site {
            name: path.tx.clone(),
            lat_deg: path.tx_lat,
            lon_deg: path.tx_lon,
        },
        rx: Site {
            name: path.rx.clone(),
            lat_deg: path.rx_lat,
            lon_deg: path.rx_lon,
        },
        month,
        year: 2026,
        ssn,
        power_watts: path.watts(),
        freqs_mhz: vec![path.freq_mhz],
        required_snr_db: 24.0,
        noise_dbw: 145.0,
        fof2: FoF2Model::Ccir,
        layer_multipliers: [1.0, 1.0, 1.0, 1.0],
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        ionosphere: Ionosphere::default(),
        model: Model::Compatible,
    }
}

/// Predicted SNR per UT hour for the request's one frequency. VOACAP's
/// hour 24 is the day's midnight and lands in slot 0.
fn snr_hours(root: &Path, req: &Request) -> Result<[Option<f64>; 24], String> {
    let Report::Systems(prediction) = predict(root, req, Task::Systems)? else {
        return Err("Systems task answered with a different report".to_string());
    };
    let mut snr = [None::<f64>; 24];
    // Indexed writes keep the 24-to-0 hour fold in one place.
    for hour in &prediction.hours {
        let value = f64::from(hour.son[0].sndb);
        if value > IMPLAUSIBLE_SNR_DB {
            snr[(hour.gmt as usize) % 24] = Some(value);
        }
    }
    Ok(snr)
}

/// Runs both models for a month, or loads the cached result.
fn gather(dir: &Path, stations: &Path, cache_dir: &Path) -> Result<(String, Vec<Sample>), String> {
    let data = wspr::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let cache_file = cache_dir.join(format!("{}.essnv.csv", data.month));
    if let Some(samples) = load_cache(&cache_file) {
        eprintln!("{}: {} samples (cached)", data.month, samples.len());
        return Ok((data.month, samples));
    }

    let daily = wspr::load_daily(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let month_number = data
        .month_number()
        .ok_or_else(|| format!("{}: unreadable month.txt", dir.display()))?;
    let ssn = wspr::smoothed_ssn(&data.month)
        .ok_or_else(|| format!("no smoothed SSN for {}", data.month))?;
    let index_by_day: BTreeMap<u8, f64> = sonde::essn_series(dir, stations)?;
    if index_by_day.is_empty() {
        return Err(format!("{}: no fitted days", data.month));
    }
    let runs = day_runs(&index_by_day, &data.month, month_number, cache_dir)?;

    let mut samples = Vec::new();
    // A loop per path: every iteration runs the engine, and the first
    // error ends the month.
    for (index, path) in data.paths.iter().enumerate() {
        let observed: HashMap<(u8, u8), f64> = daily
            .get(&path.key())
            .map(|rows| {
                rows.iter()
                    .filter(|s| s.reports >= MIN_SPOTS_PER_DAY)
                    .map(|s| ((s.day, s.hour), s.snr_median))
                    .collect()
            })
            .unwrap_or_default();
        samples.extend(path_samples(
            path,
            index,
            month_number,
            ssn,
            &runs,
            &observed,
        )?);
    }
    eprintln!(
        "{}: {} samples from {} paths, {} fitted days",
        data.month,
        samples.len(),
        data.paths.len(),
        index_by_day.len()
    );
    save_cache(&cache_file, &samples);
    Ok((data.month, samples))
}

/// The engine run behind each fitted day: the index itself at or above
/// the map's lower plane; below it, the run floors at zero and a
/// synthesized coefficient overlay pins foF2 to the fitted line — the
/// same floor `Conditioning::Daily` applies (`src/truecast/api.rs`).
fn day_runs(
    index_by_day: &BTreeMap<u8, f64>,
    month: &str,
    month_number: u32,
    cache_dir: &Path,
) -> Result<BTreeMap<u8, (PathBuf, f64)>, String> {
    index_by_day
        .iter()
        .map(|(day, essn)| {
            if *essn >= 0.0 {
                return Ok((*day, (data::embedded_root(), *essn)));
            }
            let map = irtam::ccir_at(&data::embedded_root(), month_number, *essn)?;
            let dir = cache_dir.join(format!("essnv-overlay-{month}-{day:02}"));
            let root = irtam::overlay_with(&map, &dir)?;
            Ok((*day, (root, 0.0)))
        })
        .collect()
}

/// One path's scored triples: the climatology run once, then one run
/// per fitted day that has observations.
fn path_samples(
    path: &WsprPath,
    index: usize,
    month_number: u32,
    ssn: f64,
    runs: &BTreeMap<u8, (PathBuf, f64)>,
    observed: &HashMap<(u8, u8), f64>,
) -> Result<Vec<Sample>, String> {
    let climatology = snr_hours(
        &data::embedded_root(),
        &path_request(path, month_number, ssn),
    )?;
    let mut samples = Vec::new();
    // A loop per fitted day: each iteration is an engine run.
    for (day, (root, run_ssn)) in runs {
        if !observed.keys().any(|(d, _)| d == day) {
            continue;
        }
        let daily_snr = snr_hours(root, &path_request(path, month_number, *run_ssn))?;
        for hour in 0..24u8 {
            let (Some(&obs), Some(clim), Some(daily_value)) = (
                observed.get(&(*day, hour)),
                climatology[usize::from(hour)],
                daily_snr[usize::from(hour)],
            ) else {
                continue;
            };
            samples.push(Sample {
                path: index,
                day: *day,
                hour,
                observed: obs,
                climatology: clim,
                essn: daily_value,
            });
        }
    }
    Ok(samples)
}

fn save_cache(path: &Path, samples: &[Sample]) {
    let mut out = String::from("path,day,hour,observed,climatology,essn\n");
    for s in samples {
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            s.path, s.day, s.hour, s.observed, s.climatology, s.essn
        ));
    }
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, out) {
        eprintln!("cache write failed ({}): {e}", path.display());
    }
}

fn load_cache(path: &Path) -> Option<Vec<Sample>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next()? != "path,day,hour,observed,climatology,essn" {
        return None;
    }
    lines
        .map(|line| {
            let fields: Vec<&str> = line.split(',').collect();
            let [path, day, hour, observed, climatology, essn] = fields[..] else {
                return None;
            };
            Some(Sample {
                path: path.parse().ok()?,
                day: day.parse().ok()?,
                hour: hour.parse().ok()?,
                observed: observed.parse().ok()?,
                climatology: climatology.parse().ok()?,
                essn: essn.parse().ok()?,
            })
        })
        .collect()
}

/// One model's samples as the shared scoring shape.
fn scored(samples: &[Sample], pick: &dyn Fn(&Sample) -> f64) -> Vec<Scored> {
    samples
        .iter()
        .map(|s| Scored {
            path: s.path,
            day: s.day,
            hour: s.hour,
            observed: s.observed,
            predicted: pick(s),
        })
        .collect()
}

fn deviation_row(label: &str, pairs: &[&wspr::DeviationPair]) {
    let obs: Vec<f64> = pairs.iter().map(|p| p.observed).collect();
    let pred: Vec<f64> = pairs.iter().map(|p| p.predicted).collect();
    let corr = correlation(&obs, &pred).map_or("n/a".to_string(), |c| format!("{c:+.3}"));
    let mut sizes: Vec<f64> = pred.iter().map(|d| d.abs()).collect();
    let mut observed_sizes: Vec<f64> = obs.iter().map(|d| d.abs()).collect();
    println!(
        "| {label} | {} | {corr} | {:.2} | {:.2} |",
        pairs.len(),
        median_in_place(&mut sizes),
        median_in_place(&mut observed_sizes),
    );
}

fn report(month: &str, samples: &[Sample], table: Option<&GeomagTable>) {
    println!("## {month} ({} path-day-hours)\n", samples.len());

    let clim = scored(samples, &|s| s.climatology);
    let essn = scored(samples, &|s| s.essn);
    println!("Absolute error, one offset per path (median absolute error, dB):\n");
    println!("| model | error |");
    println!("| --- | --: |");
    println!("| climatology | {:.2} |", offset_adjusted_mae(&clim));
    println!("| essn | {:.2} |", offset_adjusted_mae(&essn));

    let (year, month_no) = month
        .split_once('-')
        .and_then(|(y, m)| Some((y.parse::<u32>().ok()?, m.parse::<u32>().ok()?)))
        .unwrap_or((0, 0));
    let pairs = deviations(&essn);
    let guard = deviations(&clim);
    let guard_corr = correlation(
        &guard.iter().map(|p| p.observed).collect::<Vec<_>>(),
        &guard.iter().map(|p| p.predicted).collect::<Vec<_>>(),
    )
    .unwrap_or(0.0);

    println!("\nDay-to-day deviations from each path-hour's monthly median");
    println!(
        "(climatology guard: {guard_corr:+.3}, must be +0.000 — a model that \
         never varies by day cannot correlate):\n"
    );
    println!("| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |");
    println!("| --- | --: | --: | --: | --: |");
    deviation_row("all days", &pairs.iter().collect::<Vec<_>>());
    if table.is_some() {
        let kp_of = |p: &wspr::DeviationPair| {
            table.and_then(|t| t.kp_max_lookback(year, month_no, p.day, p.hour, 24))
        };
        let group = |lo: f64, hi: f64| {
            pairs
                .iter()
                .filter(|p| kp_of(p).is_some_and(|k| k >= lo && k < hi))
                .collect::<Vec<_>>()
        };
        deviation_row("quiet (Kp < 3)", &group(0.0, 3.0));
        deviation_row("unsettled (3-5)", &group(3.0, 5.0));
        deviation_row("storm (Kp >= 5)", &group(5.0, 10.0));
    }
    println!();
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).peekable();
    let mut kp: Option<PathBuf> = None;
    let mut stations = PathBuf::from("tools/giro-stations.tsv");
    let mut months: Vec<PathBuf> = Vec::new();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--kp" => kp = args.next().map(PathBuf::from),
            "--stations" => {
                if let Some(path) = args.next() {
                    stations = PathBuf::from(path);
                }
            }
            _ => months.push(PathBuf::from(arg)),
        }
    }
    if months.is_empty() {
        eprintln!(
            "usage: essn_validate [--kp data/kp_daily.txt] \
             [--stations tools/giro-stations.tsv] data/YYYY-MM ..."
        );
        return ExitCode::FAILURE;
    }
    let table = kp.as_deref().and_then(|path| geomag::load(path).ok());
    if table.is_none() {
        eprintln!("no Kp table; storm split skipped");
    }

    println!("# The fitted daily index against monthly climatology, on real links\n");
    println!(
        "Same engine, same configuration, one change: the sunspot number is \
         the day's fitted index from GIRO soundings instead of the month's \
         smoothed value. Scored against per-day WSPR medians.\n"
    );
    for dir in &months {
        match gather(dir, &stations, Path::new("data/cache")) {
            Ok((month, samples)) => report(&month, &samples, table.as_ref()),
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
