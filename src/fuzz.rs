//! Generates valid input decks from a seed.
//!
//! The 96-case sweep in [`crate::sweep`] covers regimes chosen by hand.
//! That is the right shape for porting one method, and the wrong shape
//! for a library: a hand-built grid only holds combinations somebody
//! thought of, and the number of combinations grows with every method,
//! antenna and card the port adds. This builds decks from a seed
//! instead, so the port can be checked against inputs nobody chose and
//! any failure replays exactly from the seed that produced it.
//!
//! Every generated deck is a *valid* one. Degenerate inputs — a
//! zero-length path, a frequency of zero — are left out on purpose:
//! the reference engine's behaviour there is a separate question from
//! whether the port matches it, and mixing the two would make failures
//! hard to read.

use crate::deck::{AntennaChoice, DeckCase, FREQ_SLOTS};

/// Mean Earth radius, used only to place the second endpoint.
///
/// The engine computes its own path length from the two endpoints, so a
/// case's distance band describes how the case was chosen and is not a
/// claim about the distance the engine will compute.
const EARTH_RADIUS_KM: f64 = 6371.2;

/// Great-circle distance bands in km, with the reason each is in the set.
///
/// Cases cycle through these by index rather than drawing at random, so
/// a short run still covers every band.
pub const DISTANCE_BANDS: &[(f64, f64, &str)] = &[
    (20.0, 250.0, "very short, a single low-elevation hop"),
    (250.0, 2000.0, "short, one or two hops"),
    (2000.0, 7000.0, "the short model out to its 7000 km limit"),
    (7000.0, 10000.0, "the band where both models run and smooth"),
    (10000.0, 16000.0, "the long model alone"),
    (16000.0, 19800.0, "near antipodal"),
];

/// A seeded generator.
///
/// `splitmix64`, named rather than improvised, so a seed keeps meaning
/// the same thing if this file is ever revisited.
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// Uniform in `[0, 1)`.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// Uniform in `[lo, hi)`.
    pub fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (hi - lo) * self.unit()
    }

    /// Uniform integer in `[lo, hi]`, both ends included.
    pub fn int(&mut self, lo: i64, hi: i64) -> i64 {
        let span = (hi - lo + 1) as u64;
        lo + (self.next_u64() % span) as i64
    }

    /// True with probability `p`.
    pub fn chance(&mut self, p: f64) -> bool {
        self.unit() < p
    }
}

/// Rounds to the two decimals the input card carries.
///
/// Both engines must be given the value the card holds, not the value
/// before rounding, or they are being asked different questions.
fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

fn normalize_lon(lon: f64) -> f64 {
    let mut l = lon;
    while l > 180.0 {
        l -= 360.0;
    }
    while l <= -180.0 {
        l += 360.0;
    }
    l
}

/// The point `distance_km` from `(lat, lon)` along `bearing_deg`.
fn destination(lat: f64, lon: f64, bearing_deg: f64, distance_km: f64) -> (f64, f64) {
    let d = distance_km / EARTH_RADIUS_KM;
    let lat1 = lat.to_radians();
    let lon1 = lon.to_radians();
    let th = bearing_deg.to_radians();
    let lat2 = (lat1.sin() * d.cos() + lat1.cos() * d.sin() * th.cos()).asin();
    let lon2 = lon1
        + (th.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());
    (lat2.to_degrees(), normalize_lon(lon2.to_degrees()))
}

/// Which distance band case `index` draws from.
pub fn band_for(index: u64) -> (f64, f64, &'static str) {
    DISTANCE_BANDS[(index as usize) % DISTANCE_BANDS.len()]
}

/// Builds case `index`.
///
/// The index is both the seed and the band selector, so the whole
/// corpus is a function of the case number: `--seed 4217` reproduces
/// exactly the deck that failed.
pub fn fuzz_case(index: u64) -> DeckCase {
    // Offset the seed so case 0 is not the generator's degenerate state.
    let mut rng = Rng::new(index.wrapping_mul(0x2545_F491_4F6C_DD1D).wrapping_add(0x1234_5678));
    let (near, far, _) = band_for(index);

    // Endpoints: a random start, then a bearing and a distance from the
    // band. Drawing two independent points instead would put almost
    // every case between 5000 and 15000 km and never test the rest.
    let from_lat = round2(rng.range(-88.0, 88.0));
    let from_lon = round2(rng.range(-180.0, 180.0));
    let bearing = rng.range(0.0, 360.0);
    let distance = rng.range(near, far);
    let (to_lat_raw, to_lon_raw) = destination(from_lat, from_lon, bearing, distance);
    let to_lat = round2(to_lat_raw.clamp(-89.99, 89.99));
    let to_lon = round2(to_lon_raw);

    let count = rng.int(1, FREQ_SLOTS as i64) as usize;
    let mut freqs_mhz: Vec<f64> = (0..count).map(|_| round2(rng.range(1.6, 30.0))).collect();
    // Pin an edge sometimes: the lowest frequency the model accepts and
    // the highest, where the "above 30 MHz" output sentinels live.
    if rng.chance(0.15) {
        freqs_mhz[0] = 2.0;
    }
    if rng.chance(0.15) {
        let last = count - 1;
        freqs_mhz[last] = 30.0;
    }
    // Two slots at one frequency is legal and exercises the slot
    // bookkeeping rather than the physics.
    if count > 1 && rng.chance(0.08) {
        let dup = rng.int(1, count as i64 - 1) as usize;
        freqs_mhz[dup] = freqs_mhz[dup - 1];
    }
    freqs_mhz.sort_by(|a, b| a.partial_cmp(b).expect("generated frequencies are finite"));

    DeckCase {
        id: format!("fz{index:06}"),
        from_lat,
        from_lon,
        to_lat,
        to_lon,
        month: rng.int(1, 12) as u32,
        // NYEAR is CHARACTER*5 in the engine: a header label that no
        // computation reads, so varying it would only vary the label.
        year: 2026,
        ssn: rng.int(0, 250) as f64,
        // Log-uniform from 1 W to 500 kW: the interesting range is the
        // low end, where a linear draw would place almost nothing.
        watts: 10f64.powf(rng.range(0.0, 5.7)).round().max(1.0),
        required_snr_db: rng.int(0, 90) as f64,
        noise_dbw: rng.int(100, 170) as f64,
        sporadic_e: rng.chance(0.5),
        tx_antenna: pick_antenna(&mut rng),
        rx_antenna: pick_antenna(&mut rng),
        freqs_mhz,
    }
}

/// Antennas a case can draw, spanning every computable family: the
/// CCIR curtain and log-periodics, the gain tables, IONCAP and
/// HFMUFES patterns and the NOSC cone. Harris types are absent because
/// the reference cannot compute them either.
const ANTENNAS: &[&str] = &[
    "default/ccir.001",
    "default/ccir.010",
    "default/ccir.019",
    "default/swwhip.voa",
    "samples/sample.05",
    "samples/sample.10",
    "samples/sample.13",
    "samples/sample.14",
    "samples/sample.12",
    "samples/sample.21",
    "samples/sample.24",
    "samples/sample.27",
    "samples/sample.31",
    "samples/sample.34",
    "samples/sample.36",
    "samples/sample.43",
    "samples/sample.48",
];

/// Half the cases stay isotropic; the rest draw an antenna and a beam.
fn pick_antenna(rng: &mut Rng) -> Option<AntennaChoice> {
    if rng.chance(0.5) {
        return None;
    }
    let file = ANTENNAS[rng.int(0, ANTENNAS.len() as i64 - 1) as usize];
    Some(AntennaChoice {
        file: file.to_string(),
        design_freq: 0.0,
        beam_deg: (rng.int(0, 359) as f64 * 10.0).round() / 10.0,
    })
}

/// Cases `from` up to but not including `from + count`.
pub fn fuzz_cases(from: u64, count: u64) -> Vec<DeckCase> {
    (from..from + count).map(fuzz_case).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::build_deck;

    #[test]
    fn a_case_is_a_function_of_its_index() {
        assert_eq!(fuzz_case(7), fuzz_case(7));
        assert_ne!(fuzz_case(7), fuzz_case(8));
    }

    #[test]
    fn every_case_builds_a_valid_deck() {
        for case in fuzz_cases(0, 400) {
            build_deck(&case).unwrap_or_else(|e| panic!("{}: {e}", case.id));
        }
    }

    #[test]
    fn frequencies_are_ascending_and_fit_the_card() {
        for case in fuzz_cases(0, 400) {
            assert!(!case.freqs_mhz.is_empty());
            assert!(case.freqs_mhz.len() <= FREQ_SLOTS);
            assert!(case.freqs_mhz.windows(2).all(|w| w[0] <= w[1]), "{:?}", case.freqs_mhz);
            assert!(case.freqs_mhz.iter().all(|f| *f >= 1.6 && *f <= 30.0));
        }
    }

    #[test]
    fn the_first_cases_cover_every_distance_band() {
        let bands: Vec<&str> = (0..DISTANCE_BANDS.len() as u64)
            .map(|i| band_for(i).2)
            .collect();
        for (_, _, name) in DISTANCE_BANDS {
            assert!(bands.contains(name), "band {name} never selected");
        }
    }

    #[test]
    fn endpoints_land_in_their_band() {
        // Checked with the same radius the generator used, so this tests
        // the geometry rather than agreeing with the engine.
        for index in 0..300u64 {
            let c = fuzz_case(index);
            let (near, far, name) = band_for(index);
            let (lat1, lon1) = (c.from_lat.to_radians(), c.from_lon.to_radians());
            let (lat2, lon2) = (c.to_lat.to_radians(), c.to_lon.to_radians());
            let central = (lat1.sin() * lat2.sin()
                + lat1.cos() * lat2.cos() * (lon2 - lon1).cos())
            .clamp(-1.0, 1.0)
            .acos();
            let km = central * EARTH_RADIUS_KM;
            // Rounding the endpoints to the card's two decimals moves
            // them by up to about a kilometre.
            assert!(
                km >= near - 2.0 && km <= far + 2.0,
                "case {index} ({name}) came out at {km:.1} km, outside {near}-{far}"
            );
        }
    }

    #[test]
    fn power_stays_inside_the_cards_column() {
        for case in fuzz_cases(0, 400) {
            assert!(case.watts >= 1.0 && case.watts <= 500_000.0);
        }
    }
}
