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

use crate::deck::{AntennaChoice, DeckCase, Edp, EfVar, EsVar, FREQ_SLOTS};

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
    let lon2 = lon1 + (th.sin() * d.sin() * lat1.cos()).atan2(d.cos() - lat1.sin() * lat2.sin());
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
    let mut rng = Rng::new(
        index
            .wrapping_mul(0x2545_F491_4F6C_DD1D)
            .wrapping_add(0x1234_5678),
    );
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

    let ionosphere = pick_ionosphere(&mut rng);
    DeckCase {
        id: format!("fz{index:06}"),
        rx_label: "sweep".to_string(),
        method: 30,
        ursi: false,
        fprob: None,
        botlines: None,
        toplines: None,
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
        outgraph: pick_outgraph(&mut rng),
        integrate: pick_integrate(&mut rng),
        comment: pick_comment(&mut rng),
        extra_cards: pick_extra_cards(&mut rng),
        krun: ionosphere.0,
        efvar: ionosphere.1,
        esvar: ionosphere.2,
        edp: pick_edp(&mut rng),
        tx_antennas: pick_antennas(&mut rng, 1),
        rx_antennas: pick_antennas(&mut rng, 2),
        freqs_mhz,
    }
}

/// The `EXECUTE` card's `KRUN` field with the `EFVAR` and `ESVAR` cards
/// that make it meaningful, drawn together because `KRUN` on its own
/// leaves the `blkdat` presets standing and those describe no
/// ionosphere at all.
///
/// Every control point gets a card whatever the path's own count, which
/// is what the reference stores.
fn pick_ionosphere(rng: &mut Rng) -> (i32, Vec<EfVar>, Vec<EsVar>) {
    // The cards carry two decimals for a frequency and one for a
    // height, so the case has to hold what the card can say.
    let f2 = |v: f64| (v * 100.0).round() / 100.0;
    let f1 = |v: f64| (v * 10.0).round() / 10.0;
    if !rng.chance(0.12) {
        return (0, Vec::new(), Vec::new());
    }
    let krun = rng.int(1, 3) as i32;
    let efvar = (1..=5)
        .map(|area| {
            // An F1 layer within 0.2 MHz of the E layer is removed by
            // IONSET, so draw one that is either clearly there or gone.
            let fe = rng.range(0.5, 4.0);
            let f1v = if rng.chance(0.3) {
                0.0
            } else {
                rng.range(fe + 1.0, fe + 3.0)
            };
            let f2v = rng.range(f1v.max(fe) + 1.5, f1v.max(fe) + 10.0);
            EfVar {
                area,
                fi: [f2(fe), f2(f1v), f2(f2v)],
                yi: [
                    f1(rng.range(15.0, 25.0)),
                    f1(rng.range(20.0, 60.0)),
                    f1(rng.range(60.0, 120.0)),
                ],
                hi: [
                    f1(rng.range(100.0, 120.0)),
                    f1(rng.range(180.0, 250.0)),
                    f1(rng.range(250.0, 400.0)),
                ],
            }
        })
        .collect();
    let esvar = (1..=5)
        .map(|area| {
            let low = rng.range(0.5, 3.0);
            let med = low + rng.range(0.5, 3.0);
            EsVar {
                area,
                fs: [f2(low), f2(med), f2(med + rng.range(0.5, 6.0))],
                hs: f1(rng.range(100.0, 120.0)),
            }
        })
        .collect();
    (krun, efvar, esvar)
}

/// An `EDP` card and its profile: 50 rising true heights and the plasma
/// frequency squared at each. With one, `LECDEN` leaves the profile
/// alone for every sample area.
fn pick_edp(rng: &mut Rng) -> Option<Edp> {
    if !rng.chance(0.06) {
        return None;
    }
    let mut htr = [0.0f64; 50];
    let mut fnsq = [0.0f64; 50];
    let round = |v: f64| (v * 10.0).round() / 10.0;
    let mut h = rng.range(65.0, 75.0);
    let mut f = 0.0;
    for i in 0..50 {
        htr[i] = round(h);
        fnsq[i] = round(f);
        h += rng.range(4.0, 14.0);
        f += rng.range(0.1, 4.0);
    }
    Some(Edp {
        area: rng.int(1, 3) as i32,
        htr,
        fnsq,
    })
}

/// Cards that reach no computation, drawn so a run carrying one can be
/// compared against the reference. `FREEFORM` sets `ITYPE` and `ANTOUT`
/// sets `IANTOU`; nothing reads either.
fn pick_extra_cards(rng: &mut Rng) -> Vec<String> {
    let mut cards = Vec::new();
    if rng.chance(0.15) {
        cards.push(if rng.chance(0.5) {
            "FREEFORM  ON".to_string()
        } else {
            "FREEFORM  OFF".to_string()
        });
    }
    if rng.chance(0.15) {
        cards.push(if rng.chance(0.5) {
            "ANTOUT    ON".to_string()
        } else {
            "ANTOUT    OFF".to_string()
        });
    }
    if rng.chance(0.15) {
        // Control point 1 given values nothing like the path's, to make
        // any surviving field visible. `GEOM`, `MAGVAR`, `GEOTIM` and
        // `SIGDIS` overwrite every one of them.
        cards.push(
            "SAMPLE        145.00N    10.00E    40.00N     500. 1.0012.0030.00 0.10 0.00 4.00"
                .to_string(),
        );
    }
    cards
}

/// A sixth of the cases carry a `COMMENT` card. Half of those begin
/// with `GROUP`, which is the spelling the listing moves to the end.
fn pick_comment(rng: &mut Rng) -> Option<String> {
    if !rng.chance(0.17) {
        return None;
    }
    Some(if rng.chance(0.5) {
        format!("GROUP {} test", rng.int(1, 99))
    } else {
        format!("case note {}", rng.int(1, 99))
    })
}

/// A fifth of the cases carry an `INTEGRATE` card, which switches the
/// layer heights to the fast parabolic path. Negative values are drawn
/// too, because a negative one leaves the default path in place.
fn pick_integrate(rng: &mut Rng) -> Option<i32> {
    if !rng.chance(0.2) {
        return None;
    }
    Some(rng.int(-2, 3) as i32)
}

/// A quarter of the cases carry an `OUTGRAPH` card, drawn from the
/// whole card-method range plus the values the driver has to reject:
/// zero, a negative number, and a number past the table.
fn pick_outgraph(rng: &mut Rng) -> Option<Vec<i32>> {
    if !rng.chance(0.25) {
        return None;
    }
    let count = rng.int(1, 12) as usize;
    Some(
        (0..count)
            .map(|_| match rng.int(0, 9) {
                0 => 0,
                1 => -(rng.int(1, 29) as i32),
                2 => rng.int(30, 40) as i32,
                _ => rng.int(1, 29) as i32,
            })
            .collect(),
    )
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

/// The `ANTENNA` cards for one end: half the ends stay isotropic, and
/// the rest draw one to three cards.
///
/// Several cards per end adds no physics — each card's table is computed
/// the way one card's always was — so what these draws have to exercise
/// is `GAIN`'s search over the cards. The three arrangements that search
/// can meet are all drawn here: ranges that meet exactly, ranges that
/// leave a gap so some frequency has no antenna and takes zero gain, and
/// ranges that overlap so the card's position decides which one answers.
///
/// A transmit card also carries its own power, which `PWRDB` looks up per
/// frequency, so cards at that end sometimes get different powers. On a
/// receive card the same column is a gain that replaces the design
/// frequency.
fn pick_antennas(rng: &mut Rng, iat: i32) -> Vec<AntennaChoice> {
    if rng.chance(0.5) {
        return Vec::new();
    }
    let draw = |rng: &mut Rng| -> AntennaChoice {
        let file = ANTENNAS[rng.int(0, ANTENNAS.len() as i64 - 1) as usize];
        AntennaChoice::whole_band(file, (rng.int(0, 3590) as f64) / 10.0)
    };
    let count = match rng.int(1, 100) {
        n if n <= 60 => 1,
        n if n <= 85 => 2,
        _ => 3,
    };
    let mut cards: Vec<AntennaChoice> = (0..count).map(|_| draw(rng)).collect();

    // Split 2 to 30 MHz at `count - 1` interior points, then move the
    // later cards' lower edge to make the bands meet, leave a gap or
    // overlap.
    let mut splits: Vec<i32> = (1..count).map(|_| rng.int(4, 27) as i32).collect();
    splits.sort_unstable();
    let shift = match rng.int(1, 3) {
        1 => 1,                       // the bands meet
        2 => rng.int(2, 4) as i32,    // a gap: some frequency has no card
        _ => -(rng.int(1, 3) as i32), // an overlap: the first card wins
    };
    let mut lo = 2;
    for (i, card) in cards.iter_mut().enumerate() {
        card.min_freq = lo;
        card.max_freq = splits.get(i).copied().unwrap_or(30);
        lo = (card.max_freq + shift).clamp(2, 30);
    }

    if iat == 1 && count > 1 && rng.chance(0.5) {
        // Each band on its own power, in kilowatts.
        for card in cards.iter_mut() {
            card.last_field = Some(round2(10f64.powf(rng.range(-3.0, 2.7))));
        }
    }
    if iat == 2 && rng.chance(0.2) {
        // The receive card's gain column, which stands in for the design
        // frequency once it is not zero.
        for card in cards.iter_mut() {
            card.last_field = Some(round2(rng.range(2.0, 30.0)));
        }
    }
    cards
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
            assert!(
                case.freqs_mhz.windows(2).all(|w| w[0] <= w[1]),
                "{:?}",
                case.freqs_mhz
            );
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
            let central = (lat1.sin() * lat2.sin() + lat1.cos() * lat2.cos() * (lon2 - lon1).cos())
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
