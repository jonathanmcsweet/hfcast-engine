//! Scores predicted ionospheric characteristics against ionosonde truth.
//!
//! This is the instrument `docs/irtam.md` names as open work: WSPR medians
//! cap how much skill any model can show, because a handful of noisy
//! reports per path-hour is a blunt ruler. An ionosonde measures foF2,
//! foE, hmF2 and MUF(3000) directly, in the model's own units, over a
//! known point. Scoring against those separates "the model adds little"
//! from "the ruler cannot see what it adds".
//!
//! The method: for each station in the month's bundle, run the engine over
//! a ~111 km probe path centered on the station, so the single control
//! point is the station itself. `Task::Parameters` returns the unrounded
//! layer parameters per hour. Those are compared with the station's own
//! scaled soundings (`src/giro.rs`), per day and hour, for each model
//! column — climatology as shipped, and climatology with the day's IRTAM
//! foF2 map written over the coefficient file (`src/irtam.rs`).
//!
//! Climatology scores exactly zero day-to-day correlation by construction;
//! any other value means the harness is broken. This is the same silent-
//! failure guard `irtam_validate` uses.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::nowcast::api::LayerHour;
use crate::voacap::data;
use crate::{essn, giro, irtam, stats, wspr};

/// The probe geometry and the secant now live with the nowcast API, so
/// the harness and the deployable pipeline read the same engine table.
pub use crate::nowcast::api::{secant_factor, PROBE_OFFSET_DEG};

/// NVIS ground ranges scored, in km. Zero is straight up.
pub const NVIS_RANGES_KM: [f64; 3] = [0.0, 300.0, 600.0];

/// Band edges scored for NVIS usability, in MHz: 80 m, 60 m, 40 m, 30 m.
pub const NVIS_BANDS_MHZ: [f64; 4] = [3.6, 5.35, 7.1, 10.1];

/// Days with a storm block at or above this Kp count as storm days.
pub const STORM_KP: f64 = 5.0;

/// One scored (station, day, hour, characteristic) cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    pub station: String,
    pub day: u8,
    /// UT hour, 0 to 23.
    pub hour: u8,
    /// `foF2`, `foE`, `hmF2` or `MUFD` (MHz, MHz, km, MHz).
    pub characteristic: String,
    pub observed: f64,
    pub climatology: f64,
    /// The day-informed value: IRTAM foF2 for the frequency rows, IRTAM
    /// hmF2 for the height rows. None where the day had no readable map.
    pub irtam: Option<f64>,
    /// Height rows only: climatology's M(3000)F2 through the corrected
    /// Dudeney form instead of the engine's plain `1490/M - 176`. This
    /// column separates "the formula runs high" from "the map runs high".
    pub dudeney: Option<f64>,
    /// Frequency rows only: the model at the day's effective sunspot
    /// number, fitted from every OTHER station's readings that day
    /// (`src/essn.rs`). This is the deployable-skill column: no map of
    /// the scored station's own data stands behind it.
    pub essn: Option<f64>,
    /// The day's leave-this-station-out index itself — the input the
    /// essn column and the nowcast `Conditioning::Daily` consume.
    /// Cached so `sonde --engine nowcast` can replay the deployable
    /// API against the research columns.
    pub essn_index: Option<f64>,
}

/// The four characteristics one predicted hour carries, in the bundle's
/// names and units.
///
/// The engine's F2 working frequency (`f2z`) is the extraordinary-wave
/// value: the map's foF2 plus half the gyrofrequency (`TIMVAR` adds
/// `0.5*GYZ`). An ionosonde scales the ordinary wave, so the half
/// gyrofrequency comes back off before the comparison — without this the
/// whole column reads about 0.55 MHz high and the error is the magnetic
/// field, not the model. MUF(3000) is the ordinary-wave foF2 times the
/// M(3000)F2 factor, which is DIDBase's MUFD convention.
fn predicted_chars(layer: &LayerHour) -> [(&'static str, f64); 4] {
    let fof2 = layer.f2z - layer.fh2;
    [
        ("foF2", fof2),
        ("foE", layer.fe),
        ("hmF2", layer.h2),
        ("MUFD", fof2 * layer.m3000),
    ]
}

/// The IRTAM maps of one characteristic present in the bundle, per day.
fn irtam_days(
    month_dir: &Path,
    month: &str,
    characteristic: &str,
    parse: fn(&str) -> Result<irtam::IrtamMap, String>,
) -> BTreeMap<u8, irtam::IrtamMap> {
    let (year, mm) = month.split_once('-').unwrap_or((month, ""));
    (1..=31)
        .filter_map(|day| {
            let name = format!("IRTAM_{characteristic}_COEFFS_{year}{mm}{day:02}_234500.ASC");
            let text = std::fs::read_to_string(month_dir.join("irtam").join(name)).ok()?;
            parse(&text).ok().map(|map| (day, map))
        })
        .collect()
}

/// A root whose foF2 coefficients are this map's, via the overlay form.
/// The directory holds one synthesized `coeffs/fof2CCIR.daw`.
fn overlay_for(map: &irtam::IrtamMap, dir: &Path) -> Result<PathBuf, String> {
    let coeffs = dir.join("coeffs");
    std::fs::create_dir_all(&coeffs).map_err(|e| e.to_string())?;
    std::fs::write(coeffs.join("fof2CCIR.daw"), irtam::daw_file(map)).map_err(|e| e.to_string())?;
    Ok(data::overlay_root(dir))
}

/// Gathers every sample for one month bundle. Slow (hundreds of engine
/// runs), so `cache` short-circuits it; delete the cache file to regather.
pub fn gather(
    month_dir: &Path,
    stations_tsv: &Path,
    cache_dir: &Path,
) -> Result<(String, Vec<Sample>), String> {
    let month = month_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("month directory has no name")?
        .to_string();
    let cache = cache_dir.join(format!("{month}.sonde.csv"));
    if let Some(samples) = load_cache(&cache) {
        return Ok((month, samples));
    }

    let ssn = wspr::smoothed_ssn(&month).ok_or(format!("no smoothed SSN for {month}"))?;
    let month_number: u32 = month
        .split_once('-')
        .and_then(|(_, m)| m.parse().ok())
        .ok_or(format!("{month} is not YYYY-MM"))?;
    let stations = giro::load_stations(stations_tsv).map_err(|e| e.to_string())?;
    let observed = giro::load_month(month_dir, &stations);
    if observed.is_empty() {
        return Err(format!("{month}: no GIRO data; run tools/fetch-giro.sh"));
    }
    let fof2_maps = irtam_days(month_dir, &month, "foF2", irtam::parse_asc);
    let hmf2_maps = irtam_days(month_dir, &month, "hmF2", irtam::parse_asc_hmf2);

    // First pass: every station's predicted tables. Second pass needs
    // all of them at once, because a station's effective-index column is
    // fitted from every other station's readings.
    let mut tables = Vec::new();
    // A loop for the same reason as below: the overlay directory reuse
    // and the error path.
    for station in &observed {
        tables.push(station_tables(
            station,
            month_number,
            ssn,
            &month,
            cache_dir,
            &fof2_maps,
            &hmf2_maps,
        )?);
    }

    let solutions: Vec<essn::Solution> = observed
        .iter()
        .zip(&tables)
        .flat_map(|(station, t)| fof2_solutions(station, &t.plane0, &t.plane100))
        .collect();

    let samples: Vec<Sample> = observed
        .iter()
        .zip(&tables)
        .flat_map(|(station, t)| station_samples(station, t, &solutions))
        .collect();
    save_cache(&cache, &samples);
    Ok((month, samples))
}

/// Every predicted table one station needs: climatology at the smoothed
/// number, the two map planes for the index fit, and the per-day
/// assimilated maps.
struct StationTables {
    climatology: Vec<LayerHour>,
    plane0: Vec<LayerHour>,
    plane100: Vec<LayerHour>,
    irtam_by_day: BTreeMap<u8, Vec<LayerHour>>,
    heights_by_day: BTreeMap<u8, Vec<f64>>,
}

fn station_tables(
    station: &giro::StationMonth,
    month_number: u32,
    ssn: f64,
    month: &str,
    cache_dir: &Path,
    fof2_maps: &BTreeMap<u8, irtam::IrtamMap>,
    hmf2_maps: &BTreeMap<u8, irtam::IrtamMap>,
) -> Result<StationTables, String> {
    let probe = |root: &Path, at_ssn: f64| {
        crate::nowcast::api::probe_hours(
            root,
            station.meta.lat,
            station.meta.lon,
            month_number,
            at_ssn,
        )
    };
    let climatology = probe(&data::embedded_root(), ssn)?;
    let plane0 = probe(&data::embedded_root(), 0.0)?;
    let plane100 = probe(&data::embedded_root(), 100.0)?;
    let irtam_by_day: BTreeMap<u8, _> = fof2_maps
        .iter()
        .map(|(day, map)| {
            let dir = cache_dir.join(format!("sonde-overlay-{month}-{day:02}"));
            overlay_for(map, &dir).and_then(|root| probe(&root, ssn).map(|h| (*day, h)))
        })
        .collect::<Result<_, _>>()?;
    // The hmF2 map goes through the same foF2 slot: the engine's own
    // Jones-Gallet evaluator computes it at the station, and the
    // half-gyrofrequency the engine adds is the same one
    // `predicted_chars` takes back off, so the "foF2" value of this
    // run is the IRTAM height, unshifted.
    let heights_by_day: BTreeMap<u8, Vec<f64>> = hmf2_maps
        .iter()
        .map(|(day, map)| {
            let dir = cache_dir.join(format!("sonde-overlay-hmf2-{month}-{day:02}"));
            overlay_for(map, &dir).and_then(|root| {
                probe(&root, ssn).map(|hours| {
                    let heights = hours.iter().map(|layer| layer.f2z - layer.fh2).collect();
                    (*day, heights)
                })
            })
        })
        .collect::<Result<_, _>>()?;
    Ok(StationTables {
        climatology,
        plane0,
        plane100,
        irtam_by_day,
        heights_by_day,
    })
}

/// One station's per-sample index solutions, from its foF2 readings and
/// its two plane tables.
fn fof2_solutions(
    station: &giro::StationMonth,
    plane0: &[LayerHour],
    plane100: &[LayerHour],
) -> Vec<essn::Solution> {
    let Some(readings) = station.chars.get("foF2") else {
        return Vec::new();
    };
    (1..=31u8)
        .flat_map(|day| {
            (0..24u8).filter_map(move |hour| {
                let observed = giro::at_hour(readings, day, hour)?;
                let f0 = plane0[usize::from(hour)];
                let f100 = plane100[usize::from(hour)];
                essn::solve(observed, f0.f2z - f0.fh2, f100.f2z - f100.fh2).map(|value| {
                    essn::Solution {
                        station: station.meta.ursi.clone(),
                        day,
                        value,
                    }
                })
            })
        })
        .collect()
}

/// Joins one station's observations with the predicted hours.
fn station_samples(
    station: &giro::StationMonth,
    tables: &StationTables,
    solutions: &[essn::Solution],
) -> Vec<Sample> {
    // The day's leave-this-station-out index, once per day rather than
    // once per sample.
    let index_by_day: BTreeMap<u8, Option<f64>> = (1..=31u8)
        .map(|day| {
            (
                day,
                essn::essn_excluding(solutions, day, &station.meta.ursi),
            )
        })
        .collect();
    // Borrowed once so the move closures below copy the reference, not
    // the map.
    let index_by_day = &index_by_day;
    station
        .chars
        .iter()
        .flat_map(|(name, readings)| {
            (1..=31u8).flat_map(move |day| {
                let index = index_by_day.get(&day).copied().flatten();
                (0..24u8).filter_map(move |hour| {
                    let observed = giro::at_hour(readings, day, hour)?;
                    let slot = predicted_chars(&tables.climatology[usize::from(hour)]);
                    Some(Sample {
                        station: station.meta.ursi.clone(),
                        day,
                        hour,
                        characteristic: name.clone(),
                        observed,
                        climatology: value_of(&slot, name)?,
                        irtam: irtam_of(tables, name, day, hour),
                        dudeney: (name == "hmF2").then(|| dudeney_of(&slot)).flatten(),
                        essn: index.and_then(|value| essn_value(tables, name, hour, value)),
                        essn_index: index,
                    })
                })
            })
        })
        .collect()
}

/// The month's all-station daily index series, fitted from the bundle's
/// GIRO soundings: what a deployed device with the sounding feed really
/// has per day. Two engine runs per station (the two map planes); days
/// with too few usable solutions are absent, as `essn::essn_by_day`
/// leaves them.
pub fn essn_series(month_dir: &Path, stations_tsv: &Path) -> Result<BTreeMap<u8, f64>, String> {
    let month = month_dir
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("month directory has no name")?;
    let month_number: u32 = month
        .split_once('-')
        .and_then(|(_, m)| m.parse().ok())
        .ok_or(format!("{month} is not YYYY-MM"))?;
    let stations = giro::load_stations(stations_tsv).map_err(|e| e.to_string())?;
    let observed = giro::load_month(month_dir, &stations);
    if observed.is_empty() {
        return Err(format!("{month}: no GIRO data; run tools/fetch-giro.sh"));
    }
    let mut solutions = Vec::new();
    // A loop for the error path: one station failing to predict is a
    // real fault, not a station to skip.
    for station in &observed {
        let probe = |ssn: f64| {
            crate::nowcast::api::probe_hours(
                &data::embedded_root(),
                station.meta.lat,
                station.meta.lon,
                month_number,
                ssn,
            )
        };
        let (plane0, plane100) = (probe(0.0)?, probe(100.0)?);
        solutions.extend(fof2_solutions(station, &plane0, &plane100));
    }
    Ok(essn::essn_by_day(&solutions))
}

/// The day-informed column value: the IRTAM hmF2 evaluation for the
/// height rows, the day's IRTAM foF2 run for the rest. None where the
/// day had no readable map.
fn irtam_of(tables: &StationTables, name: &str, day: u8, hour: u8) -> Option<f64> {
    if name == "hmF2" {
        tables
            .heights_by_day
            .get(&day)
            .and_then(|heights| heights.get(usize::from(hour)).copied())
    } else {
        tables
            .irtam_by_day
            .get(&day)
            .and_then(|hours| value_of(&predicted_chars(&hours[usize::from(hour)]), name))
    }
}

/// The model at the fitted index, for the frequency rows. foF2 reads its
/// own line; MUF(3000) is the product of the foF2 line and the factor's
/// line, each linear in the index. Heights and foE stay out: the index
/// is fitted to foF2 and would only pretend to inform them.
fn essn_value(tables: &StationTables, name: &str, hour: u8, index: f64) -> Option<f64> {
    let p0 = tables.plane0[usize::from(hour)];
    let p100 = tables.plane100[usize::from(hour)];
    let (f0, f100) = (p0.f2z - p0.fh2, p100.f2z - p100.fh2);
    match name {
        "foF2" => Some(essn::at(f0, f100, index)),
        "MUFD" => Some(essn::at(f0, f100, index) * essn::at(p0.m3000, p100.m3000, index)),
        _ => None,
    }
}

/// Climatology's own M(3000)F2 and frequencies through the corrected
/// Dudeney form. The factor comes back out of the MUFD column, which is
/// the ordinary-wave foF2 times it.
fn dudeney_of(slot: &[(&'static str, f64); 4]) -> Option<f64> {
    let fof2 = value_of(slot, "foF2")?;
    let foe = value_of(slot, "foE")?;
    let m3000 = value_of(slot, "MUFD")? / fof2;
    (fof2 > 0.0).then(|| irtam::hmf2_dudeney(m3000, fof2, foe))
}

fn value_of(slot: &[(&'static str, f64); 4], name: &str) -> Option<f64> {
    slot.iter()
        .find(|(char_name, _)| *char_name == name)
        .map(|(_, value)| *value)
}

// ---- cache ----------------------------------------------------------

const CACHE_HEADER: &str =
    "station,day,hour,char,observed,climatology,irtam,dudeney,essn,essn_index";

fn save_cache(path: &Path, samples: &[Sample]) {
    let column = |value: Option<f64>| value.map(|v| format!("{v:.4}")).unwrap_or_default();
    let body = samples
        .iter()
        .map(|s| {
            format!(
                "{},{},{},{},{:.4},{:.4},{},{},{},{}",
                s.station,
                s.day,
                s.hour,
                s.characteristic,
                s.observed,
                s.climatology,
                column(s.irtam),
                column(s.dudeney),
                column(s.essn),
                column(s.essn_index)
            )
        })
        .fold(format!("{CACHE_HEADER}\n"), |mut out, line| {
            out.push_str(&line);
            out.push('\n');
            out
        });
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Err(e) = std::fs::write(path, body) {
        eprintln!("cache not written to {}: {e}", path.display());
    }
}

fn load_cache(path: &Path) -> Option<Vec<Sample>> {
    let text = std::fs::read_to_string(path).ok()?;
    let mut lines = text.lines();
    if lines.next() != Some(CACHE_HEADER) {
        return None;
    }
    lines.map(parse_cache_line).collect()
}

fn parse_cache_line(line: &str) -> Option<Sample> {
    let fields: Vec<&str> = line.split(',').collect();
    let [station, day, hour, characteristic, observed, climatology, irtam, dudeney, essn, essn_index] =
        fields[..]
    else {
        return None;
    };
    let optional = |field: &str| -> Option<Option<f64>> {
        if field.is_empty() {
            Some(None)
        } else {
            field.parse().ok().map(Some)
        }
    };
    Some(Sample {
        station: station.to_string(),
        day: day.parse().ok()?,
        hour: hour.parse().ok()?,
        characteristic: characteristic.to_string(),
        observed: observed.parse().ok()?,
        climatology: climatology.parse().ok()?,
        irtam: optional(irtam)?,
        dudeney: optional(dudeney)?,
        essn: optional(essn)?,
        essn_index: optional(essn_index)?,
    })
}

// ---- metrics --------------------------------------------------------

/// Bias, absolute error and RMS of (model - observed) for one selection.
pub fn errors(pairs: &[(f64, f64)]) -> Option<(f64, f64, f64)> {
    if pairs.is_empty() {
        return None;
    }
    let residuals: Vec<f64> = pairs
        .iter()
        .map(|(model, observed)| model - observed)
        .collect();
    let absolute: Vec<f64> = residuals.iter().map(|r| r.abs()).collect();
    Some((
        stats::median(&residuals),
        stats::median(&absolute),
        stats::rms(&residuals),
    ))
}

/// Day-to-day skill: the correlation between model and observed deviations
/// from each (station, hour) cell's own monthly median, pooled over cells
/// with at least five days. A model that never varies by day — climatology —
/// has zero variance and reports exactly 0.000, which is the guard.
pub fn day_to_day(samples: &[&Sample], pick: &dyn Fn(&Sample) -> Option<f64>) -> (f64, usize) {
    let mut cells: BTreeMap<(String, u8), Vec<(f64, f64)>> = BTreeMap::new();
    for sample in samples {
        if let Some(model) = pick(sample) {
            cells
                .entry((sample.station.clone(), sample.hour))
                .or_default()
                .push((sample.observed, model));
        }
    }
    let (mut observed_dev, mut model_dev) = (Vec::new(), Vec::new());
    for days in cells.into_values().filter(|days| days.len() >= 5) {
        let observed_median = stats::median(&days.iter().map(|d| d.0).collect::<Vec<_>>());
        let model_median = stats::median(&days.iter().map(|d| d.1).collect::<Vec<_>>());
        for (observed, model) in days {
            observed_dev.push(observed - observed_median);
            model_dev.push(model - model_median);
        }
    }
    let pairs = observed_dev.len();
    let correlation = stats::correlation(&observed_dev, &model_dev).unwrap_or(0.0);
    (correlation, pairs)
}

/// The NVIS usability confusion counts for one model at one band and range:
/// (both usable, both unusable, model said usable but it was not, model
/// missed a usable hour).
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct BandCalls {
    pub hits: u32,
    pub correct_rejections: u32,
    pub false_alarms: u32,
    pub misses: u32,
}

impl BandCalls {
    pub fn count(&mut self, observed_muf: f64, model_muf: f64, band_mhz: f64) {
        match (band_mhz <= observed_muf, band_mhz <= model_muf) {
            (true, true) => self.hits += 1,
            (false, false) => self.correct_rejections += 1,
            (false, true) => self.false_alarms += 1,
            (true, false) => self.misses += 1,
        }
    }

    pub fn accuracy(&self) -> f64 {
        let total = self.hits + self.correct_rejections + self.false_alarms + self.misses;
        if total == 0 {
            return f64::NAN;
        }
        f64::from(self.hits + self.correct_rejections) / f64::from(total)
    }
}

/// One (station, day, hour) with both foF2 and hmF2 present, for NVIS
/// arithmetic. The identity fields let a report condition a cell on its
/// own storm state.
#[derive(Debug, Clone, PartialEq)]
pub struct NvisCell {
    pub station: String,
    pub day: u8,
    pub hour: u8,
    pub observed_fof2: f64,
    pub observed_hmf2: f64,
    pub climatology_fof2: f64,
    pub climatology_hmf2: f64,
    pub irtam_fof2: Option<f64>,
    pub irtam_hmf2: Option<f64>,
    pub dudeney_hmf2: Option<f64>,
    pub essn_fof2: Option<f64>,
}

/// Joins the foF2 and hmF2 samples into NVIS cells.
pub fn nvis_cells(samples: &[Sample]) -> Vec<NvisCell> {
    let mut fof2: BTreeMap<(String, u8, u8), &Sample> = BTreeMap::new();
    let mut hmf2: BTreeMap<(String, u8, u8), &Sample> = BTreeMap::new();
    for sample in samples {
        let key = (sample.station.clone(), sample.day, sample.hour);
        match sample.characteristic.as_str() {
            "foF2" => {
                fof2.insert(key, sample);
            }
            "hmF2" => {
                hmf2.insert(key, sample);
            }
            _ => {}
        }
    }
    fof2.iter()
        .filter_map(|(key, f)| {
            let h = hmf2.get(key)?;
            Some(NvisCell {
                station: key.0.clone(),
                day: key.1,
                hour: key.2,
                observed_fof2: f.observed,
                observed_hmf2: h.observed,
                climatology_fof2: f.climatology,
                climatology_hmf2: h.climatology,
                irtam_fof2: f.irtam,
                irtam_hmf2: h.irtam,
                dudeney_hmf2: h.dudeney,
                essn_fof2: f.essn,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn band_calls_land_in_the_right_boxes() {
        let mut calls = BandCalls::default();
        calls.count(8.0, 8.5, 7.1); // both say 40 m works
        calls.count(6.0, 8.5, 7.1); // model said yes, the sky said no
        calls.count(8.0, 6.5, 7.1); // model missed an open band
        calls.count(6.0, 6.5, 7.1); // both say closed
        assert_eq!(
            calls,
            BandCalls {
                hits: 1,
                correct_rejections: 1,
                false_alarms: 1,
                misses: 1
            }
        );
        assert!((calls.accuracy() - 0.5).abs() < 1e-12);
    }

    #[test]
    fn climatology_day_to_day_is_exactly_zero() {
        // Ten days at one station-hour: observed varies, the model repeats
        // one number. The guard: this must print 0.000, never a residue.
        let samples: Vec<Sample> = (1..=10u8)
            .map(|day| Sample {
                station: "TEST".into(),
                day,
                hour: 12,
                characteristic: "foF2".into(),
                observed: 5.0 + f64::from(day) * 0.1,
                climatology: 6.0,
                irtam: None,
                dudeney: None,
                essn: None,
                essn_index: None,
            })
            .collect();
        let refs: Vec<&Sample> = samples.iter().collect();
        let (correlation, pairs) = day_to_day(&refs, &|s| Some(s.climatology));
        assert_eq!(pairs, 10);
        assert_eq!(correlation, 0.0);
    }

    #[test]
    fn a_varying_model_correlates_when_it_tracks_the_sky() {
        let samples: Vec<Sample> = (1..=10u8)
            .map(|day| Sample {
                station: "TEST".into(),
                day,
                hour: 12,
                characteristic: "foF2".into(),
                observed: 5.0 + f64::from(day) * 0.1,
                climatology: 6.0,
                irtam: Some(4.0 + f64::from(day) * 0.1),
                dudeney: None,
                essn: None,
                essn_index: None,
            })
            .collect();
        let refs: Vec<&Sample> = samples.iter().collect();
        let (correlation, _) = day_to_day(&refs, &|s| s.irtam);
        assert!((correlation - 1.0).abs() < 1e-9);
    }

    #[test]
    fn the_cache_round_trips() {
        let samples = vec![
            Sample {
                station: "JR055".into(),
                day: 1,
                hour: 0,
                characteristic: "foF2".into(),
                observed: 4.95,
                climatology: 5.2,
                irtam: Some(4.8),
                dudeney: None,
                essn: Some(5.1),
                essn_index: Some(52.5),
            },
            Sample {
                station: "JR055".into(),
                day: 2,
                hour: 23,
                characteristic: "hmF2".into(),
                observed: 310.0,
                climatology: 295.5,
                irtam: Some(301.0),
                dudeney: Some(288.2),
                essn: None,
                essn_index: None,
            },
        ];
        let dir = std::env::temp_dir().join(format!("sonde-cache-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        let path = dir.join("t.sonde.csv");
        save_cache(&path, &samples);
        assert_eq!(load_cache(&path), Some(samples));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn errors_report_bias_and_spread() {
        let (bias, mae, rms) = errors(&[(6.0, 5.0), (7.0, 5.0), (5.0, 5.0)]).expect("pairs");
        assert_eq!(bias, 1.0);
        assert_eq!(mae, 1.0);
        assert!(rms > 1.0);
    }
}
