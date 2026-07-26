//! The LUFFY mode loop: raysets, per-mode losses, sporadic-E modes, the
//! long-path model and the combined reliability — `penang`, `findf`,
//! `fdist`, `inmuf`, `regmod`, `esmod`, `esreg`, `allmodes`, `relbil`,
//! `serprb`, `mpath`, `setlng`, `gain`, the long-path chain (`gmloss`,
//! `settxr`, `seltxr`, `lngpat`, `convh`, `gettop`, `tabs`, `babs`) and
//! the 7000-10000 km smoothing blend from the end of `luffy`.
//!
//! Scope notes, mirroring the source:
//! - `esreg` presets its two mode slots and then returns — the body after
//!   the preset is dead code behind an unconditional `RETURN` labelled
//!   "ERRORS IN THE CODE BELOW". Only the preset is ported.
//! - `settxr`/`seltxr` are the `_orig` variants (`version.w32` ends in W).
//! - Antennas are the isotrope: gain 0 dB and antenna efficiency 0 at
//!   every angle and frequency, so `GAIN` with `ITR > 0` reduces to the
//!   constants below and `PWRDB` to a single deck-derived value. Only the
//!   ground-reflection branch (`ITR < 0`) carries arithmetic.
//! - The LUF paths (`IPFG >= 300`, `ITRUN = 8`) are out of scope: method
//!   30 runs `ITRUN = 7`, which only ever calls `LUFFY(100)`/`LUFFY(200)`.
//!
//! Several `COMMON` blocks persist across hours and frequencies and are
//! read before being rewritten (`/SON/` after a frequency with no modes,
//! `/REFLX/` rows above the count a `findf` call wrote, `/ZON/` slots
//! without a mode). [`ModeLoopState`] models that persistence: one value
//! per case, zero-initialised like a static Fortran COMMON, carried
//! across the hour loop.

use super::con::{D2R, DCL, PIO2, R2D, RZ, VOFL, R};
use super::ionogram::{Ionogram, ANG};
use super::ionosphere::{EsParams, LayerParams};
use super::magnetic::MagneticVars;
use super::muf::{IonoState, MufHour};
use super::noise::NoiseResult;
use super::sigdis::{prbmuf, xlin, SignalDistribution};

/// `/LPATH/` constants from `blkdat`.
const DELOPT: R = 3.0;
const GMIN: R = 3.0;
const YMIN: R = 0.1;

/// Deck-level system parameters the mode loop needs.
#[derive(Debug, Clone, Copy)]
pub struct DeckParams {
    /// Minimum takeoff angle, degrees.
    pub amind: R,
    /// Required signal-to-noise ratio, dB.
    pub rsn: R,
    /// Required reliability, per cent (`LUFP`).
    pub lufp: i32,
    /// Multipath power tolerance, dB, and maximum tolerable time delay.
    pub pmp: R,
    pub dmp: R,
    /// The `METHOD` card's first field, before `DECRED` rewrites 30 to
    /// 20 with `MSPEC = 121`. `MPATH` is the only computation that
    /// reads it.
    pub method: u32,
}

/// The `/GEOG/` block: per-sample-area scalars (5 slots).
#[derive(Debug, Clone, Default)]
pub struct Geog {
    pub gyz: [R; 5],
    pub clck: [R; 5],
    pub abiy: [R; 5],
    pub artic: [R; 5],
    pub sigpat: [R; 5],
    pub epspat: [R; 5],
}

impl Geog {
    /// Fills the slots from the per-point stage outputs (before any
    /// `SETLNG` replication).
    pub fn from_points(
        layers: &[LayerParams],
        mags: &[MagneticVars],
        grounds: &[(R, R)],
    ) -> Self {
        let mut g = Geog::default();
        for (k, ((lp, mag), gr)) in layers.iter().zip(mags).zip(grounds).enumerate() {
            g.gyz[k] = mag.gyz;
            g.clck[k] = lp.clck;
            g.abiy[k] = lp.abiy;
            g.sigpat[k] = gr.0;
            g.epspat[k] = gr.1;
        }
        g
    }

    /// `SIGDIS` overwrites `ABIY(1..KFX)` and `ARTIC(1..KFX)` with its
    /// clamped values; slots above `KFX` keep what `SETLNG` left.
    pub fn apply_sigdis(&mut self, sig: &SignalDistribution) {
        for (k, (a, r)) in sig.abiy.iter().zip(&sig.artic).enumerate() {
            self.abiy[k] = *a;
            self.artic[k] = *r;
        }
    }
}

/// One sample area's ionogram tables and reflectrix integers, the
/// `/RON/` `HPRIM`..`AFAC` columns plus `/RAYS/` `IFOB`. Rows of `ifob`
/// beyond the hour's `nang` keep old values exactly as the COMMON does.
#[derive(Debug, Clone)]
pub struct AreaTables {
    pub ion: Ionogram,
    pub ifob: [[i32; 30]; 40],
}

impl Default for AreaTables {
    fn default() -> Self {
        AreaTables {
            ion: Ionogram {
                fvert: [0.0; 30],
                hprim: [0.0; 30],
                htrue: [0.0; 30],
                afac: [0.0; 30],
            },
            ifob: [[0; 30]; 40],
        }
    }
}

impl AreaTables {
    /// Stores a freshly computed area: the ionogram and its `FOBBY`
    /// reflectrix rows (only the first `nang` rows are rewritten).
    pub fn update(&mut self, ion: Ionogram, fob: &[[i32; 30]]) {
        self.ion = ion;
        for (row, src) in self.ifob.iter_mut().zip(fob) {
            *row = *src;
        }
    }
}

/// One area's `/REFLX/` columns plus its `/LOSX/` columns.
#[derive(Debug, Clone)]
pub struct Reflectrix {
    pub delfx: [R; 45],
    pub hpflx: [R; 45],
    pub htflx: [R; 45],
    pub gdflx: [R; 45],
    pub fvflx: [R; 45],
    pub afflx: [R; 45],
    pub imode: [i32; 45],
    pub dskpkm: R,
    pub delskp: R,
    pub hpskp: R,
    pub htskp: R,
    pub dmaxkm: R,
    pub fvskp: R,
    pub iskp: i32,
    pub delpen: [R; 3],
    /// Index of the last row `findf` wrote (1-based; stale if a call
    /// wrote none, exactly like `IAFTXR`).
    pub iaftxr: usize,
    /// Rows written by the most recent `findf` call (not in the Fortran;
    /// used to bound trace comparison to fresh rows).
    pub rows_this_call: usize,
    // `/LOSX/` columns, written by `settxr`.
    pub andvx: [R; 45],
    pub advx: [R; 45],
    pub aofx: [R; 45],
    pub grlosx: [R; 45],
    pub tgainx: [R; 45],
    pub gml: [R; 45],
    pub fhp: [R; 45],
}

impl Default for Reflectrix {
    fn default() -> Self {
        Reflectrix {
            delfx: [0.0; 45],
            hpflx: [0.0; 45],
            htflx: [0.0; 45],
            gdflx: [0.0; 45],
            fvflx: [0.0; 45],
            afflx: [0.0; 45],
            imode: [0; 45],
            dskpkm: 0.0,
            delskp: 0.0,
            hpskp: 0.0,
            htskp: 0.0,
            dmaxkm: 0.0,
            fvskp: 0.0,
            iskp: 0,
            delpen: [0.0; 3],
            iaftxr: 0,
            rows_this_call: 0,
            andvx: [0.0; 45],
            advx: [0.0; 45],
            aofx: [0.0; 45],
            grlosx: [0.0; 45],
            tgainx: [0.0; 45],
            gml: [0.0; 45],
            fhp: [0.0; 45],
        }
    }
}

/// `/MODES/`: up to six raysets for one hop distance, one column per
/// sample area. `GHOP` is a single scalar shared by all three columns
/// and lives in [`ModeLoopState`], not here.
#[derive(Debug, Clone, Default)]
pub struct HopModes {
    pub delmod: [R; 6],
    pub hpmod: [R; 6],
    pub htmod: [R; 6],
    pub fvmod: [R; 6],
    pub itmod: [i32; 6],
    pub afmod: [R; 6],
}

/// `/ZON/`: the seven per-hop mode slots.
#[derive(Debug, Clone, Default)]
pub struct Zon {
    pub abps: [R; 7],
    pub crel: [R; 7],
    pub eff: [R; 7],
    pub fldst: [R; 7],
    pub grlos: [R; 7],
    pub hn: [R; 7],
    pub hp: [R; 7],
    pub prob: [R; 7],
    pub rely: [R; 7],
    pub rgain: [R; 7],
    pub sigpow: [R; 7],
    pub sn: [R; 7],
    pub spro: [R; 7],
    pub tgain: [R; 7],
    pub timed: [R; 7],
    pub tloss: [R; 7],
    pub b: [R; 7],
    pub fslos: [R; 7],
    pub adv: [R; 7],
    pub obf: [R; 7],
    pub nmode: [i32; 7],
    pub tllow: [R; 7],
    pub tlhgh: [R; 7],
}

/// `/allMODE/`: the accumulated mode list for one frequency (20 slots).
#[derive(Debug, Clone)]
pub struct AllModes {
    pub abps: [R; 20],
    pub crel: [R; 20],
    pub fldst: [R; 20],
    pub hn: [R; 20],
    pub hp: [R; 20],
    pub prob: [R; 20],
    pub rely: [R; 20],
    pub rgain: [R; 20],
    pub sigpow: [R; 20],
    pub sn: [R; 20],
    pub spro: [R; 20],
    pub tgain: [R; 20],
    pub timed: [R; 20],
    pub tloss: [R; 20],
    pub b: [R; 20],
    pub fslos: [R; 20],
    pub grlos: [R; 20],
    pub adv: [R; 20],
    pub obf: [R; 20],
    pub nmode: [i32; 20],
    pub tllow: [R; 20],
    pub tlhgh: [R; 20],
    pub eff: [R; 20],
    /// Most reliable mode, 1-based (`NREL`).
    pub nrel: usize,
    pub nmmod: usize,
}

impl Default for AllModes {
    fn default() -> Self {
        AllModes {
            abps: [0.0; 20],
            crel: [0.0; 20],
            fldst: [0.0; 20],
            hn: [0.0; 20],
            hp: [0.0; 20],
            prob: [0.0; 20],
            rely: [0.0; 20],
            rgain: [0.0; 20],
            sigpow: [0.0; 20],
            sn: [0.0; 20],
            spro: [0.0; 20],
            tgain: [0.0; 20],
            timed: [0.0; 20],
            tloss: [0.0; 20],
            b: [0.0; 20],
            fslos: [0.0; 20],
            grlos: [0.0; 20],
            adv: [0.0; 20],
            obf: [0.0; 20],
            nmode: [0; 20],
            tllow: [0.0; 20],
            tlhgh: [0.0; 20],
            eff: [0.0; 20],
            nrel: 0,
            nmmod: 0,
        }
    }
}

impl AllModes {
    /// `ALLMODES` with `iflg = 0`: reset for a new frequency. Only the
    /// four listed arrays are cleared; the rest keep old values.
    pub fn reset(&mut self) {
        self.nmmod = 0;
        for i in 0..20 {
            self.tloss[i] = 99999.0;
            self.tllow[i] = 999.0;
            self.tlhgh[i] = 999.0;
            self.hp[i] = -1.0;
        }
    }

    /// `ALLMODES` accumulation: copies `/ZON/` slots `ist..=lst`
    /// (1-based) that hold a mode.
    pub fn accumulate(&mut self, zon: &Zon, ist: usize, lst: usize) {
        for i in (ist - 1)..lst {
            if zon.hp[i] > 0.0 {
                let n = self.nmmod;
                self.nmmod += 1;
                self.tloss[n] = zon.tloss[i];
                self.tllow[n] = zon.tllow[i];
                self.tlhgh[n] = zon.tlhgh[i];
                self.hp[n] = zon.hp[i];
                self.crel[n] = zon.crel[i];
                self.rely[n] = zon.rely[i];
                self.hn[n] = zon.hn[i];
                self.nmode[n] = zon.nmode[i];
                self.sn[n] = zon.sn[i];
                self.fldst[n] = zon.fldst[i];
                self.sigpow[n] = zon.sigpow[i];
                self.b[n] = zon.b[i];
                self.timed[n] = zon.timed[i];
                self.abps[n] = zon.abps[i];
                self.prob[n] = zon.prob[i];
                self.rgain[n] = zon.rgain[i];
                self.tgain[n] = zon.tgain[i];
                self.fslos[n] = zon.fslos[i];
                self.spro[n] = zon.spro[i];
                self.eff[n] = zon.eff[i];
                self.grlos[n] = zon.grlos[i];
                self.adv[n] = zon.adv[i];
                self.obf[n] = zon.obf[i];
            }
        }
    }
}

/// One frequency slot of `/SON/`, plus the gain and noise stores other
/// commons carry per slot (`/cgains/`, `/sncom/`, `/DUDL_NOIS/`). Layer
/// labels are kept as the `NMODE` integer the `LAYTYP` lookup would
/// receive.
#[derive(Debug, Clone, Copy, Default)]
pub struct Son {
    pub angle: R,
    pub angler: R,
    pub cprob: R,
    pub dblos: R,
    pub dblosl: R,
    pub dblosu: R,
    pub dbu: R,
    pub delay: R,
    pub dbw: R,
    pub nhp: i32,
    pub xnynois: R,
    pub probmp: R,
    pub reliab: R,
    pub sndb: R,
    pub snpr: R,
    pub snrlw: R,
    pub snrup: R,
    pub sprob: R,
    pub vhigh: R,
    pub rneff: R,
    pub mode_layer: i32,
    pub moder_layer: i32,
    pub snxx: R,
    pub gaint: R,
    pub gainr: R,
    pub du_nois: R,
    pub dl_nois: R,
    /// `MDL`: b' ', b'S', b'L' or b'M'.
    pub mdl: u8,
}

/// Everything the mode loop keeps across hours for one case — the
/// persistent COMMON blocks.
#[derive(Debug, Clone)]
pub struct ModeLoopState {
    pub areas: [AreaTables; 3],
    pub reflectrix: [Reflectrix; 3],
    /// `/MODES/` `DELMOD`-`AFMOD`, one column per area. A column keeps
    /// its contents until something writes it again, so a pass whose
    /// `FDIST` fills one column and whose `INMUF` reads another sees
    /// what the last write left there.
    pub modes: [HopModes; 3],
    /// `/MODES/` `GHOP`: the current hop's angular distance, shared by
    /// every column and rewritten by `INMUF`.
    pub ghop: R,
    pub zon: Zon,
    pub all: AllModes,
    pub son: [Son; 13],
    /// `EFFlp(45)`, written by `settxr` for the receiver end.
    pub efflp: [R; 45],
    /// `/DON/` `D10R`/`D50R`/`D90R`: written by `relbil` per mode and
    /// read after its selection loop, so a single-mode frequency whose
    /// mode has no height reads the previous call's values.
    pub d10r: R,
    pub d50r: R,
    pub d90r: R,
}

impl Default for ModeLoopState {
    fn default() -> Self {
        ModeLoopState {
            areas: Default::default(),
            reflectrix: Default::default(),
            modes: Default::default(),
            ghop: 0.0,
            zon: Zon::default(),
            all: AllModes::default(),
            son: [Son::default(); 13],
            efflp: [0.0; 45],
            d10r: 0.0,
            d50r: 0.0,
            d90r: 0.0,
        }
    }
}

/// Immutable per-pass context for the frequency loop.
pub struct PassCtx<'a> {
    pub state: &'a IonoState,
    /// The installed antennas (`/cantenna/`), for `GAIN` and `PWRDB`.
    pub ants: &'a super::antenna::AntennaSet,
    pub fs: &'a [[R; 3]; 5],
    pub hs: &'a [R; 5],
    pub geog: &'a Geog,
    pub sig: &'a SignalDistribution,
    pub deck: DeckParams,
    pub gcd: R,
    pub gcdkm: R,
    /// Controlling sample area (0-based `JMODE - 1`).
    pub jmode: usize,
    /// The area index `K` left in `LUFFY` after the electron-density
    /// chain, which is what `FINDF` and `FDIST` are called with. It is
    /// `jmode` for the systems passes. For the short LUF pass it is the
    /// receiver-end area instead: the test that ends the chain,
    /// `IF((IPFG.EQ.100).OR.(K.GT.1))GO TO 87`, only names `IPFG` 100,
    /// so `IPFG` 300 falls through and runs the long-path receiver area
    /// as well, leaving `K = KFX`. The mode routines still read column
    /// `JMODE`, so that pass builds its reflectrix from one area and its
    /// modes from another. A bug, kept as written.
    pub kctl: usize,
    pub nang: usize,
    /// `IPFG = 200`: the long-path model.
    pub long: bool,
}

/// Fills the `/ES/` five-slot arrays from the per-point Es parameters
/// (before `SETLNG` replication).
pub fn es_slots(es: &[EsParams]) -> ([[R; 3]; 5], [R; 5]) {
    let mut fs = [[0.0; 3]; 5];
    let mut hs = [0.0; 5];
    for (k, e) in es.iter().enumerate() {
        fs[k] = e.fs;
        hs[k] = e.hs;
    }
    (fs, hs)
}

// ---------------------------------------------------------------------
// SETLNG

/// Port of `SETLNG`: replicates the sample-area arrays so all five
/// scalar slots (and profile slots up to 3) are valid. `KFX = 3` is a
/// no-op. The `CLAT`/`CLONG`/`GLAT`/`RD` replication is omitted: nothing
/// in the mode loop reads them.
pub fn setlng(
    state: &mut IonoState,
    fs: &mut [[R; 3]; 5],
    hs: &mut [R; 5],
    geog: &mut Geog,
    areas: &mut [AreaTables; 3],
) {
    if state.kfx >= 3 {
        return;
    }
    if state.kfx <= 1 {
        for is in 1..5 {
            geog.gyz[is] = geog.gyz[0];
            geog.clck[is] = geog.clck[0];
            geog.abiy[is] = geog.abiy[0];
            geog.artic[is] = geog.artic[0];
            geog.sigpat[is] = geog.sigpat[0];
            geog.epspat[is] = geog.epspat[0];
            hs[is] = hs[0];
            fs[is] = fs[0];
            state.fi[is] = state.fi[0];
            state.yi[is] = state.yi[0];
            state.hi[is] = state.hi[0];
        }
        areas[1] = areas[0].clone();
        areas[2] = areas[0].clone();
    } else {
        for is in 3..5 {
            geog.gyz[is] = geog.gyz[2];
            geog.clck[is] = geog.clck[2];
            geog.abiy[is] = geog.abiy[2];
            geog.artic[is] = geog.artic[2];
            geog.sigpat[is] = geog.sigpat[2];
            geog.epspat[is] = geog.epspat[2];
            hs[is] = hs[2];
            fs[is] = fs[2];
            state.fi[is] = state.fi[2];
            state.yi[is] = state.yi[2];
            state.hi[is] = state.hi[2];
        }
        areas[2] = areas[1].clone();
    }
}

// ---------------------------------------------------------------------
// Antenna gain and the Fresnel ground-reflection loss

/// `GAIN` with `ITR < 0`: the Fresnel ground-reflection loss between
/// hops. `delta` in radians; `sigma`/`er` are the point's ground
/// constants. Zero conductivity returns zero loss.
pub fn gain_ground(delta: R, fmc: R, sigma: R, er: R) -> R {
    if sigma <= 0.0 {
        return 0.0;
    }
    let x = 18000.0 * sigma / fmc;
    let t = delta.cos();
    let q = delta.sin();
    let r = q * q;
    let s = r * r;
    let ert = er - t * t;
    let rho = (ert * ert + x * x).sqrt();
    let rho12 = rho.sqrt();
    let alpha = -(x / ert).atan();
    let u = er * er + x * x;
    let v = u.sqrt();
    let asxv = (x / v).asin();
    let cv = (rho * rho + u * u * s - 2.0 * rho * u * r * (alpha + 2.0 * asxv).cos()).sqrt()
        / (rho + u * r + 2.0 * rho12 * v * q * (alpha * 0.5 + asxv).cos());
    let ch = (rho * rho + s - 2.0 * rho * r * alpha.cos()).sqrt()
        / (rho + r + 2.0 * rho12 * q * (alpha * 0.5).cos());
    let mut rain = 4.3429 * (0.5 * (ch * ch + cv * cv)).ln();
    rain = rain.abs();
    if delta <= 0.000_000_01 {
        rain = 6.0;
    }
    rain
}

/// Average ground loss over the control points, the `GAIN(-IG,..)` loop
/// shared by `regmod`, `esmod` and `settxr`.
fn ground_loss_avg(ctx: &PassCtx, del: R, freq: R, km: usize) -> R {
    let mut y: R = 0.0;
    for ig in 0..km {
        y += gain_ground(del, freq, ctx.geog.sigpat[ig], ctx.geog.epspat[ig]);
    }
    y / km as R
}

// ---------------------------------------------------------------------
// FNORML

/// Port of `FNORML`: the cumulative normal distribution.
pub fn fnorml(ypx: R) -> R {
    const C: [R; 4] = [0.196854, 0.115194, 0.000344, 0.019527];
    let yp = ypx.abs().min(5.0);
    let mut qx = 1.0 + yp * (C[0] + yp * (C[1] + yp * (C[2] + yp * C[3])));
    qx = qx * qx * qx * qx;
    qx = 0.5 * (1.0 / qx);
    if ypx < 0.0 {
        qx
    } else {
        1.0 - qx
    }
}

// ---------------------------------------------------------------------
// PENANG and FINDF

/// Port of `PENANG`: penetration angles per layer for `freq` in area
/// `k`. May flag the area dead (`dskpkm = dmaxkm = -1`).
fn penang(state: &IonoState, area: &AreaTables, k: usize, freq: R, rfx: &mut Reflectrix) {
    let fmhz = freq;
    let ion = &area.ion;
    // E layer cusp.
    let ht = ion.htrue[9];
    let fv = ion.fvert[9];
    let mut frat = fv / freq;
    frat *= frat;
    if frat >= 0.9999 {
        rfx.delpen = [89.9, 90.0, 90.0];
        return;
    }
    let cdel = (RZ + ht) * (1.0 - frat).sqrt() / RZ;
    if cdel <= 0.999999 {
        rfx.delpen[0] = cdel.acos() * R2D;
    } else {
        rfx.delpen[0] = 0.0;
    }
    if state.fi[k][1] > 0.0 {
        // F1 layer cusp.
        let ht = ion.htrue[19];
        let fv = ion.fvert[19];
        let mut frat = fv / freq;
        frat *= frat;
        if frat >= 0.9999 {
            rfx.delpen[1] = 89.9;
            rfx.delpen[2] = 90.0;
            return;
        }
        let cdel = (RZ + ht) * (1.0 - frat).sqrt() / RZ;
        if cdel <= 0.999999 {
            rfx.delpen[1] = cdel.acos() * R2D;
        } else {
            rfx.delpen[1] = 0.0;
        }
    } else {
        rfx.delpen[1] = rfx.delpen[0];
    }
    // F layer: reflection until the maximum of (RZ+HT)*MU over the top
    // three table rows, not the middle of the layer.
    if ion.fvert[29] - freq + 0.0001 >= 0.0 {
        rfx.delpen[2] = 89.9;
        return;
    }
    let xm28 = (RZ + ion.htrue[27]) * (1.0 - (ion.fvert[27] / fmhz).powi(2)).sqrt();
    let xm29 = (RZ + ion.htrue[28]) * (1.0 - (ion.fvert[28] / fmhz).powi(2)).sqrt();
    let xm30 = (RZ + ion.htrue[29]) * (1.0 - (ion.fvert[29] / fmhz).powi(2)).sqrt();
    let cdel = if xm30 >= xm29 {
        if xm30 >= xm28 {
            xm30 / RZ
        } else {
            xm28 / RZ
        }
    } else if xm29 >= xm28 {
        xm29 / RZ
    } else {
        xm28 / RZ
    };
    if cdel <= 0.999999 {
        rfx.delpen[2] = cdel.acos() * R2D;
    } else {
        rfx.delpen[2] = 0.0;
        rfx.dskpkm = -1.0;
        rfx.dmaxkm = -1.0;
    }
}

/// Port of `FINDF`: builds the reflectrix table for `freq` in area `k`
/// (0-based), inserting layer cusps and the Martyn spherical correction,
/// and finds the skip and maximum distances.
pub fn findf(
    rfx: &mut Reflectrix,
    state: &IonoState,
    area: &AreaTables,
    k: usize,
    freq: R,
    amind: R,
    nang: usize,
) {
    let jfhz = (1000.0 * freq) as i32;
    rfx.dmaxkm = 0.0;
    rfx.dskpkm = 10000.0;
    for ia in 0..45 {
        rfx.hpflx[ia] = 0.0;
        rfx.delfx[ia] = 0.0;
        rfx.gdflx[ia] = 0.0;
    }
    rfx.rows_this_call = 0;
    let fc2 = state.fi[k][2] * state.fi[k][2];
    // PENANG may leave dskpkm/dmaxkm at -1 (F layer dead at every
    // angle); the search still runs for the lower layers and dmaxkm
    // recovers if a row qualifies, while dskpkm stays negative.
    penang(state, area, k, freq, rfx);
    let ion = &area.ion;

    let mut ia: i32 = 0;
    let mut iaf: i32 = 1;
    // Layer bounds, all 1-based like the source.
    let mut icusp: i32 = -1;
    let mut il: usize = 1;
    let mut ih: i32 = 1;
    let mut ilow: i32 = 1;
    let mut ihigh: i32 = 10;

    // The statement labels of the search, as machine states.
    #[derive(Clone, Copy, PartialEq)]
    enum L {
        Search,    // 275
        NextLayer, // 265
        Fill(Fill),
        NextCusp, // 350
        Done,     // 400
    }
    #[derive(Clone, Copy, PartialEq)]
    enum Fill {
        Exact,       // 325: table row IH at ANG(IA)
        Interp,      // 340: between rows IH and IH+1
        Cusp,        // 345: layer cusp
        NextCusp360, // 360: cusp for the next layer
    }
    let mut label = L::Search;
    loop {
        match label {
            L::Search => {
                if rfx.delpen[il - 1] <= 0.0 {
                    label = L::NextLayer;
                    continue;
                }
                if rfx.delpen[il - 1] > 89.99 {
                    label = L::Done;
                    continue;
                }
                ia += 1;
                if ia > nang as i32 {
                    label = L::Done;
                    continue;
                }
                if rfx.delpen[il - 1] - ANG[ia as usize - 1] <= 0.0 {
                    label = L::Fill(Fill::Cusp);
                    continue;
                }
                // 305: search this angle's column for the frequency.
                loop {
                    if area.ifob[ia as usize - 1][ilow as usize - 1] >= jfhz {
                        label = L::Fill(Fill::Exact);
                        break;
                    }
                    if ih >= ihigh {
                        label = L::Search;
                        break;
                    }
                    let v = area.ifob[ia as usize - 1][ih as usize - 1];
                    if v == jfhz {
                        label = L::Fill(Fill::Exact);
                        break;
                    }
                    if v > jfhz {
                        ih += 1;
                        continue;
                    }
                    if area.ifob[ia as usize - 1][ih as usize] < jfhz {
                        ih += 1;
                        continue;
                    }
                    label = L::Fill(Fill::Interp);
                    break;
                }
            }
            L::NextLayer => {
                // 265: computed GO TO on IL.
                match il {
                    1 => {
                        // 225: F region.
                        ih = 11;
                        ilow = 11;
                        if state.fi[k][1] > 0.0 {
                            il = 2;
                            icusp = -1;
                            ihigh = 20;
                        } else {
                            il = 3;
                            icusp = -1;
                            ihigh = 30;
                        }
                        label = L::Search;
                    }
                    2 => {
                        // 255: F2 above F1.
                        il = 3;
                        icusp = -1;
                        ilow = ihigh + 1;
                        ihigh = 30;
                        ih = 21;
                        label = L::Search;
                    }
                    _ => label = L::Done,
                }
            }
            L::NextCusp => {
                // 350: after a completed cusp, try the next layer.
                if il >= 3 || rfx.delpen[il - 1] >= 89.9 {
                    label = L::Done;
                } else {
                    label = L::Fill(Fill::NextCusp360);
                }
            }
            L::Fill(fill) => {
                let (fv, hp);
                let i = iaf as usize - 1;
                match fill {
                    Fill::Exact => {
                        let h = ih as usize - 1;
                        rfx.delfx[i] = ANG[ia as usize - 1];
                        rfx.htflx[i] = ion.htrue[h];
                        rfx.afflx[i] = ion.afac[h];
                        fv = ion.fvert[h];
                        hp = ion.hprim[h];
                        rfx.imode[i] = il as i32;
                    }
                    Fill::Interp => {
                        let h = ih as usize - 1;
                        let slopd = (area.ifob[ia as usize - 1][h + 1]
                            - area.ifob[ia as usize - 1][h]) as R;
                        let slopd = slopd.max(1.0);
                        let slope = (jfhz - area.ifob[ia as usize - 1][h]) as R / slopd;
                        rfx.htflx[i] = ion.htrue[h] + slope * (ion.htrue[h + 1] - ion.htrue[h]);
                        fv = ion.fvert[h] + slope * (ion.fvert[h + 1] - ion.fvert[h]);
                        rfx.delfx[i] = ANG[ia as usize - 1];
                        hp = ion.hprim[h] + slope * (ion.hprim[h + 1] - ion.hprim[h]);
                        rfx.afflx[i] = ion.afac[h] + slope * (ion.afac[h + 1] - ion.afac[h]);
                        rfx.imode[i] = il as i32;
                    }
                    Fill::Cusp => {
                        let h = ihigh as usize - 1;
                        rfx.delfx[i] = rfx.delpen[il - 1];
                        rfx.htflx[i] = ion.htrue[h];
                        rfx.afflx[i] = ion.afac[h];
                        fv = ion.fvert[h];
                        hp = ion.hprim[h];
                        ia -= 1;
                        icusp = 0;
                        rfx.imode[i] = il as i32;
                    }
                    Fill::NextCusp360 => {
                        let h = ihigh as usize; // IHIGH + 1, 0-based
                        rfx.delfx[i] = rfx.delfx[i - 1] + 0.001;
                        rfx.htflx[i] = ion.htrue[h];
                        rfx.afflx[i] = ion.afac[h];
                        fv = ion.fvert[h];
                        hp = ion.hprim[h];
                        icusp = 1;
                        rfx.imode[i] = if state.fi[k][1] <= 0.0 {
                            3
                        } else {
                            il as i32 + 1
                        };
                    }
                }
                // 375: the Martyn spherical-ionosphere correction.
                let del = rfx.delfx[i] * D2R;
                let rcosd = RZ * del.cos();
                let xfsq = freq * freq / fc2;
                let ht = rfx.htflx[i];
                let xmut = 1.0 - fv * fv / (freq * freq);
                let xhp = (hp - ht) / RZ;
                let sph = xfsq * xmut * xhp * (ht + 2.0 * (RZ + ht) * xhp);
                let hp = hp + sph;
                let phe = (rcosd / (RZ + hp)).asin();
                let gdr = 2.0 * RZ * (PIO2 - del - phe);
                rfx.gdflx[i] = gdr;
                rfx.hpflx[i] = hp;
                rfx.fvflx[i] = fv;
                if rfx.dskpkm > gdr {
                    rfx.dskpkm = gdr;
                    rfx.delskp = rfx.delfx[i];
                    rfx.htskp = ht;
                    rfx.hpskp = hp;
                    rfx.fvskp = fv;
                    rfx.iskp = il as i32;
                }
                if rfx.dmaxkm <= gdr && rfx.delfx[i] >= amind {
                    rfx.dmaxkm = gdr;
                }
                rfx.iaftxr = iaf as usize;
                rfx.rows_this_call = iaf as usize;
                iaf += 1;
                if iaf > 45 {
                    label = L::Done;
                    continue;
                }
                label = match icusp.cmp(&0) {
                    std::cmp::Ordering::Less => L::Search,
                    std::cmp::Ordering::Equal => L::NextCusp,
                    std::cmp::Ordering::Greater => L::NextLayer,
                };
            }
            L::Done => break,
        }
    }
}

// ---------------------------------------------------------------------
// FDIST

/// Port of `FDIST`: finds up to six raysets for the hop distance
/// `ghop` (radians) at `freq` by searching the reflectrix table.
pub fn fdist(m: &mut HopModes, rfx: &Reflectrix, ghop: R, amind: R, freq: R) {
    m.delmod = [0.0; 6];
    m.hpmod = [-1.0; 6];
    m.htmod = [0.0; 6];
    m.fvmod = [0.0; 6];
    m.itmod = [5; 6];
    m.afmod = [0.0; 6];
    let dhopkm = ghop * RZ;
    if dhopkm >= rfx.dmaxkm {
        return;
    }
    let mut ih: usize = 0; // 1-based after the first increment
    let mut il: usize = 0;
    'layer: loop {
        il += 1; // 140
        'row: loop {
            ih += 1; // 145
            if ih > 44 {
                break 'layer;
            }
            if rfx.hpflx[ih] <= 0.0 {
                break 'layer;
            }
            let g1 = rfx.gdflx[ih - 1];
            let g2 = rfx.gdflx[ih];
            // Fill from table row `ih`, the row after it, or by
            // interpolation between them.
            enum F {
                Row,
                RowNext,
                Interp,
            }
            let fill = if g1 < g2 {
                // 170: ascending distances.
                if g1 < dhopkm {
                    if dhopkm < g2 {
                        F::Interp
                    } else if dhopkm == g2 {
                        F::RowNext
                    } else {
                        continue 'row;
                    }
                } else if g1 == dhopkm {
                    F::Row
                } else {
                    continue 'row;
                }
            } else if g1 == g2 {
                // 160: a flat segment. The 1997 rewrite skips only when
                // the hop distance is within a metre of the segment and
                // otherwise fills — the reverse of the original
                // equality-only fill.
                if (dhopkm - g1).abs() <= 0.001 {
                    continue 'row;
                }
                F::Row
            } else {
                // 215: descending distances.
                if g1 < dhopkm {
                    continue 'row;
                } else if g1 == dhopkm {
                    F::Row
                } else if dhopkm < g2 {
                    continue 'row;
                } else if dhopkm == g2 {
                    F::RowNext
                } else {
                    F::Interp
                }
            };
            let i = il - 1;
            match fill {
                F::Row | F::RowNext => {
                    if matches!(fill, F::RowNext) {
                        ih += 1; // 176
                    }
                    m.delmod[i] = rfx.delfx[ih - 1];
                    m.hpmod[i] = rfx.hpflx[ih - 1];
                    m.htmod[i] = rfx.htflx[ih - 1];
                    m.itmod[i] = rfx.imode[ih - 1];
                    m.afmod[i] = rfx.afflx[ih - 1];
                    m.fvmod[i] = rfx.fvflx[ih - 1];
                }
                F::Interp => {
                    // 180.
                    let dth2 = rfx.gdflx[ih] - rfx.gdflx[ih - 1];
                    if dth2.abs() <= 0.001 {
                        m.delmod[i] = rfx.delfx[ih - 1];
                        m.hpmod[i] = rfx.hpflx[ih - 1];
                        m.htmod[i] = rfx.htflx[ih - 1];
                        m.itmod[i] = rfx.imode[ih - 1];
                        m.afmod[i] = rfx.afflx[ih - 1];
                        m.fvmod[i] = rfx.fvflx[ih - 1];
                    } else {
                        let dth = dhopkm - rfx.gdflx[ih - 1];
                        let thet = 0.5 * dhopkm / RZ;
                        let hp1 = rfx.hpflx[ih - 1];
                        let hp2 = rfx.hpflx[ih];
                        let ht1 = rfx.htflx[ih - 1];
                        let ht2 = rfx.htflx[ih];
                        let hp = hp1 + (hp2 - hp1) * dth / dth2;
                        let ht = ht1 + (ht2 - ht1) * dth / dth2;
                        m.afmod[i] =
                            rfx.afflx[ih - 1] + (rfx.afflx[ih] - rfx.afflx[ih - 1]) * dth / dth2;
                        m.hpmod[i] = hp;
                        let st = thet.sin();
                        let tanp = st / (1.0 - thet.cos() + hp / RZ);
                        let phe = tanp.atan();
                        // Force correct geometry and Snell's law.
                        m.delmod[i] = (PIO2 - phe - thet) * R2D;
                        let sphi = RZ * (m.delmod[i] * D2R).cos() / (RZ + ht);
                        let sphi = (1.0 - sphi * sphi).max(0.000001);
                        m.fvmod[i] = freq * sphi.sqrt();
                        m.htmod[i] = ht;
                        m.itmod[i] = rfx.imode[ih - 1];
                    }
                }
            }
            // 305: minimum-angle check after interpolation.
            if m.delmod[i] >= amind {
                if il >= 6 {
                    break 'layer;
                }
                continue 'layer; // 315 -> 140
            }
            m.hpmod[i] = -m.hpmod[i].abs();
            continue 'row; // ICEPAC addition: keep searching this slot.
        }
    }
}

// ---------------------------------------------------------------------
// REGMOD

/// Port of `REGMOD`: losses for the raysets of one hop count — free
/// space, D-E absorption, deviative loss, Es obscuration, ground loss,
/// over-the-MUF loss with the 2006 low-MUFday extra loss — into `/ZON/`
/// slots 1-6. `fsdead` is `IFOB(1,3,JMODE)/1000` capped at 3 MHz.
#[allow(clippy::too_many_arguments)]
fn regmod(
    zon: &mut Zon,
    modes: &HopModes,
    ghop: R,
    ctx: &PassCtx,
    muf: &MufHour,
    noise: &NoiseResult,
    freq: R,
    fsdead: R,
) {
    let k = ctx.jmode;
    let l = [1usize, 3, 5][k]; // LX(K)
    let ac = 677.2 * ctx.sig.acav;
    let bc = (freq + ctx.geog.gyz[l - 1]).powf(1.98);
    let ihop = (ctx.gcd / ghop + 0.01) as i32;
    let hop = ihop as R;
    for im in 0..7 {
        zon.hn[im] = -1.0;
        zon.hp[im] = -1.0;
        if im >= 6 {
            continue;
        }
        if modes.hpmod[im] <= 0.0 {
            continue;
        }
        let del = (D2R * modes.delmod[im]).min(89.99 * D2R);
        let cdel = del.cos();
        let psi = ghop * 0.5;
        let phe = PIO2 - psi - del;
        let path = 2.0 * (modes.hpmod[im] + RZ * (1.0 - psi.cos())) / phe.cos();
        let path = (path * hop).abs();
        zon.timed[im] = path / VOFL;
        zon.fslos[im] = 32.45 + 20.0 * (path * freq).log10();
        let obfu: R;
        let obfl: R;
        if ctx.state.fi[k][0] - modes.fvmod[im] >= 0.0 {
            // D-E mode.
            let xnsq = if ctx.sig.htloss - modes.htmod[im] <= 0.0 {
                10.2
            } else {
                let hnux = 61.0 + 3.0 * (modes.htmod[im] - 70.0) / 18.0;
                ctx.sig.xnuz * (-2.0 * (hnux - 60.0) / ctx.sig.hnu).exp()
            };
            // The classical height of 100 km is assumed above 100 km.
            let heff = modes.htmod[im].min(100.0);
            let sinp = RZ * cdel / (RZ + heff);
            let secp = 1.0 / (1.0 - sinp * sinp).sqrt();
            zon.abps[im] = secp * ac / (bc + xnsq);
            // Remove the E-layer bending effect.
            let xv = (modes.fvmod[im] / ctx.state.fi[k][0]).max(ctx.sig.xve);
            let adx = ctx.sig.afe + ctx.sig.bfe * xv.ln();
            let secp = 1.0 / (del + psi).sin();
            zon.adv[im] = secp * modes.afmod[im]
                * ((modes.fvmod[im] + ctx.geog.gyz[l - 1]).powf(1.98) + xnsq)
                / (bc + xnsq)
                + adx;
            zon.obf[im] = 0.0;
            obfu = 0.0;
            obfl = 0.0;
        } else {
            // F-layer mode.
            let xnsq = 10.2;
            let sinp = RZ * cdel / (RZ + 100.0);
            let secp = 1.0 / (1.0 - sinp * sinp).sqrt();
            zon.abps[im] = secp * ac / (bc + xnsq);
            let secp = 1.0 / (del + psi).sin();
            zon.adv[im] = secp * modes.afmod[im]
                * ((modes.fvmod[im] + ctx.geog.gyz[l - 1]).powf(1.98) + xnsq)
                / (bc + xnsq);
            zon.obf[im] = 0.0;
            if ctx.fs[k][1] > 0.0 {
                let fmhz = freq.max(fsdead);
                let sins = RZ * cdel / (RZ + ctx.hs[k]);
                let secs = 1.0 / (1.0 - sins * sins).sqrt();
                let esd = ctx.fs[k][1] * secs;
                let pros = prbmuf(muf, fmhz, esd, muf.layers[3].ymuf, 4).min(0.90);
                zon.obf[im] = -10.0 * (1.0 - pros).log10();
                // The source computes both deciles at the same oblique
                // frequency (FS(1,K)·sec), against YFOT then YHPF.
                let esd = ctx.fs[k][0] * secs;
                let pros = prbmuf(muf, fmhz, esd, muf.layers[3].yfot, 4).min(0.90);
                obfu = -10.0 * (1.0 - pros).log10();
                let pros = prbmuf(muf, fmhz, esd, muf.layers[3].yhpf, 4).min(0.9);
                obfl = -10.0 * (1.0 - pros).log10();
            } else {
                obfu = 0.0;
                obfl = 0.0;
            }
        }
        zon.grlos[im] = ground_loss_avg(ctx, del, freq, ctx.state.km);
        let (tgain, _teff) = ctx.ants.gain(1, del, freq);
        let (rgain, reff) = ctx.ants.gain(2, del, freq);
        zon.tgain[im] = tgain;
        zon.rgain[im] = rgain;
        zon.eff[im] = reff;
        // Only two hops carry the obscuration.
        let hops = hop.min(2.0);
        let mut xtlos = zon.fslos[im]
            + hop * (zon.abps[im] + zon.adv[im])
            + zon.grlos[im] * (hop - 1.0)
            + hops * zon.obf[im]
            + ctx.sig.asm
            - zon.rgain[im]
            - zon.tgain[im];
        let ismod = modes.itmod[im] as usize;
        let sphet = RZ * cdel / (RZ + modes.htmod[im]);
        let cphet = (1.0 - sphet * sphet).max(0.000001).sqrt();
        // The 1995 change: MUFday from the specific hop MUF.
        let lay = muf.layers[ismod - 1];
        let psi2 = ctx.gcd / 2.0 / hop;
        let cpsi = psi2.cos();
        let spsi = psi2.sin();
        let tanp = spsi / (1.0 - cpsi + lay.hpmuf / RZ);
        let phe2 = tanp.atan();
        let del2 = PIO2 - phe2 - psi2;
        let cdel2 = del2.cos();
        let sphe = RZ * cdel2 / (RZ + lay.htmuf);
        let xmuf = lay.fvmuf / (1.0 - sphe * sphe).sqrt();
        zon.prob[im] = prbmuf(muf, freq, xmuf, lay.ymuf, ismod);
        if zon.prob[im] < 0.0001 {
            // The source STOPs on a non-positive MUFday; unreachable
            // because PRBMUF's polynomial value is positive whenever
            // its distribution is set.
            assert!(zon.prob[im] > 0.0, "REGMOD: non-positive MUFday");
            // The 2006 extra loss: 0 to 24 dB for MUFday 1e-4 to 1e-7.
            let xfac = zon.prob[im].log10().clamp(-7.0, -4.0);
            let ghlos = (xfac + 4.0) * 8.0;
            xtlos -= ghlos;
        }
        let xmuf = lay.fvmuf / cphet;
        let p = prbmuf(muf, freq, xmuf, lay.ymuf, ismod);
        let p = if p <= 0.000001 { 0.000001 } else { p };
        let xls = -10.0 * p.log10() / cphet;
        xtlos += xls * hop;
        let cpr = lay.fvmuf / lay.ymuf;
        let fvfot = lay.yfot * cpr;
        let xmuf = fvfot / cphet;
        let pf = prbmuf(muf, freq, xmuf, lay.yfot, ismod);
        let pf = if pf <= 0.000001 { 0.000001 } else { pf };
        let xlsl = -10.0 * pf.log10() / cphet;
        let fvhpf = lay.yhpf * cpr;
        let xmuf = fvhpf / cphet;
        let pf = prbmuf(muf, freq, xmuf, lay.yhpf, ismod);
        let pf = if pf <= 0.000001 { 0.000001 } else { pf };
        let xlsu = -10.0 * pf.log10() / cphet;
        // Signal-level deciles (TLLOW is the lower, TLHGH the upper).
        zon.tllow[im] = (ctx.sig.dsl + hops * (obfl - zon.obf[im]) + hop * (xlsl - xls)).min(25.0);
        zon.tlhgh[im] = (ctx.sig.dsu + hops * (zon.obf[im] - obfu) + hop * (xls - xlsu)).min(25.0);
        zon.tloss[im] = xtlos;
        zon.fldst[im] =
            107.2 + ctx.ants.pwrdb(freq) + 20.0 * freq.log10() - xtlos - zon.rgain[im];
        zon.sigpow[im] = ctx.ants.pwrdb(freq) - xtlos;
        zon.sn[im] = zon.sigpow[im] - noise.rcnse - zon.eff[im];
        zon.b[im] = modes.delmod[im];
        zon.nmode[im] = ismod as i32;
        zon.hp[im] = modes.hpmod[im];
        zon.hn[im] = ihop as R;
    }
}

// ---------------------------------------------------------------------
// INMUF

/// Port of `INMUF`: compacts the raysets, inserts over-the-MUF or
/// zero-distance modes, temporarily rescales the layer MUF
/// distributions for higher hop counts, calls `REGMOD`, and restores.
// The source's dead stores (INM after the last read, the counted but
// unread zero-distance slot) are kept.
#[allow(clippy::too_many_arguments, unused_assignments, clippy::needless_range_loop)]
fn inmuf(
    zon: &mut Zon,
    modes: &mut HopModes,
    ghop: &mut R,
    muf: &mut MufHour,
    ctx: &PassCtx,
    area: &AreaTables,
    noise: &NoiseResult,
    freq: R,
    ihop: i32,
    fsdead: R,
) {
    const EPS: R = 0.4001;
    let k = ctx.jmode;
    let mut ireset = false;
    let mut osave = [(0.0 as R, 0.0 as R, 0.0 as R, 0.0 as R, 0.0 as R); 3];
    let mut inm = [-1i32; 3];
    // Compact the modes that exist (virtual height at least 70 km).
    let mut inum: usize = 0;
    for im in 0..6 {
        if modes.hpmod[im] - 70.0 < 0.0 {
            continue;
        }
        let itp = modes.itmod[im] as usize;
        inm[itp - 1] = 1;
        modes.delmod[inum] = modes.delmod[im];
        modes.hpmod[inum] = modes.hpmod[im];
        modes.htmod[inum] = modes.htmod[im];
        modes.fvmod[inum] = modes.fvmod[im];
        modes.afmod[inum] = modes.afmod[im];
        modes.itmod[inum] = modes.itmod[im];
        inum += 1;
    }
    // Very short distance: takeoff angle at the MUF is near vertical.
    let zero_distance = muf.layers[muf.modmuf as usize - 1].delmuf >= 89.9;
    let mut done = false;
    if !zero_distance {
        for ily in 0..3 {
            if ily == 1 && ctx.state.fi[k][1] <= 0.0 {
                continue;
            }
            if inum >= 5 {
                break;
            }
            if inm[ily] < 0
                && (freq + EPS) >= muf.layers[ily].ymuf
                && ihop == muf.layers[ily].nhopmf
            {
                // Insert the layer's over-the-MUF mode.
                modes.delmod[inum] = muf.layers[ily].delmuf;
                modes.hpmod[inum] = muf.layers[ily].hpmuf;
                modes.htmod[inum] = muf.layers[ily].htmuf;
                modes.fvmod[inum] = muf.layers[ily].fvmuf;
                modes.afmod[inum] = muf.layers[ily].afmuf;
                modes.itmod[inum] = ily as i32 + 1;
                inm[ily] = 1;
                inum += 1;
            }
        }
        if ihop == muf.layers[muf.modmuf as usize - 1].nhopmf {
            if inum > 0 {
                done = true;
            }
            // Otherwise fall through to the zero-distance search below.
        } else {
            // Rescale the layer MUFs to this hop count.
            ireset = true;
            for il in 0..3 {
                let l = &muf.layers[il];
                osave[il] = (l.sigl, l.sigu, l.yfot, l.yhpf, l.ymuf);
            }
            for jh in 0..3 {
                if inum >= 5 {
                    break;
                }
                if inm[jh] > 0 {
                    continue;
                }
                if jh == 1 && ctx.state.fi[k][1] <= 0.0 {
                    continue;
                }
                if ihop <= muf.layers[jh].nhopmf {
                    continue;
                }
                let fv = muf.layers[jh].fvmuf;
                let ht = muf.layers[jh].htmuf;
                let hpver = xlin(fv, &area.ion.fvert, &area.ion.hprim);
                let xhp = (hpver - ht) / RZ;
                let hop = ihop as R;
                let psi = 0.5 * ctx.gcd / hop;
                let tdel = (psi.cos() - RZ / (RZ + hpver)) / psi.sin();
                let cdel = 1.0 / (1.0 + tdel * tdel).sqrt();
                let sphe = RZ * cdel / (RZ + ht);
                let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
                let esd = fv * secp;
                // The Martyn correction, as in CURMUF.
                let fob1 = esd;
                let xmut = sphe * sphe;
                let xfsq = fob1 * fob1 / (ctx.state.fi[k][jh] * ctx.state.fi[k][jh]);
                let sph = xfsq * xmut * xhp * (ht + 2.0 * (RZ + ht) * xhp);
                let hpnow = hpver + sph;
                let tdel = (psi.cos() - RZ / (RZ + hpnow)) / psi.sin();
                let cdel = 1.0 / (1.0 + tdel * tdel).sqrt();
                let sphe = RZ * cdel / (RZ + ht);
                let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
                let esd = fv * secp;
                if freq - esd + EPS >= 0.0 {
                    // The rescaled MUF admits this frequency.
                    muf.layers[jh].ymuf = esd;
                    muf.layers[jh].sigl = esd * osave[jh].0 / osave[jh].4;
                    muf.layers[jh].yfot = muf.layers[jh].ymuf - 1.28 * muf.layers[jh].sigl;
                    muf.layers[jh].sigu = esd * osave[jh].1 / osave[jh].4;
                    muf.layers[jh].yhpf = muf.layers[jh].ymuf + 1.28 * muf.layers[jh].sigu;
                    modes.delmod[inum] = tdel.atan() * R2D;
                    modes.hpmod[inum] = hpnow;
                    modes.htmod[inum] = muf.layers[jh].htmuf;
                    modes.fvmod[inum] = muf.layers[jh].fvmuf;
                    modes.afmod[inum] = muf.layers[jh].afmuf;
                    modes.itmod[inum] = jh as i32 + 1;
                    inum += 1;
                    *ghop = 2.0 * psi;
                }
            }
            done = true;
        }
    }
    if !done {
        // Zero-distance mode straight from the ionogram (label 250).
        let slot = inum;
        // inum counts the slot even when the interpolation below fails,
        // exactly like the source.
        #[allow(unused_assignments)]
        {
            inum += 1;
        }
        let fv = freq - 0.001;
        let mut fmax = area.ion.fvert[0];
        let mut hit: Option<std::cmp::Ordering> = None;
        let mut ihx = 0usize;
        for ih in 1..30 {
            ihx = ih;
            if area.ion.fvert[ih] > fmax {
                fmax = area.ion.fvert[ih];
            }
            let d = fv - area.ion.fvert[ih];
            if d < 0.0 {
                hit = Some(std::cmp::Ordering::Less);
                break;
            }
            if d == 0.0 {
                hit = Some(std::cmp::Ordering::Equal);
                break;
            }
        }
        let mut filled = false;
        match hit {
            None => {
                // Frequency above every table value: the slot stays
                // unfilled (its virtual height keeps the -1 preset).
            }
            Some(std::cmp::Ordering::Less) => {
                let slope = area.ion.fvert[ihx] - area.ion.fvert[ihx - 1];
                if slope > 0.01 {
                    let slp = (fv - area.ion.fvert[ihx - 1]) / slope;
                    modes.delmod[slot] = 90.0;
                    modes.hpmod[slot] =
                        area.ion.hprim[ihx - 1] + (area.ion.hprim[ihx] - area.ion.hprim[ihx - 1]) * slp;
                    modes.htmod[slot] =
                        area.ion.htrue[ihx - 1] + (area.ion.htrue[ihx] - area.ion.htrue[ihx - 1]) * slp;
                    modes.fvmod[slot] = fv;
                    modes.afmod[slot] =
                        area.ion.afac[ihx - 1] + (area.ion.afac[ihx] - area.ion.afac[ihx - 1]) * slp;
                }
                filled = true;
            }
            Some(_) => {
                modes.delmod[slot] = 90.0;
                modes.hpmod[slot] = area.ion.hprim[ihx];
                modes.htmod[slot] = area.ion.htrue[ihx];
                modes.fvmod[slot] = area.ion.fvert[ihx];
                modes.afmod[slot] = area.ion.afac[ihx];
                filled = true;
            }
        }
        if filled {
            // Classify by the true height against the layer maxima.
            for ily in 0..3 {
                if ily == 1 && ctx.state.fi[k][1] <= 0.0 {
                    continue;
                }
                if modes.htmod[slot] <= ctx.state.hi[k][ily] {
                    modes.itmod[slot] = ily as i32 + 1;
                    inm[ily] = 1;
                    break;
                }
            }
        }
    }
    regmod(zon, modes, *ghop, ctx, muf, noise, freq, fsdead);
    if ireset {
        for il in 0..3 {
            muf.layers[il].sigl = osave[il].0;
            muf.layers[il].sigu = osave[il].1;
            muf.layers[il].yfot = osave[il].2;
            muf.layers[il].yhpf = osave[il].3;
            muf.layers[il].ymuf = osave[il].4;
        }
    }
}

// ---------------------------------------------------------------------
// ESMOD and ESREG

/// Port of `ESMOD`: up to two sporadic-E hops into `/ZON/` slots 4-5.
/// `fsdead` is the low-frequency cutoff from the controlling area.
fn esmod(
    zon: &mut Zon,
    ctx: &PassCtx,
    muf: &MufHour,
    noise: &NoiseResult,
    freq: R,
    fsdead: R,
) {
    // The weakest Es area governs: all modes are at least this good.
    let mut k = 0usize;
    for is in 0..ctx.state.km {
        if ctx.fs[k][1] - ctx.fs[is][1] > 0.0 {
            k = is;
        }
    }
    for i in 3..5 {
        zon.obf[i] = 1000.0;
        zon.adv[i] = 1000.0;
        zon.fslos[i] = 1000.0;
        zon.tloss[i] = 1000.0;
        zon.abps[i] = 1000.0;
        zon.eff[i] = 0.0;
        zon.grlos[i] = 1000.0;
        zon.rgain[i] = 0.0;
        zon.tgain[i] = 0.0;
        zon.hn[i] = -1.0;
        zon.prob[i] = 0.001;
        zon.crel[i] = -1000.0;
        zon.rely[i] = 0.001;
        zon.spro[i] = 0.001;
        zon.fldst[i] = -1000.0;
        zon.sigpow[i] = -1000.0;
        zon.sn[i] = -1000.0;
        zon.hp[i] = -1.0;
        zon.b[i] = -1.0;
        zon.nmode[i] = 5;
        zon.tllow[i] = 10.0;
        zon.timed[i] = -1.0;
        zon.tlhgh[i] = 10.0;
    }
    if freq <= fsdead {
        return;
    }
    if ctx.fs[k][1] <= 0.0 {
        return;
    }
    // Virtual heights equal true heights for the thin Es layer.
    let sdmax = 2.0 * RZ * (PIO2 - (1.0 / (1.0 + ctx.hs[k] / RZ)).asin());
    let ihsrt = (ctx.gcdkm / sdmax + 1.0) as i32;
    let ihstp = 2;
    if ihsrt > 2 {
        return;
    }
    let mut ih = 3usize;
    let ac = 677.2 * ctx.sig.acav / ((freq + ctx.geog.gyz[k]).powf(1.98) + 10.2);
    for ihop in ihsrt..=ihstp {
        ih += 1;
        let i = ih - 1;
        let gp = ihop as R;
        let ghop = ctx.gcd / gp;
        let thet = 0.5 * ghop;
        let tans = thet.sin() / (1.0 - thet.cos() + ctx.hs[k] / RZ);
        let psi = tans.atan();
        let secs = 1.0 / psi.cos();
        let sfvmod = freq / secs;
        let esd = ctx.fs[k][1] * secs;
        let del = PIO2 - thet - psi;
        let cdel = del.cos();
        let adel = del * R2D;
        if adel < ctx.deck.amind {
            continue;
        }
        let path = 2.0 * (0.5 * ghop).sin() * (RZ + ctx.hs[k]) / cdel;
        let hop = ihop as R;
        let path = hop * path;
        let sflos = 32.45 + 20.0 * (path * freq).log10();
        let sinp = RZ * cdel / (RZ + 100.0);
        let cosp = (1.0 - sinp * sinp).sqrt();
        let mut sabps = ac / cosp;
        // Remove E-layer bending above the Es height when the Es
        // equivalent vertical is below the E critical.
        let mut adx: R = 0.0;
        if ctx.state.fi[k][0] > sfvmod {
            adx = ctx.sig.afe + ctx.sig.bfe * (sfvmod / ctx.state.fi[k][0]).ln();
        }
        sabps += adx;
        // Reflection losses from the probability of reflection.
        let pros = prbmuf(muf, freq, esd, muf.layers[3].ymuf, 4).min(0.90);
        let refm = 8.9136 * pros.powf(-0.7);
        let esd = ctx.fs[k][0] * secs;
        let ps = prbmuf(muf, freq, esd, muf.layers[3].yfot, 4).min(0.9);
        let refl = 8.9136 * ps.powf(-0.7);
        let esd = ctx.fs[k][2] * secs;
        let ps = prbmuf(muf, freq, esd, muf.layers[3].yhpf, 4).min(0.9);
        let refu = 8.9136 * ps.powf(-0.7);
        zon.tllow[i] = (ctx.sig.dsl + hop * (refl - refm)).min(25.0);
        zon.tlhgh[i] = (ctx.sig.dsu + hop * (refm - refu)).min(25.0);
        let sgrlos = ground_loss_avg(ctx, del, freq, ctx.state.km);
        let (stgain, _steff) = ctx.ants.gain(1, del, freq);
        let (srgain, sreff) = ctx.ants.gain(2, del, freq);
        zon.eff[i] = sreff;
        // Note the ADX double-count: SABPS already contains it and the
        // sum below adds it once more, as the source does.
        let xtlos = sflos + hop * (sabps + refm + adx) + (hop - 1.0) * sgrlos - srgain - stgain
            + ctx.sig.asm;
        zon.fldst[i] = 107.2 + ctx.ants.pwrdb(freq) + 20.0 * freq.log10() - xtlos - srgain;
        zon.sigpow[i] = ctx.ants.pwrdb(freq) - xtlos;
        let pros2 = prbmuf(muf, freq, muf.layers[3].ymuf, muf.layers[3].ymuf, 4);
        zon.obf[i] = 8.9136 * pros2.powf(-0.7);
        zon.adv[i] = 0.0;
        zon.fslos[i] = sflos;
        zon.tloss[i] = xtlos;
        zon.abps[i] = ac / cosp + adx;
        zon.grlos[i] = sgrlos;
        zon.rgain[i] = srgain;
        zon.tgain[i] = stgain;
        zon.hn[i] = hop;
        zon.sigpow[i] = ctx.ants.pwrdb(freq) - xtlos;
        zon.sn[i] = zon.sigpow[i] - noise.rcnse - zon.eff[i];
        zon.hp[i] = ctx.hs[k];
        zon.b[i] = adel;
        zon.nmode[i] = 4;
        // F-days from the unclamped reflection probability.
        zon.prob[i] = pros2.powi(ihop);
        zon.timed[i] = path / VOFL;
    }
}

/// Port of `ESREG`: presets `/ZON/` slots 6-7. The Es-F mixed-mode
/// calculation after the preset sits behind an unconditional `RETURN`
/// ("ERRORS IN THE CODE BELOW") and is not ported.
fn esreg(zon: &mut Zon) {
    for i in 5..7 {
        zon.abps[i] = 1000.0;
        zon.crel[i] = 1000.0;
        zon.eff[i] = 0.0;
        zon.fldst[i] = -1000.0;
        zon.grlos[i] = 1000.0;
        zon.hn[i] = -1.0;
        zon.hp[i] = -1.0;
        zon.prob[i] = 0.001;
        zon.rely[i] = 0.001;
        zon.rgain[i] = 0.0;
        zon.tgain[i] = 0.0;
        zon.timed[i] = -1.0;
        zon.tloss[i] = 1000.0;
        zon.b[i] = -1.0;
        zon.fslos[i] = 1000.0;
        zon.adv[i] = 1000.0;
        zon.obf[i] = 1000.0;
        zon.nmode[i] = 5;
        zon.tllow[i] = 10.0;
        zon.tlhgh[i] = 10.0;
        zon.sigpow[i] = -1000.0;
        zon.sn[i] = -1000.0;
        zon.spro[i] = 0.001;
    }
}

// ---------------------------------------------------------------------
// RELBIL, SERPRB, MPATH

/// The normal-distribution table shared by `RELBIL` and `SERPRB`.
const TME: [R; 10] = [
    0.0, 0.1257, 0.2533, 0.3853, 0.5244, 0.6745, 0.8416, 1.0364, 1.2815, 1.6449,
];

/// Port of `RELBIL`: reliability per mode, selection of the most
/// reliable, power-summed combination and the required power gain.
/// Writes the frequency's `/SON/` slot.
fn relbil(
    lp: &mut ModeLoopState,
    ifx: usize,
    noise: &NoiseResult,
    deck: &DeckParams,
    ants: &super::antenna::AntennaSet,
    freq: R,
) {
    const XEPS: R = 0.05;
    let inum = lp.all.nmmod;
    if inum == 0 {
        return;
    }
    let du2 = noise.du * noise.du;
    let dl2 = noise.dl * noise.dl;
    for im in 0..inum {
        if lp.all.hp[im] <= 70.0 {
            lp.all.crel[im] = 0.001;
            lp.all.rely[im] = 0.001;
        } else {
            let dslf = lp.all.tllow[im];
            let dsuf = lp.all.tlhgh[im];
            // The 1994 change: the low end of the SNR distribution
            // combines high noise with low signal.
            lp.d10r = (du2 + dslf * dslf).sqrt();
            lp.d50r = lp.all.sn[im];
            lp.d90r = (dl2 + dsuf * dsuf).sqrt();
            let z = deck.rsn - lp.d50r;
            let z = if z <= 0.0 {
                z / (lp.d10r / 1.28)
            } else {
                z / (lp.d90r / 1.28)
            };
            lp.all.rely[im] = 1.0 - fnorml(z);
            lp.all.crel[im] = 1000.0;
        }
    }
    // Most reliable mode: start from the maximum reliability, then
    // prefer fewer hops (or better SNR at equal hops) among modes
    // within XEPS of it.
    let mut irmax = 0usize;
    for im in 1..inum {
        if lp.all.rely[im] > lp.all.rely[irmax] {
            irmax = im;
        }
    }
    let mut ir = irmax;
    let mut xrel = lp.all.rely[ir];
    let mut xhn = lp.all.hn[ir];
    let mut xsn = lp.all.sn[ir];
    for im in 0..inum {
        if im == irmax {
            continue;
        }
        if xrel < 0.00000001 {
            xrel = 0.00000001;
        }
        let reltest = (lp.all.rely[im] - xrel).abs();
        if reltest >= XEPS {
            continue;
        }
        let better = if (xhn - lp.all.hn[im]).abs() <= XEPS {
            xsn < lp.all.sn[im]
        } else {
            xhn > lp.all.hn[im]
        };
        if better {
            ir = im;
            xhn = lp.all.hn[im];
            xsn = lp.all.sn[im];
            xrel = lp.all.rely[im];
        }
    }
    lp.all.nrel = ir + 1;
    let is = lp.all.nmode[ir];
    if inum == 1 {
        lp.son[ifx].reliab = lp.all.rely[ir];
        lp.son[ifx].dblosl = lp.all.tllow[ir];
        lp.son[ifx].dblosu = lp.all.tlhgh[ir];
        lp.son[ifx].dbu = lp.all.fldst[ir];
        lp.son[ifx].sndb = lp.all.sn[ir];
        lp.son[ifx].dbw = lp.all.sigpow[ir];
    } else {
        // Add the signals in watts (random phase).
        let mut xdslw: R = 0.0;
        let mut xsigs: R = 0.0;
        let mut xdsup: R = 0.0;
        let mut xfld: R = 0.0;
        let mut dxsigs: R = -1000.0;
        let mut dxfld: R = -1000.0;
        let mut dxdslw: R = -1000.0;
        let mut dxdsup: R = -1000.0;
        for iv in 0..inum {
            dxsigs = dxsigs.max(lp.all.sigpow[iv]);
            dxfld = dxfld.max(lp.all.fldst[iv]);
            dxdslw = dxdslw.max(lp.all.sigpow[iv] - lp.all.tllow[iv]);
            dxdsup = dxdsup.max(lp.all.sigpow[iv] + lp.all.tlhgh[iv]);
        }
        for im in 0..inum {
            let zexp = 0.1 * (lp.all.sigpow[im] - lp.all.tllow[im] - dxdslw);
            if zexp > -10.0 {
                xdslw += (10.0 as R).powf(zexp);
            }
            let zexp = 0.1 * (lp.all.sigpow[im] - dxsigs);
            if zexp > -10.0 {
                xsigs += (10.0 as R).powf(zexp);
            }
            let zexp = 0.1 * (lp.all.sigpow[im] + lp.all.tlhgh[im] - dxdsup);
            if zexp > -10.0 {
                xdsup += (10.0 as R).powf(zexp);
            }
            // Field strength separately, because of the receive antenna.
            let zexp = 0.1 * (lp.all.fldst[im] - dxfld);
            if zexp > -10.0 {
                xfld += (10.0 as R).powf(zexp);
            }
        }
        let mut sigmed: R = -500.0;
        if xsigs > 0.0 {
            sigmed = dxsigs + 10.0 * xsigs.log10();
        }
        let dblosl = if xdslw > 0.0 {
            (sigmed - 10.0 * xdslw.log10() - dxdslw).abs()
        } else {
            0.0
        };
        // The 2002 minimum and maximum.
        let dblosl = dblosl.clamp(0.2, 30.0);
        let dblosu = if xdsup > 0.0 {
            (dxdsup + 10.0 * xdsup.log10() - sigmed).abs()
        } else {
            0.0
        };
        let dblosu = dblosu.clamp(0.2, 30.0);
        lp.son[ifx].dblosl = dblosl;
        lp.son[ifx].dblosu = dblosu;
        lp.son[ifx].dbw = sigmed;
        let delsig = sigmed - lp.all.sigpow[ir];
        lp.son[ifx].dbu = if xfld > 0.0 {
            dxfld + 10.0 * xfld.log10()
        } else {
            -500.0
        };
        lp.son[ifx].sndb = lp.all.sn[ir] + delsig;
        // Reliability of the sum of modes.
        lp.d10r = (du2 + dblosl * dblosl).sqrt();
        lp.d50r = lp.son[ifx].sndb;
        lp.d90r = (dl2 + dblosu * dblosu).sqrt();
        if lp.d10r < 0.2 {
            lp.d10r = 0.2;
        }
        if lp.d90r > 30.0 {
            lp.d90r = 30.0;
        }
        let z = deck.rsn - lp.d50r;
        let z = if z <= 0.0 {
            z / (lp.d10r / 1.28)
        } else {
            z / (lp.d90r / 1.28)
        };
        lp.son[ifx].reliab = 1.0 - fnorml(z);
    }
    lp.son[ifx].gaint = lp.all.tgain[ir];
    lp.son[ifx].gainr = lp.all.rgain[ir];
    lp.son[ifx].snrlw = lp.d10r;
    lp.son[ifx].snrup = lp.d90r;
    lp.son[ifx].angle = lp.all.b[ir];
    lp.son[ifx].vhigh = lp.all.hp[ir];
    lp.son[ifx].delay = lp.all.timed[ir];
    lp.son[ifx].dblos = lp.all.tloss[ir];
    // The 2014 change: with summed signal powers the transmission loss
    // is recalculated from the summed power.
    lp.son[ifx].dblos = ants.pwrdb(freq) - lp.son[ifx].dbw;
    lp.son[ifx].cprob = lp.all.prob[ir];
    lp.son[ifx].mode_layer = is;
    lp.son[ifx].nhp = lp.all.hn[ir] as i32;
    lp.son[ifx].xnynois = noise.rcnse;
    lp.son[ifx].du_nois = noise.du;
    lp.son[ifx].dl_nois = noise.dl;
    lp.son[ifx].rneff = lp.all.eff[ir];
    // Required power gain for the specified reliability.
    let itm = (((deck.lufp - 50).abs() / 5 + 1).min(10)) as usize;
    let tmx = TME[itm - 1] / TME[8];
    lp.son[ifx].snpr = if deck.lufp < 50 {
        -(lp.d50r + tmx * lp.d90r) + deck.rsn
    } else {
        -(lp.d50r - tmx * lp.d10r) + deck.rsn
    };
    lp.son[ifx].snxx = deck.rsn - lp.son[ifx].snpr;
    let _ = freq;
}

/// Port of `SERPRB`: the service probability per mode, keeping the
/// maximum. (`D10S`/`D50S`/`D90S` feed only the method-25 output and
/// are not kept.)
fn serprb(
    lp: &mut ModeLoopState,
    ifx: usize,
    noise: &NoiseResult,
    sig: &SignalDistribution,
    deck: &DeckParams,
) {
    // DR is the prediction error in the required SNR.
    const DR: R = 2.0;
    let itm = (((deck.lufp - 50).abs() / 5 + 1).min(10)) as usize;
    let tmx = TME[itm - 1];
    let mut d10sa = [0.0 as R; 20];
    let mut d50sa = [0.0 as R; 20];
    for im in 0..lp.all.nmmod {
        if lp.all.hp[im] <= 70.0 {
            lp.all.spro[im] = 0.001;
            continue;
        }
        let (dn, ds, xlh, dso, dno);
        if deck.lufp >= 50 {
            dn = tmx * noise.du / DCL;
            ds = tmx * lp.all.tllow[im] / DCL;
            xlh = -1.0 as R;
            dso = tmx * sig.sus;
            dno = tmx * noise.sygu / DCL;
        } else {
            dn = tmx * noise.dl / DCL;
            ds = tmx * lp.all.tlhgh[im] / DCL;
            xlh = 1.0;
            dso = tmx * sig.sls;
            dno = tmx * noise.sygl / DCL;
        }
        d50sa[im] = (dn * dn + ds * ds).sqrt();
        d10sa[im] = d50sa[im]
            + (noise.sigm * noise.sigm + sig.ads * sig.ads + dno * dno + dso * dso + DR * DR)
                .sqrt();
        d50sa[im] = lp.all.sn[im] + xlh * d50sa[im];
        let z = (deck.rsn - d50sa[im]) / d10sa[im];
        lp.all.spro[im] = 1.0 - fnorml(z);
    }
    let mut imax = 0usize;
    for i in 0..lp.all.nmmod {
        if lp.all.spro[i] > lp.all.spro[imax] {
            imax = i;
        }
    }
    lp.son[ifx].sprob = lp.all.spro[imax];
}

/// Port of `MPATH` (2009 revision): the reliability of the next most
/// probable mode outside the tolerable time delay whose power is within
/// `PMP` of the most reliable mode. Method 30 skips it beyond 7000 km.
fn mpath(lp: &mut ModeLoopState, ifx: usize, deck: &DeckParams, gcdkm: R) {
    lp.son[ifx].probmp = 0.001;
    if deck.dmp <= 0.0 || deck.pmp <= 0.0 {
        return;
    }
    // `IF(method.eq.20 .and. mspec.eq.121 .and. gcdkm.gt.7000.)`:
    // multipath is declared invalid past 7000 km for card method 30
    // alone, because that is where its two models blend. The other
    // systems methods compute it at any distance.
    if deck.method == 30 && gcdkm > 7000.0 {
        return;
    }
    if lp.all.nmmod == 0 {
        // NREL is stale here; the mode loop below would be empty, so
        // the source's out-of-range SIGPOW read has no visible effect.
        return;
    }
    let nrel = lp.all.nrel - 1;
    let sig_power = lp.all.sigpow[nrel];
    let sig_power_limit = sig_power - deck.pmp;
    let ttim = lp.all.timed[nrel];
    for im in 0..lp.all.nmmod {
        if im == nrel {
            continue;
        }
        if lp.all.hp[im] <= 0.0 {
            continue;
        }
        if (lp.all.timed[im] - ttim).abs() <= deck.dmp {
            continue;
        }
        if lp.all.sigpow[im] < sig_power_limit {
            continue;
        }
        lp.son[ifx].probmp = lp.son[ifx].probmp.max(lp.all.rely[im]);
    }
}

// ---------------------------------------------------------------------
// The long-path chain: SETTXR, SELTXR, GMLOSS, LNGPAT and helpers

/// Port of `SETTXR_orig`: losses for every reflectrix row at the two
/// path ends, forcing an over-the-MUF row when no row qualifies.
// The loops walk several parallel Fortran arrays by index.
#[allow(clippy::needless_range_loop)]
fn settxr(lp: &mut ModeLoopState, ctx: &PassCtx, muf: &MufHour, freq: R, itxrcp: [usize; 2]) {
    let ModeLoopState {
        areas,
        reflectrix,
        efflp,
        all,
        ..
    } = lp;
    let dend = ctx.gcdkm.min(4000.0);
    for jj in 0..2 {
        let k = itxrcp[jj] - 1;
        let rfx = &mut reflectrix[k];
        for ia in 0..45 {
            rfx.gml[ia] = -999.0;
            rfx.fhp[ia] = 999.0;
        }
        // Force the over-the-MUF mode into row 1 unless a row supports
        // at least one hop at a legal angle.
        let mut force = true;
        for ia in 0..45 {
            if rfx.hpflx[ia] - 70.0 < 0.0 {
                break;
            }
            let xhpm = dend / rfx.gdflx[ia];
            if xhpm >= 0.9 && rfx.delfx[ia] >= ctx.deck.amind {
                force = false;
                break;
            }
        }
        if force {
            let im = muf.modmuf as usize - 1;
            rfx.delfx[0] = muf.layers[im].delmuf;
            rfx.hpflx[0] = muf.layers[im].hpmuf;
            rfx.htflx[0] = muf.layers[im].htmuf;
            rfx.fvflx[0] = muf.layers[im].fvmuf;
            rfx.afflx[0] = muf.layers[im].afmuf;
            let xhop = muf.layers[im].nhopmf as R;
            rfx.gdflx[0] = ctx.gcdkm / xhop;
            rfx.imode[0] = muf.modmuf;
        }
        let bc = (freq + ctx.geog.gyz[k]).powf(1.98);
        let ac = 677.2 * ctx.geog.abiy[k];
        for ia in 0..45 {
            rfx.tgainx[ia] = -10.0;
            rfx.andvx[ia] = 1000.0;
            rfx.advx[ia] = 1000.0;
            rfx.grlosx[ia] = 0.0;
            if rfx.hpflx[ia] < 70.0 {
                continue;
            }
            if rfx.delfx[ia] < ctx.deck.amind {
                continue;
            }
            let del = rfx.delfx[ia] * D2R;
            let cdel = del.cos();
            if ctx.state.fi[k][0] >= rfx.fvflx[ia] {
                // D-E layer mode.
                let xnsq = if ctx.sig.htloss <= rfx.htflx[ia] {
                    10.2
                } else {
                    let hnux = 61.0 + 3.0 * (rfx.htflx[ia] - 70.0) / 18.0;
                    ctx.sig.xnuz * (-2.0 * (hnux - 60.0) / ctx.sig.hnu).exp()
                };
                let heff = rfx.htflx[ia].min(100.0);
                let sinp = RZ * cdel / (RZ + heff);
                let secp = 1.0 / (1.0 - sinp * sinp).sqrt();
                rfx.andvx[ia] = secp * ac / (bc + xnsq);
                let xv = (rfx.fvflx[ia] / ctx.state.fi[k][0]).max(ctx.sig.xve);
                let adx = ctx.sig.afe + ctx.sig.bfe * xv.ln();
                let sinp = RZ * cdel / (RZ + rfx.hpflx[ia]);
                let secp = 1.0 / (1.0 - sinp * sinp).sqrt();
                rfx.advx[ia] = secp
                    * rfx.afflx[ia]
                    * ((rfx.fvflx[ia] + ctx.geog.gyz[k]).powf(1.98) + xnsq)
                    / (bc + xnsq)
                    + adx;
                rfx.aofx[ia] = 0.0;
            } else {
                // F layer mode.
                let xnsq = 10.2;
                let sinp = RZ * cdel / (RZ + 100.0);
                let secp = 1.0 / (1.0 - sinp * sinp).sqrt();
                rfx.andvx[ia] = secp * ac / (bc + xnsq);
                let sinp = RZ * cdel / (RZ + rfx.hpflx[ia]);
                let secp = 1.0 / (1.0 - sinp * sinp).sqrt();
                rfx.advx[ia] = secp
                    * rfx.afflx[ia]
                    * ((rfx.fvflx[ia] + ctx.geog.gyz[k]).powf(1.98) + xnsq)
                    / (bc + xnsq);
                rfx.aofx[ia] = 0.0;
                if ctx.fs[k][1] > 0.0 {
                    // Es obscuration; note no 3 MHz cap on FSDEAD here.
                    let fsdead = areas[k].ifob[0][2] as R / 1000.0;
                    let fmhz = freq.max(fsdead);
                    let sins = RZ * cdel / (RZ + ctx.hs[k]);
                    let secs = 1.0 / (1.0 - sins * sins).sqrt();
                    let esd = ctx.fs[k][1] * secs;
                    let p = prbmuf(muf, fmhz, esd, muf.layers[3].ymuf, 4).clamp(0.1, 0.9);
                    rfx.aofx[ia] = -10.0 * (1.0 - p).log10();
                }
            }
            let mut y: R = 0.0;
            for ig in 0..ctx.state.km {
                y += gain_ground(del, freq, ctx.geog.sigpat[ig], ctx.geog.epspat[ig]);
            }
            rfx.grlosx[ia] = y / ctx.state.km as R;
            // GAIN(JJ, ...): JJ is 1 at the transmit end, 2 at the
            // receive end of the long path.
            let (g, teff) = ctx.ants.gain(jj as i32 + 1, del, freq);
            rfx.tgainx[ia] = g;
            if jj == 1 {
                efflp[ia] = teff;
            }
            let sphet = RZ * cdel / (RZ + rfx.htflx[ia]);
            let cphet = (1.0 - sphet * sphet).max(0.00000001).sqrt();
            // The source overrides the row's mode with MODMUF here.
            let is = muf.modmuf as usize;
            let pros = prbmuf(muf, freq, muf.layers[is - 1].ymuf, muf.layers[is - 1].ymuf, is);
            let xls = -10.0 * pros.log10() / cphet;
            rfx.andvx[ia] += xls;
            all.prob[jj] = pros;
        }
    }
}

/// Port of `SELTXR_orig`: selects the reflectrix row at each end with
/// the best gain-minus-loss, breaking near-ties by hop fraction and
/// then by closeness to the a-priori optimum angle. Returns `LTXRGM`
/// (1-based; zero or less means no mode).
// XHPM's later updates mirror the source's dead stores.
#[allow(unused_assignments)]
fn seltxr(lp: &mut ModeLoopState, ctx: &PassCtx, itxrcp: [usize; 2]) -> [i32; 2] {
    let dend = ctx.gcdkm.min(4000.0);
    let mut ltxrgm = [1i32; 2];
    for jj in 0..2 {
        let k = itxrcp[jj] - 1;
        let rfx = &mut lp.reflectrix[k];
        let mut l = 1i32;
        let xhpm0 = dend / rfx.gdflx[0];
        let ihop0 = xhpm0 as i32;
        let mut failed = false;
        if xhpm0 < 0.9 {
            loop {
                l += 1;
                if l > 45 {
                    // The source would read past the table; its IAFTXR
                    // guard below normally stops the walk first.
                    failed = true;
                    break;
                }
                if rfx.hpflx[l as usize - 1] <= 70.0 {
                    failed = true;
                    break;
                }
                if l > rfx.iaftxr as i32 {
                    failed = true;
                    break;
                }
                if rfx.delfx[l as usize - 1] < ctx.deck.amind {
                    continue;
                }
                break;
            }
        }
        if failed {
            ltxrgm[jj] = l - 1;
            continue;
        }
        // HOPX and YHPM keep the first row's hop split even when the
        // walk above advanced the row, exactly like the source.
        let mut xhpm = xhpm0;
        let hopx = ihop0 as R;
        let mut yhpm = xhpm0 - hopx;
        // Only the XFRACT = 0.5 pass runs (the 0.05 pass is behind an
        // INFO diagnostic bit).
        if yhpm > 0.5 {
            yhpm = 1.0 - yhpm;
        }
        let li = l as usize - 1;
        let mut gmax = rfx.tgainx[li] - xhpm * (rfx.andvx[li] + rfx.advx[li]);
        let mut delmax = (rfx.delfx[li] - DELOPT).abs();
        let ls = l;
        for ia in ls..=(rfx.iaftxr as i32) {
            let i = ia as usize - 1;
            if rfx.hpflx[i] <= 70.0 {
                continue;
            }
            let xhop = dend / rfx.gdflx[i];
            let ihop = xhop as i32;
            if xhop < 0.9 {
                continue;
            }
            let hopx = ihop as R;
            let mut yhop = xhop - hopx;
            if yhop > 0.5 {
                yhop = 1.0 - yhop;
            }
            let gnow = rfx.tgainx[i] - xhop * (rfx.andvx[i] + rfx.advx[i]);
            rfx.gml[i] = gnow;
            rfx.fhp[i] = yhop;
            let delnow = (rfx.delfx[i] - DELOPT).abs();
            // First choice gain-minus-loss, second the hop fraction,
            // third the takeoff angle.
            let select = if (gnow - gmax).abs() - GMIN <= 0.0 {
                if (yhop - yhpm).abs() - YMIN <= 0.0 {
                    delnow < delmax
                } else {
                    yhop < yhpm
                }
            } else {
                gnow > gmax
            };
            if select {
                l = ia;
                xhpm = xhop;
                gmax = gnow;
                delmax = delnow;
                yhpm = yhop;
            }
        }
        ltxrgm[jj] = l;
    }
    ltxrgm
}

/// Port of `GMLOSS`: presets `/ZON/` and calls `SETTXR`. (The
/// `TXRGML` fill is omitted: nothing reads it.)
fn gmloss(lp: &mut ModeLoopState, ctx: &PassCtx, muf: &MufHour, freq: R, itxrcp: [usize; 2]) {
    for im in 0..7 {
        lp.zon.obf[im] = 1000.0;
        lp.zon.adv[im] = 1000.0;
        lp.zon.fslos[im] = 1000.0;
        lp.zon.tloss[im] = 1000.0;
        lp.zon.abps[im] = 1000.0;
        lp.zon.eff[im] = 0.0;
        lp.zon.grlos[im] = 1000.0;
        lp.zon.rgain[im] = 0.0;
        lp.zon.tgain[im] = 0.0;
        lp.zon.hn[im] = -1.0;
        lp.zon.prob[im] = 0.001;
        lp.zon.crel[im] = 0.001;
        lp.zon.rely[im] = 0.001;
        lp.zon.spro[im] = 0.001;
        lp.zon.fldst[im] = -1000.0;
        lp.zon.sigpow[im] = -1000.0;
        lp.zon.sn[im] = -1000.0;
        lp.zon.timed[im] = -1.0;
        lp.zon.hp[im] = -1.0;
        lp.zon.b[im] = -1.0;
        lp.zon.nmode[im] = 5;
    }
    settxr(lp, ctx, muf, freq, itxrcp);
}

/// Port of `CONVH`: the geometrical convergence factor and group path.
fn convh(gd: R, phe: R, del: R, hp: R, ray: R) -> (R, R) {
    let denom = gd.sin().abs().max(0.000001);
    let psi = PIO2 - del - phe;
    let gm = (gd - 2.0 * psi).max(0.001);
    let ptot = 2.0 * ray + (RZ + hp) * gm;
    let smallc = (ptot / RZ) * del.cos() / denom;
    let ch = (10.0 * smallc.log10()).min(15.0);
    (ch, ptot)
}

/// Port of `GETTOP`: the over-the-top distance supporting M modes
/// (night-day-night paths).
fn gettop(state: &IonoState, dikm: R, freq: R) -> R {
    let del = DELOPT * D2R;
    let rcosd = RZ * del.cos();
    let mut fpe = [0.0 as R; 3];
    for (jf, f) in fpe.iter_mut().enumerate() {
        let ht = state.hi[jf][0];
        let fv = state.fi[jf][0];
        let sphe = rcosd / (RZ + ht);
        let sphe = (sphe * sphe).min(0.9999);
        *f = fv / (1.0 - sphe).sqrt();
    }
    let fp1 = fpe[0].max(fpe[2]);
    let fp2 = fpe[1];
    let fp3 = fpe[0].min(fpe[2]);
    if fp1 - fp2 >= 0.0 {
        return 0.0;
    }
    if freq - fp2 - 0.001 >= 0.0 {
        return 0.0;
    }
    if freq - fp1 + 0.001 <= 0.0 {
        return 0.0;
    }
    let gmkm = 0.5 * dikm * (fp2 - freq) * (1.0 / (fp2 - fp1) + 1.0 / (fp2 - fp3));
    if gmkm - 1000.0 <= 0.0 {
        return 0.0;
    }
    // Must be able to get up there for an M mode.
    if dikm - gmkm - 1000.0 <= 0.0 {
        return 0.0;
    }
    gmkm
}

/// Port of `TABS`: loss per kilometre for the path portion that misses
/// the absorbing region.
fn tabs_loss(geog: &Geog, del: R, hp: R, dm: R, freq: R) -> R {
    if dm <= 0.0 || hp - 70.0 <= 0.0 {
        return 1000.0;
    }
    let xi: R = 0.1;
    let ab = 10.2 + (freq + geog.gyz[2]).powf(1.98);
    let am = 677.2 * xi / ab;
    let rcosd = RZ * del.cos();
    let sphe = rcosd / (RZ + hp);
    let phe = (sphe / (1.0 - sphe * sphe).sqrt()).atan();
    let psi = PIO2 - phe - del;
    let xmhop = 0.5 * dm / psi;
    let sphe = rcosd / (RZ + 100.0);
    let scphe = 1.0 / (1.0 - sphe * sphe).sqrt();
    let port = xmhop * scphe / (dm * RZ);
    am * port
}

/// Port of `BABS`: loss per kilometre through the absorbing region.
fn babs_loss(geog: &Geog, acav: R, del: R, hp: R, df: R, freq: R) -> R {
    if df <= 0.0 {
        return 1000.0;
    }
    if hp - 70.0 < 0.0 {
        return 1000.0;
    }
    let af = acav;
    let ab = 10.2 + (freq + geog.gyz[2]).powf(1.98);
    let af = 677.2 * af / ab;
    let rcosd = RZ * del.cos();
    let sphe = rcosd / (RZ + hp);
    let phe = (sphe / (1.0 - sphe * sphe).sqrt()).atan();
    let psi = PIO2 - phe - del;
    let xmhop = 0.5 * df / psi;
    let sphe = rcosd / (RZ + 100.0);
    let scphe = 1.0 / (1.0 - sphe * sphe).sqrt();
    let port = xmhop * scphe / (df * RZ);
    af * port
}

/// Port of `LNGPAT`: assembles the single long-path mode into the
/// accumulated-mode slot 1 (with the reception angle in slot 2).
#[allow(clippy::too_many_arguments)]
fn lngpat(
    lp: &mut ModeLoopState,
    ctx: &PassCtx,
    muf: &MufHour,
    noise: &NoiseResult,
    freq: R,
    itxrcp: [usize; 2],
    ltxrgm: [i32; 2],
) {
    if ltxrgm[0] <= 0 || ltxrgm[1] <= 0 {
        return;
    }
    let k2 = itxrcp[1] - 1;
    let i1 = ltxrgm[0] as usize - 1;
    let i2 = ltxrgm[1] as usize - 1;
    let (r1, r2) = if k2 == 0 {
        let r = &lp.reflectrix[0];
        (r, r)
    } else {
        (&lp.reflectrix[0], &lp.reflectrix[k2])
    };
    let del = 0.5 * D2R * (r1.delfx[i1] + r2.delfx[i2]);
    let hpx = 0.5 * (r1.hpflx[i1] + r2.hpflx[i2]);
    let rcosd = RZ * del.cos();
    let sphe = rcosd / (RZ + hpx);
    let phe = (sphe / (1.0 - sphe * sphe).sqrt()).atan();
    let dt = 0.5 * r1.gdflx[i1] / RZ;
    let dr = 0.5 * r2.gdflx[i2] / RZ;
    let ray = RZ * (phe + del).cos() / sphe;
    let (ch, ptot) = convh(ctx.gcd, phe, del, hpx, ray);
    let free = 36.58 + 20.0 * (0.6214 * ptot * freq).log10() - ch;
    let di = ctx.gcd - dt - dr;
    let dikm = di * RZ;
    let (dm, am, af, df);
    if dikm <= -1.0 {
        dm = 0.0;
        am = 0.0;
        af = 0.0;
        df = 0.0;
    } else {
        let gmkm = gettop(ctx.state, dikm, freq);
        dm = gmkm / RZ;
        df = (di - dm).max(0.0);
        am = tabs_loss(ctx.geog, del, hpx, dm, freq);
        af = babs_loss(ctx.geog, ctx.sig.acav, del, hpx, df, freq);
    }
    let xlm = am * dm * RZ;
    let xlf = af * df * RZ;
    let xlt = (r1.andvx[i1] + r1.advx[i1]) / 2.0;
    let xlr = (r2.andvx[i2] + r2.advx[i2]) / 2.0;
    let gloss = 0.5 * (r1.grlosx[i1] + r2.grlosx[i2]);
    let fhop = (df / (dr + dt)).max(1.0);
    let tlosl = free + xlt + xlm + xlf + xlr + (fhop - 1.0) * gloss + ctx.sig.asm
        - r2.tgainx[i2]
        - r1.tgainx[i1];
    let (b1, b2) = (r1.delfx[i1], r2.delfx[i2]);
    let (n1, n2) = (r1.imode[i1], r2.imode[i2]);
    let (rg, tg) = (r2.tgainx[i2], r1.tgainx[i1]);
    let (adv1, abps1, obf1) = (
        (r1.advx[i1] + r2.advx[i2]) * 0.5,
        (r1.andvx[i1] + r2.andvx[i2]) * 0.5,
        (r1.aofx[i1] + r2.aofx[i2]) * 0.5,
    );
    lp.all.nmmod = 1;
    lp.all.grlos[0] = gloss;
    lp.all.hn[0] = (0.5 * ctx.gcd / (dr + dt)).max(1.0);
    lp.all.hp[0] = hpx;
    lp.all.rgain[0] = rg;
    lp.all.tgain[0] = tg;
    lp.all.eff[0] = lp.efflp[i2];
    lp.all.timed[0] = ptot / VOFL;
    lp.all.tloss[0] = tlosl;
    lp.all.b[0] = b1;
    lp.all.b[1] = b2;
    lp.all.nmode[0] = n1;
    lp.all.nmode[1] = n2;
    lp.all.fslos[0] = free;
    lp.all.adv[0] = adv1;
    lp.all.abps[0] = abps1;
    lp.all.obf[0] = obf1;
    lp.all.sigpow[0] = ctx.ants.pwrdb(freq) - tlosl;
    lp.all.fldst[0] =
        107.2 + ctx.ants.pwrdb(freq) + 20.0 * freq.log10() - tlosl - lp.all.rgain[0];
    lp.all.sn[0] = lp.all.sigpow[0] - noise.rcnse - lp.all.eff[0];
    // Decile adjustments per mode type, averaged when the two ends
    // reflect from different layers.
    let decile = |is: i32| -> (R, R) {
        let lay = &muf.layers[is as usize - 1];
        let p = prbmuf(muf, freq, lay.ymuf, lay.ymuf, is as usize);
        let zlmuf = -10.0 * p.log10();
        let p = prbmuf(muf, freq, lay.yfot, lay.yfot, is as usize);
        let zlfot = -10.0 * p.log10();
        let p = prbmuf(muf, freq, lay.yhpf, lay.yhpf, is as usize);
        let zlhpf = -10.0 * p.log10();
        let cpr = lay.fvmuf / lay.ymuf;
        ((zlfot - zlmuf) / cpr, (zlmuf - zlhpf) / cpr)
    };
    let (mut dslf, mut dsuf) = decile(lp.all.nmode[0]);
    if lp.all.nmode[0] != lp.all.nmode[1] {
        let (dslff, dsuff) = (dslf, dsuf);
        let (d2l, d2u) = decile(lp.all.nmode[1]);
        dsuf = 0.5 * (d2u + dsuff);
        dslf = 0.5 * (d2l + dslff);
    }
    lp.all.tllow[0] = (ctx.sig.dsl + lp.all.hn[0] * dslf).min(25.0);
    lp.all.tlhgh[0] = (ctx.sig.dsu + lp.all.hn[0] * dsuf).min(25.0);
    lp.all.prob[0] = lp.all.prob[0].min(lp.all.prob[1]);
}

// ---------------------------------------------------------------------
// The frequency loop and the smoothing blend

/// Per-hour save arrays for the smoothing blend (`LUFFY`'s local
/// `y`/`x`/`z` arrays): index 0 is the short-path pass, 1 the long.
#[derive(Debug, Clone, Default)]
pub struct HourSaves {
    pub son: [[Son; 2]; 13],
    pub ytgain: [[R; 2]; 13],
    pub yrgain: [[R; 2]; 13],
    pub zdu: [R; 13],
    pub zdl: [R; 13],
    pub inmode: [i32; 13],
    pub zangler: [R; 13],
    pub zmoder: [i32; 13],
}

/// A snapshot of the reflectrix rows one `findf` call wrote, for trace
/// comparison.
#[derive(Debug, Clone)]
pub struct RfxSnapshot {
    /// 0-based area.
    pub k: usize,
    pub khz: i32,
    /// Per row: delfx, hpflx, htflx, gdflx, fvflx, afflx, imode.
    pub rows: Vec<[R; 7]>,
    pub dskpkm: R,
    pub dmaxkm: R,
    /// delskp, hpskp, htskp, fvskp, iskp — only when a row was written.
    pub skip: Option<[R; 5]>,
    pub delpen: [R; 3],
}

fn rfx_snapshot(rfx: &Reflectrix, k: usize, khz: i32) -> RfxSnapshot {
    let n = rfx.rows_this_call;
    RfxSnapshot {
        k,
        khz,
        rows: (0..n)
            .map(|i| {
                [
                    rfx.delfx[i],
                    rfx.hpflx[i],
                    rfx.htflx[i],
                    rfx.gdflx[i],
                    rfx.fvflx[i],
                    rfx.afflx[i],
                    rfx.imode[i] as R,
                ]
            })
            .collect(),
        dskpkm: rfx.dskpkm,
        dmaxkm: rfx.dmaxkm,
        skip: (n > 0).then_some([
            rfx.delskp,
            rfx.hpskp,
            rfx.htskp,
            rfx.fvskp,
            rfx.iskp as R,
        ]),
        delpen: rfx.delpen,
    }
}

/// The long-path loss tables at both ends, for trace comparison.
#[derive(Debug, Clone)]
pub struct LosSnapshot {
    pub ltxrgm: [i32; 2],
    /// Per end: (area, 45 rows of andvx, advx, aofx, grlosx, tgainx).
    pub ends: [(usize, Vec<[R; 5]>); 2],
}

/// The intermediates of one frequency slot, for trace comparison.
#[derive(Debug, Clone)]
pub struct FreqDebug {
    pub khz: i32,
    pub rfx: Vec<RfxSnapshot>,
    /// `/ZON/` after each hop's `inmuf` (keyed by hop count) and after
    /// `esreg` (keyed 99).
    pub zons: Vec<(i32, Zon)>,
    pub amd: Option<AllModes>,
    pub los: Option<LosSnapshot>,
    pub son: Son,
    /// `NREL` after `relbil` (stale from the previous frequency when
    /// no modes existed).
    pub nrel: usize,
}

fn nint_khz(freq: R) -> i32 {
    (freq * 1000.0).round() as i32
}

/// One pass of `LUFFY`'s frequency loop (`IPFG` 100 or 200): the hop
/// loop with sporadic-E modes for the short path, or the two-end
/// `findf`/`gmloss`/`seltxr`/`lngpat` chain for the long path, then
/// `genois`, `relbil`, `serprb` and `mpath` per frequency. `frel` is
/// the deck's slots 1-11 plus the circuit MUF in slot 12; `noise_for`
/// evaluates `genois` at a frequency. Returns per-slot intermediates.
pub fn luffy_freq_loop(
    lp: &mut ModeLoopState,
    ctx: &PassCtx,
    muf: &mut MufHour,
    noise_for: &dyn Fn(R) -> NoiseResult,
    frel: &[R; 12],
    saves: &mut HourSaves,
) -> Vec<Option<FreqDebug>> {
    let idx = usize::from(ctx.long);
    let mut ihmin = muf.layers[0].nhopmf;
    if muf.layers[1].nhopmf > 0 && muf.layers[1].nhopmf < ihmin {
        ihmin = muf.layers[1].nhopmf;
    }
    if muf.layers[2].nhopmf < ihmin {
        ihmin = muf.layers[2].nhopmf;
    }
    lp.son[12].nhp = -1;
    lp.son[12].mdl = if ctx.long { b'L' } else { b'S' };
    for son in lp.son.iter_mut().take(12) {
        son.mdl = b' ';
    }
    let itxrcp = [1usize, if ctx.state.kfx == 1 { 2 } else { ctx.state.kfx }];
    let mut out: Vec<Option<FreqDebug>> = Vec::with_capacity(12);
    for (ifx, &freq) in frel.iter().enumerate() {
        lp.son[ifx].nhp = -1;
        lp.son[ifx].sprob = 0.0;
        lp.son[ifx].probmp = 0.0;
        if freq <= 0.0 {
            out.push(None);
            continue;
        }
        let khz = nint_khz(freq);
        lp.all.reset();
        let noise = noise_for(freq);
        let mut dbg = FreqDebug {
            khz,
            rfx: Vec::new(),
            zons: Vec::new(),
            amd: None,
            los: None,
            son: Son::default(),
            nrel: 0,
        };
        if !ctx.long {
            let jm = ctx.jmode;
            // `K`, which is `JMODE` in this pass: the reflectrix and
            // the raysets come from the same area the mode routines
            // read. Only the short LUF pass separates the two.
            let kc = ctx.kctl;
            findf(
                &mut lp.reflectrix[kc],
                ctx.state,
                &lp.areas[kc],
                kc,
                freq,
                ctx.deck.amind,
                ctx.nang,
            );
            dbg.rfx.push(rfx_snapshot(&lp.reflectrix[kc], kc, khz));
            let fsdead = ((lp.areas[jm].ifob[0][2] as R) / 1000.0).min(3.0);
            let (mut ihsrt, ihstp);
            if lp.reflectrix[kc].dmaxkm <= 0.0 {
                // Only the over-the-MUF mode is possible.
                ihsrt = ihmin;
                ihstp = ihmin;
            } else {
                ihsrt = (ctx.gcdkm / lp.reflectrix[kc].dmaxkm + 1.0) as i32;
                let mut ihmax = (ctx.gcdkm / lp.reflectrix[kc].dskpkm) as i32;
                if ihsrt < ihmin {
                    ihsrt = ihmin;
                }
                if ihmax < ihsrt {
                    ihmax = ihsrt;
                }
                ihstp = ihmax.min(ihsrt + 2);
                if ihsrt > ihmin {
                    ihsrt = ihmin.max(ihstp - 2);
                }
            }
            for ihop in ihsrt..=ihstp {
                let hop = ihop as R;
                lp.ghop = ctx.gcd / hop;
                fdist(
                    &mut lp.modes[kc],
                    &lp.reflectrix[kc],
                    lp.ghop,
                    ctx.deck.amind,
                    freq,
                );
                let (modes, ghop) = (&mut lp.modes[jm], &mut lp.ghop);
                inmuf(
                    &mut lp.zon,
                    modes,
                    ghop,
                    muf,
                    ctx,
                    &lp.areas[jm],
                    &noise,
                    freq,
                    ihop,
                    fsdead,
                );
                lp.all.accumulate(&lp.zon, 1, 6);
                dbg.zons.push((ihop, lp.zon.clone()));
            }
            esmod(&mut lp.zon, ctx, muf, &noise, freq, fsdead);
            esreg(&mut lp.zon);
            dbg.zons.push((99, lp.zon.clone()));
            lp.all.accumulate(&lp.zon, 4, 5);
            dbg.amd = Some(lp.all.clone());
        } else {
            for &end in &itxrcp {
                let k = end - 1;
                findf(
                    &mut lp.reflectrix[k],
                    ctx.state,
                    &lp.areas[k],
                    k,
                    freq,
                    ctx.deck.amind,
                    ctx.nang,
                );
                dbg.rfx.push(rfx_snapshot(&lp.reflectrix[k], k, khz));
            }
            gmloss(lp, ctx, muf, freq, itxrcp);
            let ltxrgm = seltxr(lp, ctx, itxrcp);
            lngpat(lp, ctx, muf, &noise, freq, itxrcp, ltxrgm);
            dbg.los = Some(LosSnapshot {
                ltxrgm,
                ends: [0, 1].map(|jj: usize| {
                    let k = itxrcp[jj] - 1;
                    let rfx = &lp.reflectrix[k];
                    (
                        k,
                        (0..45)
                            .map(|i| {
                                [
                                    rfx.andvx[i],
                                    rfx.advx[i],
                                    rfx.aofx[i],
                                    rfx.grlosx[i],
                                    rfx.tgainx[i],
                                ]
                            })
                            .collect(),
                    )
                }),
            });
            dbg.amd = Some(lp.all.clone());
        }
        // The second GENOIS call recomputes the identical values; reuse.
        relbil(lp, ifx, &noise, &ctx.deck, ctx.ants, freq);
        // LINBOT(14) is always on for method 30, so SERPRB always runs.
        serprb(lp, ifx, &noise, ctx.sig, &ctx.deck);
        if !ctx.long {
            mpath(lp, ifx, &ctx.deck, ctx.gcdkm);
        } else {
            lp.son[ifx].moder_layer = lp.all.nmode[1];
            lp.son[ifx].angler = lp.all.b[1];
        }
        dbg.son = lp.son[ifx];
        dbg.nrel = lp.all.nrel;
        // The MSPEC = 121 save block.
        saves.son[ifx][idx] = lp.son[ifx];
        saves.ytgain[ifx][idx] = lp.all.tgain[0];
        saves.yrgain[ifx][idx] = lp.all.rgain[0];
        if ctx.long {
            saves.zdu[ifx] = noise.du;
            saves.zdl[ifx] = noise.dl;
            saves.inmode[ifx] = lp.all.nmode[0];
            saves.zangler[ifx] = lp.son[ifx].angler;
            saves.zmoder[ifx] = lp.son[ifx].moder_layer;
        }
        out.push(Some(dbg));
    }
    out
}


/// `FRQCOM`: the 2-40 MHz frequency complement the LUF search sweeps.
///
/// Slot 12 (index 11) always carries the circuit MUF. `ifreq = -10`
/// instead sets slot 1 to the FOT and returns. The remaining slots
/// spread from 2 MHz up to the HPF (at most 40), inserting the lower
/// of the E and F2 MUFs as an explicit point when it falls inside the
/// range.
pub fn frqcom(muf: &MufHour, ifreq: i32) -> [R; 13] {
    const FLOW: R = 2.0;
    const FHIGH: R = 40.0;
    let mut frea = [0.0 as R; 13];
    frea[11] = muf.allmuf;
    if ifreq + 10 == 0 {
        frea[0] = muf.fot;
        return frea;
    }
    let mut xfl = muf.emuf.min(muf.f2muf);
    let mut xfh = muf.hpf;
    xfl = xfl.max(FLOW);
    xfh = xfh.min(FHIGH);
    if xfh <= FLOW {
        // Case 1, "not likely to occur".
        frea[0] = FLOW;
        for i in 1..11 {
            frea[i] = frea[i - 1] + 2.0;
        }
        return frea;
    }
    if xfl <= FLOW {
        // Case 2, nighttime.
        let delf = (xfh - FLOW) / 11.0;
        frea[0] = FLOW;
        for i in 1..11 {
            frea[i] = frea[i - 1] + delf;
        }
        frea[12] = 0.0;
        return frea;
    }
    if xfh <= FLOW + 20.0 {
        // Case 3: insert the smaller MUF within equal increments.
        let delf = (xfh - FLOW) / 9.0;
        let mut ne = ((xfl - FLOW) / delf) as i32 + 2;
        frea[0] = FLOW;
        for i in 1..ne as usize {
            frea[i] = frea[i - 1] + delf;
        }
        frea[ne as usize - 1] = xfl;
        frea[ne as usize] = frea[ne as usize - 2] + delf;
        ne += 2;
        let ne = (ne as usize).min(11);
        for i in (ne - 1)..11 {
            frea[i] = frea[i - 1] + delf;
        }
        frea[12] = 0.0;
        return frea;
    }
    if xfl <= FLOW + 2.0 {
        // Case 4: the lower MUF between 2 and 4 MHz.
        let delf = (xfh - (FLOW + 2.0)) / 9.0;
        frea[0] = FLOW;
        frea[1] = xfl;
        frea[2] = FLOW + 2.0;
        for i in 3..11 {
            frea[i] = frea[i - 1] + delf;
        }
        frea[12] = 0.0;
        return frea;
    }
    if FHIGH - 2.0 - FLOW <= 0.0 {
        // Case 6, unreachable with the fixed 2-40 MHz limits.
        let delf = ((FHIGH - 2.0) - FLOW) / 9.0;
        frea[0] = FLOW;
        for i in 1..10 {
            frea[i] = frea[i - 1] + delf;
        }
        frea[10] = xfl;
        frea[12] = 0.0;
        return frea;
    }
    // Case 5: equal increments to the lower MUF, then to the limit.
    let mut ne = (((xfl - FLOW) / 2.0) as i32).min(7);
    let xne = ne as R;
    let delf = (xfl - FLOW) / xne;
    frea[0] = FLOW;
    for i in 1..ne as usize {
        frea[i] = frea[i - 1] + delf;
    }
    let xne = ne as R;
    let delf = (xfh - frea[ne as usize - 1]) / (12.0 - xne - 1.0);
    ne += 1;
    for i in (ne as usize - 1)..11 {
        frea[i] = frea[i - 1] + delf;
    }
    frea[12] = 0.0;
    frea
}

/// `LUFFY` for `IPFG` 300 and 400: the LUF search. Sweeps the
/// frequency complement, computes the reliability of each frequency
/// with the same short or long chain as the systems passes, and stops
/// at the first that meets the required reliability, interpolating the
/// LUF between it and the one before. Returns the LUF (negative when
/// no frequency qualified) and the complement with slot 13 filled.
///
/// Differences from the systems passes, as the source has them: a
/// short-path frequency whose reflectrix has no reachable distance is
/// skipped rather than forced into a single over-the-MUF mode, and no
/// service probability, multipath or output happens.
pub fn luffy_luf(
    lp: &mut ModeLoopState,
    ctx: &PassCtx,
    muf: &mut MufHour,
    noise_for: &dyn Fn(R) -> NoiseResult,
    lufp: i32,
) -> (R, [R; 13]) {
    let mut frea = frqcom(muf, 0);
    let pluf = 0.01 * lufp as R;
    let mut ihmin = muf.layers[0].nhopmf;
    if muf.layers[1].nhopmf > 0 && muf.layers[1].nhopmf < ihmin {
        ihmin = muf.layers[1].nhopmf;
    }
    if muf.layers[2].nhopmf < ihmin {
        ihmin = muf.layers[2].nhopmf;
    }
    lp.son[12].nhp = -1;
    lp.son[12].mdl = if ctx.long { b'L' } else { b'S' };
    for son in lp.son.iter_mut().take(12) {
        son.mdl = b' ';
    }
    let itxrcp = [1usize, if ctx.state.kfx == 1 { 2 } else { ctx.state.kfx }];
    for ifx in 0..12usize {
        let freq = frea[ifx];
        lp.son[ifx].reliab = 0.0;
        lp.all.reset();
        let noise = noise_for(freq);
        if !ctx.long {
            let jm = ctx.jmode;
            let kc = ctx.kctl;
            findf(
                &mut lp.reflectrix[kc],
                ctx.state,
                &lp.areas[kc],
                kc,
                freq,
                ctx.deck.amind,
                ctx.nang,
            );
            if lp.reflectrix[kc].dmaxkm <= 0.0 {
                // Nothing reflects at this frequency: skip it.
                continue;
            }
            let fsdead = ((lp.areas[jm].ifob[0][2] as R) / 1000.0).min(3.0);
            let mut ihsrt = (ctx.gcdkm / lp.reflectrix[kc].dmaxkm + 1.0) as i32;
            let mut ihmax = (ctx.gcdkm / lp.reflectrix[kc].dskpkm) as i32;
            if ihsrt < ihmin {
                ihsrt = ihmin;
            }
            if ihmax < ihsrt {
                ihmax = ihsrt;
            }
            let ihstp = ihmax.min(ihsrt + 2);
            if ihsrt > ihmin {
                ihsrt = ihmin.max(ihstp - 2);
            }
            for ihop in ihsrt..=ihstp {
                let hop = ihop as R;
                lp.ghop = ctx.gcd / hop;
                fdist(
                    &mut lp.modes[kc],
                    &lp.reflectrix[kc],
                    lp.ghop,
                    ctx.deck.amind,
                    freq,
                );
                let (modes, ghop) = (&mut lp.modes[jm], &mut lp.ghop);
                inmuf(
                    &mut lp.zon,
                    modes,
                    ghop,
                    muf,
                    ctx,
                    &lp.areas[jm],
                    &noise,
                    freq,
                    ihop,
                    fsdead,
                );
                lp.all.accumulate(&lp.zon, 1, 6);
            }
            esmod(&mut lp.zon, ctx, muf, &noise, freq, fsdead);
            esreg(&mut lp.zon);
            lp.all.accumulate(&lp.zon, 4, 5);
        } else {
            for &end in &itxrcp {
                let k = end - 1;
                findf(
                    &mut lp.reflectrix[k],
                    ctx.state,
                    &lp.areas[k],
                    k,
                    freq,
                    ctx.deck.amind,
                    ctx.nang,
                );
            }
            gmloss(lp, ctx, muf, freq, itxrcp);
            let ltxrgm = seltxr(lp, ctx, itxrcp);
            lngpat(lp, ctx, muf, &noise, freq, itxrcp, ltxrgm);
        }
        relbil(lp, ifx, &noise, &ctx.deck, ctx.ants, freq);
        if lp.son[ifx].reliab >= pluf {
            let xluf = if ifx == 0 {
                freq
            } else {
                let flow = frea[ifx - 1];
                let fhigh = frea[ifx];
                let rlow = lp.son[ifx - 1].reliab;
                let rhigh = lp.son[ifx].reliab;
                flow + (fhigh - flow) * (pluf - rlow) / (rhigh - rlow)
            };
            frea[12] = xluf;
            return (xluf, frea);
        }
    }
    // No LUF found: take the highest reliability. The comparison is
    // against the *first* slot's reliability throughout — REL is never
    // updated — so IG lands on the last slot beating slot 1, not on
    // the true maximum. Kept as written.
    let mut ig = 0usize;
    let rel = lp.son[0].reliab;
    for i in 1..12 {
        if lp.son[i].reliab > rel {
            ig = i;
        }
    }
    let xluf = -frea[ig];
    frea[12] = frea[ig];
    (xluf, frea)
}

/// One frequency's final values after the smoothing blend.
#[derive(Debug, Clone)]
pub struct SmoothDebug {
    pub khz: i32,
    pub son: Son,
}

/// The 7000-10000 km long/short smoothing from the end of `LUFFY`
/// (the VOA memo of 15 Jan 1991), run after both passes. `ctx` is the
/// long-path pass's context.
pub fn luffy_smooth(
    lp: &mut ModeLoopState,
    ctx: &PassCtx,
    noise_for: &dyn Fn(R) -> NoiseResult,
    frel: &[R; 12],
    saves: &HourSaves,
) -> Vec<Option<SmoothDebug>> {
    lp.son[12].mdl = b'M';
    let mut out: Vec<Option<SmoothDebug>> = Vec::with_capacity(12);
    for (ifx, &freq) in frel.iter().enumerate() {
        lp.son[ifx].mdl = b' ';
        if freq <= 0.0 {
            out.push(None);
            continue;
        }
        let khz = nint_khz(freq);
        let slpld = saves.son[ifx][1].dbw - saves.son[ifx][1].dblosl.abs();
        let sspld = saves.son[ifx][0].dbw - saves.son[ifx][0].dblosl.abs();
        let delpow = slpld - sspld;
        let idx: usize;
        if delpow < 0.0 {
            // Use the short path.
            idx = 0;
            lp.son[ifx].mdl = b'S';
            lp.son[ifx].moder_layer = saves.son[ifx][0].mode_layer;
            lp.son[ifx].angler = saves.son[ifx][0].angle;
            // The source stores the gains into the accumulated-mode
            // arrays indexed by frequency; kept for fidelity.
            lp.all.tgain[ifx] = saves.ytgain[ifx][0];
            lp.all.rgain[ifx] = saves.yrgain[ifx][0];
            lp.son[ifx].gaint = lp.all.tgain[ifx];
            lp.son[ifx].gainr = lp.all.rgain[ifx];
            lp.son[ifx].snxx = ctx.deck.rsn - saves.son[ifx][0].snpr;
        } else {
            // Use the long path, or smoothing below 10000 km.
            idx = 1;
            lp.son[ifx].mdl = b'L';
            lp.son[ifx].moder_layer = saves.zmoder[ifx];
            lp.son[ifx].angler = saves.zangler[ifx];
            lp.all.b[0] = saves.son[ifx][1].angle;
            lp.all.tllow[0] = saves.son[ifx][1].dblosl;
            lp.all.tlhgh[0] = saves.son[ifx][1].dblosu;
            lp.all.timed[0] = saves.son[ifx][1].delay;
            lp.all.hn[0] = saves.son[ifx][1].nhp as R;
            lp.all.hp[0] = saves.son[ifx][1].vhigh;
            lp.all.prob[0] = saves.son[ifx][1].cprob;
            lp.all.nmode[0] = saves.inmode[ifx];
            lp.all.tgain[0] = saves.ytgain[ifx][1];
            lp.all.rgain[0] = saves.yrgain[ifx][1];
            lp.all.eff[0] = lp.son[ifx].rneff;
        }
        let sv = saves.son[ifx][idx];
        lp.son[ifx].mode_layer = sv.mode_layer;
        lp.son[ifx].angle = sv.angle;
        lp.son[ifx].cprob = sv.cprob;
        lp.son[ifx].dblos = sv.dblos;
        lp.son[ifx].dblosl = sv.dblosl;
        lp.son[ifx].dblosu = sv.dblosu;
        lp.son[ifx].delay = sv.delay;
        lp.son[ifx].dbw = sv.dbw;
        lp.son[ifx].nhp = sv.nhp;
        // RCNSE = XNYNOIS(IF): the current slot value, not the save.
        let rcnse = lp.son[ifx].xnynois;
        lp.son[ifx].vhigh = sv.vhigh;
        lp.son[ifx].dbu = sv.dbu;
        lp.son[ifx].reliab = sv.reliab;
        lp.son[ifx].sndb = sv.sndb;
        lp.son[ifx].snpr = sv.snpr;
        lp.son[ifx].snrlw = sv.snrlw;
        lp.son[ifx].snrup = sv.snrup;
        lp.son[ifx].sprob = sv.sprob;
        lp.son[ifx].probmp = sv.probmp;
        if delpow >= 0.0 && ctx.gcdkm < 10000.0 {
            // The smoothing blend proper.
            lp.son[ifx].mdl = b'M';
            let disint = (ctx.gcdkm - 7000.0) / 3000.0;
            let delx = if delpow > 375.0 {
                // Avoid overflow.
                (10.0 as R).powf(37.5)
            } else {
                (10.0 as R).powf(delpow / 10.0)
            };
            let smooth = sspld + 10.0 * (disint * (delx - 1.0) + 1.0).log10();
            lp.all.sigpow[0] = smooth + lp.all.tllow[0];
            lp.all.tloss[0] = ctx.ants.pwrdb(freq) - lp.all.sigpow[0];
            lp.all.sn[0] = lp.all.sigpow[0] - rcnse - lp.all.eff[0];
            lp.all.fldst[0] = 107.2 + ctx.ants.pwrdb(freq) + 20.0 * freq.log10()
                - lp.all.tloss[0]
                - lp.all.rgain[0];
            lp.all.nmmod = 1;
            let noise = noise_for(freq);
            relbil(lp, ifx, &noise, &ctx.deck, ctx.ants, freq);
            serprb(lp, ifx, &noise, ctx.sig, &ctx.deck);
        }
        out.push(Some(SmoothDebug {
            khz,
            son: lp.son[ifx],
        }));
    }
    out
}

/// The slot-12 rewrite from `OUTBOD`: when the circuit MUF is above
/// 30 MHz the printer overwrites the MUF frequency slot with "no
/// antenna support" sentinels (`MODE`/`MODER` become "NA", kept here as
/// layer code 6). It runs after every hour's output, so the sentinels
/// are what the next hour's stale reads see.
pub fn outbod_sentinels(son: &mut [Son; 13], allmuf: R) {
    if allmuf > 30.0 {
        son[11].nhp = 0;
        son[11].mode_layer = 6;
        son[11].moder_layer = 6;
        son[11].angle = 99.9;
        son[11].delay = 99.9;
        son[11].vhigh = 1000.0;
        son[11].dblos = 1000.0;
        son[11].dbu = -999.0;
        son[11].dbw = -999.0;
        son[11].sndb = -999.0;
        son[11].snpr = -999.0;
        son[11].reliab = 0.0;
        son[11].probmp = 0.0;
        son[11].sprob = 0.0;
        son[11].snxx = -999.0;
    }
}

/// Port of `SETLUF`: the hour's LUF from the frequency complement and
/// computed reliabilities. Returns `XLUF(IT)` (negative when no
/// frequency reaches the required reliability, carrying the most
/// reliable one).
pub fn setluf(son: &[Son; 13], frel: &[R; 12], lufp: i32) -> R {
    let mut rf = frel[0];
    let mut rel = son[0].reliab;
    let pluf = 0.01 * lufp as R;
    for ifx in 0..11 {
        if frel[ifx] <= 0.0 {
            continue;
        }
        if son[ifx].reliab >= pluf {
            return frel[ifx];
        }
        if son[ifx].reliab > rel {
            rel = son[ifx].reliab;
            rf = frel[ifx];
        }
    }
    -rf
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fnorml_matches_the_source_symmetry() {
        // FNORML(0) = 0.5, and P(-x) = 1 - P(x).
        assert!((fnorml(0.0) - 0.5).abs() < 1e-6);
        let p = fnorml(1.2815);
        assert!((p - 0.9).abs() < 0.001, "got {p}");
        assert!((fnorml(-1.2815) - (1.0 - p)).abs() < 1e-6);
    }

    #[test]
    fn allmodes_reset_clears_only_the_four_arrays() {
        let mut all = AllModes {
            nmmod: 3,
            ..Default::default()
        };
        all.sn[0] = 42.0;
        all.tloss[0] = 1.0;
        all.reset();
        assert_eq!(all.nmmod, 0);
        assert_eq!(all.tloss[0], 99999.0);
        assert_eq!(all.hp[0], -1.0);
        // SN keeps its old value, as the COMMON does.
        assert_eq!(all.sn[0], 42.0);
    }

    #[test]
    fn allmodes_accumulates_only_slots_with_modes() {
        let mut all = AllModes::default();
        all.reset();
        let mut zon = Zon::default();
        zon.hp[0] = 300.0;
        zon.sn[0] = 10.0;
        zon.hp[2] = 250.0;
        zon.sn[2] = 5.0;
        all.accumulate(&zon, 1, 6);
        assert_eq!(all.nmmod, 2);
        assert_eq!(all.sn[0], 10.0);
        assert_eq!(all.sn[1], 5.0);
    }

    #[test]
    fn gain_ground_returns_zero_without_conductivity_and_six_at_grazing() {
        assert_eq!(gain_ground(0.1, 10.0, 0.0, 4.0), 0.0);
        // At zero elevation the loss is pinned to 6 dB.
        assert_eq!(gain_ground(0.0, 10.0, 5.0, 80.0), 6.0);
        // Sea water at a moderate angle loses little.
        let sea = gain_ground(0.3, 10.0, 5.0, 80.0);
        assert!(sea > 0.0 && sea < 1.0, "got {sea}");
    }

    #[test]
    fn setluf_picks_the_first_reliable_frequency_or_flags_the_best() {
        let mut son = [Son::default(); 13];
        let mut frel = [0.0 as R; 12];
        frel[0] = 7.0;
        frel[1] = 10.0;
        son[0].reliab = 0.5;
        son[1].reliab = 0.95;
        assert_eq!(setluf(&son, &frel, 90), 10.0);
        son[1].reliab = 0.7;
        // Nothing reaches 90 per cent: negative, carrying the best.
        assert_eq!(setluf(&son, &frel, 90), -10.0);
    }

    #[test]
    fn setlng_replicates_one_area_into_all_slots() {
        let mut state = IonoState::from_layers(&[]);
        state.km = 1;
        state.kfx = 1;
        state.fi[0] = [3.0, 0.0, 8.0];
        let mut fs = [[0.0 as R; 3]; 5];
        fs[0] = [2.0, 3.0, 4.0];
        let mut hs = [0.0 as R; 5];
        hs[0] = 110.0;
        let mut geog = Geog::default();
        geog.gyz[0] = 1.2;
        let mut areas: [AreaTables; 3] = Default::default();
        areas[0].ion.fvert[0] = 1.5;
        setlng(&mut state, &mut fs, &mut hs, &mut geog, &mut areas);
        assert_eq!(state.fi[4], [3.0, 0.0, 8.0]);
        assert_eq!(fs[3], [2.0, 3.0, 4.0]);
        assert_eq!(hs[2], 110.0);
        assert_eq!(geog.gyz[1], 1.2);
        assert_eq!(areas[2].ion.fvert[0], 1.5);
    }
}
