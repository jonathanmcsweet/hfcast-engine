//! Signal-distribution tables and absorption-loss parameters per hour.
//!
//! Port of `sigdis.for` (the `_orig` variant — the installed version
//! file ends in `W`), with its helpers `syssy.for` (excess-system-loss
//! table lookup), `xlin.for` (linear interpolation) and `prbmuf.for`
//! (the engine's standard over-the-MUF probability).
//!
//! `SIGDIS` averages the excess-system-loss tables over the ionosphere
//! slots, sets the D-E absorption parameters, and adjusts the signal
//! distribution deciles for sporadic-E obscuration and F2 over-the-MUF
//! loss at a reference frequency. The `W` version applies the 1999
//! absorption fix (`ABIY` floored at 0.1); the `I`/HAM variants do not
//! and are not ported.

#![allow(clippy::excessive_precision)]

use super::coefficients::CoefficientSet;
use super::con::{R, R2D};
use super::ionogram::Ionogram;
use super::muf::{IonoState, MufHour};

/// Port of `XLIN`: linear interpolation of `yn` at `x` over the `xn`
/// grid, with the source's exact handling of flat and decreasing
/// segments.
pub fn xlin(x: R, xn: &[R], yn: &[R]) -> R {
    if xn[0] - x > 0.0 {
        return yn[0];
    }
    for j in 0..xn.len() - 1 {
        let d = xn[j] - xn[j + 1];
        let interpolate =
            |j: usize| yn[j] + (x - xn[j]) * (yn[j + 1] - yn[j]) / (xn[j + 1] - xn[j]);
        if d < 0.0 {
            if xn[j] - x <= 0.0 {
                if x - xn[j + 1] < 0.0 {
                    return interpolate(j);
                }
            } else {
                return interpolate(j);
            }
        } else if d == 0.0 {
            if x - xn[j] == 0.0 {
                return yn[j];
            }
        } else if xn[j] - x >= 0.0 && x - xn[j + 1] > 0.0 {
            return interpolate(j);
        }
    }
    yn[yn.len() - 1]
}

/// Port of `PRBMUF`: the probability that the operating frequency is
/// usable against the MUF distribution of layer `il` (1-based: E, F1,
/// F2, Es). `fgo` is the layer MUF, `fset` where the distribution
/// median sits; the deciles come from the hour's layer results.
pub fn prbmuf(hour: &MufHour, fmhz: R, fgo: R, fset: R, il: usize) -> R {
    const C: [R; 4] = [0.196854, 0.115194, 0.000344, 0.019527];
    let z = fmhz - fgo;
    if fset <= 0.0 {
        return if z <= 0.0 { 1.0 } else { 0.0 };
    }
    let layer = &hour.layers[il - 1];
    let sig = if z <= 0.0 {
        fgo * layer.sigl / fset
    } else {
        fgo * layer.sigu / fset
    };
    let sig = sig.max(0.001);
    let z = z / sig;
    let yp = z.abs().min(5.0);
    let mut qx = 1.0 + yp * (C[0] + yp * (C[1] + yp * (C[2] + yp * C[3])));
    qx = qx * qx * qx * qx;
    qx = 0.5 * (1.0 / qx);
    if z < 0.0 {
        1.0 - qx
    } else {
        qx
    }
}

/// One `SYSSY` lookup: median and deciles of excess system loss plus
/// its prediction errors.
pub struct SystemLoss {
    pub fm: R,
    pub su: R,
    pub sl: R,
    pub fmp: R,
    pub sup: R,
    pub slp: R,
}

/// Port of `SYSSY`: table lookup in `SYS(9,16,6)` and `PERR(9,4,6)` by
/// geomagnetic latitude `g` (radians), local time `pt` (hours) and area
/// distance class `nn` (2 short, 5 long). The no-data branch (its
/// `DL/1.29` typo included) is unreachable once redmap has run and is
/// not ported.
pub fn syssy(set: &CoefficientSet, g: R, pt: R, nn: usize) -> SystemLoss {
    let j = (pt + 0.5) as i32;
    let gg = g.abs() * 5.729577E1;
    let kj = (((gg - 40.0) / 5.0) + 1.5) as i32;
    let kj = kj.clamp(1, 9) as usize;
    let (kk, nd) = if g < 0.0 { (8, 3) } else { (0, 0) };
    let mut lj = (j as R / 3.0 + 0.67) as i32;
    if lj < 1 {
        lj = 8;
    }
    let lol = lj as usize + kk;
    let mut ld = (j as R / 6.0 + 0.67) as i32;
    if ld < 1 {
        ld = 4;
    }
    let ld = ld as usize;
    // SYS(KJ,LOL,NN) with the Fortran indices reversed.
    let fm = set.sys[nn - 1][lol - 1][kj - 1];
    let du = set.sys[nn][lol - 1][kj - 1];
    let dl = set.sys[nn - 2][lol - 1][kj - 1];
    let fmp = set.perr[nd][ld - 1][kj - 1];
    let sup = set.perr[nd + 1][ld - 1][kj - 1];
    let slp = set.perr[nd + 2][ld - 1][kj - 1];
    SystemLoss {
        fm,
        su: du / 1.28,
        sl: dl / 1.28,
        fmp,
        sup,
        slp,
    }
}

/// The outputs of `SIGDIS`: the `/SIGD/` block, the `/TON/` averages,
/// and the per-slot `ABIY`/`ARTIC` stores.
#[derive(Debug, Clone)]
pub struct SignalDistribution {
    /// Lower and upper decile adjustments of the signal distribution.
    pub dsl: R,
    pub dsu: R,
    /// Residual (auroral) loss adjustment to the median signal level.
    pub asm: R,
    /// Average geomagnetic latitude, radians.
    pub aglat: R,
    /// Average absorption index and E critical frequency.
    pub acav: R,
    pub feav: R,
    /// E-mode loss equation adjustments.
    pub afe: R,
    pub bfe: R,
    /// D-E absorption parameters.
    pub hnu: R,
    pub htloss: R,
    pub xnuz: R,
    pub xve: R,
    /// `/TON/` averages: excess loss median, deciles, prediction errors.
    pub adj: R,
    pub su: R,
    pub sl: R,
    pub ads: R,
    pub sus: R,
    pub sls: R,
    /// Clamped absorption index per slot (`ABIY(1..KFX)`).
    pub abiy: Vec<R>,
    /// Excess system loss per slot (`ARTIC(1..KFX)`).
    pub artic: Vec<R>,
}

/// Port of `SIGDIS_orig`. `glat` and `clck` are per control point
/// (radians, hours), indexed by slot number exactly as the Fortran
/// indexes its unshuffled `/GEOG/` arrays; `ion` is the ionogram of the
/// controlling area `jmode`.
#[allow(clippy::too_many_arguments)]
pub fn sigdis(
    set: &CoefficientSet,
    state: &IonoState,
    hour: &MufHour,
    ion: &Ionogram,
    glat: &[R],
    clck: &[R],
    jmode: usize,
    gcdkm: R,
) -> SignalDistribution {
    let kfxx = state.kfx;
    let xkf = kfxx as R;
    let mut xadj: R = 0.0;
    let mut xsu: R = 0.0;
    let mut xsl: R = 0.0;
    let mut xfmp: R = 0.0;
    let mut xsup: R = 0.0;
    let mut xslp: R = 0.0;
    let mut ac: R = 0.0;
    let mut feav: R = 0.0;
    let mut glav: R = 0.0;
    let mut abiy = Vec::with_capacity(kfxx);
    let mut artic = Vec::with_capacity(kfxx);
    for k in 0..kfxx {
        // Absorption index with the 1999 lower-limit fix.
        let a = (-0.04 + (-2.937 + 0.8445 * state.fi[k][0]).exp()).max(0.1);
        abiy.push(a);
        let idp = if gcdkm > 2500.0 { 5 } else { 2 };
        let loss = syssy(set, glat[k], clck[k], idp);
        xadj += loss.fm;
        xsu += loss.su;
        xsl += loss.sl;
        xfmp += loss.fmp;
        xsup += loss.sup;
        xslp += loss.slp;
        feav += state.fi[k][0];
        ac += a;
        artic.push(loss.fm);
        glav += glat[k].abs();
    }
    glav /= xkf;
    let aglat = glav;
    let acav = ac / xkf;
    feav /= xkf;
    let adj = xadj / xkf;
    let su = xsu / xkf;
    let sl = xsl / xkf;
    let ads = xfmp / xkf;
    let sus = xsup / xkf;
    let sls = xslp / xkf;

    // D-E absorption loss parameters.
    let htloss: R = 88.0;
    let xve = xlin(90.0, &ion.htrue, &ion.fvert) / state.fi[jmode][0];
    let xnuz: R = 63.07;
    let hnu: R = 4.39;
    let (afe, bfe): (R, R) = if feav - 2.0 > 0.0 {
        // Adjustment to the CCIR 252 (Haydon, Lucas) loss equation.
        (1.359, 8.617)
    } else if feav - 0.5 <= 0.0 {
        (0.0, 0.0)
    } else {
        (
            1.359 * (feav - 0.5) / 1.5,
            8.617 * (feav - 0.5) / 1.5,
        )
    };

    // The signal distribution table frequency FTAB, from the F2 FOT,
    // pulled toward 10 MHz near the poles.
    let glav = glav * R2D;
    let ftab = if glav - 40.0 <= 0.0 {
        hour.layers[2].yfot
    } else if glav - 50.0 <= 0.0 {
        let f = hour.layers[2].yfot;
        f - (glav - 40.0) * (f - 10.0) / 10.0
    } else {
        10.0
    };

    // Remove Es obscuration and F2 over-the-MUF at FTAB from the
    // tables; each mode and frequency replaces them as necessary.
    let mut eslsm: R = 0.0;
    if hour.layers[3].ymuf > 0.0 {
        let pes = prbmuf(hour, ftab, hour.layers[3].ymuf, hour.layers[3].ymuf, 4)
            .clamp(0.1, 0.9);
        eslsm = -10.0 * (1.0 - pes).log10();
    }
    let pf2 = prbmuf(hour, ftab, hour.layers[2].ymuf, hour.layers[2].ymuf, 3).max(0.1);
    let f2lsm = -10.0 * pf2.log10();
    let asm = (adj - eslsm - f2lsm).max(0.0);

    // Upper decile.
    let mut pes: R = 0.0;
    if hour.layers[3].yfot > 0.0 {
        pes = prbmuf(hour, ftab, hour.layers[3].yfot, hour.layers[3].yfot, 4)
            .clamp(0.1, 0.9);
    }
    let pf2 = prbmuf(hour, ftab, hour.layers[2].yhpf, hour.layers[2].yhpf, 3).max(0.1);
    let dsu = (1.28 * sl
        - (10.0 * (1.0 - pes).log10() + eslsm)
        - (10.0 * pf2.log10() + f2lsm))
        .max(0.5);

    // Lower decile.
    let mut pes: R = 0.0;
    if hour.layers[3].yhpf > 0.0 {
        pes = prbmuf(hour, ftab, hour.layers[3].yhpf, hour.layers[3].yhpf, 4)
            .clamp(0.1, 0.9);
    }
    let pf2 = prbmuf(hour, ftab, hour.layers[2].yfot, hour.layers[2].yfot, 3).max(0.1);
    let dsl = (1.28 * su
        - (-10.0 * (1.0 - pes).log10() - eslsm)
        - (-10.0 * pf2.log10() - f2lsm))
        .max(1.0);

    SignalDistribution {
        dsl,
        dsu,
        asm,
        aglat,
        acav,
        feav,
        afe,
        bfe,
        hnu,
        htloss,
        xnuz,
        xve,
        adj,
        su,
        sl,
        ads,
        sus,
        sls,
        abiy,
        artic,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xlin_interpolates_and_extrapolates_like_the_source() {
        let xn = [1.0 as R, 2.0, 3.0];
        let yn = [10.0 as R, 20.0, 40.0];
        assert_eq!(xlin(1.5, &xn, &yn), 15.0);
        // Below the grid: the first value.
        assert_eq!(xlin(0.5, &xn, &yn), 10.0);
        // Above the grid: the last value.
        assert_eq!(xlin(9.0, &xn, &yn), 40.0);
    }

    #[test]
    fn prbmuf_is_a_falling_probability_through_the_muf() {
        let mut hour = crate::engine::muf::MufHour {
            emuf: 0.0,
            f1muf: 0.0,
            f2muf: 10.0,
            esmuf: 0.0,
            allmuf: 10.0,
            fot: 8.0,
            hpf: 12.0,
            angmuf: 5.0,
            modmuf: 3,
            layers: [Default::default(); 4],
            ks: 0,
        };
        hour.layers[2].sigl = 1.0;
        hour.layers[2].sigu = 1.0;
        let below = prbmuf(&hour, 8.0, 10.0, 10.0, 3);
        let at = prbmuf(&hour, 10.0, 10.0, 10.0, 3);
        let above = prbmuf(&hour, 12.0, 10.0, 10.0, 3);
        assert!(below > 0.9, "below {below}");
        assert!((at - 0.5).abs() < 0.01, "at {at}");
        assert!(above < 0.1, "above {above}");
        // No distribution: a step function.
        assert_eq!(prbmuf(&hour, 8.0, 10.0, 0.0, 3), 1.0);
        assert_eq!(prbmuf(&hour, 12.0, 10.0, 0.0, 3), 0.0);
    }
}
