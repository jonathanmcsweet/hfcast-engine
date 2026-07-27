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

use super::area::{pwrcut, xlimit6, Grid, Projection};
use super::coefficients::{redmap, CoefficientSet, FoF2Model};
use super::con::{MagneticPole, D2R, R, R2D};
use super::geometry::{path_geometry, PathGeometry};
use super::ionogram::{alosfv, fobby, genion, sang, selmod};
use super::ionosphere::{
    alatd, cofion, esind, geotim, ground_constants, layer_parameters, virtim,
};
use super::antenna::{
    area_table, dazel0, point_to_point_table, read_antenna, AntennaEnd, AntennaSet, AntennaSetup,
    Installation,
};
use super::magnetic::{magvar, MagneticVars};
use super::modes::{
    es_slots, luffy_freq_loop, luffy_luf, luffy_smooth, outbod_sentinels, setlng, setluf,
    DeckParams, Geog, HourSaves, ModeLoopState, PassCtx, Son,
};
use super::muf::{curmuf, ionset, lecden, nommuf, IonoState};
use super::noise::{anois1, genois};
use super::output::AntennaLine;
use super::sigdis::{sigdis, SignalDistribution};

/// One `ANTENNA` card's fields, minus the end it serves.
#[derive(Debug, Clone)]
pub struct AntennaCardSpec {
    /// Path under `<itshfbc>/antennas`, e.g. `samples/sample.21`.
    pub file: String,
    pub design_freq: R,
    pub beam_deg: R,
    /// The card's frequency range in whole MHz. `GAIN` takes the first
    /// card serving the end whose range holds the frequency, so several
    /// cards split the bands between them and a frequency in no card's
    /// range gets no antenna.
    pub min_freq: i32,
    pub max_freq: i32,
    /// The card's last field: kilowatts on a transmit card, and on a
    /// receive card a gain that replaces the design frequency when it is
    /// not zero.
    pub power_field: R,
}

impl AntennaCardSpec {
    /// The default card every prediction used before antennas were
    /// wired in.
    pub fn isotrope(power_field: R) -> Self {
        Self {
            file: "default/isotrope".to_string(),
            design_freq: 0.0,
            beam_deg: 0.0,
            min_freq: 2,
            max_freq: 30,
            power_field,
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
    /// Whether the deck's `FPROB` card leaves sporadic E on. Kept for
    /// callers that describe a case that way; the engine reads
    /// [`RunInputs::psc`].
    pub sporadic_e: bool,
    /// The `ANTENNA` cards at each end, in card order. There is no
    /// separate transmit power: the deck carries it on the transmit card,
    /// and `PWRDB` reads it from the card matching the frequency.
    pub tx_antennas: Vec<AntennaCardSpec>,
    pub rx_antennas: Vec<AntennaCardSpec>,
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
            sporadic_e: c.sporadic_e,
            // The same resolved card list the deck text is written from,
            // so the two descriptions of one case cannot disagree.
            tx_antennas: card_specs(c, 1),
            rx_antennas: card_specs(c, 2),
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

/// The `ANTENNA` cards of one end, as the engine takes them.
///
/// [`DeckCase::antenna_cards`] resolves the defaults — an empty list is
/// one isotrope, and a card without a last field takes the deck's
/// power — so the deck text and this list are written from one decision.
fn card_specs(c: &DeckCase, iat: i32) -> Vec<AntennaCardSpec> {
    c.antenna_cards()
        .into_iter()
        .filter(|(end, _)| *end == iat)
        .map(|(_, card)| AntennaCardSpec {
            file: card.file,
            design_freq: card.design_freq as R,
            beam_deg: card.beam_deg as R,
            min_freq: card.min_freq,
            max_freq: card.max_freq,
            power_field: card.last_field.unwrap_or(0.0) as R,
        })
        .collect()
}

/// `ANTCALC` for one run: computes every card's gain table from its
/// definition file and installs them as `DECRED` reads them back.
///
/// The order is every transmit card then every receive card, which is how
/// the deck numbers them and therefore the order `GAIN` searches. Within
/// one end the first card whose frequency range holds the frequency wins,
/// so overlapping cards are resolved by position and a frequency in no
/// card's range gets no antenna at all.
fn build_antennas(itshfbc: &Path, inp: &RunInputs) -> Result<AntennaSet, String> {
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
    let ends = [
        (1, AntennaEnd::Transmit, taz, &inp.tx_antennas),
        (2, AntennaEnd::Receive, raz, &inp.rx_antennas),
    ];
    let mut ants = AntennaSet::default();
    for (iat, end, azimuth_deg, cards) in ends {
        for card in cards {
            let file = read_antenna(itshfbc, &card.file)?;
            let table = point_to_point_table(&AntennaSetup {
                file: &file,
                end,
                min_freq: card.min_freq,
                max_freq: card.max_freq,
                design_freq: card.design_freq,
                beam_deg: card.beam_deg,
                power_field: card.power_field,
                azimuth_deg,
            })
            .map_err(|e| e.to_string())?;
            // Only a transmit card carries power; the receive card's
            // last field is a gain and never becomes a `pwrdba`.
            let kw = if iat == 1 { card.power_field } else { 0.0 };
            ants.install(Installation {
                iat,
                min_freq: card.min_freq,
                max_freq: card.max_freq,
                table,
                power_kw: kw,
                file: card.file.clone(),
            });
        }
    }
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

/// Everything one hour of the prediction reads that the hour loop does
/// not change: the loaded maps, the path, and the antenna tables.
///
/// [`run`] builds this once and walks 24 hours; an area run builds it per
/// grid point and asks for one hour. Extracting it is what lets both
/// callers execute the same hour body rather than two copies that can
/// drift apart.
struct HourSetup<'a> {
    set: &'a CoefficientSet,
    cof: Vec<R>,
    geo: PathGeometry,
    mags: Vec<MagneticVars>,
    grounds: Vec<(R, R)>,
    clats: Vec<R>,
    glats: Vec<R>,
    nang: usize,
    ants: AntennaSet,
    deck: DeckParams,
    base_frel: [R; 12],
    from_lon_rad: R,
    to_lon_rad: R,
    to_lat_rad: R,
    to_lon_deg: R,
    psc: [R; 4],
    month: u32,
    ssn: R,
    noise_dbw: i32,
    method: u32,
    /// Whether the area driver's own comparison applies: `HFAREA` tests
    /// the path length against `GCDLNG` with `.GT.` where `HFMUFS` uses
    /// `.GE.`, so a path of exactly 10000 km takes the short model in an
    /// area run and the long one point to point.
    area: bool,
}

/// Builds the per-path half of a run: geometry, magnetic field, ground
/// constants and the antenna tables, against maps already loaded.
fn hour_setup<'a>(
    itshfbc: &Path,
    inp: &RunInputs,
    set: &'a CoefficientSet,
    ants: Option<AntennaSet>,
) -> Result<HourSetup<'a>, String> {
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
    let cof = cofion(set);
    let grounds = ground_constants(set, &geo.points, &mags);
    let _ = alatd(&geo.points);
    let clats: Vec<R> = geo.points.iter().map(|p| p.lat).collect();
    let glats: Vec<R> = geo.points.iter().map(|p| p.gmlat).collect();
    let nang = sang(geo.gcd_km, 0.1);
    // An area run computes its antennas once for the whole grid, so the
    // caller may pass them in rather than have every point rebuild them.
    let ants = match ants {
        Some(a) => a,
        None => build_antennas(itshfbc, inp)?,
    };
    let mut base_frel = [0.0 as R; 12];
    for (slot, f) in base_frel.iter_mut().zip(&inp.freqs_mhz) {
        *slot = *f;
    }
    Ok(HourSetup {
        set,
        cof,
        geo,
        mags,
        grounds,
        clats,
        glats,
        nang,
        ants,
        deck: DeckParams {
            amind: 0.1,
            rsn: inp.required_snr_db,
            lufp: 90,
            pmp: 3.0,
            dmp: 0.1,
            method: inp.method,
        },
        base_frel,
        from_lon_rad: inp.from_lon_deg as R * D2R,
        to_lon_rad: inp.to_lon_deg as R * D2R,
        to_lat_rad: inp.to_lat_deg as R * D2R,
        to_lon_deg: inp.to_lon_deg as R,
        psc: inp.psc,
        month: inp.month,
        ssn: inp.ssn,
        noise_dbw: inp.noise_dbw,
        method: inp.method,
        area: false,
    })
}

/// What the listing header says about the path and the antennas.
///
/// The engine works all of it out on the way to a prediction — the
/// bearings and the path length in `GEOM`, each card's model label in
/// `ANTCALC` — so it comes back with the hours rather than being
/// recomputed by whoever prints the header.
#[derive(Debug, Clone)]
pub struct PathReport {
    /// The receive latitude as `GEOM` leaves it.
    pub rlatd: R,
    /// Bearings each way, degrees.
    pub btrd: R,
    pub brtd: R,
    pub gcd_km: R,
    /// The cards in slot order: every transmit card, then every receive
    /// card.
    pub antennas: Vec<AntennaLine>,
}

/// A whole run: the hours and what the header needs.
#[derive(Debug, Clone)]
pub struct Prediction {
    pub hours: Vec<HourPrediction>,
    pub path: PathReport,
}

/// Runs the full prediction for all 24 hours, with the header's path and
/// antenna description.
pub fn run_listing(itshfbc: &Path, inp: &RunInputs) -> Result<Prediction, String> {
    let set: CoefficientSet =
        redmap(itshfbc, inp.fof2, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let s = hour_setup(itshfbc, inp, &set, None)?;
    let path = PathReport {
        rlatd: s.geo.rlatd,
        btrd: s.geo.btr_deg(),
        brtd: s.geo.brt_deg(),
        gcd_km: s.geo.gcd_km,
        antennas: AntennaLine::from_set(&s.ants),
    };
    let mut lp = ModeLoopState::default();
    let mut fsecv_carry = [0.0 as R; 3];
    let mut hours = Vec::with_capacity(24);
    for jt in 1..=24i32 {
        hours.push(hour_body(&s, jt, &mut lp, &mut fsecv_carry));
    }
    Ok(Prediction { hours, path })
}

/// Runs the full prediction for all 24 hours.
pub fn run(itshfbc: &Path, inp: &RunInputs) -> Result<Vec<HourPrediction>, String> {
    run_listing(itshfbc, inp).map(|p| p.hours)
}

/// Runs one hour on its own, from the program-start state.
///
/// This is what an area run needs: it computes only the hour its input
/// file names. Taking one hour out of [`run`]'s output would be a
/// different computation, because `FSECV` carries from each hour into the
/// next — hour 18 of a 24-hour run starts from hour 17's value where a
/// single-hour run starts from zero.
pub fn run_hour(itshfbc: &Path, inp: &RunInputs, jt: i32) -> Result<HourPrediction, String> {
    let set: CoefficientSet =
        redmap(itshfbc, inp.fof2, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let s = hour_setup(itshfbc, inp, &set, None)?;
    let mut lp = ModeLoopState::default();
    let mut fsecv = [0.0 as R; 3];
    Ok(hour_body(&s, jt, &mut lp, &mut fsecv))
}

/// One hour of `HFMUFS`: the MUF, the LUFFY passes with the smoothing
/// blend, `SETLUF` and `OUTBOD`'s sentinels.
///
/// `lp` and `fsecv` are the state the Fortran keeps in COMMON between
/// hours. They are arguments rather than locals because the hour loop's
/// answers depend on them: several blocks are read stale by design.
fn hour_body(
    s: &HourSetup,
    jt: i32,
    lp: &mut ModeLoopState,
    fsecv_carry: &mut [R; 3],
) -> HourPrediction {
    let (set, geo, ants, deck, psc) = (s.set, &s.geo, &s.ants, s.deck, s.psc);
    {
        let gmt = jt as R;
        let ab = virtim(&s.cof, &set.ikim, gmt);
        let params = layer_parameters(
            set,
            &ab,
            &geo.points,
            &s.mags,
            s.month,
            s.ssn,
            gmt,
            &psc,
        );
        let es = esind(set, &ab, &geo.points, &s.mags, &psc);
        let mut state = IonoState::from_layers(&params);
        state.fsecv = *fsecv_carry;
        ionset(&mut state);
        let mut es_state = es.clone();
        let clcks: Vec<R> = params.iter().map(|p| p.clck).collect();
        let hour = curmuf(
            &mut state,
            &mut es_state,
            &set.f2d,
            &s.clats,
            &clcks,
            geo.gcd,
            geo.gcd_km,
            deck.amind,
            s.ssn,
        );
        let times = geotim(jt, 1, s.from_lon_rad, s.to_lon_rad);
        let an = anois1(set, times.gmtr, s.to_lat_rad, s.to_lon_rad, s.to_lon_deg);
        let fof2_end = state.fi[state.kfx - 1][2];
        let noise_for = |f: R| {
            let reff = ants.gain(2, 0.0, f).1;
            genois(reff, set, &an, f, s.to_lat_rad, fof2_end, s.noise_dbw)
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
        let plans: Vec<PassPlan> = if s.method != 30 {
            let long = match s.method {
                21 => true,
                22 | 25 => false,
                // `HFAREA` compares with `.GT.`, `HFMUFS` with `.GE.`.
                _ if s.area => geo.gcd_km > 10000.0,
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
        let mut geog = Geog::from_points(&params, &s.mags, &s.grounds);
        let mut hour_m = hour.clone();
        let mut saves = HourSaves::default();
        let mut frel = s.base_frel;
        frel[11] = hour.allmuf;
        let mut sd_last: Option<SignalDistribution> = None;
        for plan in &plans {
            for &k in &plan.areas {
                lecden(&mut state, k);
                let mut ion = genion(&state, k);
                let table = fobby(&ion, s.nang);
                alosfv(&state, k, &mut ion, &hour.layers);
                lp.areas[k].update(ion, &table);
            }
            setlng(&mut state, &mut fs, &mut hs, &mut geog, &mut lp.areas);
            sd_last = Some(sigdis(
                set,
                &state,
                &hour,
                &lp.areas[jmode].ion,
                &s.glats,
                &clcks,
                jmode,
                geo.gcd_km,
            ));
            let sd = sd_last.as_ref().expect("just set");
            geog.apply_sigdis(sd);
            let ctx = PassCtx {
                state: &state,
                ants,
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
                nang: s.nang,
                long: plan.long,
            };
            luffy_freq_loop(lp, &ctx, &mut hour_m, &noise_for, &frel, &mut saves);
        }
        if plans.len() == 2 {
            let sd = sd_last.as_ref().expect("two passes ran");
            let ctx = PassCtx {
                state: &state,
                ants,
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
                nang: s.nang,
                long: true,
            };
            luffy_smooth(lp, &ctx, &noise_for, &frel, &saves);
        }
        let last_long = plans.last().map(|p| p.long).unwrap_or(false);
        let xluf = setluf(&lp.son, &frel, deck.lufp);
        outbod_sentinels(&mut lp.son, hour.allmuf);
        *fsecv_carry = state.fsecv;
        HourPrediction {
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
        }
    }
}

// ---------------------------------------------------------------------
// Area coverage: the grid loop and OUTAREA's row

/// An area run's inputs: the grid, the transmitter at its centre, and the
/// one hour and frequency set the run asks for.
#[derive(Debug, Clone)]
pub struct AreaInputs {
    pub grid: Grid,
    /// Transmitter, degrees. The distributed file puts it at the grid
    /// centre, but the two are separate fields.
    pub tx_lat_deg: f64,
    pub tx_lon_deg: f64,
    pub month: u32,
    pub ssn: R,
    /// The hour the input file names, 1-24. An area run computes one.
    pub hour: i32,
    pub freqs_mhz: Vec<R>,
    pub required_snr_db: R,
    pub noise_dbw: i32,
    pub watts: R,
    pub psc: [R; 4],
    pub method: u32,
    pub fof2: FoF2Model,
    /// Inverse coverage: the grid supplies the **transmitter** and
    /// `tx_lat_deg` and `tx_lon_deg` become the fixed receiver. The input
    /// file still calls that end `Transmit`, and so do these fields,
    /// because `HFAREA` swaps the roles rather than the file.
    pub inverse: bool,
    /// `None` is the isotrope card at that end. An area run has one card
    /// per end: the input file has one line for each, and the reference's
    /// area gain table holds two antennas.
    pub tx_antenna: Option<AntennaCardSpec>,
    pub rx_antenna: Option<AntennaCardSpec>,
}

/// One grid point's output row: the indices, the coordinates and
/// `OUTAREA`'s value columns already rendered in its formats.
#[derive(Debug, Clone)]
pub struct AreaPoint {
    pub ix: usize,
    pub iy: usize,
    pub lat: R,
    /// The receiver's longitude, as the prediction used it: folded into
    /// 0 to 360, which is how `GRIDXY` returns it.
    pub lon: R,
    /// The longitude `OUTAREA` prints, which under the latitude and
    /// longitude projection is the same meridian unfolded past zero.
    pub print_lon: R,
    /// The 24 six-character fields, in `OUTAREA`'s order.
    pub fields: Vec<String>,
}

impl AreaPoint {
    /// The row as `OUTAREA` writes it: `(2i3,2f10.4,24a6)`.
    pub fn row(&self) -> String {
        format!(
            "{:3}{:3}{}{}{}",
            self.ix,
            self.iy,
            f_fixed(self.lat, 10, 4),
            f_fixed(self.print_lon, 10, 4),
            self.fields.concat()
        )
    }
}

/// `OUTAREA`'s longitude for one row.
///
/// `GRIDXY` folds every longitude into 0 to 360, but a grid described in
/// degrees with a negative western edge reads better unfolded, so under
/// the latitude and longitude projection the print step subtracts 360
/// again — from the first column, and from any value past 180. It is a
/// rendering adjustment and not a different mesh: the prediction used the
/// folded value.
///
/// The pole is the case that needs the last line. `HFAREA` forces the
/// longitude to zero within a tenth of a degree of either pole, so the
/// first column there would print -360; the source answers zero, with a
/// comment saying the pole probably caused it.
fn print_longitude(grid: &Grid, ix: usize, lon: R) -> R {
    let mut out = lon;
    if grid.projection != Projection::GreatCircle && grid.xmin < 0.0 && (ix == 1 || out > 180.0) {
        out -= 360.0;
    }
    if out < -359.0 {
        out = 0.0;
    }
    out
}

/// Fortran fixed-point editing as the reference's own build renders it.
///
/// Every source file is compiled with `-fno-sign-zero`, so a negative
/// value that rounds to zero in its field prints without a minus sign —
/// a latitude of -1.6e-10 in an `F10.4` field is `0.0000`, not `-0.0000`.
/// The listing comparisons could never see this, because they parse the
/// numbers back and `-0.0` equals `0.0`.
pub fn f_fixed(v: R, width: usize, decimals: usize) -> String {
    let s = format!("{:w$.d$}", f64::from(v), w = width, d = decimals);
    if s.bytes().any(|b| b.is_ascii_digit() && b != b'0') {
        s
    } else {
        format!("{:w$.d$}", f64::from(v).abs(), w = width, d = decimals)
    }
}

/// Fortran `F6.i` with `XLIMIT6`'s clamp, which is how every `OUTAREA`
/// value column but the MUF is written.
fn f6(v: R, decimals: usize) -> String {
    let s = f_fixed(xlimit6(v, decimals), 6, decimals);
    if s.len() > 6 {
        "******".to_string()
    } else {
        s
    }
}

/// Runs an area coverage grid: the same one-hour prediction at every grid
/// point, in `HFAREA`'s own point order.
///
/// The mode-loop state and `FSECV` carry from one grid point to the next,
/// exactly as they carry from hour to hour in a point-to-point run: the
/// Fortran keeps them in COMMON and `HFAREA` does not reset them between
/// points. Only the first point starts from the program-start zero.
pub fn run_area(itshfbc: &Path, area: &AreaInputs) -> Result<Vec<AreaPoint>, String> {
    let set: CoefficientSet =
        redmap(itshfbc, area.fof2, area.month, area.ssn).map_err(|e| e.to_string())?;
    let nf = area.freqs_mhz.iter().take_while(|f| **f != 0.0).count().max(1);
    let ants = build_area_antennas(itshfbc, area, nf)?;
    let mut lp = ModeLoopState::default();
    let mut fsecv = [0.0 as R; 3];
    let mut out = Vec::with_capacity(area.grid.nx * area.grid.ny);
    let (fixed_lat, fixed_lon) = (area.tx_lat_deg as R, area.tx_lon_deg as R);
    for iy in 1..=area.grid.ny {
        for ix in 1..=area.grid.nx {
            // The grid point is the receiver in a normal run and the
            // transmitter in an inverse one. Either way it is the point
            // the output row names.
            let (glon, glat) = if area.inverse {
                area.grid.transmitter(ix, iy, fixed_lat, fixed_lon)
            } else {
                area.grid.receiver(ix, iy, fixed_lat, fixed_lon)
            };
            let (from, to) = if area.inverse {
                ((glat, glon), (fixed_lat, fixed_lon))
            } else {
                ((fixed_lat, fixed_lon), (glat, glon))
            };
            let inp = RunInputs {
                from_lat_deg: f64::from(from.0),
                from_lon_deg: f64::from(from.1),
                to_lat_deg: f64::from(to.0),
                to_lon_deg: f64::from(to.1),
                month: area.month,
                ssn: area.ssn,
                freqs_mhz: area.freqs_mhz.clone(),
                required_snr_db: area.required_snr_db,
                noise_dbw: area.noise_dbw,
                sporadic_e: area.psc[3] != 0.0,
                // The antennas are built once for the whole grid and
                // passed to `hour_setup`, so this list stays empty.
                tx_antennas: Vec::new(),
                rx_antennas: Vec::new(),
                method: area.method,
                fof2: area.fof2,
                psc: area.psc,
            };
            let mut s = hour_setup(itshfbc, &inp, &set, Some(ants.clone()))?;
            // `HFAREA` compares against `GCDLNG` with `.GT.` where the
            // point-to-point driver uses `.GE.`. It matters only at
            // exactly 10000 km, but it is a real difference.
            s.area = true;
            // `GEOM` runs per grid point, and an area antenna is cut
            // along the bearings it leaves behind.
            s.ants.btrd = s.geo.btr * R2D;
            s.ants.brtd = s.geo.brt * R2D;
            if area.inverse {
                // `HFAREA` re-aims the transmit antenna at the fixed
                // station from each grid point, replacing whatever beam
                // the card asked for. It writes the first antenna slot,
                // which is the one the area lookup reads for the
                // transmitter. A multi-frequency inverse run is
                // unaffected: its table was already cut along one
                // bearing and no longer consults the beam.
                let (ztaz, _) = dazel0(glat, glon, fixed_lat, fixed_lon);
                if let Some(first) = s.ants.ants.first_mut() {
                    first.table.beam_main = ztaz;
                }
            }
            let h = hour_body(&s, area.hour, &mut lp, &mut fsecv);
            out.push(area_point(&area.grid, ix, iy, glat, glon, &h, nf));
        }
    }
    Ok(out)
}

/// `ANTCALC` for an area run: which of its two branches the run takes,
/// and both ends' tables.
///
/// A single-frequency run builds the 360-azimuth table and the prediction
/// re-cuts it per grid point. Several frequencies take the ordinary
/// point-to-point table instead — `ANTCALC` tests `freqarea(2)` before it
/// tests for area coverage — cut along one bearing for the whole grid:
/// the deck `AREAMAP` writes names the plot centre as the receiver, so
/// that is the bearing, whatever the grid point.
fn build_area_antennas(
    itshfbc: &Path,
    area: &AreaInputs,
    nf: usize,
) -> Result<AntennaSet, String> {
    let pwrkw = area.watts / 1000.0;
    let tx_iso = AntennaCardSpec::isotrope(pwrkw);
    let rx_iso = AntennaCardSpec::isotrope(0.0);
    let tx = area.tx_antenna.as_ref().unwrap_or(&tx_iso);
    let rx = area.rx_antenna.as_ref().unwrap_or(&rx_iso);
    let txf = read_antenna(itshfbc, &tx.file)?;
    let rxf = read_antenna(itshfbc, &rx.file)?;
    let centre = (area.grid.plat, area.grid.plon);
    let (taz, _) = dazel0(
        area.tx_lat_deg as R,
        area.tx_lon_deg as R,
        centre.0,
        centre.1,
    );
    let (raz, _) = dazel0(
        centre.0,
        centre.1,
        area.tx_lat_deg as R,
        area.tx_lon_deg as R,
    );
    let ends = [
        (1, tx, &txf, AntennaEnd::Transmit, taz),
        (2, rx, &rxf, AntennaEnd::Receive, raz),
    ];

    let mut ants = AntennaSet::default();
    for (iat, card, file, end, azimuth_deg) in ends {
        let setup = AntennaSetup {
            file,
            end,
            min_freq: card.min_freq,
            max_freq: card.max_freq,
            design_freq: card.design_freq,
            beam_deg: card.beam_deg,
            power_field: card.power_field,
            azimuth_deg,
        };
        let kw = if iat == 1 { pwrkw } else { 0.0 };
        let installed = |table| Installation {
            iat,
            min_freq: card.min_freq,
            max_freq: card.max_freq,
            table,
            power_kw: kw,
            file: card.file.clone(),
        };
        if nf > 1 {
            let table = point_to_point_table(&setup).map_err(|e| e.to_string())?;
            ants.install(installed(table));
        } else {
            let (header, table) =
                area_table(&setup, area.freqs_mhz[0]).map_err(|e| e.to_string())?;
            ants.install_area(installed(header), table)?;
        }
    }
    Ok(ants)
}

/// `OUTAREA`'s value columns for one grid point.
///
/// Six of them are the largest value over the run's frequencies rather
/// than the first frequency's: the reference walks the frequencies
/// overwriting slot 1, so slot 1 holds the maximum by the time it is
/// printed — and the power cut, which reads the same slot, sees the
/// maximised median against unmaximised decile deviations.
fn area_point(
    grid: &Grid,
    ix: usize,
    iy: usize,
    lat: R,
    lon: R,
    h: &HourPrediction,
    nf: usize,
) -> AreaPoint {
    let print_lon = print_longitude(grid, ix, lon);
    let s0 = &h.son[0];
    let (mut dbu, mut dbw, mut sndb) = (s0.dbu, s0.dbw, s0.sndb);
    let (mut reliab, mut sprob, mut snxx) = (s0.reliab, s0.sprob, s0.snxx);
    for s in h.son.iter().take(nf).skip(1) {
        if s.dbu > dbu {
            dbu = s.dbu;
        }
        if s.dbw > dbw {
            dbw = s.dbw;
        }
        if s.sndb > sndb {
            sndb = s.sndb;
        }
        if s.reliab > reliab {
            reliab = s.reliab;
        }
        if s.sprob > sprob {
            sprob = s.sprob;
        }
        if s.snxx > snxx {
            snxx = s.snxx;
        }
    }
    // `ANGLER` falls back to the transmit angle when it is not positive.
    let angr = if s0.angler <= 0.0 { s0.angle } else { s0.angler };
    if nf > 1 {
        // With more than one frequency `OUTAREA` prints seven columns
        // instead of twenty-four: the MUF and the six values that are
        // maxima over the frequencies.
        return AreaPoint {
            ix,
            iy,
            lat,
            lon,
            print_lon,
            fields: vec![
                f_fixed(h.frel[11], 6, 2),
                f6(dbu, 1),
                f6(dbw, 1),
                f6(sndb, 1),
                f6(reliab, 3),
                f6(sprob, 3),
                f6(snxx, 1),
            ],
        };
    }
    let mode = if h.long_model {
        format!("  {}{}", laytyp(s0.mode_layer), laytyp(s0.moder_layer))
    } else {
        format!("  {:2}{}", s0.nhp, laytyp(s0.mode_layer))
    };
    let fields = vec![
        f_fixed(h.frel[11], 6, 2),
        mode,
        f6(s0.angle, 2),
        f6(s0.delay, 2),
        f6(s0.vhigh, 1),
        f6(s0.cprob, 3),
        f6(s0.dblos, 1),
        f6(dbu, 1),
        f6(dbw, 1),
        f6(s0.xnynois + s0.rneff, 1),
        f6(sndb, 1),
        f6(s0.snpr, 1),
        f6(reliab, 3),
        f6(s0.probmp, 3),
        f6(sprob, 3),
        f6(s0.gaint, 2),
        f6(s0.gainr, 2),
        f6(snxx, 1),
        f6(s0.du_nois, 2),
        f6(s0.dl_nois, 2),
        f6(s0.dblosl, 2),
        f6(s0.dblosu, 2),
        f6(pwrcut(sndb, s0.snrlw, s0.snrup, 88.0, 91.0), 3),
        f6(angr, 2),
    ];
    AreaPoint {
        ix,
        iy,
        lat,
        lon,
        print_lon,
        fields,
    }
}

// ---------------------------------------------------------------------
// The listing body, formatted like OUTBOD

/// Fortran `F5.1`: width 5, one decimal, asterisks on overflow.
fn f5_1(v: R) -> String {
    let s = f_fixed(v, 5, 1);
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// Fortran `F5.2`.
fn f5_2(v: R) -> String {
    let s = f_fixed(v, 5, 2);
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
    hours.iter().map(|h| hour_block(h, lines).0).collect()
}

/// One hour of the body: the blank line `OUTBOD`'s format opens with,
/// the FREQ line, and the selected rows.
///
/// The flag is false when no slot has a mode. `OUTBOD` then returns
/// after the frequency line, so the hour prints nothing else.
pub fn hour_block(h: &HourPrediction, lines: &[usize]) -> (String, bool) {
    let mut out = String::new();
    {
        // The FREQ line, after the blank record its format opens with:
        // hour, the MUF, the eleven card frequencies.
        out.push('\n');
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
            return (out, false);
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
            // The long model prints a second angle line straight after
            // TANGLE: the angle at the receiving end. `OUTLIN` does not
            // count it, so a long-path page runs one line over the
            // limit for every hour on it.
            if line == 2 && h.long_model {
                let fields: Vec<String> =
                    slots.iter().map(|&i| f5_1(h.son[i].angler)).collect();
                out.push_str(&row("RANGLE", f5_1(muf.angler), fields, jfreq));
                out.push('\n');
            }
        }
    }
    (out, true)
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
