//! Reads GIRO FastChar month files fetched by `tools/fetch-giro.sh`.
//!
//! GIRO (the Global Ionospheric Radio Observatory) publishes scaled
//! ionosonde characteristics — foF2, foE, hmF2, MUF(3000) — per station at
//! a cadence of minutes. These are direct measurements of the ionosphere
//! over a known point, which makes them the ground truth the WSPR medians
//! cannot be: absolute, located, and in the model's own units (MHz, km).
//!
//! The response format is plain text: comment lines start with `#`, then
//! one row per sounding as timestamp, autoscaling confidence score, value,
//! and qualifying letters. The confidence floor and its reasoning are the
//! app's (`mobile/src/data/ionosonde.ts`): a score of zero means the scaler
//! had none, and such rows sit visibly wrong between their neighbours. A
//! score of 999 means a person scaled the ionogram by hand, which is better
//! than the autoscaler, not worse — it passes the floor.
//!
//! All GIRO measurements are CC-BY-NC-SA 4.0 and carry the Lowell GIRO
//! Data Center rules of the road; `tools/fetch-giro.sh` writes the
//! attribution beside the data.

use std::collections::BTreeMap;
use std::io;
use std::path::Path;

/// Rows scored below this are dropped rather than read as measurements.
/// Same value and reasoning as the app's reader.
pub const MIN_CONFIDENCE: f64 = 50.0;

/// A reading counts for an hour when it lies within this many minutes of
/// the top of that hour. Half the sounding interval of the slowest common
/// cadence, so an hour either has a nearby reading or honestly has none.
pub const WINDOW_MINUTES: i64 = 30;

/// The characteristics `tools/fetch-giro.sh` downloads, by DIDBase name.
/// MUFD is MUF(3000): the maximum usable frequency for a 3000 km hop.
pub const CHARACTERISTICS: [&str; 4] = ["foF2", "foE", "hmF2", "MUFD"];

/// One station from `tools/giro-stations.tsv`.
#[derive(Debug, Clone, PartialEq)]
pub struct StationMeta {
    /// URSI code, the API's identifier.
    pub ursi: String,
    pub name: String,
    pub lat: f64,
    /// Degrees east, -180..180 (the repository's convention; the service
    /// itself prints 0..360 in its headers).
    pub lon: f64,
}

/// Reads the station list: tab-separated `ursi name lat lon`, `#` comments.
pub fn load_stations(path: &Path) -> io::Result<Vec<StationMeta>> {
    Ok(parse_stations(&std::fs::read_to_string(path)?))
}

fn parse_stations(text: &str) -> Vec<StationMeta> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let fields: Vec<&str> = line.split('\t').map(str::trim).collect();
            let [ursi, name, lat, lon] = fields[..] else {
                return None;
            };
            Some(StationMeta {
                ursi: ursi.to_string(),
                name: name.to_string(),
                lat: lat.parse().ok()?,
                lon: lon.parse().ok()?,
            })
        })
        .collect()
}

/// One scaled sounding value.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// Minutes since 00:00 UT on day 1 of the month. One axis rather than
    /// (day, hour, minute), so "nearest to this hour" is a subtraction.
    pub minute_of_month: i64,
    pub value: f64,
    /// Autoscaling confidence score; 999 means scaled by hand.
    pub confidence: f64,
}

/// Parses one FastChar response body. Rows that are not usable data — bad
/// numbers, non-positive values, scores below the floor — are dropped here,
/// so every `Reading` that comes out counts.
pub fn parse(text: &str) -> Vec<Reading> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(read_row)
        .collect()
}

fn read_row(line: &str) -> Option<Reading> {
    let mut fields = line.split_whitespace();
    let stamp = fields.next()?;
    let confidence: f64 = fields.next()?.parse().ok()?;
    let value: f64 = fields.next()?.parse().ok()?;
    // NaN parses as a number, so finiteness is its own check.
    if !value.is_finite() || value <= 0.0 || confidence < MIN_CONFIDENCE {
        return None;
    }
    Some(Reading {
        minute_of_month: minute_of_month(stamp)?,
        value,
        confidence,
    })
}

/// `2025-06-01T00:08:16.000Z` → minutes since the month began. Seconds are
/// dropped: at a 30-minute window they cannot move a match.
fn minute_of_month(stamp: &str) -> Option<i64> {
    let (date, time) = stamp.split_once('T')?;
    let day: i64 = date.split('-').nth(2)?.parse().ok()?;
    let mut clock = time.split(':');
    let hour: i64 = clock.next()?.parse().ok()?;
    let minute: i64 = clock.next()?.parse().ok()?;
    if !(1..=31).contains(&day) || !(0..24).contains(&hour) || !(0..60).contains(&minute) {
        return None;
    }
    Some(((day - 1) * 24 + hour) * 60 + minute)
}

/// One station's month: its metadata and the readings per characteristic,
/// for the characteristic files that exist. A station with no files at all
/// is simply absent — coverage differs by year, and absence is ordinary.
#[derive(Debug)]
pub struct StationMonth {
    pub meta: StationMeta,
    pub chars: BTreeMap<String, Vec<Reading>>,
}

/// Loads every station's files under `<month-dir>/giro/<URSI>/<char>.txt`.
pub fn load_month(month_dir: &Path, stations: &[StationMeta]) -> Vec<StationMonth> {
    stations
        .iter()
        .filter_map(|meta| {
            let chars: BTreeMap<String, Vec<Reading>> = CHARACTERISTICS
                .iter()
                .filter_map(|name| {
                    let file = month_dir
                        .join("giro")
                        .join(&meta.ursi)
                        .join(format!("{name}.txt"));
                    let text = std::fs::read_to_string(file).ok()?;
                    let readings = parse(&text);
                    (!readings.is_empty()).then(|| (name.to_string(), readings))
                })
                .collect();
            (!chars.is_empty()).then(|| StationMonth {
                meta: meta.clone(),
                chars,
            })
        })
        .collect()
}

/// The reading nearest the top of the given hour, within the window, or
/// None when the hour honestly has none. On an exact tie in distance the
/// later reading wins, for the app's reason: the fresher sounding describes
/// the hour it leads into better than the staler one describes the hour it
/// trails.
pub fn at_hour(readings: &[Reading], day: u8, hour: u8) -> Option<f64> {
    let target = ((i64::from(day) - 1) * 24 + i64::from(hour)) * 60;
    readings
        .iter()
        .filter(|r| (r.minute_of_month - target).abs() <= WINDOW_MINUTES)
        .min_by_key(|r| {
            let delta = (r.minute_of_month - target).abs();
            // Two keys: distance first, then a preference for the later
            // side, encoded so that -delta of the later reading is smaller.
            (delta, -r.minute_of_month)
        })
        .map(|r| r.value)
}

#[cfg(test)]
mod tests {
    use super::*;

    // Real rows from JR055, 2025-06-01 (fetched 2026-08-12). The first is
    // the case the confidence floor exists for: score zero, 3.9 MHz between
    // neighbours near 5.
    const SAMPLE: &str = "\
# Time                    CS   foF2 QD
2025-06-01T00:03:16.000Z   0  3.900 //
2025-06-01T00:08:16.000Z  90  4.950 //
2025-06-01T00:13:16.000Z  60  5.125 //
2025-06-01T00:18:16.000Z  80  4.925 //
";

    #[test]
    fn drops_the_unscored_row_and_keeps_the_rest() {
        let readings = parse(SAMPLE);
        assert_eq!(readings.len(), 3);
        assert_eq!(readings[0].value, 4.950);
        assert_eq!(readings[0].minute_of_month, 8);
    }

    #[test]
    fn hand_scaled_rows_pass_the_floor() {
        let readings = parse("2025-06-15T12:00:00.000Z 999  6.100 //\n");
        assert_eq!(readings.len(), 1);
        assert_eq!(readings[0].confidence, 999.0);
    }

    #[test]
    fn unknown_scores_and_bad_values_are_dropped() {
        assert!(parse("2025-06-15T12:00:00.000Z  -1  6.100 //\n").is_empty());
        assert!(parse("2025-06-15T12:00:00.000Z  90  0.000 //\n").is_empty());
        assert!(parse("not-a-stamp  90  6.100 //\n").is_empty());
    }

    #[test]
    fn the_nearest_usable_reading_answers_for_the_hour() {
        let readings = parse(SAMPLE);
        // Hour 0 of day 1: the zero-scored 00:03 row is gone, so 00:08 is
        // nearest.
        assert_eq!(at_hour(&readings, 1, 0), Some(4.950));
        // Hour 1 of day 1: the latest reading is 00:18, 42 minutes away —
        // outside the window, so the hour has no answer.
        assert_eq!(at_hour(&readings, 1, 1), None);
    }

    #[test]
    fn an_exact_tie_goes_to_the_later_reading() {
        let text = "\
2025-06-02T11:50:00.000Z  80  5.000 //
2025-06-02T12:10:00.000Z  80  6.000 //
";
        assert_eq!(at_hour(&parse(text), 2, 12), Some(6.000));
    }

    #[test]
    fn the_station_list_reads_and_comments_are_skipped() {
        let stations = parse_stations(
            "# ursi\tname\tlat\tlon\nJR055\tJuliusruh\t54.6\t13.4\nBC840\tBoulder\t40.0\t-105.3\n",
        );
        assert_eq!(stations.len(), 2);
        assert_eq!(stations[1].ursi, "BC840");
        assert_eq!(stations[1].lon, -105.3);
    }

    #[test]
    fn a_minute_of_month_is_one_axis() {
        // Day 2 at 01:30 is 24h + 90min into the month.
        assert_eq!(
            minute_of_month("2025-06-02T01:30:59.000Z"),
            Some(24 * 60 + 90)
        );
        assert_eq!(minute_of_month("2025-06-00T01:30:00.000Z"), None);
    }
}
