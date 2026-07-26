//! Fits the amplitude correction on one month and proves it on others.
//!
//! The validation found both engines predict daily swings several times larger
//! than measured, while getting the timing roughly right. That is the easiest
//! kind of error to correct: shrink each prediction toward its own daily
//! median by a factor `k`,
//!
//! ```text
//!   corrected(h) = centre + k * (predicted(h) - centre)
//! ```
//!
//! where `centre` is the median of the prediction over the day. Everything in
//! that formula is known in production — no observations are needed at
//! prediction time — so a `k` that works is directly shippable.
//!
//! The honest test is out of sample: fit `k` on one month, apply it unchanged
//! to a different month, and score against that month's measurements. A factor
//! fitted and scored on the same month proves nothing.
//!
//! Input files are the per-hour dumps written by `validate --dump`.
//!
//! Usage: `calibrate --fit <month.csv> --test <month.csv> [--test <month.csv> …]`

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use propcore::stats::median;

/// The four predictors carried in a dump, in column order.
const PREDICTORS: [&str; 4] = [
    "VOACAP",
    "ITU-R P.533",
    "VOACAP, signal only",
    "P.533, signal only",
];

/// Bands with fewer fitted paths than this fall back to the global factor.
const MIN_BAND_PATHS: usize = 8;

/// One path's month: observations and the four predictions, hour-aligned.
struct PathSeries {
    band: i32,
    observed: Vec<f64>,
    predicted: [Vec<f64>; 4],
}

fn load_dump(path: &Path) -> Result<Vec<PathSeries>, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut lines = text.lines();
    let header = lines
        .next()
        .ok_or_else(|| format!("{}: empty", path.display()))?;
    if header != "label,band,km,observed,voacap_snr,itu_snr,voacap_signal,itu_signal" {
        return Err(format!("{}: unexpected header {header:?}", path.display()));
    }

    let mut by_label: BTreeMap<String, PathSeries> = BTreeMap::new();
    for line in lines {
        let fields: Vec<&str> = line.split(',').collect();
        if fields.len() != 8 {
            continue;
        }
        let parse = |i: usize| -> Option<f64> { fields[i].parse().ok() };
        let (Some(band), Some(obs), Some(v), Some(i), Some(vs), Some(is)) =
            (parse(1), parse(3), parse(4), parse(5), parse(6), parse(7))
        else {
            continue;
        };
        let entry = by_label
            .entry(fields[0].to_string())
            .or_insert_with(|| PathSeries {
                band: band as i32,
                observed: Vec::new(),
                predicted: [Vec::new(), Vec::new(), Vec::new(), Vec::new()],
            });
        entry.observed.push(obs);
        entry.predicted[0].push(v);
        entry.predicted[1].push(i);
        entry.predicted[2].push(vs);
        entry.predicted[3].push(is);
    }

    Ok(by_label.into_values().collect())
}

/// Accumulates the pooled least-squares slope through the origin.
///
/// Per path, both sides are centred on their own medians, so the unknown
/// station offset drops out before fitting. Pooling the centred points and
/// fitting one slope weights every hour equally, which matches how the
/// correction will be applied.
#[derive(Default, Clone, Copy)]
struct SlopeFit {
    sum_xy: f64,
    sum_xx: f64,
    paths: usize,
}

impl SlopeFit {
    fn add_path(&mut self, observed: &[f64], predicted: &[f64]) {
        let centre_p = median(&mut predicted.to_vec());
        let centre_o = median(&mut observed.to_vec());
        for (p, o) in predicted.iter().zip(observed) {
            let x = p - centre_p;
            let y = o - centre_o;
            self.sum_xy += x * y;
            self.sum_xx += x * x;
        }
        self.paths += 1;
    }

    fn k(&self) -> Option<f64> {
        if self.sum_xx <= 0.0 {
            None
        } else {
            Some(self.sum_xy / self.sum_xx)
        }
    }
}

struct Fitted {
    global: f64,
    /// Only bands with at least [`MIN_BAND_PATHS`] fitted paths.
    per_band: BTreeMap<i32, f64>,
}

fn fit(paths: &[PathSeries], predictor: usize) -> Option<Fitted> {
    let mut global = SlopeFit::default();
    let mut bands: BTreeMap<i32, SlopeFit> = BTreeMap::new();

    for p in paths {
        global.add_path(&p.observed, &p.predicted[predictor]);
        bands
            .entry(p.band)
            .or_default()
            .add_path(&p.observed, &p.predicted[predictor]);
    }

    let per_band = bands
        .into_iter()
        .filter(|(_, fit)| fit.paths >= MIN_BAND_PATHS)
        .filter_map(|(band, fit)| fit.k().map(|k| (band, k)))
        .collect();

    Some(Fitted {
        global: global.k()?,
        per_band,
    })
}

/// Scores a corrected predictor on one month.
///
/// The per-path offset is still fitted at scoring time, exactly as in the
/// validation: the stations' antennas are unknown there too, and the question
/// is whether the *shape* now has the right amplitude.
fn evaluate(
    paths: &[PathSeries],
    predictor: usize,
    k_for: impl Fn(&PathSeries) -> f64,
) -> (f64, f64) {
    let mut errors: Vec<f64> = Vec::new();

    for p in paths {
        let k = k_for(p);
        let predicted = &p.predicted[predictor];
        let centre = median(&mut predicted.to_vec());
        let corrected: Vec<f64> = predicted
            .iter()
            .map(|v| centre + k * (v - centre))
            .collect();

        let mut residuals: Vec<f64> = corrected
            .iter()
            .zip(&p.observed)
            .map(|(c, o)| c - o)
            .collect();
        let offset = median(&mut residuals.clone());
        errors.extend(residuals.iter_mut().map(|r| (*r - offset).abs()));
    }

    let med = median(&mut errors.clone());
    let rms = propcore::stats::rms(&errors);
    (med, rms)
}

/// The reference line: each path's own measured median, which needs a month of
/// observations of that exact path and so is not available in production.
fn evaluate_flat(paths: &[PathSeries]) -> (f64, f64) {
    let mut errors: Vec<f64> = Vec::new();
    for p in paths {
        let centre = median(&mut p.observed.clone());
        errors.extend(p.observed.iter().map(|o| (o - centre).abs()));
    }
    let med = median(&mut errors.clone());
    let rms = propcore::stats::rms(&errors);
    (med, rms)
}

/// Names a dump by its month directory and file, such as `2025-06/hours-es`.
///
/// Every dump file is called `hours.csv` or `hours-es.csv`; the month lives in
/// the directory name, so the stem alone would label every month identically.
fn month_name(path: &Path) -> String {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());
    match path.parent().and_then(|p| p.file_name()) {
        Some(dir) => format!("{}/{stem}", dir.to_string_lossy()),
        None => stem,
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
    let fit_files = args_of("--fit");
    let test_files = args_of("--test");
    let [fit_file] = fit_files.as_slice() else {
        eprintln!("usage: calibrate --fit <month.csv> --test <month.csv> [--test …]");
        return ExitCode::FAILURE;
    };
    if test_files.is_empty() {
        eprintln!("at least one --test file is required");
        return ExitCode::FAILURE;
    }

    let fit_paths = match load_dump(fit_file) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    let fitted: Vec<Option<Fitted>> = (0..PREDICTORS.len()).map(|i| fit(&fit_paths, i)).collect();

    println!(
        "# Amplitude correction fitted on {}\n",
        month_name(fit_file)
    );
    println!("Correction: `corrected = centre + k * (predicted - centre)`, with");
    println!("`centre` the prediction's own daily median. Factors:\n");
    println!("| predictor | global k | per-band k |");
    println!("| --- | --: | --- |");
    for (i, name) in PREDICTORS.iter().enumerate() {
        match &fitted[i] {
            Some(f) => {
                let bands: Vec<String> = f
                    .per_band
                    .iter()
                    .map(|(b, k)| format!("{b} MHz: {k:.2}"))
                    .collect();
                println!("| {name} | {:.3} | {} |", f.global, bands.join(", "));
            }
            None => println!("| {name} | — | — |"),
        }
    }

    for test_file in &test_files {
        let test_paths = match load_dump(test_file) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        println!("\n## Tested on {}\n", month_name(test_file));
        println!("| predictor | raw | global k | per-band k |");
        println!("| --- | --: | --: | --: |");
        for (i, name) in PREDICTORS.iter().enumerate() {
            let Some(f) = &fitted[i] else {
                println!("| {name} | — | — | — |");
                continue;
            };
            let (raw_med, _) = evaluate(&test_paths, i, |_| 1.0);
            let (glob_med, _) = evaluate(&test_paths, i, |_| f.global);
            let (band_med, _) = evaluate(&test_paths, i, |p| {
                f.per_band.get(&p.band).copied().unwrap_or(f.global)
            });
            println!("| {name} | {raw_med:.2} | {glob_med:.2} | {band_med:.2} |");
        }
        let (flat_med, _) = evaluate_flat(&test_paths);
        println!("| flat baseline (needs the month's own data) | {flat_med:.2} | — | — |");
        println!("\nNumbers are median absolute error in dB after per-path offset removal.");
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic path whose observation is exactly a scaled prediction.
    fn path_with_slope(k: f64, band: i32) -> PathSeries {
        let predicted: Vec<f64> = (0..24).map(|h| (h as f64) - 11.5).collect();
        let observed: Vec<f64> = predicted.iter().map(|p| 7.0 + k * p).collect();
        PathSeries {
            band,
            observed,
            predicted: [
                predicted.clone(),
                predicted.clone(),
                predicted.clone(),
                predicted,
            ],
        }
    }

    #[test]
    fn fit_recovers_a_known_shrink_factor() {
        let paths = vec![path_with_slope(0.25, 7), path_with_slope(0.25, 14)];
        let fitted = fit(&paths, 0).expect("fits");
        assert!((fitted.global - 0.25).abs() < 1e-9);
    }

    #[test]
    fn small_bands_fall_back_to_the_global_factor() {
        // Two paths per band is below MIN_BAND_PATHS, so no per-band entry.
        let paths = vec![path_with_slope(0.25, 7), path_with_slope(0.25, 14)];
        let fitted = fit(&paths, 0).expect("fits");
        assert!(fitted.per_band.is_empty());
    }

    #[test]
    fn the_right_factor_scores_better_than_none() {
        let paths = vec![path_with_slope(0.2, 7)];
        let (raw, _) = evaluate(&paths, 0, |_| 1.0);
        let (corrected, _) = evaluate(&paths, 0, |_| 0.2);
        assert!(corrected < raw, "corrected {corrected} vs raw {raw}");
        assert!(corrected < 1e-9, "a perfect factor leaves no error");
    }

    #[test]
    fn the_station_offset_does_not_affect_the_fit() {
        // Identical shapes at very different absolute levels must fit the
        // same factor, because the offset is unknown in production.
        let mut a = path_with_slope(0.3, 7);
        let mut b = path_with_slope(0.3, 7);
        for o in &mut a.observed {
            *o += 40.0;
        }
        for o in &mut b.observed {
            *o -= 40.0;
        }
        let fitted = fit(&[a, b], 0).expect("fits");
        assert!((fitted.global - 0.3).abs() < 1e-9);
    }

    #[test]
    fn loads_a_dump_grouped_by_path() {
        let dir = std::env::temp_dir().join("propcore-calib-test");
        fs::create_dir_all(&dir).expect("dir");
        let file = dir.join("dump.csv");
        fs::write(
            &file,
            "label,band,km,observed,voacap_snr,itu_snr,voacap_signal,itu_signal\n\
             A>B 40m,7,1000,-10,-5,-6,-100,-101\n\
             A>B 40m,7,1000,-12,-9,-8,-104,-103\n\
             C>D 20m,14,2000,-3,-1,-2,-90,-91\n",
        )
        .expect("write");
        let paths = load_dump(&file).expect("load");
        assert_eq!(paths.len(), 2);
        let long = paths.iter().find(|p| p.band == 7).expect("40m path");
        assert_eq!(long.observed, vec![-10.0, -12.0]);
        assert_eq!(long.predicted[0], vec![-5.0, -9.0]);
        let _ = fs::remove_dir_all(&dir);
    }
}
