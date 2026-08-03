//! Is the day-to-day error predictable at all?
//!
//! Before building anything that forecasts a particular day rather than a
//! typical month, it is worth knowing whether the thing such a model would
//! have to predict carries any structure. This measures that, and nothing
//! else: the lag-1, lag-2 and lag-3 autocorrelation of the daily residual.
//!
//! ## Why no engine run appears below
//!
//! The residual is the observed daily value minus what the pipeline
//! predicts for that path-hour. VOACAP is monthly climatology, so within
//! one month that prediction is **the same number every day** — and the
//! corrections applied on top of it (the swing factor, the spread scales)
//! are functions of hour and path, not of day. Subtracting a constant from
//! a series does not change its autocorrelation, because autocorrelation
//! is computed about the series' own mean.
//!
//! So the lag-k autocorrelation of the residual is exactly the lag-k
//! autocorrelation of the observations about their own monthly centre, and
//! the model cancels out. That makes this a statement about the radio
//! rather than about this engine, and it is why the number below is an
//! upper bound for *any* daily model, learned or physical, not just for
//! one built on this port.
//!
//! Usage: `cargo run --release --bin daily -- [--data DIR] [--min-hours N]`
//!
//! `--data` holds one subdirectory per month, each as `tools/fetch-wspr.sh`
//! writes it, plus `kp_daily.txt` from `tools/fetch-kp.sh`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use hfcast::geomag::{self, GeomagTable};
use hfcast::stats::{median, median_in_place};
use hfcast::wspr::{self, PathKey};

/// A day needs this many hours present before its mean residual is used.
/// A day represented by two hours is mostly telling us which hours it was
/// represented in, not how the ionosphere behaved.
const DEFAULT_MIN_HOURS: usize = 8;

/// Kp at or above this counts the day as geomagnetically disturbed. The
/// same threshold the soak uses, and close to where the storm-spread
/// widening in `docs/storm.md` begins to bite.
const DISTURBED_KP: f64 = 5.0;

/// One path's series of daily mean residuals, in day order with gaps kept
/// as gaps: `day` is the day of the month, so a missing day breaks the
/// chain rather than being silently bridged.
struct Series {
    label: String,
    /// (day of month, mean residual in dB, standard score of that residual)
    points: Vec<(u8, f64, f64)>,
}

/// Sum of products and count, accumulated across paths.
#[derive(Default, Clone, Copy)]
struct Accum {
    sum: f64,
    n: usize,
}

impl Accum {
    fn push(&mut self, product: f64) {
        self.sum += product;
        self.n += 1;
    }
    /// The pooled correlation. Both members of every pair are already
    /// standard scores within their own path, so the mean product *is* the
    /// correlation and no further normalisation applies.
    fn value(self) -> Option<f64> {
        (self.n > 1).then(|| self.sum / self.n as f64)
    }
}

fn mean(values: &[f64]) -> f64 {
    values.iter().sum::<f64>() / values.len() as f64
}

fn std_dev(values: &[f64], m: f64) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let var = values.iter().map(|v| (v - m).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    var.sqrt()
}

/// Builds one series per path for a month.
///
/// The residual is a deviation from the path-hour's own median across the
/// month, which removes the two unknowns that would otherwise dominate: the
/// station's antennas and local noise (constant per path) and the shape of
/// the diurnal curve (constant per hour). What is left is "how far did this
/// day sit from a typical day".
fn series_for_month(dir: &Path, min_hours: usize) -> Result<Vec<Series>, String> {
    let data = wspr::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let daily = wspr::load_daily(dir).map_err(|e| format!("{}: {e}", dir.display()))?;

    let mut labels: BTreeMap<PathKey, String> = BTreeMap::new();
    for p in &data.paths {
        labels.insert(p.key(), p.label());
    }

    let mut out = Vec::new();
    for (key, samples) in &daily {
        // Per hour: every day's value, so the hour's own centre can be taken.
        let mut by_hour: BTreeMap<u8, Vec<(u8, f64)>> = BTreeMap::new();
        for s in samples {
            by_hour.entry(s.hour).or_default().push((s.day, s.snr_median));
        }

        // Per day: the residuals of that day across all hours.
        let mut by_day: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
        for days in by_hour.values() {
            if days.len() < 2 {
                // One day at this hour makes its own centre, so the residual
                // would be exactly zero and carry nothing.
                continue;
            }
            let mut values: Vec<f64> = days.iter().map(|(_, v)| *v).collect();
            let centre = median_in_place(&mut values);
            for (day, value) in days {
                by_day.entry(*day).or_default().push(value - centre);
            }
        }

        let mut points: Vec<(u8, f64)> = by_day
            .into_iter()
            .filter(|(_, rs)| rs.len() >= min_hours)
            .map(|(day, rs)| (day, mean(&rs)))
            .collect();
        points.sort_by_key(|(day, _)| *day);
        if points.len() < 5 {
            continue;
        }

        // Standardise within the path so that a path with a wide spread does
        // not dominate the pooled figure.
        let values: Vec<f64> = points.iter().map(|(_, v)| *v).collect();
        let m = mean(&values);
        let sd = std_dev(&values, m);
        if sd <= 0.0 {
            continue;
        }
        out.push(Series {
            label: labels
                .get(key)
                .cloned()
                .unwrap_or_else(|| format!("{} to {} {} MHz", key.0, key.1, key.2)),
            points: points.into_iter().map(|(d, v)| (d, v, (v - m) / sd)).collect(),
        });
    }
    out.sort_by(|a, b| a.label.cmp(&b.label));
    Ok(out)
}

/// Whether a calendar day was geomagnetically disturbed.
fn disturbed(kp: &GeomagTable, year: u32, month: u32, day: u8) -> Option<bool> {
    let d = kp.get(year, month, day)?;
    Some(d.kp.iter().copied().fold(0.0_f64, f64::max) >= DISTURBED_KP)
}

struct MonthResult {
    name: String,
    paths: usize,
    lags: [Accum; 3],
    quiet: Accum,
    disturbed: Accum,
    /// Mean absolute daily residual, dB. Says how large the thing being
    /// predicted is, which decides whether any of this would matter.
    spread_db: f64,
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    };
    let root = PathBuf::from(flag("--data").unwrap_or_else(|| "data".into()));
    let min_hours = flag("--min-hours")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(DEFAULT_MIN_HOURS);

    let kp = match geomag::load(&root.join("kp_daily.txt")) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("kp_daily.txt: {e} — run tools/fetch-kp.sh");
            return ExitCode::FAILURE;
        }
    };

    // Every subdirectory that looks like a month, in order.
    let mut months: Vec<PathBuf> = match std::fs::read_dir(&root) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir() && p.join("daily.csv").is_file())
            .collect(),
        Err(e) => {
            eprintln!("{}: {e}", root.display());
            return ExitCode::FAILURE;
        }
    };
    months.sort();
    if months.is_empty() {
        eprintln!("no month directories with a daily.csv under {}", root.display());
        return ExitCode::FAILURE;
    }

    let mut results = Vec::new();
    let mut pooled = [Accum::default(); 3];
    let mut pooled_quiet = Accum::default();
    let mut pooled_disturbed = Accum::default();
    let mut all_abs: Vec<f64> = Vec::new();
    let mut worst: Vec<(f64, String, String)> = Vec::new();

    for dir in &months {
        let name = dir.file_name().unwrap_or_default().to_string_lossy().to_string();
        let (year, month) = match (
            name.get(..4).and_then(|s| s.parse::<u32>().ok()),
            name.get(5..7).and_then(|s| s.parse::<u32>().ok()),
        ) {
            (Some(y), Some(m)) => (y, m),
            _ => {
                eprintln!("{name}: not a YYYY-MM directory, skipped");
                continue;
            }
        };

        let series = match series_for_month(dir, min_hours) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{e}");
                continue;
            }
        };

        let mut lags = [Accum::default(); 3];
        let mut quiet = Accum::default();
        let mut dist = Accum::default();
        let mut abs_here: Vec<f64> = Vec::new();

        for s in &series {
            for (_, value, _) in &s.points {
                abs_here.push(value.abs());
                all_abs.push(value.abs());
            }
            // Pairs only where both days are present and exactly k apart.
            let index: BTreeMap<u8, f64> = s.points.iter().map(|(d, _, z)| (*d, *z)).collect();
            for (day, z) in &index {
                for (lag_index, lag) in [1u8, 2, 3].into_iter().enumerate() {
                    if let Some(z2) = day.checked_add(lag).and_then(|d| index.get(&d)) {
                        let product = z * z2;
                        lags[lag_index].push(product);
                        pooled[lag_index].push(product);
                        if lag == 1 {
                            // Classified by the first day of the pair: the
                            // question is whether today's state says
                            // anything about tomorrow.
                            match disturbed(&kp, year, month, *day) {
                                Some(true) => {
                                    dist.push(product);
                                    pooled_disturbed.push(product);
                                }
                                Some(false) => {
                                    quiet.push(product);
                                    pooled_quiet.push(product);
                                }
                                None => {}
                            }
                        }
                    }
                }
            }
            if let Some(v) = lag1_for(s) {
                worst.push((v, name.clone(), s.label.clone()));
            }
        }

        let mut abs_sorted = abs_here.clone();
        results.push(MonthResult {
            name,
            paths: series.len(),
            lags,
            quiet,
            disturbed: dist,
            spread_db: if abs_sorted.is_empty() {
                0.0
            } else {
                median_in_place(&mut abs_sorted)
            },
        });
    }

    // Report.
    println!("# Daily residual autocorrelation");
    println!();
    println!("Minimum hours per day: {min_hours}. Disturbed means Kp at or above {DISTURBED_KP}.");
    println!();
    println!("| month | paths | lag 1 | lag 2 | lag 3 | pairs | median abs residual |");
    println!("| --- | --: | --: | --: | --: | --: | --: |");
    for r in &results {
        println!(
            "| {} | {} | {} | {} | {} | {} | {:.2} dB |",
            r.name,
            r.paths,
            show(r.lags[0].value()),
            show(r.lags[1].value()),
            show(r.lags[2].value()),
            r.lags[0].n,
            r.spread_db,
        );
    }
    println!();
    println!("Lag 1 split by the geomagnetic state of the first day of the pair:");
    println!();
    println!("| month | quiet | pairs | disturbed | pairs |");
    println!("| --- | --: | --: | --: | --: |");
    for r in &results {
        println!(
            "| {} | {} | {} | {} | {} |",
            r.name,
            show(r.quiet.value()),
            r.quiet.n,
            show(r.disturbed.value()),
            r.disturbed.n,
        );
    }
    println!();
    println!("| pooled | lag 1 | lag 2 | lag 3 |");
    println!("| --- | --: | --: | --: |");
    println!(
        "| all days | {} | {} | {} |",
        show(pooled[0].value()),
        show(pooled[1].value()),
        show(pooled[2].value()),
    );
    println!();
    println!("Lag-1 pairs pooled: {}.", pooled[0].n);
    println!();
    println!("| first day of the pair | lag 1 | pairs |");
    println!("| --- | --: | --: |");
    println!(
        "| quiet | {} | {} |",
        show(pooled_quiet.value()),
        pooled_quiet.n
    );
    println!(
        "| disturbed | {} | {} |",
        show(pooled_disturbed.value()),
        pooled_disturbed.n
    );
    println!();
    if !all_abs.is_empty() {
        let typical = median(&all_abs);
        println!("Median absolute daily residual over every path and day: {typical:.2} dB.");

        // What perfect use of the correlation would be worth. A predictor
        // with correlation r removes r^2 of the variance, which leaves
        // sqrt(1 - r^2) of the deviation. This is the ceiling for a daily
        // model that predicts the next day from this one, and it is the
        // number that decides whether any of the rest is worth building.
        if let Some(r) = pooled[0].value() {
            let variance_share = r * r;
            let left = (1.0 - variance_share).max(0.0).sqrt();
            println!();
            println!(
                "A predictor with r = {r:.3} would explain {:.1}% of the daily variance, \
                 leaving {:.1}% of the deviation — so it would shrink a typical \
                 {typical:.2} dB daily residual to {:.2} dB, a gain of {:.2} dB.",
                variance_share * 100.0,
                left * 100.0,
                typical * left,
                typical * (1.0 - left),
            );
        }
    }

    worst.sort_by(|a, b| b.0.total_cmp(&a.0));
    println!();
    println!("Highest per-path lag-1, which is where a daily model would start:");
    println!();
    println!("| path | month | lag 1 |");
    println!("| --- | --- | --: |");
    for (v, month, label) in worst.iter().take(10) {
        println!("| {label} | {month} | {v:+.3} |");
    }

    ExitCode::SUCCESS
}

/// A single path's lag-1, for the per-path table. Separate from the pooled
/// accumulation so one path's figure is never a slice of the pool.
fn lag1_for(s: &Series) -> Option<f64> {
    let index: BTreeMap<u8, f64> = s.points.iter().map(|(d, _, z)| (*d, *z)).collect();
    let mut acc = Accum::default();
    for (day, z) in &index {
        if let Some(z2) = day.checked_add(1).and_then(|d| index.get(&d)) {
            acc.push(z * z2);
        }
    }
    acc.value()
}

fn show(v: Option<f64>) -> String {
    v.map_or_else(|| "—".to_string(), |x| format!("{x:+.3}"))
}
