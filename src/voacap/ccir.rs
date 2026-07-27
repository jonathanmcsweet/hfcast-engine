//! The CCIR antenna models (`vendor/voacapl/src/wp10dwin`), antenna
//! types 1-9 and the NTIA curtain array, type 12.
//!
//! Types 1-9 are the REC705 patterns: `antinit2` extracts the
//! definition file's parameters and precomputes tables (`trigfun`,
//! `parmprec`, `logparm` for the log-periodics), `gainorm` scans for
//! the pattern maximum, and `gainrel`/`ccirgain` evaluate the relative
//! gain per direction. Type 12 is the NTIA Report 87-215 curtain
//! (`curtain`, `pattrn0`, `f2`, `dbltrap`), a separate model with its
//! own normalising table read from the definition file.
//!
//! Dead code in the source, ported as absent: `antinit2` returns
//! before `parmprec` for type 10 ("vertical monopole is a table"), so
//! `parmprec`'s monopole branch with its Bessel and surface-impedance
//! helpers (`surfim`, `bessel`) is unreachable from any driver; and
//! `dirgain`, the Simpson-rule directivity integration, has no callers
//! at all.

// The literals are the source's own digits (3.1415927, 299.8, and the
// curve-fit coefficients); replacing them with exact constants would
// change the arithmetic.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use super::antenna::AntennaFile;
use super::con::R;

/// `p1` in `/general/`: the CCIR code's own pi.
const P1: R = 3.1415927;
/// `q1`: degrees to radians.
const Q1: R = P1 / 180.0;
/// The antenna floor value, `/floorc/`: gains are clamped at -30 dB
/// (the code stores 30 and negates).
const FLOOR: R = 30.0;

/// One CCIR antenna at one operating frequency: everything `antinit2`
/// leaves in the COMMON blocks for `gainrel` to read.
pub struct CcirAntenna {
    iant: i32,
    /// `/general/`: ground and frequency.
    d1: R,
    e1: R,
    beta: R,
    /// `/cros/`: the per-type pattern constants.
    cpfr: R,
    cp11: R,
    fr1: R,
    h1: R,
    pfr: R,
    p11: R,
    p11s: R,
    q2: R,
    r2: R,
    r3: R,
    sl: R,
    /// `/surf/` direction cosines of the log-periodic boom or the
    /// rhombic half-angle.
    costh: R,
    sinth: R,
    /// `/logp/`: log-periodic element geometry, 1-based as in the
    /// source.
    fi: [R; 36],
    rim: [R; 36],
    x: [R; 36],
    y: [R; 36],
    iek: [usize; 36],
    rl: [R; 36],
    h: [R; 31],
    /// `/ccparm/` nat: how many log-periodic elements are active.
    nat: usize,
    /// `/trig/`: sines and cosines per whole degree, with the doctored
    /// end values.
    a: [R; 91],
    b: [R; 91],
    /// `/warr/`: ground-reflection factors per elevation degree.
    w3: [R; 91],
    w4: [R; 91],
    w5: [R; 91],
    w6: [R; 91],
    w9: [R; 91],
    w10: [R; 91],
    /// `/wwar/`: the same per log-periodic element.
    ww1: Vec<[R; 31]>,
    ww2: Vec<[R; 31]>,
    ww3: Vec<[R; 31]>,
    ww4: Vec<[R; 31]>,
    /// `gainorm`'s outputs: the pattern maximum and where it is.
    pub z6: R,
    pub umax: R,
    pub vmax: R,
    ifs: i32,
}

/// `trigfun`: the whole-degree sine and cosine tables.
///
/// The end values are doctored on purpose — `a(90)` is `sin(90.05°)`,
/// `b(90)` is `cos(90.105°)` (slightly negative), and `b(0)` is
/// `cos(0.005)` in *radians*, the one entry where the degree factor is
/// missing. All three are kept.
fn trigfun() -> ([R; 91], [R; 91]) {
    let mut a = [0.0 as R; 91];
    let mut b = [0.0 as R; 91];
    for z in 0..=90usize {
        a[z] = (z as R * Q1).sin();
        b[z] = (z as R * Q1).cos();
    }
    a[90] = (90.05 * Q1).sin();
    b[90] = (90.105 * Q1).cos();
    b[0] = (0.005 as R).cos();
    (a, b)
}

/// `refcof`: the ground reflection coefficients at one direction.
/// Returns `(t1, t2, t3, t4)` — vertical then horizontal, real and
/// imaginary.
fn refcof(a0: R, b0: R, d1: R, e1: R) -> (R, R, R, R) {
    let c1 = e1 - b0 * b0;
    let c11 = (c1 * c1 + d1 * d1).sqrt();
    let e2 = ((c1 + c11).abs() / 2.0).sqrt();
    let f2 = ((-c1 + c11).abs() / 2.0).sqrt();
    let rk1 = e1 * a0 - e2;
    let rl1 = rk1 + e2 * 2.0;
    let rm1 = d1 * a0 - f2;
    let rn1 = rm1 + f2 * 2.0;
    let rl1n = rl1 * rl1 + rn1 * rn1;
    let t1 = (rk1 * rl1 + rm1 * rn1) / rl1n;
    let t2 = (rk1 * rn1 - rm1 * rl1) / rl1n;
    let g1 = a0 - e2;
    let h2 = a0 + e2;
    let hn = h2 * h2 + f2 * f2;
    let t3 = (g1 * h2 - f2 * f2) / hn;
    let t4 = (h2 * f2 + g1 * f2) / hn;
    (t1, t2, t3, t4)
}

/// `parabol`: fits `y = aa x^2 + bb x + cc` through three points.
fn parabol(x1: R, x2: R, x3: R, y1: R, y2: R, y3: R) -> (R, R, R) {
    let dd = x1 * x1 * (x2 - x3) - x1 * (x2 * x2 - x3 * x3) + (x2 * x2 * x3 - x3 * x3 * x2);
    let da = y1 * (x2 - x3) - x1 * (y2 - y3) + (y2 * x3 - x2 * y3);
    let db = x1 * x1 * (y2 - y3) - y1 * (x2 * x2 - x3 * x3) + (x2 * x2 * y3 - x3 * x3 * y2);
    let dc = x1 * x1 * (x2 * y3 - y2 * x3) - x1 * (x2 * x2 * y3 - y2 * x3 * x3)
        + y1 * (x2 * x2 * x3 - x3 * x3 * x2);
    (da / dd, db / dd, dc / dd)
}

/// `curcal`: the empirical active-element current curves, five
/// parabolic segments over the element-length range.
#[allow(clippy::type_complexity)]
fn curcal(rl0: R, rllow: R, rlc: R, rlup: R) -> ([R; 5], [R; 5], [R; 5]) {
    let mut aa = [0.0 as R; 5];
    let mut bb = [0.0 as R; 5];
    let mut cc = [0.0 as R; 5];

    let (a, b, c) = parabol(
        rllow,
        rllow + (rl0 - rllow) / 4.0,
        rllow + (rl0 - rllow) / 2.0,
        0.316,
        0.676,
        0.876,
    );
    aa[0] = a;
    bb[0] = b;
    cc[0] = c;

    let (a, b, c) = parabol(
        rl0,
        rllow + 3.0 * (rl0 - rllow) / 4.0,
        rllow + (rl0 - rllow) / 2.0,
        1.0,
        0.976,
        0.876,
    );
    aa[1] = a;
    bb[1] = b;
    cc[1] = c;

    // The third segment is fitted as a parabola with its vertex at
    // (rl0, 1) rather than through three points.
    let x1 = rl0;
    let y1 = 1.0;
    let x2 = rllow + 3.0 * (rl0 - rllow) / 4.0;
    let y2 = 0.976;
    aa[2] = (y2 - y1) / ((x2 - x1) * (x2 - x1));
    bb[2] = -2.0 * aa[2] * x1;
    cc[2] = y1 + x1 * x1 * aa[2];

    let (a, b, c) = parabol(rl0, rl0 + (rlc - rl0) / 2.0, rlc, 1.0, 0.922, 0.707);
    aa[3] = a;
    bb[3] = b;
    cc[3] = c;

    let (a, b, c) = parabol(rlc + (rlup - rlc) / 2.0, rlup, rlc, 0.501, 0.316, 0.707);
    aa[4] = a;
    bb[4] = b;
    cc[4] = c;

    (aa, bb, cc)
}

/// `gainterb`: interpolates a per-MHz maximum gain (types 5-7).
fn gainterb(gainab: &[R; 30], freq: R) -> R {
    let mut idx = freq as usize;
    if idx == 30 {
        idx = 29;
    }
    let fact = freq - idx as R;
    let lo = gainab[idx.saturating_sub(1)];
    let hi = gainab[idx.min(29)];
    lo + (hi - lo) * fact
}

/// `gainterp1`: one frequency's maximum gain over the frequency ratio.
fn gainterp1(gainaa: &[R; 3], fr: R, iant: i32) -> R {
    const FRS: [[R; 3]; 2] = [[0.7, 1.0, 1.4], [0.85, 1.0, 1.2]];
    let jant = if iant == 1 { 0 } else { 1 };
    let idx = if fr < 1.0 { 0 } else { 1 };
    gainaa[idx]
        + (gainaa[idx + 1] - gainaa[idx]) * (fr - FRS[jant][idx]) / (FRS[jant][idx + 1] - FRS[jant][idx])
}

/// `gainterp`: the maximum gain over operating and design frequency
/// (types 1-4, 8 and 9). Out-of-ratio designs answer -30.
fn gainterp(gainmax: &[[R; 3]; 2], freq_oper: R, freq_design: R, iant: i32) -> R {
    let fr = freq_oper / freq_design;
    let outside = if iant == 1 {
        !(0.7..=1.4).contains(&fr)
    } else {
        !(0.85..=1.2).contains(&fr)
    };
    if outside {
        return -30.0;
    }
    let g1 = gainterp1(&gainmax[0], fr, iant);
    let g2 = gainterp1(&gainmax[1], fr, iant);
    g1 + (g2 - g1) * (freq_oper - 2.0) / 28.0
}

/// `setmaxgain`: the maximum gain for the operating frequency, and the
/// `parm` slots it overwrites on the way (`parm(5)`, `parm(8)`, and
/// `parm(6)` for the quadrant antenna).
///
/// Returns `giso`, or `None` for `modegain` 3 (the curtain), where the
/// source falls through every branch and leaves `giso` stale.
pub fn setmaxgain(file: &AntennaFile, parm: &mut [R; 20], freq_oper: R, freq_design_card: R) -> Option<R> {
    let iant = file.jant();
    match file.modegain {
        0 => Some(parm[0]),
        1 => {
            let mut foper = freq_oper;
            if !(2.0..=30.0).contains(&foper) {
                foper = 10.0;
            }
            let mut fdesign = freq_design_card;
            // A design value between .7 and 1.4 is read as a frequency
            // ratio rather than a frequency.
            if (0.7..=1.4).contains(&fdesign) {
                fdesign = foper / freq_design_card;
            }
            if !(2.0..=30.0).contains(&fdesign) {
                fdesign = foper;
            }
            parm[4] = foper;
            parm[7] = fdesign;
            if iant == 8 {
                parm[5] = fdesign;
            }
            Some(gainterp(&file.gainmax, foper, fdesign, iant))
        }
        2 => {
            parm[4] = freq_oper;
            Some(gainterb(&file.gainmaxb, freq_oper))
        }
        _ => None,
    }
}

/// `logparm`: the log-periodic element geometry, active-element
/// selection and current distribution.
#[allow(clippy::too_many_arguments)]
fn logparm(
    ant: &mut CcirAntenna,
    iant: i32,
    nel: usize,
    rlnel: R,
    rl1: R,
    hnel: R,
    h1: R,
    dc: R,
    z0: R,
    rlambda: R,
) {
    let mut d = [0.0 as R; 31];
    let mut ria = [0.0 as R; 31];

    ant.rl[nel] = rlnel;
    ant.rl[1] = rl1;
    ant.h[nel] = hnel;
    ant.h[1] = h1;
    let rlr = rl1 / rlnel;
    let exl = 1.0 / (nel as R - 1.0);
    let tau = rlr.powf(exl);
    let rld = rlnel - rl1;
    ant.sinth = (hnel - h1) / dc;
    ant.costh = (1.0 - ant.sinth * ant.sinth).sqrt();

    let sigma;
    if iant != 5 {
        // Vertical log-periodic.
        let dl1 = dc * rl1 / rld;
        let da1 = dl1 * ant.costh;
        let db1 = dl1 * ant.sinth;
        let dc1 = db1 - rl1;
        let tg23 = ant.sinth / ant.costh;
        let alfa23 = tg23.atan();
        let tg3 = dc1 / da1;
        let alfa3 = tg3.atan();
        let alfa2 = alfa23 - alfa3;
        sigma = (1.0 - tau) / (4.0 * (ant.sinth - tg3 * ant.costh));
        let theta = alfa3.sin() * ant.costh / alfa2.sin();
        ant.h[nel] = ant.rl[nel] * (1.0 + theta);
        let h0 = h1 - rl1 * (1.0 + theta);
        for i in (1..nel).rev() {
            ant.rl[i] = ant.rl[i + 1] * tau;
            d[i] = 4.0 * ant.rl[i + 1] * sigma;
            ant.h[i] = ant.rl[i] * (1.0 + theta) + h0;
        }
        ant.y[1] = ant.rl[1] / (tg23 * (1.0 - tg3 / tg23));
        for i in 2..=nel {
            ant.y[i] = ant.y[i - 1] + d[i - 1] * ant.costh;
        }
    } else {
        // Horizontal log-periodic.
        let alftan = rld / dc;
        sigma = (1.0 - tau) / (4.0 * alftan);
        d[nel] = 4.0 * ant.rl[nel] * sigma;
        for i in (1..nel).rev() {
            ant.rl[i] = ant.rl[i + 1] * tau;
            d[i] = 4.0 * ant.rl[i + 1] * sigma;
        }
        ant.x[1] = ant.rl[1] / alftan * ant.costh;
        for i in 2..=nel {
            ant.x[i] = ant.x[i - 1] + d[i - 1] * ant.costh;
            ant.h[i] = ant.h[i - 1] + d[i - 1] * ant.sinth;
        }
    }

    // The active band: elements whose length falls between llow and
    // lup for this operating wavelength (l/a = 500 assumed).
    let z0m = z0 / 1000.0;
    let shf = 1.098790227 - 1.055146365 * z0m + 3.208544524 * (z0m * z0m)
        - 5.766460847 * z0m.powi(3)
        + 4.054233788 * z0m.powi(4);
    let rlc = rlambda * shf / 4.0;
    let bar = 1.1 + 30.7 * sigma * (1.0 - tau);
    let rllow = rlc / bar;
    let rlup = 1.1 * rlc;
    let rl0 = rllow + 0.7166 * (rlc - rllow);

    let mut k = 0usize;
    for i in 1..=nel {
        if ant.rl[i] >= rllow && ant.rl[i] <= rlup {
            k += 1;
            ant.iek[k] = i;
        }
    }
    ant.nat = k;

    let (aa, bb, cc) = curcal(rl0, rllow, rlc, rlup);
    for kk in 1..=ant.nat {
        let i = ant.iek[kk];
        let l = ant.rl[i];
        let seg = if l >= rllow && l < rllow + (rl0 - rllow) / 2.0 {
            0
        } else if l >= rllow + (rl0 - rllow) / 2.0 && l < rllow + 3.0 * (rl0 - rllow) / 4.0 {
            1
        } else if l >= rllow + 3.0 * (rl0 - rllow) / 4.0 && l <= rl0 {
            2
        } else if l >= rl0 && l <= rlc {
            3
        } else if l >= rlc && l <= rlup {
            4
        } else {
            // "current calculation error!" — the source prints and
            // carries on with the element's current undefined.
            usize::MAX
        };
        if seg != usize::MAX {
            // aa * rl(i)**2: the square binds first, so the product
            // is aa times l-squared, not (aa times l) times l.
            ria[i] = aa[seg] * (l * l) + bb[seg] * l + cc[seg];
        }
        let af = if l >= rllow && l <= rl0 {
            150.0 / (rl0 - rllow)
        } else {
            142.0 / (rlup - rl0)
        };
        let bf = -af * rl0;
        ant.fi[i] = af * l + bf;
    }

    // Normalise the currents to the largest, and phases to its phase.
    let mut riamax: R = 0.0;
    let mut fimin: R = 0.0;
    for kk in 1..=ant.nat {
        let i = ant.iek[kk];
        if ria[i] >= riamax {
            riamax = ria[i];
            fimin = ant.fi[i];
        }
    }
    for kk in 1..=ant.nat {
        let i = ant.iek[kk];
        let rinm = ria[i] / riamax;
        ant.fi[i] -= fimin;
        ant.rim[i] = rinm / (ant.beta * ant.rl[i]).sin();
    }
}

/// `parmprec`: the per-elevation ground-reflection tables, always on
/// imperfect ground here (`iperf` is 1 only from the dead `dirgain`).
fn parmprec(ant: &mut CcirAntenna) {
    let iant = ant.iant;
    for u in 0..=90usize {
        let b0 = ant.b[u];
        let a0 = ant.a[u];

        if iant <= 4 || iant == 8 || iant == 9 {
            let pfra = ant.pfr * a0 * 2.0;
            let mut w1: R = 0.0;
            let mut w2: R = 0.0;
            if iant >= 4 {
                let pfrh = pfra * ant.h1;
                w1 = pfrh.cos();
                w2 = pfrh.sin();
            } else {
                let count = nint(ant.r3 - 1.0);
                for il in 0..=count {
                    let rik = il as R / 2.0;
                    let h11 = (ant.h1 + rik) * pfra;
                    w2 += h11.sin();
                    w1 += h11.cos();
                }
            }
            let (t1, t2, t3, t4) = refcof(a0, b0, ant.d1, ant.e1);
            let w31 = 1.0 - t1;
            let w32 = 1.0 + t1;
            let w33 = 1.0 + t3;
            let w34 = 1.0 - t3;
            ant.w3[u] = w31 * w1 - t2 * w2;
            ant.w4[u] = w32 * w2 - t2 * w1;
            ant.w5[u] = w33 * w1 + t4 * w2;
            ant.w6[u] = w34 * w2 + t4 * w1;
            ant.w9[u] = w32 * w1 + t2 * w2;
            ant.w10[u] = w31 * w2 + t2 * w1;
        } else if iant == 5 || iant == 6 {
            let mut delta = ant.beta * 2.0 * a0;
            if iant == 6 {
                delta /= 2.0;
            }
            for k in 1..=ant.nat {
                let i = ant.iek[k];
                let au = (delta * ant.h[i]).sin();
                let bu = (delta * ant.h[i]).cos();
                let (t1, t2, t3, t4) = refcof(a0, b0, ant.d1, ant.e1);
                if iant == 6 {
                    ant.ww1[u][i] = (1.0 + t1) * bu + t2 * au;
                    ant.ww2[u][i] = (1.0 - t1) * au + t2 * bu;
                } else {
                    ant.ww1[u][i] = 1.0 - t1 * bu - t2 * au;
                    ant.ww2[u][i] = t1 * au - t2 * bu;
                    ant.ww3[u][i] = 1.0 + t3 * bu + t4 * au;
                    ant.ww4[u][i] = t4 * bu - t3 * au;
                }
            }
        } else if iant == 7 {
            let (t1, t2, t3, t4) = refcof(a0, b0, ant.d1, ant.e1);
            ant.w3[u] = t1;
            ant.w4[u] = t2;
            ant.w5[u] = t3;
            ant.w6[u] = t4;
        }
        // iant 10 is unreachable: antinit2 returns before parmprec for
        // the monopole, so the Bessel/surface-impedance branch is dead.
    }
}

/// Fortran's `NINT`.
fn nint(v: R) -> i32 {
    if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    }
}

/// Fortran's `IFIX`: truncate toward zero.
fn ifix(v: R) -> usize {
    (v as i32).clamp(0, 90) as usize
}

impl CcirAntenna {
    /// `gainrel`: relative gain of type `iant` at elevation `u` and
    /// off-azimuth `v`, both degrees. Angles index whole-degree tables,
    /// so fractions truncate.
    pub fn gainrel(&self, u: R, v: R) -> R {
        let iant = self.iant;
        if iant == 0 {
            return 0.0;
        }
        let iu = ifix(u);
        let a0 = self.a[iu];
        let b0 = self.b[iu];
        // Fold the azimuth into the first quadrant of the table, with
        // the sign pattern of each quadrant.
        let (a1, b1) = if (0.0..=90.0).contains(&v) || (360.0..=450.0).contains(&v) {
            let kv = (v % 360.0) as i32;
            let jv = kv.clamp(0, 90) as usize;
            (self.a[jv], self.b[jv])
        } else if (v > 90.0 && v <= 180.0) || (v > 450.0 && v <= 540.0) {
            let kv = (v % 360.0) as i32;
            let jv = (180 - kv).clamp(0, 90) as usize;
            (self.a[jv], -self.b[jv])
        } else if v > 180.0 && v <= 270.0 {
            let jv = ((v - 180.0) as i32).clamp(0, 90) as usize;
            (-self.a[jv], -self.b[jv])
        } else {
            let jv = ((360.0 - v) as i32).clamp(0, 90) as usize;
            (-self.a[jv], self.b[jv])
        };
        let a11 = a1 * b0;
        let a12 = b1 * b0;

        match iant {
            1 => {
                // Multiband aperiodic reflector dipole array.
                let b2 = (self.p11 * a11).cos() - self.cp11;
                let w0 = (1.0 - a11 * a11).abs();
                let z1 = b2 / w0;
                let fx = self.fr1 * b0;
                let fy = 1.0 + 1.0 / (fx * fx);
                let q2 = 1.0 - 1.0 / fy.sqrt();
                let kv = (v % 360.0) as i32;
                let z2 = if !(90..=270).contains(&kv) {
                    (1.0 + q2 * q2 - 2.0 * q2 * (self.p11s * b0 * b1).cos())
                        .abs()
                        .sqrt()
                } else {
                    1.0 - q2
                };
                let pfb = self.pfr * b0;
                let asl = a1 - self.sl;
                let pfrb = pfb * asl;
                let mut u1: R = 0.0;
                let mut u2: R = 0.0;
                for il in 0..(self.r2 - 1.0) as i32 + 1 {
                    let u12 = pfrb * il as R;
                    u2 += u12.sin();
                    u1 += u12.cos();
                }
                let z3 = (u1 * u1 + u2 * u2).sqrt();
                let w7t = a1 * a0;
                let w7 = w7t * w7t * (self.w4[iu] * self.w4[iu] + self.w3[iu] * self.w3[iu]);
                let w8 = b1 * b1 * (self.w5[iu] * self.w5[iu] + self.w6[iu] * self.w6[iu]);
                let z4 = (w7 + w8).abs().sqrt();
                z1 * z2 * z3 * z4
            }
            2 => {
                // Dual-band centre-fed tuned reflector dipole array.
                let pfb = self.pfr * b0;
                let b2 = (self.p11 * a11).cos() - self.cp11;
                let w0 = (1.0 - a11 * a11).abs();
                let z1 = b2 / w0;
                let z2 = (1.0 + self.q2 * self.q2
                    + 2.0 * self.q2 * (self.p11 - self.p11 * b0 * b1).cos())
                .abs()
                .sqrt();
                let asl = a1 - self.sl;
                let pfrb = pfb * asl;
                let mut u1: R = 0.0;
                let mut u2: R = 0.0;
                for il in 0..(self.r2 - 1.0) as i32 + 1 {
                    let u12 = pfrb * il as R;
                    u2 += u12.sin();
                    u1 += u12.cos();
                }
                let z3 = (u1 * u1 + u2 * u2).sqrt();
                let w7t = a1 * a0;
                let w7 = w7t * w7t * (self.w4[iu] * self.w4[iu] + self.w3[iu] * self.w3[iu]);
                let w8 = b1 * b1 * (self.w5[iu] * self.w5[iu] + self.w6[iu] * self.w6[iu]);
                let z4 = (w7 + w8).abs().sqrt();
                z1 * z2 * z3 * z4
            }
            3 => {
                // Dual-band end-fed tuned reflector dipole array.
                let b2 = (self.pfr * a11).cos() - self.cpfr;
                let w0 = (1.0 - a11 * a11).abs();
                let z1 = b2 / w0;
                let z2 = (1.0 + self.q2 * self.q2
                    + 2.0 * self.q2 * (self.p11 - self.p11 * b0 * b1).cos())
                .abs()
                .sqrt();
                let pfb = self.pfr * b0 * 2.0;
                let asl = a1 - self.sl;
                let pfrb = pfb * asl;
                let mut u1: R = 0.0;
                let mut u2: R = 0.0;
                for il in 0..(self.r2 / 2.0 - 1.0) as i32 + 1 {
                    let u12 = pfrb * il as R;
                    u2 += u12.sin();
                    u1 += u12.cos();
                }
                let z3 = (u1 * u1 + u2 * u2).sqrt();
                let w7t = a1 * a0;
                let w7 = w7t * w7t * (self.w4[iu] * self.w4[iu] + self.w3[iu] * self.w3[iu]);
                let w8 = b1 * b1 * (self.w5[iu] * self.w5[iu] + self.w6[iu] * self.w6[iu]);
                let z4 = (w7 + w8).abs().sqrt();
                z1 * z2 * z3 * z4
            }
            4 => {
                // Tropical antennas.
                let pfb = self.pfr * b0;
                let b2 = (self.p11 * a11).cos() - self.cp11;
                let w0 = (1.0 - a11 * a11).abs();
                let z1 = b2 / w0;
                let pfra = pfb * b1;
                let mut u3: R = 0.0;
                let mut u4: R = 0.0;
                for il in 0..(self.r3 - 1.0) as i32 + 1 {
                    let u13 = pfra * il as R;
                    u4 += u13.sin();
                    u3 += u13.cos();
                }
                let z2 = (u3 * u3 + u4 * u4).sqrt();
                let asl = a1 - self.sl;
                let pfrb = pfb * asl;
                let mut u1: R = 0.0;
                let mut u2: R = 0.0;
                for il in 0..(self.r2 - 1.0) as i32 + 1 {
                    let u12 = pfrb * il as R;
                    u2 += u12.sin();
                    u1 += u12.cos();
                }
                let z3 = (u1 * u1 + u2 * u2).sqrt();
                let w7t = a1 * a0;
                let w7 = w7t * w7t * (self.w4[iu] * self.w4[iu] + self.w3[iu] * self.w3[iu]);
                let w8 = b1 * b1 * (self.w5[iu] * self.w5[iu] + self.w6[iu] * self.w6[iu]);
                let z4 = (w7 + w8).abs().sqrt();
                z1 * z2 * z3 * z4
            }
            5 => {
                // Horizontal log-periodic.
                let b3 = -a12 * self.costh + a0 * self.sinth;
                let gamma = self.beta * b3 / self.costh;
                let df = 1.0 - b0 * b0 * a1 * a1;
                let mut str_: R = 0.0;
                let mut sti: R = 0.0;
                let mut sfr: R = 0.0;
                let mut sfi: R = 0.0;
                for k in 1..=self.nat {
                    let i = self.iek[k];
                    let fg = self.beta * self.rl[i];
                    let ft = ((fg * b0 * a1).cos() - fg.cos()) / df;
                    let aw = (gamma * self.x[i] + self.fi[i] * Q1).sin();
                    let bw = (gamma * self.x[i] + self.fi[i] * Q1).cos();
                    let wr = aw * self.ww1[iu][i] - bw * self.ww2[iu][i];
                    let wi = bw * self.ww1[iu][i] + aw * self.ww2[iu][i];
                    str_ += self.rim[i] * ft * wr;
                    sti += self.rim[i] * ft * wi;
                    let ur = bw * self.ww3[iu][i] - aw * self.ww4[iu][i];
                    let ui = aw * self.ww3[iu][i] + bw * self.ww4[iu][i];
                    sfr += self.rim[i] * ft * ur;
                    sfi += self.rim[i] * ft * ui;
                }
                let etheta = (str_ * str_ + sti * sti).sqrt() * a1 * a0;
                let efi = -(sfr * sfr + sfi * sfi).sqrt() * b1;
                (etheta * etheta + efi * efi).sqrt()
            }
            6 => {
                // Vertical log-periodic.
                let b3 = -a12 * self.costh + a0 * self.sinth;
                let gamma = self.beta * b3 / self.costh;
                let mut sfr: R = 0.0;
                let mut sfi: R = 0.0;
                for k in 1..=self.nat {
                    let i = self.iek[k];
                    let fg = self.beta * self.rl[i];
                    let ft = ((fg * a0).cos() - fg.cos()) / b0;
                    let aw = (gamma * self.y[i] + self.fi[i] * Q1).sin();
                    let bw = (gamma * self.y[i] + self.fi[i] * Q1).cos();
                    let ur = bw * self.ww1[iu][i] - aw * self.ww2[iu][i];
                    let ui = bw * self.ww2[iu][i] + aw * self.ww1[iu][i];
                    let fti = self.rim[i] * ft;
                    sfr += fti * ur;
                    sfi += fti * ui;
                }
                (sfr * sfr + sfi * sfi).sqrt()
            }
            7 => {
                // Horizontal rhombic.
                let a2 = self.sinth * b1 + self.costh * a1;
                let b2 = self.costh * b1 - self.sinth * a1;
                let a3 = self.sinth * b1 - self.costh * a1;
                let b3 = self.costh * b1 + self.sinth * a1;
                let t5 = a2 * b0 - 1.0;
                let t6 = a3 * b0 - 1.0;
                let a4 = a2 / t5;
                let b4 = b2 / t5;
                let a5 = a3 / t6;
                let b5 = b3 / t6;
                let a6 = a4 - a5;
                let b6 = b4 + b5;
                let t7 = t5 + t6;
                let c2 = (self.sl * t5).sin();
                let d2 = (self.sl * t5).cos();
                let c3 = (self.sl * t6).sin();
                let d3 = (self.sl * t6).cos();
                let c4 = (self.sl * t7).sin();
                let d4 = (self.sl * t7).cos();
                let r1 = 1.0 - d3 - d2 + d4;
                let r2 = c4 - c3 - c2;
                let a7 = (self.h1 * a0).sin();
                let b7 = (self.h1 * a0).cos();
                let r3 = 1.0 + self.w5[iu] * b7 + self.w6[iu] * a7;
                let r4 = self.w5[iu] * a7 - self.w6[iu] * b7;
                let h4 = r1 * r3 - r2 * r4;
                let h5 = r1 * r4 + r2 * r3;
                let r5 = 1.0 - self.w3[iu] * b7 - self.w4[iu] * a7;
                let r6 = self.w3[iu] * a7 - self.w4[iu] * b7;
                let w1 = r1 * r5 - r2 * r6;
                let w2 = r1 * r6 + r2 * r5;
                let h6 = 30.0 * b6 * (h4 * h4 + h5 * h5).sqrt();
                let w3x = 30.0 * a0 * a6 * (w1 * w1 + w2 * w2).sqrt();
                (h6 * h6 + w3x * w3x).sqrt()
            }
            8 | 9 => {
                // Quadrant and cross-dipole antennas.
                let aa1 = 0.707 * (b1 - a1);
                let bb1 = 0.707 * (b1 + a1);
                let a11 = aa1 * b0;
                let a12 = bb1 * b0;
                let b2 = (self.p11 * a11).cos() - self.cp11;
                let w0 = (1.0 - a11 * a11).abs();
                let z1 = b2 / w0;
                let b3 = (self.p11 * a12).cos() - self.cp11;
                let b4a = self.p11 * (bb1 - aa1) * b0;
                let (b5, b4) = if iant == 8 {
                    (b4a.sin(), b4a.cos())
                } else {
                    (0.0, 1.0)
                };
                let w00 = 1.0 - a12 * a12;
                let z2 = b3 / w00;
                let mut w7 = aa1 * a0 * b0 * z1;
                let w71y = w7 * self.w9[iu];
                let w72y = w7 * self.w10[iu];
                w7 = -bb1 * a0 * b0 * z2;
                let w711x = w7 * self.w9[iu];
                let w721x = w7 * self.w10[iu];
                let w71x = w711x * b4 - w721x * b5;
                let w72x = w711x * b5 + w721x * b4;
                w7 = -bb1 * a0 * a0 * z2;
                let w31x = w7 * self.w3[iu];
                let w41x = w7 * self.w4[iu];
                let w3x = w31x * b4 - w41x * b5;
                let w4x = w31x * b5 + w41x * b4;
                w7 = aa1 * a0 * a0 * z1;
                let w3y = w7 * self.w3[iu];
                let w4y = w7 * self.w4[iu];
                let w7s = (w3x + w3y) * (w3x + w3y)
                    + (w4x + w4y) * (w4x + w4y)
                    + (w71x + w71y) * (w71x + w71y)
                    + (w72x + w72y) * (w72x + w72y);
                let w81x = -aa1 * self.w5[iu] * z2;
                let w8y = -bb1 * self.w5[iu] * z1;
                let w91x = -aa1 * self.w6[iu] * z2;
                let w8x = w81x * b4 - w91x * b5;
                let w9x = w81x * b5 + w91x * b4;
                let w9y = -bb1 * self.w6[iu] * z1;
                let w8 = (w8x + w8y) * (w8x + w8y) + (w9x + w9y) * (w9x + w9y);
                (w7s + w8).sqrt()
            }
            _ => 0.0,
        }
    }

    /// `azmax`: refines the azimuth of maximum gain around
    /// `[vmin1, vmax1]` in steps of `vm`. Returns `(rmax, vmax)`.
    fn azmax(&self, u: R, vmin1: R, vmax1: R, vm: R, rmax0: R, vmax0: R) -> (R, R) {
        let mut rmax = rmax0;
        let mut vmax = vmax0;
        let mut j = 0i32;
        // Fortran's real DO loop: the count is fixed up front.
        let count = ((vmax1 - vmin1 + vm) / vm) as i32;
        let mut v = vmin1;
        for _ in 0..count {
            let z9 = self.gainrel(u, v);
            if z9 < rmax {
                j = 0;
            } else if z9 > rmax {
                rmax = z9;
                vmax = v;
            } else {
                // A plateau: place the maximum at its centre.
                j += 1;
                rmax = z9;
                if v != vmax {
                    vmax = v - j as R / 2.0 + 1.0;
                }
            }
            v += vm;
        }
        (rmax, vmax)
    }

    /// `gainorm`: finds the pattern's maximum (`z6`) and its direction.
    fn gainorm(&mut self) {
        let iant = self.iant;
        let ifs = self.ifs;
        if iant == 4 || (iant < 4 && ifs != 0) {
            // A slewed or tropical pattern: search the horizontal
            // plane first.
            let u: R = if iant == 4 { 45.0 } else { 5.0 };
            let mut rmax: R = 0.0;
            let mut vmax = self.vmax;
            let mut z9: R = 0.0;
            let mut v: R = 270.0;
            for _ in 0..19 {
                z9 = self.gainrel(u, v);
                if z9 > rmax {
                    rmax = z9;
                    vmax = v;
                }
                v += 10.0;
            }
            self.z6 = z9;
            let (_, refined) = self.azmax(u, vmax - 10.0, vmax + 10.0, 1.0, rmax, vmax);
            self.vmax = refined;
        } else {
            self.vmax = 0.0;
        }
        let v = self.vmax;
        let mut wmx: R = 0.0;
        for iu in 1..=90 {
            let u = iu as R;
            let z9 = self.gainrel(u, v);
            if z9 >= wmx {
                wmx = z9;
                self.umax = u;
            }
        }
        self.z6 = wmx;
        if ifs != 0 || iant == 4 {
            let u = self.umax;
            let (rmax, vmax) =
                self.azmax(u, self.vmax - 5.0, self.vmax + 5.0, 1.0, self.z6, self.vmax);
            self.z6 = rmax;
            self.vmax = vmax;
            if self.vmax >= 360.0 {
                self.vmax -= 360.0;
            }
        }
    }

    /// `ccirgain`: gain in dBi at elevation `u` and off-azimuth `v`,
    /// both degrees.
    pub fn ccirgain(&self, u: R, v: R, giso: R) -> R {
        if self.iant == 0 {
            return giso;
        }
        let z9 = self.gainrel(u, v);
        if self.z6 == 0.0 {
            return -FLOOR;
        }
        let dgs = z9 / self.z6;
        let mut g9 = if dgs <= 0.03162278 {
            -30.0
        } else {
            20.0 * dgs.log10()
        };
        if g9 < -FLOOR {
            g9 = -FLOOR;
        }
        let mut gain = g9 + giso;
        if gain < -FLOOR {
            gain = -FLOOR;
        }
        gain
    }
}

/// `antinit2` for types 1-9: extracts the file's parameters at one
/// operating frequency (`parm(5)`, already set by `setmaxgain`) and
/// builds every table `gainrel` needs.
pub fn antinit2(file: &AntennaFile, parm: &[R; 20]) -> CcirAntenna {
    let iant = file.jant();
    let (a, b) = trigfun();
    let mut ant = CcirAntenna {
        iant,
        d1: 0.0,
        e1: 0.0,
        beta: 0.0,
        cpfr: 0.0,
        cp11: 0.0,
        fr1: 0.0,
        h1: 0.0,
        pfr: 0.0,
        p11: 0.0,
        p11s: 0.0,
        q2: 0.0,
        r2: 0.0,
        r3: 0.0,
        sl: 0.0,
        costh: 0.0,
        sinth: 0.0,
        fi: [0.0; 36],
        rim: [0.0; 36],
        x: [0.0; 36],
        y: [0.0; 36],
        iek: [0; 36],
        rl: [0.0; 36],
        h: [0.0; 31],
        nat: 0,
        a,
        b,
        w3: [0.0; 91],
        w4: [0.0; 91],
        w5: [0.0; 91],
        w6: [0.0; 91],
        w9: [0.0; 91],
        w10: [0.0; 91],
        ww1: vec![[0.0; 31]; 91],
        ww2: vec![[0.0; 31]; 91],
        ww3: vec![[0.0; 31]; 91],
        ww4: vec![[0.0; 31]; 91],
        z6: 0.0,
        umax: 0.0,
        vmax: 0.0,
        ifs: 0,
    };

    ant.e1 = parm[2];
    let s1 = parm[3];
    let f1 = parm[4];
    ant.d1 = 18000.0 * s1 / f1;
    let rlambda = 299.8 / f1;
    ant.beta = 2.0 * P1 / rlambda;

    match iant {
        1 => {
            ant.r2 = parm[5];
            ant.r3 = parm[6];
            let fd = parm[7];
            ant.h1 = parm[8];
            ant.ifs = parm[9] as i32;
            let sd = parm[10];
            let ws = parm[11];
            let wd = parm[12];
            ant.umax = parm[13];
            ant.vmax = parm[14];
            ant.z6 = parm[16];
            let fr = f1 / fd;
            ant.pfr = P1 * fr;
            ant.p11 = ant.pfr / 2.0;
            ant.p11s = 8.0 * ant.p11 * sd;
            ant.cpfr = ant.pfr.cos();
            ant.cp11 = ant.p11.cos();
            ant.sl = (ant.ifs as R * Q1).sin();
            let fdx = ws / (wd * P1 * 0.001);
            let fdx1 = fdx.ln();
            let fdx2 = 0.048 * ws;
            ant.fr1 = fdx1 * fdx2 * fr;
        }
        2 | 3 => {
            ant.r2 = parm[5];
            ant.r3 = parm[6];
            let fd = parm[7];
            ant.h1 = parm[8];
            ant.ifs = parm[9] as i32;
            ant.q2 = parm[10];
            ant.umax = parm[11];
            ant.vmax = parm[12];
            let fr = f1 / fd;
            ant.pfr = P1 * fr;
            ant.p11 = ant.pfr / 2.0;
            ant.cpfr = ant.pfr.cos();
            ant.cp11 = ant.p11.cos();
            ant.sl = (ant.ifs as R * Q1).sin();
        }
        4 => {
            ant.r2 = parm[5];
            ant.r3 = parm[6];
            let fd = parm[7];
            ant.h1 = parm[8];
            ant.ifs = parm[9] as i32;
            ant.umax = parm[10];
            ant.vmax = parm[11];
            let fr = f1 / fd;
            ant.pfr = P1 * fr;
            ant.p11 = ant.pfr / 2.0;
            ant.cpfr = ant.pfr.cos();
            ant.cp11 = ant.p11.cos();
            ant.sl = (ant.ifs as R * Q1).sin();
        }
        5 | 6 => {
            let nel = parm[5] as usize;
            let rl1 = parm[6] / 2.0;
            let rlnel = parm[7] / 2.0;
            let dc = parm[8];
            let h1 = parm[9];
            let hnel = parm[10];
            let z0 = parm[11];
            logparm(&mut ant, iant, nel, rlnel, rl1, hnel, h1, dc, z0, rlambda);
            ant.ifs = 0;
        }
        7 => {
            ant.sl = parm[5] * ant.beta;
            ant.h1 = parm[6] * ant.beta * 2.0;
            let gamma = parm[7] * Q1;
            ant.sinth = gamma.sin();
            ant.costh = gamma.cos();
            ant.ifs = 0;
        }
        8 | 9 => {
            let fd = parm[5];
            ant.h1 = parm[6];
            ant.umax = parm[10];
            ant.vmax = parm[11];
            let fr = f1 / fd;
            ant.pfr = P1 * fr;
            ant.p11 = ant.pfr / 2.0;
            ant.cp11 = ant.p11.cos();
            ant.ifs = 0;
        }
        _ => {}
    }

    parmprec(&mut ant);
    ant.gainorm();
    ant
}

/// The NTIA curtain, type 12 (`curtain`, `pattrn0`, `f2`, `dbltrap`).
pub mod curtain {
    use super::super::con::R;

    const PI: R = 3.1415926;
    const VOFL: R = 299.79246;
    const PI2: R = 6.283185307;
    const PIO2: R = 1.570796326;
    const D2R: R = 0.01745329251;

    /// The slew-angle phase table, `IPHASE(14,8)`: one column per
    /// 4-degree slew step.
    const IPHASE: [[i32; 14]; 8] = [
        [0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0],
        [0, 0, 31, 31, 0, 77, 77, 109, 109, 0, 155, 155, 186, 186],
        [0, 47, 56, 103, 0, 139, 185, 195, 242, 0, 278, 324, 334, 381],
        [0, 47, 81, 128, 0, 200, 246, 281, 327, 0, 399, 446, 480, 527],
        [0, 47, 105, 152, 0, 260, 306, 365, 411, 0, 519, 566, 624, 671],
        [0, 47, 129, 176, 0, 318, 365, 447, 494, 0, 636, 683, 765, 812],
        [0, 90, 152, 242, 0, 375, 465, 527, 617, 0, 750, 840, 903, 993],
        [0, 90, 180, 270, 0, 444, 534, 624, 714, 0, 888, 978, 1068, 1158],
    ];

    /// The vertical excitation modes, `IVMA`: a sign per stack pair.
    const IVMA: [&str; 15] = [
        "+000", "0+00", "00+0", "++00", "+0+0", "0++0", "+-00", "+0-0", "0+-0", "+++0", "++-0",
        "+-+0", "+--0", "++++", "++--",
    ];

    /// The `/ANTDAT/` and `/FWAVE/` state `pattrn0` computes from.
    struct Curtain {
        nostak: usize,
        eil: R,
        ceil: R,
        xr: R,
        c: [R; 8],
        r: [R; 14],
        ps: [R; 14],
        y: [R; 14],
        z: [R; 8],
        nbs: usize,
    }

    fn nint(v: R) -> i32 {
        if v >= 0.0 {
            (v + 0.5) as i32
        } else {
            (v - 0.5) as i32
        }
    }

    /// `F2`: the curtain's unnormalised power pattern at one direction
    /// (angles in radians).
    fn f2(ant: &Curtain, theta: R, phi: R) -> R {
        let cphi = phi.cos();
        let sphi = phi.sin();
        let ctheta = theta.cos();
        let stheta = theta.sin();
        let cpsi = ctheta * sphi;
        let mut spsi2 = 1.0 - cpsi * cpsi;
        if spsi2 <= 1.0e-12 {
            spsi2 = 1.0e-12;
        }
        let ef = ((ant.eil * cpsi).cos() - ant.ceil) / spsi2;
        let mut fyr: R = 0.0;
        let mut fyi: R = 0.0;
        for i in 0..ant.nbs {
            let arg = ant.y[i] * cpsi + ant.ps[i];
            fyr += ant.r[i] * arg.cos();
            fyi += ant.r[i] * arg.sin();
        }
        let fx = (ant.xr * ctheta * cphi).sin();
        let mut fzphr: R = 0.0;
        for i in 0..ant.nostak {
            fzphr += ant.c[i] * (ant.z[i] * stheta).sin();
        }
        // COF = STHETA * SPHI is formed first, so the product
        // associates as fzphr times that, not left to right.
        let cof = stheta * sphi;
        let fzthr = fzphr * cof;
        let fzphr = fzphr * cphi;
        ef * ef * fx * fx * (fyr * fyr + fyi * fyi) * (fzthr * fzthr + fzphr * fzphr)
    }

    /// `DBLTRAP`: trapezoidal integration of the pattern over the
    /// forward hemisphere, for self-normalisation when the definition
    /// file carries no `GainNorm` table.
    fn dbltrap(ant: &Curtain) -> R {
        let phiz = -90.0 * D2R;
        let phif = 90.0 * D2R;
        let mut tint: R = 0.0;
        let mut t: R = 0.0;
        for ip1 in 1..=91 {
            let mut p = phiz + D2R;
            let mut pint: R = 0.0;
            for _ in 1..=179 {
                pint += f2(ant, t, p);
                p += D2R;
            }
            pint *= D2R;
            pint += (D2R / 2.0) * (f2(ant, t, phif) + f2(ant, t, phiz));
            if ip1 == 1 || ip1 == 91 {
                tint += D2R * pint * t.cos() / 2.0;
            } else {
                tint += D2R * pint * t.cos();
            }
            t += D2R;
        }
        tint
    }

    /// `Curtain` + `PATTRN0`: gain in dB at `azimd` degrees off
    /// boresight and `elevd` degrees elevation. `gnorm` is the
    /// normalising factor from the file's `GainNorm` table; -99999
    /// asks for self-normalisation by integration.
    pub fn gain(parm: &[R; 20], azimd: R, elevd: R, gnorm: R) -> R {
        let dfmhz = parm[7];
        let wave_design = VOFL / dfmhz;
        let to_metres = |v: R| if v < 0.0 { -v * wave_design } else { v };
        let nostak = nint(parm[6]).clamp(0, 8) as usize;
        let stkspm = to_metres(parm[11]);
        let numbay = nint(parm[5]);
        let bayspm = to_metres(parm[10]);
        let diplnm = to_metres(parm[8]);
        let rrspm = to_metres(parm[12]);
        let stkhtm = to_metres(parm[9]);
        let mode = nint(parm[13]).clamp(1, 15) as usize;

        let mut stkrat = [0.0 as R; 8];
        for (i, ch) in IVMA[mode - 1].chars().enumerate() {
            let k = 2 * i;
            let v = match ch {
                '+' => 1.0,
                '-' => -1.0,
                _ => 0.0,
            };
            stkrat[k] = v;
            stkrat[k + 1] = v;
        }
        for slot in stkrat.iter_mut().skip(nostak) {
            *slot = 0.0;
        }

        let islew = nint(parm[14]);
        let kslew = ((islew.abs() / 4) + 1).clamp(1, 8) as usize;
        let mut bayphs = [0.0 as R; 14];
        for i in 0..14 {
            bayphs[i] = if islew < 0 {
                IPHASE[kslew - 1][i] as R
            } else {
                -(IPHASE[kslew - 1][i]) as R
            };
        }

        let ofmhz = parm[4];
        let mut iaz = false;
        let mut azim = azimd * D2R;
        if azim.abs() > PIO2 && azim.abs() < 3.0 * PIO2 {
            azim -= PI.copysign(azim);
            iaz = true;
        }

        // PATTRN0.
        let wave = VOFL / ofmhz;
        let beta = PI2 / wave;
        let xh = stkhtm * beta;
        let xb = beta * bayspm;
        let el = diplnm * beta;
        let eil = el / 2.0;
        let ceil = eil.cos();
        let xr = rrspm * beta;
        let xs = stkspm * beta;

        let mut ant = Curtain {
            nostak,
            eil,
            ceil,
            xr,
            c: [0.0; 8],
            r: [0.0; 14],
            ps: [0.0; 14],
            y: [0.0; 14],
            z: [0.0; 8],
            nbs: numbay.unsigned_abs().min(14) as usize,
        };
        for (is, (z, c)) in ant.z.iter_mut().zip(ant.c.iter_mut()).enumerate().take(nostak) {
            *z = xs * is as R + xh;
            *c = stkrat[is];
        }
        let odrat = D2R * ofmhz / dfmhz;
        for (ib, ((ps, r), y)) in ant
            .ps
            .iter_mut()
            .zip(ant.r.iter_mut())
            .zip(ant.y.iter_mut())
            .enumerate()
            .take(ant.nbs)
        {
            *ps = odrat * bayphs[ib];
            *r = 1.0;
            *y = ib as R * xb;
        }
        ant.y[0] = 0.0;

        let factor = if gnorm == -99999.0 {
            4.0 * PI / dbltrap(&ant)
        } else {
            gnorm
        };
        let mut xgn: R = -1000.0;
        let xgain = f2(&ant, elevd * D2R, azim);
        if xgain != 0.0 {
            xgn = 10.0 * (xgain * factor).abs().log10();
        }
        if iaz {
            xgn -= 20.0; // Backward radiation.
        }
        xgn
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trig_tables_have_their_doctored_ends() {
        let (a, b) = trigfun();
        assert!((a[45] - (45.0 * Q1).sin()).abs() < 1e-7);
        // b(0) is cos(0.005 radians), not cos(0 degrees).
        assert!(b[0] < 1.0 && b[0] > 0.99998, "b[0] = {}", b[0]);
        // b(90) is cos(90.105 degrees): slightly negative.
        assert!(b[90] < 0.0, "b[90] = {}", b[90]);
    }

    #[test]
    fn refcof_is_bounded_reflection() {
        // Coefficients of a passive ground have magnitude at most 1.
        let (t1, t2, t3, t4) = refcof((30.0 * Q1).sin(), (30.0 * Q1).cos(), 4.5, 15.0);
        assert!((t1 * t1 + t2 * t2).sqrt() <= 1.0 + 1e-6);
        assert!((t3 * t3 + t4 * t4).sqrt() <= 1.0 + 1e-6);
    }

    #[test]
    fn parabol_passes_through_its_points() {
        let (a, b, c) = parabol(1.0, 2.0, 3.0, 1.0, 4.0, 9.0);
        for (x, y) in [(1.0, 1.0), (2.0, 4.0), (3.0, 9.0)] {
            let f: R = a * x * x + b * x + c;
            assert!((f - y).abs() < 1e-4, "at {x}: {f} vs {y}");
        }
    }

    #[test]
    fn gainterp_answers_minus_thirty_outside_the_ratio_band() {
        let gm = [[19.72, 22.13, 23.86], [19.39, 21.89, 23.69]];
        assert_eq!(gainterp(&gm, 10.0, 20.0, 1), -30.0);
        // At ratio 1 the answer interpolates the middle column.
        let g = gainterp(&gm, 2.0, 2.0, 1);
        assert!((g - 22.13).abs() < 1e-4, "{g}");
    }
}
