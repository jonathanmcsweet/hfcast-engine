//! Measures the engine's day-to-day spread claims against per-day WSPR records.
//!
//! VOACAP claims a spread for every hour: 10% of days fall more than `SNR LW`
//! dB below that hour's monthly median, and 10% rise more than `SNR UP` above
//! it. The functions here pair those claims with what actually happened, day
//! by day. Both the `reliability` binary (overall calibration) and the `storm`
//! binary (quiet against disturbed days) are built on them.
//!
//! ## Why deviations, not absolute levels
//!
//! A path's absolute level is unknown (antennas), so probabilities against an
//! absolute threshold cannot be checked directly. Deviations from the path's
//! own monthly median are offset-free: "how often did a day fall 6 dB below
//! this hour's median" needs no knowledge of the antennas at all, and it is
//! exactly the quantity the deciles claim to describe.
//!
//! ## Censoring, handled per bin
//!
//! WSPR cannot report below its decode floor (about -29 dB), so downward
//! deviations from a low median are invisible. Every observed frequency here
//! is computed only where the deviation in question would still have been
//! comfortably decodable; bins that a path-hour cannot measure are excluded
//! for that path-hour rather than silently counted as "never happened".
//! Missing days are excluded too — a silent day can be a weak day or a
//! switched-off transmitter, and the two cannot be told apart.

use std::collections::BTreeMap;
use std::path::Path;

use crate::deck::{build_deck, DeckCase};
use crate::listing::parse_listing;
use crate::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};
use crate::stats::{median, phi, quantile};
use crate::wspr::{self, smoothed_ssn};

pub const VOACAP_VARIANT: &str = "O2";
const CONCURRENCY: usize = 4;

/// Matches the validated server configuration: sporadic-E on.
const SPORADIC_E: bool = true;

/// A day's median at one hour counts only with at least this many reports.
const MIN_SPOTS_PER_DAY: u32 = 4;

/// A path-hour needs this many measured days before its spread means anything.
const MIN_DAYS: usize = 20;

/// Downward deviations are only scored where the deviated value would still be
/// clearly decodable.
pub const CENSOR_SAFE_DB: f64 = -26.0;

/// Upward deviations are only scored well below where the reported scale
/// saturates.
pub const TOP_SAFE_DB: f64 = 12.0;

/// A decile is this many standard deviations of a normal distribution.
pub const DECILE_TO_SIGMA: f64 = 1.2816;

/// Deviation sizes scored, in dB.
pub const DEVIATIONS: [f64; 4] = [3.0, 6.0, 10.0, 15.0];

/// One measured day at one path-hour.
#[derive(Debug, Clone)]
pub struct DaySample {
    /// Day of the month, 1-31.
    pub day: u8,
    /// Median reported SNR that day at this hour, dB.
    pub value: f64,
}

/// One path-hour: the engine's spread claim and the measured days.
pub struct SpreadRecord {
    /// Engine decile distances, dB.
    pub lw: f64,
    pub up: f64,
    /// UTC hour, 0-23 — needed to look up the geomagnetic state.
    pub hour: u8,
    /// Observed daily medians for this hour across the month.
    pub days: Vec<DaySample>,
    /// Median of the day values.
    pub centre: f64,
}

pub struct MonthSpread {
    /// Month as `YYYY-MM`.
    pub name: String,
    pub year: u32,
    pub month: u32,
    pub records: Vec<SpreadRecord>,
    pub paths_run: usize,
    pub failures: usize,
}

/// Runs the engine for every path of a month and pairs its spread claims with
/// the measured days.
pub fn gather(dir: &Path) -> Result<MonthSpread, String> {
    let data = wspr::load(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let daily = wspr::load_daily(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (Some(year), Some(month)) = (data.year(), data.month_number()) else {
        return Err(format!("{}: unreadable month.txt", dir.display()));
    };
    let ssn =
        smoothed_ssn(&data.month).ok_or_else(|| format!("no smoothed SSN for {}", data.month))?;
    let voacap_bin = variant_bin(VOACAP_VARIANT);

    let outcomes = map_limit(&data.paths, CONCURRENCY, |path, index| {
        let case = DeckCase {
            id: format!("r{index}"),
            from_lat: path.tx_lat,
            from_lon: path.tx_lon,
            to_lat: path.rx_lat,
            to_lon: path.rx_lon,
            method: 30,
            ursi: false,
            fprob: None,
            botlines: None,
            toplines: None,
            month,
            year,
            ssn,
            watts: 1.0,
            required_snr_db: 24.0,
            noise_dbw: 145.0,
            freqs_mhz: vec![path.freq_mhz],
            tx_antennas: Vec::new(),
            rx_antennas: Vec::new(),
            outgraph: None,
            integrate: None,
            comment: None,
            extra_cards: Vec::new(),
            sporadic_e: SPORADIC_E,
        };
        let deck = build_deck(&case).map_err(|e| e.to_string())?;
        let root = IsolatedRoot::create(&format!("rel-{index}")).map_err(|e| e.to_string())?;
        let listing = run_deck(&voacap_bin, root.path(), &deck).map_err(|e| e.to_string())?;
        let parsed = parse_listing(&listing);

        // Engine decile distances per hour, slot 0 (the only frequency).
        let mut lw = [None::<f64>; 24];
        let mut up = [None::<f64>; 24];
        for s in parsed.numeric.iter().filter(|s| s.slot == 0) {
            match s.row.as_str() {
                "SNR LW" if s.value > 0.0 => lw[s.hour as usize] = Some(s.value),
                "SNR UP" if s.value > 0.0 => up[s.hour as usize] = Some(s.value),
                _ => {}
            }
        }

        // Measured days per hour.
        let mut days: [Vec<DaySample>; 24] = std::array::from_fn(|_| Vec::new());
        if let Some(samples) = daily.get(&path.key()) {
            for s in samples {
                if s.reports >= MIN_SPOTS_PER_DAY {
                    days[s.hour as usize].push(DaySample {
                        day: s.day,
                        value: s.snr_median,
                    });
                }
            }
        }

        let mut records = Vec::new();
        for hour in 0..24 {
            let (Some(lw), Some(up)) = (lw[hour], up[hour]) else {
                continue;
            };
            let observed = &days[hour];
            if observed.len() < MIN_DAYS {
                continue;
            }
            let mut values: Vec<f64> = observed.iter().map(|d| d.value).collect();
            records.push(SpreadRecord {
                lw,
                up,
                hour: hour as u8,
                days: observed.clone(),
                centre: median(&mut values),
            });
        }
        Ok::<Vec<SpreadRecord>, String>(records)
    });

    let mut records = Vec::new();
    let mut failures = 0usize;
    for outcome in outcomes {
        match outcome {
            Ok(mut r) => records.append(&mut r),
            Err(_) => failures += 1,
        }
    }

    Ok(MonthSpread {
        name: data.month,
        year,
        month,
        records,
        paths_run: data.paths.len(),
        failures,
    })
}

/// Writes gathered records to a cache file, so analyses can be re-run
/// without repeating hundreds of engine runs. One line per record:
/// `lw,up,hour,centre` followed by `day:value` pairs.
pub fn save_month(path: &Path, m: &MonthSpread) -> std::io::Result<()> {
    let mut out = String::new();
    out.push_str(&format!(
        "# spread-cache v1 {} {} {} {} {}\n",
        m.name, m.year, m.month, m.paths_run, m.failures
    ));
    for r in &m.records {
        out.push_str(&format!("{},{},{},{}", r.lw, r.up, r.hour, r.centre));
        for d in &r.days {
            out.push_str(&format!(",{}:{}", d.day, d.value));
        }
        out.push('\n');
    }
    std::fs::write(path, out)
}

/// Reads a cache file written by [`save_month`]. `None` when the file is
/// missing or does not parse, in which case the caller should gather afresh.
pub fn load_month(path: &Path) -> Option<MonthSpread> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines.next()?.split_whitespace().collect();
    if header.len() != 8 || header[1] != "spread-cache" || header[2] != "v1" {
        return None;
    }
    let mut records = Vec::new();
    for line in lines {
        let mut fields = line.split(',');
        let lw: f64 = fields.next()?.parse().ok()?;
        let up: f64 = fields.next()?.parse().ok()?;
        let hour: u8 = fields.next()?.parse().ok()?;
        let centre: f64 = fields.next()?.parse().ok()?;
        let mut days = Vec::new();
        for pair in fields {
            let (day, value) = pair.split_once(':')?;
            days.push(DaySample {
                day: day.parse().ok()?,
                value: value.parse().ok()?,
            });
        }
        records.push(SpreadRecord {
            lw,
            up,
            hour,
            days,
            centre,
        });
    }
    Some(MonthSpread {
        name: header[3].to_string(),
        year: header[4].parse().ok()?,
        month: header[5].parse().ok()?,
        records,
        paths_run: header[6].parse().ok()?,
        failures: header[7].parse().ok()?,
    })
}

/// Observed decile distances, where the floor allows them to be measured.
///
/// The lower distance is only trusted when the observed lower decile sits
/// clearly above the decode floor; otherwise it is truncated, not measured.
pub fn observed_deciles(r: &SpreadRecord) -> (Option<f64>, Option<f64>) {
    let mut values: Vec<f64> = r.days.iter().map(|d| d.value).collect();
    let q10 = quantile(&mut values, 0.1);
    let q90 = quantile(&mut values, 0.9);
    let lower = if q10 >= CENSOR_SAFE_DB {
        Some(r.centre - q10)
    } else {
        None
    };
    let upper = if q90 <= TOP_SAFE_DB {
        Some(q90 - r.centre)
    } else {
        None
    };
    (lower, upper)
}

/// Pooled least-squares ratio of observed decile distance to predicted.
pub fn fit_scale(records: &[SpreadRecord], lower: bool) -> (f64, usize) {
    let mut sum_xy = 0.0;
    let mut sum_xx = 0.0;
    let mut n = 0usize;
    for r in records {
        let (l, u) = observed_deciles(r);
        let (observed, predicted) = if lower { (l, r.lw) } else { (u, r.up) };
        let Some(observed) = observed else { continue };
        sum_xy += observed * predicted;
        sum_xx += predicted * predicted;
        n += 1;
    }
    if sum_xx <= 0.0 {
        (1.0, 0)
    } else {
        (sum_xy / sum_xx, n)
    }
}

pub struct CalibrationBin {
    pub predicted_sum: f64,
    pub beyond: usize,
    pub total_days: usize,
    pub path_hours: usize,
}

impl CalibrationBin {
    fn new() -> Self {
        Self {
            predicted_sum: 0.0,
            beyond: 0,
            total_days: 0,
            path_hours: 0,
        }
    }

    pub fn observed_percent(&self) -> f64 {
        if self.total_days == 0 {
            0.0
        } else {
            100.0 * self.beyond as f64 / self.total_days as f64
        }
    }

    pub fn predicted_percent(&self) -> f64 {
        if self.total_days == 0 {
            0.0
        } else {
            100.0 * self.predicted_sum / self.total_days as f64
        }
    }
}

/// For each deviation size: the mean predicted probability of a day deviating
/// that far, against the measured frequency, on the side chosen.
///
/// `keep` selects which days count — the storm analysis passes the
/// geomagnetic condition of each day-hour; passing `|_, _| true` scores every
/// day. Censor-safety stays per path-hour regardless of the filter.
pub fn calibration(
    records: &[SpreadRecord],
    lower: bool,
    scale: f64,
    keep: &dyn Fn(&SpreadRecord, &DaySample) -> bool,
) -> BTreeMap<String, CalibrationBin> {
    let mut bins: BTreeMap<String, CalibrationBin> = BTreeMap::new();

    for r in records {
        let selected: Vec<&DaySample> = r.days.iter().filter(|d| keep(r, d)).collect();
        if selected.is_empty() {
            continue;
        }
        for delta in DEVIATIONS {
            // Skip bins this path-hour cannot measure honestly.
            if lower && r.centre - delta < CENSOR_SAFE_DB {
                continue;
            }
            if !lower && r.centre + delta > TOP_SAFE_DB {
                continue;
            }
            let decile = if lower { r.lw } else { r.up };
            let sigma = (decile * scale).max(1e-6) / DECILE_TO_SIGMA;
            let predicted = phi(-delta / sigma);
            let beyond = selected
                .iter()
                .filter(|d| {
                    if lower {
                        d.value <= r.centre - delta
                    } else {
                        d.value >= r.centre + delta
                    }
                })
                .count();

            let bin = bins
                .entry(format!("{delta:>2.0} dB"))
                .or_insert_with(CalibrationBin::new);
            bin.predicted_sum += predicted * selected.len() as f64;
            bin.beyond += beyond;
            bin.total_days += selected.len();
            bin.path_hours += 1;
        }
    }

    bins
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(lw: f64, up: f64, values: Vec<f64>) -> SpreadRecord {
        let mut sorted = values.clone();
        let centre = median(&mut sorted);
        SpreadRecord {
            lw,
            up,
            hour: 12,
            days: values
                .into_iter()
                .enumerate()
                .map(|(i, value)| DaySample {
                    day: (i + 1) as u8,
                    value,
                })
                .collect(),
            centre,
        }
    }

    /// Thirty days spread symmetrically 0..±9 dB around -5: observed decile
    /// distance is about 8 dB each side.
    fn synthetic_days() -> Vec<f64> {
        (0..30)
            .map(|i| -5.0 + 9.0 * ((i as f64 / 29.0) * 2.0 - 1.0))
            .collect()
    }

    #[test]
    fn fit_recovers_an_overstated_spread() {
        // Engine claims 16 dB deciles; reality is about 8. Scale ≈ 0.5.
        let records = vec![record(16.0, 16.0, synthetic_days())];
        let (scale_low, n) = fit_scale(&records, true);
        assert_eq!(n, 1);
        assert!((scale_low - 0.5).abs() < 0.05, "scale_low {scale_low}");
    }

    #[test]
    fn censored_lower_deciles_are_not_fitted() {
        // Median -20, spread 9: the lower decile sits below the safe line, so
        // the lower side must be excluded rather than fitted from a truncated
        // number.
        let days: Vec<f64> = synthetic_days().iter().map(|d| d - 15.0).collect();
        let records = vec![record(16.0, 16.0, days)];
        let (_, n_low) = fit_scale(&records, true);
        let (_, n_up) = fit_scale(&records, false);
        assert_eq!(n_low, 0);
        assert_eq!(n_up, 1);
    }

    #[test]
    fn calibration_counts_what_actually_happened() {
        // Twenty days, one of them 10 dB below the median of 0.
        let mut days = vec![0.0; 9];
        days.push(-10.0);
        for _ in 0..2 {
            days.extend_from_slice(&[0.0; 5]);
        }
        let records = vec![record(10.0, 10.0, days)];
        let bins = calibration(&records, true, 1.0, &|_, _| true);
        let bin = bins.get(" 6 dB").expect("6 dB bin");
        assert_eq!(bin.beyond, 1);
        assert_eq!(bin.total_days, 20);
    }

    #[test]
    fn cache_round_trips() {
        let m = MonthSpread {
            name: "2025-06".to_string(),
            year: 2025,
            month: 6,
            records: vec![record(16.0, 8.5, synthetic_days())],
            paths_run: 150,
            failures: 2,
        };
        let path = std::env::temp_dir().join("propcore-spread-cache-test.csv");
        save_month(&path, &m).expect("writable");
        let back = load_month(&path).expect("parses");
        std::fs::remove_file(&path).ok();
        assert_eq!(back.name, m.name);
        assert_eq!(back.year, 2025);
        assert_eq!(back.records.len(), 1);
        assert_eq!(back.records[0].days.len(), 30);
        assert!((back.records[0].centre - m.records[0].centre).abs() < 1e-12);
        assert!((back.records[0].up - 8.5).abs() < 1e-12);
    }

    #[test]
    fn calibration_filter_selects_days() {
        // Same twenty days, but only the deviating day kept: the observed
        // frequency becomes 1 of 1.
        let mut days = vec![0.0; 19];
        days.push(-10.0);
        let records = vec![record(10.0, 10.0, days)];
        let bins = calibration(&records, true, 1.0, &|_, d| d.value < -5.0);
        let bin = bins.get(" 6 dB").expect("6 dB bin");
        assert_eq!(bin.beyond, 1);
        assert_eq!(bin.total_days, 1);
    }
}
