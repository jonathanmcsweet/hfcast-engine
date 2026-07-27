//! The IONCAP antenna models: `ioninit`, `iongain`, `cisi`
//! (`vendor/voacapl/src/wp10dwin`), antenna types 21-30.
//!
//! These are the closed-form patterns IONCAP carried: rhombics, the
//! vertical monopole, dipole and Yagi, the log-periodic, curtain,
//! sloping vee and inverted L. Each is a Fresnel ground-reflection
//! calculation over the conductivity and dielectric constant the
//! definition file names, so unlike the table types the ground under
//! the antenna is part of the pattern.
//!
//! `kop` here is the IONCAP index 1-10 (`jant - 20`).

// The literals are the source's own digits: 1.5708 and 1.570796 are
// what the Fortran wrote, not shortened pi constants, and swapping in
// the exact constant would change the arithmetic.
#![allow(
    clippy::approx_constant,
    clippy::excessive_precision,
    clippy::inconsistent_digit_grouping
)]

use super::con::{PI, PIO2, R};
use super::model::Model;

/// "RA" for a vertical monopole at h/lambda = 0.2.
const RINTW: R = 18.06;

/// The minimum antenna gain the model reports, dB.
const FLOOR: R = -30.0;

/// What `ioninit` extracts from the definition file's parameters for
/// one antenna. The meaning of each field depends on `kop`.
#[derive(Debug, Clone, Copy)]
pub struct IoncapParams {
    /// Ground conductivity, mhos/metre (`parm(4)`).
    pub asig: R,
    /// Ground relative dielectric constant (`parm(3)`).
    pub aeps: R,
    /// An angle: rhombic tilt or half-apex, degrees.
    pub and: R,
    /// A length: leg length, element length or height, metres
    /// (negative means wavelengths).
    pub anl: R,
    /// A height, metres (negative means wavelengths) — except the
    /// monopole, where it is the gain above a dipole.
    pub anh: R,
    /// Extra values per type: terminated height, curtain geometry,
    /// the dipole's gain offset.
    pub aex: [R; 4],
}

/// The `SAVE` statement's residue. `ionGAIN2`'s locals are static, and
/// one of them is read before it is written: the zero-elevation exit
/// jumps straight to the efficiency dispatch, whose monopole and
/// log-periodic arm tests `X` — the height in wavelengths left over
/// from the previous call (zero at program start). Every elevation-0
/// table row for those types therefore depends on the call before it,
/// and the caller must keep this state alive for as long as the
/// Fortran program would (one `ANTCALC` run spans every card).
#[derive(Debug, Clone, Copy, Default)]
pub struct IoncapState {
    x: R,
}

/// `ioninit`: maps the definition file's `parm` slots onto the pattern
/// arguments for IONCAP antenna `index` (1-10).
pub fn ioninit(index: i32, parm: &[R; 20]) -> IoncapParams {
    let mut p = IoncapParams {
        asig: parm[3],
        aeps: parm[2],
        and: 0.0,
        anl: 0.0,
        anh: 0.0,
        aex: [0.0; 4],
    };
    match index {
        // Terminated horizontal rhombic.
        1 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
        }
        // Vertical monopole: `anh` is gain above a dipole, not height.
        2 => {
            p.anl = parm[5];
            p.anh = parm[6];
        }
        // Horizontal dipole and Yagi; `aex(1)` is gain above a
        // half-wavelength dipole.
        3 | 4 => {
            p.anl = parm[5];
            p.anh = parm[6];
            p.aex[0] = parm[7];
        }
        // Vertical log-periodic: the height must be a quarter
        // wavelength, so the source hard-codes it.
        5 => {
            p.anl = -0.25;
            p.aex[0] = parm[5];
        }
        // Curtain.
        6 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
            p.aex[0] = parm[8];
            p.aex[1] = parm[9];
            p.aex[2] = parm[10];
            p.aex[3] = parm[11];
        }
        // Sloping vee.
        7 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
            p.aex[0] = parm[8];
        }
        // Inverted L.
        8 => {
            p.anl = parm[5];
            p.anh = parm[6];
        }
        // Sloping rhombic.
        9 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
            p.aex[0] = parm[8];
        }
        // Interlaced rhombic.
        10 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
            p.aex[0] = parm[8];
            p.aex[1] = parm[9];
        }
        _ => {}
    }
    p
}

/// `CISI`: the cosine and sine integrals of `x`, returned as
/// `(ci, si)`. Series below 1, a rational approximation above.
pub fn cisi(x: R) -> (R, R) {
    if x < 1.0 {
        let sq = x * x;
        let mut ci = 0.577_215_664_9 + x.ln();
        let mut term = -sq / 4.0;
        let mut g: R = 4.0;
        loop {
            ci += term;
            term = -term * sq * (g - 2.0) / ((g - 1.0) * g * g);
            g += 2.0;
            if term.abs() <= 0.00005 {
                break;
            }
        }
        let mut si = 0.0;
        let mut term = x;
        let mut g: R = 3.0;
        loop {
            si += term;
            term = -term * sq * (g - 2.0) / ((g - 1.0) * g * g);
            g += 2.0;
            if term.abs() <= 0.00005 {
                break;
            }
        }
        (ci, si)
    } else {
        let x2 = x * x;
        let t = x.cos();
        let s = x.sin();
        let mut g = ((((x2 + 48.196_927) * x2 + 482.485_984) * x2 + 1114.978_885) * x2
            + 449.690_326)
            * x2;
        g = ((((x2 + 42.252_855) * x2 + 302.757_865) * x2 + 352.018_498) * x2 + 21.821_899) / g;
        let mut f = ((((x2 + 40.021_433) * x2 + 322.624_911) * x2 + 570.236_28) * x2
            + 157.105_423)
            * x;
        f = ((((x2 + 38.027_264) * x2 + 265.187_033) * x2 + 335.677_32) * x2 + 38.102_495) / f;
        let si = 1.5708 - f * t - g * s;
        let ci = f * s - g * t;
        (ci, si)
    }
}

/// `ionGAIN`: gain and efficiency for IONCAP antenna `kop` at
/// off-azimuth `toaz` (degrees), elevation `delta` (radians) and
/// `fmc` MHz. Returns `(rain, eff)` in dB.
///
/// For the dipole and Yagi (`kop` 3 and 4) the pattern is blended
/// toward its vertical value by `sin(delta)^4`, so straight up reads
/// the same whatever the azimuth.
pub fn iongain(
    state: &mut IoncapState,
    kop: i32,
    toaz: R,
    p: &IoncapParams,
    delta: R,
    fmc: R,
    model: Model,
) -> (R, R) {
    let (mut rain, eff) = iongain2(state, kop, toaz, p, delta, fmc, model);
    if kop == 3 || kop == 4 {
        let (rain_90, _eff_90) = iongain2(state, kop, 0.0, p, 1.570_796, fmc, model);
        let gmorph = delta.sin().powi(4);
        rain = rain_90 * gmorph + rain * (1.0 - gmorph);
    }
    (rain, eff)
}

/// The tail of `ionGAIN2`: labels 385 (into dB), 610 (efficiency by
/// type) and 405 (add efficiency and offset, then the floor clamp,
/// which the monopole skips).
fn finish(kop: i32, mut rain: R, x: R, sok: R, eff8: R, from_385: bool) -> (R, R) {
    if from_385 {
        rain = rain.max(0.00001);
        rain = 10.0 * rain.log10();
    }
    let eff = match kop {
        1 | 7 | 9 | 10 => -1.7,
        2 | 5 => {
            if x >= 0.35 {
                0.0
            } else {
                -((((6416.702_573 * x - 6091.332_95) * x + 2179.890_548) * x - 364.817_380_3)
                    * x
                    + 25.646_201_46)
            }
        }
        // The inverted L computed its efficiency inline; label 630
        // keeps it.
        8 => eff8,
        _ => 0.0,
    };
    rain += eff + sok;
    if kop != 2 && rain <= FLOOR {
        rain = FLOOR + eff;
    }
    (rain, eff)
}

/// `ionGAIN2`, transcribed in source order. The reflection-coefficient
/// equations are IONCAP report volume 1 page 115.
// The curtain branch stores SOK twice as the source does (labels 272
// and just before 610); the first is a dead store kept for fidelity.
#[allow(unused_assignments)]
fn iongain2(
    state: &mut IoncapState,
    kop: i32,
    toaz: R,
    p: &IoncapParams,
    delta: R,
    fmc: R,
    model: Model,
) -> (R, R) {
    let cot = |x: R| 1.0 / x.tan();

    let mut sok: R;
    let mut rain: R = 0.0;

    let sigma = p.asig;
    let er = p.aeps;
    let beta = toaz;
    let phi = p.and;
    let mut el = p.anl;
    let mut h = p.anh;
    let ex = p.aex;
    // The source STOPs on a non-conducting ground; a definition file
    // that omits the conductivity cannot be computed.
    assert!(sigma > 0.0, "In GAIN, SIGMA<=0.");

    if delta <= 0.0 {
        // Elevation angle zero: force the floor. This path reaches the
        // efficiency dispatch without setting X, so the monopole and
        // log-periodic read the previous call's value — the stale-X
        // quirk `IoncapState` exists for.
        return finish(kop, FLOOR, state.x, 0.0, 0.0, false);
    }
    let relta = delta;
    let mut x = 18000.0 * sigma / fmc;
    let t = relta.cos();
    let q = relta.sin();
    let r = q * q;
    let s = r * r;
    let ert = er - t * t;
    let rho = (ert * ert + x * x).sqrt();
    let rho12 = rho.sqrt();
    let alpha = -(x / ert).atan();
    let u = er * er + x * x;
    let v = u.sqrt();
    let asxv = (x / v).asin();
    let mut cv = (rho * rho + u * u * s - 2.0 * rho * u * r * (alpha + 2.0 * asxv).cos()).sqrt()
        / (rho + u * r + 2.0 * rho12 * v * q * (alpha * 0.5 + asxv).cos());
    let a = 2.0 * rho12 * q * v * (alpha * 0.5 + asxv).sin();
    let wave = 299.7925 / fmc;
    let b = rho - u * r;
    let mut psiv = if b < 0.0 {
        (a / b).atan() + 3.141_593
    } else if b == 0.0 {
        if a < 0.0 {
            -1.570_796
        } else if a == 0.0 {
            0.0
        } else {
            1.570_796
        }
    } else {
        (a / b).atan()
    };
    // Label 145: every type starts from EX(1); most reset it below.
    sok = ex[0];
    let reta = beta * 0.017_453_293;
    let mut psih = (2.0 * rho12 * q * (alpha * 0.5).sin() / (rho - r)).atan();
    let sb = reta.sin();
    let cb = reta.cos();
    x = 1.0;
    let mut ch = (rho * rho + s - 2.0 * rho * r * alpha.cos()).sqrt()
        / (rho + r + 2.0 * rho12 * q * (alpha * 0.5).cos());
    if (cv - ch).abs() <= 0.001 {
        cv = ch;
    }
    let (fac, el1) = if el < 0.0 {
        let el1 = el.abs();
        (3.1416 * el1, el1)
    } else {
        (3.1416 * el / wave, el / wave)
    };
    let (mut hwave, mut h1) = if h > 0.0 {
        (6.2832 * h / wave, h / wave)
    } else {
        let h1 = h.abs();
        (6.2832 * h1, h1)
    };
    let rhi = phi * 0.017_453_293;

    let mut eff8: R = 0.0;
    let mut skip_385 = false;

    match kop {
        // Terminated rhombic, KOP=1.
        1 => {
            let u1 = 1.0 - t * (rhi + reta).sin();
            let u2 = 1.0 - t * (rhi - reta).sin();
            rain = 3.2
                * (rhi.cos() * (fac * u1).sin() * (fac * u2).sin() / (u1 * u2)).powi(2)
                * ((cb - rhi.sin() * t).powi(2)
                    * (ch * ch + 1.0 - 2.0 * ch * (psih - 2.0 * hwave * q).cos())
                    + sb * sb
                        * (cv * cv + 1.0 - 2.0 * cv * (psiv - 2.0 * hwave * q).cos())
                        * r);
        }
        // Terminated interlaced rhombic, KOP=10.
        10 => {
            let ht = ex[0];
            sok = 0.0;
            let ss = ex[1];
            let e = rhi.sin();
            let f = rhi.cos();
            let d = (ss * ss + ht * ht).sqrt();
            let sg = ht / d;
            let cg = ss / d;
            let htwave = 6.283_185_308 * ht / wave;
            let yh = psih - 2.0 * hwave * q;
            let yv = psiv - 2.0 * hwave * q;
            let u1 = 1.0 - t * (reta + rhi).sin();
            let u2 = 1.0 - t * (rhi - reta).sin();
            let elfac = f * (fac * u1).sin() * (fac * u2).sin() / (u1 * u2);
            let elfac2 = elfac * elfac;
            let y = 6.283_185_308 / wave * (ss - d * (q * sg + t * cg * cb));
            let z = y - 2.0 * htwave * q;
            let h1v = 1.0 + y.cos() - ch * (yh.cos() * (1.0 + z.cos()) - yh.sin() * z.sin());
            let h2 = -y.sin() - ch * (-yh.sin() * (1.0 + z.cos()) - yh.cos() * z.sin());
            let hk = h1v * h1v + h2 * h2;
            let brk = cb - e * t;
            let hrain = elfac2 * brk * brk * hk;
            let v1 = 1.0 + y.cos() - cv * (yv.cos() * (1.0 + z.cos()) - yv.sin() * z.sin());
            let v2 = -y.sin() - cv * (-yv.sin() * (1.0 + z.cos()) - yv.cos() * z.sin());
            let vk = v1 * v1 + v2 * v2;
            let brv = sb * sb * r;
            let vrain = elfac2 * brv * vk;
            rain = (hrain + vrain) * 0.8;
        }
        // Inverted L, KOP=8.
        8 => {
            let fac2 = fac * 2.0;
            let twave = hwave * 0.15916;
            eff8 = if twave < 0.20 {
                let e = twave * (6.335 + twave * (67.95 - twave * (693.0 - 1600.0 * twave)));
                20.0 * e.log10()
            } else {
                0.0
            };
            let hac2 = hwave + hwave;
            let hac4 = hac2 + hac2;
            let (w5, w6) = cisi(hac2);
            let (w7, w8) = cisi(hac4);
            let cin2 = 0.577_215 + hac2.ln() - w5;
            let cin4 = 0.577_215 + hac4.ln() - w7;
            let ra = 30.0
                * (-0.5 * hac2.cos() * cin4
                    + (1.0 + hac2.cos()) * cin2
                    + hac2.sin() * (0.5 * w8 - w6));
            let u = t * sb;
            let hk = 1.0 + ch * ch - 2.0 * ch * (psih - 2.0 * hwave * q).cos();
            let c = 1.0
                - (fac2 * u).cos() * fac2.cos()
                - u * (fac2 * u).sin()
                - 0.5 * fac2.sin() * fac2.sin() * (1.0 - u * u);
            let hq = q * hwave;
            let chq = hq.cos();
            let shq = hq.sin();
            let cfac2 = fac2.cos();
            let sfac2 = fac2.sin();
            let a = cfac2 * chq - q * sfac2 * shq - (hwave + fac2).cos();
            let b = q * sfac2 * chq + cfac2 * shq - q * (hwave + fac2).sin();
            let bprim = if a != 0.0 { (b / a).atan() } else { 0.0 };
            let sphip = 1.0 - u * u;
            let g2 = if sphip != 0.0 {
                hk * (2.0 * c * cb / sphip).powi(2)
            } else {
                0.0
            };
            let f1 = if t != 0.0 {
                (a * a + b * b) * (1.0 - 2.0 * cv * (psiv - 2.0 * bprim).cos() + cv * cv)
                    / (t * t)
            } else {
                0.0
            };
            rain = 30.0 * (g2 + f1) / ra;
            x = 1.0;
        }
        // Terminated sloping vee (7) and sloping rhombic (9).
        7 | 9 => {
            let mut ht = ex[0];
            sok = 0.0;
            if ht < 0.0 {
                ht = ht.abs();
            } else {
                ht /= wave;
            }
            let mut g = (ht - h1) / el1;
            if kop == 9 {
                g *= 0.5;
            }
            let pp = (1.0 - g * g).sqrt();
            let e = rhi.sin();
            let rhi = (e / pp).asin();
            let sisum = (rhi + reta).sin();
            let sidif = (reta - rhi).sin();
            let cosum = (rhi + reta).cos();
            let codif = (rhi - reta).cos();
            let copsi1 = q * g + t * pp * codif;
            let copsi2 = q * g + t * pp * cosum;
            let copsi3 = -q * g + t * pp * codif;
            let copsi4 = -q * g + t * pp * cosum;
            let copsi5 = t * g + q * pp * codif;
            let copsi6 = t * g + q * pp * cosum;
            let copsi7 = -t * g + q * pp * codif;
            let copsi8 = -t * g + q * pp * cosum;
            let u1 = 1.0 - copsi1;
            let u2 = 1.0 - copsi2;
            let u3 = 1.0 - copsi3;
            let u4 = 1.0 - copsi4;
            let mut w1 = (psih - 2.0 * hwave * q).cos();
            let mut w2 = (psih - 2.0 * hwave * q).sin();
            let mut w3 = (psiv - 2.0 * hwave * q).cos();
            let mut w4 = (psiv - 2.0 * hwave * q).sin();
            let fac2 = fac * 2.0;
            if kop == 7 {
                let y1 = u1 * sisum;
                let y2 = u2 * sidif;
                let y3 = u3 * sisum;
                let y4 = u4 * sidif;
                let z1 = (fac2 * u1).cos() - 1.0;
                let z2 = (fac2 * u2).cos() - 1.0;
                let z3 = (fac2 * u3).cos() - 1.0;
                let z4 = (fac2 * u4).cos() - 1.0;
                let v1 = (fac2 * u1).sin();
                let v2 = (fac2 * u2).sin();
                let v3 = (fac2 * u3).sin();
                let v4 = (fac2 * u4).sin();
                let uc27 = u2 * copsi7;
                let uc18 = u1 * copsi8;
                let uc45 = u4 * copsi5;
                let uc36 = u3 * copsi6;
                rain = 0.025
                    * (pp * pp
                        * (((y2 * z1 - y1 * z2) / (u1 * u2)
                            - ch / (u3 * u4)
                                * (y4 * (w1 * z3 + w2 * v3) - y3 * (w1 * z4 + w2 * v4)))
                        .powi(2)
                            + ((-y2 * v1 + y1 * v2) / (u1 * u2)
                                - ch / (u3 * u4)
                                    * (y4 * (w2 * z3 - w1 * v3) - y3 * (w2 * z4 - w1 * v4)))
                            .powi(2))
                        + ((z1 * uc27 - z2 * uc18) / (u1 * u2)
                            - cv / (u3 * u4)
                                * (w3 * (z3 * uc45 - z4 * uc36) + w4 * (v3 * uc45 - v4 * uc36)))
                        .powi(2)
                        + ((-v1 * uc27 + v2 * uc18) / (u1 * u2)
                            - cv / (u3 * u4)
                                * (w3 * (-v3 * uc45 + v4 * uc36)
                                    + w4 * (z3 * uc45 - z4 * uc36)))
                        .powi(2));
                rain *= 2.0;
                x = 1.0;
            } else {
                // Finish the sloping rhombic, label 235.
                let a6 = 1.0 + (fac2 * (u1 + u2)).cos() - (fac2 * u1).cos() - (fac2 * u2).cos();
                let b6 = -(fac2 * (u1 + u2)).sin() + (fac2 * u1).sin() + (fac2 * u2).sin();
                let a7 = 1.0 + (fac2 * (u3 + u4)).cos() - (fac2 * u3).cos() - (fac2 * u4).cos();
                let b7 = -(fac2 * (u3 + u4)).sin() + (fac2 * u3).sin() + (fac2 * u4).sin();
                let x7 = copsi8 / u2 - copsi7 / u1;
                let y7 = copsi5 / u3 - copsi6 / u4;
                let x8 = sidif / u1 - sisum / u2;
                let y8 = sidif / u3 - sisum / u4;
                // "Switch to Ma's use" of the reflection phases.
                psiv -= PI;
                psih += PI;
                w1 = (psih - 2.0 * hwave * q).cos();
                w2 = (psih - 2.0 * hwave * q).sin();
                w3 = (psiv - 2.0 * hwave * q).cos();
                w4 = (psiv - 2.0 * hwave * q).sin();
                let f7 = x7 * a6 + y7 * cv * (a7 * w3 - b7 * w4);
                let g7 = x7 * b6 + y7 * cv * (a7 * w4 + b7 * w3);
                let f8 = x8 * a6 + y8 * ch * (a7 * w1 - b7 * w2);
                let g8 = x8 * b6 + y8 * ch * (a7 * w2 + b7 * w1);
                rain = 0.05 * (f7 * f7 + g7 * g7 + pp * pp * (f8 * f8 + g8 * g8));
            }
        }
        // Curtain, KOP=6.
        6 => {
            // The source compares against the integer literal 0001 —
            // one radian, not the intended .0001 — so every elevation
            // within a radian of vertical (above about 33 degrees)
            // takes the floor. And on that path SOK is still EX(1),
            // the elements per bay, so the answer is the floor plus
            // that count.
            //
            // The fix is the threshold alone. What it leaves behind is
            // the guard against a division by zero at exactly vertical,
            // which is what the decimal point was there for.
            let near_vertical = if model.curtain_elevation_threshold() {
                0.0001
            } else {
                1.0
            };
            if (delta - PIO2).abs() > near_vertical {
                sok = 0.0;
                let dy = if ex[1] <= 0.0 { ex[1].abs() } else { ex[1] / wave };
                let dz = if ex[2] <= 0.0 { ex[2].abs() } else { ex[2] / wave };
                let dx = if ex[3] <= 0.0 { ex[3].abs() } else { ex[3] / wave };
                let chi = ch;
                let psihi = psih;
                ch = -chi * psihi.cos();
                psih = -chi * psihi.sin();
                let (ep, nep) = if phi % 2.0 != 0.0 { (1.0, 1) } else { (0.0, 0) };
                let nn = ((phi - ep) / 2.0) as i32;
                let m = ex[0] as i32;
                let cvi = cv;
                let psivi = psiv;
                cv = -cvi * psivi.cos();
                psiv = -cvi * psivi.sin();
                let fac1 = fac;
                let fac3 = PI * dy;
                let denom = 1.0 - sb * sb * t * t;
                let sel = (fac1 * sb * t).cos() - fac1.cos();
                let sv = sb * q;
                let sz = (2.0 * PI * dx * cb * t).sin();
                let mut sx = ep;
                let fac4 = t * sb;
                for i in 1..=nn {
                    let fn_ = (2 * i - 1 + nep) as R;
                    sx += 2.0 * (fn_ * fac3 * fac4).cos();
                }
                let fac6 = (sel * sx * sz) / denom;
                let mut hreal = 0.0;
                let mut himg = 0.0;
                let mut vreal = 0.0;
                let mut vimg = 0.0;
                // CA and SA are DIMENSION(10) in the source; more than
                // ten elements per bay would write past them.
                let mut ca = [0.0 as R; 10];
                let mut sa = [0.0 as R; 10];
                for (j, (c, s)) in ca.iter_mut().zip(sa.iter_mut()).enumerate().take(m as usize)
                {
                    let fm = j as R;
                    let arg = 2.0 * PI * q * (h1 + fm * dz);
                    *c = arg.cos();
                    *s = arg.sin();
                }
                for j in 0..(m as usize).min(10) {
                    hreal += ca[j] * (1.0 + ch) + psih * sa[j];
                    himg += sa[j] * (1.0 - ch) + psih * ca[j];
                }
                let eh = fac6 * cb * (hreal * hreal + himg * himg).sqrt();
                for j in 0..(m as usize).min(10) {
                    vreal += ca[j] * (1.0 + cv) + psiv * sa[j];
                    vimg += sa[j] * (1.0 - cv) + psiv * ca[j];
                }
                let ev = fac6 * sv * (vreal * vreal + vimg * vimg).sqrt();
                let value = (eh * eh + ev * ev).sqrt();
                if value <= 0.00001 {
                    state.x = x;
                    return finish(kop, FLOOR, x, 0.0, 0.0, false);
                }
                rain = 20.0 * value.log10();
                sok = 0.0;
                skip_385 = true;
            } else {
                state.x = x;
                return finish(kop, FLOOR, x, sok, 0.0, false);
            }
        }
        // Horizontal dipole and Yagi, KOP=3 and 4 (the 1994 rewrite;
        // the original expression is the commented block in the
        // source).
        3 | 4 => {
            let (w, w1) = cisi(fac * 4.0);
            let (w2, w3) = cisi(fac * 2.0);
            let g = cot(fac);
            let ss = sb * sb;
            let c = t * t;
            let ci_kl = w2;
            let si_kl = w3;
            let ci_2kl = w;
            let si_2kl = w1;
            let cin_kl = 0.577 + (2.0 * fac).ln() - ci_kl;
            let cin_2kl = 0.577 + (4.0 * fac).ln() - ci_2kl;
            let sin2kl: R = 1.0;
            let xr = 30.0
                * ((1.0 - g * g) * cin_2kl
                    + 4.0 * g * g * cin_kl
                    + 2.0 * g * (si_2kl - 2.0 * si_kl))
                / sin2kl;
            let xkv = cv * cv + 1.0 - 2.0 * cv * (psiv - 2.0 * hwave * q).cos();
            let xkh = ch * ch + 1.0 - 2.0 * ch * (psih - 2.0 * hwave * q).cos();
            let xt1 = (fac * sb * t).cos() - fac.cos();
            let xb1 = 1.0 - ss * c;
            let xt2 = (xkv * ss * r + xkh * cb * cb) / sin2kl;
            rain = 120.0 / xr * (xt1 / xb1).powi(2) * xt2;
        }
        // Vertical log-periodic (5) and vertical monopole (2).
        2 | 5 => {
            sok = if kop == 5 { ex[0] } else { h };
            // Label 351: the monopole's height arrives in EL, so the
            // two are swapped before the common code below.
            std::mem::swap(&mut h, &mut el);
            let _ = el;
            let (a, xv) = if h > 0.0 {
                (6.283_185 * h / wave, h / wave)
            } else {
                h1 = h.abs();
                (h1 * 6.283_185, h1)
            };
            x = xv;
            let d = 2.0 * a;
            let z = 2.0 * d;
            let (w, w1) = cisi(z);
            let (w2, w3) = cisi(d);
            // 6.283 here, not 6.2832: the source uses the shorter
            // constant in this one place.
            hwave = 6.283 * x;
            let mut ra = 30.0
                * (-0.5 * d.cos() * (0.577_215 + z.ln() - w)
                    + (1.0 + d.cos()) * (0.577_215 + d.ln() - w2)
                    + d.sin() * (0.5 * w1 - w3));
            if el1 < 0.2 {
                ra = 400.0 * el1 * el1 * RINTW / 16.0;
            }
            let mut denom = (hwave * q).cos() - hwave.cos();
            if denom == 0.0 {
                denom = 0.000_000_01;
            }
            let bprim = (((hwave * q).sin() - q * hwave.sin()) / denom).atan();
            rain = 30.0
                * ((((hwave * q).cos() - hwave.cos()) / (t * bprim.cos())).powi(2)
                    * (cv * cv + 1.0 - 2.0 * cv * (psiv - 2.0 * bprim).cos()))
                / ra;
        }
        _ => {}
    }

    state.x = x;
    finish(kop, rain, x, sok, eff8, !skip_385)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dipole_params() -> IoncapParams {
        // A half-wave dipole at 10 metres over average ground.
        let mut parm = [0.0 as R; 20];
        parm[2] = 15.0; // dielectric
        parm[3] = 0.005; // conductivity
        parm[5] = -0.5; // length, wavelengths
        parm[6] = 10.0; // height, metres
        ioninit(3, &parm)
    }

    #[test]
    fn cisi_matches_the_tabulated_integrals() {
        // Ci(1) = 0.3374, Si(1) = 0.9461 to four places.
        let (ci, si) = cisi(1.0);
        assert!((ci - 0.3374).abs() < 5e-4, "Ci(1) = {ci}");
        assert!((si - 0.9461).abs() < 5e-4, "Si(1) = {si}");
        // Ci(10) = -0.0455, Si(10) = 1.6583.
        let (ci, si) = cisi(10.0);
        assert!((ci - -0.0455).abs() < 5e-3, "Ci(10) = {ci}");
        assert!((si - 1.6583).abs() < 5e-3, "Si(10) = {si}");
    }

    #[test]
    fn zero_elevation_is_the_floor() {
        let p = dipole_params();
        let mut st = IoncapState::default();
        let (rain, _) = iongain(&mut st, 3, 0.0, &p, 0.0, 10.0, Model::Compatible);
        assert_eq!(rain, FLOOR);
    }

    #[test]
    fn a_dipole_answers_a_finite_gain_above_the_horizon() {
        let p = dipole_params();
        let mut st = IoncapState::default();
        let (rain, eff) = iongain(&mut st, 3, 0.0, &p, 30.0_f32.to_radians(), 10.0, Model::Compatible);
        assert!(rain.is_finite());
        assert!((FLOOR..15.0).contains(&rain), "gain {rain}");
        assert_eq!(eff, 0.0);
    }

    #[test]
    fn straight_up_reads_the_same_at_every_azimuth() {
        // The wrapper's whole purpose for types 3 and 4.
        let p = dipole_params();
        let mut st = IoncapState::default();
        let (a, _) = iongain(&mut st, 3, 0.0, &p, 1.570_796, 10.0, Model::Compatible);
        let (b, _) = iongain(&mut st, 3, 90.0, &p, 1.570_796, 10.0, Model::Compatible);
        assert!((a - b).abs() < 0.01, "{a} vs {b}");
    }

    fn curtain_params() -> IoncapParams {
        let mut parm = [0.0 as R; 20];
        parm[2] = 15.0;
        parm[3] = 0.005;
        parm[5] = 2.0; // bays
        parm[6] = -0.5; // element length
        parm[7] = 20.0; // height of first element
        parm[8] = 4.0; // elements per bay
        parm[9] = -0.5; // element spacing
        parm[10] = -0.5; // vertical spacing
        parm[11] = -0.25; // screen distance
        ioninit(6, &parm)
    }

    #[test]
    fn the_curtain_floors_within_a_radian_of_vertical() {
        // The integer-literal comparison in the source: elevations
        // above about 33 degrees take the floor — plus EX(1), because
        // SOK has not been reset on that path.
        let p = curtain_params();
        let mut st = IoncapState::default();
        let (low, _) = iongain(&mut st, 6, 0.0, &p, 20.0_f32.to_radians(), 10.0, Model::Compatible);
        let (high, _) = iongain(&mut st, 6, 0.0, &p, 45.0_f32.to_radians(), 10.0, Model::Compatible);
        assert!(low > FLOOR, "20 degrees should compute: {low}");
        // Floor plus the four elements per bay that SOK still holds.
        assert_eq!(high, FLOOR + 4.0);
    }

    #[test]
    fn the_corrected_curtain_computes_above_thirty_three_degrees() {
        let p = curtain_params();
        let mut st = IoncapState::default();
        let (high, _) =
            iongain(&mut st, 6, 0.0, &p, 45.0_f32.to_radians(), 10.0, Model::Corrected);
        assert!(
            high > FLOOR + 4.0,
            "45 degrees should compute on the corrected tier: {high}"
        );

        // The decimal point the source lost was guarding a division by
        // zero at exactly vertical, so that case still takes the floor
        // on both tiers.
        let (vertical, _) = iongain(&mut st, 6, 0.0, &p, PIO2, 10.0, Model::Corrected);
        assert_eq!(vertical, FLOOR + 4.0);
    }

    #[test]
    fn the_stale_x_quirk_changes_the_zero_elevation_answer() {
        // A monopole call leaves X small; the next zero-elevation call
        // reads it in the efficiency polynomial. This is the SAVE
        // statement's observable effect.
        let mut parm = [0.0 as R; 20];
        parm[2] = 15.0;
        parm[3] = 0.005;
        parm[5] = 5.0; // height, metres
        parm[6] = 0.0; // gain above dipole
        let p = ioninit(2, &parm);

        let mut fresh = IoncapState::default();
        let (at_zero_fresh, _) = iongain(&mut fresh, 2, 0.0, &p, 0.0, 10.0, Model::Compatible);

        let mut warmed = IoncapState::default();
        // 5 m at 10 MHz is 0.1668 wavelengths: X ends below 0.35.
        let _ = iongain(&mut warmed, 2, 0.0, &p, 30.0_f32.to_radians(), 10.0, Model::Compatible);
        let (at_zero_warm, _) = iongain(&mut warmed, 2, 0.0, &p, 0.0, 10.0, Model::Compatible);

        assert_ne!(at_zero_fresh, at_zero_warm);
    }
}
