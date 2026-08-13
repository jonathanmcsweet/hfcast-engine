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

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hfcast::geomag::{self, GeomagTable};
use hfcast::sonde::{
    self, day_to_day, errors, nvis_cells, secant_factor, BandCalls, Sample, NVIS_BANDS_MHZ,
    NVIS_RANGES_KM, STORM_KP,
};

fn check(dir: &Path) {
    let name = dir.file_name().and_then(|n| n.to_str()).unwrap_or("?");
    let wspr = dir.join("month.txt").is_file();
    let irtam = (1..=31)
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
        "{name}: wspr {}, irtam foF2 {irtam}/31 days, giro {giro} stations",
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

/// Whether the sample's day-hour sits at or above the storm threshold,
/// judged over the trailing 24 hours. None when the Kp file lacks the day.
fn storminess(table: Option<&GeomagTable>, month: &str, s: &Sample) -> Option<bool> {
    let (year, mm) = year_month(month)?;
    let kp = table?.kp_max_lookback(year, mm, s.day, s.hour, 24)?;
    Some(kp >= STORM_KP)
}

fn report(month: &str, samples: &[Sample], table: Option<&GeomagTable>) {
    let stations: BTreeSet<&str> = samples.iter().map(|s| s.station.as_str()).collect();
    println!("\n## {month}\n");
    println!(
        "{} samples from {} stations: {}",
        samples.len(),
        stations.len(),
        stations.into_iter().collect::<Vec<_>>().join(" ")
    );

    let climatology: &dyn Fn(&Sample) -> Option<f64> = &|s| Some(s.climatology);
    let irtam: &dyn Fn(&Sample) -> Option<f64> = &|s| s.irtam;
    let all: &dyn Fn(&Sample) -> bool = &|_| true;

    // Loops, not maps: these iterate to print, and the report reads in
    // this order.
    for characteristic in ["foF2", "hmF2", "MUFD", "foE"] {
        println!("\n### {characteristic} (model - observed)\n");
        println!("| model                    |    bias |    MAE |    RMS |     n |");
        println!("| ------------------------ | ------: | -----: | -----: | ----: |");
        error_row(
            "climatology",
            &pairs(samples, characteristic, climatology, all),
        );
        error_row("irtam", &pairs(samples, characteristic, irtam, all));
        let essn: &dyn Fn(&Sample) -> Option<f64> = &|s| s.essn;
        if matches!(characteristic, "foF2" | "MUFD") {
            error_row("essn (holdout)", &pairs(samples, characteristic, essn, all));
        }
        if characteristic == "hmF2" {
            let dudeney: &dyn Fn(&Sample) -> Option<f64> = &|s| s.dudeney;
            error_row(
                "climatology+dudeney",
                &pairs(samples, characteristic, dudeney, all),
            );
        }
        if table.is_some() {
            for (label, want_storm) in [("climatology, quiet", false), ("climatology, storm", true)]
            {
                let keep: &dyn Fn(&Sample) -> bool =
                    &|s| storminess(table, month, s) == Some(want_storm);
                error_row(label, &pairs(samples, characteristic, climatology, keep));
                let irtam_label = label.replace("climatology", "irtam");
                error_row(&irtam_label, &pairs(samples, characteristic, irtam, keep));
                if matches!(characteristic, "foF2" | "MUFD") {
                    let essn_label = label.replace("climatology", "essn");
                    error_row(&essn_label, &pairs(samples, characteristic, essn, keep));
                }
            }
        }

        let of_char: Vec<&Sample> = samples
            .iter()
            .filter(|s| s.characteristic == characteristic)
            .collect();
        let (clim_corr, pairs_n) = day_to_day(&of_char, climatology);
        let (irtam_corr, _) = day_to_day(&of_char, irtam);
        let (essn_corr, _) = day_to_day(&of_char, essn);
        println!(
            "\nday-to-day: climatology {clim_corr:+.3} (guard: must be +0.000), \
             irtam {irtam_corr:+.3}, essn {essn_corr:+.3}, {pairs_n} day pairs"
        );
    }

    report_nvis(samples);
}

/// NVIS: MUF(d) error and band calls at each scored ground range. The
/// measured MUF uses measured foF2 and measured hmF2; each model uses its
/// own foF2 with its own (here: climatology) hmF2.
fn report_nvis(samples: &[Sample]) {
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
        let models: [(&str, Vec<Option<f64>>); 5] = [
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
        ];
        for (label, predicted) in models {
            let muf_pairs: Vec<(f64, f64)> = predicted
                .iter()
                .zip(&observed)
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
                None => println!("| {range:4.0}k | {label:<12} |       - |      - |      - |                - |"),
            }
        }
    }
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1).peekable();
    let mut kp: Option<PathBuf> = None;
    let mut stations = PathBuf::from("tools/giro-stations.tsv");
    let mut months: Vec<PathBuf> = Vec::new();
    let mut check_only = false;
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--kp" => kp = args.next().map(PathBuf::from),
            "--stations" => {
                if let Some(path) = args.next() {
                    stations = PathBuf::from(path);
                }
            }
            "--check" => check_only = true,
            _ => months.push(PathBuf::from(arg)),
        }
    }
    if months.is_empty() {
        eprintln!(
            "usage: sonde [--check] [--kp data/kp_daily.txt] \
             [--stations tools/giro-stations.tsv] data/YYYY-MM ..."
        );
        return ExitCode::FAILURE;
    }
    if check_only {
        for month in &months {
            check(month);
        }
        return ExitCode::SUCCESS;
    }

    let table = kp.as_deref().map(|path| match geomag::load(path) {
        Ok(table) => table,
        Err(e) => {
            eprintln!("no Kp table from {}: {e}", path.display());
            GeomagTable::default()
        }
    });

    for month_dir in &months {
        match sonde::gather(month_dir, &stations, Path::new("data/cache")) {
            Ok((month, samples)) => report(&month, &samples, table.as_ref()),
            Err(e) => {
                eprintln!("{}: {e}", month_dir.display());
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
