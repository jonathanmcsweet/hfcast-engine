//! Do geomagnetic storms need a wider spread? Measured, not assumed.
//!
//! The reliability validation (`reliability`, `docs/reliability.md`) showed
//! the calibrated spread matches reality for deviations of 3-10 dB but
//! under-predicts the rare deep fades. The suspected cause is geomagnetic
//! storms. This program tests that directly: every measured day-hour is
//! tagged with the Kp index around its own 3-hour block (from the GFZ
//! record, see [`propcore::geomag`]), and the spread is measured separately
//! for quiet and disturbed conditions.
//!
//! ## Method
//!
//! Each day's deviation from its path-hour's monthly median is divided by
//! the spread the calibrated model claims for that path-hour, giving a z
//! value. If the calibration is honest for a group of day-hours, 10% of its
//! z values fall below -1.28 (the definition of a decile). The ratio between
//! a group's measured 10% point and -1.28 is the widening factor that group
//! needs. Pooling z values across path-hours is what makes storm days
//! measurable at all: any single path-hour sees only a few of them.
//!
//! Censoring: pooled z quantiles use only path-hours whose median sits high
//! enough that a deviation of `Z_OBSERVABLE_DB` was still decodable, so the
//! pool is complete down to that depth. The frequency tables use the same
//! per-bin rules as the reliability check.
//!
//! Usage: `storm --kp <kp-file> --fit <month-dir> [--fit …] [--test <month-dir> …]`

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::ExitCode;

use propcore::geomag::{self, GeomagTable};
use propcore::runner::variant_bin;
use propcore::spread::{
    calibration, gather, load_month, save_month, DaySample, MonthSpread, SpreadRecord,
    CENSOR_SAFE_DB, DECILE_TO_SIGMA, DEVIATIONS, TOP_SAFE_DB, VOACAP_VARIANT,
};
use propcore::stats::quantile;

/// The spread scales the server ships (`server/src/voacap/correct.ts`).
const SHIPPED_SCALE_LOW: f64 = 0.40;
const SHIPPED_SCALE_UP: f64 = 0.59;

/// Pooled z quantiles only use path-hours observable to this deviation depth.
const Z_OBSERVABLE_DB: f64 = 15.0;

/// How far back to look for a disturbance. Ionospheric storm effects outlast
/// the geomagnetic disturbance itself, often by a day.
const LOOKBACK_HOURS: u8 = 24;

/// Kp group boundaries: below the first is quiet, at or above the second is
/// a storm, between them is unsettled.
const KP_QUIET_BELOW: f64 = 3.0;
const KP_STORM_FROM: f64 = 5.0;

/// A group needs at least this many pooled days before a quantile means much.
const MIN_POOL: usize = 200;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Group {
    Quiet,
    Unsettled,
    Storm,
}

const GROUPS: [Group; 3] = [Group::Quiet, Group::Unsettled, Group::Storm];

impl Group {
    fn label(self) -> &'static str {
        match self {
            Group::Quiet => "quiet (Kp < 3)",
            Group::Unsettled => "unsettled (3-5)",
            Group::Storm => "storm (Kp >= 5)",
        }
    }

    fn of(kp: f64) -> Group {
        if kp >= KP_STORM_FROM {
            Group::Storm
        } else if kp >= KP_QUIET_BELOW {
            Group::Unsettled
        } else {
            Group::Quiet
        }
    }

    fn index(self) -> usize {
        match self {
            Group::Quiet => 0,
            Group::Unsettled => 1,
            Group::Storm => 2,
        }
    }
}

/// A gathered month with the calendar identity its geomagnetic lookup needs.
struct TaggedMonth {
    spread: MonthSpread,
    year: u32,
    month: u32,
}

impl TaggedMonth {
    /// Group of one measured day-hour, by the highest Kp in the preceding
    /// `LOOKBACK_HOURS`. `None` when the geomagnetic record is missing.
    fn group_of(&self, table: &GeomagTable, r: &SpreadRecord, d: &DaySample) -> Option<Group> {
        let kp = table.kp_max_lookback(self.year, self.month, d.day, r.hour, LOOKBACK_HOURS)?;
        Some(Group::of(kp))
    }
}

/// Finer Kp bands, for fitting how the widening grows with storm strength.
const KP_BANDS: [(f64, f64, &str); 7] = [
    (0.0, 2.0, "Kp < 2"),
    (2.0, 3.0, "2-3"),
    (3.0, 4.0, "3-4"),
    (4.0, 5.0, "4-5"),
    (5.0, 6.0, "5-6"),
    (6.0, 7.0, "6-7"),
    (7.0, 10.0, "Kp >= 7"),
];

/// Signed z values per group, one pool per side. A day enters the "below"
/// pool of its group whenever its path-hour is observable deep enough, no
/// matter which side it fell on: low quantiles need the good days in the
/// denominator.
struct ZPools {
    below: [Vec<f64>; 3],
    above: [Vec<f64>; 3],
    /// Below-side pools again, split by the finer Kp bands.
    below_by_band: [Vec<f64>; 7],
    unclassified: usize,
}

fn pool_z(months: &[TaggedMonth], table: &GeomagTable) -> ZPools {
    let mut pools = ZPools {
        below: std::array::from_fn(|_| Vec::new()),
        above: std::array::from_fn(|_| Vec::new()),
        below_by_band: std::array::from_fn(|_| Vec::new()),
        unclassified: 0,
    };
    for m in months {
        for r in &m.spread.records {
            let below_ok = r.centre - Z_OBSERVABLE_DB >= CENSOR_SAFE_DB;
            let above_ok = r.centre + Z_OBSERVABLE_DB <= TOP_SAFE_DB;
            if !below_ok && !above_ok {
                continue;
            }
            let sigma_low = (r.lw * SHIPPED_SCALE_LOW).max(1e-6) / DECILE_TO_SIGMA;
            let sigma_up = (r.up * SHIPPED_SCALE_UP).max(1e-6) / DECILE_TO_SIGMA;
            for d in &r.days {
                let Some(kp) =
                    table.kp_max_lookback(m.year, m.month, d.day, r.hour, LOOKBACK_HOURS)
                else {
                    pools.unclassified += 1;
                    continue;
                };
                let group = Group::of(kp);
                let deviation = d.value - r.centre;
                let sigma = if deviation < 0.0 { sigma_low } else { sigma_up };
                let z = deviation / sigma;
                if below_ok {
                    pools.below[group.index()].push(z);
                    if let Some(band) = KP_BANDS
                        .iter()
                        .position(|(lo, hi, _)| kp >= *lo && kp < *hi)
                    {
                        pools.below_by_band[band].push(z);
                    }
                }
                if above_ok {
                    pools.above[group.index()].push(z);
                }
            }
        }
    }
    pools
}

fn print_band_table(pools: &ZPools) {
    println!("| Kp (24h max) | day-hours | z at 10% | widening needed |");
    println!("| --- | --: | --: | --: |");
    for (i, (_, _, label)) in KP_BANDS.iter().enumerate() {
        let pool = &pools.below_by_band[i];
        match widening_below(pool) {
            Some(w) => {
                let z10 = quantile(&mut pool.to_vec(), 0.1);
                println!("| {label} | {} | {z10:.2} | x {w:.2} |", pool.len());
            }
            None => println!("| {label} | {} | too few | |", pool.len()),
        }
    }
}

/// The widening a group's below side needs: its measured 10% point against
/// the -1.28 a calibrated model would show there.
fn widening_below(pool: &[f64]) -> Option<f64> {
    if pool.len() < MIN_POOL {
        return None;
    }
    let z10 = quantile(&mut pool.to_vec(), 0.1);
    Some((-z10 / DECILE_TO_SIGMA).max(0.0))
}

/// The graded rule under test: no widening while the last 24 hours stayed
/// below Kp 4.75, then half a unit of widening per Kp step, capped where the
/// measurements end. The line is drawn through the per-band widening tables
/// of all eight months; the evidence that it generalises is that the June
/// band gradient and the seven-other-months band gradient agree.
fn rule_widening(kp_max_24h: f64) -> f64 {
    (1.0 + 0.5 * (kp_max_24h - 4.75)).clamp(1.0, 2.5)
}

fn print_z_summary(pools: &ZPools, side_below: bool) {
    if side_below {
        println!("| condition | day-hours | z at 10% | z at 5% | z at 2% | widening needed |");
    } else {
        println!("| condition | day-hours | z at 90% | z at 95% | z at 98% | widening needed |");
    }
    println!("| --- | --: | --: | --: | --: | --: |");
    for group in GROUPS {
        let pool = if side_below {
            &pools.below[group.index()]
        } else {
            &pools.above[group.index()]
        };
        if pool.len() < MIN_POOL {
            println!("| {} | {} | too few | | | |", group.label(), pool.len());
            continue;
        }
        let q = |f: f64| quantile(&mut pool.to_vec(), f);
        let (a, b, c) = if side_below {
            (q(0.10), q(0.05), q(0.02))
        } else {
            (q(0.90), q(0.95), q(0.98))
        };
        let widening = (a.abs() / DECILE_TO_SIGMA).max(0.0);
        println!(
            "| {} | {} | {a:.2} | {b:.2} | {c:.2} | x {widening:.2} |",
            group.label(),
            pool.len(),
        );
    }
}

/// Merged calibration bins for one group across months: predicted percent at
/// the shipped scale, at the widened scale, and what actually happened.
fn group_table(
    months: &[TaggedMonth],
    table: &GeomagTable,
    group: Group,
    lower: bool,
    storm_widening: f64,
) -> BTreeMap<String, (f64, f64, usize, usize)> {
    let shipped = if lower {
        SHIPPED_SCALE_LOW
    } else {
        SHIPPED_SCALE_UP
    };
    let widened = if group == Group::Storm {
        shipped * storm_widening
    } else {
        shipped
    };
    let mut merged: BTreeMap<String, (f64, f64, usize, usize)> = BTreeMap::new();
    for m in months {
        let keep = |r: &SpreadRecord, d: &DaySample| m.group_of(table, r, d) == Some(group);
        let plain = calibration(&m.spread.records, lower, shipped, &keep);
        let wide = calibration(&m.spread.records, lower, widened, &keep);
        for (label, bin) in &plain {
            let widened_sum = wide.get(label).map_or(0.0, |b| b.predicted_sum);
            let entry = merged.entry(label.clone()).or_insert((0.0, 0.0, 0, 0));
            entry.0 += bin.predicted_sum;
            entry.1 += widened_sum;
            entry.2 += bin.beyond;
            entry.3 += bin.total_days;
        }
    }
    merged
}

fn print_group_tables(
    months: &[TaggedMonth],
    table: &GeomagTable,
    lower: bool,
    storm_widening: f64,
) {
    let side = if lower { "below" } else { "above" };
    for group in GROUPS {
        println!("\n{} — days {side} the median:\n", group.label());
        println!(
            "| deviation | calibrated model says | with storm widening | actually happened | days |"
        );
        println!("| --- | --: | --: | --: | --: |");
        for (label, (plain, wide, beyond, total)) in
            group_table(months, table, group, lower, storm_widening)
        {
            if total == 0 {
                continue;
            }
            let pct = |x: f64| 100.0 * x / total as f64;
            println!(
                "| {label} {side} | {:.1}% | {:.1}% | {:.1}% | {total} |",
                pct(plain),
                pct(wide),
                pct(beyond as f64),
            );
        }
    }
}

/// Checks the graded rule the way the app will use it: every day-hour gets
/// its own widening from its own Kp history, and predicted frequencies are
/// compared with measured ones per storm-strength band.
fn rule_check(months: &[TaggedMonth], table: &GeomagTable) {
    struct Sums {
        shipped: f64,
        ruled: f64,
        beyond: usize,
        total: usize,
    }
    // Bands at and above the rule's threshold; quiet bands are unchanged.
    let bands: [(f64, f64, &str); 4] = [
        (4.0, 5.0, "4-5"),
        (5.0, 6.0, "5-6"),
        (6.0, 7.0, "6-7"),
        (7.0, 10.0, "Kp >= 7"),
    ];
    let mut sums: Vec<Vec<Sums>> = (0..bands.len())
        .map(|_| {
            DEVIATIONS
                .iter()
                .map(|_| Sums {
                    shipped: 0.0,
                    ruled: 0.0,
                    beyond: 0,
                    total: 0,
                })
                .collect()
        })
        .collect();

    for m in months {
        for r in &m.spread.records {
            let sigma = (r.lw * SHIPPED_SCALE_LOW).max(1e-6) / DECILE_TO_SIGMA;
            for d in &r.days {
                let Some(kp) =
                    table.kp_max_lookback(m.year, m.month, d.day, r.hour, LOOKBACK_HOURS)
                else {
                    continue;
                };
                let Some(band) = bands.iter().position(|(lo, hi, _)| kp >= *lo && kp < *hi) else {
                    continue;
                };
                for (i, delta) in DEVIATIONS.iter().enumerate() {
                    if r.centre - delta < CENSOR_SAFE_DB {
                        continue;
                    }
                    let s = &mut sums[band][i];
                    s.shipped += propcore::stats::phi(-delta / sigma);
                    s.ruled += propcore::stats::phi(-delta / (sigma * rule_widening(kp)));
                    s.beyond += usize::from(d.value <= r.centre - delta);
                    s.total += 1;
                }
            }
        }
    }

    for (b, (_, _, label)) in bands.iter().enumerate() {
        println!("\n{label} — days below the median:\n");
        println!(
            "| deviation | calibrated model says | with graded rule | actually happened | days |"
        );
        println!("| --- | --: | --: | --: | --: |");
        for (i, delta) in DEVIATIONS.iter().enumerate() {
            let s = &sums[b][i];
            if s.total == 0 {
                continue;
            }
            let pct = |x: f64| 100.0 * x / s.total as f64;
            println!(
                "| {delta:>2.0} dB below | {:.1}% | {:.1}% | {:.1}% | {} |",
                pct(s.shipped),
                pct(s.ruled),
                pct(s.beyond as f64),
                s.total
            );
        }
    }
}

fn census(months: &[TaggedMonth], table: &GeomagTable) {
    println!("| month | quiet | unsettled | storm | no record |");
    println!("| --- | --: | --: | --: | --: |");
    for m in months {
        let mut counts = [0usize; 3];
        let mut missing = 0usize;
        for r in &m.spread.records {
            for d in &r.days {
                match m.group_of(table, r, d) {
                    Some(g) => counts[g.index()] += 1,
                    None => missing += 1,
                }
            }
        }
        println!(
            "| {} | {} | {} | {} | {} |",
            m.spread.name, counts[0], counts[1], counts[2], missing
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

fn gather_tagged(dirs: &[PathBuf], cache: Option<&PathBuf>) -> Result<Vec<TaggedMonth>, String> {
    let mut months = Vec::new();
    for dir in dirs {
        let cache_file = cache.map(|c| {
            let stem = dir
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "month".to_string());
            c.join(format!("{stem}.spread"))
        });
        let cached = cache_file.as_ref().and_then(|f| load_month(f));
        let m = match cached {
            Some(m) => {
                eprintln!("{}: {} spread records (cached)", m.name, m.records.len());
                m
            }
            None => {
                let m = gather(dir)?;
                eprintln!(
                    "{}: {} spread records from {} paths ({} failed)",
                    m.name,
                    m.records.len(),
                    m.paths_run,
                    m.failures
                );
                if let Some(f) = &cache_file {
                    if let Err(e) = save_month(f, &m) {
                        eprintln!("cache write failed ({}): {e}", f.display());
                    }
                }
                m
            }
        };
        months.push(TaggedMonth {
            year: m.year,
            month: m.month,
            spread: m,
        });
    }
    Ok(months)
}

fn print_month_set(months: &[TaggedMonth], table: &GeomagTable, heading: &str) {
    println!("## Day-hours per condition ({heading})\n");
    census(months, table);
    let pools = pool_z(months, table);
    if pools.unclassified > 0 {
        println!(
            "\n{} day-hours had no geomagnetic record.",
            pools.unclassified
        );
    }
    println!("\n## Pooled z quantiles ({heading})\n");
    println!("Days below the median:\n");
    print_z_summary(&pools, true);
    println!("\nDays above the median:\n");
    print_z_summary(&pools, false);
    println!("\n## Widening by storm strength, below side ({heading})\n");
    print_band_table(&pools);
}

fn main() -> ExitCode {
    let kp_files = args_of("--kp");
    let fit_dirs = args_of("--fit");
    let test_dirs = args_of("--test");
    let cache_dirs = args_of("--cache");
    let cache = cache_dirs.first();
    if let Some(c) = cache {
        if let Err(e) = std::fs::create_dir_all(c) {
            eprintln!("cannot create cache directory {}: {e}", c.display());
            return ExitCode::FAILURE;
        }
    }
    let usage =
        "usage: storm --kp <kp-file> --fit <month-dir> [--fit …] [--test <month-dir> …] [--cache <dir>]";
    let Some(kp_file) = kp_files.first() else {
        eprintln!("{usage}");
        return ExitCode::FAILURE;
    };
    if fit_dirs.is_empty() {
        eprintln!("{usage}");
        return ExitCode::FAILURE;
    }
    if !variant_bin(VOACAP_VARIANT).is_file() {
        eprintln!("no voacapl variant binary; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }
    let table = match geomag::load(kp_file) {
        Ok(t) if !t.is_empty() => t,
        Ok(_) => {
            eprintln!("{}: no usable lines", kp_file.display());
            return ExitCode::FAILURE;
        }
        Err(e) => {
            eprintln!("{}: {e}", kp_file.display());
            return ExitCode::FAILURE;
        }
    };

    let fit_months = match gather_tagged(&fit_dirs, cache) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };

    println!("# Do storm days need a wider spread?\n");
    println!(
        "Every measured day-hour is tagged with the highest Kp of the \
         preceding {LOOKBACK_HOURS} hours. z is the day's deviation divided \
         by the calibrated model's claimed spread for that path-hour; if the \
         calibration holds for a group, its z at 10% is -1.28, and the \
         widening column is how much wider the spread must be to make that \
         true.\n"
    );

    print_month_set(&fit_months, &table, "fit months");

    let fit_pools = pool_z(&fit_months, &table);
    let storm_widening = widening_below(&fit_pools.below[Group::Storm.index()])
        .unwrap_or(1.0)
        .max(1.0);
    println!("\nStorm widening fitted on the fit months (below side): x {storm_widening:.2}\n");

    if !test_dirs.is_empty() {
        let test_months = match gather_tagged(&test_dirs, cache) {
            Ok(m) => m,
            Err(e) => {
                eprintln!("{e}");
                return ExitCode::FAILURE;
            }
        };
        print_month_set(&test_months, &table, "test months");
        println!(
            "\n## Frequencies by condition, test months (storm widening x {storm_widening:.2})"
        );
        print_group_tables(&test_months, &table, true, storm_widening);
        print_group_tables(&test_months, &table, false, storm_widening);
        println!(
            "\n## Graded rule on the test months: widening 1 + 0.5 x (Kp24 - 4.75), capped at 2.5"
        );
        rule_check(&test_months, &table);
    }

    ExitCode::SUCCESS
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_split_at_the_documented_boundaries() {
        assert_eq!(Group::of(2.9), Group::Quiet);
        assert_eq!(Group::of(3.0), Group::Unsettled);
        assert_eq!(Group::of(4.9), Group::Unsettled);
        assert_eq!(Group::of(5.0), Group::Storm);
        assert_eq!(Group::of(8.7), Group::Storm);
    }

    #[test]
    fn widening_reads_the_ten_percent_point() {
        // A pool whose 10% point is exactly -2.5632 needs twice the spread.
        let pool: Vec<f64> = (0..1000)
            .map(|i| {
                let f = i as f64 / 999.0;
                // Piecewise: bottom 10% at -2.5632, the rest at 0 or above.
                if f < 0.1 {
                    -2.5632
                } else {
                    f
                }
            })
            .collect();
        let w = widening_below(&pool).expect("pool is large enough");
        assert!((w - 2.0).abs() < 0.05, "widening {w}");
    }

    #[test]
    fn widening_refuses_a_small_pool() {
        assert_eq!(widening_below(&[-3.0; 50]), None);
    }

    #[test]
    fn rule_is_flat_when_quiet_and_capped_when_severe() {
        assert_eq!(rule_widening(1.0), 1.0);
        assert_eq!(rule_widening(4.75), 1.0);
        assert!((rule_widening(5.5) - 1.375).abs() < 1e-9);
        assert!((rule_widening(6.5) - 1.875).abs() < 1e-9);
        assert_eq!(rule_widening(9.0), 2.5);
    }
}
