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

fn number(fields: &[String], index: &HashMap<String, usize>, name: &str) -> Option<f64> {
    get(fields, index, name)?.parse().ok()
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
