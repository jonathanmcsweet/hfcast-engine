//! Scores predicted ionospheric characteristics against ionosonde truth.
//!
//! For each month bundle: engine runs over station-centered probe paths,
//! compared with the station's own scaled soundings. Models scored:
//! climatology as shipped, and climatology with each day's IRTAM foF2 map
//! written over the coefficient file. See `src/sonde.rs` for the method
//! and `docs/ionosonde.md` for the results.
//!
//! Needs the embedded coefficients: `cargo run --release --all-features
//! --bin sonde -- --kp data/kp_daily.txt data/2025-06 ...`
//!
//! `--check <dir>` prints what a bundle holds and runs nothing.
//! `--fit-storm` fits the storm ratio table (`src/stormfit.rs`) from the
//! given months and prints it as Rust source instead of a report.
//! `--fit-edge` fits the absorption-edge level model
//! (`EDGE_RATIO_MODEL`: ln ratio over index and season harmonics) on
//! the given months, leaving the held-out months out of the fit, and
//! prints the held-out verdict.
//! `--fit-offline` fits the never-online day-of-year correction curve
//! (`OFFLINE_ANOMALY_MODEL`) the same way and prints its verdict.
//! `--fit-sync` fits the sync-decay weight (`SYNC_DECAY`) and prints
//! the held-out staleness-ladder verdict.
//! `--sync-record` prints the JSON a build bakes into the app: the
//! last measured day's index and anomaly for the given month.
//! `--engine truecast` replays the truecast point API over the cached
//! cells and fails if it disagrees with the research columns.
//! `--ledger` prints one CSV line per month: the most recent day with
//! samples, scored on its own rows — the live loop's trend line
//! (`tools/live-check.sh`, `docs/soak.md`).
//! `--daily` prints one CSV line for every day of every month given —
//! the whole-archive daily comparison (`tools/backfill.sh`).

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hfcast::geomag::{self, GeomagTable};
use hfcast::giro::{self, StationMeta};
use hfcast::sonde::{
    self, day_to_day, errors, nvis_cells, secant_factor, BandCalls, Sample, NVIS_BANDS_MHZ,
    NVIS_RANGES_KM, STORM_KP,
};
use hfcast::stormfit;
use hfcast::truecast::api::{self as truecast, Conditioning};

/// How many days the bundle's month really has, so the `--check` view
/// does not report a complete April as 30 of 31.
fn days_in_month(name: &str) -> u32 {
    let Some((year, month)) = year_month(name) else {
        return 31;
    };
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    match month {
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => 31,
    }
}

fn check(dir: &Path) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let wspr = dir.join("month.txt").is_file();
    let days = days_in_month(name);
    let irtam = (1..=days)
        .filter(|day| {
            let file = format!(
                "IRTAM_foF2_COEFFS_{}_234500.ASC",
                format_args!("{}{:02}", name.replace('-', ""), day)
            );
            dir.join("irtam").join(file).is_file()
        })
        .count();
    let giro = std::fs::read_dir(dir.join("giro"))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .filter(|e| e.path().is_dir())
                .count()
        })
        .unwrap_or(0);
    println!(
        "{name}: wspr {}, irtam foF2 {irtam}/{days} days, giro {giro} stations",
        ok(wspr)
    );
}

fn ok(present: bool) -> &'static str {
    if present {
        "present"
    } else {
        "absent"
    }
}

/// (year, month) from a `YYYY-MM` bundle name.
fn year_month(month: &str) -> Option<(u32, u32)> {
    let (y, m) = month.split_once('-')?;
    Some((y.parse().ok()?, m.parse().ok()?))
}

/// One error row: label, then bias / MAE / RMS / n, or a dash when empty.
fn error_row(label: &str, pairs: &[(f64, f64)]) {
    match errors(pairs) {
        Some((bias, mae, rms)) => println!(
            "| {label:<24} | {bias:+7.3} | {mae:6.3} | {rms:6.3} | {n:5} |",
            n = pairs.len()
        ),
        None => println!("| {label:<24} |       - |      - |      - |     0 |"),
    }
}

/// Pairs of (model, observed) for one characteristic under one model
/// column, optionally restricted by a day filter.
fn pairs(
    samples: &[Sample],
    characteristic: &str,
    pick: &dyn Fn(&Sample) -> Option<f64>,
    keep: &dyn Fn(&Sample) -> bool,
) -> Vec<(f64, f64)> {
    samples
        .iter()
        .filter(|s| s.characteristic == characteristic && keep(s))
        .filter_map(|s| Some((pick(s)?, s.observed)))
        .collect()
}

/// The pairs of one characteristic with each station's constant offset
/// removed: per station, the median (predicted - observed) is
/// subtracted from every prediction. For fmin this is the score that
/// matters — the probe's link budget and the sounder's threshold are
/// unknown but constant, so the level is theirs and the residual is
/// the model's. The same argument the WSPR paths use.
fn offset_adjusted_pairs(
    samples: &[Sample],
    characteristic: &str,
    pick: &dyn Fn(&Sample) -> Option<f64>,
) -> Vec<(f64, f64)> {
    let mut by_station: BTreeMap<&str, Vec<(f64, f64)>> = BTreeMap::new();
    for s in samples
        .iter()
        .filter(|s| s.characteristic == characteristic)
    {
        if let Some(predicted) = pick(s) {
            by_station
                .entry(s.station.as_str())
                .or_default()
                .push((predicted, s.observed));
        }
    }
    by_station
        .into_values()
        .flat_map(|rows| {
            let mut diffs: Vec<f64> = rows.iter().map(|(p, o)| p - o).collect();
            let offset = hfcast::stats::median_in_place(&mut diffs);
            rows.into_iter()
                .map(move |(p, o)| (p - offset, o))
                .collect::<Vec<_>>()
        })
        .collect()
}

/// Whether the sample's day-hour sits at or above the storm threshold,
/// judged over the trailing 24 hours. None when the Kp file lacks the day.
fn storminess(table: Option<&GeomagTable>, month: &str, s: &Sample) -> Option<bool> {
    let (year, mm) = year_month(month)?;
    let kp = table?.kp_max_lookback(year, mm, s.day, s.hour, 24)?;
    Some(kp >= STORM_KP)
}

/// One month's (bin, observed/predicted) storm-fit samples: every foF2
/// sample that has an essn prediction and a known trailing-24-hour Kp.
fn storm_samples(
    month: &str,
    samples: &[Sample],
    table: &GeomagTable,
    stations: &BTreeMap<String, StationMeta>,
) -> Vec<(usize, f64)> {
    let Some((year, mm)) = year_month(month) else {
        return Vec::new();
    };
    // The phantom day 31 is dropped as in `--daily` (roadmap: bound
    // the gather) so a refit never learns from double-counted samples.
    let limit = days_in_month(month) as u8;
    samples
        .iter()
        .filter(|s| s.day <= limit)
        .filter(|s| s.characteristic == "foF2")
        .filter_map(|s| {
            let predicted = s.essn.filter(|value| *value > 0.0)?;
            let meta = stations.get(&s.station)?;
            let kp = table.kp_max_lookback(year, mm, s.day, s.hour, 24)?;
            Some((
                stormfit::bin(mm, meta.lat, meta.lon, s.hour, kp),
                s.observed / predicted,
            ))
        })
        .collect()
}

/// The held-out months: never in a fit, always in the verdict.
///
/// Chosen by rule from the Kp record and the solar cycle before the
/// 2026-08 whole-archive refit, so the verdict covers every stratum
/// the table claims to serve (see `docs/ionosonde.md`):
/// - 2015-03, 2022-09: the original pair, held out since the first fit.
/// - 2024-05: peak Kp 9.0, the strongest month in the record.
/// - 2018-08: peak Kp 7.3, the only severe month of the deep minimum.
/// - 2019-03 (quiet March, minimum) and 2024-03 (severe March,
///   maximum): the lower-edge season verdict.
/// - 2020-05 (peak Kp 3.3, quietest month in the record) and 2024-01
///   (quietest solar-maximum month, winter): the quiet-safety verdict.
const HELD_OUT: [&str; 8] = [
    "2015-03", "2018-08", "2019-03", "2020-05", "2022-09", "2024-01", "2024-03", "2024-05",
];

/// One fmin row of the edge fit: where it was measured, when, the
/// observed fmin, the day-conditioned probe edge, and the day's index.
struct EdgeRow {
    station: String,
    day: u8,
    observed: f64,
    edge: f64,
    index: f64,
}

/// One month's fmin rows: every sample with a day-conditioned probe
/// edge and a fitted day index. The phantom day 31 is dropped as in
/// `--daily` (roadmap: bound the gather) so a refit never learns from
/// double-counted samples.
fn edge_rows(month: &str, samples: &[Sample]) -> Vec<EdgeRow> {
    let limit = days_in_month(month) as u8;
    samples
        .iter()
        .filter(|s| s.day <= limit)
        .filter(|s| s.characteristic == "fmin" && s.observed > 0.0)
        .filter_map(|s| {
            let (edge, index) = s.essn.zip(s.essn_index)?;
            Some(EdgeRow {
                station: s.station.clone(),
                day: s.day,
                observed: s.observed,
                edge,
                index,
            })
        })
        .collect()
}

/// The `--fit-storm` mode: gather the months, fit, print the table.
fn run_fit_storm(
    args: &Args,
    table: Option<&GeomagTable>,
    station_meta: &BTreeMap<String, StationMeta>,
) -> ExitCode {
    let Some(table) = table.filter(|t| !t.is_empty()) else {
        eprintln!("--fit-storm needs --kp with a readable file");
        return ExitCode::FAILURE;
    };
    let mut fit_samples = Vec::new();
    if !over_months(args, &mut |month, samples| {
        if HELD_OUT.contains(&month) {
            eprintln!("{month}: held out, not fitted");
            return;
        }
        fit_samples.extend(storm_samples(month, samples, table, station_meta));
    }) {
        return ExitCode::FAILURE;
    }
    fit_storm_report(&fit_samples);
    ExitCode::SUCCESS
}

/// The `--ledger` mode: the trend line the live loop appends per run.
fn run_ledger(args: &Args) -> ExitCode {
    println!(
        "month,day,n_fof2,essn_bias,essn_mae,clim_bias,clim_mae,\
         essn_index,n_fmin,edge_bias,edge_mae"
    );
    let mut all = true;
    if !over_months(
        args,
        &mut |month, samples| match ledger_line(month, samples) {
            Some(line) => println!("{month},{line}"),
            None => all = false,
        },
    ) || !all
    {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// Scores the most recent day with samples on its own rows. During a
/// live day the day is partial and its numbers firm up run by run —
/// the ledger records the scored day, so repeated lines for one day
/// are the day filling in, not a fault.
fn ledger_line(month: &str, samples: &[Sample]) -> Option<String> {
    let day = samples.iter().map(|s| s.day).max()?;
    day_line(month, samples, day)
}

/// The `--daily` mode: every day of every month given, one line each —
/// the whole-archive daily comparison behind `docs/comparison.md`. The
/// last column is the day's peak Kp (the storm meter), for storm
/// marking in whatever reads the file.
fn run_daily(args: &Args, table: Option<&GeomagTable>) -> ExitCode {
    println!(
        "month,day,n_fof2,essn_bias,essn_mae,clim_bias,clim_mae,\
         essn_index,n_fmin,edge_bias,edge_mae,kp_max"
    );
    if !over_months(args, &mut |month, samples| {
        // The gather probes days 1..=31 whatever the month, and the
        // last phantom day catches the true last day's final half hour
        // through the ±30-minute window (roadmap: bound the gather).
        // The daily view drops it rather than print a June 31.
        let limit = days_in_month(month) as u8;
        let days: BTreeSet<u8> = samples
            .iter()
            .map(|s| s.day)
            .filter(|d| *d <= limit)
            .collect();
        for day in days {
            let kp = year_month(month)
                .and_then(|(y, m)| table?.kp_max_lookback(y, m, day, 23, 24))
                .map_or(String::new(), |kp| format!("{kp:.1}"));
            if let Some(line) = day_line(month, samples, day) {
                println!("{month},{line},{kp}");
            }
        }
    }) {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// One day scored on its own rows: both engines against the day's
/// observations, the day's fitted index, and the calibrated edge.
fn day_line(month: &str, samples: &[Sample], day: u8) -> Option<String> {
    let (_, month_number) = year_month(month)?;
    let rows: Vec<&Sample> = samples.iter().filter(|s| s.day == day).collect();
    let against = |characteristic: &str, pick: &dyn Fn(&Sample) -> Option<f64>| {
        let pairs: Vec<(f64, f64)> = rows
            .iter()
            .filter(|s| s.characteristic == characteristic)
            .filter_map(|s| Some((pick(s)?, s.observed)))
            .collect();
        (pairs.len(), errors(&pairs))
    };
    let two = |stats: Option<(f64, f64, f64)>| match stats {
        Some((bias, mae, _)) => format!("{bias:+.3},{mae:.3}"),
        None => ",".to_string(),
    };
    let (n_fof2, essn) = against("foF2", &|s| s.essn);
    let (_, clim) = against("foF2", &|s| Some(s.climatology));
    let (n_fmin, edge) = against("fmin", &|s| {
        let (e, index) = s.essn.zip(s.essn_index)?;
        Some(e / truecast::edge_fmin_ratio(month_number, index))
    });
    let mut indexes: Vec<f64> = rows.iter().filter_map(|s| s.essn_index).collect();
    let index = if indexes.is_empty() {
        String::new()
    } else {
        format!("{:.1}", hfcast::stats::median_in_place(&mut indexes))
    };
    Some(format!(
        "{day},{n_fof2},{},{},{index},{n_fmin},{}",
        two(essn),
        two(clim),
        two(edge)
    ))
}

/// One month's inputs to the offline fit and verdict: the month's
/// smoothed sunspot number, each day's median fitted index, and every
/// foF2 sample's own index line for rescoring.
struct OfflineMonth {
    name: String,
    month_number: u32,
    r12: f64,
    /// (calendar day, that day's median fitted index).
    day_indexes: Vec<(u8, f64)>,
    /// (day, observed, climatology, essn, day index) per foF2 sample.
    fof2: Vec<(u8, f64, f64, f64, f64)>,
}

/// Below this distance between a sample's two line points (the day
/// index and R12), the line is undefined and the sample is scored as
/// climatology for every model — those days the prediction barely
/// moves with the index anyway.
const OFFLINE_LINE_EPS: f64 = 3.0;

fn offline_month(month: &str, samples: &[Sample]) -> Option<OfflineMonth> {
    let r12 = hfcast::wspr::smoothed_ssn(month)?;
    let (_, month_number) = year_month(month)?;
    let limit = days_in_month(month) as u8;
    let rows: Vec<&Sample> = samples
        .iter()
        .filter(|s| s.day <= limit && s.characteristic == "foF2")
        .collect();
    let day_indexes: Vec<(u8, f64)> = (1..=limit)
        .filter_map(|day| {
            let mut indexes: Vec<f64> = rows
                .iter()
                .filter(|s| s.day == day)
                .filter_map(|s| s.essn_index)
                .collect();
            (!indexes.is_empty()).then(|| (day, hfcast::stats::median_in_place(&mut indexes)))
        })
        .collect();
    let fof2 = rows
        .iter()
        .filter_map(|s| {
            let (essn, index) = s.essn.zip(s.essn_index)?;
            Some((s.day, s.observed, s.climatology, essn, index))
        })
        .collect();
    Some(OfflineMonth {
        name: month.to_string(),
        month_number,
        r12,
        day_indexes,
        fof2,
    })
}

/// The `--fit-offline` mode: the day-of-year correction curve a
/// never-online device adds to its shipped smoothed sunspot number —
/// a distinct value for every calendar day, no monthly plateaus. The
/// fit: each fit day's (median index minus the month's R12) regressed
/// on the day-of-year's two harmonics, equal weight per day; months
/// at or past `wspr::SSN_PREDICTED_FROM` are excluded because their
/// R12 is itself a prediction. The verdict rescores every foF2 sample
/// on its own index line (the essn fit's construction: foF2 is linear
/// in the index) at R12 plus the curve.
fn run_fit_offline(args: &Args) -> ExitCode {
    let mut months: Vec<OfflineMonth> = Vec::new();
    let mut skipped = Vec::new();
    if !over_months(
        args,
        &mut |month, samples| match offline_month(month, samples) {
            Some(m) => months.push(m),
            None => skipped.push(month.to_string()),
        },
    ) {
        return ExitCode::FAILURE;
    }
    for month in &skipped {
        eprintln!("{month}: no smoothed SSN entry, not scored");
    }
    fit_offline_report(&months);
    ExitCode::SUCCESS
}

/// The offline curve's feature row for one calendar day: the shape
/// `truecast::api::offline_anomaly` evaluates.
fn offline_features(month_number: u32, day: u8) -> [f64; 5] {
    const OFFSET: [u32; 12] = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
    let doy = (OFFSET[(month_number as usize - 1).min(11)] + u32::from(day)).min(365);
    let a = std::f64::consts::TAU * f64::from(doy) / 365.0;
    [1.0, a.cos(), a.sin(), (2.0 * a).cos(), (2.0 * a).sin()]
}

fn fit_offline_report(months: &[OfflineMonth]) {
    let points: Vec<([f64; 5], f64, f64)> = months
        .iter()
        .filter(|m| !HELD_OUT.contains(&m.name.as_str()))
        .filter(|m| m.name.as_str() < hfcast::wspr::SSN_PREDICTED_FROM)
        .flat_map(|m| {
            m.day_indexes
                .iter()
                .map(|(day, index)| (offline_features(m.month_number, *day), index - m.r12, 1.0))
        })
        .collect();
    if points.is_empty() {
        println!("no fit days with an observed R12; nothing to fit");
        return;
    }
    let model = least_squares(&points);
    let coeffs: Vec<String> = model.iter().map(|c| format!("{c:.2}")).collect();
    println!(
        "pub const OFFLINE_ANOMALY_MODEL: [f64; 5] = [{}];",
        coeffs.join(", ")
    );
    println!("({} day medians fitted)", points.len());
    let anomaly_at = |month_number: u32, day: u8| -> f64 {
        let x = offline_features(month_number, day);
        model.iter().zip(x).map(|(c, f)| c * f).sum()
    };
    println!();
    println!("| month | n | clim bias / MAE | offline bias / MAE | essn MAE |");
    println!("| --- | ---: | --- | --- | ---: |");
    for m in months {
        let held = if HELD_OUT.contains(&m.name.as_str()) {
            " (held out)"
        } else if m.name.as_str() >= hfcast::wspr::SSN_PREDICTED_FROM {
            " (predicted R12, not fitted)"
        } else {
            ""
        };
        let n = m.fof2.len() as f64;
        let (mut cb, mut ca, mut ob, mut oa, mut ea) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (day, obs, clim, essn, index) in &m.fof2 {
            let offline = if (index - m.r12).abs() < OFFLINE_LINE_EPS {
                *clim
            } else {
                clim + (essn - clim) * anomaly_at(m.month_number, *day) / (index - m.r12)
            };
            cb += clim - obs;
            ca += (clim - obs).abs();
            ob += offline - obs;
            oa += (offline - obs).abs();
            ea += (essn - obs).abs();
        }
        println!(
            "| {}{held} | {} | {:+.2} / {:.2} | {:+.2} / {:.2} | {:.2} |",
            m.name,
            m.fof2.len(),
            cb / n,
            ca / n,
            ob / n,
            oa / n,
            ea / n
        );
    }
}

/// Days since the civil epoch (1970-01-01) for a Gregorian date, so
/// sync staleness can be counted across month and year boundaries
/// without a calendar dependency.
fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let mp = i64::from((month + 9) % 12);
    let doy = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The staleness buckets the sync decay is fitted over: short lags
/// resolved finely, the long tail pooled.
const SYNC_BUCKETS: [(u32, u32); 9] = [
    (1, 1),
    (2, 3),
    (4, 7),
    (8, 14),
    (15, 30),
    (31, 60),
    (61, 120),
    (121, 240),
    (241, 400),
];

/// The verdict's staleness ladder: how old the baked record is.
const SYNC_LADDER: [u32; 6] = [0, 7, 30, 90, 180, 365];

/// The `--fit-sync` mode: how fast a synced index loses its value.
/// Each archive day's curve-relative anomaly (median index minus the
/// month's R12 minus the offline curve) is paired with the day N back;
/// one weight per staleness bucket by least squares, then the decay
/// `a exp(-N/tau) + c` fitted through the buckets (square-root sample
/// weighting, so the short lags a fresh build lives at are not
/// swamped by the huge far buckets). The verdict rescores the
/// held-out months' foF2 samples with the record aged along a
/// staleness ladder.
fn run_fit_sync(args: &Args) -> ExitCode {
    let mut months: Vec<OfflineMonth> = Vec::new();
    if !over_months(args, &mut |month, samples| {
        if let Some(m) = offline_month(month, samples) {
            months.push(m);
        }
    }) {
        return ExitCode::FAILURE;
    }
    fit_sync_report(&months);
    ExitCode::SUCCESS
}

/// The curve-relative anomaly per epoch day, and whether that day may
/// enter the fit (held-out and predicted-R12 months are sources for
/// the verdict but never fit pairs).
fn sync_series(months: &[OfflineMonth]) -> BTreeMap<i64, (f64, bool)> {
    months
        .iter()
        .filter_map(|m| Some((m, year_month(&m.name)?.0)))
        .flat_map(|(m, year)| {
            let fittable = !HELD_OUT.contains(&m.name.as_str())
                && m.name.as_str() < hfcast::wspr::SSN_PREDICTED_FROM;
            m.day_indexes.iter().map(move |(day, index)| {
                let epoch = days_from_civil(i64::from(year), m.month_number, u32::from(*day));
                let relative =
                    index - m.r12 - truecast::offline_anomaly(m.month_number, u32::from(*day));
                (epoch, (relative, fittable))
            })
        })
        .collect()
}

/// One bucket's least-squares weight: (bucket midpoint, w, pairs).
/// Only fittable days pair; the buckets come from `SYNC_BUCKETS`.
fn sync_bucket_weights(series: &BTreeMap<i64, (f64, bool)>) -> Vec<(f64, f64, f64)> {
    SYNC_BUCKETS
        .iter()
        .map(|(lo, hi)| {
            let (mut sxy, mut sxx, mut n) = (0.0, 0.0, 0usize);
            // Nested lag walk: a flat map over (day, lag) would build
            // the pair list only to fold it away again.
            for (epoch, (value, fittable)) in series {
                if !fittable {
                    continue;
                }
                for lag in *lo..=*hi {
                    if let Some((prev, true)) = series.get(&(epoch - i64::from(lag))) {
                        sxy += prev * value;
                        sxx += prev * prev;
                        n += 1;
                    }
                }
            }
            let w = if sxx > 0.0 { sxy / sxx } else { 0.0 };
            (f64::midpoint(f64::from(*lo), f64::from(*hi)), w, n as f64)
        })
        .collect()
}

/// Grid search for the decay `a exp(-N/tau) + c` through the bucket
/// weights, square-root sample weighting so the short lags a fresh
/// build lives at are not swamped by the huge far buckets.
fn fit_sync_decay(buckets: &[(f64, f64, f64)]) -> [f64; 3] {
    let candidates = (2..=120u32).step_by(2).flat_map(|tau| {
        (16..=40).flat_map(move |a| {
            (0..=10).map(move |c| [f64::from(a) / 40.0, f64::from(tau), f64::from(c) / 40.0])
        })
    });
    candidates
        .filter(|[a, _, c]| a + c <= 1.0)
        .map(|params| {
            let [a, tau, c] = params;
            let err: f64 = buckets
                .iter()
                .map(|(mid, w, n)| n.sqrt() * (a * (-mid / tau).exp() + c - w).powi(2))
                .sum();
            (err, params)
        })
        .min_by(|(e1, _), (e2, _)| e1.total_cmp(e2))
        .map(|(_, params)| params)
        .expect("the grid is not empty")
}

/// One held-out month's verdict row: MAE for climatology, the offline
/// curve, and the sync record aged along the ladder.
fn sync_verdict_row(
    m: &OfflineMonth,
    year: u32,
    series: &BTreeMap<i64, (f64, bool)>,
    weight: &dyn Fn(f64) -> f64,
) -> String {
    let mut sums = vec![(0.0, 0usize); 2 + SYNC_LADDER.len()];
    for (day, obs, clim, essn, index) in &m.fof2 {
        if (index - m.r12).abs() < OFFLINE_LINE_EPS {
            continue;
        }
        let slope = (essn - clim) / (index - m.r12);
        let curve = truecast::offline_anomaly(m.month_number, u32::from(*day));
        let epoch = days_from_civil(i64::from(year), m.month_number, u32::from(*day));
        let mut add = |slot: usize, err: Option<f64>| {
            if let Some(e) = err {
                sums[slot].0 += e;
                sums[slot].1 += 1;
            }
        };
        add(0, Some((clim - obs).abs()));
        add(1, Some((clim + slope * curve - obs).abs()));
        for (i, lag) in SYNC_LADDER.iter().enumerate() {
            let aged = series.get(&(epoch - i64::from(*lag))).map(|(relative, _)| {
                let anomaly = curve + weight(f64::from(*lag)) * relative;
                (clim + slope * anomaly - obs).abs()
            });
            add(2 + i, aged);
        }
    }
    let cells: Vec<String> = sums
        .iter()
        .map(|(total, n)| {
            if *n == 0 {
                "-".to_string()
            } else {
                format!("{:.3}", total / *n as f64)
            }
        })
        .collect();
    format!(
        "| {} (held out) | {} | {} |",
        m.name,
        sums[0].1,
        cells.join(" | ")
    )
}

fn fit_sync_report(months: &[OfflineMonth]) {
    let series = sync_series(months);
    let buckets = sync_bucket_weights(&series);
    if buckets.iter().all(|(_, _, n)| *n == 0.0) {
        println!("no fit pairs; nothing to fit");
        return;
    }
    println!("| staleness | fitted w | pairs |");
    println!("| --- | ---: | ---: |");
    for ((lo, hi), (_, w, n)) in SYNC_BUCKETS.iter().zip(&buckets) {
        println!("| {lo}-{hi} days | {w:.3} | {n:.0} |");
    }
    let [a, tau, c] = fit_sync_decay(&buckets);
    println!();
    println!("pub const SYNC_DECAY: [f64; 3] = [{a}, {tau:.1}, {c}];");
    let weight = move |days: f64| a * (-days / tau).exp() + c;
    println!();
    let header: Vec<String> = SYNC_LADDER.iter().map(|n| format!("sync {n}d")).collect();
    println!("| month | n | clim | offline | {} |", header.join(" | "));
    println!(
        "| --- | ---: | ---: | ---: |{}",
        " ---: |".repeat(SYNC_LADDER.len())
    );
    months
        .iter()
        .filter(|m| HELD_OUT.contains(&m.name.as_str()))
        .filter_map(|m| Some((m, year_month(&m.name)?.0)))
        .for_each(|(m, year)| println!("{}", sync_verdict_row(m, year, &series, &weight)));
}

/// The `--sync-record` mode: the JSON a build bakes into the app so a
/// never-connecting device still starts from a measured day. The last
/// day with samples in the given month, exactly as the live ledger
/// scores it; the anomaly is against the embedded smoothed sunspot
/// table, so the app must ship the same table version.
fn run_sync_record(args: &Args) -> ExitCode {
    let mut printed = false;
    if !over_months(args, &mut |month, samples| {
        let Some(m) = offline_month(month, samples) else {
            eprintln!("{month}: no smoothed SSN entry");
            return;
        };
        let Some((day, index)) = m.day_indexes.last() else {
            eprintln!("{month}: no days with samples");
            return;
        };
        println!(
            "{{\"date\":\"{}-{:02}\",\"index\":{index:.1},\"anomaly\":{:.1}}}",
            m.name,
            day,
            index - m.r12
        );
        printed = true;
    }) || !printed
    {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}

/// The `--fit-edge` mode: gather the months, fit, print the verdict.
fn run_fit_edge(args: &Args) -> ExitCode {
    let mut months: Vec<(String, Vec<EdgeRow>)> = Vec::new();
    if !over_months(args, &mut |month, samples| {
        months.push((month.to_string(), edge_rows(month, samples)));
    }) {
        return ExitCode::FAILURE;
    }
    fit_edge_report(&months);
    ExitCode::SUCCESS
}

/// The edge model's feature row for one month number and day index:
/// `[1, index, cos a, sin a, cos 2a, sin 2a]`, `a = 2π month / 12` —
/// the shape `truecast::api::edge_fmin_ratio` evaluates.
fn edge_features(month_number: u32, index: f64) -> [f64; 6] {
    let a = std::f64::consts::TAU * f64::from(month_number) / 12.0;
    let i = index.clamp(truecast::EDGE_INDEX_SPAN.0, truecast::EDGE_INDEX_SPAN.1);
    [1.0, i, a.cos(), a.sin(), (2.0 * a).cos(), (2.0 * a).sin()]
}

/// Solves NxN normal equations by Gaussian elimination with partial
/// pivoting. Small and dense; no library earns its place.
fn solve<const N: usize>(mut a: [[f64; N]; N], mut b: [f64; N]) -> [f64; N] {
    // Elimination is inherently sequential row mutation; the
    // functional form would rebuild the matrix per step.
    for col in 0..N {
        let pivot = (col..N)
            .max_by(|p, q| a[*p][col].abs().total_cmp(&a[*q][col].abs()))
            .expect("a pivot row exists");
        a.swap(col, pivot);
        b.swap(col, pivot);
        let pivot_row = a[col];
        for row in 0..N {
            if row != col && a[row][col] != 0.0 {
                let f = a[row][col] / pivot_row[col];
                a[row]
                    .iter_mut()
                    .zip(pivot_row)
                    .for_each(|(value, p)| *value -= f * p);
                b[row] -= f * b[col];
            }
        }
    }
    std::array::from_fn(|i| b[i] / a[i][i])
}

/// Fits weighted least squares over N features from (features, value,
/// weight) points: builds the normal equations and solves them.
fn least_squares<const N: usize>(points: &[([f64; N], f64, f64)]) -> [f64; N] {
    let mut normal = [[0.0; N]; N];
    let mut rhs = [0.0; N];
    for (x, y, w) in points {
        for p in 0..N {
            for q in 0..N {
                normal[p][q] += w * x[p] * x[q];
            }
            rhs[p] += w * x[p] * y;
        }
    }
    solve(normal, rhs)
}

/// One station-day's accumulating (ratios, indexes) under the edge fit.
type StationDayValues<'a> = BTreeMap<(&'a str, u8), (Vec<f64>, Vec<f64>)>;

/// Fits the absorption-edge level model on the months that are not
/// held out and prints every month's raw and model-corrected error, so
/// the held-out rows are the deployable verdict. The fit: each
/// station-day's median ln(edge over fmin) regressed on the day's
/// index and the calendar season's two harmonics, weighted by the
/// day's sample count — printed for `truecast::api::EDGE_RATIO_MODEL`.
/// Day medians rather than raw pairs so one noisy ionogram cannot
/// steer the level.
fn fit_edge_report(months: &[(String, Vec<EdgeRow>)]) {
    let points: Vec<([f64; 6], f64, f64)> = months
        .iter()
        .filter(|(name, _)| !HELD_OUT.contains(&name.as_str()))
        .filter_map(|(name, rows)| Some((year_month(name)?.1, rows)))
        .flat_map(|(mnum, rows)| {
            let mut by_day: StationDayValues = BTreeMap::new();
            for r in rows {
                let (ratios, indexes) = by_day.entry((r.station.as_str(), r.day)).or_default();
                ratios.push(r.edge / r.observed);
                indexes.push(r.index);
            }
            by_day
                .into_values()
                .map(|(mut ratios, mut indexes)| {
                    let weight = ratios.len() as f64;
                    let ratio = hfcast::stats::median_in_place(&mut ratios);
                    let index = hfcast::stats::median_in_place(&mut indexes);
                    (edge_features(mnum, index), ratio.ln(), weight)
                })
                .collect::<Vec<_>>()
        })
        .collect();
    if points.is_empty() {
        println!("no fmin rows outside the held-out months; nothing to fit");
        return;
    }
    let model = least_squares(&points);
    let coeffs: Vec<String> = model.iter().map(|c| format!("{c:.6}")).collect();
    println!(
        "pub const EDGE_RATIO_MODEL: [f64; 6] = [{}];",
        coeffs.join(", ")
    );
    println!("({} station-day medians fitted)", points.len());
    let ratio_at = |mnum: u32, index: f64| -> f64 {
        let x = edge_features(mnum, index);
        model.iter().zip(x).map(|(c, f)| c * f).sum::<f64>().exp()
    };
    println!();
    println!("| month | n | own ratio | raw bias / MAE | corrected bias / MAE |");
    println!("| --- | ---: | ---: | --- | --- |");
    for (name, rows) in months {
        let Some((_, mnum)) = year_month(name) else {
            continue;
        };
        let held = if HELD_OUT.contains(&name.as_str()) {
            " (held out)"
        } else {
            ""
        };
        let n = rows.len() as f64;
        let mut own: Vec<f64> = rows.iter().map(|r| r.edge / r.observed).collect();
        let own_ratio = hfcast::stats::median_in_place(&mut own);
        let (mut rb, mut ra, mut cb, mut ca) = (0.0, 0.0, 0.0, 0.0);
        for r in rows {
            let raw = r.edge - r.observed;
            let corrected = r.edge / ratio_at(mnum, r.index) - r.observed;
            rb += raw;
            ra += raw.abs();
            cb += corrected;
            ca += corrected.abs();
        }
        println!(
            "| {name}{held} | {} | {own_ratio:.3} | {:+.2} / {:.2} | {:+.2} / {:.2} |",
            rows.len(),
            rb / n,
            ra / n,
            cb / n,
            ca / n
        );
    }
}

/// Prints the fitted table as Rust source for `stormfit::FITTED`, then a
/// summary of the fitted bins for the docs. Grouping: one line per
/// (class, band, season), its four local-time quarters left to right.
fn fit_storm_report(samples: &[(usize, f64)]) {
    let (ratios, counts) = stormfit::fit(samples);
    let kp_names = ["quiet", "active", "storm", "severe"];
    let lat_names = ["low", "mid", "high"];
    let season_names = ["summer", "equinox", "winter"];
    println!("pub const FITTED: [f64; N_BINS] = [");
    // Loops print in table order; the grouping is the output format.
    for (kp, kp_name) in kp_names.iter().enumerate() {
        for (lat, lat_name) in lat_names.iter().enumerate() {
            for (season, season_name) in season_names.iter().enumerate() {
                let base =
                    ((kp * stormfit::N_LAT + lat) * stormfit::N_SEASON + season) * stormfit::N_LT;
                let quarters: Vec<String> = (0..stormfit::N_LT)
                    .map(|lt| format!("{:.4},", ratios[base + lt]))
                    .collect();
                println!(
                    "    {} // {kp_name} {lat_name} {season_name}",
                    quarters.join(" ")
                );
            }
        }
    }
    println!("];");
    println!(
        "\n{} samples. Bins with fewer than {} own samples borrow the \
         season pool; quiet bins stay 1.0 by construction:\n",
        samples.len(),
        stormfit::MIN_BIN
    );
    println!("| class  | band | season  | LT quarter | ratio |    n |");
    println!("| ------ | ---- | ------- | ---------- | ----: | ---: |");
    let lt_names = ["00-06", "06-12", "12-18", "18-24"];
    for b in 0..stormfit::N_BINS {
        let kp = b / (stormfit::N_LT * stormfit::N_SEASON * stormfit::N_LAT);
        if kp == 0 || (ratios[b] == 1.0 && counts[b] == 0) {
            continue;
        }
        let lat = (b / (stormfit::N_LT * stormfit::N_SEASON)) % stormfit::N_LAT;
        let season = (b / stormfit::N_LT) % stormfit::N_SEASON;
        println!(
            "| {:<6} | {:<4} | {:<7} | {:<10} | {:.3} | {:4} |",
            kp_names[kp],
            lat_names[lat],
            season_names[season],
            lt_names[b % stormfit::N_LT],
            ratios[b],
            counts[b]
        );
    }
}

/// The essn prediction times the embedded storm ratio. The ratio is
/// fitted to foF2 and multiplies MUFD identically, since MUFD is the
/// foF2 line times the factor line. Unknown storm state (no Kp file,
/// missing lookback days) leaves the prediction alone — exactly what a
/// deployed device would do. The `--engine truecast` check replays the
/// truecast API against this same function, so the deployable path and
/// the research column cannot drift apart.
fn essn_storm_value(
    s: &Sample,
    month: &str,
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) -> Option<f64> {
    let predicted = s.essn?;
    let bin = stations
        .get(&s.station)
        .zip(year_month(month))
        .and_then(|(meta, (year, mm))| {
            let kp = table?.kp_max_lookback(year, mm, s.day, s.hour, 24)?;
            Some(stormfit::bin(mm, meta.lat, meta.lon, s.hour, kp))
        });
    Some(predicted * stormfit::correction(&stormfit::FITTED, bin))
}

/// The running worst disagreement of one comparison.
#[derive(Default)]
struct WorstDelta {
    max: f64,
    n: usize,
}

impl WorstDelta {
    fn track(&mut self, model: f64, reference: f64) {
        self.max = self.max.max((model - reference).abs());
        self.n += 1;
    }

    fn line(&self, label: &str, tolerance: f64) -> String {
        let verdict = if self.max <= tolerance { "ok" } else { "FAIL" };
        format!(
            "  {label:<24} max |d| {max:.5} over {n} samples, tolerance {tolerance} — {verdict}",
            max = self.max,
            n = self.n
        )
    }

    fn passes(&self, tolerance: f64) -> bool {
        self.max <= tolerance
    }
}

/// Replays the truecast point API over every cached cell of the month and
/// measures the worst disagreement against the research columns. The
/// climatology comparisons must agree to cache rounding (5e-5: the same
/// engine run on both sides). The daily comparison crosses two f32
/// rounding paths — the research column interpolates the answer line
/// between the two map planes, the API blends coefficients and then
/// evaluates — which differ by up to about 0.03 MHz where the harmonic
/// series cancels at night (measured over all eight months). The faults
/// this check exists for are an order larger: a wrong storm bin moves a
/// storm hour by about 0.25 MHz, a shifted hour by about 1 MHz.
fn verify_truecast(
    month: &str,
    samples: &[Sample],
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) -> bool {
    const TOL_CLIMATOLOGY: f64 = 1e-3;
    const TOL_DAILY: f64 = 5e-2;
    let Some(ssn) = hfcast::wspr::smoothed_ssn(month) else {
        eprintln!("no smoothed SSN for {month}");
        return false;
    };
    let by_station: BTreeMap<&str, Vec<&Sample>> =
        samples.iter().fold(BTreeMap::new(), |mut map, s| {
            map.entry(s.station.as_str()).or_default().push(s);
            map
        });

    let mut deltas = TruecastDeltas::default();
    // A loop over stations: each iteration runs the engine and
    // accumulates into the trackers, and an engine error ends the check.
    for (code, rows) in &by_station {
        let outcome = stations
            .get(*code)
            .ok_or(format!("{code} is not in the station list"))
            .and_then(|meta| check_station(month, ssn, meta, rows, table, stations, &mut deltas));
        if let Err(e) = outcome {
            eprintln!("{e}");
            return false;
        }
    }

    println!("\n## {month}: truecast API against the research columns\n");
    println!(
        "{}",
        deltas.clim_fof2.line("climatology foF2", TOL_CLIMATOLOGY)
    );
    println!(
        "{}",
        deltas.clim_foe.line("climatology foE", TOL_CLIMATOLOGY)
    );
    println!(
        "{}",
        deltas
            .clim_hmf2
            .line("climatology hmF2/dudeney", TOL_CLIMATOLOGY)
    );
    println!(
        "{}",
        deltas.daily_fof2.line("daily foF2/essn+storm", TOL_DAILY)
    );
    deltas.clim_fof2.passes(TOL_CLIMATOLOGY)
        && deltas.clim_foe.passes(TOL_CLIMATOLOGY)
        && deltas.clim_hmf2.passes(TOL_CLIMATOLOGY)
        && deltas.daily_fof2.passes(TOL_DAILY)
}

/// The four disagreement trackers of the truecast check.
#[derive(Default)]
struct TruecastDeltas {
    clim_fof2: WorstDelta,
    clim_foe: WorstDelta,
    clim_hmf2: WorstDelta,
    daily_fof2: WorstDelta,
}

/// One station's replay: the climatology day against the climatology
/// and dudeney columns, then each indexed day against the essn+storm
/// column.
fn check_station(
    month: &str,
    ssn: f64,
    meta: &StationMeta,
    rows: &[&Sample],
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
    deltas: &mut TruecastDeltas,
) -> Result<(), String> {
    let root = hfcast::voacap::data::embedded_root();
    let mm = year_month(month).map(|(_, mm)| mm).ok_or("bad month")?;
    let conditioning = Conditioning::Climatology { ssn };
    let clim_day = truecast::day(&root, meta.lat, meta.lon, mm, &conditioning)?;
    for s in rows {
        let answer = &clim_day[usize::from(s.hour)];
        match s.characteristic.as_str() {
            "foF2" => deltas.clim_fof2.track(answer.fof2_mhz, s.climatology),
            "foE" => deltas.clim_foe.track(answer.foe_mhz, s.climatology),
            "hmF2" => {
                if let Some(dudeney) = s.dudeney {
                    deltas.clim_hmf2.track(answer.hmf2_km, dudeney);
                }
            }
            _ => {}
        }
    }
    check_station_days(month, meta, rows, table, stations, deltas)
}

/// The daily half of one station's replay, one engine run per day that
/// has a fitted index.
fn check_station_days(
    month: &str,
    meta: &StationMeta,
    rows: &[&Sample],
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
    deltas: &mut TruecastDeltas,
) -> Result<(), String> {
    let root = hfcast::voacap::data::embedded_root();
    let (year, mm) = year_month(month).ok_or("bad month")?;
    let indexed_days: BTreeMap<u8, f64> = rows
        .iter()
        .filter_map(|s| s.essn_index.map(|index| (s.day, index)))
        .collect();
    // A loop over days: each iteration is an engine run.
    for (day, index) in indexed_days {
        let kp_max24 = std::array::from_fn(|h| {
            table.and_then(|t| t.kp_max_lookback(year, mm, day, h as u8, 24))
        });
        let conditioning = Conditioning::daily_by_hour(index, kp_max24);
        let daily = truecast::day(&root, meta.lat, meta.lon, mm, &conditioning)?;
        for s in rows
            .iter()
            .filter(|s| s.day == day && s.characteristic == "foF2")
        {
            if let Some(reference) = essn_storm_value(s, month, table, stations) {
                deltas
                    .daily_fof2
                    .track(daily[usize::from(s.hour)].fof2_mhz, reference);
            }
        }
    }
    Ok(())
}

fn report(
    month: &str,
    samples: &[Sample],
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) {
    let station_codes: BTreeSet<&str> = samples.iter().map(|s| s.station.as_str()).collect();
    println!("\n## {month}\n");
    println!(
        "{} samples from {} stations: {}",
        samples.len(),
        station_codes.len(),
        station_codes.into_iter().collect::<Vec<_>>().join(" ")
    );

    // Loops, not maps: these iterate to print, and the report reads in
    // this order. The fmin section is the lower edge: observed fmin
    // against the engine's absorption edge over the probe path. Both
    // carry constant system factors (the probe's link budget, the
    // sounder's threshold), so its bias column is an offset to read
    // past, and the shape and day columns are the score. The storm
    // rows stay foF2-family: the embedded ratio is a foF2 correction
    // and has no claim on absorption.
    for characteristic in ["foF2", "hmF2", "MUFD", "foE", "fmin"] {
        println!("\n### {characteristic} (model - observed)\n");
        println!("| model                    |    bias |    MAE |    RMS |     n |");
        println!("| ------------------------ | ------: | -----: | -----: | ----: |");
        characteristic_rows(month, samples, characteristic, table, stations);
        storm_split_rows(month, samples, characteristic, table, stations);
        day_to_day_line(month, samples, characteristic, table, stations);
    }

    report_nvis(month, samples, table, stations);
}

/// The whole-month error rows of one characteristic's table.
fn characteristic_rows(
    month: &str,
    samples: &[Sample],
    characteristic: &str,
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) {
    let climatology: &dyn Fn(&Sample) -> Option<f64> = &|s| Some(s.climatology);
    let irtam: &dyn Fn(&Sample) -> Option<f64> = &|s| s.irtam;
    let essn: &dyn Fn(&Sample) -> Option<f64> = &|s| s.essn;
    let all: &dyn Fn(&Sample) -> bool = &|_| true;
    error_row(
        "climatology",
        &pairs(samples, characteristic, climatology, all),
    );
    error_row("irtam", &pairs(samples, characteristic, irtam, all));
    if matches!(characteristic, "foF2" | "MUFD" | "fmin") {
        error_row("essn (holdout)", &pairs(samples, characteristic, essn, all));
    }
    if matches!(characteristic, "foF2" | "MUFD") {
        let essn_storm: &dyn Fn(&Sample) -> Option<f64> =
            &|s| essn_storm_value(s, month, table, stations);
        error_row(
            "essn+storm",
            &pairs(samples, characteristic, essn_storm, all),
        );
    }
    if characteristic == "fmin" {
        error_row(
            "climatology - offsets",
            &offset_adjusted_pairs(samples, characteristic, climatology),
        );
        error_row(
            "essn - offsets",
            &offset_adjusted_pairs(samples, characteristic, essn),
        );
    }
    if characteristic == "hmF2" {
        let dudeney: &dyn Fn(&Sample) -> Option<f64> = &|s| s.dudeney;
        error_row(
            "climatology+dudeney",
            &pairs(samples, characteristic, dudeney, all),
        );
    }
}

/// The quiet/storm split rows, when a Kp table is loaded.
fn storm_split_rows(
    month: &str,
    samples: &[Sample],
    characteristic: &str,
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) {
    if table.is_none() {
        return;
    }
    let climatology: &dyn Fn(&Sample) -> Option<f64> = &|s| Some(s.climatology);
    let irtam: &dyn Fn(&Sample) -> Option<f64> = &|s| s.irtam;
    let essn: &dyn Fn(&Sample) -> Option<f64> = &|s| s.essn;
    for (label, want_storm) in [("climatology, quiet", false), ("climatology, storm", true)] {
        let keep: &dyn Fn(&Sample) -> bool = &|s| storminess(table, month, s) == Some(want_storm);
        error_row(label, &pairs(samples, characteristic, climatology, keep));
        let irtam_label = label.replace("climatology", "irtam");
        error_row(&irtam_label, &pairs(samples, characteristic, irtam, keep));
        if matches!(characteristic, "foF2" | "MUFD" | "fmin") {
            let essn_label = label.replace("climatology", "essn");
            error_row(&essn_label, &pairs(samples, characteristic, essn, keep));
        }
        if matches!(characteristic, "foF2" | "MUFD") {
            let essn_storm: &dyn Fn(&Sample) -> Option<f64> =
                &|s| essn_storm_value(s, month, table, stations);
            let storm_label = label.replace("climatology", "essn+storm");
            error_row(
                &storm_label,
                &pairs(samples, characteristic, essn_storm, keep),
            );
        }
    }
}

/// The day-to-day correlation line under one characteristic's table,
/// with the climatology zero guard.
fn day_to_day_line(
    month: &str,
    samples: &[Sample],
    characteristic: &str,
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) {
    let climatology: &dyn Fn(&Sample) -> Option<f64> = &|s| Some(s.climatology);
    let irtam: &dyn Fn(&Sample) -> Option<f64> = &|s| s.irtam;
    let essn: &dyn Fn(&Sample) -> Option<f64> = &|s| s.essn;
    let of_char: Vec<&Sample> = samples
        .iter()
        .filter(|s| s.characteristic == characteristic)
        .collect();
    let (clim_corr, pairs_n) = day_to_day(&of_char, climatology);
    let (irtam_corr, _) = day_to_day(&of_char, irtam);
    let (essn_corr, _) = day_to_day(&of_char, essn);
    let storm_note = if matches!(characteristic, "foF2" | "MUFD") {
        let essn_storm: &dyn Fn(&Sample) -> Option<f64> =
            &|s| essn_storm_value(s, month, table, stations);
        let (storm_corr, _) = day_to_day(&of_char, essn_storm);
        format!(", essn+storm {storm_corr:+.3}")
    } else {
        String::new()
    };
    println!(
        "\nday-to-day: climatology {clim_corr:+.3} (guard: must be +0.000), \
         irtam {irtam_corr:+.3}, essn {essn_corr:+.3}{storm_note}, {pairs_n} day pairs"
    );
}

/// The storm ratio for one NVIS cell, or 1.0 where the state is
/// unknown — the same rule as `essn_storm_value`, over the cell's own
/// place and hour.
fn cell_storm_ratio(
    cell: &sonde::NvisCell,
    month: &str,
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) -> f64 {
    let bin = stations
        .get(&cell.station)
        .zip(year_month(month))
        .and_then(|(meta, (year, mm))| {
            let kp = table?.kp_max_lookback(year, mm, cell.day, cell.hour, 24)?;
            Some(stormfit::bin(mm, meta.lat, meta.lon, cell.hour, kp))
        });
    stormfit::correction(&stormfit::FITTED, bin)
}

/// NVIS: MUF(d) error and band calls at each scored ground range. The
/// measured MUF uses measured foF2 and measured hmF2; each model uses its
/// own foF2 with its own (here: climatology) hmF2.
fn report_nvis(
    month: &str,
    samples: &[Sample],
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) {
    let cells = nvis_cells(samples);
    if cells.is_empty() {
        println!("\n### NVIS: no hours with both foF2 and hmF2 measured");
        return;
    }
    println!(
        "\n### NVIS MUF(d) from foF2 x secant (n = {})\n",
        cells.len()
    );
    println!("| range | model        |    bias |    MAE |    RMS | band calls right |");
    println!("| ----: | ------------ | ------: | -----: | -----: | ---------------: |");
    // A loop for ordered printing, as above.
    for range in NVIS_RANGES_KM {
        let observed: Vec<f64> = cells
            .iter()
            .map(|c| c.observed_fof2 * secant_factor(range, c.observed_hmf2))
            .collect();
        for (label, predicted) in nvis_models(&cells, range, month, table, stations) {
            nvis_row(range, label, &predicted, &observed);
        }
    }
}

/// The model columns of the NVIS table at one ground range.
fn nvis_models(
    cells: &[sonde::NvisCell],
    range: f64,
    month: &str,
    table: Option<&GeomagTable>,
    stations: &BTreeMap<String, StationMeta>,
) -> [(&'static str, Vec<Option<f64>>); 6] {
    [
        (
            "climatology",
            cells
                .iter()
                .map(|c| Some(c.climatology_fof2 * secant_factor(range, c.climatology_hmf2)))
                .collect(),
        ),
        (
            // The height fix alone: same foF2, corrected formula.
            "clim+dudeney",
            cells
                .iter()
                .map(|c| {
                    c.dudeney_hmf2
                        .map(|h| c.climatology_fof2 * secant_factor(range, h))
                })
                .collect(),
        ),
        (
            // The daily frequency alone, over the shipped height.
            "irtam-foF2",
            cells
                .iter()
                .map(|c| {
                    c.irtam_fof2
                        .map(|f| f * secant_factor(range, c.climatology_hmf2))
                })
                .collect(),
        ),
        (
            // The deployable offline pair: holdout index over the
            // corrected height form.
            "essn+dudeney",
            cells
                .iter()
                .map(|c| {
                    c.essn_fof2.map(|f| {
                        f * secant_factor(range, c.dudeney_hmf2.unwrap_or(c.climatology_hmf2))
                    })
                })
                .collect(),
        ),
        (
            // The full deployable pipeline: what the truecast point
            // API answers under daily conditioning with the storm
            // state known.
            "essn+st+dud",
            cells
                .iter()
                .map(|c| {
                    c.essn_fof2.map(|f| {
                        f * cell_storm_ratio(c, month, table, stations)
                            * secant_factor(range, c.dudeney_hmf2.unwrap_or(c.climatology_hmf2))
                    })
                })
                .collect(),
        ),
        (
            // Both assimilated maps together.
            "irtam-both",
            cells
                .iter()
                .map(|c| {
                    c.irtam_fof2
                        .zip(c.irtam_hmf2)
                        .map(|(f, h)| f * secant_factor(range, h))
                })
                .collect(),
        ),
    ]
}

/// One printed NVIS row: MUF errors and the band-call rate.
fn nvis_row(range: f64, label: &str, predicted: &[Option<f64>], observed: &[f64]) {
    let muf_pairs: Vec<(f64, f64)> = predicted
        .iter()
        .zip(observed)
        .filter_map(|(p, o)| Some(((*p)?, *o)))
        .collect();
    let mut calls = BandCalls::default();
    for ((p, o), band) in muf_pairs
        .iter()
        .flat_map(|pair| NVIS_BANDS_MHZ.iter().map(move |b| (pair, b)))
        .map(|((p, o), b)| ((*p, *o), *b))
    {
        calls.count(o, p, band);
    }
    match errors(&muf_pairs) {
        Some((bias, mae, rms)) => println!(
            "| {range:4.0}k | {label:<12} | {bias:+7.3} | {mae:6.3} | {rms:6.3} | {:15.1}% |",
            calls.accuracy() * 100.0
        ),
        None => {
            println!(
                "| {range:4.0}k | {label:<12} |       - |      - |      - |                - |"
            );
        }
    }
}

struct Args {
    kp: Option<PathBuf>,
    stations: PathBuf,
    months: Vec<PathBuf>,
    check_only: bool,
    fit_storm: bool,
    fit_edge: bool,
    fit_offline: bool,
    fit_sync: bool,
    sync_record: bool,
    ledger: bool,
    daily: bool,
    engine: String,
}

fn parse_args() -> Args {
    let mut args = std::env::args().skip(1).peekable();
    let mut parsed = Args {
        kp: None,
        stations: PathBuf::from("tools/giro-stations.tsv"),
        months: Vec::new(),
        check_only: false,
        fit_storm: false,
        fit_edge: false,
        fit_offline: false,
        fit_sync: false,
        sync_record: false,
        ledger: false,
        daily: false,
        engine: "parity".to_string(),
    };
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--kp" => parsed.kp = args.next().map(PathBuf::from),
            "--stations" => {
                if let Some(path) = args.next() {
                    parsed.stations = PathBuf::from(path);
                }
            }
            "--engine" => {
                if let Some(name) = args.next() {
                    parsed.engine = name;
                }
            }
            name => match mode_flag(&mut parsed, name) {
                Some(flag) => *flag = true,
                None => parsed.months.push(PathBuf::from(arg)),
            },
        }
    }
    parsed
}

/// The mode flag a bare argument names, if it is one.
fn mode_flag<'a>(parsed: &'a mut Args, name: &str) -> Option<&'a mut bool> {
    match name {
        "--check" => Some(&mut parsed.check_only),
        "--fit-storm" => Some(&mut parsed.fit_storm),
        "--fit-edge" => Some(&mut parsed.fit_edge),
        "--fit-offline" => Some(&mut parsed.fit_offline),
        "--fit-sync" => Some(&mut parsed.fit_sync),
        "--sync-record" => Some(&mut parsed.sync_record),
        "--ledger" => Some(&mut parsed.ledger),
        "--daily" => Some(&mut parsed.daily),
        _ => None,
    }
}

/// Gathers every month and hands each to `consume`, stopping on the
/// first bundle that cannot be read. True when every bundle gathered.
fn over_months(args: &Args, consume: &mut dyn FnMut(&str, &[Sample])) -> bool {
    // A loop for the early return with its error line.
    for month_dir in &args.months {
        match sonde::gather(month_dir, &args.stations, Path::new("data/cache")) {
            Ok((month, samples)) => consume(&month, &samples),
            Err(e) => {
                eprintln!("{}: {e}", month_dir.display());
                return false;
            }
        }
    }
    true
}

fn main() -> ExitCode {
    let args = parse_args();
    if args.months.is_empty() || !matches!(args.engine.as_str(), "parity" | "truecast") {
        eprintln!(
            "usage: sonde [--check] [--fit-storm] [--fit-edge] [--fit-offline] [--fit-sync] \
             [--sync-record] [--ledger] [--daily] \
             [--engine parity|truecast] [--kp data/kp_daily.txt] \
             [--stations tools/giro-stations.tsv] data/YYYY-MM ..."
        );
        return ExitCode::FAILURE;
    }
    if args.check_only {
        for month in &args.months {
            check(month);
        }
        return ExitCode::SUCCESS;
    }

    let table = args.kp.as_deref().map(|path| match geomag::load(path) {
        Ok(table) => table,
        Err(e) => {
            eprintln!("no Kp table from {}: {e}", path.display());
            GeomagTable::default()
        }
    });
    let station_meta: BTreeMap<String, StationMeta> = match giro::load_stations(&args.stations) {
        Ok(list) => list.into_iter().map(|m| (m.ursi.clone(), m)).collect(),
        Err(e) => {
            eprintln!("no stations from {}: {e}", args.stations.display());
            return ExitCode::FAILURE;
        }
    };

    if let Some(code) = fit_mode(&args, table.as_ref(), &station_meta) {
        return code;
    }

    if args.engine == "truecast" {
        let mut all_pass = true;
        if !over_months(&args, &mut |month, samples| {
            all_pass &= verify_truecast(month, samples, table.as_ref(), &station_meta);
        }) || !all_pass
        {
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

    if over_months(&args, &mut |month, samples| {
        report(month, samples, table.as_ref(), &station_meta);
    }) {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}

/// The fit and ledger modes, in flag order. None means the ordinary
/// report (or the truecast replay) runs instead.
fn fit_mode(
    args: &Args,
    table: Option<&GeomagTable>,
    station_meta: &BTreeMap<String, StationMeta>,
) -> Option<ExitCode> {
    if args.fit_storm {
        return Some(run_fit_storm(args, table, station_meta));
    }
    if args.fit_edge {
        return Some(run_fit_edge(args));
    }
    if args.fit_offline {
        return Some(run_fit_offline(args));
    }
    if args.fit_sync {
        return Some(run_fit_sync(args));
    }
    if args.sync_record {
        return Some(run_sync_record(args));
    }
    if args.ledger {
        return Some(run_ledger(args));
    }
    if args.daily {
        return Some(run_daily(args, table));
    }
    None
}
