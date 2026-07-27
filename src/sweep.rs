//! Enumerates the sweep cases.
//!
//! The aim is coverage of the model's regimes rather than a dense grid. VOACAP
//! takes visibly different branches for short single-hop paths, long multi-hop
//! paths, high-latitude paths and paths crossing the equator, so the case list
//! picks paths that land in each. Month and sunspot number then vary the
//! ionosphere underneath them.

use crate::deck::DeckCase;

/// The amateur bands the app predicts, ascending. Nine fit in eleven slots.
pub const AMATEUR_FREQS_MHZ: &[f64] = &[1.84, 3.75, 7.1, 10.12, 14.2, 18.1, 21.2, 24.94, 28.4];

/// Solstice and equinox months, where the ionosphere differs most.
const MONTHS: &[u32] = &[1, 4, 7, 10];

/// Solar minimum, mid-cycle, and a strong maximum.
const SUNSPOTS: &[f64] = &[10.0, 70.0, 150.0];

pub struct PathSpec {
    pub id: &'static str,
    pub from_lat: f64,
    pub from_lon: f64,
    pub to_lat: f64,
    pub to_lon: f64,
    /// Why this path is in the set, kept so a surprising result can be read.
    pub regime: &'static str,
}

pub const PATHS: &[PathSpec] = &[
    PathSpec {
        id: "short-eu",
        from_lat: 51.5,
        from_lon: -0.13,
        to_lat: 48.86,
        to_lon: 2.35,
        regime: "very short, single hop",
    },
    PathSpec {
        id: "med-eu",
        from_lat: 35.8,
        from_lon: -5.9,
        to_lat: 44.9,
        to_lon: 20.5,
        regime: "medium mid-latitude, the vendor test circuit",
    },
    PathSpec {
        id: "long-ew",
        from_lat: 47.6,
        from_lon: -122.33,
        to_lat: 35.68,
        to_lon: 139.65,
        regime: "long east-west, wide local-time spread, multi-hop",
    },
    PathSpec {
        id: "long-ns",
        from_lat: 60.17,
        from_lon: 24.94,
        to_lat: -33.92,
        to_lon: 18.42,
        regime: "long north-south crossing the equator",
    },
    PathSpec {
        id: "polar",
        from_lat: 64.84,
        from_lon: -147.72,
        to_lat: 69.65,
        to_lon: 18.96,
        regime: "trans-polar, auroral absorption",
    },
    PathSpec {
        id: "equatorial",
        from_lat: -1.29,
        from_lon: 36.82,
        to_lat: 1.35,
        to_lon: 103.82,
        regime: "equatorial, near the anomaly crests",
    },
    PathSpec {
        id: "antipodal",
        from_lat: -33.87,
        from_lon: 151.21,
        to_lat: 51.5,
        to_lon: -0.13,
        regime: "near-antipodal, the longest path the model handles",
    },
    PathSpec {
        id: "south-am",
        from_lat: -34.6,
        from_lon: -58.38,
        to_lat: 40.71,
        to_lon: -74.01,
        regime: "long north-south in the western hemisphere",
    },
];

struct System {
    required_snr_db: f64,
    noise_dbw: f64,
    watts: f64,
}

/// Two setups: a quiet rural receiver wanting an SSB-grade signal, and a noisy
/// urban one wanting the vendor test's much stricter margin. Applying both to
/// every path would double the corpus for little extra branch coverage, so they
/// alternate by path instead.
const SYSTEMS: &[System] = &[
    System {
        required_snr_db: 24.0,
        noise_dbw: 145.0,
        watts: 100.0,
    },
    System {
        required_snr_db: 73.0,
        noise_dbw: 125.0,
        watts: 1000.0,
    },
];

pub fn sweep_cases() -> Vec<DeckCase> {
    let mut cases = Vec::with_capacity(PATHS.len() * MONTHS.len() * SUNSPOTS.len());

    for (path_index, p) in PATHS.iter().enumerate() {
        let system = &SYSTEMS[path_index % SYSTEMS.len()];
        for &month in MONTHS {
            for &ssn in SUNSPOTS {
                cases.push(DeckCase {
                    id: format!("{}-m{month:02}-s{ssn:.0}", p.id),
                    from_lat: p.from_lat,
                    from_lon: p.from_lon,
                    to_lat: p.to_lat,
                    to_lon: p.to_lon,
                    method: 30,
                    ursi: false,
                    fprob: None,
                    botlines: None,
                    toplines: None,
                    month,
                    year: 2026,
                    ssn,
                    watts: system.watts,
                    required_snr_db: system.required_snr_db,
                    noise_dbw: system.noise_dbw,
                    freqs_mhz: AMATEUR_FREQS_MHZ.to_vec(),
                    tx_antennas: Vec::new(),
                    rx_antennas: Vec::new(),
                    // The validated server configuration runs with the
                    // sporadic-E layer on, so the tolerance must cover the
                    // code paths that setting exercises.
                    sporadic_e: true,
                });
            }
        }
    }

    cases
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::deck::build_deck;
    use std::collections::BTreeSet;

    #[test]
    fn every_case_has_a_unique_id() {
        let cases = sweep_cases();
        let ids: BTreeSet<&str> = cases.iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids.len(), cases.len());
    }

    #[test]
    fn the_case_count_is_the_full_grid() {
        assert_eq!(
            sweep_cases().len(),
            PATHS.len() * MONTHS.len() * SUNSPOTS.len()
        );
    }

    #[test]
    fn every_case_produces_a_valid_deck() {
        for c in sweep_cases() {
            build_deck(&c).unwrap_or_else(|e| panic!("case {}: {e}", c.id));
        }
    }

    #[test]
    fn case_ids_fit_the_label_column() {
        // The LABEL card gives each endpoint 20 columns; a longer id is
        // truncated, which would make two cases indistinguishable in a listing.
        for c in sweep_cases() {
            assert!(
                c.id.len() <= 20,
                "id too long for the label field: {}",
                c.id
            );
        }
    }
}
