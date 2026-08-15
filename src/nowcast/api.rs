//! The nowcast point API: conditioned ionospheric answers at a place.
//!
//! One call answers "what is the ionosphere over this point at this
//! hour" in the ionosonde's own conventions: ordinary-wave foF2 in MHz,
//! foE in MHz, hmF2 in km through Dudeney's corrected form, and the
//! M(3000)F2 factor. NVIS-range MUF comes from the mirror-geometry
//! secant ([`PointAnswer::muf_at`]).
//!
//! The conditioning input is the day knob the parity engine does not
//! have. [`Conditioning::Climatology`] is the engine as shipped, at the
//! month's smoothed sunspot number. [`Conditioning::Daily`] runs at a
//! fitted daily index (`src/essn.rs`) — floored at zero for every
//! channel except foF2, which follows the fitted line wherever the fit
//! put it — and, when the trailing-24-hour Kp maximum is known,
//! multiplies foF2 by the embedded storm ratio (`src/stormfit.rs`).
//! Both were scored held-out against ionosonde truth before this API
//! existed; the numbers are in `docs/ionosonde.md`, and the floor's
//! link-level justification is in `docs/essn-wspr.md`.

use std::path::Path;

use crate::api::{predict, FoF2Model, Ionosphere, Model, Report, Request, Site, Task};
use crate::{irtam, stormfit};

/// Half the probe path's latitude span. The path runs from half a
/// degree north of the point to half a degree south, so its midpoint —
/// the one control point on a path this short — is the point itself.
pub const PROBE_OFFSET_DEG: f64 = 0.5;

/// What the day's prediction is conditioned on.
#[derive(Debug, Clone, PartialEq)]
pub enum Conditioning {
    /// The engine as shipped: the month's smoothed sunspot number.
    Climatology { ssn: f64 },
    /// A daily index fitted from live soundings, and the trailing
    /// 24-hour Kp maximum per UT hour where the index feed has one —
    /// the maximum moves during a day as storm blocks enter and leave
    /// the window. A missing hour gets no storm correction: the table
    /// is the identity, exactly what a device without the feed can
    /// honestly do.
    Daily {
        essn: f64,
        /// Boxed so the enum stays the size of its common variant.
        kp_max24: Box<[Option<f64>; 24]>,
    },
}

impl Conditioning {
    /// Daily conditioning with one storm state for the whole day, for
    /// callers that hold a single Kp number rather than the hourly
    /// record.
    pub fn daily(essn: f64, kp_max24: Option<f64>) -> Self {
        Self::Daily {
            essn,
            kp_max24: Box::new([kp_max24; 24]),
        }
    }

    /// Daily conditioning with the hourly storm record.
    pub fn daily_by_hour(essn: f64, kp_max24: [Option<f64>; 24]) -> Self {
        Self::Daily {
            essn,
            kp_max24: Box::new(kp_max24),
        }
    }

    /// The sunspot number the engine runs at. A daily index below zero
    /// is floored: below the map's lower plane there is no measured
    /// state for foE, absorption, noise or heights to extrapolate into,
    /// and the link study measured that extrapolation as the whole
    /// solar-minimum cost (`docs/essn-wspr.md`). Only foF2 follows the
    /// fitted line below zero ([`day`]), because the fit inverts that
    /// same line.
    fn ssn(&self) -> f64 {
        match self {
            Self::Climatology { ssn } => *ssn,
            Self::Daily { essn, .. } => essn.max(0.0),
        }
    }

    /// The index foF2 follows — the fitted value, unfloored.
    fn fof2_ssn(&self) -> f64 {
        match self {
            Self::Climatology { ssn } => *ssn,
            Self::Daily { essn, .. } => *essn,
        }
    }

    /// The storm ratio for one place-hour under this conditioning.
    fn storm_ratio(&self, month: u32, lat_deg: f64, lon_deg: f64, ut_hour: u8) -> f64 {
        let Self::Daily { kp_max24, .. } = self else {
            return 1.0;
        };
        let Some(kp) = kp_max24[usize::from(ut_hour) % 24] else {
            return 1.0;
        };
        let bin = stormfit::bin(month, lat_deg, lon_deg, ut_hour, kp);
        stormfit::correction(&stormfit::FITTED, Some(bin))
    }
}

/// One point question.
#[derive(Debug, Clone, PartialEq)]
pub struct PointRequest {
    pub lat_deg: f64,
    pub lon_deg: f64,
    pub month: u32,
    /// UT hour, 0 to 23.
    pub ut_hour: u8,
    pub conditioning: Conditioning,
}

/// One point answer, in ionosonde conventions.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PointAnswer {
    /// Ordinary-wave F2 critical frequency, MHz.
    pub fof2_mhz: f64,
    /// E-layer critical frequency, MHz.
    pub foe_mhz: f64,
    /// F2 peak height through Dudeney's corrected form, km.
    pub hmf2_km: f64,
    /// The M(3000)F2 propagation factor.
    pub m3000: f64,
}

impl PointAnswer {
    /// MUF over a ground range, from foF2 and the mirror geometry.
    pub fn muf_at(&self, range_km: f64) -> f64 {
        self.fof2_mhz * secant_factor(range_km, self.hmf2_km)
    }

    /// MUF(3000), DIDBase's MUFD convention.
    pub fn muf3000_mhz(&self) -> f64 {
        self.fof2_mhz * self.m3000
    }
}

/// The raw layer values of one predicted hour, engine conventions.
/// `f2z` is the extraordinary-wave F2 frequency: the map's foF2 plus
/// half the gyrofrequency `fh2`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LayerHour {
    pub fe: f64,
    pub f2z: f64,
    pub fh2: f64,
    pub h2: f64,
    pub m3000: f64,
}

/// The engine request for one point's probe path. Values other than the
/// path and month are fixed choices; the `Parameters` task reads none
/// of the system fields.
fn probe_request(lat_deg: f64, lon_deg: f64, month: u32, ssn: f64) -> Request {
    let site = |suffix: &str, lat: f64| Site {
        name: format!("probe-{suffix}"),
        lat_deg: lat,
        lon_deg,
    };
    Request {
        tx: site("n", lat_deg + PROBE_OFFSET_DEG),
        rx: site("s", lat_deg - PROBE_OFFSET_DEG),
        month,
        year: 2026,
        ssn,
        power_watts: 100.0,
        freqs_mhz: vec![7.1],
        required_snr_db: 24.0,
        noise_dbw: 145.0,
        fof2: FoF2Model::Ccir,
        layer_multipliers: [1.0, 1.0, 1.0, 1.0],
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        ionosphere: Ionosphere::default(),
        model: Model::Compatible,
    }
}

/// The raw layer values per UT hour 0..24 over one point. VOACAP's
/// hours run 1 to 24; hour 24 is the day's midnight and lands in slot
/// 0. The harness (`src/sonde.rs`) reads the same table, so the two
/// pipelines cannot disagree about what the engine said.
pub fn probe_hours(
    root: &Path,
    lat_deg: f64,
    lon_deg: f64,
    month: u32,
    ssn: f64,
) -> Result<Vec<LayerHour>, String> {
    let req = probe_request(lat_deg, lon_deg, month, ssn);
    let Report::Parameters(rows) = predict(root, &req, Task::Parameters)? else {
        return Err("Parameters task answered with a different report".to_string());
    };
    let mut by_hour = vec![None; 24];
    // One row per control point per hour; a probe path has one point,
    // so this writes each hour once. Indexing keeps the 24-to-0 hour
    // fold in one place.
    for row in &rows {
        by_hour[(row.gmt as usize) % 24] = Some(LayerHour {
            fe: f64::from(row.fe),
            f2z: f64::from(row.f2z),
            fh2: f64::from(row.fh2),
            h2: f64::from(row.h2),
            m3000: f64::from(row.m3000),
        });
    }
    by_hour
        .into_iter()
        .enumerate()
        .map(|(hour, slot)| slot.ok_or(format!("no parameters for hour {hour}")))
        .collect()
}

/// The frequency ladder the absorption-edge probe sweeps, MHz. Twelve
/// is the engine's frequency-slot limit; the steps are near-geometric
/// so each carries a similar share of the absorption's 1/f-squared
/// growth.
pub const EDGE_LADDER_MHZ: [f64; 10] = [2.0, 2.4, 2.9, 3.5, 4.2, 5.0, 6.0, 7.2, 8.6, 10.3];

/// How far below the hour's own SNR plateau the edge sits. Relative to
/// the plateau rather than absolute, the way a sounder's fmin is
/// relative to its own echo strength — so a station's level (noise
/// floor, path constants) cancels and only the shape is read. The
/// engine's LUF task cannot serve here: its scan floors at 2 MHz and
/// when no frequency meets the requirement its answer flips to the
/// best frequency near the MUF — a different edge (measured
/// 2026-08-13, `docs/ionosonde.md`).
pub const EDGE_DROP_DB: f64 = 6.0;

/// The engine's absorption edge per UT hour over the probe path: the
/// lowest frequency at which predicted SNR is within [`EDGE_DROP_DB`]
/// of the hour's plateau, interpolated on [`EDGE_LADDER_MHZ`]. The
/// ionogram counterpart is fmin. None where the whole ladder sits
/// within the drop (no edge above 2 MHz — the usual night state, where
/// a sounder's fmin is its instrument floor too).
pub fn probe_edge(
    root: &Path,
    lat_deg: f64,
    lon_deg: f64,
    month: u32,
    ssn: f64,
) -> Result<Vec<Option<f64>>, String> {
    let mut req = probe_request(lat_deg, lon_deg, month, ssn);
    req.freqs_mhz = EDGE_LADDER_MHZ.to_vec();
    let Report::Systems(prediction) = predict(root, &req, Task::Systems)? else {
        return Err("Systems task answered with a different report".to_string());
    };
    let mut by_hour = vec![None; 24];
    // Indexing keeps the 24-to-0 hour fold in one place, as above.
    for hour in &prediction.hours {
        let snr: Vec<f64> = (0..EDGE_LADDER_MHZ.len())
            .map(|i| f64::from(hour.son[i].sndb))
            .collect();
        by_hour[(hour.gmt as usize) % 24] = edge_crossing(&snr);
    }
    Ok(by_hour)
}

/// The single fitted level of the first edge calibration (2026-08-13,
/// six months). Kept only so dependents of that calibration still
/// compile; the shipped level is [`edge_fmin_ratio`], which the
/// whole-archive refit showed varies with solar activity and season
/// (`docs/refit.md`).
#[deprecated(note = "use edge_fmin_ratio(month, index): the level varies")]
pub const EDGE_FMIN_RATIO: f64 = 1.6138;

/// The fitted level model between the probe edge and the ionogram's
/// fmin convention: coefficients of ln(ratio) over
/// `[1, index, cos a, sin a, cos 2a, sin 2a]`, `a = 2π month / 12`.
///
/// Fitted 2026-08-15 on the whole archive's day-station medians
/// (weighted least squares, `sonde --fit-edge`; eight held-out months
/// never touched the fit — verdict in `docs/refit.md`). The index term
/// is the larger effect: the level runs about 1.3 near solar minimum
/// and past 2.0 at maximum, which a single constant split the
/// difference on. The season term follows the calendar for every
/// station; mirroring it for the southern hemisphere was measured and
/// did not survive the held-out verdict (six southern stations reach
/// only −34°, so a deeper network could reopen the question).
pub const EDGE_RATIO_MODEL: [f64; 6] =
    [0.428202, 0.001881, 0.096822, 0.045227, -0.074819, -0.038815];

/// The span of daily indexes the model was fitted on (the archive's
/// measured range, 2015-01 to 2026-07). Outside it the ratio holds the
/// boundary value rather than extrapolate.
pub const EDGE_INDEX_SPAN: (f64, f64) = (-25.0, 200.0);

/// The fitted probe-edge over fmin level for one month and daily index.
pub fn edge_fmin_ratio(month: u32, index: f64) -> f64 {
    let i = index.clamp(EDGE_INDEX_SPAN.0, EDGE_INDEX_SPAN.1);
    let a = std::f64::consts::TAU * f64::from(month) / 12.0;
    let [c0, c1, c2, c3, c4, c5] = EDGE_RATIO_MODEL;
    (c0 + c1 * i + c2 * a.cos() + c3 * a.sin() + c4 * (2.0 * a).cos() + c5 * (2.0 * a).sin()).exp()
}

/// The usable window's lower edge per UT hour over the probe path, on
/// the ionogram's fmin convention: [`probe_edge`] at the conditioning's
/// index (floored, as every engine channel is), divided by the fitted
/// [`edge_fmin_ratio`] at that month and index. None keeps its probe
/// meaning — no edge above the ladder's floor, the usual night state.
pub fn lower_edge(
    root: &Path,
    lat_deg: f64,
    lon_deg: f64,
    month: u32,
    conditioning: &Conditioning,
) -> Result<Vec<Option<f64>>, String> {
    let ratio = edge_fmin_ratio(month, conditioning.ssn());
    let edges = probe_edge(root, lat_deg, lon_deg, month, conditioning.ssn())?;
    Ok(edges
        .into_iter()
        .map(|e| on_fmin_convention(e, ratio))
        .collect())
}

/// One probe edge onto the ionogram convention.
fn on_fmin_convention(edge: Option<f64>, ratio: f64) -> Option<f64> {
    edge.map(|e| e / ratio)
}

/// Where the SNR curve rises through plateau minus the drop, scanning
/// up the ladder. None when the first rung is already inside the drop.
fn edge_crossing(snr: &[f64]) -> Option<f64> {
    let plateau = snr.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let edge = plateau - EDGE_DROP_DB;
    if snr[0] >= edge {
        return None;
    }
    let i = snr.iter().position(|s| *s >= edge)?;
    let (f_low, f_high) = (EDGE_LADDER_MHZ[i - 1], EDGE_LADDER_MHZ[i]);
    let (s_low, s_high) = (snr[i - 1], snr[i]);
    Some(f_low + (f_high - f_low) * (edge - s_low) / (s_high - s_low))
}

/// All 24 hours of one conditioned day over a point.
pub fn day(
    root: &Path,
    lat_deg: f64,
    lon_deg: f64,
    month: u32,
    conditioning: &Conditioning,
) -> Result<Vec<PointAnswer>, String> {
    let hours = probe_hours(root, lat_deg, lon_deg, month, conditioning.ssn())?;
    // Below the floor, foF2 alone follows the fitted index: one more
    // probe at the unfloored value, read for its foF2 only.
    let fof2_hours = if conditioning.fof2_ssn() < conditioning.ssn() {
        Some(probe_hours(
            root,
            lat_deg,
            lon_deg,
            month,
            conditioning.fof2_ssn(),
        )?)
    } else {
        None
    };
    Ok(hours
        .iter()
        .enumerate()
        .map(|(hour, layer)| {
            let fof2_layer = fof2_hours.as_deref().map_or(layer, |h| &h[hour]);
            let fof2 = fof2_layer.f2z - fof2_layer.fh2;
            let ratio = conditioning.storm_ratio(month, lat_deg, lon_deg, hour as u8);
            PointAnswer {
                fof2_mhz: fof2 * ratio,
                foe_mhz: layer.fe,
                // The height reads the run's own uncorrected foF2: the
                // storm ratio is fitted to foF2 alone, and Dudeney's
                // relation was scored with the run's values.
                hmf2_km: irtam::hmf2_dudeney(layer.m3000, fof2, layer.fe),
                m3000: layer.m3000,
            }
        })
        .collect())
}

/// One conditioned point-hour.
pub fn point(root: &Path, req: &PointRequest) -> Result<PointAnswer, String> {
    let answers = day(root, req.lat_deg, req.lon_deg, req.month, &req.conditioning)?;
    answers
        .get(usize::from(req.ut_hour) % 24)
        .copied()
        .ok_or(format!("no answer for hour {}", req.ut_hour))
}

/// The obliquity factor for a mirror reflection at `hmf2_km` over a
/// ground range of `distance_km`, curved earth. MUF(d) = foF2 x this.
/// At zero range it is exactly 1; at 600 km under a 300 km layer it is
/// about 1.4 — the small-secant regime where foF2 error dominates.
pub fn secant_factor(distance_km: f64, hmf2_km: f64) -> f64 {
    const EARTH_RADIUS_KM: f64 = 6371.0;
    if distance_km <= 0.0 || hmf2_km <= 0.0 {
        return 1.0;
    }
    let half_angle = distance_km / (2.0 * EARTH_RADIUS_KM);
    let ratio = EARTH_RADIUS_KM / (EARTH_RADIUS_KM + hmf2_km);
    let elevation = ((half_angle.cos() - ratio) / half_angle.sin()).atan();
    let sin_incidence = ratio * elevation.cos();
    1.0 / (1.0 - sin_incidence * sin_incidence).sqrt()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn straight_up_needs_no_obliquity() {
        assert_eq!(secant_factor(0.0, 300.0), 1.0);
    }

    #[test]
    fn the_factor_matches_the_mirror_geometry() {
        // 600 km under a 300 km mirror: worked by hand from the curved-
        // earth construction, about 1.40.
        let k = secant_factor(600.0, 300.0);
        assert!((k - 1.397).abs() < 0.01, "k = {k}");
        // A lower mirror bends more.
        assert!(secant_factor(600.0, 250.0) > k);
    }

    #[test]
    fn the_answer_derives_its_mufs() {
        let answer = PointAnswer {
            fof2_mhz: 5.0,
            foe_mhz: 2.0,
            hmf2_km: 300.0,
            m3000: 3.2,
        };
        assert_eq!(answer.muf_at(0.0), 5.0);
        assert!((answer.muf3000_mhz() - 16.0).abs() < 1e-12);
        assert!(answer.muf_at(600.0) > 5.0);
    }

    #[test]
    fn conditioning_without_kp_has_no_storm_ratio() {
        let daily = Conditioning::daily(80.0, None);
        assert_eq!(daily.storm_ratio(3, 45.0, 10.0, 13), 1.0);
        let clim = Conditioning::Climatology { ssn: 80.0 };
        assert_eq!(clim.storm_ratio(3, 45.0, 10.0, 13), 1.0);
    }

    #[test]
    fn a_known_storm_state_reads_the_embedded_table() {
        let daily = Conditioning::daily(80.0, Some(8.0));
        let expected = stormfit::FITTED[stormfit::bin(3, 45.0, 10.0, 13, 8.0)];
        assert_eq!(daily.storm_ratio(3, 45.0, 10.0, 13), expected);
        // The mid-latitude severe bin is a real fitted value.
        assert_ne!(expected, 1.0);
    }

    #[cfg(feature = "embedded-coefficients")]
    mod with_coefficients {
        use super::super::*;
        use crate::voacap::data;

        const JULIUSRUH: (f64, f64) = (54.6, 13.4);

        fn june_day(conditioning: &Conditioning) -> Vec<PointAnswer> {
            day(
                &data::embedded_root(),
                JULIUSRUH.0,
                JULIUSRUH.1,
                6,
                conditioning,
            )
            .expect("the embedded root answers")
        }

        #[test]
        fn a_day_is_24_finite_plausible_hours() {
            let answers = june_day(&Conditioning::Climatology { ssn: 80.0 });
            assert_eq!(answers.len(), 24);
            for a in &answers {
                assert!(a.fof2_mhz.is_finite() && a.fof2_mhz > 1.0 && a.fof2_mhz < 20.0);
                assert!(a.foe_mhz.is_finite() && a.foe_mhz >= 0.0);
                assert!(a.hmf2_km.is_finite() && a.hmf2_km > 150.0 && a.hmf2_km < 500.0);
                assert!(a.m3000.is_finite() && a.m3000 > 2.0 && a.m3000 < 4.0);
            }
        }

        #[test]
        fn the_same_question_gets_the_same_answer() {
            let conditioning = Conditioning::daily(63.0, Some(5.5));
            assert_eq!(june_day(&conditioning), june_day(&conditioning));
        }

        #[test]
        fn a_point_is_its_days_hour() {
            let conditioning = Conditioning::Climatology { ssn: 80.0 };
            let answers = june_day(&conditioning);
            let one = point(
                &data::embedded_root(),
                &PointRequest {
                    lat_deg: JULIUSRUH.0,
                    lon_deg: JULIUSRUH.1,
                    month: 6,
                    ut_hour: 13,
                    conditioning,
                },
            )
            .expect("the embedded root answers");
            assert_eq!(one, answers[13]);
        }

        #[test]
        fn climatology_is_the_parity_engine_in_sonde_conventions() {
            // The tripwire: under climatology conditioning, foF2 must be
            // exactly the parity engine's extraordinary-wave value minus
            // half the gyrofrequency, and the height exactly Dudeney over
            // the run's own values. When a later phase replaces the inner
            // physics, this equality becomes a measured envelope.
            let hours = probe_hours(&data::embedded_root(), JULIUSRUH.0, JULIUSRUH.1, 6, 80.0)
                .expect("the embedded root answers");
            let answers = june_day(&Conditioning::Climatology { ssn: 80.0 });
            for (layer, answer) in hours.iter().zip(&answers) {
                assert_eq!(answer.fof2_mhz, layer.f2z - layer.fh2);
                assert_eq!(answer.foe_mhz, layer.fe);
                assert_eq!(answer.m3000, layer.m3000);
                let dudeney =
                    crate::irtam::hmf2_dudeney(layer.m3000, layer.f2z - layer.fh2, layer.fe);
                assert_eq!(answer.hmf2_km, dudeney);
            }
        }

        #[test]
        fn daily_fof2_is_linear_in_the_index() {
            // foF2 is an exact line in the sunspot number (two map
            // planes, linear blend), so the midpoint index must land on
            // the midpoint frequency within f32 rounding.
            let at = |essn: f64| june_day(&Conditioning::daily(essn, None));
            let (low, mid, high) = (at(0.0), at(50.0), at(100.0));
            for hour in 0..24 {
                let expected = (low[hour].fof2_mhz + high[hour].fof2_mhz) / 2.0;
                assert!(
                    (mid[hour].fof2_mhz - expected).abs() < 5e-3,
                    "hour {hour}: {} vs {expected}",
                    mid[hour].fof2_mhz
                );
            }
        }

        #[test]
        fn the_probe_edge_has_the_absorption_shape() {
            // D-region absorption is a daylight phenomenon: at local
            // noon the edge sits mid-band; at local midnight the whole
            // ladder is within the drop and there is no edge, the way
            // a night ionogram's fmin is the instrument's floor.
            let edge = probe_edge(&data::embedded_root(), JULIUSRUH.0, JULIUSRUH.1, 6, 80.0)
                .expect("the embedded root answers");
            assert_eq!(edge.len(), 24);
            for value in edge.iter().flatten() {
                assert!((2.0..10.3).contains(value), "off the ladder: {value}");
            }
            // Local noon at 13.4 E is near 11 UT; local midnight near 23 UT.
            let noon = edge[11].expect("a daytime edge");
            assert!((2.0..7.0).contains(&noon), "noon edge {noon}");
            assert_eq!(edge[23], None);
        }

        #[test]
        fn the_lower_edge_is_the_probe_on_the_ionogram_convention() {
            let ratio = edge_fmin_ratio(6, 50.0);
            assert_eq!(on_fmin_convention(None, ratio), None);
            let mapped =
                on_fmin_convention(Some(ratio * 2.5), ratio).expect("an edge comes through");
            assert!((mapped - 2.5).abs() < 1e-12, "mapped {mapped}");
        }

        #[test]
        fn the_edge_level_rises_with_the_index_and_holds_at_the_span() {
            // The fitted sign: more solar activity, higher level.
            assert!(edge_fmin_ratio(6, 150.0) > edge_fmin_ratio(6, 0.0));
            // Outside the measured span the boundary value answers.
            assert_eq!(
                edge_fmin_ratio(6, EDGE_INDEX_SPAN.1 + 500.0),
                edge_fmin_ratio(6, EDGE_INDEX_SPAN.1)
            );
            assert_eq!(
                edge_fmin_ratio(6, EDGE_INDEX_SPAN.0 - 500.0),
                edge_fmin_ratio(6, EDGE_INDEX_SPAN.0)
            );
        }

        #[test]
        fn below_the_floor_only_fof2_follows_the_index() {
            // At an index of -20, foE, M(3000)F2 and the run behind the
            // height must be the index-zero run's, while foF2 keeps
            // following the fitted line below it.
            let below = june_day(&Conditioning::daily(-20.0, None));
            let floor = june_day(&Conditioning::daily(0.0, None));
            let above = june_day(&Conditioning::daily(20.0, None));
            for hour in 0..24 {
                assert_eq!(below[hour].foe_mhz, floor[hour].foe_mhz);
                assert_eq!(below[hour].m3000, floor[hour].m3000);
                // The line through 0 and +20 extended to -20, within
                // f32 rounding.
                let expected = 2.0 * floor[hour].fof2_mhz - above[hour].fof2_mhz;
                assert!(
                    (below[hour].fof2_mhz - expected).abs() < 5e-3,
                    "hour {hour}: {} vs {expected}",
                    below[hour].fof2_mhz
                );
                assert!(below[hour].fof2_mhz < floor[hour].fof2_mhz);
            }
        }

        #[test]
        fn the_storm_ratio_multiplies_the_daily_answer() {
            let quiet = june_day(&Conditioning::daily(63.0, None));
            let severe = june_day(&Conditioning::daily(63.0, Some(8.0)));
            for (hour, (q, s)) in quiet.iter().zip(&severe).enumerate() {
                let ratio = stormfit::correction(
                    &stormfit::FITTED,
                    Some(stormfit::bin(6, JULIUSRUH.0, JULIUSRUH.1, hour as u8, 8.0)),
                );
                assert!((s.fof2_mhz - q.fof2_mhz * ratio).abs() < 1e-12);
                // The height is not storm-corrected.
                assert_eq!(s.hmf2_km, q.hmf2_km);
            }
        }
    }
}
