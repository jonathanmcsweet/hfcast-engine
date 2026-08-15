//! Fits an effective sunspot number to a day of ionosonde readings.
//!
//! The engine's foF2 map holds two planes, sunspot number 0 and 100,
//! blended linearly (`redmap`). Predicted foF2 at a fixed place and hour
//! is therefore an exact line in the sunspot number, and two engine runs
//! per station — one per plane — give the whole line. A measured foF2
//! then has a closed-form per-sample answer: the sunspot number that
//! would have predicted it. One day's effective index is the median of
//! its samples, which is robust against a bad scaling or a disturbed
//! station.
//!
//! The deployable-skill question is answered by leaving the scored
//! station out of its own fit (`essn_excluding`): the index a field
//! device could really have had, from everyone else's soundings.
//!
//! This is also the principled replacement for the application's
//! `kpDerate` heuristic, which the app's roadmap flags as unpublished.

use std::collections::BTreeMap;

/// A per-sample slope below this (MHz per 100 sunspots) carries almost
/// no information about the index and would divide noise by nearly
/// nothing; such samples are left out of the fit.
pub const MIN_SLOPE: f64 = 0.05;

/// Days with fewer usable samples than this get no index: a fit from a
/// handful of readings would follow one station's noise.
pub const MIN_SAMPLES: usize = 12;

/// Fitted values are held to this range. The map is defined at 0 and
/// 100 and read linearly beyond; far outside, the line stops meaning
/// anything ionospheric.
pub const ESSN_RANGE: (f64, f64) = (-25.0, 300.0);

/// One per-sample solution: the sunspot number that would have
/// predicted the observed foF2 at this station-day-hour.
#[derive(Debug, Clone, PartialEq)]
pub struct Solution {
    pub station: String,
    pub day: u8,
    pub value: f64,
}

/// The solution for one observation on the line `f(s) = f0 + slope*s/100`.
/// None when the line is too flat to invert.
pub fn solve(observed: f64, f0: f64, f100: f64) -> Option<f64> {
    let slope = f100 - f0;
    (slope.abs() >= MIN_SLOPE).then(|| {
        let (low, high) = ESSN_RANGE;
        (100.0 * (observed - f0) / slope).clamp(low, high)
    })
}

/// The value of a plane-pair at a fitted index.
pub fn at(f0: f64, f100: f64, essn: f64) -> f64 {
    f0 + (f100 - f0) * essn / 100.0
}

/// One day's index with `station` left out of the fit, or None when the
/// remaining samples are too few. The median over samples, not over
/// stations: a station that reports all day carries the weight of its
/// evidence.
pub fn essn_excluding(solutions: &[Solution], day: u8, station: &str) -> Option<f64> {
    let mut others: Vec<f64> = solutions
        .iter()
        .filter(|s| s.day == day && s.station != station)
        .map(|s| s.value)
        .collect();
    (others.len() >= MIN_SAMPLES).then(|| crate::stats::median_in_place(&mut others))
}

/// Every day's all-station index, for the report: how the fitted index
/// moved through the month against the smoothed number the engine used.
pub fn essn_by_day(solutions: &[Solution]) -> BTreeMap<u8, f64> {
    let mut by_day: BTreeMap<u8, Vec<f64>> = BTreeMap::new();
    for s in solutions {
        by_day.entry(s.day).or_default().push(s.value);
    }
    by_day
        .into_iter()
        .filter(|(_, values)| values.len() >= MIN_SAMPLES)
        .map(|(day, mut values)| (day, crate::stats::median_in_place(&mut values)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_solution_inverts_the_line() {
        // f0 = 4, f100 = 6: 5 MHz observed means sunspot number 50.
        assert_eq!(solve(5.0, 4.0, 6.0), Some(50.0));
        assert_eq!(at(4.0, 6.0, 50.0), 5.0);
    }

    #[test]
    fn a_flat_line_gives_no_solution() {
        assert_eq!(solve(5.0, 4.0, 4.01), None);
    }

    #[test]
    fn solutions_stay_inside_the_defined_range() {
        // An observation far above the line clamps instead of exploding.
        assert_eq!(solve(60.0, 4.0, 6.0), Some(ESSN_RANGE.1));
    }

    fn day(values: &[(u8, &str, f64)]) -> Vec<Solution> {
        values
            .iter()
            .map(|(day, station, value)| Solution {
                station: (*station).to_string(),
                day: *day,
                value: *value,
            })
            .collect()
    }

    #[test]
    fn the_scored_station_is_left_out() {
        // Twelve samples from A at 100 and twelve from B at 60: scoring A
        // must see only B's, and the other way around.
        let mut samples = Vec::new();
        for _ in 0..12 {
            samples.push((1u8, "A", 100.0));
            samples.push((1u8, "B", 60.0));
        }
        let solutions = day(&samples);
        assert_eq!(essn_excluding(&solutions, 1, "A"), Some(60.0));
        assert_eq!(essn_excluding(&solutions, 1, "B"), Some(100.0));
    }

    #[test]
    fn too_few_remaining_samples_give_no_index() {
        let solutions = day(&[(1, "A", 80.0), (1, "B", 90.0)]);
        assert_eq!(essn_excluding(&solutions, 1, "A"), None);
    }

    #[test]
    fn the_daily_series_is_per_day() {
        let mut samples = Vec::new();
        for _ in 0..12 {
            samples.push((1u8, "A", 100.0));
            samples.push((2u8, "A", 80.0));
        }
        let series = essn_by_day(&day(&samples));
        assert_eq!(series.get(&1), Some(&100.0));
        assert_eq!(series.get(&2), Some(&80.0));
    }
}
