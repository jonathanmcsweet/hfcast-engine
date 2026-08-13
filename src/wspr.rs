//! Reads the aggregated WSPR reception reports written by `tools/fetch-wspr.sh`.
//!
//! WSPR is the only large public source that records both ends of the
//! experiment: every report carries the transmit power, the measured
//! signal-to-noise ratio, both locations and a timestamp. That is the input and
//! output pair a propagation model claims to predict.
//!
//! A path here is a fixed pair of stations on a fixed band. Holding the
//! stations fixed also holds fixed the two things nobody knows — their antennas
//! and the receiver's local noise — so within one path those become a single
//! constant offset rather than scatter. Fitting and removing that offset is
//! what makes the comparison possible; it is also why this measures how well a
//! model tracks the daily *shape* of a circuit rather than its absolute level.
//!
//! Two conventions matter when comparing with a model:
//!
//! - WSPR reports signal-to-noise ratio in a 2500 Hz reference bandwidth.
//! - VOACAP reports it in 1 Hz. Its own listing confirms this: signal power
//!   minus noise power equals the printed SNR exactly, and its noise figure is
//!   a 1 Hz value. The difference is [`WSPR_BANDWIDTH_OFFSET_DB`].

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::Path;

/// WSPR's reference bandwidth, in hertz.
pub const WSPR_BANDWIDTH_HZ: f64 = 2500.0;

/// Subtract this from a 1 Hz signal-to-noise ratio to reach WSPR's reference.
///
/// `10 * log10(2500)`, about 34.0 dB.
pub const WSPR_BANDWIDTH_OFFSET_DB: f64 = 33.979_400_086_720_38;

/// Roughly where the WSPR decoder stops returning anything.
///
/// Reports below this do not exist, so a median near it is truncated from
/// below rather than measured, and reads higher than the truth.
pub const DECODE_FLOOR_DB: f64 = -29.0;

/// Identifies one transmitter, receiver and band.
pub type PathKey = (String, String, i32);

/// Smoothed sunspot number (R12) per month, from NOAA SWPC's observed solar
/// cycle indices.
///
/// Deliberately a table rather than a fetch: R12 for a past month never
/// changes once published, and a validation run should not depend on a
/// network service being up or on which day it was run.
///
/// Months at the end marked predicted are for the live loop
/// (`tools/live-check.sh`): R12 is a 13-month smooth, so the current
/// month cannot have an observed value yet. SWPC's predicted value
/// stands in and is replaced when the observed one lands. The daily
/// index fit does not depend on this number (foF2 is linear in the
/// index, so the fit finds its own level); only the climatology
/// column's own score reads it.
pub const SMOOTHED_SSN: &[(&str, f64)] = &[
    ("2015-03", 82.1),
    ("2019-06", 3.7),
    ("2019-12", 1.8),
    ("2022-09", 96.5),
    ("2024-12", 151.2),
    ("2025-03", 135.9),
    ("2025-04", 133.4),
    ("2025-05", 128.6),
    ("2025-06", 124.7),
    ("2025-07", 122.5),
    ("2025-08", 118.4),
    // Predicted (SWPC predicted-solar-cycle, fetched 2026-08-13).
    ("2026-08", 95.4),
];

pub fn smoothed_ssn(month: &str) -> Option<f64> {
    SMOOTHED_SSN
        .iter()
        .find(|(m, _)| *m == month)
        .map(|(_, v)| *v)
}

#[derive(Debug, Clone, PartialEq)]
pub struct WsprPath {
    pub tx: String,
    pub rx: String,
    /// Band in MHz as WSPR labels it, such as 14 for the 20 m band.
    pub band: i32,
    pub reports: u64,
    pub km: f64,
    /// Reported transmit power in dBm.
    pub power_dbm: f64,
    pub tx_lat: f64,
    pub tx_lon: f64,
    pub rx_lat: f64,
    pub rx_lon: f64,
    /// The actual transmitted frequency, not the band label.
    pub freq_mhz: f64,
}

impl WsprPath {
    pub fn key(&self) -> PathKey {
        (self.tx.clone(), self.rx.clone(), self.band)
    }

    /// Transmit power in watts. WSPR reports dBm, where 30 dBm is one watt.
    pub fn watts(&self) -> f64 {
        10f64.powf((self.power_dbm - 30.0) / 10.0)
    }

    pub fn label(&self) -> String {
        format!("{}>{} {}m", self.tx, self.rx, band_metres(self.band))
    }
}

/// Approximate wavelength in metres, for labelling only.
fn band_metres(band: i32) -> i32 {
    match band {
        1 => 160,
        3 => 80,
        5 => 60,
        7 => 40,
        10 => 30,
        14 => 20,
        18 => 17,
        21 => 15,
        24 => 12,
        28 => 10,
        50 => 6,
        other => other,
    }
}

/// Median reported signal-to-noise ratio per UTC hour, indexed 0-23.
pub type HourlySnr = [Option<f64>; 24];

/// One day's reports for one path at one hour.
#[derive(Debug, Clone, PartialEq)]
pub struct DailySample {
    /// Day of the month, 1-31.
    pub day: u8,
    /// UTC hour, 0-23.
    pub hour: u8,
    pub reports: u32,
    pub snr_median: f64,
}

#[derive(Debug, Clone)]
pub struct WsprData {
    pub paths: Vec<WsprPath>,
    pub hourly: HashMap<PathKey, HourlySnr>,
    /// Month the reports came from, as `YYYY-MM`.
    pub month: String,
}

impl WsprData {
    /// Month number, 1-12.
    pub fn month_number(&self) -> Option<u32> {
        self.month.split('-').nth(1)?.parse().ok()
    }

    pub fn year(&self) -> Option<u32> {
        self.month.split('-').next()?.parse().ok()
    }
}

/// Splits one CSV line, honouring double quotes around fields.
///
/// Call signs contain `/` and `-` but not commas, so this only has to handle
/// quoting, not escaping.
fn split_csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut current = String::new();
    let mut quoted = false;

    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => fields.push(std::mem::take(&mut current)),
            c => current.push(c),
        }
    }
    fields.push(current);
    fields
}

/// Maps a header row to column positions, so column order is not assumed.
fn header_index(line: &str) -> HashMap<String, usize> {
    split_csv(line)
        .into_iter()
        .enumerate()
        .map(|(i, name)| (name, i))
        .collect()
}

fn get<'a>(fields: &'a [String], index: &HashMap<String, usize>, name: &str) -> Option<&'a str> {
    Some(fields.get(*index.get(name)?)?.as_str())
}

/// One numeric field, or `None` when the column holds no usable number.
///
/// `f64::from_str` accepts `NaN` and `inf`, which no reading is, and a
/// row that carried one was kept: `stats::median` then stopped the whole
/// run at its own check, inside a `thread::scope`, so one bad row ended
/// a month rather than being left out of it.
fn number(fields: &[String], index: &HashMap<String, usize>, name: &str) -> Option<f64> {
    let value: f64 = get(fields, index, name)?.parse().ok()?;
    value.is_finite().then_some(value)
}

pub fn load(dir: &Path) -> io::Result<WsprData> {
    let paths_text = fs::read_to_string(dir.join("paths.csv"))?;
    let hourly_text = fs::read_to_string(dir.join("hourly.csv"))?;
    let month = fs::read_to_string(dir.join("month.txt"))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    Ok(WsprData {
        paths: parse_paths(&paths_text),
        hourly: parse_hourly(&hourly_text),
        month,
    })
}

pub fn parse_paths(text: &str) -> Vec<WsprPath> {
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return Vec::new();
    };
    let index = header_index(header);

    lines
        .filter_map(|line| {
            let f = split_csv(line);
            Some(WsprPath {
                tx: get(&f, &index, "tx_sign")?.to_string(),
                rx: get(&f, &index, "rx_sign")?.to_string(),
                band: number(&f, &index, "band")? as i32,
                reports: number(&f, &index, "reports")? as u64,
                km: number(&f, &index, "km")?,
                power_dbm: number(&f, &index, "power_dbm")?,
                tx_lat: number(&f, &index, "tx_lat")?,
                tx_lon: number(&f, &index, "tx_lon")?,
                rx_lat: number(&f, &index, "rx_lat")?,
                rx_lon: number(&f, &index, "rx_lon")?,
                freq_mhz: number(&f, &index, "freq_hz")? / 1e6,
            })
        })
        .collect()
}

/// Reads `daily.csv`, one row per path, day and hour.
pub fn parse_daily(text: &str) -> HashMap<PathKey, Vec<DailySample>> {
    let mut out: HashMap<PathKey, Vec<DailySample>> = HashMap::new();
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return out;
    };
    let index = header_index(header);

    for line in lines {
        let f = split_csv(line);
        let (Some(tx), Some(rx), Some(band), Some(day), Some(hour), Some(reports), Some(snr)) = (
            get(&f, &index, "tx_sign"),
            get(&f, &index, "rx_sign"),
            number(&f, &index, "band"),
            number(&f, &index, "day"),
            number(&f, &index, "hour"),
            number(&f, &index, "reports"),
            number(&f, &index, "snr_median"),
        ) else {
            continue;
        };
        if !(1.0..=31.0).contains(&day) || !(0.0..=23.0).contains(&hour) {
            continue;
        }
        out.entry((tx.to_string(), rx.to_string(), band as i32))
            .or_default()
            .push(DailySample {
                day: day as u8,
                hour: hour as u8,
                reports: reports as u32,
                snr_median: snr,
            });
    }

    out
}

/// Loads `daily.csv` from a month directory.
pub fn load_daily(dir: &Path) -> io::Result<HashMap<PathKey, Vec<DailySample>>> {
    Ok(parse_daily(&fs::read_to_string(dir.join("daily.csv"))?))
}

// ---- link-level scoring ----------------------------------------------
//
// The conventions of docs/irtam.md, shared so every daily-model study
// scores the same way: absolute error after one offset per path (the
// station's antennas and local noise are unknown but constant), and
// day-to-day deviations from each path-hour's own monthly median (where
// a model that never varies by day scores exactly zero).

/// One scored model value against one observed path-day-hour.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scored {
    /// Path identity: an index into the month's path list.
    pub path: usize,
    pub day: u8,
    pub hour: u8,
    pub observed: f64,
    pub predicted: f64,
}

/// Median absolute error after removing one offset per path, dB.
pub fn offset_adjusted_mae(samples: &[Scored]) -> f64 {
    let mut by_path: HashMap<usize, Vec<f64>> = HashMap::new();
    for s in samples {
        by_path
            .entry(s.path)
            .or_default()
            .push(s.observed - s.predicted);
    }
    let offsets: HashMap<usize, f64> = by_path
        .into_iter()
        .map(|(p, mut residuals)| (p, crate::stats::median_in_place(&mut residuals)))
        .collect();
    let mut errors: Vec<f64> = samples
        .iter()
        .map(|s| (s.observed - s.predicted - offsets[&s.path]).abs())
        .collect();
    crate::stats::median_in_place(&mut errors)
}

/// One deviation pair: how far the day sat from its path-hour's monthly
/// median, observed and as the model predicted.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeviationPair {
    pub path: usize,
    pub day: u8,
    pub hour: u8,
    pub observed: f64,
    pub predicted: f64,
}

/// Deviations of observation and model from their own per-path-hour
/// monthly medians, over path-hours with at least five scored days.
pub fn deviations(samples: &[Scored]) -> Vec<DeviationPair> {
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
            .push(s.predicted);
    }
    let centre = |m: &HashMap<(usize, u8), Vec<f64>>| -> HashMap<(usize, u8), f64> {
        m.iter()
            .filter(|(_, v)| v.len() >= 5)
            .map(|(k, v)| (*k, crate::stats::median(v)))
            .collect()
    };
    let obs_centre = centre(&obs_by_hour);
    let pred_centre = centre(&pred_by_hour);
    samples
        .iter()
        .filter_map(|s| {
            let key = (s.path, s.hour);
            let (oc, pc) = (obs_centre.get(&key)?, pred_centre.get(&key)?);
            Some(DeviationPair {
                path: s.path,
                day: s.day,
                hour: s.hour,
                observed: s.observed - oc,
                predicted: s.predicted - pc,
            })
        })
        .collect()
}

pub fn parse_hourly(text: &str) -> HashMap<PathKey, HourlySnr> {
    let mut out: HashMap<PathKey, HourlySnr> = HashMap::new();
    let mut lines = text.lines();
    let Some(header) = lines.next() else {
        return out;
    };
    let index = header_index(header);

    for line in lines {
        let f = split_csv(line);
        let (Some(tx), Some(rx), Some(band), Some(hour), Some(snr)) = (
            get(&f, &index, "tx_sign"),
            get(&f, &index, "rx_sign"),
            number(&f, &index, "band"),
            number(&f, &index, "hour"),
            number(&f, &index, "snr_median"),
        ) else {
            continue;
        };
        let hour = hour as usize;
        if hour > 23 {
            continue;
        }
        let entry = out
            .entry((tx.to_string(), rx.to_string(), band as i32))
            .or_insert([None; 24]);
        entry[hour] = Some(snr);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATHS: &str = concat!(
        "\"tx_sign\",\"rx_sign\",\"band\",\"reports\",\"km\",\"power_dbm\",",
        "\"tx_lat\",\"tx_lon\",\"rx_lat\",\"rx_lon\",\"freq_hz\"\n",
        "\"WW0WWV\",\"VE6JY\",7,20560,1564,30,40.688,-104.958,53.729,-112.792,7040052\n",
        "\"2E0DLC\",\"EA8BFK\",14,5000,2800,23,52.5,-2.0,28.1,-15.4,14097043\n",
    );

    const HOURLY: &str = concat!(
        "\"tx_sign\",\"rx_sign\",\"band\",\"hour\",\"reports\",\"snr_median\"\n",
        "\"2E0DLC\",\"EA8BFK\",14,0,238,-18\n",
        "\"2E0DLC\",\"EA8BFK\",14,23,240,-12.5\n",
    );

    #[test]
    fn a_row_with_no_usable_number_is_left_out() {
        // `NaN` and `inf` parse as numbers. A row with one was kept, and
        // `stats::median` then stopped the run at its own check — inside
        // a `thread::scope`, so one row ended a whole month.
        let daily = concat!(
            "\"tx_sign\",\"rx_sign\",\"band\",\"day\",\"hour\",\"reports\",\"snr_median\"\n",
            "\"2E0DLC\",\"EA8BFK\",14,1,0,10,-18\n",
            "\"2E0DLC\",\"EA8BFK\",14,1,1,10,NaN\n",
            "\"2E0DLC\",\"EA8BFK\",14,1,2,10,inf\n",
            "\"2E0DLC\",\"EA8BFK\",14,1,3,10,-12.5\n",
        );
        let rows = parse_daily(daily);
        let samples = rows
            .get(&("2E0DLC".to_string(), "EA8BFK".to_string(), 14))
            .expect("the path");
        assert_eq!(samples.len(), 2);
        assert!(samples.iter().all(|s| s.snr_median.is_finite()));
    }

    #[test]
    fn reads_quoted_call_signs_with_punctuation() {
        let fields = split_csv("\"WW0WWV\",\"W7YSB/R2\",7,100");
        assert_eq!(fields, vec!["WW0WWV", "W7YSB/R2", "7", "100"]);
    }

    #[test]
    fn parses_paths_by_column_name() {
        let paths = parse_paths(PATHS);
        assert_eq!(paths.len(), 2);
        assert_eq!(paths[0].tx, "WW0WWV");
        assert_eq!(paths[0].band, 7);
        assert!((paths[0].freq_mhz - 7.040052).abs() < 1e-9);
    }

    #[test]
    fn converts_dbm_to_watts() {
        let paths = parse_paths(PATHS);
        // 30 dBm is one watt; 23 dBm is the 200 mW most WSPR beacons run.
        assert!((paths[0].watts() - 1.0).abs() < 1e-9);
        assert!((paths[1].watts() - 0.199_526_231).abs() < 1e-6);
    }

    #[test]
    fn parses_hourly_into_a_sparse_day() {
        let hourly = parse_hourly(HOURLY);
        let key = ("2E0DLC".to_string(), "EA8BFK".to_string(), 14);
        let day = hourly.get(&key).expect("path present");
        assert_eq!(day[0], Some(-18.0));
        assert_eq!(day[23], Some(-12.5));
        // Hours with no reports stay empty rather than becoming zero.
        assert_eq!(day[5], None);
    }

    #[test]
    fn parses_daily_rows_per_path() {
        let text = concat!(
            "\"tx_sign\",\"rx_sign\",\"band\",\"day\",\"hour\",\"reports\",\"snr_median\"\n",
            "\"2E0DLC\",\"EA8BFK\",14,1,0,12,-18\n",
            "\"2E0DLC\",\"EA8BFK\",14,2,0,9,-21.5\n",
            "\"2E0DLC\",\"EA8BFK\",14,2,1,7,-15\n",
        );
        let daily = parse_daily(text);
        let key = ("2E0DLC".to_string(), "EA8BFK".to_string(), 14);
        let samples = daily.get(&key).expect("path present");
        assert_eq!(samples.len(), 3);
        assert_eq!(samples[1].day, 2);
        assert_eq!(samples[1].snr_median, -21.5);
        assert_eq!(samples[2].hour, 1);
    }

    #[test]
    fn the_ssn_table_covers_every_fetched_month() {
        for month in [
            "2015-03", "2019-06", "2019-12", "2022-09", "2024-12", "2025-03", "2025-06", "2025-07",
        ] {
            assert!(smoothed_ssn(month).is_some(), "no SSN for {month}");
        }
    }

    #[test]
    fn bandwidth_offset_matches_the_reference_bandwidth() {
        let expected = 10.0 * WSPR_BANDWIDTH_HZ.log10();
        assert!((WSPR_BANDWIDTH_OFFSET_DB - expected).abs() < 1e-9);
    }

    #[test]
    fn labels_bands_by_wavelength() {
        let paths = parse_paths(PATHS);
        assert_eq!(paths[0].label(), "WW0WWV>VE6JY 40m");
        assert_eq!(paths[1].label(), "2E0DLC>EA8BFK 20m");
    }
}
