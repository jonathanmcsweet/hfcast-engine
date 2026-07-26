//! The combined noise distribution at the receiver.
//!
//! Port of `anois1.for` (1 MHz atmospheric noise for the local-time
//! block and its neighbour), `genfam.for` (frequency dependence of
//! atmospheric noise from the FAM/DUD tables) and `genois.for` (the
//! combined atmospheric + galactic + man-made distribution, Spaulding's
//! method with the 2007 Caruana modification — the current path, not
//! `genois_old`).
//!
//! The receiver antenna is the app's isotrope (gain 0 dBi, efficiency
//! 0), so `GAIN`'s table interpolation reduces to zero; the porttest
//! trace dumps the Fortran's `EFF` to prove that stays true.

#![allow(clippy::excessive_precision)]

use super::coefficients::CoefficientSet;
use super::con::{R, R2D};
use super::ionosphere::noisy;

/// `GENOIS` DATA: man-made noise environments (industrial, residential,
/// rural, remote, noisy, quiet).
const CONN: [R; 6] = [27.7, 27.7, 27.7, 28.6, 37.5, 29.1];
const XNINT: [R; 6] = [76.8, 72.5, 67.2, 53.6, 83.2, 65.2];
const DFAC: R = 7.87384;
const BFAC: R = 30.99872;
const CFAC: R = 5.56765;

/// The 1 MHz atmospheric noise state (`/ANOIS/` after `ANOIS1`).
#[derive(Debug, Clone, Copy)]
pub struct Anois {
    /// Noise of the current and the neighbouring 4-hour block, dB above
    /// kTb at 1 MHz.
    pub atnu: R,
    pub atny: R,
    /// Receiver local mean time and the block-centre time.
    pub cc: R,
    pub tm: R,
    /// The two time-block indices, 1-based.
    pub kj: usize,
    pub jk: usize,
}

/// Port of `ANOIS1`: evaluates the 1 MHz atmospheric noise maps at the
/// receiver for the local-time block containing `gmtr` and its nearest
/// neighbour. `rlong_deg` is the receiver longitude as the deck gave it
/// (used verbatim for east longitudes, exactly as the Fortran does).
pub fn anois1(set: &CoefficientSet, gmtr: R, rlat: R, rlong: R, rlong_deg: R) -> Anois {
    let cc = gmtr;
    let kj = if cc - 22.0 < 0.0 {
        (cc / 4.0 + 1.0) as i32
    } else {
        6
    };
    let tm = (4 * kj - 2) as R;
    let mut jk = if cc - tm < 0.0 {
        kj - 1
    } else if cc - tm == 0.0 {
        kj
    } else {
        kj + 1
    };
    if jk <= 0 {
        jk = 6;
    } else if jk > 6 {
        jk = 1;
    }
    let ceg = if rlong < 0.0 {
        360.0 + rlong * R2D
    } else {
        rlong_deg
    };
    let xla = rlat * R2D;
    let kj = kj as usize;
    let jk = jk as usize;
    let atnu = noisy(&set.fakp[kj - 1], &set.fakabp[kj - 1], false, xla, ceg);
    let atny = noisy(&set.fakp[jk - 1], &set.fakabp[jk - 1], false, xla, ceg);
    Anois {
        atnu,
        atny,
        cc,
        tm,
        kj,
        jk,
    }
}

/// One `GENFAM` evaluation: the frequency-varied noise and its deciles
/// and prediction errors.
pub struct Fam {
    pub fa: R,
    pub dua: R,
    pub dla: R,
    pub dms: R,
    pub dus: R,
    pub dls: R,
}

/// Port of `GENFAM`: frequency dependence of atmospheric noise. `y2` is
/// the latitude (only its sign is used — southern latitudes use table
/// blocks 7-12), `iblk` the time block, `z` the 1 MHz value.
pub fn genfam(set: &CoefficientSet, y2: R, iblk: usize, freq: R, z: R) -> Fam {
    let ibk = if y2 < 0.0 { iblk + 6 } else { iblk };
    let fam = &set.fam[ibk - 1];
    let mut x = freq.log10();
    let u = (8.0 * (2.0 as R).powf(x) - 11.0) / 4.0;
    // Two passes of the paired polynomials: first at U1 = -0.75 to get
    // the 1 MHz anchor, then at U for the target frequency.
    let poly = |u1: R| {
        let mut pz = u1 * fam[0] + fam[1];
        let mut px = u1 * fam[7] + fam[8];
        for i in 2..7 {
            pz = u1 * pz + fam[i];
            px = u1 * px + fam[i + 7];
        }
        (pz, px)
    };
    let (pz, px) = poly(-0.75);
    let mut cz = z * pz + px;
    cz = z + z - cz;
    let (pz, px) = poly(u);
    let fa = cz * pz + px;
    // The decile and error curves end at 20 MHz (10 MHz for sigma Fam).
    if freq > 20.0 {
        x = (20.0 as R).log10();
    }
    let mut v = [0.0 as R; 5];
    for (i, value) in v.iter_mut().enumerate() {
        if i == 4 && freq > 10.0 {
            x = 1.0;
        }
        // DUD(J,IBK,I) with the Fortran indices reversed.
        let mut y = set.dud[i][ibk - 1][0];
        for j in 1..5 {
            y = y * x + set.dud[i][ibk - 1][j];
        }
        *value = y;
    }
    Fam {
        fa,
        dua: v[0],
        dla: v[1],
        dus: v[2],
        dls: v[3],
        dms: v[4],
    }
}

/// The combined noise outputs (`/ANOIS/` and `/TON/` fields `GENOIS`
/// writes).
#[derive(Debug, Clone, Copy)]
pub struct NoiseResult {
    /// Combined noise at the receiver, dBW in 1 Hz.
    pub rcnse: R,
    /// Upper and lower decile of the combined distribution.
    pub du: R,
    pub dl: R,
    /// Prediction errors: median, upper, lower.
    pub sigm: R,
    pub sygu: R,
    pub sygl: R,
    /// The component levels after conversion to dBW.
    pub atnos: R,
    pub gnos: R,
    pub xnois: R,
    /// The 3 MHz man-made value reported back (`ZNOISE`).
    pub znoise: R,
    /// Receiver antenna efficiency (zero for the isotrope).
    pub eff: R,
}

/// One noise component's power-sum terms in the Fortran's mixed
/// precision: the `EXP` calls are 4-byte, the accumulations 8-byte.
fn component(level: R, du: R, dl: R) -> (f64, f64, f64, f64) {
    let au = f64::from(((du / DFAC).powi(2) + (level / 4.34294)).exp());
    let vu = au * au * f64::from((du * du / BFAC).exp() - 1.0);
    let al = f64::from(((dl / DFAC).powi(2) + (level / 4.34294)).exp());
    let vl = al * al * f64::from((dl * dl / BFAC).exp() - 1.0);
    (au, vu, al, vl)
}

/// Port of `GENOIS` (the current Spaulding + Caruana path): combines
/// atmospheric, galactic and man-made noise at `freq`. `fof2_end` is
/// `FI(3,KFX)` — galactic noise is ignored below the F2 critical of the
/// receiver-end ionosphere. `noise_card` is the deck's noise value
/// (positive: dB below 1 W at 3 MHz; zero or negative: environment
/// index).
pub fn genois(
    set: &CoefficientSet,
    anois: &Anois,
    freq: R,
    rlat: R,
    fof2_end: R,
    noise_card: i32,
) -> NoiseResult {
    let dume = freq.min(55.0);

    // Frequency-dependent atmospheric noise, interpolated between the
    // two local-time blocks.
    let a = genfam(set, rlat, anois.kj, dume, anois.atnu);
    let b = genfam(set, rlat, anois.jk, dume, anois.atny);
    let slop = (anois.cc - anois.tm).abs() / 4.0;
    let mut atnos = a.fa + (b.fa - a.fa) * slop;
    let dua = a.dua + (b.dua - a.dua) * slop;
    let dla = a.dla + (b.dla - a.dla) * slop;
    let sma = a.dms + (b.dms - a.dms) * slop;
    let sua = a.dus + (b.dus - a.dus) * slop;
    let sla = a.dls + (b.dls - a.dls) * slop;
    let (au1, vu1, al1, vl1) = component(atnos, dua, dla);

    // Galactic noise, ignored when it cannot penetrate the F2 layer.
    let mut gnos = 52.0 - 23.0 * freq.log10();
    let dug: R = 2.0;
    let dlg: R = 2.0;
    let smg: R = 0.5;
    let sug: R = 0.2;
    let slg: R = 0.2;
    let galactic = freq > fof2_end;
    let (au2, vu2, al2, vl2) = if galactic {
        component(gnos, dug, dlg)
    } else {
        gnos = 0.0;
        (0.0, 0.0, 0.0, 0.0)
    };

    // Man-made noise from the deck value.
    let man = noise_card;
    let mut xnois = man as R;
    let znoise;
    if man > 0 {
        // A 3 MHz dB-below-1-W value: convert to Fa at 1 MHz, then to
        // the operating frequency.
        znoise = xnois;
        xnois = 204.0 - xnois + 13.22;
        xnois -= 27.7 * freq.log10();
    } else {
        let ma = if man == 0 { 4 } else { man.unsigned_abs().min(4) as usize };
        xnois = XNINT[ma - 1] - CONN[ma - 1] * freq.log10();
        znoise = 204.0 - XNINT[ma - 1] + CONN[ma - 1] * (3.0 as R).log10();
    }
    let dum: R = 9.7;
    let dlm: R = 6.0;
    let sum: R = 1.5;
    let smm: R = 5.4;
    let slm: R = 1.5;
    let (au3, vu3, al3, vl3) = component(xnois, dum, dlm);

    let vu = vu1 + vu2 + vu3;
    let vl = vl1 + vl2 + vl3;
    let au = au1 + au2 + au3;
    let al = al1 + al2 + al3;

    // The isotrope receiver antenna: gain 0, efficiency 0.
    let eff: R = 0.0;

    // Switch to dB above 1 W and power-sum for the Caruana check.
    atnos -= 204.0;
    gnos -= 204.0;
    xnois -= 204.0;
    let rnse = 4.34294
        * ((10.0 as R).powf(atnos * 0.1)
            + (10.0 as R).powf(gnos * 0.1)
            + (10.0 as R).powf(xnois * 0.1))
        .ln();

    let mut sigtsqu = (1.0 + vu / (au * au)).ln();
    let mut sigtsql = (1.0 + vl / (al * al)).ln();
    if dua > 12.0 || dla > 12.0 {
        // Caruana's modification: cap the Spaulding variances where the
        // method breaks down.
        let dxx = f64::from(rnse) + 204.0;
        let dxx2 = 2.0 * (au.ln() - dxx / 4.34294f64);
        let dxx3 = 2.0 * (al.ln() - dxx / 4.34294f64);
        if sigtsqu > dxx2 && dxx2 > 0.0 {
            sigtsqu = dxx2;
        }
        if sigtsql > dxx3 && dxx3 > 0.0 {
            sigtsql = dxx3;
        }
    }
    let xrnse = (f64::from(4.34294 as R) * (au.ln() - sigtsqu / 2.0) - 204.0) as R;
    let du = (f64::from(CFAC) * sigtsqu.sqrt()) as R;
    let dl = (f64::from(CFAC) * sigtsql.sqrt()) as R;

    // Prediction errors, weighted by each component's share.
    let qpa = (10.0 as R).powf((atnos - xrnse) * 0.1);
    let qpg = if galactic {
        (10.0 as R).powf((gnos - xrnse) * 0.1)
    } else {
        0.0
    };
    let qpm = (10.0 as R).powf((xnois - xrnse) * 0.1);
    let sigm = ((qpa * sma).powi(2) + (qpg * smg).powi(2) + (qpm * smm).powi(2)).sqrt();

    let pv = qpa * ((dua - du) * 0.23026).exp();
    let sygu1 = (pv * sua).powi(2) + ((pv - qpa) * sma).powi(2);
    let pv = qpg * ((dug - du) * 0.23026).exp();
    let sygu2 = (pv * sug).powi(2) + ((pv - qpg) * smg).powi(2);
    let pv = qpm * ((dum - du) * 0.23026).exp();
    let sygu3 = (pv * sum).powi(2) + ((pv - qpm) * smm).powi(2);
    let sygu = (sygu1 + sygu2 + sygu3).sqrt();

    let pv = qpa * ((dla - dl) * 0.23026).exp();
    let sygl1 = (pv * sla).powi(2) + ((pv - qpa) * sma).powi(2);
    let pv = qpg * ((dlg - dl) * 0.23026).exp();
    let sygl2 = (pv * slg).powi(2) + ((pv - qpg) * smg).powi(2);
    let pv = qpm * ((dlm - dl) * 0.23026).exp();
    let sygl3 = (pv * slm).powi(2) + ((pv - qpm) * smm).powi(2);
    let sygl = (sygl1 + sygl2 + sygl3).sqrt();

    NoiseResult {
        rcnse: xrnse,
        du,
        dl,
        sigm,
        sygu,
        sygl,
        atnos,
        gnos,
        xnois,
        znoise,
        eff,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn time_blocks_wrap_like_the_source() {
        // The block logic alone, checked at the wrap points.
        let cases: [(R, usize, usize); 4] = [
            (0.5, 1, 6),  // just after midnight: neighbour is block 6
            (2.0, 1, 1),  // block centre: neighbour is itself
            (3.9, 1, 2),  // late in the block: next block
            (23.0, 6, 1), // last block wraps to the first
        ];
        for (cc, kj, jk) in cases {
            let kjc = if cc - 22.0 < 0.0 {
                (cc / 4.0 + 1.0) as i32
            } else {
                6
            };
            let tm = (4 * kjc - 2) as R;
            let mut jkc = if cc - tm < 0.0 {
                kjc - 1
            } else if cc - tm == 0.0 {
                kjc
            } else {
                kjc + 1
            };
            if jkc <= 0 {
                jkc = 6;
            } else if jkc > 6 {
                jkc = 1;
            }
            assert_eq!((kjc as usize, jkc as usize), (kj, jk), "cc {cc}");
        }
    }

    #[test]
    fn genois_combines_toward_the_loudest_component() {
        // With a very loud man-made level the combined noise sits near
        // it; galactic is suppressed below foF2.
        let set = quiet_set();
        let anois = Anois {
            atnu: 40.0,
            atny: 40.0,
            cc: 12.0,
            tm: 14.0,
            kj: 4,
            jk: 3,
        };
        let loud = genois(&set, &anois, 10.0, 0.5, 15.0, 125);
        // 204-125+13.22 - 27.7*log10(10) = 64.5 dB>kTb, i.e. -139.5 dBW.
        assert!(
            (f64::from(loud.rcnse) + 139.0).abs() < 5.0,
            "rcnse {}",
            loud.rcnse
        );
        assert!(loud.du > 0.0 && loud.dl > 0.0);
        // Above foF2 the galactic component joins.
        let with_gal = genois(&set, &anois, 25.0, 0.5, 15.0, 125);
        assert!(with_gal.gnos != 0.0);
    }

    /// A coefficient set whose noise tables are zero — GENFAM then
    /// returns small polynomial constants, which is fine for shape
    /// tests.
    fn quiet_set() -> CoefficientSet {
        CoefficientSet {
            ikim: [[0; 10]; 6],
            f2cof: [[0.0; 13]; 76],
            fm3cof: [[0.0; 9]; 49],
            esmcof: [[0.0; 7]; 61],
            eslcof: [[0.0; 5]; 55],
            esucof: [[0.0; 5]; 55],
            ercof: [[0.0; 9]; 22],
            f2d: [[[0.0; 16]; 6]; 6],
            fakp: [[[0.0; 29]; 16]; 6],
            fakmap: [[0.0; 29]; 16],
            hmym: [[0.0; 29]; 16],
            fakabp: [[0.0; 2]; 6],
            abmap: [[0.0; 2]; 3],
            dud: [[[0.0; 5]; 12]; 5],
            fam: [[0.0; 14]; 12],
            sys: [[[0.0; 9]; 16]; 6],
            perr: [[[0.0; 9]; 4]; 6],
            anew: [0.0; 3],
            bnew: [0.0; 3],
            achi: [0.0; 2],
            bchi: [0.0; 2],
        }
    }
}
