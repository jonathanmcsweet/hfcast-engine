//! The whole point-to-point prediction, end to end: what `HFMUFS`
//! drives per hour — geometry, magnetic field, coefficients, layer
//! parameters, MUF, the LUFFY passes with the smoothing blend, `SETLUF`
//! and `OUTBOD`'s sentinels — with the same deck assumptions as the
//! server (method 30, all 24 UTC hours, isotropic antennas, one power).
//!
//! [`listing_text`] renders the result as the method-30 listing body:
//! the same rows, formats and rounding as `OUTBOD`/`FLOLIN`/`FIXLIN`,
//! so `listing::parse_listing` reads it exactly as it reads the
//! Fortran engine's output. That is the whole-engine comparison
//! surface — the tolerance envelope in `docs/sensitivity.md` is defined
//! on these printed cells.

use std::path::Path;

use crate::deck::DeckCase;

use super::coefficients::{redmap, CoefficientSet, FoF2Model};
use super::con::{MagneticPole, D2R, R, R2D};
use super::geometry::path_geometry;
use super::ionogram::{alosfv, fobby, genion, sang, selmod};
use super::ionosphere::{
    alatd, cofion, esind, geotim, ground_constants, layer_parameters, virtim,
};
use super::antenna::{
    dazel0, point_to_point_table, read_antenna, AntennaEnd, AntennaSet, AntennaSetup,
};
use super::magnetic::magvar;
use super::modes::{
    es_slots, luffy_freq_loop, luffy_luf, luffy_smooth, outbod_sentinels, setlng, setluf,
    DeckParams, Geog, HourSaves, ModeLoopState, PassCtx, Son,
};
use super::muf::{curmuf, ionset, lecden, nommuf, IonoState};
use super::noise::{anois1, genois};
use super::sigdis::{sigdis, SignalDistribution};

/// One `ANTENNA` card's fields, minus the end and power.
#[derive(Debug, Clone)]
pub struct AntennaCardSpec {
    /// Path under `<itshfbc>/antennas`, e.g. `samples/sample.21`.
    pub file: String,
    pub design_freq: R,
    pub beam_deg: R,
    pub min_freq: i32,
    pub max_freq: i32,
}

impl AntennaCardSpec {
    /// The default card every prediction used before antennas were
    /// wired in.
    pub fn isotrope() -> Self {
        Self {
            file: "default/isotrope".to_string(),
            design_freq: 0.0,
            beam_deg: 0.0,
            min_freq: 2,
            max_freq: 30,
        }
    }
}

/// Everything a prediction needs; the deck-card equivalents.
#[derive(Debug, Clone)]
pub struct RunInputs {
    pub from_lat_deg: f64,
    pub from_lon_deg: f64,
    pub to_lat_deg: f64,
    pub to_lon_deg: f64,
    /// 1-12.
    pub month: u32,
    pub ssn: R,
    /// Frequencies in MHz, at most 11 (the card slots).
    pub freqs_mhz: Vec<R>,
    pub required_snr_db: R,
    /// Man-made noise at 3 MHz, dB below 1 W (positive).
    pub noise_dbw: i32,
    pub watts: R,
    /// Whether the deck's `FPROB` card leaves sporadic E on. Kept for
    /// callers that describe a case that way; the engine reads
    /// [`RunInputs::psc`].
    pub sporadic_e: bool,
    /// `None` is the isotrope card at that end.
    pub tx_antenna: Option<AntennaCardSpec>,
    pub rx_antenna: Option<AntennaCardSpec>,
    /// The receive card's last field: a non-zero value becomes the
    /// receive isotrope's gain.
    pub rx_gain_field: R,
    /// The `METHOD` card's first field, before `DECRED` rewrites 30 to
    /// 20. It selects which model runs and which lines print.
    pub method: u32,
    /// The `COEFFS` card: the foF2 map set to read.
    pub fof2: FoF2Model,
    /// The `FPROB` card: the E, F1, F2 and sporadic-E multipliers.
    pub psc: [R; 4],
}

/// Asks the engine the same question the deck card asks.
///
/// The harness describes a case once, as a [`DeckCase`], and both
/// engines are driven from that one description. Building the Rust
/// inputs separately would let the two drift apart and turn a
/// comparison into a comparison of two different questions.
impl From<&DeckCase> for RunInputs {
    fn from(c: &DeckCase) -> Self {
        Self {
            from_lat_deg: c.from_lat,
            from_lon_deg: c.from_lon,
            to_lat_deg: c.to_lat,
            to_lon_deg: c.to_lon,
            month: c.month,
            ssn: c.ssn as R,
            freqs_mhz: c.freqs_mhz.iter().map(|f| *f as R).collect(),
            required_snr_db: c.required_snr_db as R,
            noise_dbw: c.noise_dbw as i32,
            watts: c.watts as R,
            sporadic_e: c.sporadic_e,
            tx_antenna: c.tx_antenna.as_ref().map(|a| AntennaCardSpec {
                file: a.file.clone(),
                design_freq: a.design_freq as R,
                beam_deg: a.beam_deg as R,
                min_freq: 2,
                max_freq: 30,
            }),
            rx_antenna: c.rx_antenna.as_ref().map(|a| AntennaCardSpec {
                file: a.file.clone(),
                design_freq: a.design_freq as R,
                beam_deg: a.beam_deg as R,
                min_freq: 2,
                max_freq: 30,
            }),
            rx_gain_field: 0.0,
            method: c.method,
            fof2: if c.ursi {
                FoF2Model::Ursi
            } else {
                FoF2Model::Ccir
            },
            psc: c.fprob().map(|v| v as R),
        }
    }
}

/// `ANTCALC` for one run: computes both ends' gain tables from their
/// definition files and installs them as `DECRED` reads them back.
fn build_antennas(itshfbc: &Path, inp: &RunInputs) -> Result<AntennaSet, String> {
    let pwrkw = inp.watts / 1000.0;
    let (taz, _) = dazel0(
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
    );
    let (raz, _) = dazel0(
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
    );
    let iso = AntennaCardSpec::isotrope();
    let tx = inp.tx_antenna.as_ref().unwrap_or(&iso);
    let rx = inp.rx_antenna.as_ref().unwrap_or(&iso);
    let mut ants = AntennaSet::default();
    let txf = read_antenna(itshfbc, &tx.file)?;
    let tx_table = point_to_point_table(&AntennaSetup {
        file: &txf,
        end: AntennaEnd::Transmit,
        min_freq: tx.min_freq,
        max_freq: tx.max_freq,
        design_freq: tx.design_freq,
        beam_deg: tx.beam_deg,
        power_field: pwrkw,
        azimuth_deg: taz,
    })
    .map_err(|e| e.to_string())?;
    ants.install(1, tx.min_freq, tx.max_freq, tx_table, pwrkw);
    let rxf = read_antenna(itshfbc, &rx.file)?;
    let rx_table = point_to_point_table(&AntennaSetup {
        file: &rxf,
        end: AntennaEnd::Receive,
        min_freq: rx.min_freq,
        max_freq: rx.max_freq,
        design_freq: rx.design_freq,
        beam_deg: rx.beam_deg,
        power_field: inp.rx_gain_field,
        azimuth_deg: raz,
    })
    .map_err(|e| e.to_string())?;
    ants.install(2, rx.min_freq, rx.max_freq, rx_table, 0.0);
    Ok(ants)
}

/// One line of `OUTPAR`: the ionospheric parameters at one control
/// point for one hour. Card method 1 (`ITRUN = 1`) prints these and
/// computes nothing else, so they are the layer parameters as
/// `TIMVAR`, `F2VAR` and `ESIND` leave them, before `IONSET` reshapes
/// the profile.
#[derive(Debug, Clone, Copy)]
pub struct ParRow {
    /// Geographic latitude and longitude of the point, degrees.
    pub lat: R,
    pub lon: R,
    /// Local mean time at the point and the hour's UT.
    pub lmt: R,
    pub gmt: R,
    /// E critical frequency.
    pub fe: R,
    /// F1 critical frequency, semithickness and height.
    pub f1: R,
    pub y1: R,
    pub h1: R,
    /// Half the gyrofrequency.
    pub fh2: R,
    /// F2 critical frequency, semithickness and height.
    pub f2z: R,
    pub y2: R,
    pub h2: R,
    /// Sporadic E: the lower decile, median and upper decile.
    pub es: R,
    pub med: R,
    pub esu: R,
    /// M(3000)F2, the virtual height at 0.834 of the F2 critical
    /// frequency, and the F2 height-to-semithickness ratio.
    pub m3000: R,
    pub hpf2: R,
    pub rat: R,
    /// Sun zenith angle, and the maximum at which an F1 layer exists.
    pub zen: R,
    pub zmax: R,
    /// Geomagnetic latitude, degrees.
    pub magl: R,
}

/// Runs the ionospheric parameters alone for all 24 hours: `ITRUN = 1`,
/// card method 1. Returns one row per control point per hour, in the
/// order `OUTPAR` prints them.
pub fn run_par(itshfbc: &Path, inp: &RunInputs) -> Result<Vec<ParRow>, String> {
    let pole = MagneticPole::for_tree(itshfbc);
    let geo = path_geometry(
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
        false,
        pole,
    );
    let mags: Vec<_> = geo.points.iter().map(|p| magvar(p.lat, p.lon)).collect();
    let set: CoefficientSet =
        redmap(itshfbc, inp.fof2, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let cof = cofion(&set);
    let grounds = ground_constants(&set, &geo.points, &mags);
    let _ = alatd(&geo.points);
    let psc = inp.psc;

    let mut out = Vec::with_capacity(24 * geo.points.len());
    for jt in 1..=24i32 {
        let gmt = jt as R;
        let ab = virtim(&cof, &set.ikim, gmt);
        let params = layer_parameters(
            &set, &ab, &geo.points, &mags, inp.month, inp.ssn, gmt, &psc,
        );
        let es = esind(&set, &ab, &geo.points, &mags, &psc);
        let geog = Geog::from_points(&params, &mags, &grounds);
        for (k, p) in params.iter().enumerate() {
            out.push(ParRow {
                lat: geo.points[k].lat * R2D,
                lon: geo.points[k].lon * R2D,
                lmt: p.clck,
                gmt,
                fe: p.fi[0],
                f1: p.fi[1],
                y1: p.yi[1],
                h1: p.hi[1],
                fh2: geog.gyz[k] / 2.0,
                f2z: p.fi[2],
                y2: p.yi[2],
                h2: p.hi[2],
                es: es[k].fs[0],
                med: es[k].fs[1],
                esu: es[k].fs[2],
                m3000: p.f2m3,
                hpf2: p.hpf2,
                rat: p.rat,
                zen: p.zenang,
                zmax: p.zenmax,
                magl: geo.points[k].gmlat * R2D,
            });
        }
    }
    Ok(out)
}

/// One hour of a MUF-only run (`ITRUN` 3 and 4, card methods 3 to 11):
/// what `CURMUF` leaves for `OUTMUF` and `OUTLAY` to print.
#[derive(Debug, Clone)]
pub struct MufHourOut {
    pub gmt: R,
    pub lmt: R,
    pub fot: R,
    pub hpf: R,
    pub esmuf: R,
    pub allmuf: R,
    /// E, F1, F2 and Es. `OUTLAY` prints slots 1 and 2 on its first
    /// line and 3 and 4 on its second, under headings that name the
    /// F1 and F2 layers the other way round.
    pub layers: [super::muf::LayerMuf; 4],
}

/// Runs the MUF computation alone for all 24 hours, with no systems
/// model after it. Card methods 7 to 11 (`ITRUN = 4`) take the MUFs
/// from the complete electron-density profile with `CURMUF`; methods
/// 3 to 6 (`ITRUN = 3`) take them from the manual nomogram method with
/// `NOMMUF`, which fills no per-layer detail.
pub fn run_muf(itshfbc: &Path, inp: &RunInputs) -> Result<Vec<MufHourOut>, String> {
    let pole = MagneticPole::for_tree(itshfbc);
    let geo = path_geometry(
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
        false,
        pole,
    );
    let mags: Vec<_> = geo.points.iter().map(|p| magvar(p.lat, p.lon)).collect();
    let set: CoefficientSet =
        redmap(itshfbc, inp.fof2, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let cof = cofion(&set);
    let _ = alatd(&geo.points);
    let clats: Vec<R> = geo.points.iter().map(|p| p.lat).collect();
    let psc = inp.psc;
    let from_lon_rad = inp.from_lon_deg as R * D2R;
    let to_lon_rad = inp.to_lon_deg as R * D2R;

    let mut fsecv_carry = [0.0 as R; 3];
    let mut out = Vec::with_capacity(24);
    for jt in 1..=24i32 {
        let gmt = jt as R;
        let ab = virtim(&cof, &set.ikim, gmt);
        let params = layer_parameters(
            &set, &ab, &geo.points, &mags, inp.month, inp.ssn, gmt, &psc,
        );
        let es = esind(&set, &ab, &geo.points, &mags, &psc);
        let mut state = IonoState::from_layers(&params);
        state.fsecv = fsecv_carry;
        ionset(&mut state);
        let mut es_state = es.clone();
        let clcks: Vec<R> = params.iter().map(|p| p.clck).collect();
        let hour = if (3..=6).contains(&inp.method) {
            let (fs, hs) = es_slots(&es_state);
            let mut f2m3 = [0.0 as R; 5];
            for (k, p) in params.iter().enumerate() {
                f2m3[k] = p.f2m3;
            }
            nommuf(
                &state.fi,
                &f2m3,
                &fs,
                &hs,
                state.km,
                geo.gcd,
                geo.gcd_km,
            )
        } else {
            curmuf(
                &mut state,
                &mut es_state,
                &set.f2d,
                &clats,
                &clcks,
                geo.gcd,
                geo.gcd_km,
                0.1,
                inp.ssn,
            )
        };
        let times = geotim(jt, 1, from_lon_rad, to_lon_rad);
        out.push(MufHourOut {
            gmt,
            lmt: times.lmt_tx,
            fot: hour.fot,
            hpf: hour.hpf,
            esmuf: hour.esmuf,
            allmuf: hour.allmuf,
            layers: hour.layers,
        });
        fsecv_carry = state.fsecv;
    }
    Ok(out)
}

/// One hour of a LUF run (`ITRUN = 8`, card methods 26-29): the MUF
/// block plus the LUF the search found. A negative LUF means no
/// frequency met the required reliability, and its magnitude is the
/// most reliable frequency of the sweep.
#[derive(Debug, Clone, Copy)]
pub struct LufHour {
    pub gmt: R,
    pub lmt: R,
    pub fot: R,
    pub hpf: R,
    pub esmuf: R,
    pub allmuf: R,
    pub xluf: R,
}

/// Runs the LUF computation for all 24 hours: `LUFFY` with `IPFG` 300
/// below 10000 km and 400 beyond, sweeping the frequency complement
/// for the lowest frequency meeting the required reliability.
pub fn run_luf(itshfbc: &Path, inp: &RunInputs) -> Result<Vec<LufHour>, String> {
    let pole = MagneticPole::for_tree(itshfbc);
    let geo = path_geometry(
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
        false,
        pole,
    );
    let mags: Vec<_> = geo.points.iter().map(|p| magvar(p.lat, p.lon)).collect();
    let set: CoefficientSet =
        redmap(itshfbc, inp.fof2, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let cof = cofion(&set);
    let grounds = ground_constants(&set, &geo.points, &mags);
    let _ = alatd(&geo.points);
    let clats: Vec<R> = geo.points.iter().map(|p| p.lat).collect();
    let glats: Vec<R> = geo.points.iter().map(|p| p.gmlat).collect();
    let psc = inp.psc;
    let nang = sang(geo.gcd_km, 0.1);
    let ants = build_antennas(itshfbc, inp)?;
    let deck = DeckParams {
        amind: 0.1,
        rsn: inp.required_snr_db,
        lufp: 90,
        pmp: 3.0,
        dmp: 0.1,
        method: inp.method,
    };
    let from_lon_rad = inp.from_lon_deg as R * D2R;
    let to_lon_rad = inp.to_lon_deg as R * D2R;
    let to_lat_rad = inp.to_lat_deg as R * D2R;

    // GCDLNG: the long model beyond 10000 km.
    let long = geo.gcd_km >= 10000.0;

    let mut lp = ModeLoopState::default();
    let mut fsecv_carry = [0.0 as R; 3];
    let mut out = Vec::with_capacity(24);
    for jt in 1..=24i32 {
        let gmt = jt as R;
        let ab = virtim(&cof, &set.ikim, gmt);
        let params = layer_parameters(
            &set,
            &ab,
            &geo.points,
            &mags,
            inp.month,
            inp.ssn,
            gmt,
            &psc,
        );
        let es = esind(&set, &ab, &geo.points, &mags, &psc);
        let mut state = IonoState::from_layers(&params);
        state.fsecv = fsecv_carry;
        ionset(&mut state);
        let mut es_state = es.clone();
        let clcks: Vec<R> = params.iter().map(|p| p.clck).collect();
        let mut hour = curmuf(
            &mut state,
            &mut es_state,
            &set.f2d,
            &clats,
            &clcks,
            geo.gcd,
            geo.gcd_km,
            deck.amind,
            inp.ssn,
        );
        let times = geotim(jt, 1, from_lon_rad, to_lon_rad);
        let an = anois1(&set, times.gmtr, to_lat_rad, to_lon_rad, inp.to_lon_deg as R);
        let fof2_end = state.fi[state.kfx - 1][2];
        let noise_for = |f: R| {
            let reff = ants.gain(2, 0.0, f).1;
            genois(reff, &set, &an, f, to_lat_rad, fof2_end, inp.noise_dbw)
        };

        let jmode = selmod(&state);
        // The electron-density chain, and the area index `K` it leaves
        // behind. It starts at `JMODE` for the short pass and at area 1
        // for the long one, and runs a second time for the receiver-end
        // area unless the first area was already past area 1. The test
        // that ends it names only `IPFG` 100, so both LUF passes take
        // the second area even though only the long one uses it, and
        // the short LUF pass is left with `K = KFX` where the systems
        // pass has `K = JMODE`.
        let first = if long { 0 } else { jmode };
        let (areas, kctl): (Vec<usize>, usize) = if first > 0 {
            (vec![first], first)
        } else if state.kfx > 1 {
            (vec![first, state.kfx - 1], state.kfx - 1)
        } else {
            (vec![first], first)
        };
        let (mut fs, mut hs) = es_slots(&es_state);
        let mut geog = Geog::from_points(&params, &mags, &grounds);
        for &k in &areas {
            lecden(&mut state, k);
            let mut ion = genion(&state, k);
            let table = fobby(&ion, nang);
            alosfv(&state, k, &mut ion, &hour.layers);
            lp.areas[k].update(ion, &table);
        }
        setlng(&mut state, &mut fs, &mut hs, &mut geog, &mut lp.areas);
        let sd = sigdis(
            &set,
            &state,
            &hour,
            &lp.areas[jmode].ion,
            &glats,
            &clcks,
            jmode,
            geo.gcd_km,
        );
        geog.apply_sigdis(&sd);
        let ctx = PassCtx {
            state: &state,
            ants: &ants,
            fs: &fs,
            hs: &hs,
            geog: &geog,
            sig: &sd,
            deck,
            gcd: geo.gcd,
            gcdkm: geo.gcd_km,
            jmode,
            kctl,
            nang,
            long,
        };
        let (xluf, _frea) = luffy_luf(&mut lp, &ctx, &mut hour, &noise_for, deck.lufp);
        out.push(LufHour {
            gmt,
            lmt: times.lmt_tx,
            fot: hour.fot,
            hpf: hour.hpf,
            esmuf: hour.esmuf,
            allmuf: hour.allmuf,
            xluf,
        });
        fsecv_carry = state.fsecv;
    }
    Ok(out)
}

/// One hour's outputs: the MUF block and the thirteen `/SON/` slots
/// (slot 12 is the at-the-MUF column).
#[derive(Debug, Clone)]
pub struct HourPrediction {
    /// UT hour 1-24.
    pub gmt: R,
    pub allmuf: R,
    pub fot: R,
    pub hpf: R,
    pub angmuf: R,
    pub xluf: R,
    pub frel: [R; 12],
    pub son: [Son; 13],
    /// The `JLONG` flag after the hour: the listing's MODE row uses the
    /// long-path format when the last pass was the long model.
    pub long_model: bool,
}

/// Runs the full prediction for all 24 hours.
pub fn run(itshfbc: &Path, inp: &RunInputs) -> Result<Vec<HourPrediction>, String> {
    let pole = MagneticPole::for_tree(itshfbc);
    let geo = path_geometry(
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
        false,
        pole,
    );
    let mags: Vec<_> = geo.points.iter().map(|p| magvar(p.lat, p.lon)).collect();
    let set: CoefficientSet =
        redmap(itshfbc, inp.fof2, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let cof = cofion(&set);
    let grounds = ground_constants(&set, &geo.points, &mags);
    let _ = alatd(&geo.points);
    let clats: Vec<R> = geo.points.iter().map(|p| p.lat).collect();
    let glats: Vec<R> = geo.points.iter().map(|p| p.gmlat).collect();
    let psc = inp.psc;
    let nang = sang(geo.gcd_km, 0.1);
    let ants = build_antennas(itshfbc, inp)?;
    let deck = DeckParams {
        amind: 0.1,
        rsn: inp.required_snr_db,
        lufp: 90,
        pmp: 3.0,
        dmp: 0.1,
        method: inp.method,
    };
    let mut base_frel = [0.0 as R; 12];
    for (slot, f) in base_frel.iter_mut().zip(&inp.freqs_mhz) {
        *slot = *f;
    }
    let from_lon_rad = inp.from_lon_deg as R * D2R;
    let to_lon_rad = inp.to_lon_deg as R * D2R;
    let to_lat_rad = inp.to_lat_deg as R * D2R;

    let mut lp = ModeLoopState::default();
    let mut fsecv_carry = [0.0 as R; 3];
    let mut out = Vec::with_capacity(24);
    for jt in 1..=24i32 {
        let gmt = jt as R;
        let ab = virtim(&cof, &set.ikim, gmt);
        let params = layer_parameters(
            &set,
            &ab,
            &geo.points,
            &mags,
            inp.month,
            inp.ssn,
            gmt,
            &psc,
        );
        let es = esind(&set, &ab, &geo.points, &mags, &psc);
        let mut state = IonoState::from_layers(&params);
        state.fsecv = fsecv_carry;
        ionset(&mut state);
        let mut es_state = es.clone();
        let clcks: Vec<R> = params.iter().map(|p| p.clck).collect();
        let hour = curmuf(
            &mut state,
            &mut es_state,
            &set.f2d,
            &clats,
            &clcks,
            geo.gcd,
            geo.gcd_km,
            deck.amind,
            inp.ssn,
        );
        let times = geotim(jt, 1, from_lon_rad, to_lon_rad);
        let an = anois1(&set, times.gmtr, to_lat_rad, to_lon_rad, inp.to_lon_deg as R);
        let fof2_end = state.fi[state.kfx - 1][2];
        let noise_for = |f: R| {
            let reff = ants.gain(2, 0.0, f).1;
            genois(reff, &set, &an, f, to_lat_rad, fof2_end, inp.noise_dbw)
        };

        // The LUFFY passes. Card method 30 is the only one `DECRED`
        // gives `MSPEC = 121`, so it is the only one that runs both
        // models between 7000 and 10000 km and blends them. Method 21
        // forces the long model at any distance and method 22 the
        // short one; every other systems method takes the short model
        // below `GCDLNG` and the long one at or beyond it.
        let jmode = selmod(&state);
        struct PassPlan {
            long: bool,
            areas: Vec<usize>,
        }
        let long_areas = |kfx: usize| -> Vec<usize> {
            if kfx > 1 {
                vec![0, kfx - 1]
            } else {
                vec![0]
            }
        };
        let plans: Vec<PassPlan> = if inp.method != 30 {
            let long = match inp.method {
                21 => true,
                22 | 25 => false,
                _ => geo.gcd_km >= 10000.0,
            };
            vec![PassPlan {
                long,
                areas: if long {
                    long_areas(state.kfx)
                } else {
                    vec![jmode]
                },
            }]
        } else if geo.gcd_km > 10000.0 {
            vec![PassPlan {
                long: true,
                areas: if state.kfx > 1 {
                    vec![0, state.kfx - 1]
                } else {
                    vec![0]
                },
            }]
        } else if geo.gcd_km >= 7000.0 {
            let mut p2 = Vec::new();
            if jmode != 0 {
                p2.push(0);
            }
            if state.kfx > 1 && jmode != state.kfx - 1 {
                p2.push(state.kfx - 1);
            }
            vec![
                PassPlan {
                    long: false,
                    areas: vec![jmode],
                },
                PassPlan {
                    long: true,
                    areas: p2,
                },
            ]
        } else {
            vec![PassPlan {
                long: false,
                areas: vec![jmode],
            }]
        };
        let (mut fs, mut hs) = es_slots(&es_state);
        let mut geog = Geog::from_points(&params, &mags, &grounds);
        let mut hour_m = hour.clone();
        let mut saves = HourSaves::default();
        let mut frel = base_frel;
        frel[11] = hour.allmuf;
        let mut sd_last: Option<SignalDistribution> = None;
        for plan in &plans {
            for &k in &plan.areas {
                lecden(&mut state, k);
                let mut ion = genion(&state, k);
                let table = fobby(&ion, nang);
                alosfv(&state, k, &mut ion, &hour.layers);
                lp.areas[k].update(ion, &table);
            }
            setlng(&mut state, &mut fs, &mut hs, &mut geog, &mut lp.areas);
            sd_last = Some(sigdis(
                &set,
                &state,
                &hour,
                &lp.areas[jmode].ion,
                &glats,
                &clcks,
                jmode,
                geo.gcd_km,
            ));
            let sd = sd_last.as_ref().expect("just set");
            geog.apply_sigdis(sd);
            let ctx = PassCtx {
                state: &state,
                ants: &ants,
                fs: &fs,
                hs: &hs,
                geog: &geog,
                sig: sd,
                deck,
                gcd: geo.gcd,
                gcdkm: geo.gcd_km,
                jmode,
                // The systems passes end their area chain at `JMODE`;
                // only the short LUF pass differs.
                kctl: jmode,
                nang,
                long: plan.long,
            };
            luffy_freq_loop(&mut lp, &ctx, &mut hour_m, &noise_for, &frel, &mut saves);
        }
        if plans.len() == 2 {
            let sd = sd_last.as_ref().expect("two passes ran");
            let ctx = PassCtx {
                state: &state,
                ants: &ants,
                fs: &fs,
                hs: &hs,
                geog: &geog,
                sig: sd,
                deck,
                gcd: geo.gcd,
                gcdkm: geo.gcd_km,
                jmode,
                // The systems passes end their area chain at `JMODE`;
                // only the short LUF pass differs.
                kctl: jmode,
                nang,
                long: true,
            };
            luffy_smooth(&mut lp, &ctx, &noise_for, &frel, &saves);
        }
        let last_long = plans.last().map(|p| p.long).unwrap_or(false);
        let xluf = setluf(&lp.son, &frel, deck.lufp);
        outbod_sentinels(&mut lp.son, hour.allmuf);
        out.push(HourPrediction {
            gmt,
            allmuf: hour.allmuf,
            fot: hour.fot,
            hpf: hour.hpf,
            angmuf: hour.angmuf,
            xluf,
            frel,
            son: lp.son,
            // `JLONG`, as the last pass left it: the MODE row prints
            // hop count and layer for the short model, and the two end
            // layers for the long one.
            long_model: last_long,
        });
        fsecv_carry = state.fsecv;
    }
    Ok(out)
}

// ---------------------------------------------------------------------
// The listing body, formatted like OUTBOD

/// Fortran `F5.1`: width 5, one decimal, asterisks on overflow.
fn f5_1(v: R) -> String {
    let s = format!("{:5.1}", f64::from(v));
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// Fortran `F5.2`.
fn f5_2(v: R) -> String {
    let s = format!("{:5.2}", f64::from(v));
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// Fortran `ANINT` then `I5`: round half away from zero.
fn i5(v: R) -> String {
    let s = format!("{:5}", v.round() as i64);
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// The two-character `LAYTYP` label for a layer index (0 keeps the
/// program-start NUL bytes, 6 is `OUTBOD`'s "NA" sentinel).
fn laytyp(layer: i32) -> &'static str {
    match layer {
        1 => " E",
        2 => "F1",
        3 => "F2",
        4 => "ES",
        5 => " N",
        6 => "NA",
        _ => "\u{0}\u{0}",
    }
}

/// One data row: six spaces, the at-the-MUF field, `jfreq` slot fields,
/// dashes to eleven slots, then the six-character label.
fn row(label: &str, muf_field: String, fields: Vec<String>, jfreq: i32) -> String {
    let mut line = String::from("      ");
    line.push_str(&muf_field);
    for f in &fields {
        line.push_str(f);
    }
    for _ in fields.len()..11 {
        line.push_str("   - "); // 1X + NDASH ('  - ')
    }
    line.push(' ');
    line.push_str(label);
    let _ = jfreq;
    line
}

/// `SETOUT`'s body-line selection: which of `OUTBOD2`'s 22 lines the
/// method prints, in the order `OUTBOD` prints them. `DECRED` rewrites
/// card method 30 to method 20 first, so both select the same 21 lines.
///
/// Method 23 selects nothing here: its lines come from `TOPLINES` and
/// `BOTLINES` cards, and with no cards `SETOUT` leaves every line off.
pub fn body_lines(method: u32, botlines: Option<&[u32]>) -> Vec<usize> {
    // A `BOTLINES` card overrides the method's own selection, for any
    // method and not only 23: `SETOUT`'s jump past that block is
    // commented out, so it applies to whatever ran before it. The
    // lines then print in the order the card lists them rather than in
    // numeric order, because `OUTBOD` walks the card for this path.
    if let Some(card) = botlines {
        return card.iter().filter(|l| **l > 0).map(|l| *l as usize).collect();
    }
    let method = if method == 30 { 20 } else { method };
    let mut bot = Vec::new();
    match method {
        // The three methods that do not print consecutive lines.
        17 => bot.extend([1, 2, 5, 7, 10, 12]),
        18 => bot.extend([1, 2, 5, 7, 10, 14]),
        24 => bot.push(12),
        // Method 23 prints what its `TOPLINES` and `BOTLINES` cards
        // say, and nothing without them.
        23 => {}
        _ => {
            let nbod = match method {
                16 => 13,
                19 => 5,
                20..=22 => 21,
                25 => 22,
                // Every other method prints the mode line alone; the
                // ones that print nothing here have their own output
                // routine instead of `OUTBOD`.
                _ => 1,
            };
            bot.extend(1..=nbod);
        }
    }
    bot
}

/// Renders the hours as the listing body `OUTBOD` prints: the FREQ line
/// and the rows `lines` names, in that order, for comparison via
/// `listing::parse_listing`. [`body_lines`] gives a method's selection.
pub fn listing_text(hours: &[HourPrediction], lines: &[usize]) -> String {
    let mut out = String::new();
    for h in hours {
        // The FREQ line: hour, the MUF, the eleven card frequencies.
        let mut line = format!("  {:4.1}", f64::from(h.gmt));
        line.push_str(&f5_1(h.allmuf));
        for f in &h.frel[..11] {
            line.push_str(&f5_1(*f));
        }
        line.push_str(" FREQ");
        out.push_str(&line);
        out.push('\n');

        // JFREQ: the last slot with a mode, bounded by the last slot
        // with a frequency; nothing prints when no slot has a mode.
        let mut ifreq = 1usize;
        for ifq in 2..=11 {
            if h.frel[ifq - 1] > 0.0 {
                ifreq = ifq;
            }
        }
        let mut jfreq: i32 = -1;
        for ifq in 1..=ifreq {
            if h.son[ifq - 1].nhp > 0 {
                jfreq = ifq as i32;
            }
        }
        let jfreq = jfreq.min(12);
        if jfreq <= 0 {
            continue;
        }
        let slots: Vec<usize> = (0..jfreq as usize).collect();
        let muf = &h.son[11];

        // MODE, line 1: the short model prints hop count and layer,
        // the long one the layer at each end.
        let mode_field = |s: &Son| {
            if h.long_model {
                format!(" {}{}", laytyp(s.mode_layer), laytyp(s.moder_layer))
            } else {
                format!(" {:2}{}", s.nhp, laytyp(s.mode_layer))
            }
        };

        // `OUTBOD2` lines 2 to 22, in its order and formats.
        type Field = (&'static str, fn(&Son) -> String);
        let rows: [Field; 21] = [
            ("TANGLE", |s| f5_1(s.angle)),
            ("DELAY ", |s| f5_1(s.delay)),
            ("V HITE", |s| i5(s.vhigh)),
            ("MUFday", |s| f5_2(s.cprob)),
            ("LOSS  ", |s| i5(s.dblos)),
            ("DBU   ", |s| i5(s.dbu)),
            ("S DBW ", |s| i5(s.dbw)),
            ("N DBW ", |s| i5(s.xnynois + s.rneff)),
            ("SNR   ", |s| i5(s.sndb)),
            ("RPWRG ", |s| i5(s.snpr)),
            ("REL   ", |s| f5_2(s.reliab)),
            ("MPROB ", |s| f5_2(s.probmp)),
            ("S PRB ", |s| f5_2(s.sprob)),
            ("SIG LW", |s| f5_1(s.dblosl)),
            ("SIG UP", |s| f5_1(s.dblosu)),
            ("SNR LW", |s| f5_1(s.snrlw)),
            ("SNR UP", |s| f5_1(s.snrup)),
            ("TGAIN ", |s| f5_1(s.gaint)),
            ("RGAIN ", |s| f5_1(s.gainr)),
            ("SNRxx ", |s| i5(s.snxx)),
            ("DBM   ", |s| i5(s.dbw + 30.0)),
        ];
        for &line in lines {
            // `OUTBOD2` dispatches on the line number with a computed
            // GO TO of 22 labels. A `BOTLINES` card may name a line
            // past those, and `SETOUT` lets values up to 25 through:
            // the jump then falls through to the statement after it,
            // which is the MODE line.
            let numeric = (2..=22).contains(&line);
            let (label, field) = if numeric {
                rows[line - 2]
            } else {
                ("MODE  ", (|_: &Son| String::new()) as fn(&Son) -> String)
            };
            let fields: Vec<String> = if numeric {
                slots.iter().map(|&i| field(&h.son[i])).collect()
            } else {
                slots.iter().map(|&i| mode_field(&h.son[i])).collect()
            };
            let muf_field = if numeric {
                field(muf)
            } else {
                mode_field(muf)
            };
            out.push_str(&row(label, muf_field, fields, jfreq));
            out.push('\n');
            // The long path prints its reception angle after TANGLE as
            // RANGLE; the comparison surface ignores it, so it is not
            // rendered here.
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_match_the_fortran_edit_descriptors() {
        assert_eq!(f5_1(12.34), " 12.3");
        assert_eq!(f5_1(-999.0), "*****");
        assert_eq!(f5_2(0.999), " 1.00");
        assert_eq!(i5(-998.6), " -999");
        assert_eq!(i5(2.5), "    3"); // ANINT rounds half away from zero
    }

    #[test]
    fn setout_selects_the_lines_each_method_prints() {
        let on = |m: u32| body_lines(m, None);
        // Card method 30 is method 20 after DECRED rewrites it.
        assert_eq!(on(30), on(20));
        assert_eq!(on(20), (1..=21).collect::<Vec<_>>());
        assert_eq!(on(16), (1..=13).collect::<Vec<_>>());
        assert_eq!(on(17), vec![1, 2, 5, 7, 10, 12]);
        assert_eq!(on(18), vec![1, 2, 5, 7, 10, 14]);
        assert_eq!(on(19), (1..=5).collect::<Vec<_>>());
        assert_eq!(on(24), vec![12]);
        assert_eq!(on(25), (1..=22).collect::<Vec<_>>());
        // Method 23 prints what its TOPLINES and BOTLINES cards say,
        // and nothing without them.
        assert!(on(23).is_empty());
        // A BOTLINES card overrides any method, in the card's order.
        assert_eq!(body_lines(30, Some(&[12, 2, 0, 5])), vec![12, 2, 5]);
    }

    #[test]
    fn listing_lines_have_the_parser_geometry() {
        let mut son = [Son::default(); 13];
        son[0].nhp = 1;
        son[0].mode_layer = 3;
        son[0].reliab = 0.87;
        let h = HourPrediction {
            gmt: 1.0,
            allmuf: 12.3,
            fot: 10.0,
            hpf: 14.0,
            angmuf: 5.0,
            xluf: 7.0,
            frel: [7.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 12.3],
            son,
            long_model: false,
        };
        let text = listing_text(&[h], &body_lines(30, None));
        let parsed = crate::listing::parse_listing(&text);
        let rel: Vec<_> = parsed
            .numeric
            .iter()
            .filter(|s| s.row == "REL" && s.slot == 0)
            .collect();
        assert_eq!(rel.len(), 1);
        assert!((rel[0].value - 0.87).abs() < 1e-9);
        let muf: Vec<_> = parsed.numeric.iter().filter(|s| s.row == "MUF").collect();
        assert_eq!(muf.len(), 1);
        assert!((muf[0].value - 12.3).abs() < 1e-9);
        let modes: Vec<_> = parsed.modes.iter().filter(|m| m.slot == 0).collect();
        assert_eq!(modes[0].mode, "1F2");
    }
}
