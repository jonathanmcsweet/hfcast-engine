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
//! `--engine nowcast` replays the nowcast point API over the cached
//! cells and fails if it disagrees with the research columns.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hfcast::geomag::{self, GeomagTable};
use hfcast::giro::{self, StationMeta};
use hfcast::nowcast::api::{self as nowcast, Conditioning};
use hfcast::sonde::{
    self, day_to_day, errors, nvis_cells, secant_factor, BandCalls, Sample, NVIS_BANDS_MHZ,
    NVIS_RANGES_KM, STORM_KP,
};
use hfcast::stormfit;

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
    samples
        .iter()
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
/// deployed device would do. The `--engine nowcast` check replays the
/// nowcast API against this same function, so the deployable path and
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

/// Replays the nowcast point API over every cached cell of the month and
/// measures the worst disagreement against the research columns. The
/// climatology comparisons must agree to cache rounding (5e-5: the same
/// engine run on both sides). The daily comparison crosses two f32
/// rounding paths — the research column interpolates the answer line
/// between the two map planes, the API blends coefficients and then
/// evaluates — which differ by up to about 0.03 MHz where the harmonic
/// series cancels at night (measured over all eight months). The faults
/// this check exists for are an order larger: a wrong storm bin moves a
/// storm hour by about 0.25 MHz, a shifted hour by about 1 MHz.
fn verify_nowcast(
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

    let mut deltas = NowcastDeltas::default();
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

    println!("\n## {month}: nowcast API against the research columns\n");
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

/// The four disagreement trackers of the nowcast check.
#[derive(Default)]
struct NowcastDeltas {
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
    deltas: &mut NowcastDeltas,
) -> Result<(), String> {
    let root = hfcast::voacap::data::embedded_root();
    let mm = year_month(month).map(|(_, mm)| mm).ok_or("bad month")?;
    let conditioning = Conditioning::Climatology { ssn };
    let clim_day = nowcast::day(&root, meta.lat, meta.lon, mm, &conditioning)?;
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
    deltas: &mut NowcastDeltas,
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
        let daily = nowcast::day(&root, meta.lat, meta.lon, mm, &conditioning)?;
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
            // The full deployable pipeline: what the nowcast point
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
            "--check" => parsed.check_only = true,
            "--fit-storm" => parsed.fit_storm = true,
            "--engine" => {
                if let Some(name) = args.next() {
                    parsed.engine = name;
                }
            }
            _ => parsed.months.push(PathBuf::from(arg)),
        }
    }
    parsed
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
    if args.months.is_empty() || !matches!(args.engine.as_str(), "parity" | "nowcast") {
        eprintln!(
            "usage: sonde [--check] [--fit-storm] [--engine parity|nowcast] \
             [--kp data/kp_daily.txt] [--stations tools/giro-stations.tsv] data/YYYY-MM ..."
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

    if args.fit_storm {
        let Some(table) = table.as_ref().filter(|t| !t.is_empty()) else {
            eprintln!("--fit-storm needs --kp with a readable file");
            return ExitCode::FAILURE;
        };
        let mut fit_samples = Vec::new();
        if !over_months(&args, &mut |month, samples| {
            fit_samples.extend(storm_samples(month, samples, table, &station_meta));
        }) {
            return ExitCode::FAILURE;
        }
        fit_storm_report(&fit_samples);
        return ExitCode::SUCCESS;
    }

    if args.engine == "nowcast" {
        let mut all_pass = true;
        if !over_months(&args, &mut |month, samples| {
            all_pass &= verify_nowcast(month, samples, table.as_ref(), &station_meta);
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
