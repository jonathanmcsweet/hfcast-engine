//! Reads the GFZ Potsdam daily geomagnetic index file.
//!
//! The file `Kp_ap_Ap_SN_F107_since_1932.txt` (from <https://kp.gfz.de>) has
//! one line per UT day: eight Kp values (one per 3-hour block), eight ap
//! values, the daily Ap, sunspot number and solar flux. `tools/fetch-kp.sh`
//! downloads it into `data/`.
//!
//! Kp measures how disturbed the Earth's magnetic field is: below 3 is quiet,
//! 5 and above is a geomagnetic storm. The storm analysis tags each measured
//! day-hour with the Kp of its own 3-hour block, so the question "are storm
//! hours worse than the model claims" can be asked directly.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// One UT day of index values.
#[derive(Debug, Clone, PartialEq)]
pub struct DayGeomag {
    /// Kp per 3-hour block: index 0 covers 00-03 UT, index 7 covers 21-24 UT.
    pub kp: [f64; 8],
    /// Daily equivalent amplitude, the whole-day summary.
    pub ap: f64,
}

impl DayGeomag {
    /// Kp of the 3-hour block containing the given UT hour.
    pub fn kp_at_hour(&self, hour: u8) -> f64 {
        self.kp[usize::from(hour.min(23)) / 3]
    }
}

/// The whole file, keyed by the file's own day counter (days since
/// 1932-01-01), so adjacent calendar days are adjacent keys even across a
/// month boundary.
#[derive(Debug, Default)]
pub struct GeomagTable {
    days: BTreeMap<i64, DayGeomag>,
    index: BTreeMap<(u32, u32, u8), i64>,
}

impl GeomagTable {
    pub fn get(&self, year: u32, month: u32, day: u8) -> Option<&DayGeomag> {
        self.days.get(self.index.get(&(year, month, day))?)
    }

    /// Highest Kp over the block containing the hour and the preceding
    /// `hours_back` hours. Ionospheric storm effects outlast the disturbance
    /// itself, so "was there a storm recently" needs a lookback.
    pub fn kp_max_lookback(
        &self,
        year: u32,
        month: u32,
        day: u8,
        hour: u8,
        hours_back: u8,
    ) -> Option<f64> {
        let day_key = *self.index.get(&(year, month, day))?;
        let block = i64::from(hour.min(23)) / 3;
        let blocks_back = (i64::from(hours_back) + 2) / 3;
        let mut max = f64::NEG_INFINITY;
        for offset in 0..=blocks_back {
            let b = block - offset;
            let (key, block_of_day) = (day_key + b.div_euclid(8), b.rem_euclid(8));
            let d = self.days.get(&key)?;
            max = max.max(d.kp[block_of_day as usize]);
        }
        Some(max)
    }

    pub fn is_empty(&self) -> bool {
        self.days.is_empty()
    }
}

/// Parses the GFZ file. Lines with missing values (marked -1) are skipped;
/// header lines start with `#`.
pub fn parse(text: &str) -> GeomagTable {
    let mut table = GeomagTable::default();
    for line in text.lines() {
        if line.starts_with('#') {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        // year month day days days_m bsr db kp1..kp8 ap1..ap8 ap sn f107o f107a d
        if fields.len() < 24 {
            continue;
        }
        let parsed = (
            fields[0].parse::<u32>(),
            fields[1].parse::<u32>(),
            fields[2].parse::<u8>(),
            fields[3].parse::<i64>(),
            fields[23].parse::<f64>(),
        );
        let (Ok(year), Ok(month), Ok(day), Ok(day_key), Ok(ap)) = parsed else {
            continue;
        };
        let kp_fields: Vec<f64> = fields[7..15]
            .iter()
            .filter_map(|f| f.parse().ok())
            .collect();
        let Ok(kp) = <[f64; 8]>::try_from(kp_fields) else {
            continue;
        };
        if ap < 0.0 || kp.iter().any(|k| *k < 0.0) {
            continue;
        }
        table.days.insert(day_key, DayGeomag { kp, ap });
        table.index.insert((year, month, day), day_key);
    }
    table
}

pub fn load(path: &Path) -> io::Result<GeomagTable> {
    Ok(parse(&std::fs::read_to_string(path)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    // Two real lines from the file: the 2025-06-01 storm and the quiet day
    // after it ended, plus a month boundary pair.
    const SAMPLE: &str = "\
# header line
2025 05 31 34119 34119.5 2616  1  1.667  2.000  2.333  2.667  1.667  2.000  4.333  5.333    6    7    9   12    6    7   32   48   16 118    132.8    136.7 2
2025 06 01 34120 34120.5 2616  2  5.000  5.667  7.667  7.333  7.333  5.000  6.667  3.667   48   67  179  154  154   48  111   22    98 124    150.3    154.6 2
2025 06 04 34123 34123.5 2616  5  2.333  1.667  1.333  1.000  0.667  1.333  2.667  2.000    9    6    5    4    3    5   12    7    6  77    128.5    132.2 2
1932 01 01     0     0.5  818 15 -1.000  1.000  1.667  2.000  2.333  2.667  2.000  1.667   -1    4    6    7    9   12    7    6    6 -1     -1.0     -1.0 2
";

    #[test]
    fn parses_real_lines_and_finds_the_block() {
        let table = parse(SAMPLE);
        let storm = table.get(2025, 6, 1).expect("storm day present");
        assert_eq!(storm.ap, 98.0);
        // Hour 7 falls in block 06-09 UT, the third block.
        assert!((storm.kp_at_hour(7) - 7.667).abs() < 1e-9);
        let quiet = table.get(2025, 6, 4).expect("quiet day present");
        assert_eq!(quiet.ap, 6.0);
    }

    #[test]
    fn skips_days_with_missing_values() {
        let table = parse(SAMPLE);
        assert!(table.get(1932, 1, 1).is_none());
    }

    #[test]
    fn lookback_crosses_the_day_boundary() {
        let table = parse(SAMPLE);
        // 2025-06-01 02 UT, looking back 24 hours, must see the previous
        // day's evening blocks (Kp up to 5.333).
        let max = table
            .kp_max_lookback(2025, 6, 1, 2, 24)
            .expect("both days present");
        assert!((max - 5.333).abs() < 1e-9);
        // Without lookback it is just the current block.
        let now = table.kp_max_lookback(2025, 6, 1, 2, 0).expect("present");
        assert!((now - 5.0).abs() < 1e-9);
    }

    #[test]
    fn lookback_refuses_when_history_is_missing() {
        let table = parse(SAMPLE);
        // 2025-06-04 with a 24-hour lookback needs 2025-06-03, which is not
        // in the sample; a partial answer would misclassify, so none is right.
        assert_eq!(table.kp_max_lookback(2025, 6, 4, 12, 24), None);
    }
}
