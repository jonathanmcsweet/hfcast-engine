//! A Kp-conditioned correction table for storm-time foF2.
//!
//! The effective daily index (`src/essn.rs`) removes the day's global
//! level, including most of a storm day's average depression. What it
//! cannot carry is structure within the day: an ionospheric storm
//! moves mid-latitude foF2 with local time and season — the negative
//! phase from disturbed thermospheric composition. This module bins
//! that residual — observed foF2 over the essn prediction — by storm
//! class, geomagnetic latitude, season and local time, and fits one
//! median ratio per bin.
//!
//! The storm class comes from the highest Kp over the trailing 24 hours
//! (`geomag::kp_max_lookback`), because ionospheric storm effects
//! outlast the magnetic disturbance. Quiet bins are never fitted: on a
//! quiet day the table is exactly the identity, so a storm mode cannot
//! move a quiet forecast. Low-latitude bins are never fitted either —
//! the measured reason is at [`fit`].
//!
//! `FITTED` is the embedded table, fitted on the six fit months by
//! `sonde --fit-storm` and scored on the two held-out storm months
//! (2015-03, 2022-09). The gate it must pass, and the result, are in
//! `docs/ionosonde.md`.

/// Kp class edges: quiet below 4, active 4 to 5, storm 5 to 7, severe
/// at 7 and above. Class 0 is the identity by construction.
pub const KP_EDGES: [f64; 3] = [4.0, 5.0, 7.0];

pub const N_KP: usize = 4;
pub const N_LAT: usize = 3;
pub const N_SEASON: usize = 3;
pub const N_LT: usize = 4;
pub const N_BINS: usize = N_KP * N_LAT * N_SEASON * N_LT;

/// A bin's own samples must reach this count to fit its ratio; below
/// it, the season-pooled bin answers, and below that the identity.
pub const MIN_BIN: usize = 50;

/// The storm class of a trailing-24-hour maximum Kp.
pub fn kp_class(kp_max24: f64) -> usize {
    KP_EDGES.iter().filter(|edge| kp_max24 >= **edge).count()
}

/// Centered-dipole geomagnetic latitude, degrees. Pole at 80.7 N,
/// 72.7 W (IGRF epoch 2020); the bands are 15 degrees wide, so the
/// pole's slow drift cannot move a station across one.
pub fn gmlat(lat_deg: f64, lon_deg: f64) -> f64 {
    const POLE_LAT_DEG: f64 = 80.7;
    const POLE_LON_DEG: f64 = -72.7;
    let (lat, pole) = (lat_deg.to_radians(), POLE_LAT_DEG.to_radians());
    let dlon = (lon_deg - POLE_LON_DEG).to_radians();
    (lat.sin() * pole.sin() + lat.cos() * pole.cos() * dlon.cos())
        .clamp(-1.0, 1.0)
        .asin()
        .to_degrees()
}

/// Latitude band by absolute geomagnetic latitude: low below 40,
/// mid 40 to 55, high at 55 and above.
pub fn lat_band(gmlat_deg: f64) -> usize {
    match gmlat_deg.abs() {
        v if v < 40.0 => 0,
        v if v < 55.0 => 1,
        _ => 2,
    }
}

/// Local season: 0 summer, 1 equinox, 2 winter. Geographic hemisphere
/// decides which solstice months are summer; the equator's assignment
/// is arbitrary but stable, and the low band spans both hemispheres
/// anyway.
pub fn season(month: u32, lat_deg: f64) -> usize {
    let northern_season = match month {
        5..=8 => 0,
        3 | 4 | 9 | 10 => 1,
        _ => 2,
    };
    if lat_deg >= 0.0 || northern_season == 1 {
        northern_season
    } else {
        2 - northern_season
    }
}

/// Local-time quarter of the day: 0 covers 00-06 LT, 3 covers 18-24 LT.
pub fn lt_class(ut_hour: u8, lon_deg: f64) -> usize {
    let local = (f64::from(ut_hour) + lon_deg / 15.0).rem_euclid(24.0);
    ((local / 6.0) as usize).min(N_LT - 1)
}

/// The flat table index of one (month, place, hour, storm state).
pub fn bin(month: u32, lat_deg: f64, lon_deg: f64, ut_hour: u8, kp_max24: f64) -> usize {
    let lat = lat_band(gmlat(lat_deg, lon_deg));
    ((kp_class(kp_max24) * N_LAT + lat) * N_SEASON + season(month, lat_deg)) * N_LT
        + lt_class(ut_hour, lon_deg)
}

/// The same bin with the season dimension removed, for the fallback
/// pool: a sparse (class, band, season, quarter) bin borrows from its
/// all-season neighbours before giving up.
pub fn pooled_index(bin: usize) -> usize {
    let lt = bin % N_LT;
    let lat = (bin / (N_LT * N_SEASON)) % N_LAT;
    let kp = bin / (N_LT * N_SEASON * N_LAT);
    (kp * N_LAT + lat) * N_LT + lt
}

fn kp_class_of_bin(bin: usize) -> usize {
    bin / (N_LT * N_SEASON * N_LAT)
}

fn lat_band_of_bin(bin: usize) -> usize {
    (bin / (N_LT * N_SEASON)) % N_LAT
}

/// Fits the table from (bin, observed/predicted) samples. Returns the
/// ratios and each bin's own sample count. Two bands of bins stay 1.0
/// whatever the data says:
///
/// - Quiet bins: the identity on quiet days is the contract, not a
///   fitting outcome.
/// - Low-latitude bins: the equatorial storm response is driven by
///   penetration electric fields whose sign turns on the event's timing
///   against local time, which a Kp class cannot carry. Measured
///   (2026-08-13): fitting these bins gained in sample and reversed on
///   the held-out 2015-03 storm — foF2 RMS 1.32 to 1.65 MHz, day-to-day
///   correlation +0.390 to +0.245 — while the mid-latitude bins
///   transferred. The exclusion is the measurement, kept as code.
pub fn fit(samples: &[(usize, f64)]) -> ([f64; N_BINS], [usize; N_BINS]) {
    // Indexed accumulation: a fold over samples would rebuild the
    // whole table once per sample.
    let mut per_bin: Vec<Vec<f64>> = vec![Vec::new(); N_BINS];
    let mut per_pool: Vec<Vec<f64>> = vec![Vec::new(); N_KP * N_LAT * N_LT];
    for (bin, ratio) in samples {
        per_bin[*bin].push(*ratio);
        per_pool[pooled_index(*bin)].push(*ratio);
    }
    let counts: [usize; N_BINS] = std::array::from_fn(|b| per_bin[b].len());
    let ratios: [f64; N_BINS] = std::array::from_fn(|b| {
        if kp_class_of_bin(b) == 0 || lat_band_of_bin(b) == 0 {
            return 1.0;
        }
        let own = &mut per_bin[b];
        if own.len() >= MIN_BIN {
            return crate::stats::median_in_place(own);
        }
        let pool = &mut per_pool[pooled_index(b)];
        if pool.len() >= MIN_BIN {
            crate::stats::median_in_place(pool)
        } else {
            1.0
        }
    });
    (ratios, counts)
}

/// The correction for one prediction: the table's ratio when the storm
/// state is known, the identity when the Kp record has no answer.
pub fn correction(ratios: &[f64; N_BINS], bin: Option<usize>) -> f64 {
    bin.map(|b| ratios[b]).unwrap_or(1.0)
}

/// The embedded table: `sonde --fit-storm` over the six fit months
/// (2019-06, 2019-12, 2024-12, 2025-03, 2025-06, 2025-07), 2026-08-13.
/// The held-out storm months 2015-03 and 2022-09 never touched this
/// fit. Regenerate with:
/// `cargo run --release --all-features --bin sonde -- --fit-storm
///  --kp data/kp_daily.txt data/2019-06 data/2019-12 data/2024-12
///  data/2025-03 data/2025-06 data/2025-07`
#[rustfmt::skip]
pub const FITTED: [f64; N_BINS] = [
    1.0000, 1.0000, 1.0000, 1.0000, // quiet low summer
    1.0000, 1.0000, 1.0000, 1.0000, // quiet low equinox
    1.0000, 1.0000, 1.0000, 1.0000, // quiet low winter
    1.0000, 1.0000, 1.0000, 1.0000, // quiet mid summer
    1.0000, 1.0000, 1.0000, 1.0000, // quiet mid equinox
    1.0000, 1.0000, 1.0000, 1.0000, // quiet mid winter
    1.0000, 1.0000, 1.0000, 1.0000, // quiet high summer
    1.0000, 1.0000, 1.0000, 1.0000, // quiet high equinox
    1.0000, 1.0000, 1.0000, 1.0000, // quiet high winter
    1.0000, 1.0000, 1.0000, 1.0000, // active low summer
    1.0000, 1.0000, 1.0000, 1.0000, // active low equinox
    1.0000, 1.0000, 1.0000, 1.0000, // active low winter
    0.9914, 0.9707, 1.0052, 1.0247, // active mid summer
    0.9586, 0.9953, 1.0149, 1.0086, // active mid equinox
    1.0815, 1.0196, 0.9922, 1.0057, // active mid winter
    1.0000, 1.0000, 1.0000, 1.0000, // active high summer
    1.0000, 1.0000, 1.0000, 1.0000, // active high equinox
    1.0000, 1.0000, 1.0000, 1.0000, // active high winter
    1.0000, 1.0000, 1.0000, 1.0000, // storm low summer
    1.0000, 1.0000, 1.0000, 1.0000, // storm low equinox
    1.0000, 1.0000, 1.0000, 1.0000, // storm low winter
    0.9461, 0.9028, 1.0207, 1.0020, // storm mid summer
    0.9041, 0.9672, 1.0609, 1.0192, // storm mid equinox
    0.9082, 0.9923, 1.0357, 0.9205, // storm mid winter
    1.0000, 1.0000, 1.0000, 1.0000, // storm high summer
    1.0000, 1.0000, 1.0000, 1.0000, // storm high equinox
    1.0000, 1.0000, 1.0000, 1.0000, // storm high winter
    1.0000, 1.0000, 1.0000, 1.0000, // severe low summer
    1.0000, 1.0000, 1.0000, 1.0000, // severe low equinox
    1.0000, 1.0000, 1.0000, 1.0000, // severe low winter
    0.8190, 0.8270, 1.0219, 0.8767, // severe mid summer
    0.8190, 0.8270, 1.0219, 0.8767, // severe mid equinox
    0.8190, 0.8270, 1.0219, 0.8767, // severe mid winter
    1.0000, 1.0000, 1.0000, 1.0000, // severe high summer
    1.0000, 1.0000, 1.0000, 1.0000, // severe high equinox
    1.0000, 1.0000, 1.0000, 1.0000, // severe high winter
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dipole_latitude_matches_known_places() {
        // The geomagnetic pole itself.
        assert!(gmlat(80.7, -72.7) > 89.9);
        // Boulder sits magnetically north of its geographic latitude.
        let boulder = gmlat(40.0, -105.3);
        assert!((45.0..51.0).contains(&boulder), "boulder = {boulder}");
        // Jicamarca sits nearly on the magnetic equator.
        assert!(gmlat(-12.0, -76.8).abs() < 5.0);
    }

    #[test]
    fn kp_classes_split_at_the_edges() {
        assert_eq!(kp_class(3.9), 0);
        assert_eq!(kp_class(4.0), 1);
        assert_eq!(kp_class(5.0), 2);
        assert_eq!(kp_class(7.0), 3);
        assert_eq!(kp_class(9.0), 3);
    }

    #[test]
    fn seasons_swap_across_the_equator() {
        assert_eq!(season(6, 54.6), 0); // Juliusruh June: summer
        assert_eq!(season(6, -33.3), 2); // Grahamstown June: winter
        assert_eq!(season(12, -33.3), 0); // Grahamstown December: summer
        assert_eq!(season(3, 54.6), 1); // equinox both sides
        assert_eq!(season(3, -33.3), 1);
    }

    #[test]
    fn local_time_wraps_the_date_line() {
        // 23 UT at 30 E is 01 LT: the first quarter.
        assert_eq!(lt_class(23, 30.0), 0);
        // 0 UT at 105 W is 17 LT: the afternoon quarter.
        assert_eq!(lt_class(0, -105.3), 2);
    }

    #[test]
    fn bins_cover_the_table_and_pool_drops_only_season() {
        // A loop over the full cross product: the property is exhaustive
        // coverage, which a sampled functional check would not give.
        for kp in [0.0, 4.5, 6.0, 8.0] {
            for (lat, lon) in [(10.0, 0.0), (45.0, 10.0), (65.0, 20.0)] {
                for month in [3, 6, 12] {
                    for hour in [0, 7, 13, 20] {
                        let b = bin(month, lat, lon, hour, kp);
                        assert!(b < N_BINS);
                        let p = pooled_index(b);
                        assert!(p < N_KP * N_LAT * N_LT);
                        // The pool keeps class, band and quarter.
                        assert_eq!(p / (N_LAT * N_LT), kp_class(kp));
                        assert_eq!(p % N_LT, lt_class(hour, lon));
                    }
                }
            }
        }
    }

    #[test]
    fn quiet_bins_are_the_identity_whatever_the_data() {
        let quiet_bin = bin(6, 45.0, 10.0, 13, 0.0);
        let samples: Vec<(usize, f64)> = (0..100).map(|_| (quiet_bin, 0.8)).collect();
        let (ratios, counts) = fit(&samples);
        assert_eq!(counts[quiet_bin], 100);
        assert_eq!(ratios[quiet_bin], 1.0);
    }

    #[test]
    fn low_latitude_bins_are_the_identity_whatever_the_data() {
        // A well-fed severe bin over Jicamarca must still fit nothing:
        // the low-band exclusion is measured, not a data shortage.
        let low_bin = bin(3, -12.0, -76.8, 17, 8.0);
        assert_eq!(super::lat_band_of_bin(low_bin), 0);
        let samples: Vec<(usize, f64)> = (0..100).map(|_| (low_bin, 1.5)).collect();
        let (ratios, counts) = fit(&samples);
        assert_eq!(counts[low_bin], 100);
        assert_eq!(ratios[low_bin], 1.0);
    }

    #[test]
    fn a_fed_storm_bin_takes_its_median() {
        let storm_bin = bin(6, 45.0, 10.0, 13, 6.0);
        let samples: Vec<(usize, f64)> = (0..MIN_BIN)
            .map(|i| (storm_bin, if i % 2 == 0 { 0.7 } else { 0.9 }))
            .collect();
        let (ratios, _) = fit(&samples);
        assert!((ratios[storm_bin] - 0.8).abs() < 1e-12);
    }

    #[test]
    fn a_sparse_bin_borrows_from_its_season_pool() {
        // Plenty of summer samples, three winter ones: the winter bin
        // answers with the pooled median, not the identity and not the
        // three samples alone.
        let summer = bin(6, 45.0, 10.0, 13, 6.0);
        let winter = bin(12, 45.0, 10.0, 13, 6.0);
        assert_ne!(summer, winter);
        assert_eq!(pooled_index(summer), pooled_index(winter));
        let samples: Vec<(usize, f64)> = (0..MIN_BIN)
            .map(|_| (summer, 0.8))
            .chain((0..3).map(|_| (winter, 1.4)))
            .collect();
        let (ratios, counts) = fit(&samples);
        assert_eq!(counts[winter], 3);
        assert!((ratios[winter] - 0.8).abs() < 1e-12);
    }

    #[test]
    fn an_unfed_bin_and_an_unknown_state_are_the_identity() {
        let (ratios, _) = fit(&[]);
        assert!(ratios.iter().all(|r| *r == 1.0));
        assert_eq!(correction(&ratios, None), 1.0);
    }
}
