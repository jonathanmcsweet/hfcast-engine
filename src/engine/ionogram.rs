//! The ionogram, reflectrix and deviative-loss tables per sample area.
//!
//! Port of `sang.for` (elevation-angle scan limit), `selmod.for`
//! (controlling-area selection), `genion.for` (30-point ionogram),
//! `fobby.for` (oblique-frequency reflectrix) and `alosfv.for`
//! (deviative loss factors). `LUFFY` runs this chain after an
//! `lecden` call for the chosen area: once for the short-path control
//! area, or for both end areas on long paths.
//!
//! `genion` is ported for the `IEDP < 0` configuration only (the decks
//! carry no INTEGRATE card): every ionogram height comes from `gethp`
//! on the density profile, so the parabolic fast path is dead code.

// Constants are kept digit for digit with the Fortran.
#![allow(clippy::excessive_precision)]

use super::con::{D2R, R, RZ};
use super::muf::{gethp, IonoState, LayerMuf};

/// The elevation-angle scan, `ANG(40)` from `blkdat.for`, degrees.
pub const ANG: [R; 40] = [
    0.0, 0.5, 1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 18.0, 20.0, 22.0, 24.0,
    26.0, 28.0, 30.0, 32.0, 34.0, 36.0, 38.0, 40.0, 42.0, 44.0, 46.0, 48.0, 50.0, 52.0, 54.0,
    56.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0, 89.99,
];

/// `NANGX`: how many of the scan angles apply per 2000 km of distance.
const NANGX: [usize; 8] = [40, 34, 29, 24, 19, 14, 12, 9];

/// Port of `SANG`: the number of scan angles for this path length,
/// raised only if the minimum takeoff angle demands it.
pub fn sang(gcdkm: R, amind: R) -> usize {
    let id = ((gcdkm / 2000.0 + 1.0) as i32).min(8);
    let mut iang = NANGX[id as usize - 1];
    if amind > 0.0 && ANG[iang - 1] - 10.0 - amind < 0.0 {
        for ia in 1..=40 {
            iang = ia;
            if ANG[iang - 1] - 8.5 - amind >= 0.0 {
                break;
            }
        }
    }
    iang
}

/// Port of `SELMOD`: which sample area controls the ray-path
/// calculations (`JMODE`, 0-based here).
pub fn selmod(s: &IonoState) -> usize {
    if s.kfx == 1 {
        0
    } else if s.kfx == 2 {
        // The two slots share the F layer (set equal in ionset), so the
        // E layer decides.
        if s.fi[0][0] > s.fi[1][0] {
            1
        } else {
            0
        }
    } else {
        let delfi = s.fi[0][2] - s.fi[2][2];
        if delfi.abs() < 0.01 {
            if s.fi[0][0] > s.fi[2][0] {
                2
            } else {
                0
            }
        } else if delfi > 0.0 {
            2
        } else {
            0
        }
    }
}

/// The per-area ionogram tables the Fortran keeps in `/RON/`:
/// `FVERT(30)`, `HPRIM(30)`, `HTRUE(30)`, `AFAC(30)`.
#[derive(Debug, Clone, Copy)]
pub struct Ionogram {
    /// Vertical sounding frequencies, MHz.
    pub fvert: [R; 30],
    /// Virtual heights of reflection, km.
    pub hprim: [R; 30],
    /// True heights of reflection, km.
    pub htrue: [R; 30],
    /// Deviative loss factors.
    pub afac: [R; 30],
}

/// Port of `GENION` for `IEDP < 0`: the sounding-frequency grid and its
/// true and virtual heights from the density profile (which `lecden`
/// must have built for the same area first). `afac` is zeroed here and
/// filled by [`alosfv`].
pub fn genion(s: &IonoState, k: usize) -> Ionogram {
    let fi0 = s.fi[k][0];
    let fi1 = s.fi[k][1];
    let fi2 = s.fi[k][2];
    let mut fv = [0.0 as R; 30];
    // D-E region tail; XTAIL must agree with lecden.
    let xtail: R = 0.85;
    let fex = fi0 * (1.0 - xtail * xtail).sqrt();
    fv[0] = 0.01;
    fv[3] = fex;
    let fdif = (fv[3] - fv[0]) / 3.0;
    fv[1] = fv[0] + fdif;
    fv[2] = fv[1] + fdif;
    // E region nose.
    fv[8] = 0.957 * fi0;
    fv[9] = 0.99 * fi0;
    let fdif = (fv[8] - fv[3]) / 5.0;
    for i in 4..8 {
        fv[i] = fv[i - 1] + fdif;
    }
    // E-F cusp and F region nose.
    fv[10] = 1.05 * fi0;
    fv[29] = 0.99 * fi2;
    fv[28] = 0.98 * fi2;
    fv[27] = 0.96 * fi2;
    fv[26] = 0.92 * fi2;
    if fi1 <= 0.0 {
        // F2 layer, no F1 layer.
        let fdif = (fv[26] - fv[10]) / 16.0;
        for i in 11..26 {
            fv[i] = fv[i - 1] + fdif;
        }
    } else {
        // F1 and F2 layers, with the F1-F2 cusp at index 20.
        fv[19] = 0.99 * fi1;
        let fdif = (fv[19] - fv[10]) / 9.0;
        for i in 11..19 {
            fv[i] = fv[i - 1] + fdif;
        }
        fv[20] = 1.01 * fi1;
        let fdif = (fv[26] - fv[20]) / 6.0;
        for i in 21..26 {
            fv[i] = fv[i - 1] + fdif;
        }
    }
    let mut ion = Ionogram {
        fvert: fv,
        hprim: [0.0; 30],
        htrue: [0.0; 30],
        afac: [0.0; 30],
    };
    for i in 0..30 {
        let (hp, ht) = gethp(s, ion.fvert[i]);
        ion.hprim[i] = hp;
        ion.htrue[i] = ht;
    }
    ion
}

/// Port of `FOBBY`: the reflectrix — oblique frequencies in kHz (as
/// integers, exactly the Fortran's `IFOB`) per scan angle and ionogram
/// point.
pub fn fobby(ion: &Ionogram, nang: usize) -> Vec<[i32; 30]> {
    (0..nang)
        .map(|ia| {
            let del = ANG[ia] * D2R;
            let rcosd = RZ * del.cos();
            let mut row = [0i32; 30];
            for (ih, out) in row.iter_mut().enumerate() {
                let fvv = ion.fvert[ih];
                let sphe = rcosd / (RZ + ion.htrue[ih]);
                let sqcos = (1.0 - sphe * sphe).max(0.000001);
                let cphe = sqcos.sqrt();
                let freq = fvv / cphe;
                *out = (1000.0 * freq) as i32;
            }
            row
        })
        .collect()
}

/// Port of `ALOSFV`: fills the deviative loss factors for area `k`.
/// Nonzero only for reflections from above the height at each layer's
/// MUF; the exponential continuity constants follow the source.
pub fn alosfv(s: &IonoState, k: usize, ion: &mut Ionogram, layers: &[LayerMuf; 4]) {
    let a1: R = 0.2;
    // E layer.
    let hm1 = layers[0].hpmuf - layers[0].htmuf;
    let hz = s.hi[k][0] - s.yi[k][0];
    let mut cf: R = 0.0;
    for ih in 0..10 {
        if ion.htrue[ih] - layers[0].htmuf <= 0.0 {
            ion.afac[ih] = 0.0;
        } else {
            let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][0]).max(-10.0);
            cf = a1 * zexp.exp();
            ion.afac[ih] = cf * (ion.hprim[ih] - ion.htrue[ih] - hm1);
        }
    }
    if s.fi[k][1] <= 0.0 {
        // F2 layer with no F1 ledge: continuity at the E-F2 cusp.
        let a2 = cf;
        let hz = ion.htrue[10];
        let hm2 = ion.hprim[12] - ion.htrue[12];
        for ih in 10..12 {
            let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][2]).max(-10.0);
            let c = (a2 * zexp.exp()).max(0.05);
            ion.afac[ih] = c * (ion.hprim[ih] - ion.htrue[ih] - hm2);
        }
        ion.afac[12] = 0.0;
        let a3: R = 0.1;
        let hm2 = layers[2].hpmuf - layers[2].htmuf;
        for ih in 13..30 {
            if ion.htrue[ih] - layers[2].htmuf <= 0.0 {
                ion.afac[ih] = 0.0;
            } else {
                let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][2]).max(-10.0);
                // The source forces this floor to 0.5, unlike the 0.05
                // used everywhere else.
                let c = (a3 * zexp.exp()).max(0.5);
                ion.afac[ih] = c * (ion.hprim[ih] - ion.htrue[ih] - hm2);
            }
        }
    } else {
        // F1 layer: continuity at the E-F1 cusp.
        let a2 = cf;
        let hz = ion.htrue[10];
        let hm2 = ion.hprim[12] - ion.htrue[12];
        for ih in 10..12 {
            let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][1]).max(-10.0);
            let c = (a2 * zexp.exp()).max(0.05);
            ion.afac[ih] = c * (ion.hprim[ih] - ion.htrue[ih] - hm2);
        }
        ion.afac[12] = 0.0;
        // (The source sets A(2) = 0.1 here; that store is dead — the
        // next loop reads A(3), which is 0.1 at this point.)
        let a3: R = 0.1;
        let htm = layers[1].htmuf;
        let hm2 = layers[1].hpmuf - htm;
        let mut cf2: R = 0.0;
        let mut ran_f1_tail = false;
        for ih in 13..20 {
            if ion.htrue[ih] - htm <= 0.0 {
                ion.afac[ih] = 0.0;
            } else {
                let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][1]).max(-10.0);
                cf2 = (a3 * zexp.exp()).max(0.05);
                ran_f1_tail = true;
                ion.afac[ih] = cf2 * (ion.hprim[ih] - ion.htrue[ih] - hm2);
            }
        }
        // F2 layer with F1 ledge: continuity at the F1-F2 cusp. The
        // source seeds A(3) with CF, which holds the last value computed
        // in the previous loop (or the E-F1 cusp value if none was).
        let a3 = if ran_f1_tail {
            cf2
        } else {
            (a2 * ((-2.0 * (ion.htrue[11] - hz) / s.yi[k][1]).max(-10.0)).exp()).max(0.05)
        };
        let hz = ion.htrue[20];
        let hm2 = ion.hprim[22] - ion.htrue[22];
        for ih in 20..22 {
            let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][2]).max(-10.0);
            let c = (a3 * zexp.exp()).max(0.05);
            ion.afac[ih] = c * (ion.hprim[ih] - ion.htrue[ih] - hm2);
        }
        ion.afac[22] = 0.0;
        let a3: R = 0.1;
        let htm = layers[2].htmuf;
        let hm2 = layers[2].hpmuf - htm;
        for ih in 23..30 {
            if ion.htrue[ih] - htm <= 0.0 {
                ion.afac[ih] = 0.0;
            } else {
                let zexp = (-2.0 * (ion.htrue[ih] - hz) / s.yi[k][2]).max(-10.0);
                let c = (a3 * zexp.exp()).max(0.05);
                ion.afac[ih] = c * (ion.hprim[ih] - ion.htrue[ih] - hm2);
            }
        }
    }
    for a in ion.afac.iter_mut() {
        if *a < 0.0 {
            *a = 0.0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::ionosphere::LayerParams;
    use crate::engine::muf::{ionset, lecden};

    fn state() -> IonoState {
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
        let mut s = IonoState::from_layers(&[p]);
        ionset(&mut s);
        lecden(&mut s, 0);
        s
    }

    #[test]
    fn sang_narrows_the_scan_with_distance() {
        assert_eq!(sang(500.0, 0.0), 40);
        assert_eq!(sang(2500.0, 0.0), 34);
        assert_eq!(sang(15000.0, 0.0), 9);
        // A minimum takeoff angle above the table's top angle minus 10
        // degrees widens the scan to cover it.
        assert_eq!(sang(15000.0, 0.1), 10);
        assert!(sang(15000.0, 30.0) > 10);
    }

    #[test]
    fn the_ionogram_grid_brackets_the_criticals() {
        let s = state();
        let ion = genion(&s, 0);
        // The grid ends just below the F2 critical and heights rise.
        assert!((ion.fvert[29] - 0.99 * s.fi[0][2]).abs() < 1e-5);
        assert!(ion.htrue[29] > ion.htrue[0]);
        // Virtual height is never below true height.
        for i in 0..30 {
            assert!(
                ion.hprim[i] >= ion.htrue[i] - 1e-3,
                "index {i}: {} vs {}",
                ion.hprim[i],
                ion.htrue[i]
            );
        }
    }

    #[test]
    fn the_reflectrix_grows_toward_low_angles() {
        let s = state();
        let ion = genion(&s, 0);
        let table = fobby(&ion, 40);
        // Lower elevation angles give larger oblique frequencies.
        assert!(table[0][29] > table[39][29]);
        // Near-vertical incidence approaches the sounding frequency.
        let vertical_khz = (1000.0 * ion.fvert[29]) as i32;
        assert!((table[39][29] - vertical_khz).abs() < vertical_khz / 10);
    }
}
