//! Layer MUFs and the circuit MUF for one hour.
//!
//! Port of `ionset.for` (slot assignment), `lecden.for` (electron density
//! profile), `gethp.for` (true and virtual heights by 20-point Gaussian
//! integration), `f2dis.for` (F2 MUF deciles) and `curmuf.for` (layer and
//! circuit MUFs with the Martyn's-theorem correction).
//!
//! Two configuration facts fix which paths run and are the only ones
//! ported: the installed `version.w32` ends in `W`, so `CURMUF_orig`
//! runs (not Alex Shovkoplyas's variant), and the app's decks carry no
//! INTEGRATE card, so `IEDP = -1` (from `blkdat.for`) and heights always
//! come from the density profile via `GETHP` — the parabolic shortcuts
//! (`BENDY`, `PEN`) are dead code here and are not ported.
//!
//! Mutations the Fortran hides in COMMON blocks are explicit here:
//! `ionset` compresses the five sample areas into up to three ionosphere
//! slots, `lecden` reshapes the F1 layer of the controlling slot, and
//! `curmuf` rewrites the sporadic-E lower decile of the controlling area.

// Constants and Gauss nodes are kept digit for digit with the Fortran.
#![allow(clippy::excessive_precision)]

use super::con::{D2R, PIO2, R, R2D, RZ};
use super::ionosphere::{EsParams, LayerParams};

/// 20-point Gauss abscissas `XT` from `blkdat.for`.
const XT: [R; 20] = [
    0.0387724175,
    0.1160840707,
    0.1926975807,
    0.2681521850,
    0.3419940908,
    0.4137792043,
    0.4830758017,
    0.5494671251,
    0.6125538897,
    0.6719566846,
    0.7273182552,
    0.7783056514,
    0.8246122308,
    0.8659595032,
    0.9020988070,
    0.9328128083,
    0.9579168192,
    0.9772599500,
    0.9907262387,
    0.9982377097,
];
/// The matching Gauss weights `WT`.
const WT: [R; 20] = [
    0.0775059480,
    0.0770398182,
    0.0761103619,
    0.0747231691,
    0.0728865824,
    0.0706116474,
    0.0679120458,
    0.0648040135,
    0.0613062425,
    0.0574397691,
    0.0532278470,
    0.0486958076,
    0.0438709082,
    0.0387821680,
    0.0334601953,
    0.0279370070,
    0.0222458492,
    0.0164210584,
    0.0104982845,
    0.0045212771,
];
/// `voacapw.for` integration setup: NT=20 points, linear transformation.
const NPL: i32 = 1;
const XNPL: R = 1.0;
const TWDIV: R = 0.5;

/// `curmuf.for` DATA.
const FQDEL: R = 0.1;
const NFQ: i32 = 4;
const AE: R = 0.5;
const A1: R = 0.5;
const A2: R = 0.5;
const DZF: R = 2000.0;
const BEX: R = 9.5;

/// The layer state the Fortran keeps in `/RON/`: `FI`, `YI`, `HI` slots
/// plus the density profile. `fi[k][i]` is Fortran `FI(i+1, k+1)`.
#[derive(Debug, Clone)]
pub struct IonoState {
    pub fi: [[R; 3]; 5],
    pub yi: [[R; 3]; 5],
    pub hi: [[R; 3]; 5],
    /// Number of sample areas (control points).
    pub km: usize,
    /// Number of ionosphere slots after `ionset`.
    pub kfx: usize,
    /// True heights `HTR(·,1)`, km.
    pub htr: [R; 50],
    /// Plasma frequency squared `FNSQ(·,1)`, MHz².
    pub fnsq: [R; 50],
    /// `FSECV(k)` per slot — only written on some `lecden` paths. The
    /// Fortran keeps it in a COMMON block, so an hour whose `lecden`
    /// takes the no-F1 path sees the previous hour's value: a caller
    /// stepping through hours must copy this field from the previous
    /// hour's state into the next (`from_layers` starts it at the
    /// program-start value, zero).
    pub fsecv: [R; 3],
    /// `IEDP` of `/MFAC/`: how the layer heights are obtained. Below
    /// zero — the program-start value, and what a deck without an
    /// `INTEGRATE` card leaves — every height comes from `gethp` on the
    /// density profile. At zero or above the E layer takes fixed
    /// heights and a profile with no F1 layer takes parabolic segments.
    ///
    /// An `INTEGRATE` card always sets it to at least 1, even in its
    /// `OFF` form: `DECRED` assigns 1 before it tests for `OFF` and
    /// never restores the program-start value.
    pub iedp: i32,
}

impl IonoState {
    /// Loads the per-point layer parameters into the `/RON/` arrays.
    pub fn from_layers(params: &[LayerParams]) -> Self {
        let mut state = Self {
            fi: [[0.0; 3]; 5],
            yi: [[0.0; 3]; 5],
            hi: [[0.0; 3]; 5],
            km: params.len(),
            kfx: 0,
            htr: [0.0; 50],
            fnsq: [0.0; 50],
            fsecv: [0.0; 3],
            iedp: -1,
        };
        for (k, p) in params.iter().enumerate() {
            state.fi[k] = p.fi;
            state.yi[k] = p.yi;
            state.hi[k] = p.hi;
        }
        state
    }
}

/// Port of `IONSET`: assigns layers to ionosphere slots — the E layer
/// from the closer sample area, the F layer from the farther — then
/// enforces layer consistency per slot.
pub fn ionset(s: &mut IonoState) {
    if s.km <= 1 {
        s.kfx = 1;
    } else if s.km <= 3 {
        s.kfx = 2;
        // F2 of slot 1 from the midpoint; E and F1 of slot 2 from point 3.
        s.fi[0][2] = s.fi[1][2];
        s.yi[0][2] = s.yi[1][2];
        s.hi[0][2] = s.hi[1][2];
        s.fi[1][0] = s.fi[2][0];
        s.yi[1][0] = s.yi[2][0];
        s.hi[1][0] = s.hi[2][0];
        s.fi[1][1] = s.fi[2][1];
        s.yi[1][1] = s.yi[2][1];
        s.hi[1][1] = s.hi[2][1];
    } else {
        s.kfx = 3;
        // Five points: F2 slots from points 2, 3, 4; E and F1 slots from
        // points 1, 3, 5.
        s.fi[0][2] = s.fi[1][2];
        s.yi[0][2] = s.yi[1][2];
        s.hi[0][2] = s.hi[1][2];
        for i in 0..3 {
            s.fi[1][i] = s.fi[2][i];
            s.yi[1][i] = s.yi[2][i];
            s.hi[1][i] = s.hi[2][i];
        }
        s.fi[2][0] = s.fi[4][0];
        s.yi[2][0] = s.yi[4][0];
        s.hi[2][0] = s.hi[4][0];
        s.fi[2][1] = s.fi[4][1];
        s.yi[2][1] = s.yi[4][1];
        s.hi[2][1] = s.hi[4][1];
        s.fi[2][2] = s.fi[3][2];
        s.yi[2][2] = s.yi[3][2];
        s.hi[2][2] = s.hi[3][2];
    }
    for k in 0..s.kfx {
        if s.fi[k][1] > 0.0 {
            if s.fi[k][1] - s.fi[k][0] - 0.2 <= 0.0
                || s.fi[k][2] - s.fi[k][1] - 0.2 <= 0.0
            {
                // E-F1 or F1-F2 criticals too close: drop the F1 layer.
                s.fi[k][1] = 0.0;
                s.yi[k][1] = 0.0;
                s.hi[k][1] = 0.0;
            } else {
                s.hi[k][1] = s.hi[k][1].min(s.hi[k][2]);
            }
        }
        let hdif = s.hi[k][2] - s.hi[k][0] - 2.0;
        s.yi[k][2] = s.yi[k][2].min(hdif);
    }
}

/// Port of `LECDEN`: builds the 50-point electron density profile for
/// slot `k` (0-based) into `htr`/`fnsq`, reshaping the slot's F1 layer
/// where the source does.
pub fn lecden(s: &mut IonoState, k: usize) {
    let xlow: R = 0.8516;
    let hz = s.hi[k][0] - s.yi[k][0];
    let xup = 0.98 * s.fi[k][0] / s.fi[k][2];
    let hlow = hz + s.yi[k][0] * (1.0 + (1.0 - xlow * xlow).sqrt());
    let hte = s.hi[k][0] + s.yi[k][0];
    let mut hb2 = s.hi[k][2] - s.yi[k][2];
    let fce = s.fi[k][0] * s.fi[k][0];
    let fc2 = s.fi[k][2] * s.fi[k][2];
    // The E-F valley is filled linearly from (FLOW,HLOW) to (FUP,HUP).
    let hup = hb2 + s.yi[k][2] * (1.0 - (1.0 - xup * xup).sqrt());
    let fup = xup * xup * fc2;
    let flow = xlow * xlow * fce;
    let mut asp: R = 0.0;
    if hup - hlow > 0.0 {
        asp = (fup - flow) / (hup - hlow);
    }
    let mut lin = false;
    let mut fc1: R = 0.0;
    let mut ht1: R = 0.0;
    let mut s1: R = 0.0;
    if s.fi[k][1] > 0.0 {
        fc1 = s.fi[k][1] * s.fi[k][1];
        let hb1 = s.hi[k][1] - s.yi[k][1];
        ht1 = s.hi[k][1] + s.yi[k][1];
        // Height of F2 at the F1 critical frequency.
        let htw = hb2 + s.yi[k][2] * (1.0 - (1.0 - fc1 / fc2).sqrt());
        if htw > s.hi[k][1] + 0.001 {
            lin = false;
            // Force F1 above the E layer.
            s.yi[k][1] = s.yi[k][1].min(s.hi[k][1] - s.hi[k][0] + 1.0);
            s.fsecv[k] = -1.0;
        } else {
            // Force F1 to the F2 height at its critical frequency.
            let mut ys = (htw - hb1).max(1.0);
            s1 = fc1 / ys;
            s.hi[k][1] = htw;
            s.yi[k][1] = ys;
            if hb2 - hb1 < 0.0 {
                // Avoid a spurious layer.
                s.yi[k][1] = s.hi[k][1] - hb2;
                lin = false;
                s.yi[k][1] = s.yi[k][1].min(s.hi[k][1] - s.hi[k][0] + 1.0);
                s.fsecv[k] = -1.0;
            } else {
                lin = true;
                s.fsecv[k] = flow.sqrt();
                let mut denom = 1.0 - (s.fi[k][0] / s.fi[k][1]).powi(2);
                // The F1 line must not obscure the E layer.
                denom = denom.max(0.17);
                let yb = (s.hi[k][1] - s.hi[k][0]) / denom;
                if ys - yb >= 0.0 {
                    // F1 passes through the E nose (CCIR 1976 shape).
                    // (The source also sets HB1 here; that store is dead —
                    // HB1 is recomputed before the density loop reads it.)
                    ys = yb;
                    s.yi[k][1] = ys;
                    s1 = fc1 / ys;
                }
                ht1 = htw;
            }
        }
    }
    // D-region tail below the E layer.
    let hd: R = 70.0;
    let xtail: R = 0.85;
    let hex = s.hi[k][0] - xtail * s.yi[k][0];
    let fnx = 1.0 - xtail * xtail;
    let alp = 2.0 * (s.hi[k][0] - hex) / (fnx * s.yi[k][0] * s.yi[k][0]);
    let fsq = fnx * (-alp * (hex - hd)).exp();
    let htr = &mut s.htr;
    htr[0] = hd;
    htr[4] = hex;
    let mut hdif = ((htr[4] - htr[0]) * 0.25).max(0.0);
    htr[3] = htr[4] - hdif.min(1.0);
    htr[1] = htr[0] + hdif;
    htr[2] = (htr[1] + htr[3]) * 0.5;
    // E below the nose.
    htr[10] = s.hi[k][0];
    hdif = (htr[10] - htr[4]) / 6.0;
    for ih in 5..10 {
        htr[ih] = htr[ih - 1] + hdif;
    }
    // E above the nose.
    htr[16] = s.hi[k][0] + s.yi[k][0];
    hdif = (htr[16] - htr[10]) / 6.0;
    for ih in 11..16 {
        htr[ih] = htr[ih - 1] + hdif;
    }
    htr[10] = 0.5 * (htr[9] + htr[11]);
    htr[49] = s.hi[k][2];
    let mut f2_only = s.fi[k][1] <= 0.0;
    if !f2_only {
        hb2 = s.hi[k][2] - s.yi[k][2];
        let hb1n = s.hi[k][1] - s.yi[k][1] + 0.00001;
        if hb2 - hb1n < 0.0 {
            f2_only = true;
        }
    }
    if f2_only {
        // F2 layer, no F1 layer.
        htr[17] = s.hi[k][2] - s.yi[k][2];
        hdif = (htr[49] - htr[17]) / 32.0;
        for ih in 18..49 {
            htr[ih] = htr[ih - 1] + hdif;
        }
    } else {
        // F1 and F2 layers.
        htr[17] = (s.hi[k][1] - s.yi[k][1]).max(htr[16] + 1.0);
        htr[27] = s.hi[k][1];
        hdif = (htr[27] - htr[17]) / 10.0;
        for ih in 18..27 {
            htr[ih] = htr[ih - 1] + hdif;
        }
        hdif = (htr[49] - htr[27]) / 22.0;
        for ih in 28..49 {
            htr[ih] = htr[ih - 1] + hdif;
        }
    }
    // Density at each profile height: the maximum over the layer shapes.
    let hb1 = s.hi[k][0].max(s.hi[k][1] - s.yi[k][1]);
    let hb2v = s.hi[k][2] - s.yi[k][2];
    for ih in 0..50 {
        let h = s.htr[ih];
        let mut fnd: R = 0.0;
        let mut fne: R = 0.0;
        let mut fn1: R = 0.0;
        let mut fn2: R = 0.0;
        let mut fnval: R = 0.0;
        if h - hlow > 0.0 && h - hup < 0.0 {
            // Linear valley.
            fnval = fup + asp * (h - hup);
        }
        if h - hex >= 0.0 {
            if h - hte <= 0.0 {
                // Parabolic E.
                let z = (h - s.hi[k][0]) / s.yi[k][0];
                fne = fce * (1.0 - z * z);
                if h - hex < 0.0 {
                    fnd = fce * fsq * (alp * (h - hd)).exp();
                }
            }
        } else {
            // Exponential D-E tail.
            fnd = fce * fsq * (alp * (h - hd)).exp();
        }
        if s.fi[k][1] > 0.0 && hb1 - h <= 0.0 && h - ht1 <= 0.0 {
            if lin {
                // Linear F1.
                fn1 = s1 * (h - (s.hi[k][1] - s.yi[k][1]));
            } else {
                // Parabolic F1.
                let z = (h - s.hi[k][1]) / s.yi[k][1];
                fn1 = fc1 * (1.0 - z * z);
            }
        }
        if hb2v - h <= 0.0 {
            // Parabolic F2.
            let z = (h - s.hi[k][2]) / s.yi[k][2];
            fn2 = fc2 * (1.0 - z * z);
        }
        s.fnsq[ih] = fnd.max(fne).max(fnval).max(fn1).max(fn2);
    }
}

/// The Fortran's shared interpolation shape: given parallel arrays and a
/// probe on the first, interpolate on the second, handling flat and
/// decreasing segments exactly as the source's arithmetic IFs do.
fn profile_interpolate(from: &[R; 50], to: &[R; 50], probe: R) -> R {
    for ih in 0..49 {
        let d = from[ih] - from[ih + 1];
        if d < 0.0 {
            if from[ih] - probe <= 0.0 {
                if probe - from[ih + 1] < 0.0 {
                    return to[ih] + (probe - from[ih]) * (to[ih + 1] - to[ih]) / (from[ih + 1] - from[ih]);
                }
            } else {
                return to[ih] + (probe - from[ih]) * (to[ih + 1] - to[ih]) / (from[ih + 1] - from[ih]);
            }
        } else if d == 0.0 {
            if probe - from[ih] == 0.0 {
                return to[ih];
            }
        } else if from[ih] - probe >= 0.0 && probe - from[ih + 1] > 0.0 {
            return to[ih] + (probe - from[ih]) * (to[ih + 1] - to[ih]) / (from[ih + 1] - from[ih]);
        }
    }
    to[49]
}

/// Port of `GETHP`: the true and virtual heights `(hpx, htx)` for a
/// vertical frequency, from the density profile by Gaussian integration.
/// `BENDY`: the bending a parabolic layer adds to the virtual height.
pub fn bendy(s: &IonoState, i: usize, k: usize, f: R) -> R {
    let x = (f / s.fi[k][i]).min(0.999);
    0.5 * x * s.yi[k][i] * ((1.0 + x) / (1.0 - x)).ln()
}

/// `PEN`: the retardation a parabolic layer below the reflection adds.
pub fn pen(s: &IonoState, i: usize, k: usize, f: R) -> R {
    let x = (f / s.fi[k][i]).max(1.001);
    s.yi[k][i] * ((1.0 + x) / (x - 1.0)).ln() * x
}

pub fn gethp(s: &IonoState, fxx: R) -> (R, R) {
    let fr = fxx * fxx;
    if fr - s.fnsq[0] <= 0.0 {
        return (s.htr[0], s.htr[0]);
    }
    let mut ht = profile_interpolate(&s.fnsq, &s.htr, fr);
    let mut hp: R = 0.0;
    ht -= s.htr[0];
    let hrmz = ht * TWDIV * XNPL;
    for ig in 0..20 {
        let mut mup = [0.0 as R; 2];
        for (ib, m) in mup.iter_mut().enumerate() {
            let zg = if ib == 0 { 1.0 - XT[ig] } else { 1.0 + XT[ig] };
            let zi = ht * (1.0 - TWDIV * zg.powi(NPL)) + s.htr[0];
            let ysq = if zi - s.htr[0] <= 0.0 {
                s.fnsq[0]
            } else {
                profile_interpolate(&s.htr, &s.fnsq, zi)
            };
            let ysq = (ysq / fr).min(0.9999);
            *m = (1.0 / (1.0 - ysq).sqrt()) * zg.powi(NPL - 1);
        }
        hp += WT[ig] * (mup[0] + mup[1]);
    }
    (s.htr[0] + hrmz * hp, s.htr[0] + ht)
}

/// Port of `F2DIS`: the standard deviation of the F2 MUF from the
/// `F2D(16,6,6)` tables. `clat` in radians, `abb` is local mean time.
pub fn f2dis(f2d: &[[[R; 16]; 6]; 6], fmuf: R, ssn: R, clat: R, freq: R, abb: R) -> R {
    if f2d[0][0][0] <= 0.0 {
        // No ionospheric data: classical value.
        let sig = 0.15 * fmuf / 1.28155;
        return sig.max(0.001);
    }
    let cl = clat * 57.29577;
    let mut icc = (abb / 4.0 + 1.55) as i32;
    if icc > 6 {
        icc = 1;
    }
    let i = if freq <= fmuf { 8 } else { 0 };
    let icl = ((9.5 - cl.abs() / 10.0) as i32).clamp(1, 8);
    let iz = icl + i;
    let mut j = if ssn <= 50.0 {
        1
    } else if ssn > 100.0 {
        3
    } else {
        2
    };
    if cl <= 0.0 {
        j += 3;
    }
    let sig = (fmuf - f2d[icc as usize - 1][j - 1][iz as usize - 1] * fmuf).abs() / 1.28;
    sig.max(0.001)
}

/// Per-layer MUF outputs (`/MUFS/` slots 1-4: E, F1, F2, Es).
#[derive(Debug, Clone, Copy, Default)]
pub struct LayerMuf {
    pub sigl: R,
    pub sigu: R,
    /// Takeoff angle, degrees.
    pub delmuf: R,
    /// Virtual height, km.
    pub hpmuf: R,
    /// True height, km.
    pub htmuf: R,
    /// Equivalent vertical frequency, MHz.
    pub fvmuf: R,
    /// Deviative loss factor.
    pub afmuf: R,
    /// Number of hops.
    pub nhopmf: i32,
    /// Lower decile, median and upper decile of the MUF.
    pub yfot: R,
    pub ymuf: R,
    pub yhpf: R,
}

/// The hour's MUF outputs.
#[derive(Debug, Clone)]
pub struct MufHour {
    pub emuf: R,
    pub f1muf: R,
    pub f2muf: R,
    pub esmuf: R,
    /// Circuit MUF (Es not included).
    pub allmuf: R,
    pub fot: R,
    pub hpf: R,
    /// Takeoff angle at the circuit MUF, degrees.
    pub angmuf: R,
    /// Which layer set the circuit MUF: 1 E, 2 F1, 3 F2.
    pub modmuf: i32,
    /// E, F1, F2, Es layer details.
    pub layers: [LayerMuf; 4],
    /// The controlling sample area (0-based slot index `KS`).
    pub ks: usize,
}

/// One layer's hop geometry: the shared angle arithmetic of `CURMUF`.
/// Returns `(secp, sphe, del, xhops, psi, cpsi, spsi)` after the first
/// pass for a layer with virtual height `hp` and true height `ht`.
struct HopGeometry {
    secp: R,
    sphe: R,
    del: R,
    xhops: R,
    psi: R,
    cpsi: R,
    spsi: R,
}

fn hop_geometry(gcdkm: R, amind: R, hp: R, ht: R, split_first: bool) -> HopGeometry {
    let dele = amind.max(0.0);
    let del = dele * D2R;
    let phe = (RZ * del.cos() / (RZ + hp)).asin();
    let nhops = (0.5 * gcdkm / ((PIO2 - del - phe) * RZ)) as i32;
    let xhops = (nhops + 1) as R;
    // The E and F1 sections compute PSI in two statements, the F2 section
    // in one; the f32 roundings differ, so both spellings are kept.
    let psi = if split_first {
        let p = gcdkm / (2.0 * RZ);
        p / xhops
    } else {
        gcdkm / ((2.0 * RZ) * xhops)
    };
    let cpsi = psi.cos();
    let spsi = psi.sin();
    let tanp = spsi / (1.0 - cpsi + hp / RZ);
    let phe = tanp.atan();
    let del = PIO2 - phe - psi;
    let cdel = del.cos();
    let sphe = RZ * cdel / (RZ + ht);
    let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
    HopGeometry {
        secp,
        sphe,
        del,
        xhops,
        psi,
        cpsi,
        spsi,
    }
}

/// Port of `NOMMUF`: the layer MUFs and the circuit MUF by the manual
/// nomogram method — NBS Report 7619, programmed for the computer —
/// rather than from a complete electron-density profile. Card methods
/// 3 to 6 (`ITRUN = 3`) use it instead of [`curmuf`].
///
/// `fi` and `f2m3` are the per-control-point critical frequencies and
/// F2 M(3000) factors; `fs`/`hs` are the sporadic-E slots. There is no
/// separate F1 MUF on this path, and the returned per-layer detail
/// that [`curmuf`] fills stays at zero: `OUTLAY` is not reachable from
/// these methods.
pub fn nommuf(
    fi: &[[R; 3]; 5],
    f2m3: &[R; 5],
    fs: &[[R; 3]; 5],
    hs: &[R; 5],
    km: usize,
    gcd: R,
    gcdkm: R,
) -> MufHour {
    // The two distance factors, as polynomials in great-circle miles.
    const AE: [R; 7] = [
        -1.133_200_756E-2,
        3.761_385_053E-2,
        -5.038_476_266E-3,
        2.624_808_315E-4,
        -5.976_618_436E-6,
        1.334_494_261E-7,
        -4.368_460_907E-9,
    ];
    const AF: [R; 7] = [
        4.699_243_101E-3,
        2.264_634_341E-3,
        9.202_437_332E-5,
        6.865_259_817E-5,
        -9.985_831_104E-6,
        4.491_514_41E-7,
        -6.712_654_756E-9,
    ];
    let arc = gcdkm * 0.0062137;
    // E and F1 distance factor.
    let elfc = if arc < 16.0 {
        let mut v = AE[6];
        for c in AE[..6].iter().rev() {
            v = v * arc + c;
        }
        v * arc + 0.2085
    } else {
        1.02
    };
    // F2 distance factor.
    let flfc = if 24.0 <= arc {
        1.0
    } else {
        let mut v = AF[6];
        for c in AF[..6].iter().rev() {
            v = v * arc + c;
        }
        v * arc
    };
    let mut ec: R = 1000.0;
    for f in fi.iter().take(km) {
        if ec > f[0] {
            ec = f[0];
        }
    }
    let emuf = 4.871 * ec * elfc;
    let mut f2muf: R = 1000.0;
    for k in 0..km {
        let four = f2m3[k] * fi[k][2] * 1.1;
        let fmuf = fi[k][2] + flfc * (four - fi[k][2]);
        if fmuf < f2muf {
            f2muf = fmuf;
        }
    }
    let allmuf = if emuf >= f2muf { emuf } else { f2muf };
    let fot = (0.85 * f2muf).max(emuf);
    let hpf = (1.15 * f2muf).max(emuf);
    // Sporadic E, at a 0.5 probability of reflection.
    let mut esmuf: R = 1000.0;
    for k in 0..km {
        if fs[k][1] <= 0.0 {
            continue;
        }
        let dmax = 225.0 * hs[k].sqrt();
        let hop = gcdkm / dmax + 1.0;
        let psi = 0.5 * gcd / hop;
        let tdel = (psi.cos() - RZ / (RZ + hs[k])) / psi.sin();
        let cdel = 1.0 / (1.0 + tdel * tdel).sqrt();
        let sphe = RZ * cdel / (RZ + hs[k]);
        let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
        let esd = fs[k][1] * secp;
        if esmuf > esd {
            esmuf = esd;
        }
    }
    MufHour {
        emuf,
        // No separate F1 on this path.
        f1muf: -1.0,
        f2muf,
        esmuf,
        allmuf,
        fot,
        hpf,
        angmuf: 0.0,
        modmuf: 0,
        layers: [LayerMuf::default(); 4],
        ks: 0,
    }
}

/// Port of `CURMUF` (the `_orig` variant): layer MUFs, deciles and the
/// circuit MUF. `clat`/`clck` are per control point (radians, hours);
/// `es` is mutated where the source rewrites `FS(1,KSX)`.
#[allow(clippy::too_many_arguments)]
pub fn curmuf(
    s: &mut IonoState,
    es: &mut [EsParams],
    f2d: &[[[R; 16]; 6]; 6],
    clat: &[R],
    clck: &[R],
    gcd: R,
    gcdkm: R,
    amind: R,
    ssn: R,
) -> MufHour {
    // Select the controlling sample area KS from the transmitter-side
    // slot KT=1 and the receiver-side slot KR.
    let kt = 0usize;
    let kr = match s.kfx {
        0 | 1 => 0usize,
        2 => 1,
        _ => 2,
    };
    let compare_e = |s: &IonoState| if s.fi[kt][0] - s.fi[kr][0] <= 0.0 { kt } else { kr };
    let ks = if kr == 0 {
        kt
    } else if kr == 1 {
        compare_e(s)
    } else if (s.fi[kt][2] - s.fi[kr][2]).abs() - 0.01 <= 0.0 {
        // The 0.01 must agree with SELMOD.
        compare_e(s)
    } else if s.fi[kt][2] - s.fi[kr][2] < 0.0 {
        kt
    } else {
        kr
    };

    lecden(s, ks);

    // Tangent frequencies.
    let xte = 1.0 / (1.0 + AE * s.yi[ks][0] / s.hi[ks][0]).sqrt();
    let fxe = xte * s.fi[ks][0];
    let mut fx1: R = 0.0;
    if s.fi[ks][1] > 0.0 {
        let xt1 = 1.0 / (1.0 + A1 * s.yi[ks][1] / s.hi[ks][1]).sqrt();
        fx1 = xt1 * s.fi[ks][1];
    }
    let mut xt2 = 1.0 / (1.0 + A2 * s.yi[ks][2] / s.hi[ks][2]).sqrt();
    let mut fx2 = xt2 * s.fi[ks][2];
    // Force the F2 MUF to approach MUF(0) at short distances. `XT2` is
    // scaled too, and is read again only on the `IEDP >= 0` path, where
    // it sets the F2 true height.
    if gcdkm - DZF < 0.0 {
        let a = -1.0 + 1.0 / xt2;
        let beta = 1.0 + a * (-BEX * gcdkm / DZF).exp();
        fx2 *= beta;
        xt2 *= beta;
    }

    let mut layers = [LayerMuf::default(); 4];

    // E layer MUF. Below zero `IEDP` reads the heights off the profile;
    // at zero or above it uses the pair a 110 km, 20 km parabolic E
    // layer would give.
    let (hpe, hte) = if s.iedp < 0 {
        gethp(s, fxe)
    } else {
        (125.30, 104.25)
    };
    let g = hop_geometry(gcdkm, amind, hpe, hte, true);
    let emuf = fxe * g.secp;
    layers[0] = LayerMuf {
        sigl: (0.1 * emuf).max(0.01),
        sigu: (0.1 * emuf).max(0.01),
        delmuf: g.del * R2D,
        hpmuf: hpe,
        htmuf: hte,
        fvmuf: fxe,
        afmuf: 0.0,
        nhopmf: g.xhops as i32,
        yfot: 0.0,
        ymuf: emuf,
        yhpf: 0.0,
    };
    layers[0].yfot = emuf - 1.28 * layers[0].sigl;
    layers[0].yhpf = emuf + 1.28 * layers[0].sigu;

    // F2 layer MUF with the Martyn's-theorem iteration. The parabolic
    // form applies only when there is no F1 layer: with one there are
    // too many shapes to write down, and the source falls back to
    // `gethp` for both heights.
    let (hp2, ht2) = if s.iedp < 0 || s.fi[ks][1] > 0.0 {
        gethp(s, fx2)
    } else {
        (
            s.hi[ks][2] - s.yi[ks][2] + bendy(s, 2, ks, fx2) + (pen(s, 0, ks, fx2) - 2.0 * s.yi[ks][0]),
            s.hi[ks][2] - s.yi[ks][2] + s.yi[ks][2] * (1.0 - (1.0 - xt2 * xt2).sqrt()),
        )
    };
    let g = hop_geometry(gcdkm, amind, hp2, ht2, false);
    let mut sphe = g.sphe;
    let mut fob2 = fx2 * g.secp;
    let mut del;
    let xhp = (hp2 - ht2) / RZ;
    let mut hpx2;
    let mut ntry = 0;
    loop {
        let fob1 = fob2;
        // Correction to Martyn's theorem.
        let xmut = sphe * sphe;
        let xfsq = fob1 * fob1 / (s.fi[ks][2] * s.fi[ks][2]);
        let sph = xfsq * xmut * xhp * (ht2 + 2.0 * (RZ + ht2) * xhp);
        hpx2 = hp2 + sph;
        let tanp = g.spsi / (1.0 - g.cpsi + hpx2 / RZ);
        let phe = tanp.atan();
        del = PIO2 - phe - g.psi;
        let cdel = del.cos();
        sphe = RZ * cdel / (RZ + ht2);
        let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
        fob2 = fx2 * secp;
        ntry += 1;
        if (fob2 - fob1).abs() - FQDEL <= 0.0 || NFQ - ntry < 0 {
            break;
        }
    }
    let f2muf = fob2;
    let freq_low = 0.9 * f2muf;
    let sigl3 = f2dis(f2d, f2muf, ssn, clat[ks], freq_low, clck[ks]).max(0.01);
    let freq_high = 1.1 * f2muf;
    let sigu3 = f2dis(f2d, f2muf, ssn, clat[ks], freq_high, clck[ks]).max(0.01);
    layers[2] = LayerMuf {
        sigl: sigl3,
        sigu: sigu3,
        delmuf: del * R2D,
        hpmuf: hpx2,
        htmuf: ht2,
        fvmuf: fx2,
        afmuf: 0.0,
        nhopmf: g.xhops as i32,
        yfot: f2muf - 1.28 * sigl3,
        ymuf: f2muf,
        yhpf: f2muf + 1.28 * sigu3,
    };

    // F1 layer MUF, or a copy of the E layer when no F1 is present.
    let f1muf;
    if s.fi[ks][1] <= 0.0 {
        f1muf = emuf;
        layers[1] = layers[0];
    } else {
        let (hp1, ht1) = gethp(s, fx1);
        let g = hop_geometry(gcdkm, amind, hp1, ht1, true);
        let mut sphe = g.sphe;
        let mut fob2 = fx1 * g.secp;
        let mut del;
        let xhp = (hp1 - ht1) / RZ;
        let mut hpy2;
        let mut ntry = 0;
        loop {
            let fob1 = fob2;
            let xfsq = fob1 * fob1 / (s.fi[ks][1] * s.fi[ks][1]);
            let xmut = sphe * sphe;
            let sph = xfsq * xmut * xhp * (ht1 + 2.0 * (RZ + ht1) * xhp);
            hpy2 = hp1 + sph;
            let tanp = g.spsi / (1.0 - g.cpsi + hpy2 / RZ);
            let phe = tanp.atan();
            del = PIO2 - phe - g.psi;
            let cdel = del.cos();
            sphe = RZ * cdel / (RZ + ht1);
            let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
            fob2 = fx1 * secp;
            ntry += 1;
            if (fob2 - fob1).abs() - FQDEL <= 0.0 || NFQ - ntry < 0 {
                break;
            }
        }
        f1muf = fob2;
        let sig = (0.1 * f1muf).max(0.01);
        layers[1] = LayerMuf {
            sigl: sig,
            sigu: sig,
            delmuf: del * R2D,
            hpmuf: hpy2,
            htmuf: ht1,
            fvmuf: fx1,
            afmuf: 0.0,
            nhopmf: g.xhops as i32,
            yfot: f1muf - 1.28 * sig,
            ymuf: f1muf,
            yhpf: f1muf + 1.28 * sig,
        };
    }

    // Sporadic E MUF: the weakest reflecting area controls.
    let dels = amind.max(0.0);
    let mut esmuf: R = 1000.0;
    let mut ksx = 0usize;
    let mut tdelx: R = 0.0;
    let mut secpx: R = 0.0;
    let mut hop: R = 0.0;
    for (k, e) in es.iter().enumerate().take(s.km) {
        if e.fs[1] <= 0.0 {
            continue;
        }
        let del = dels * D2R;
        let phe = (RZ * del.cos() / (RZ + e.hs)).asin();
        let nhops = (0.5 * gcdkm / ((PIO2 - del - phe) * RZ)) as i32;
        hop = (nhops + 1) as R;
        let psi = 0.5 * gcd / hop;
        let tdel = (psi.cos() - RZ / (RZ + e.hs)) / psi.sin();
        let cdel = 1.0 / (1.0 + tdel * tdel).sqrt();
        let sphe = RZ * cdel / (RZ + e.hs);
        let secp = 1.0 / (1.0 - sphe * sphe).sqrt();
        let esd = e.fs[1] * secp;
        if esmuf - esd > 0.0 {
            esmuf = esd;
            ksx = k;
            tdelx = tdel;
            secpx = secp;
        }
    }
    if esmuf - 1000.0 >= 0.0 {
        esmuf = 0.0;
        layers[3] = LayerMuf::default();
    } else {
        let sigu4 = (es[ksx].fs[2] - es[ksx].fs[1]) / 1.28;
        let sigb = (es[ksx].fs[1] - es[ksx].fs[0]) / 1.28;
        let fzero: R = 0.1;
        let sigz = (es[ksx].fs[1] - fzero) / 3.1;
        let xes = s.fi[ks][0] - es[ksx].fs[0];
        let zdif: R = 0.1;
        let mut sigl4 = if xes + zdif < 0.0 {
            // The lower decile is a regular E: use the map's spread.
            sigb
        } else if xes - zdif < 0.0 {
            sigb + ((xes + zdif) / (2.0 * zdif)) * (sigu4 - sigb)
        } else {
            sigu4
        };
        sigl4 = sigl4.min(sigz);
        // The source rewrites the Es lower decile of the controlling area.
        es[ksx].fs[0] = es[ksx].fs[1] - 1.28 * sigl4;
        let sigl4 = sigl4 * secpx;
        let sigu4 = sigu4 * secpx;
        layers[3] = LayerMuf {
            sigl: sigl4,
            sigu: sigu4,
            delmuf: tdelx.atan() * R2D,
            hpmuf: es[ksx].hs,
            htmuf: es[ksx].hs,
            fvmuf: es[ksx].fs[1],
            afmuf: 0.0,
            nhopmf: hop as i32,
            yfot: esmuf - 1.28 * sigl4,
            ymuf: esmuf,
            yhpf: esmuf + 1.28 * sigu4,
        };
    }

    // Circuit MUF (Es not included).
    let fot = layers[0].yfot.max(layers[1].yfot).max(layers[2].yfot);
    let allmuf = emuf.max(f1muf).max(f2muf);
    let hpf = layers[0].yhpf.max(layers[1].yhpf).max(layers[2].yhpf);
    let (angmuf, modmuf) = if emuf - allmuf >= 0.0 {
        (layers[0].delmuf, 1)
    } else if f1muf - allmuf >= 0.0 {
        (layers[1].delmuf, 2)
    } else {
        (layers[2].delmuf, 3)
    };

    MufHour {
        emuf,
        f1muf,
        f2muf,
        esmuf,
        allmuf,
        fot,
        hpf,
        angmuf,
        modmuf,
        layers,
        ks,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn simple_layers() -> Vec<LayerParams> {
        // A plausible daytime mid-latitude ionosphere at one point.
        let p = LayerParams {
            fi: [3.0, 4.5, 8.0],
            yi: [20.0, 45.0, 80.0],
            hi: [110.0, 180.0, 300.0],
            f2m3: 3.0,
            hpf2: 320.0,
            rat: 3.0,
            abiy: 0.1,
            clck: 12.0,
            zenang: 30.0,
            zenmax: 80.0,
        };
        vec![p]
    }

    #[test]
    fn ionset_single_point_keeps_one_slot() {
        let mut s = IonoState::from_layers(&simple_layers());
        ionset(&mut s);
        assert_eq!(s.kfx, 1);
        assert!(s.fi[0][1] > 0.0, "F1 spacing is fine here");
    }

    #[test]
    fn ionset_drops_a_squeezed_f1() {
        let mut layers = simple_layers();
        layers[0].fi[1] = layers[0].fi[0] + 0.1; // too close to E
        let mut s = IonoState::from_layers(&layers);
        ionset(&mut s);
        assert_eq!(s.fi[0][1], 0.0);
        assert_eq!(s.hi[0][1], 0.0);
    }

    #[test]
    fn the_profile_is_monotone_enough_for_muf() {
        let mut s = IonoState::from_layers(&simple_layers());
        ionset(&mut s);
        lecden(&mut s, 0);
        // The profile spans D region to the F2 peak.
        assert_eq!(s.htr[0], 70.0);
        assert_eq!(s.htr[49], s.hi[0][2]);
        // Peak density equals the F2 critical squared.
        let fc2 = s.fi[0][2] * s.fi[0][2];
        assert!((s.fnsq[49] - fc2).abs() < 1e-3);
        // The virtual height at a mid frequency sits above the true one.
        let (hp, ht) = gethp(&s, 5.0);
        assert!(hp > ht, "hp {hp} ht {ht}");
        assert!(ht > 110.0 && ht < 300.0, "ht {ht}");
    }

    #[test]
    fn curmuf_orders_the_layer_mufs_sensibly() {
        let mut s = IonoState::from_layers(&simple_layers());
        ionset(&mut s);
        let mut es = vec![EsParams {
            fs: [1.0, 3.0, 5.0],
            hs: 110.0,
        }];
        let f2d = [[[0.8 as R; 16]; 6]; 6];
        let clat = [45.0 * D2R];
        let clck = [12.0];
        let gcdkm = 2500.0 as R;
        let gcd = gcdkm / RZ;
        let hour = curmuf(&mut s, &mut es, &f2d, &clat, &clck, gcd, gcdkm, 0.1, 70.0);
        // Oblique MUFs exceed the vertical criticals.
        assert!(hour.f2muf > s.fi[0][2], "f2muf {}", hour.f2muf);
        assert!(hour.emuf > s.fi[0][0]);
        assert_eq!(hour.allmuf, hour.emuf.max(hour.f1muf).max(hour.f2muf));
        assert_eq!(hour.modmuf, 3, "F2 should carry the circuit MUF here");
        // The Es lower decile was rewritten by the distribution logic.
        assert!(es[0].fs[0] != 1.0);
    }
}
