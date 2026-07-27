//! The HFMUFES ITS-78 antenna models: `mufesint`, `mufesgan` and their
//! impedance helpers (`vendor/voacapl/src/hfmufesw`), antenna types
//! 31-47.
//!
//! These are the Ma and Walters (1969) patterns: each is a physical
//! model with a computed input impedance — self- and mutual impedances
//! from cosine/sine-integral expressions (`csz1`, `zmut`, `coll`,
//! `ech`, `sim`), complex matrix inversion for the Yagi and
//! log-periodic currents (`cmpinv`, `matinv`), and 48-point Gaussian
//! integration for the tilted dipole and radial-ground monopole
//! (`agauss`, `mutual`, `resist`, `react`, `onej`).
//!
//! Complex arithmetic matters here: the reference's `CABS` is
//! `hypotf`, its division is libgcc's Smith algorithm and its `CSQRT`
//! is glibc's, so [`Cf32`] reproduces those exactly rather than using
//! textbook formulas that round differently.
//!
//! The reference relies on `gfortran` keeping unsaved locals alive
//! across calls: only `FLOG, C2KEL, S2KEL, ZT, RZERO, RIN` are in the
//! `SAVE` statement, but the Yagi's currents (`CIX`, `CIY`, `XK`), the
//! log-periodic's geometry and the monopole's `CAYA`/`ETA` are also
//! reused on `kas > 1` calls and merely happen to survive on the
//! stack. [`MufesState`] carries all of it explicitly.

// The literals are the source's digits.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use super::con::{D2R, GAMA, PI, PI2, PIO2, R, VOFL};

/// The antenna gain floor, dB (the 1994 change from -10).
const FLOOR: R = -30.0;
const RAIN_MIN: R = 0.001;
/// "RIN" for a vertical monopole at h/lambda = 0.2.
const RINTW: R = 18.06;
/// "RIN" for a vertical dipole at h/lambda = 0.4.
const RINFR: R = 100.34;
/// (4 - 1) / (5 - 0.0001).
const XINTR: R = 0.600012;
const SQRTWO: R = 1.41421356237;

/// A complex `REAL*4` pair with the reference's arithmetic: `CABS` is
/// `hypotf`, division is libgcc's `__divsc3` (Smith's algorithm) and
/// the square root is glibc's `csqrtf`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Cf32 {
    pub re: R,
    pub im: R,
}

impl Cf32 {
    pub const ZERO: Cf32 = Cf32 { re: 0.0, im: 0.0 };

    pub fn new(re: R, im: R) -> Self {
        Self { re, im }
    }

    pub fn conj(self) -> Self {
        Self::new(self.re, -self.im)
    }

    /// `CABS`: gfortran calls `hypotf`.
    pub fn cabs(self) -> R {
        self.re.hypot(self.im)
    }

    /// `CSQRT`: glibc's `csqrtf` for finite inputs.
    pub fn csqrt(self) -> Self {
        let (x, y) = (self.re, self.im);
        if x == 0.0 && y == 0.0 {
            return Self::ZERO;
        }
        let d = x.hypot(y);
        if x > 0.0 {
            let r = (0.5 * (d + x)).sqrt();
            let s = 0.5 * (y / r);
            Self::new(r, s)
        } else {
            let s = (0.5 * (d - x)).sqrt();
            let r = (0.5 * (y / s)).abs();
            Self::new(r, s.copysign(y))
        }
    }
}

impl std::ops::Add for Cf32 {
    type Output = Cf32;
    fn add(self, o: Cf32) -> Cf32 {
        Cf32::new(self.re + o.re, self.im + o.im)
    }
}

impl std::ops::Sub for Cf32 {
    type Output = Cf32;
    fn sub(self, o: Cf32) -> Cf32 {
        Cf32::new(self.re - o.re, self.im - o.im)
    }
}

impl std::ops::Neg for Cf32 {
    type Output = Cf32;
    fn neg(self) -> Cf32 {
        Cf32::new(-self.re, -self.im)
    }
}

impl std::ops::Mul for Cf32 {
    type Output = Cf32;
    fn mul(self, o: Cf32) -> Cf32 {
        Cf32::new(
            self.re * o.re - self.im * o.im,
            self.re * o.im + self.im * o.re,
        )
    }
}

impl std::ops::Mul<R> for Cf32 {
    type Output = Cf32;
    fn mul(self, s: R) -> Cf32 {
        Cf32::new(self.re * s, self.im * s)
    }
}

impl std::ops::Mul<Cf32> for R {
    type Output = Cf32;
    fn mul(self, o: Cf32) -> Cf32 {
        Cf32::new(self * o.re, self * o.im)
    }
}

impl std::ops::Div for Cf32 {
    type Output = Cf32;
    /// libgcc's `__divsc3`: Smith's algorithm on the larger component.
    fn div(self, o: Cf32) -> Cf32 {
        let (a, b, c, d) = (self.re, self.im, o.re, o.im);
        if c.abs() < d.abs() {
            let ratio = c / d;
            let denom = c * ratio + d;
            Cf32::new((a * ratio + b) / denom, (b * ratio - a) / denom)
        } else {
            let ratio = d / c;
            let denom = c + d * ratio;
            Cf32::new((a + b * ratio) / denom, (b - a * ratio) / denom)
        }
    }
}

impl std::ops::Div<R> for Cf32 {
    type Output = Cf32;
    fn div(self, s: R) -> Cf32 {
        Cf32::new(self.re / s, self.im / s)
    }
}

/// `CSZ1`: `Ci(x) - i Si(x)`. Series up to 6, a complex continued
/// fraction above, with the convergence test carried in `REAL*8`.
pub fn csz1(x: R) -> Cf32 {
    const TESTQ: f64 = 4.0e-9;
    if x <= 6.0 {
        let x2 = x * x;
        let mut en: R = 0.0;
        let mut tn = x;
        let mut si = x;
        loop {
            en += 1.0;
            tn = -tn * x2 * (2.0 * en - 1.0) / ((2.0 * en) * (2.0 * en + 1.0).powi(2));
            if f64::from((tn / si).abs()) <= TESTQ {
                break;
            }
            si += tn;
        }
        let mut en: R = 1.0;
        let mut tn = -x2 / 4.0;
        let mut ci = tn + GAMA + x.ln();
        loop {
            en += 1.0;
            tn = -tn * x2 * (2.0 * en - 2.0) / ((2.0 * en - 1.0) * (2.0 * en).powi(2));
            if f64::from((tn / ci).abs()) <= TESTQ {
                break;
            }
            ci += tn;
        }
        Cf32::new(ci, -si)
    } else {
        let mut am1 = Cf32::new(1.0, 0.0);
        let mut am2 = Cf32::new(1.0, 0.0);
        let mut bm1 = Cf32::new(1.0, 0.0);
        let mut bm2 = Cf32::ZERO;
        let mut p: R = 0.0;
        let mut k = 0i32;
        let mut tm1: f64 = 0.0;
        let mut z;
        loop {
            p += 1.0;
            k += 1;
            let sa = if k % 2 != 0 {
                Cf32::new(0.0, (p + 1.0) / (2.0 * x))
            } else {
                Cf32::new(0.0, p / (2.0 * x))
            };
            let a = am1 + sa * am2;
            let b = bm1 + sa * bm2;
            z = a / b;
            let t = f64::from(z.cabs());
            if ((t - tm1) / t).abs() <= TESTQ {
                break;
            }
            am2 = am1;
            am1 = a;
            bm2 = bm1;
            bm1 = b;
            tm1 = t;
        }
        let z = Cf32::new(0.0, PIO2) + Cf32::new(x.cos(), x.sin()) / (Cf32::new(0.0, x) * z);
        z.conj()
    }
}

/// `CANG`: the argument of `z` via `ATAN` plus a quadrant correction,
/// as the CDC library did it (not `atan2`).
pub fn cang(z: Cf32) -> R {
    let x = z.re;
    let y = z.im;
    if x < 0.0 {
        let base = if y < 0.0 { -PI } else { PI };
        if y == 0.0 {
            return PI;
        }
        (y / x).atan() + base
    } else if x == 0.0 {
        if y < 0.0 {
            -PIO2
        } else if y == 0.0 {
            0.0
        } else {
            PIO2
        }
    } else {
        (y / x).atan()
    }
}

/// `ONEJ`: the Bessel function J1(x), rational approximations.
fn onej(x: R) -> R {
    let r135 = PI * 3.0 / 4.0;
    if x <= 4.0 {
        let t = x / 4.0;
        let y = t * t;
        (((((((-1.289769e-4 * y + 2.2069155e-3) * y - 2.36616773e-2) * y + 0.1777582922) * y
            - 0.8888839649)
            * y
            + 2.666666054)
            * y
            - 3.999999971)
            * y
            + 2.0)
            * t
    } else {
        let t = 4.0 / x;
        let y = t * t;
        let psum = (((((4.2414e-6 * y - 2.0092e-5) * y + 5.80759e-5) * y - 2.23203e-4) * y
            + 2.9218256e-3)
            * y
            + 0.3989422819)
            * 2.50662827;
        let qsum = (((((-3.6594e-6 * y + 1.622e-5) * y - 3.98708e-5) * y + 1.064741e-4) * y
            - 6.3904e-4)
            * y
            + 3.74008364e-2)
            * 2.50662827
            * t;
        let tx = (x - r135).cos();
        let ty = (x - r135).sin();
        let ts = (2.0 / (PI * x)).sqrt();
        ts * (psum * tx - qsum * ty)
    }
}

/// The 48-point Gauss-Legendre abscissas.
const XI: [R; 48] = [
    -0.99877100725,
    -0.99353017227,
    -0.98412458372,
    -0.97059159255,
    -0.95298770316,
    -0.93138669071,
    -0.90587913672,
    -0.87657202027,
    -0.84358826162,
    -0.80706620403,
    -0.76715903252,
    -0.72403413092,
    -0.67787237963,
    -0.62886739678,
    -0.57722472608,
    -0.52316097472,
    -0.46690290475,
    -0.40868648199,
    -0.34875588629,
    -0.28736248736,
    -0.22476379039,
    -0.16122235607,
    -0.097004699209,
    -0.032380170963,
    0.032380170963,
    0.097004699209,
    0.16122235607,
    0.22476379039,
    0.28736248736,
    0.34875588629,
    0.40868648199,
    0.46690290475,
    0.52316097472,
    0.57722472608,
    0.62886739678,
    0.67787237963,
    0.72403413092,
    0.76715903252,
    0.80706620403,
    0.84358826162,
    0.87657202027,
    0.90587913672,
    0.93138669071,
    0.95298770316,
    0.97059159255,
    0.98412458372,
    0.99353017227,
    0.99877100725,
];

/// The matching weights.
const HH: [R; 48] = [
    0.0031533460523,
    0.007327553901,
    0.011477234579,
    0.015579315723,
    0.019616160457,
    0.023570760839,
    0.027426509708,
    0.031167227833,
    0.034777222565,
    0.038241351066,
    0.041545082943,
    0.044674560857,
    0.047616658492,
    0.050359035554,
    0.052890189485,
    0.055199503700,
    0.057277292100,
    0.059114839698,
    0.060704439166,
    0.062039423160,
    0.063114192286,
    0.063924238585,
    0.064466164436,
    0.064737696813,
    0.064737696813,
    0.064466164436,
    0.063924238585,
    0.063114192286,
    0.062039423160,
    0.060704439166,
    0.059114839698,
    0.057277292100,
    0.055199503700,
    0.052890189485,
    0.050359035554,
    0.047616658492,
    0.044674560857,
    0.041545082943,
    0.038241351066,
    0.034777222565,
    0.031167227833,
    0.027426509708,
    0.023570760839,
    0.019616160457,
    0.015579315723,
    0.011477234579,
    0.0073275539013,
    0.0031533460523,
];

/// `AGAUSS`: adaptive 48-point Gaussian integration over `[-h2, h2]`,
/// subdividing up to ten panels until the answer settles.
fn agauss(f: &dyn Fn(R) -> R, h2: R) -> R {
    const TESTD: R = 5.0e-8;
    if h2 == 0.0 {
        return 0.0;
    }
    let mut m = 1i32;
    let mut ans: R = 0.0;
    let mut check: R = 1000.0;
    loop {
        let mut total: R = 0.0;
        for l in 1..=m {
            let fl = l as R;
            let fm = m as R;
            let bolim = -h2 + 2.0 * h2 * (fl - 1.0) / fm;
            let uplim = -h2 + 2.0 * h2 * fl / fm;
            let mut sum: R = 0.0;
            for i in 0..48 {
                let s = 0.5 * ((uplim - bolim) * XI[i] + uplim + bolim);
                sum += HH[i] * f(s);
            }
            total += 0.5 * (uplim - bolim) * sum;
        }
        let test = ((total - ans) / total).abs();
        if test <= TESTD {
            return total;
        }
        if m >= 10 {
            // Not converged after ten panels: keep whichever of the
            // last two answers moved less.
            if check >= test {
                return total;
            }
            return ans;
        }
        check = test - TESTD;
        m += 1;
        ans = total;
    }
}

/// `/CUR/`: the geometry one impedance evaluation works on.
struct Cur {
    dij: R,
    eil: R,
    hij: R,
    kode: i32,
}

/// `ZMUT`: self- or mutual impedance between parallel dipoles of equal
/// length at spacing `dij`.
fn zmut(c: &Cur) -> Cf32 {
    let d2 = c.dij * c.dij;
    let el2 = c.eil * c.eil;
    let t = (d2 + 4.0 * el2).sqrt();
    let uz = PI2 * (t - 2.0 * c.eil);
    let vz = PI2 * (t + 2.0 * c.eil);
    let uzp = PI2 * c.dij;
    let t = (d2 + el2).sqrt();
    let u1 = PI2 * (t - c.eil);
    let v1 = PI2 * (t + c.eil);
    let w1 = 2.0 * PI2 * c.eil;
    let cw1 = w1.cos();
    let sw1 = w1.sin();
    let csu1 = csz1(u1);
    let csv1 = csz1(v1);
    let csuzp = csz1(uzp);
    let zsum = (csz1(uz) - 2.0 * csu1) * Cf32::new(cw1, -sw1)
        + (csz1(vz) - 2.0 * csv1) * Cf32::new(cw1, sw1)
        + 2.0 * (csuzp - csu1 - csv1)
        + 2.0 * csuzp * (cw1 + 1.0);
    if c.kode > 0 {
        // ZSUM * 60.0 / (1.0 - CW1): multiply first, then divide the
        // components — the scalar quotient rounds differently.
        zsum * 60.0 / (1.0 - cw1)
    } else {
        zsum * 30.0
    }
}

/// `COLL`: mutual impedance between colinear dipoles.
fn coll(c: &Cur) -> Cf32 {
    let uz = PI2 * 2.0 * (c.hij - c.eil);
    let u1 = PI2 * 2.0 * c.hij;
    let v2 = PI2 * 2.0 * (c.hij + 2.0 * c.eil);
    let u3 = PI2 * 2.0 * (c.hij + c.eil);
    let v4 = PI2 * 2.0 * (c.hij + 3.0 * c.eil);
    let csu1 = csz1(u1);
    let csv2 = csz1(v2);
    let csu3 = csz1(u3);
    let hlog = (c.hij / (c.hij + c.eil)).ln();
    let h2log = ((c.hij + 2.0 * c.eil) / (c.hij + c.eil)).ln();
    let t = PI2 * (c.hij - c.eil);
    let ze = Cf32::new(t.cos(), t.sin());
    let mut sum = ze * (csz1(uz) - csu1) + ze.conj() * (c.hij / (c.hij - c.eil)).ln();
    let t = PI2 * (c.hij + c.eil);
    let ze = Cf32::new(t.cos(), t.sin());
    sum = sum + ze * (csu3 - csv2) + ze.conj() * h2log;
    sum = sum + ze * (-csu1 + csu3) + ze.conj() * hlog;
    let t = PI2 * (c.hij + 3.0 * c.eil);
    let ze = Cf32::new(t.cos(), t.sin());
    sum = sum
        + ze * (-csv2 + csz1(v4))
        + ze.conj() * ((c.hij + 2.0 * c.eil) / (c.hij + 3.0 * c.eil)).ln();
    let ts = 2.0 * (PI2 * c.eil).cos();
    let t = PI2 * c.hij;
    let ze = Cf32::new(t.cos(), t.sin());
    sum = sum + ts * ze * (-csu1 + csu3) + ts * ze.conj() * hlog;
    let t = PI2 * (c.hij + 2.0 * c.eil);
    let ze = Cf32::new(t.cos(), t.sin());
    sum = sum + ts * ze * (csu3 - csv2) + ts * ze.conj() * h2log;
    if c.kode > 0 {
        sum * 30.0 / (1.0 - (2.0 * PI2 * c.eil).cos())
    } else {
        sum * 15.0
    }
}

/// `ECH`: mutual impedance between dipoles in echelon.
fn ech(c: &Cur) -> Cf32 {
    let d2 = c.dij * c.dij;
    let hml = c.hij - c.eil;
    let hpl = c.hij + c.eil;
    let t = (d2 + hml * hml).sqrt();
    let uz = PI2 * (t + hml);
    let vz = PI2 * (t - hml);
    let t = (d2 + hpl * hpl).sqrt();
    let uzp = PI2 * (t - hpl);
    let vzp = PI2 * (t + hpl);
    let ts = c.hij;
    let t = (d2 + ts * ts).sqrt();
    let u1 = PI2 * (t + ts);
    let v1 = PI2 * (t - ts);
    let ts = c.hij + 2.0 * c.eil;
    let t = (d2 + ts * ts).sqrt();
    let u2 = PI2 * (t - ts);
    let v2 = PI2 * (t + ts);
    let ts = c.hij + 3.0 * c.eil;
    let t = (d2 + ts * ts).sqrt();
    let u4 = PI2 * (t - ts);
    let v4 = PI2 * (t + ts);
    let csu1 = csz1(u1);
    let csv1 = csz1(v1);
    let csu2 = csz1(u2);
    let csv2 = csz1(v2);
    let csuzp = csz1(uzp);
    let csvzp = csz1(vzp);
    let mut zsum = Cf32::ZERO;
    let t = PI2 * (c.eil - c.hij);
    let ze = Cf32::new(t.cos(), t.sin());
    zsum = zsum + ze.conj() * (csz1(uz) - csu1) + ze * (csz1(vz) - csv1);
    let t = PI2 * (c.eil + c.hij);
    let ze = Cf32::new(t.cos(), t.sin());
    zsum = zsum + ze.conj() * (csuzp - csu2) + ze * (csvzp - csv2);
    let t = PI2 * (-c.eil - c.hij);
    let ze = Cf32::new(t.cos(), t.sin());
    zsum = zsum + ze.conj() * (-csu1 + csvzp) + ze * (-csv1 + csuzp);
    let t = PI2 * (3.0 * c.eil + c.hij);
    let ze = Cf32::new(t.cos(), t.sin());
    zsum = zsum + ze.conj() * (-csu2 + csz1(u4)) + ze * (-csv2 + csz1(v4));
    let ts = 2.0 * (PI2 * c.eil).cos();
    let t = PI2 * c.hij;
    let ze = Cf32::new(t.cos(), t.sin());
    zsum = zsum + ts * ze.conj() * (-csv1 + csuzp) + ts * ze * (-csu1 + csvzp);
    let t = PI2 * (2.0 * c.eil + c.hij);
    let ze = Cf32::new(t.cos(), t.sin());
    zsum = zsum + ts * ze.conj() * (csuzp - csu2) + ts * ze * (csvzp - csv2);
    if c.kode > 0 {
        zsum * 30.0 / (1.0 - (2.0 * PI2 * c.eil).cos())
    } else {
        zsum * 15.0
    }
}

/// `SIM`: mutual impedances between parallel dipoles of unequal
/// lengths, one row of results per call. Arrays are 1-based slices of
/// `/HFMUFES_ONE/`.
fn sim(d1d: &[R; 21], ell: &[R; 21], nmx: usize, no: usize, zs: &mut [Cf32; 21]) {
    for j in 1..=no {
        let djj = d1d[j];
        let djs = djj * djj;
        let hs = ell[nmx] + ell[j];
        let hd = ell[nmx] - ell[j];
        let cw1 = hs.cos();
        let cw2 = hd.cos();
        let sw1 = hs.sin();
        let sw2 = hd.sin();
        let tt = (djs + hs * hs).sqrt();
        let uz = tt - hs;
        let vz = tt + hs;
        let tt = (djs + hd * hd).sqrt();
        let uzp = tt - hd;
        let vzp = tt + hd;
        let tt = (djs + ell[nmx] * ell[nmx]).sqrt();
        let u1 = tt - ell[nmx];
        let v1 = tt + ell[nmx];
        let tt = (djs + ell[j] * ell[j]).sqrt();
        let u2 = tt - ell[j];
        let v2 = tt + ell[j];
        let z = (csz1(uz) - csz1(u1) - csz1(u2)) * Cf32::new(cw1, -sw1)
            + (csz1(vz) - csz1(v1) - csz1(v2)) * Cf32::new(cw1, sw1)
            + (csz1(uzp) - csz1(u1) - csz1(v2)) * Cf32::new(cw2, -sw2)
            + (csz1(vzp) - csz1(v1) - csz1(u2)) * Cf32::new(cw2, sw2)
            + 2.0 * csz1(djj) * (cw1 + cw2);
        zs[j] = z * 60.0 / (cw2 - cw1);
    }
}

/// `MATINV`: Gauss-Jordan inversion with complete pivoting, in place.
/// Returns the pivot bookkeeping so `CMPINV` can test for singularity.
// The elimination indexes two rows of `a` at once; an iterator form
// would need split borrows for no gain in clarity.
#[allow(clippy::needless_range_loop)]
fn matinv(a: &mut [[R; 20]; 20], n: usize) -> [i32; 20] {
    let mut ipivot = [0i32; 20];
    let mut index = [[0usize; 2]; 20];
    let mut pivot = [0.0 as R; 20];
    let mut determ: R = 1.0;
    for i in 0..n {
        let mut temp: R = 0.0;
        let mut irow = 0usize;
        let mut icolum = 0usize;
        for j1 in 0..n {
            if ipivot[j1] == 1 {
                continue;
            }
            for k in 0..n {
                match ipivot[k].cmp(&1) {
                    std::cmp::Ordering::Less => {
                        if temp.abs() <= a[j1][k].abs() {
                            irow = j1;
                            icolum = k;
                            temp = a[j1][k];
                        }
                    }
                    std::cmp::Ordering::Equal => {}
                    std::cmp::Ordering::Greater => return ipivot,
                }
            }
        }
        ipivot[icolum] += 1;
        if irow != icolum {
            determ = -determ;
            // The source swaps element-wise; whole rows is the same.
            a.swap(irow, icolum);
        }
        index[i][0] = irow;
        index[i][1] = icolum;
        pivot[i] = a[icolum][icolum];
        determ *= pivot[i];
        if determ == 0.0 {
            return ipivot;
        }
        a[icolum][icolum] = 1.0;
        for slot in a[icolum].iter_mut().take(n) {
            *slot /= pivot[i];
        }
        for i1 in 0..n {
            if i1 == icolum {
                continue;
            }
            let temp = a[i1][icolum];
            a[i1][icolum] = 0.0;
            for l2 in 0..n {
                a[i1][l2] -= a[icolum][l2] * temp;
            }
        }
    }
    for i2 in 0..n {
        let ll = n - 1 - i2;
        if index[ll][0] == index[ll][1] {
            continue;
        }
        let irow = index[ll][0];
        let icolum = index[ll][1];
        for row in a.iter_mut().take(n) {
            row.swap(irow, icolum);
        }
    }
    ipivot
}

/// `SQMULT`: `c = a * b` for square matrices.
fn sqmult(a: &[[R; 20]; 20], b: &[[R; 20]; 20], c: &mut [[R; 20]; 20], n: usize) {
    let mut col = [0.0 as R; 20];
    for j in 0..n {
        for (k, slot) in col.iter_mut().enumerate().take(n) {
            *slot = b[k][j];
        }
        for i in 0..n {
            let mut acc: R = 0.0;
            for l in 0..n {
                acc += a[i][l] * col[l];
            }
            c[i][j] = acc;
        }
    }
}

/// `CMPINV`: the inverse `c + i d` of `a + i b` via two real
/// inversions. Panics where the source STOPs on a singular matrix.
fn cmpinv(a: &mut [[R; 20]; 20], b: &mut [[R; 20]; 20], n: usize) -> ([[R; 20]; 20], [[R; 20]; 20]) {
    let mut c = [[0.0 as R; 20]; 20];
    let mut d = [[0.0 as R; 20]; 20];
    let mut swapped = false;
    loop {
        for i in 0..n {
            for j in 0..n {
                d[i][j] = -a[i][j];
            }
        }
        let ipivot = matinv(&mut d, n);
        if ipivot.iter().take(n).all(|p| *p == 1) {
            let mut t = [[0.0 as R; 20]; 20];
            sqmult(b, &d, &mut t, n);
            d = t;
            sqmult(&d, b, &mut c, n);
            for i1 in 0..n {
                for j1 in 0..n {
                    c[i1][j1] = a[i1][j1] - c[i1][j1];
                }
            }
            let ipivot = matinv(&mut c, n);
            assert!(
                ipivot.iter().take(n).all(|p| *p == 1),
                "MATRIX C IN SUBROUTINE CMPINV DOES NOT EXIST."
            );
            let mut t = [[0.0 as R; 20]; 20];
            sqmult(&c, &d, &mut t, n);
            d = t;
            if swapped {
                // Re-interchange with signs changed for the factored-out i.
                for i3 in 0..n {
                    for j3 in 0..n {
                        std::mem::swap(&mut a[i3][j3], &mut b[i3][j3]);
                        let temp = -c[i3][j3];
                        c[i3][j3] = -d[i3][j3];
                        d[i3][j3] = temp;
                    }
                }
            }
            return (c, d);
        }
        // A singular: interchange A and B (factoring out i) and retry.
        assert!(!swapped, "MATRICES A AND B BOTH SINGULAR IN SUBR CMPINV.");
        for i2 in 0..n {
            for j2 in 0..n {
                std::mem::swap(&mut a[i2][j2], &mut b[i2][j2]);
            }
        }
        swapped = true;
    }
}

/// What `mufesint` extracts from the definition file, per type.
#[derive(Debug, Clone, Copy)]
pub struct MufesParams {
    pub asig: R,
    pub aeps: R,
    pub and: R,
    pub anl: R,
    pub anh: R,
    pub aex: [R; 4],
}

/// `mufesint`: maps `parm` onto the pattern arguments for HFMUFES
/// antenna `index` (1-17).
pub fn mufesint(index: i32, parm: &[R; 20]) -> MufesParams {
    let mut p = MufesParams {
        asig: parm[3],
        aeps: parm[2],
        and: 0.0,
        anl: 0.0,
        anh: 0.0,
        aex: [0.0; 4],
    };
    match index {
        1 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
        }
        2 => {
            p.anl = parm[5];
            p.anh = parm[6];
        }
        3 => {
            p.anl = -0.5;
            p.anh = parm[6];
            p.and = parm[7];
        }
        4 => {
            p.anh = parm[5];
            p.anl = parm[6];
            p.and = parm[7];
            p.aex[0] = parm[8];
            p.aex[1] = parm[11];
            p.aex[2] = parm[9];
            p.aex[3] = parm[10];
        }
        5 => {
            p.anl = parm[5];
            p.anh = parm[6];
            p.and = parm[7];
        }
        6 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
            p.aex[0] = parm[8];
            p.aex[1] = parm[9];
            p.aex[2] = parm[10];
            p.aex[3] = parm[11];
        }
        7 | 9 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
            p.aex[0] = parm[8];
        }
        8 => {
            p.anl = parm[5];
            p.anh = parm[6];
        }
        11 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
        }
        13 => {
            p.anh = parm[5];
            p.anl = parm[6];
            p.and = parm[7];
            p.aex[0] = parm[8];
            p.aex[1] = parm[9];
            p.aex[2] = parm[10];
            p.aex[3] = parm[11];
        }
        14 => {
            p.and = parm[5];
            p.anl = parm[6];
            p.anh = parm[7];
        }
        15 => {
            p.and = parm[5];
            p.anl = parm[6];
        }
        16 => {
            p.anh = parm[5];
            p.anl = parm[6];
            p.and = parm[7];
            p.aex[0] = parm[8];
            p.aex[1] = parm[9];
            p.aex[2] = parm[10];
            p.aex[3] = parm[11];
        }
        17 => {
            p.anl = parm[5];
            p.and = parm[6];
            p.aex[0] = parm[7];
            p.aex[1] = parm[8];
        }
        _ => {}
    }
    p
}

/// The values a `kas <= 1` call computes and later calls reuse. The
/// source `SAVE`s only some of them; the rest survive on gfortran's
/// stack by accident, so this struct is the honest version of that.
#[derive(Debug, Clone)]
pub struct MufesState {
    rin: R,
    rzero: R,
    // The radial-ground monopole's integration constants.
    caya: R,
    eta: Cf32,
    // The Yagi's and log-periodic's element currents and geometry.
    n: usize,
    cix: [R; 21],
    ciy: [R; 21],
    xk: [R; 21],
    ell: [R; 21],
    // The curtain's normalisation switch.
    kode: i32,
}

impl Default for MufesState {
    fn default() -> Self {
        Self {
            rin: 0.0,
            rzero: 0.0,
            caya: 0.0,
            eta: Cf32::ZERO,
            n: 0,
            cix: [0.0; 21],
            ciy: [0.0; 21],
            xk: [0.0; 21],
            ell: [0.0; 21],
            kode: 1,
        }
    }
}

/// `MUFESGAN`: gain and efficiency in dB for HFMUFES antenna `kop` at
/// off-azimuth `toaz` (degrees), elevation `delta` (radians) and
/// `freq` MHz. `kas` counts calls per frequency: 0 and 1 recompute the
/// impedances, higher values reuse [`MufesState`].
#[allow(clippy::too_many_lines, clippy::needless_range_loop)]
pub fn mufesgan(
    st: &mut MufesState,
    kop: i32,
    kas: i32,
    toaz: R,
    p: &MufesParams,
    delta: R,
    freq: R,
) -> (R, R) {
    let ratio = SQRTWO / 4680.0;
    let beta = toaz;
    let sigma = p.asig;
    let er = p.aeps;
    let h = p.anh;
    let el = p.anl;
    let phi = p.and;
    let ex = p.aex;

    let mut rain: R;
    let mut eff: R = 0.0;

    if delta <= 0.0 {
        return (FLOOR, 0.0);
    }
    if kop == 12 {
        // Constant gain: H is already in dB.
        return (h, 0.0);
    }

    let wave = VOFL / freq;
    let q = delta.sin();
    let t = delta.cos();
    // Complex dielectric constant, eqn (11) p. 6.
    let dif = Cf32::new(er, -60.0 * sigma * wave);
    let acsq = (dif - Cf32::new(t * t, 0.0)).csqrt();
    // Fresnel coefficients, eqns (9) and (10) p. 5.
    let qper = (dif * q - acsq) / (dif * q + acsq);
    let cv = qper.cabs();
    let mut psiv = cang(qper);
    let qpar = (Cf32::new(q, 0.0) - acsq) / (Cf32::new(q, 0.0) + acsq);
    let ch = qpar.cabs();
    let mut psih = cang(qpar);

    let mut el1 = el / wave;
    if el < 0.0 {
        el1 = el.abs();
    }
    let fac = PI * el1;
    let fac2 = PI2 * el1;
    let fac4 = 2.0 * fac2;
    let mut x = h / wave;
    if h < 0.0 {
        x = h.abs();
    }
    let hwave = PI2 * x;
    let hqwave = 2.0 * hwave * q;
    let rhi = phi * D2R;
    let mut sr = rhi.sin();
    let mut cr = rhi.cos();
    let reta = beta * D2R;
    let mut sb = reta.sin();
    let mut cb = reta.cos();

    match kop {
        // Terminated rhombic, KOP=1.
        1 => {
            let tsc = 1.0 - t * sr * cb;
            let tcs = t * cr * sb;
            let u1 = tsc - tcs;
            let u2 = tsc + tcs;
            let w1 = (psih - hqwave).cos();
            let w3 = (psiv - hqwave).cos();
            rain = 3.20
                * (cr * (fac * u1).sin() * (fac * u2).sin() / (u1 * u2)).powi(2)
                * ((cb - sr * t).powi(2) * (ch * ch + 1.0 + 2.0 * ch * w1)
                    + sb * sb * (cv * cv + 1.0 - 2.0 * cv * w3) * (q * q));
            eff = -1.7;
        }
        // Vertical (2) and vertical with radial ground (17).
        2 | 17 => {
            let dmpio2 = (delta - PIO2).abs();
            rain = 0.00004;
            if dmpio2 > 0.5 * D2R {
                let sfac2 = fac2.sin();
                let cfac2 = fac2.cos();
                let hq = fac2 * q;
                let a = hq.cos() - cfac2;
                let a_s = hq.sin() - q * sfac2;
                let (c2kel, s2kel);
                if kas <= 1 {
                    let flog = fac2.ln();
                    let c2 = 2.0 * cfac2 * cfac2 - 1.0;
                    let s2 = 2.0 * cfac2 * sfac2;
                    let zt = csz1(4.0 * fac2);
                    let mut rzero = 0.5
                        * (c2 * (zt.re - flog - 1.3862943612 - GAMA) - s2 * zt.im);
                    let zt = csz1(fac4);
                    rzero = 30.0
                        * (rzero
                            + (1.0 + c2) * (-zt.re + flog + 0.6931471806 + GAMA)
                            + s2 * zt.im);
                    let mut rin = rzero;
                    if el1 < 0.2 {
                        rin = 400.0 * el1 * el1 * RINTW / 16.0;
                    }
                    st.rzero = rin;
                    st.rin = rin;
                    c2kel = c2;
                    s2kel = s2;
                } else {
                    c2kel = 2.0 * cfac2 * cfac2 - 1.0;
                    s2kel = 2.0 * cfac2 * sfac2;
                }
                if kop == 17 {
                    // Radial conductor ground system.
                    if kas <= 1 {
                        let mut aa = phi / wave;
                        if phi < 0.0 {
                            aa = phi.abs();
                        }
                        st.caya = PI2 * aa;
                        st.eta = (Cf32::new(0.0, 8.0 * (PI * PI) * freq * 0.1)
                            / Cf32::new(sigma, freq * er * 0.001 / 18.0))
                        .csqrt();
                        let alpha = cang(st.eta) + PIO2;
                        let rz = (aa * aa + el1 * el1).sqrt();
                        let r1 = aa + rz;
                        let ztr = Cf32::new(0.0, PIO2);
                        let delz = (csz1(2.0 * PI2 * (rz + el1)) + ztr) * Cf32::new(c2kel, s2kel)
                            + (csz1(2.0 * PI2 * (rz - el1)) + ztr) * Cf32::new(c2kel, -s2kel)
                            + (csz1(2.0 * st.caya) + ztr) * (2.0 * cfac2 * cfac2)
                            + (csz1(PI2 * r1) + ztr) * (4.0 * cfac2)
                            - (csz1(PI2 * (r1 - el1)) + ztr)
                                * (4.0 * cfac2)
                                * Cf32::new(cfac2, -sfac2)
                            - (csz1(PI2 * (r1 + el1)) + ztr)
                                * (4.0 * cfac2)
                                * Cf32::new(cfac2, sfac2);
                        let delr1 = (delz * st.eta / (2.0 * PI2)).re;
                        let eta1 = st.eta.re;
                        let eta2 = st.eta.im;
                        let mut delr2: R = 0.0;
                        let dp = aa / 2.0;
                        let qu = 240.0 * (PI * PI) / ex[1];
                        let cw = wave * 1000.0 / (ex[0] * ex[1]);
                        for j in 0..48 {
                            let pp = dp * (XI[j] + 1.0);
                            let rq = PI2 * (pp * pp + el1 * el1).sqrt();
                            let plog = (pp * cw).ln();
                            let qq = qu * pp * plog;
                            let eq = eta2 + qq;
                            let ta = eq.atan2(eta1);
                            delr2 += ((alpha - ta - 2.0 * rq).cos()
                                + cfac2 * cfac2 * (alpha - ta - 4.0 * PI * pp).cos()
                                - 2.0 * cfac2 * (alpha - ta - PI2 * pp - rq).cos())
                                / (eta1 * eta1 + eq * eq).sqrt()
                                * plog
                                * dp
                                * HH[j];
                        }
                        delr2 *= -120.0 * PI * st.eta.cabs() / ex[1];
                        st.rin = st.rzero + delr1 + delr2;
                    }
                    let mut hratio = Cf32::ZERO;
                    for j in 0..48 {
                        let xx = st.caya / 2.0 * (XI[j] + 1.0);
                        let td = (xx * xx + fac2 * fac2).sqrt();
                        let ts = onej(xx * t);
                        hratio = hratio
                            + HH[j]
                                * ts
                                * Cf32::new(
                                    td.cos() - xx.cos() * cfac2,
                                    -td.sin() + xx.sin() * cfac2,
                                );
                    }
                    let hratio =
                        Cf32::new(1.0, 0.0) - hratio * st.caya * st.eta * t / (120.0 * PI2 * a);
                    rain = 0.0;
                    if hratio.re.abs() <= 2.0 && hratio.im.abs() <= 1.0 {
                        let bp = a_s.atan2(a);
                        let cayvh = 1.0 + cv * cv + 2.0 * cv * (psiv - 2.0 * bp).cos();
                        let tb = a / (t * bp.cos());
                        // 30./RIN*TB**2*CAYVH*HRATIO*CONJG(HRATIO): the
                        // scalar scales HRATIO first, then the complex
                        // product with the conjugate; REAL on assignment.
                        let scale = 30.0 / st.rin * (tb * tb) * cayvh;
                        rain = ((scale * hratio) * hratio.conj()).re;
                    }
                } else {
                    let w3 = psiv.cos();
                    let w4 = psiv.sin();
                    rain = 30.0
                        * ((a * (1.0 + cv * w3) + a_s * cv * w4).powi(2)
                            + (a * cv * w4 + a_s * (1.0 - cv * w3)).powi(2))
                        / (st.rin * (t * t));
                    rain = rain.max(0.00004);
                }
            }
            if el1 < 0.35 {
                eff = -((((6416.702 * el1 - 6091.33) * el1 + 2179.89) * el1 - 364.817) * el1
                    + 25.646);
            }
        }
        // Horizontal half-wave dipole, KOP=3.
        3 => {
            let sfac = fac.sin();
            let _cfac = fac.cos();
            let w1 = (psih - hqwave).cos();
            let w3 = (psiv - hqwave).cos();
            let cphi = t * sb;
            let sphi2 = 1.0 - cphi * cphi;
            rain = 0.0;
            if sphi2 != 0.0 {
                let gi = ((fac * cphi).cos() - fac.cos()) / sphi2;
                let aa = (1.0 + cv * cv - 2.0 * cv * w3).sqrt() * gi;
                let bb = (1.0 + ch * ch + 2.0 * ch * w1).sqrt() * gi;
                if kas <= 1 {
                    let mut d1d = [0.0 as R; 2];
                    d1d[0] = 2.0 * hwave;
                    d1d[1] = ratio * fac;
                    let sfac2 = fac2.sin();
                    let cfac2 = fac2.cos();
                    let mut z = [Cf32::ZERO; 2];
                    for j in 0..2 {
                        let tt = (d1d[j] * d1d[j] + fac2 * fac2).sqrt();
                        let uz = tt - fac2;
                        let vz = tt + fac2;
                        let tt = (d1d[j] * d1d[j] + fac * fac).sqrt();
                        let u1 = tt - fac;
                        let v1 = tt + fac;
                        let zj = (csz1(uz) - 2.0 * csz1(u1)) * Cf32::new(cfac2, -sfac2)
                            + (csz1(vz) - 2.0 * csz1(v1)) * Cf32::new(cfac2, sfac2)
                            - 2.0 * (csz1(u1) + csz1(v1))
                            + 2.0 * csz1(d1d[j]) * (cfac2 + 2.0);
                        z[j] = zj * 60.0 / (1.0 - cfac2);
                    }
                    let sqrd = dif.csqrt();
                    let cxc = (z[0] * ((Cf32::new(1.0, 0.0) - sqrd) / (Cf32::new(1.0, 0.0) + sqrd)))
                        .re;
                    st.rin = z[1].re + cxc;
                }
                rain = (120.0 * (aa * aa * sb * sb * q * q + bb * bb * cb * cb))
                    / (st.rin * sfac * sfac);
            }
        }
        // Horizontal Yagi, KOP=4.
        4 => {
            rain = 0.0;
            if (0.25..=0.75).contains(&el1) {
                let w1 = ch * (psih - hqwave).cos();
                let w2 = ch * (psih - hqwave).sin();
                let w3 = cv * (psiv - hqwave).cos();
                let w4 = cv * (psiv - hqwave).sin();
                let n = ex[1] as usize;
                let nm1 = n - 1;
                let nm2 = n.saturating_sub(2);
                let mut d = [0.0 as R; 21];
                d[nm1] = ex[3] / wave;
                if ex[3] < 0.0 {
                    d[nm1] = ex[3].abs();
                }
                d[1] = ex[2] / wave;
                if ex[2] < 0.0 {
                    d[1] = ex[2].abs();
                }
                st.ell[n] = PI * phi / wave;
                if phi < 0.0 {
                    st.ell[n] = PI * phi.abs();
                }
                st.ell[nm1] = fac;
                st.ell[1] = PI * ex[0] / wave;
                if ex[0] < 0.0 {
                    st.ell[1] = PI * ex[0].abs();
                }
                st.xk[1] = 0.0;
                if n != 3 && nm2 >= 2 {
                    for j in 2..=nm2 {
                        st.ell[j] = st.ell[1];
                        d[j] = d[1];
                    }
                }
                if n >= 2 {
                    for j in 2..=n {
                        st.xk[j] = st.xk[j - 1] + d[j - 1];
                    }
                }
                if kas <= 1 {
                    st.n = n;
                    let mut yr = [[0.0 as R; 20]; 20];
                    let mut yi = [[0.0 as R; 20]; 20];
                    let mut d1d = [0.0 as R; 21];
                    let mut zs = [Cf32::ZERO; 21];
                    for i in 1..=n {
                        for k in 1..=i {
                            d1d[k] = PI2 * (st.xk[i] - st.xk[k]).abs();
                            if i == k {
                                d1d[k] = st.ell[i] / 125.1579;
                            }
                        }
                        sim(&d1d, &st.ell, i, i, &mut zs);
                        for j in 1..=i {
                            yr[i - 1][j - 1] = zs[j].re;
                            yr[j - 1][i - 1] = yr[i - 1][j - 1];
                            yi[i - 1][j - 1] = zs[j].im;
                            yi[j - 1][i - 1] = yi[i - 1][j - 1];
                        }
                    }
                    let (tx, ty) = cmpinv(&mut yr, &mut yi, n);
                    for i in 1..=n {
                        st.cix[i] = tx[i - 1][nm1 - 1];
                        st.ciy[i] = ty[i - 1][nm1 - 1];
                    }
                    let v = Cf32::new(1.0, 0.0) / Cf32::new(st.cix[nm1], st.ciy[nm1]);
                    let sum1 = v.re;
                    let tt = 4.0 * x * x;
                    for j in 1..=nm1 {
                        let tx1 = ((nm1 - j) * (nm1 - j)) as R;
                        d1d[j] = PI2 * (tt + tx1 * d[j] * d[j]).sqrt();
                    }
                    d1d[n] = PI2 * (tt + d[nm1] * d[nm1]).sqrt();
                    sim(&d1d, &st.ell, nm1, n, &mut zs);
                    let mut v = Cf32::ZERO;
                    for j in 1..=n {
                        v = v + Cf32::new(st.cix[j], st.ciy[j]) * zs[j];
                    }
                    let sqrd = dif.csqrt();
                    let sum2 = (v * (Cf32::new(1.0, 0.0) - sqrd) / (Cf32::new(1.0, 0.0) + sqrd)
                        / Cf32::new(st.cix[nm1], st.ciy[nm1]))
                    .re;
                    st.rin = sum1 + sum2;
                }
                let cpsi = t * sb;
                let spsi2 = 1.0 - cpsi * cpsi;
                if spsi2 != 0.0 {
                    let mut etr: R = 0.0;
                    let mut eti: R = 0.0;
                    let pr = -cb * PI2 * t;
                    for j in 1..=n {
                        let ctk = (pr * st.xk[j]).cos();
                        let stk = (pr * st.xk[j]).sin();
                        let sis = 1.0 / st.ell[j].sin();
                        let tt = sis * sis * ((st.ell[j] * cpsi).cos() - st.ell[j].cos());
                        etr += tt * (st.cix[j] * ctk - st.ciy[j] * stk);
                        eti += tt * (st.cix[j] * stk + st.ciy[j] * ctk);
                    }
                    let epmag = cb * cb
                        * ((etr * (1.0 + w1) - eti * w2).powi(2)
                            + (eti * (1.0 + w1) + etr * w2).powi(2));
                    let etmag = sb * sb
                        * (q * q)
                        * ((etr * (1.0 - w3) + eti * w4).powi(2)
                            + (eti * (1.0 - w3) - etr * w4).powi(2));
                    rain = 120.0 * st.rin * (etmag + epmag) / (spsi2 * spsi2);
                }
            }
        }
        // Vertical dipole, KOP=5.
        5 => {
            let tip = 0.5 * el1;
            if tip > x {
                // The antenna is not physically valid; the source
                // prints and returns the floor without the dB tail.
                return (FLOOR, eff);
            }
            let cfac = fac.cos();
            let sphi2 = 1.0 - q * q;
            rain = 0.0;
            if sphi2 != 0.0 {
                let gi = ((fac * q).cos() - cfac) / sphi2;
                let w3 = (psiv - hqwave).cos();
                let eteta1 = -t * gi * (1.0 + cv * w3);
                let w4 = (psiv - hqwave).sin();
                let eteta2 = -t * gi * cv * w4;
                let hac2 = hwave + hwave;
                let hac4 = hac2 + hac2;
                let azh = csz1(hac2);
                let w33 = azh.re;
                let w4b = -azh.im;
                let azh = csz1(hac4);
                let w5 = azh.re;
                let w6 = -azh.im;
                let mut rin = 60.0
                    * ((1.0 + hac2.cos()) * (GAMA + hac2.ln() - w33)
                        - 0.5 * hac2.cos() * (GAMA + hac4.ln() - w5)
                        + hac2.sin() * (0.5 * w6 - w4b));
                if el1 < 0.4 {
                    rin = 800.0 * el1 * el1 * RINFR / 128.0;
                }
                let fmult = 4.0 - XINTR * (sigma - 0.0001);
                rin = 128.0 * fmult * rin / RINFR;
                rain = 120.0 * (eteta1 * eteta1 + eteta2 * eteta2) / rin;
            }
        }
        // Curtain array with screen, KOP=6.
        6 => {
            if !(90.0..=270.0).contains(&beta) {
                // The forward half: compute the pattern.
                rain = curtain6(st, kas, p, delta, q, sb, cb, x, el1, wave, dif, qper, qpar);
            } else {
                rain = 0.05;
            }
        }
        // Terminated sloping vee (7) and sloping rhombic (9).
        7 | 9 => {
            let mut ht = ex[0] / wave;
            if ex[0] < 0.0 {
                ht = ex[0].abs();
            }
            let deltap = if kop == 9 {
                ((ht - x) / (2.0 * el1)).asin()
            } else {
                ((ht - x) / el1).asin()
            };
            let cdelp = deltap.cos();
            let rhi = (sr / cdelp).asin();
            cr = rhi.cos();
            sr = rhi.sin();
            let cd = cb * cr + sb * sr;
            let cs = cb * cr - sb * sr;
            let ss = sb * cr + cb * sr;
            let sd = sb * cr - cb * sr;
            let scp = t * cdelp;
            let ccp = q * cdelp;
            let sdelp = deltap.sin();
            let ssp = t * sdelp;
            let csp = q * sdelp;
            let u1 = 1.0 - (csp + scp * cd);
            let u2 = 1.0 - (csp + scp * cs);
            let u3 = 1.0 - (-csp + scp * cd);
            let u4 = 1.0 - (-csp + scp * cs);
            let cp5 = ssp + ccp * cd;
            let cp6 = ssp + ccp * cs;
            let cp7 = -ssp + ccp * cd;
            let cp8 = -ssp + ccp * cs;
            let w1 = (psih - hqwave).cos();
            let w2 = (psih - hqwave).sin();
            let w3 = (psiv - hqwave).cos();
            let w4 = (psiv - hqwave).sin();
            let fu1 = fac2 * u1;
            let v1 = fu1.sin();
            let mut z1 = fu1.cos();
            let fu2 = fac2 * u2;
            let v2 = fu2.sin();
            let mut z2 = fu2.cos();
            let fu3 = fac2 * u3;
            let v3 = fu3.sin();
            let mut z3 = fu3.cos();
            let fu4 = fac2 * u4;
            let v4 = fu4.sin();
            let mut z4 = fu4.cos();
            if kop == 9 {
                let a1 = 1.0 + z1 * z2 - v1 * v2 - z1 - z2;
                let b1 = -v1 * z2 - z1 * v2 + v1 + v2;
                let a2 = 1.0 + z3 * z4 - v3 * v4 - z3 - z4;
                let b2 = -v3 * z4 - z3 * v4 + v3 + v4;
                let cm = cp8 / u2 - cp7 / u1;
                let cn = (cp5 / u3 - cp6 / u4) * cv;
                let cmp = sd / u1 - ss / u2;
                let cnp = (sd / u3 - ss / u4) * ch;
                let am = cm * a1 + cn * (a2 * w3 - b2 * w4);
                let an = cm * b1 + cn * (a2 * w4 + b2 * w3);
                let pam = cmp * a1 + cnp * (a2 * w1 - b2 * w2);
                let pan = cmp * b1 + cnp * (a2 * w2 + b2 * w1);
                rain =
                    0.05 * (am * am + an * an + cdelp * cdelp * (pam * pam + pan * pan));
                eff = -1.7;
            } else {
                z1 -= 1.0;
                z2 -= 1.0;
                z3 -= 1.0;
                z4 -= 1.0;
                let y1 = u1 * ss;
                let y3 = u3 * ss;
                let y2 = u2 * sd;
                let y4 = u4 * sd;
                let u12 = u1 * u2;
                let u34 = u3 * u4;
                if u12 == 0.0 && u34 == 0.0 {
                    // Both denominators degenerate: the source jumps
                    // straight to the dB tail with rain and eff zero.
                    rain = 0.0;
                } else {
                    let (mut a1, mut b1, mut c1, mut d1);
                    if u12 != 0.0 {
                        a1 = (u2 * cp7 * z1 - u1 * z2 * cp8) / u12;
                        b1 = (u1 * v2 * cp8 - u2 * v1 * cp7) / u12;
                        c1 = (y1 * z2 - y2 * z1) / u12;
                        d1 = (y2 * v1 - y1 * v2) / u12;
                    } else {
                        a1 = 0.0;
                        b1 = 0.0;
                        c1 = 0.0;
                        d1 = 0.0;
                    }
                    if u34 != 0.0 {
                        let a2 = u3 * z4 * cp6 - u4 * z3 * cp5;
                        let b2 = u3 * v4 * cp6 - u4 * v3 * cp5;
                        a1 += cv * (w3 * a2 + w4 * b2) / u34;
                        b1 += cv * (-b2 * w3 + w4 * a2) / u34;
                        let aa2 = y3 * z4 - y4 * z3;
                        let bb2 = y4 * v3 - y3 * v4;
                        c1 += ch * (w1 * aa2 - w2 * bb2) / u34;
                        d1 += ch * (w1 * bb2 + w2 * aa2) / u34;
                    }
                    rain = 0.05
                        * (a1 * a1 + b1 * b1 + cdelp * cdelp * (c1 * c1 + d1 * d1));
                    eff = -1.7;
                }
            }
        }
        // Inverted L, KOP=8.
        8 => {
            let sph = sb;
            let cph = cb;
            sb = q;
            cb = t;
            let wk = PI2 / wave;
            let psig = -18000.0 * sigma / freq;
            let wk2 = wk * Cf32::new(er, psig).csqrt();
            let wkok2 = Cf32::new(wk, 0.0) / wk2;
            let wk2ok = wk2 / Cf32::new(wk, 0.0);
            let mut wl = wk * el;
            if el < 0.0 {
                wl = PI2 * el.abs();
            }
            let mut wh = wk * h;
            if h < 0.0 {
                wh = PI2 * h.abs();
            }
            let wlh = wl + wh;
            let swl = wl.sin();
            let cwl = wl.cos();
            let swlh = wlh.sin();
            let _swlh2 = swlh * swlh;
            let cwlh = wlh.cos();
            let rc = (Cf32::new(1.0, 0.0) - (wkok2 * cb) * (wkok2 * cb)).csqrt();
            let rv = (Cf32::new(sb, 0.0) - wkok2 * rc) / (Cf32::new(sb, 0.0) + wkok2 * rc);
            let rh = (Cf32::new(sb, 0.0) - wk2ok * rc) / (Cf32::new(sb, 0.0) + wk2ok * rc);
            let rvab = rv.cabs();
            let rhab = rh.cabs();
            psiv = cang(rv);
            psih = cang(rh);
            let cpsiph = cb * sph;
            let psiph = cpsiph.acos();
            let spsiph = psiph.sin();
            let spsiph2 = spsiph * spsiph;
            let wb = wh * sb;
            let swb = wb.sin();
            let cwb = wb.cos();
            let a4 = cwl * cwb - sb * swl * swb - cwlh;
            let b4 = sb * swl * cwb + cwl * swb - sb * swlh;
            let ab4 = (a4 * a4 + b4 * b4).sqrt();
            let bp = if ab4 != 0.0 { b4.atan2(a4) } else { 0.0 };
            let wc = wl * cpsiph;
            let swc = wc.sin();
            let cwc = wc.cos();
            let a5 = cwc - cwl;
            let b5 = swc - cpsiph * swl;
            let ab5 = (a5 * a5 + b5 * b5).sqrt();
            let bpp = if ab5 != 0.0 { b5.atan2(a5) } else { 0.0 };
            let parv = bpp + psiv - 2.0 * wh * sb;
            let parh = bpp + psih - 2.0 * wh * sb;
            let (f11, g11, f2v, g2v);
            if spsiph2 != 0.0 {
                let dab5 = ab5 * sph * sb / spsiph2;
                f11 = dab5 * (bpp.cos() - rvab * parv.cos());
                g11 = dab5 * (bpp.sin() - rvab * parv.sin());
                let hab5 = ab5 * cph / spsiph2;
                f2v = hab5 * (bpp.cos() + rhab * parh.cos());
                g2v = hab5 * (bpp.sin() + rhab * parh.sin());
            } else {
                f11 = 0.0;
                g11 = 0.0;
                f2v = 0.0;
                g2v = 0.0;
            }
            let (f12, g12);
            if cb != 0.0 {
                let dab4 = ab4 / cb;
                f12 = -dab4 * (bp.cos() + rvab * (psiv - bp).cos());
                g12 = -dab4 * (bp.sin() + rvab * (psiv - bp).sin());
            } else {
                f12 = 0.0;
                g12 = 0.0;
            }
            let f1 = f11 + f12;
            let g1 = g11 + g12;
            let g = 30.0 * (f1 * f1 + g1 * g1 + f2v * f2v + g2v * g2v);
            let w2h = 2.0 * wh;
            let w4h = 2.0 * w2h;
            let ci2 = csz1(w2h).re;
            let ci4 = csz1(w4h).re;
            let cin2 = GAMA + w2h.ln() - ci2;
            let cin4 = GAMA + w4h.ln() - ci4;
            let si2 = -csz1(w2h).im;
            let si4 = -csz1(w4h).im;
            let cw2h = w2h.cos();
            let sw2h = w2h.sin();
            let vrt =
                30.0 * ((1.0 + cw2h) * cin2 - 0.5 * cw2h * cin4 - sw2h * (si2 - 0.5 * si4));
            let mut rin = vrt;
            if x < 0.2 {
                rin = 400.0 * x * x * RINTW / 16.0;
            }
            let fmult = 4.0 - 3.0 * (sigma - 0.0001) / (5.0 - 0.0001);
            rin = 16.0 * fmult * rin / RINTW;
            rain = g / rin;
            if x <= 0.20 {
                eff =
                    20.0 * (x * (6.335 + x * (67.95 - x * (693.0 - x * 1600.0)))).log10();
            }
        }
        // KOP=10 is not used for HFMUFES gains; the source STOPs.
        10 => panic!("Antenna type 10 not used for HFMUFES GAIN."),
        // Sloping long wire, KOP=11.
        11 => {
            let cfac2 = fac2.cos();
            let sfac2 = fac2.sin();
            let w1 = (psih - hqwave).cos();
            let w2 = (psih - hqwave).sin();
            let w3 = (psiv - hqwave).cos();
            let w4 = (psiv - hqwave).sin();
            let crb = cr * cb * q;
            let srt = sr * t;
            let cbs = cr * sb;
            let srq = q * sr;
            let cphi = srq + t * cr * cb;
            let sphi2 = 1.0 - cphi * cphi;
            let cphip = -srq + t * cr * cb;
            let sphip2 = 1.0 - cphip * cphip;
            rain = 0.0;
            if sphi2 != 0.0 || sphip2 != 0.0 {
                let (mut ethet1, mut ethet2, mut ephi1, mut ephi2);
                if sphi2 != 0.0 {
                    let cig = ((fac2 * cphi).cos() - cfac2) / sphi2;
                    ephi1 = -cbs * cig;
                    ethet1 = (crb - srt) * cig;
                    let sig = ((fac2 * cphi).sin() - cphi * sfac2) / sphi2;
                    ethet2 = (crb - srt) * sig;
                    ephi2 = -cbs * sig;
                } else {
                    ethet1 = 0.0;
                    ethet2 = 0.0;
                    ephi1 = 0.0;
                    ephi2 = 0.0;
                }
                if sphip2 != 0.0 {
                    let cigp = ((fac2 * cphip).cos() - cfac2) / sphip2;
                    let sigp = ((fac2 * cphip).sin() - cphip * sfac2) / sphip2;
                    ethet1 -= (crb + srt) * cv * (w3 * cigp - w4 * sigp);
                    ethet2 -= (crb + srt) * cv * (w4 * cigp + w3 * sigp);
                    ephi1 -= cbs * ch * (w1 * cigp - w2 * sigp);
                    ephi2 -= cbs * ch * (w2 * cigp + w1 * sigp);
                }
                if kas <= 1 {
                    let azh = csz1(2.0 * fac4);
                    let w5 = azh.re;
                    let w6 = azh.im;
                    let azh = csz1(fac4);
                    let w33 = azh.re;
                    let w4b = azh.im;
                    let flog = fac2.ln() + GAMA;
                    st.rin = 30.0
                        * (0.5 * (flog - w5)
                            + 0.6931471806
                            + cfac2
                                * (cfac2 * (flog - 2.0 * w33 + w5) - sfac2 * (w6 - 2.0 * w4b)));
                }
                rain = 30.0
                    * (ethet1 * ethet1 + ethet2 * ethet2 + ephi1 * ephi1 + ephi2 * ephi2)
                    / st.rin;
            }
        }
        // General horizontal log-periodic, KOP=13.
        13 => {
            if kas <= 1 {
                let yz = 1.0 / ex[0];
                let n = ex[3] as usize;
                st.n = n;
                st.ell[n] = fac;
                st.xk[n] = st.ell[n] * (1.0 / (ex[1] * D2R).tan());
                for ii in 1..n {
                    let nii = n - ii;
                    let nip = nii + 1;
                    st.ell[nii] = st.ell[nip] * ex[2];
                    st.xk[nii] = st.xk[nip] * ex[2];
                }
                let mut tx = [[0.0 as R; 20]; 20];
                let mut ty = [[0.0 as R; 20]; 20];
                let mut yi = [[0.0 as R; 20]; 20];
                let nmx = n - 1;
                for i in 1..=nmx {
                    let mid = i + 1;
                    if i != 1 {
                        yi[i - 1][i - 1] = -yz
                            * (1.0 / (st.xk[mid] - st.xk[i]).tan()
                                + 1.0 / (st.xk[i] - st.xk[i - 1]).tan());
                    }
                    yi[i - 1][mid - 1] = -yz / (st.xk[mid] - st.xk[i]).sin();
                    yi[mid - 1][i - 1] = yi[i - 1][mid - 1];
                }
                yi[0][0] = -yz * (1.0 / (st.xk[2] - st.xk[1]).tan());
                let cot = 1.0 / (yz * (1.0 / (st.ell[n] / 2.0).tan()));
                let zts: R = 0.0;
                let ta = zts * zts + cot * cot;
                let yrn = zts / ta;
                yi[n - 1][n - 1] = -(cot / ta + yz * (1.0 / (st.xk[n] - st.xk[nmx]).tan()));
                let konst = SQRTWO / 177.0;
                let mut z = [[Cf32::ZERO; 21]; 21];
                let mut d1d = [0.0 as R; 21];
                let mut zs = [Cf32::ZERO; 21];
                for i in 1..=n {
                    let jend = i;
                    d1d[jend] = st.ell[jend] * konst;
                    for jl in 1..jend {
                        d1d[jl] = (st.xk[jend] - st.xk[jl]).abs();
                    }
                    sim(&d1d, &st.ell, jend, jend, &mut zs);
                    for jk in 1..=jend {
                        z[jk][jend] = zs[jk];
                        if jk != jend {
                            z[jend][jk] = zs[jk];
                        }
                    }
                }
                for j in 1..=n {
                    for i in 1..=n {
                        for k in 1..=n {
                            tx[i - 1][j - 1] -= yi[k - 1][i - 1] * z[k][j].im;
                            ty[i - 1][j - 1] += yi[k - 1][i - 1] * z[k][j].re;
                            if i == n && k == n {
                                tx[n - 1][j - 1] += yrn * z[n][j].re;
                                ty[n - 1][j - 1] += yrn * z[n][j].im;
                            }
                        }
                    }
                }
                for i in 0..n {
                    tx[i][i] += 1.0;
                }
                let (yr2, yi2) = cmpinv(&mut tx, &mut ty, n);
                let mut sum0: R = 0.0;
                for i in 1..=n {
                    st.cix[i] = yr2[i - 1][0];
                    st.ciy[i] = yi2[i - 1][0];
                    sum0 += yr2[i - 1][0] * z[i][1].re - yi2[i - 1][0] * z[i][1].im;
                }
                let mut sumd: R = 0.0;
                d1d[1] = 2.0 * hwave;
                let th = d1d[1] * d1d[1];
                let txs = 4.0 * hwave * cr;
                for nj in 2..=n {
                    sumd += st.xk[nj] - st.xk[nj - 1];
                    d1d[nj] = (th + sumd * sumd + txs * sumd).sqrt();
                }
                sim(&d1d, &st.ell, 1, n, &mut zs);
                // The source's DO 545 loop reads index I, which is
                // N+1 left over from the SUM0 loop: every term uses
                // the zeroed row past the matrix, so V is always zero
                // and SUM2 contributes nothing. Kept as written.
                let v = Cf32::ZERO;
                let sqrd = dif.csqrt();
                let sum2 =
                    (v * (Cf32::new(1.0, 0.0) - sqrd) / (Cf32::new(1.0, 0.0) + sqrd)).re;
                st.rin = sum0 + sum2;
            }
            let n = st.n;
            let cpsi = t * sb;
            let spsi2 = 1.0 - cpsi * cpsi;
            rain = 0.0;
            if spsi2 != 0.0 {
                let mut etr: R = 0.0;
                let mut eti: R = 0.0;
                let mut epr: R = 0.0;
                let mut epi: R = 0.0;
                let cq2a = cr * q;
                let cbeta = cq2a - t * cb * sr;
                let cq2 = 2.0 * cq2a;
                for j in 1..=n {
                    let arg5 = cq2 * st.xk[j];
                    let cr5 = arg5.cos();
                    let sr5 = arg5.sin();
                    let ccb = (st.xk[j] * cbeta).cos() / st.ell[j].sin();
                    let scb = (st.xk[j] * cbeta).sin() / st.ell[j].sin();
                    let tt = (st.ell[j] * cpsi).cos() - st.ell[j].cos();
                    let a = (st.cix[j] * ccb - st.ciy[j] * scb) * tt;
                    let bb = (st.ciy[j] * ccb + st.cix[j] * scb) * tt;
                    let w1 = ch * (psih - hqwave).cos();
                    let w2 = ch * (psih - hqwave).sin();
                    let w3 = cv * (psiv - hqwave).cos();
                    let w4 = cv * (psiv - hqwave).sin();
                    let c = 1.0 - (w3 * cr5 + w4 * sr5);
                    let dd = w4 * cr5 - w3 * sr5;
                    etr += a * c + bb * dd;
                    eti += bb * c - a * dd;
                    let cc = 1.0 + (w1 * cr5 + w2 * sr5);
                    let dc = w2 * cr5 - w1 * sr5;
                    epr += a * cc - bb * dc;
                    epi += bb * cc + a * dc;
                }
                let etmag = (etr * etr + eti * eti) * (q * sb / spsi2).powi(2);
                let epmag = (epr * epr + epi * epi) * (cb / spsi2).powi(2);
                rain = 120.0 * (etmag + epmag) / st.rin;
            }
        }
        // Arbitrary tilted dipole, KOP=14.
        14 => {
            let cfac = fac.cos();
            let w1 = (psih - hqwave).cos();
            let w2 = (psih - hqwave).sin();
            let w3 = (psiv - hqwave).cos();
            let w4 = (psiv - hqwave).sin();
            let tip = 0.5 * el1 * sr;
            assert!(tip <= x, "TIP > X in HUMUFES GAIN calculations.");
            let csb = cr * sb;
            let cphi = q * sr + t * csb;
            let sphi2 = 1.0 - cphi * cphi;
            let cphip = -q * sr + t * csb;
            let sphip2 = 1.0 - cphip * cphip;
            let (mut etheta1, mut ephi1);
            if sphi2 != 0.0 {
                let gi = ((fac * cphi).cos() - cfac) / sphi2;
                etheta1 = (csb * q - sr * t) * gi;
                ephi1 = cr * cb * gi;
            } else {
                etheta1 = 0.0;
                ephi1 = 0.0;
            }
            let (etheta2, ephi2);
            if sphip2 != 0.0 {
                let di = ((fac * cphip).cos() - cfac) / sphip2;
                etheta1 -= (csb * q + sr * t) * di * cv * w3;
                ephi1 += di * ch * w1 * cr * cb;
                etheta2 = -(csb * q + sr * t) * di * cv * w4;
                ephi2 = cr * cb * di * ch * w2;
            } else {
                etheta2 = 0.0;
                ephi2 = 0.0;
            }
            if kas <= 1 {
                // The mutual-impedance state, /MUT/.
                let cfac_m = fac.cos();
                let mut y0 = ratio * el1;
                let h2 = 0.5 * el1;
                let mut rhi2: R = 0.0;
                let mut z0: R = 0.0;
                let (r11, _) = mutual(cfac_m, h2, rhi2, y0, z0);
                y0 = 2.0 * x * cr;
                z0 = 2.0 * x * sr;
                rhi2 = 2.0 * rhi;
                let (r21, x21) = mutual(cfac_m, h2, rhi2, y0, z0);
                let mut zm = Cf32::new(r21, x21);
                if rhi2 > PIO2 {
                    zm = -zm;
                }
                let sqrd = dif.csqrt();
                let cxc = (zm
                    * (((Cf32::new(1.0, 0.0) - sqrd) / (Cf32::new(1.0, 0.0) + sqrd)) * cr
                        + Cf32::new(0.0, 1.0) * ((dif - sqrd) / (dif + sqrd)) * sr)
                    * Cf32::new(cr, -sr))
                .re;
                st.rin = r11 + cxc;
            }
            rain = 120.0
                * (etheta1 * etheta1 + etheta2 * etheta2 + ephi1 * ephi1 + ephi2 * ephi2)
                / st.rin;
        }
        // Half rhombic, KOP=15.
        15 => {
            let w1 = psih.cos();
            let w2 = psih.sin();
            let w3 = psiv.cos();
            let w4 = psiv.sin();
            let tt = q * sr;
            let ts = 1.0 - t * cr * cb;
            let tt2 = ts + tt;
            let ts2 = fac2 * tt2;
            let sts4 = ts2.sin() / tt2;
            let cts4 = (1.0 - ts2.cos()) / tt2;
            let tt1 = ts - tt;
            let ts1 = fac2 * tt1;
            let sts1 = ts1.sin();
            let cts1 = ts1.cos();
            let r1 = (1.0 - cts1) / tt1;
            let fi1 = sts1 / tt1;
            let r4 = (1.0 - cts1) * (fac4 * sr * q).cos() + sts1 * (fac4 * sr * q).sin();
            let fi4 = sts1 * (fac4 * sr * q).cos() - (fac4 * sr * q).sin() * (1.0 - cts1);
            let r2 = cts4 * cts1 + sts4 * sts1;
            let fi2 = cts1 * sts4 - cts4 * sts1;
            let f4c = (fi4 * cts1 - r4 * sts1) / tt1;
            let r4c = (r4 * cts1 + fi4 * sts1) / tt1;
            let rb = r1 + r2 - ((cts4 + r4c) * w3 - (sts4 + f4c) * w4) * cv;
            let bi = fi1 + fi2 - ((cts4 + r4c) * w4 + (sts4 + f4c) * w3) * cv;
            let rc = -r1 + r2 + ((-cts4 + r4c) * w3 - (-sts4 + f4c) * w4) * cv;
            let cc = -fi1 + fi2 + ((-cts4 + r4c) * w4 + (-sts4 + f4c) * w3) * cv;
            let ra = r1 + r2 + ((cts4 + r4c) * w1 - (sts4 + f4c) * w2) * ch;
            let ai = fi1 + fi2 + ((cts4 + r4c) * w2 + (sts4 + f4c) * w1) * ch;
            let em1 = (cr * cb * q * rb + sr * t * rc).powi(2)
                + (cr * cb * q * bi + sr * t * cc).powi(2);
            let enn1 = (cr * sb * ra).powi(2) + (cr * sb * ai).powi(2);
            rain = 0.1 * (enn1 + em1);
            eff = -1.7;
        }
        // Sloping double rhomboid, KOP=16.
        16 => {
            rain = rhomboid(
                p, q, t, sb, cb, sr, cr, cv, ch, psiv, psih, hqwave, fac2, fac4, wave, x, el1,
            );
            eff = -1.7;
        }
        _ => {
            rain = 0.0;
        }
    }

    // Label 615: into dB, per-type gain offsets, the floor, and the
    // efficiency fold.
    if rain <= RAIN_MIN {
        rain = RAIN_MIN;
    }
    rain = 10.0 * rain.log10();
    if kop == 2 {
        rain += h;
    }
    if kop == 3 || kop == 5 {
        rain += phi;
    }
    if rain < FLOOR {
        rain = FLOOR;
    }
    let mut raine = rain + eff;
    if raine < FLOOR {
        raine = FLOOR;
    }
    (raine, eff)
}

/// `MUTUAL`: mutual impedance between arbitrarily oriented dipoles by
/// Gaussian integration. Returns `(r21, x21)`.
fn mutual(cfac: R, h2: R, rhi2: R, y0: R, z0: R) -> (R, R) {
    let xcon = -0.1 * VOFL;
    let ct = rhi2.cos();
    let prod1 = -rhi2.sin();
    let integrand = |trig: fn(R) -> R| {
        move |s: R| -> R {
            let sz = s * ct;
            let sy = s * prod1;
            let term = y0 + sy;
            let rho2 = term * term;
            let ca = z0 + sz;
            let ca1 = ca + h2;
            let ca2 = ca - h2;
            let r = (rho2 + ca * ca).sqrt();
            let r1 = (rho2 + ca1 * ca1).sqrt();
            let r2 = (rho2 + ca2 * ca2).sqrt();
            let sr = trig(PI2 * r) / r;
            let facr = 2.0 * cfac * sr;
            let sr1 = trig(PI2 * r1) / r1;
            let sr2 = trig(PI2 * r2) / r2;
            (((sr1 * ca1 + sr2 * ca2 - facr * ca) * sy) / term + (facr - sr1 - sr2) * sz)
                * (PI2 * (h2 - s.abs())).sin()
                / s
        }
    };
    let mut x21 = 0.0;
    if y0 > 0.005 {
        x21 = xcon * agauss(&integrand(|v: R| v.cos()), h2);
    }
    let r21 = xcon * agauss(&integrand(|v: R| v.sin()), h2);
    (r21, x21)
}

/// The curtain with screen, KOP=6: the full self- and mutual
/// impedance sum over every real and image element pair, then the
/// element, bay and screen arraying factors.
#[allow(clippy::too_many_arguments)]
fn curtain6(
    st: &mut MufesState,
    kas: i32,
    p: &MufesParams,
    delta: R,
    q: R,
    sb: R,
    cb: R,
    x: R,
    el1: R,
    wave: R,
    dif: Cf32,
    qper: Cf32,
    qpar: Cf32,
) -> R {
    let phi = p.and;
    let ex = p.aex;
    let thetaz: R = 90.0;
    let deltap: R = 0.0;
    let eil = 0.5 * el1;
    let nb = phi.abs() as i32;
    let mut nbb = ((100.0 * (phi.abs() - nb as R)).abs() + 0.5) as i32;
    // Non-integer bay counts feed the bays in anti-phase mod the
    // fractional digits; same for elements below.
    let cbay: R = if nbb != 0 { -1.0 } else { 1.0 };
    if nbb == 0 {
        nbb = 1;
    }
    let ne = ex[0].abs() as i32;
    let mut nee = ((100.0 * (ex[0].abs() - ne as R)).abs() + 0.5) as i32;
    let cele: R = if nee != 0 { -1.0 } else { 1.0 };
    if nee == 0 {
        nee = 1;
    }
    let mut dy = ex[1] / wave;
    if ex[1] < 0.0 {
        dy = ex[1].abs();
    }
    let mut dz = ex[2] / wave;
    if ex[2] < 0.0 {
        dz = ex[2].abs();
    }
    let mut dx = ex[3] / wave;
    if ex[3] < 0.0 {
        dx = ex[3].abs();
    }

    let nb_u = nb.clamp(1, 5) as usize;
    let ne_u = ne.clamp(1, 5) as usize;

    if kas <= 1 {
        let mut cur = Cur {
            dij: 0.01767766952 * eil,
            eil,
            hij: eil,
            kode: 1,
        };
        st.kode = 1;
        let rzz_first = zmut(&cur);
        // Ma (1974) table 4.1 p. 254: a full-wavelength element hits
        // the resonance ceiling and switches normalisation.
        let rzz;
        if rzz_first.re >= 3631.53 {
            rzz = Cf32::new(3631.53, -2356.47);
            st.kode = -1;
            st.rin = (nb * ne) as R * 3631.53;
            let _ = rzz;
        } else {
            rzz = rzz_first;
            let mut rdzz = [Cf32::ZERO; 6];
            let mut rtzz = [Cf32::ZERO; 6];
            let mut rzb = [Cf32::ZERO; 6];
            let mut rpzb = [Cf32::ZERO; 6];
            let mut rdzb = [Cf32::ZERO; 11];
            let mut rtzb = [Cf32::ZERO; 11];
            let mut rze = [Cf32::ZERO; 6];
            let mut rpze = [Cf32::ZERO; 6];
            let mut rdze = [[Cf32::ZERO; 6]; 6];
            let mut rtze = [[Cf32::ZERO; 6]; 6];
            let mut rr = [[Cf32::ZERO; 6]; 6];
            let mut rp = [[Cf32::ZERO; 6]; 6];
            let mut rdz = [[Cf32::ZERO; 11]; 6];
            let mut rt = [[Cf32::ZERO; 11]; 6];

            cur.dij = 2.0 * dx;
            let rpzz = zmut(&cur);
            for i in 1..=ne_u {
                let ci = i as R;
                let tt = (ci - 1.0) * dz + x;
                cur.dij = 2.0 * tt;
                rdzz[i] = zmut(&cur);
                cur.dij = 2.0 * (dx * dx + tt * tt).sqrt();
                rtzz[i] = zmut(&cur);
            }
            let ts = (2.0 * dx) * (2.0 * dx);
            let ijend = ne_u - 1;
            for ij in 1..=ijend {
                let cij = ij as R;
                cur.dij = cij * dz;
                rzb[ij] = zmut(&cur);
                cur.dij = (ts + (cij * cij) * (dz * dz)).sqrt();
                rpzb[ij] = zmut(&cur);
            }
            let ipjend = 2 * ne_u;
            for ipj in 2..=ipjend {
                let cipj = ipj as R;
                let tt = (cipj - 2.0) * dz + 2.0 * x;
                cur.dij = tt;
                rdzb[ipj] = zmut(&cur);
                cur.dij = (ts + tt * tt).sqrt();
                rtzb[ipj] = zmut(&cur);
            }
            let mnend = nb_u - 1;
            for mn in 1..=mnend {
                let cmn = mn as R;
                cur.hij = cmn * dy - eil;
                rze[mn] = coll(&cur);
                cur.dij = 2.0 * dx;
                rpze[mn] = ech(&cur);
                for i in 1..=ne_u {
                    let ci = i as R;
                    let tt = 2.0 * ((ci - 1.0) * dz + x);
                    cur.dij = tt;
                    rdze[mn][i] = ech(&cur);
                    cur.dij = (ts + tt * tt).sqrt();
                    rtze[mn][i] = ech(&cur);
                }
                for ij in 1..=ijend {
                    let cij = ij as R;
                    cur.dij = cij * dz;
                    rr[mn][ij] = ech(&cur);
                    cur.dij = (ts + (cij * cij) * (dz * dz)).sqrt();
                    rp[mn][ij] = ech(&cur);
                }
                for ipj in 2..=ipjend {
                    let cipj = ipj as R;
                    let tt = (cipj - 2.0) * dz + 2.0 * x;
                    cur.dij = tt;
                    rdz[mn][ipj] = ech(&cur);
                    cur.dij = (ts + tt * tt).sqrt();
                    rt[mn][ipj] = ech(&cur);
                }
            }

            let mut zsum1 = Cf32::ZERO;
            let mut zsum2 = Cf32::ZERO;
            let mut zsum3 = Cf32::ZERO;
            let mut zsum4 = Cf32::ZERO;
            for m in 1..=nb_u {
                for i in 1..=ne_u {
                    for n in 1..=nb_u {
                        for j in 1..=ne_u {
                            let (z1, z2, z3, z4) = if m == n && i == j {
                                (rzz, rpzz, rdzz[i], rtzz[i])
                            } else if m == n {
                                let ij = i.abs_diff(j);
                                (rzb[ij], rpzb[ij], rdzb[i + j], rtzb[i + j])
                            } else if i == j {
                                let mn = m.abs_diff(n);
                                (rze[mn], rpze[mn], rdze[mn][i], rtze[mn][i])
                            } else {
                                let mn = m.abs_diff(n);
                                let ij = i.abs_diff(j);
                                (rr[mn][ij], rp[mn][ij], rdz[mn][i + j], rt[mn][i + j])
                            };
                            zsum1 = zsum1 + z1;
                            zsum2 = zsum2 + z2;
                            zsum3 = zsum3 + z3;
                            zsum4 = zsum4 + z4;
                        }
                    }
                }
            }
            let sqrd = dif.csqrt();
            let rhcp = (Cf32::new(1.0, 0.0) - sqrd) / (Cf32::new(1.0, 0.0) + sqrd);
            st.rin = (zsum1 - zsum2 + rhcp * (zsum3 - zsum4)).re;
        }
    }

    // The element arraying factor; sin(theta) = cos(delta).
    let stheta = delta.cos();
    let cpsi = stheta * sb;
    let mut azv = Cf32::ZERO;
    let mut azh = Cf32::ZERO;
    let cthetaz = (thetaz * D2R).cos();
    let sbz = (deltap * D2R).sin();
    let mut factor = cele;
    for m in 1..=ne_u {
        let em = m as R;
        let zm = x + (em - 1.0) * dz;
        if m as i32 % nee == 1 || nee == 1 {
            factor *= cele;
        }
        let tt = PI2 * zm * (q - cthetaz);
        let zt = Cf32::new(tt.cos(), tt.sin());
        let tt = 2.0 * PI2 * zm * q;
        let ztr = Cf32::new(tt.cos(), -tt.sin());
        azv = azv + zt * (Cf32::new(1.0, 0.0) - qper * ztr) * factor;
        azh = azh + zt * (Cf32::new(1.0, 0.0) + qpar * ztr) * factor;
    }
    // The bay arraying factor.
    let mut af = Cf32::new(1.0, 0.0);
    if nb_u > 1 {
        af = Cf32::ZERO;
        let mut factor = cbay;
        for n in 1..=nb_u {
            let en = n as R;
            if n as i32 % nbb == 1 || nbb == 1 {
                factor *= cbay;
            }
            let tt = PI2 * dy * (en - 1.0);
            let ts2 = cpsi - stheta * sbz;
            let zt = Cf32::new((tt * ts2).cos(), (tt * ts2).sin());
            af = af + zt * factor;
        }
    }
    // The real-image (screen) arraying factor and the gain.
    let tt = (PI2 * dx * stheta * cb).sin();
    // SB**2*Q**2*AZV*CONJG(AZV): the scalar scales AZV first, then the
    // complex product with the conjugate.
    let zt = ((sb * sb * (q * q)) * azv * azv.conj() + (cb * cb) * azh * azh.conj()) * (tt * tt);
    let spsi2 = 1.0 - cpsi * cpsi;
    if spsi2 == 0.0 {
        return 0.0;
    }
    let ttx = ((PI2 * eil * cpsi).cos() - (PI2 * eil).cos()) / spsi2;
    // ZT*AF*CONJG(AF)*TT**2/RIN, left to right.
    let train = (zt * af * af.conj() * (ttx * ttx) / st.rin).re;
    if st.kode >= 0 {
        480.0 * (1.0 / (PI2 * eil).sin()).powi(2) * train
    } else {
        // Ma (1974) eqn (4.114) p. 273: the full-wavelength current.
        let ctu = Cf32::new(-0.0419290, 0.0461374);
        let ctd = Cf32::new(-0.0184019, 0.0612938);
        let curnt = Cf32::new((PI2 * eil).sin(), 0.0)
            + ctu * (1.0 - (PI2 * eil).cos())
            + ctd * (1.0 - (PI * eil).cos());
        (Cf32::new(480.0 * train, 0.0) / (curnt * curnt.conj())).re
    }
}

/// KOP=16, the sloping double rhomboid: a single long closed-form
/// expression.
#[allow(clippy::too_many_arguments)]
fn rhomboid(
    p: &MufesParams,
    q: R,
    t: R,
    sb: R,
    cb: R,
    sr: R,
    cr: R,
    cv: R,
    ch: R,
    psiv: R,
    psih: R,
    hqwave: R,
    fac2: R,
    fac4: R,
    wave: R,
    x: R,
    el1: R,
) -> R {
    let ex = p.aex;
    let mut el2 = ex[2] / wave;
    if ex[2] < 0.0 {
        el2 = ex[2].abs();
    }
    let fak = PI2 * el2;
    let mut ht = ex[3] / wave;
    if ex[3] < 0.0 {
        ht = ex[3].abs();
    }
    let del = ((ht - x) / (el1 + el2)).asin();
    let w1 = ch * (psih - hqwave).cos();
    let w2 = ch * (psih - hqwave).sin();
    let w3 = cv * (psiv - hqwave).cos();
    let w4 = cv * (psiv - hqwave).sin();
    let cdel = del.cos();
    let sdel = del.sin();
    let sx1 = (ex[0] * D2R).sin();
    let bp1 = (sx1 / cdel).asin();
    let cp1 = bp1.cos();
    let sp1 = bp1.sin();
    let sx2 = (ex[1] * D2R).sin();
    let bp2 = (sx2 / cdel).asin();
    let cp2 = bp2.cos();
    let sp2 = bp2.sin();
    let rp1m = ((cb * cp1 - sb * sp1) * cr + (sb * cp1 + cb * sp1) * sr) * cdel;
    let rp2p = ((cb * cp2 - sb * sp2) * cr - (sb * cp2 + cb * sp2) * sr) * cdel;
    let rm2m = ((cb * cp2 + sb * sp2) * cr + (sb * cp2 - cb * sp2) * sr) * cdel;
    let rm1p = ((cb * cp1 + sb * sp1) * cr - (sb * cp1 - cb * sp1) * sr) * cdel;
    let argl1 = fac4 * sdel * q;
    let sl1 = argl1.sin();
    let cl1 = (1.0 - sl1 * sl1).sqrt();
    let w1h1 = w1 * cl1 + w2 * sl1;
    let w2h1 = w2 * cl1 - w1 * sl1;
    let w3h1 = w3 * cl1 + w4 * sl1;
    let w4h1 = w4 * cl1 - w3 * sl1;
    let argl2 = 2.0 * fak * sdel * q;
    let sl2 = argl2.sin();
    let cl2 = (1.0 - sl2 * sl2).sqrt();
    let w1h2 = w1 * cl2 + w2 * sl2;
    let w2h2 = w2 * cl2 - w1 * sl2;
    let w3h2 = w3 * cl2 + w4 * sl2;
    let w4h2 = w4 * cl2 - w3 * sl2;
    let u1 = 1.0 - (q * sdel + t * rp1m);
    let u2 = 1.0 - (q * sdel + t * rp2p);
    let u3 = 1.0 - (q * sdel + t * rm2m);
    let u4 = 1.0 - (q * sdel + t * rm1p);
    let c11 = (fac2 * u1).cos();
    let s11 = (fac2 * u1).sin();
    let c22 = (fak * u2).cos();
    let s22 = (fak * u2).sin();
    let c23 = (fak * u3).cos();
    let s23 = (fak * u3).sin();
    let c14 = (fac2 * u4).cos();
    let s14 = (fac2 * u4).sin();
    let u1g = 1.0 / (1.0 + q * sdel - t * rp1m);
    let u2g = 1.0 / (1.0 + q * sdel - t * rp2p);
    let u3g = 1.0 / (1.0 + q * sdel - t * rm2m);
    let u4g = 1.0 / (1.0 + q * sdel - t * rm1p);
    let vr1 = (1.0 - c11) / u1;
    let vi1 = s11 / u1;
    let vr2 = (1.0 - c22) / u2;
    let vi2 = s22 / u2;
    let vr3 = (1.0 - c23) / u3;
    let vi3 = s23 / u3;
    let vr4 = (1.0 - c14) / u4;
    let vi4 = s14 / u4;
    let vr1g = (w3 * (1.0 - c11) - w4 * s11) * u1g;
    let vr2g = (w3 * (1.0 - c22) - w4 * s22) * u2g;
    let vr3g = (w3 * (1.0 - c23) - w4 * s23) * u3g;
    let vr4g = (w3 * (1.0 - c14) - w4 * s14) * u4g;
    let vr5g = (w3h2 * (1.0 - c11) - w4h2 * s11) * u1g;
    let vr6g = (w3h1 * (1.0 - c22) - w4h1 * s22) * u2g;
    let vr7g = (w3h1 * (1.0 - c23) - w4h1 * s23) * u3g;
    let vr8g = (w3h2 * (1.0 - c14) - w4h2 * s14) * u4g;
    let vi1g = (w3 * s11 + w4 * (1.0 - c11)) * u1g;
    let vi2g = (w3 * s22 + w4 * (1.0 - c22)) * u2g;
    let vi3g = (w3 * s23 + w4 * (1.0 - c23)) * u3g;
    let vi4g = (w3 * s14 + w4 * (1.0 - c14)) * u4g;
    let vi5g = (w3h2 * s11 + w4h2 * (1.0 - c11)) * u1g;
    let vi6g = (w3h1 * s22 + w4h1 * (1.0 - c22)) * u2g;
    let vi7g = (w3h1 * s23 + w4h1 * (1.0 - c23)) * u3g;
    let vi8g = (w3h2 * s14 + w4h2 * (1.0 - c14)) * u4g;
    let vr1h = (w1 * (1.0 - c11) - w2 * s11) * u1g;
    let vr2h = (w1 * (1.0 - c22) - w2 * s22) * u2g;
    let vr3h = (w1 * (1.0 - c23) - w2 * s23) * u3g;
    let vr4h = (w1 * (1.0 - c14) - w2 * s14) * u4g;
    let vr5h = (w1h2 * (1.0 - c11) - w2h2 * s11) * u1g;
    let vr6h = (w1h1 * (1.0 - c22) - w2h1 * s22) * u2g;
    let vr7h = (w1h1 * (1.0 - c23) - w2h1 * s23) * u3g;
    let vr8h = (w1h2 * (1.0 - c14) - w2h2 * s14) * u4g;
    let vi1h = (w1 * s11 + w2 * (1.0 - c11)) * u1g;
    let vi2h = (w1 * s22 + w2 * (1.0 - c22)) * u2g;
    let vi3h = (w1 * s23 + w2 * (1.0 - c23)) * u3g;
    let vi4h = (w1 * s14 + w2 * (1.0 - c14)) * u4g;
    let vi5h = (w1h2 * s11 + w2h2 * (1.0 - c11)) * u1g;
    let vi6h = (w1h1 * s22 + w2h1 * (1.0 - c22)) * u2g;
    let vi7h = (w1h1 * s23 + w2h1 * (1.0 - c23)) * u3g;
    let vi8h = (w1h2 * s14 + w2h2 * (1.0 - c14)) * u4g;
    let e1r = (vr1 - vr1g) * q * rp1m - (vr1 + vr1g) * sdel * t;
    let e1i = (vi1 - vi1g) * q * rp1m - (vi1 + vi1g) * sdel * t;
    let e2r = (vr2 - vr2g) * q * rp2p - (vr2 + vr2g) * sdel * t;
    let e2i = (vi2 - vi2g) * q * rp2p - (vi2 + vi2g) * sdel * t;
    let e3r = -(vr3 - vr3g) * q * rm2m + (vr3 + vr3g) * sdel * t;
    let e3i = -(vi3 - vi3g) * q * rm2m + (vi3 + vi3g) * sdel * t;
    let e4r = -(vr4 - vr4g) * q * rm1p + (vr4 + vr4g) * sdel * t;
    let e4i = -(vi4 - vi4g) * q * rm1p + (vi4 + vi4g) * sdel * t;
    let mut e5r = -c23 * ((vr1 - vr5g) * q * rp1m - (vr1 + vr5g) * sdel * t);
    e5r -= s23 * ((vi1 - vi5g) * q * rp1m - (vi1 + vi5g) * sdel * t);
    let mut e5i = -c23 * ((vi1 - vi5g) * q * rp1m - (vi1 + vi5g) * sdel * t);
    e5i += s23 * ((vr1 - vr5g) * q * rp1m - (vr1 + vr5g) * sdel * t);
    let mut e6r = -c14 * ((vr2 - vr6g) * q * rp2p - (vr2 + vr6g) * sdel * t);
    e6r -= s14 * ((vi2 - vi6g) * q * rp2p - (vi2 + vi6g) * sdel * t);
    let mut e6i = -c14 * ((vi2 - vi6g) * q * rp2p - (vi2 + vi6g) * sdel * t);
    e6i += s14 * ((vr2 - vr6g) * q * rp2p - (vr2 + vr6g) * sdel * t);
    let mut e7r = c11 * ((vr3 - vr7g) * q * rm2m - (vr3 + vr7g) * sdel * t);
    e7r += s11 * ((vi3 - vi7g) * q * rm2m - (vi3 + vi7g) * sdel * t);
    let mut e7i = c11 * ((vi3 - vi7g) * q * rm2m - (vi3 + vi7g) * sdel * t);
    e7i -= s11 * ((vr3 - vr7g) * q * rm2m - (vr3 + vr7g) * sdel * t);
    let mut e8r = c22 * ((vr4 - vr8g) * q * rm1p - (vr4 + vr8g) * sdel * t);
    e8r += s22 * ((vi4 - vi8g) * q * rm1p - (vi4 + vi8g) * sdel * t);
    let mut e8i = c22 * ((vi4 - vi8g) * q * rm1p - (vi4 + vi8g) * sdel * t);
    e8i -= s22 * ((vr4 - vr8g) * q * rm1p - (vr4 + vr8g) * sdel * t);
    let ethr = e1r + e2r + e3r + e4r + e5r + e6r + e7r + e8r;
    let ethi = e1i + e2i + e3i + e4i + e5i + e6i + e7i + e8i;
    let sp1m = (sb * cp1 + cb * sp1) * cr - (cb * cp1 - sb * sp1) * sr;
    let sp2p = (sb * cp2 + cb * sp2) * cr + (cb * cp2 - sb * sp2) * sr;
    let sm2m = (sb * cp2 - cb * sp2) * cr - (cb * cp2 + sb * sp2) * sr;
    let sm1p = (sb * cp1 - cb * sp1) * cr + (cb * cp1 + sb * sp1) * sr;
    let p1r = -(vr1 + vr1h) * sp1m;
    let p1i = -(vi1 + vi1h) * sp1m;
    let p2r = -(vr2 + vr2h) * sp2p;
    let p2i = -(vi2 + vi2h) * sp2p;
    let p3r = (vr3 + vr3h) * sm2m;
    let p3i = (vi3 + vi3h) * sm2m;
    let p4r = (vr4 + vr4h) * sm1p;
    let p4i = (vi4 + vi4h) * sm1p;
    let p5r = ((vr1 + vr5h) * c23 + (vi1 + vi5h) * s23) * sp1m;
    let p5i = ((vi1 + vi5h) * c23 - (vr1 + vr5h) * s23) * sp1m;
    let p6r = ((vr2 + vr6h) * c14 + (vi2 + vi6h) * s14) * sp2p;
    let p6i = ((vi2 + vi6h) * c14 - (vr2 + vr6h) * s14) * sp2p;
    let p7r = -((vr3 + vr7h) * c11 + (vi3 + vi7h) * s11) * sm2m;
    let p7i = -((vi3 + vi7h) * c11 - (vr3 + vr7h) * s11) * sm2m;
    let p8r = -((vr4 + vr8h) * c22 + (vi4 + vi8h) * s22) * sm1p;
    let p8i = -((vi4 + vi8h) * c22 - (vr4 + vr8h) * s22) * sm1p;
    let ephr = (p1r + p2r + p3r + p4r + p5r + p6r + p7r + p8r) * cdel;
    let ephi = (p1i + p2i + p3i + p4i + p5i + p6i + p7i + p8i) * cdel;
    0.0296 * (ethr * ethr + ethi * ethi + ephr * ephr + ephi * ephi)
}
