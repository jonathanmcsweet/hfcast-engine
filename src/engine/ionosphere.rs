//! Ionospheric layer parameters at each control point for one hour.
//!
//! Port of the map-evaluation chain: `cngtim.for`/`geotim.for` (time
//! conversion), `virtim.for` (time variation of the coefficient maps),
//! `versy.for` (geographic evaluation), `noisy.for` (the general
//! two-variable Fourier evaluator), `ef1var.for` (E and F1 layers),
//! `timvar.for` (zenith angle, height ratio, absorption index) and
//! `f2var.for` (F2 layer, including the F1 merger near twilight).
//!
//! Data flow per hour: `virtim` reduces the 2604 interpolated map
//! coefficients to 318 time-evaluated ones (`AB`), then per control point
//! `versy` evaluates maps geographically and the layer routines produce
//! critical frequencies `FI`, semithicknesses `YI` and heights `HI` for
//! E, F1 and F2, plus the M(3000)F2 factor and absorption index.
//!
//! `versy` mixes `real*8` into the otherwise 4-byte arithmetic (the
//! source comment says "to solve floating point underflows"); the port
//! widens and narrows at exactly the statements the Fortran does.

// Constants are kept digit for digit with the Fortran source.
#![allow(clippy::excessive_precision)]

use super::coefficients::CoefficientSet;
use super::con::{D2R, R, R2D};
use super::geometry::ControlPoint;
use super::magnetic::MagneticVars;

/// Start of each map's coefficients in `COFION` (1-based, from `voacapw.for`).
const IA: [usize; 6] = [1, 276, 703, 978, 1966, 2407];
/// First dimension of each map's coefficient array.
const IB: [usize; 6] = [5, 7, 5, 13, 9, 9];
/// Start of each map's evaluated coefficients in `AB` (1-based).
const IC: [usize; 6] = [1, 56, 117, 172, 248, 297];

/// Subsolar-point latitudes per month, `SUN(2,12)` from `blkdat.for`.
const SUN: [[R; 2]; 12] = [
    [-23.05, -17.31],
    [-17.30, -7.89],
    [-7.88, 4.21],
    [4.26, 14.83],
    [14.84, 21.93],
    [21.93, 23.45],
    [23.15, 18.23],
    [18.20, 8.68],
    [8.55, -2.86],
    [-2.90, -14.16],
    [-14.20, -21.68],
    [-21.66, -23.45],
];

/// Height of maximum to semithickness ratios (`ef1var.for` DATA).
const BETAE: R = 5.5;
const BETAF1: R = 4.0;
/// `f2var.for` DATA: the retardation ratio floor and the twilight width.
const XF1: R = 1.1;
const DELZ: R = 2.0;

/// The 2604 interpolated map coefficients laid out as the Fortran's
/// `COFION` equivalence: ESLCOF, ESMCOF, ESUCOF, F2COF, FM3COF, ERCOF in
/// storage order.
pub fn cofion(set: &CoefficientSet) -> Vec<R> {
    let mut out = Vec::with_capacity(2604);
    out.extend(set.eslcof.iter().flatten());
    out.extend(set.esmcof.iter().flatten());
    out.extend(set.esucof.iter().flatten());
    out.extend(set.f2cof.iter().flatten());
    out.extend(set.fm3cof.iter().flatten());
    out.extend(set.ercof.iter().flatten());
    out
}

/// Port of `CNGTIM`: shifts `time` (hours) by the longitude offset in the
/// direction `isw` (+1 UT to local, -1 local to UT) and wraps it into
/// 0..24. Returns the day change (-1, 0 or 1) as the Fortran does.
pub fn cngtim(time: &mut R, xtheta_deg: R, isw: i32) -> R {
    if xtheta_deg == 0.0 {
        return 0.0;
    }
    let theta = if xtheta_deg < 0.0 {
        360.0 + xtheta_deg
    } else {
        xtheta_deg
    };
    let fint1 = (theta / 180.0) as i32 as R;
    let fint2 = (theta / 360.0) as i32 as R;
    let fisw = isw as R;
    *time += fisw * (theta / 15.0 - fint1 * 24.0 + fint2 * 24.0);
    // The Fortran's 23.99999999 rounds to 24.0 in 4-byte REAL.
    let int1 = ((*time - 23.99999999) / 24.0) as i32;
    let int2 = (*time / 24.0) as i32;
    let cngday = (int1 + int2) as R;
    if *time < 0.0 {
        *time += 24.0;
    }
    *time -= fint2 * 24.0;
    loop {
        if *time < 0.0 {
            *time += 24.0;
        } else if *time > 24.0 {
            *time -= 24.0;
        } else {
            break;
        }
    }
    cngday
}

/// The times `GEOTIM` derives for one hour of the run.
#[derive(Debug, Clone, Copy)]
pub struct HourTimes {
    /// Universal time, hours.
    pub gmt: R,
    /// Local mean time at the transmitter.
    pub lmt_tx: R,
    /// Local mean time at the receiver (`GMTR`).
    pub gmtr: R,
}

/// Port of `GEOTIM` (without the per-point `CLCK`, which `TIMVAR`
/// overwrites): converts the TIME-card hour `jt` to UT and local times.
/// `itim < 0` means the card gave local time at the transmitter.
pub fn geotim(jt: i32, itim: i32, tlong: R, rlong: R) -> HourTimes {
    let mut ckc = jt as R;
    let gmt;
    let lmt_tx;
    if itim < 0 {
        lmt_tx = ckc;
        cngtim(&mut ckc, tlong * R2D, -1);
        gmt = ckc;
    } else {
        gmt = ckc;
        cngtim(&mut ckc, tlong * R2D, 1);
        lmt_tx = ckc;
    }
    let mut gmtr = gmt;
    cngtim(&mut gmtr, rlong * R2D, 1);
    HourTimes { gmt, lmt_tx, gmtr }
}

/// Port of `VIRTIM`: evaluates the diurnal Fourier series of every map at
/// `gmt`, reducing `COFION` (2604 values) to `AB` (318).
pub fn virtim(cof: &[R], ikim: &[[i32; 10]; 6], gmt: R) -> [R; 318] {
    let time = (15.0 * gmt - 180.0) * D2R;
    let mut c = [0.0 as R; 8];
    let mut s = [0.0 as R; 8];
    c[0] = time.cos();
    s[0] = time.sin();
    for jb in 1..8 {
        c[jb] = c[0] * c[jb - 1] - s[0] * s[jb - 1];
        s[jb] = c[0] * s[jb - 1] + s[0] * c[jb - 1];
    }
    let mut ab = [0.0 as R; 318];
    for iz in 0..6 {
        let harmonics = ikim[iz][9];
        let terms = ikim[iz][8] + 1;
        for jb in 1..=terms as usize {
            let isubb = IA[iz] + (jb - 1) * IB[iz];
            let isuba = IC[iz] + jb - 1;
            let mut value = cof[isubb - 1];
            for ka in 1..=harmonics as usize {
                let isubc = isubb + 2 * ka - 1;
                value = value + s[ka - 1] * cof[isubc - 1] + c[ka - 1] * cof[isubc];
            }
            ab[isuba - 1] = value;
        }
    }
    ab
}

/// Port of `VERSY` for one map: evaluates map `iz` (1-based; 1-3 Es,
/// 4 foF2, 5 M(3000)F2, 6 regular E) at coordinate function argument `x`
/// (magnetic dip, or latitude for map 6), east longitude `clg` and
/// `gob = cos(latitude)`, all radians. Returns the map value `GAMMA(iz)`.
pub fn versy(ab: &[R; 318], ikim: &[[i32; 10]; 6], iz: usize, x_in: R, clg: R, gob: R) -> R {
    let ik = &ikim[iz - 1];
    let terms = ik[8] + 1;
    let k = ik[0];
    let x = f64::from(x_in);
    let sx = x.sin();
    let mut g = [0.0f64; 76];
    g[0] = 1.0;
    g[1] = sx;
    if k != 1 && k >= 2 {
        for ka in 2..=k as usize {
            g[ka] = sx * g[ka - 1];
        }
    }
    let mut kdif = ik[1] - k;
    if kdif != 0 {
        // Longitude harmonics: JG counts them, CX carries cos(lat)^JG.
        let mut jg = 1usize;
        let mut cx = f64::from(gob);
        let mut t = f64::from(clg);
        loop {
            let kk = (ik[jg - 1] + 4) as usize;
            g[kk - 3] = cx * t.cos();
            g[kk - 2] = cx * t.sin();
            let lo = ik[jg] as usize;
            if kdif != 2 && lo >= kk {
                let mut ka = kk;
                while ka <= lo {
                    g[ka - 1] = sx * g[ka - 3];
                    g[ka] = sx * g[ka - 2];
                    ka += 2;
                }
            }
            if jg == 8 {
                break;
            }
            kdif = ik[jg + 1] - lo as i32;
            if kdif == 0 {
                break;
            }
            cx *= f64::from(gob);
            jg += 1;
            // T = FJ * Y is a single-precision product widened to double.
            t = f64::from(jg as R * clg);
        }
    }
    // The summation narrows to f32 at every store, as the Fortran does.
    let mut isuba = IC[iz - 1];
    let mut gamma = (g[0] * f64::from(ab[isuba - 1])) as R;
    for jb in 2..=terms as usize {
        isuba = IC[iz - 1] + jb - 1;
        gamma = (f64::from(gamma) + f64::from(ab[isuba - 1]) * g[jb - 1]) as R;
    }
    gamma
}

/// Port of `NOISY`: evaluates a two-variable Fourier map. `plane` is one
/// `P(29,16,·)` plane, `abp` its normalising pair, `ratio_map` selects the
/// shorter series limits used for the height-ratio map (`KJ = 8`).
/// `xla` and `ceg` are the two coordinates in degrees.
pub fn noisy(plane: &[[R; 29]; 16], abp: &[R; 2], ratio_map: bool, xla: R, ceg: R) -> R {
    let (lm, ln) = if ratio_map { (15, 10) } else { (29, 15) };
    let alf = abp[0];
    let bet = abp[1];
    // Longitude series on the half angle.
    let q = 0.0087266466 * ceg;
    let c1 = q.cos();
    let s1 = q.sin();
    let mut sx = [0.0 as R; 15];
    sx[0] = s1;
    let mut cx = c1;
    for k in 1..ln {
        let tx = sx[k - 1];
        sx[k] = tx * c1 + cx * s1;
        cx = cx * c1 - tx * s1;
    }
    let mut zz = [0.0 as R; 29];
    for (j, z) in zz.iter_mut().take(lm).enumerate() {
        let mut r: R = 0.0;
        for (k, sxk) in sx.iter().take(ln).enumerate() {
            r += sxk * plane[k][j];
        }
        *z = r + plane[15][j];
    }
    // Latitude series on the angle plus 90 degrees.
    let q = 0.01745329252 * (xla + 90.0);
    let s1 = q.sin();
    let c1 = q.cos();
    let mut sk = s1;
    let mut cx = c1;
    let mut r: R = 0.0;
    for z in zz.iter().take(lm) {
        r += sk * z;
        let ss = sk * c1 + cx * s1;
        cx = cx * c1 - sk * s1;
        sk = ss;
    }
    r + alf + bet * q
}

/// The ionospheric parameters at one control point for one hour: the
/// contents of `/RON/` `FI`, `YI`, `HI` (index 0 E, 1 F1, 2 F2) and the
/// per-point parts of `/MFAC/` and `/GEOG/` after `F2VAR`.
#[derive(Debug, Clone, Copy)]
pub struct LayerParams {
    /// Critical frequencies, MHz.
    pub fi: [R; 3],
    /// Semithicknesses, km.
    pub yi: [R; 3],
    /// Heights of maximum ionisation, km.
    pub hi: [R; 3],
    /// M(3000)F2 factor.
    pub f2m3: R,
    /// Virtual height at 0.834 times the F2 critical frequency, km.
    pub hpf2: R,
    /// Ratio of F2 height of maximum to semithickness.
    pub rat: R,
    /// Absorption index.
    pub abiy: R,
    /// Local mean time at the point, hours.
    pub clck: R,
    /// Sun zenith angle, degrees.
    pub zenang: R,
    /// Maximum zenith angle at which an F1 layer exists, degrees.
    pub zenmax: R,
}

/// Ports the per-point bodies of `TIMVAR` (with its `EF1VAR` and `NOISY`
/// calls) followed by `F2VAR`, for every control point. The Fortran runs
/// each routine as its own loop over points, but no state crosses points,
/// so one pass per point computes the same values.
#[allow(clippy::too_many_arguments)]
pub fn layer_parameters(
    set: &CoefficientSet,
    ab: &[R; 318],
    points: &[ControlPoint],
    mags: &[MagneticVars],
    month: u32,
    ssn: R,
    gmt: R,
    psc: &[R; 4],
) -> Vec<LayerParams> {
    let sun = SUN[month as usize - 1];
    // Longitude of noon.
    let ssl = (180.0 - 15.0 * gmt) * D2R;
    let c360 = 360.0 * D2R;

    points
        .iter()
        .zip(mags)
        .map(|(pt, mag)| {
            let cenlat = pt.lat;
            let cenlg = pt.lon;
            let clg = if cenlg < 0.0 { c360 + cenlg } else { cenlg };
            let gob = cenlat.cos();

            // --- TIMVAR: subsolar point, local time, zenith angle.
            let cendog = (cenlat - D2R * sun[0]).abs();
            let mut ssp = sun[1] * D2R;
            let cencat = (cenlat - ssp).abs();
            if cendog - cencat > 0.0 {
                ssp = sun[0] * D2R;
            }
            let mut clock = gmt + cenlg / 0.261799387;
            if 24.0 - clock < 0.0 {
                clock -= 24.0;
            } else if clock <= 0.0 {
                clock += 24.0;
            }
            let z = cenlg - ssl;
            let cycen =
                (cenlat.sin() * ssp.sin() + cenlat.cos() * ssp.cos() * z.cos()).acos();

            // --- EF1VAR: E and F1 layers.
            let mut fi = [0.0 as R; 3];
            let mut yi = [0.0 as R; 3];
            let mut hi = [0.0 as R; 3];
            let mut gamma6 = versy(ab, &set.ikim, 6, cenlat, clg, gob);
            if gamma6 - 0.36 < 0.0 {
                // Bad map value: use the midnight value.
                gamma6 = 0.36 * (1.0 + 0.0098 * ssn).sqrt();
            }
            fi[0] = gamma6 * psc[0];
            hi[0] = 110.0;
            yi[0] = hi[0] / BETAE;
            let cycen = cycen.abs();
            let zdeg = cycen * R2D;
            let cosz = cycen.cos();
            let cosdi = mag.gmdip.cos();
            let zenmax = set.achi[0] + set.bchi[0] * ssn + (set.achi[1] + set.bchi[1] * ssn) * cosdi;
            let zenang = zdeg;
            if zdeg <= zenmax {
                let f1 = (set.anew[0] + set.bnew[0] * ssn)
                    + (set.anew[1] + set.bnew[1] * ssn) * cosz
                    + (set.anew[2] + set.bnew[2] * ssn) * cosz * cosz;
                fi[1] = f1 * psc[1];
                hi[1] = 165.0 + 0.6428 * zdeg;
                yi[1] = hi[1] / BETAF1;
            }

            // --- TIMVAR continued: height ratio map and absorption index.
            let gm = pt.gmlat.abs() * R2D - 45.0;
            let mut zn = cycen * R2D;
            if clock - 12.0 < 0.0 {
                zn = -zn;
            }
            zn += 180.0;
            // ABMAP column 2 normalises the ratio map (plane 8 of P).
            let mut rat = noisy(&set.hmym, &set.abmap[1], true, gm, zn);
            if rat - 2.0 < 0.0 {
                rat = 2.0;
            }
            let abiy = -0.04 + (-2.937 + 0.8445 * fi[0]).exp();

            // --- F2VAR: the F2 layer with E retardation and F1 merger.
            let gamma4 = versy(ab, &set.ikim, 4, mag.gmdip, clg, gob);
            let gamma5 = versy(ab, &set.ikim, 5, mag.gmdip, clg, gob);
            let f2m3 = gamma5;
            let hpf2 = 1490.0 / gamma5 - 176.0;
            fi[2] = (gamma4 + 0.5 * mag.gyz) * psc[2];
            let zmax = zenmax;
            let z = zenang;
            // F1 must be less than F2.
            fi[1] = fi[1].min(fi[2] - 0.2);
            let fc = 0.834 * fi[2];
            let ec = fi[0];
            let fcec = (fc / ec).max(1.1);
            // E layer retardation.
            let mut ret = fcec * ((fcec + 1.0) / (fcec - 1.0)).ln();
            ret = (ret - 2.0) * yi[0];
            let fc1 = fi[1];
            if fc1 > 0.0 {
                let ffec = (fc / fc1).max(XF1);
                let rft;
                if zmax - DELZ - z >= 0.0 {
                    // Away from twilight: plain F1 retardation.
                    let r = ffec * ((ffec + 1.0) / (ffec - 1.0)).ln();
                    rft = 0.5 * yi[1] * (r - 2.0);
                } else {
                    // Near day-night: force F1 up into F2 and the
                    // retardation to zero from ZN to ZMAX.
                    let zn = zmax - DELZ;
                    let hn = 165.0 + 0.6428 * zn;
                    let yn = hn * (yi[1] / hi[1]);
                    let mut rfn = ffec * ((ffec + 1.0) / (ffec - 1.0)).ln();
                    rfn = 0.5 * yn * (rfn - 2.0);
                    let sz = (z - zn) / DELZ;
                    rft = rfn * (1.0 - sz);
                    // F2 without F1.
                    let hm = hpf2 - ret;
                    let ym = hm / rat;
                    let dhn = (hm - ym) - (hn - yn);
                    if dhn > 0.0 {
                        // Bottom of F1 goes to bottom of F2.
                        let dh = dhn * (1.0 - sz);
                        hi[1] = (hm - ym) - dh + yi[1];
                        if fc1 - fc > 0.0 {
                            // F1 close to F2 in frequency too: force the
                            // F1 semithickness toward the F2 one.
                            let y1max = yn + (ym - yn) * (fc1 / fi[2] - 0.834) / 0.166;
                            yi[1] = yn + (y1max - yn) * sz;
                            hi[1] = (hm - ym) - dh + yi[1];
                        }
                    }
                }
                ret += rft;
            }
            hi[2] = hpf2 - ret;
            yi[2] = hi[2] / rat;

            LayerParams {
                fi,
                yi,
                hi,
                f2m3,
                hpf2,
                rat,
                abiy,
                clck: clock,
                zenang,
                zenmax,
            }
        })
        .collect()
}

/// Ground constants at a control point: conductivity (mhos/m) and
/// relative dielectric constant, from `geom.for`'s land-mass lookup
/// (`NOISY` map plane 7). Sea gives (5.0, 80.0), land (0.001, 4.0).
/// The longitude is the east longitude `magvar` computed, exactly as
/// the Fortran reuses its `CLG` output argument.
pub fn ground_constants(
    set: &CoefficientSet,
    points: &[ControlPoint],
    mags: &[MagneticVars],
) -> Vec<(R, R)> {
    points
        .iter()
        .zip(mags)
        .map(|(pt, mag)| {
            let rfltd = pt.lat * R2D;
            let mut clgd = mag.east_lon * R2D;
            if clgd < 0.0 {
                clgd += 360.0;
            }
            let wld = noisy(&set.fakmap, &set.abmap[0], false, rfltd, clgd);
            if wld >= 0.0 {
                (0.001, 4.0)
            } else {
                (5.0, 80.0)
            }
        })
        .collect()
}

/// `ALATD` from the end of `GEOM`: the absolute path latitude in
/// degrees — the first control point's, or the mean of the first three
/// when there is more than one.
pub fn alatd(points: &[ControlPoint]) -> R {
    if points.len() == 1 {
        points[0].lat.abs() * R2D
    } else {
        ((points[0].lat + points[1].lat + points[2].lat) / 3.0).abs() * R2D
    }
}

/// Sporadic-E parameters at one control point: the deciles of the Es
/// critical frequency and the reflection height (`/ES/`).
#[derive(Debug, Clone, Copy)]
pub struct EsParams {
    /// Critical frequency lower decile, median, upper decile, MHz.
    pub fs: [R; 3],
    /// Height of reflection, km.
    pub hs: R,
}

/// Port of `ESIND`: evaluates the three sporadic-E maps at every control
/// point. `PSC(4)` scales the frequencies — the FPROB card's fourth value,
/// 0 turning the layer off. Note the source assigns map 3 to the lower
/// decile and map 1 to the upper.
pub fn esind(
    set: &CoefficientSet,
    ab: &[R; 318],
    points: &[ControlPoint],
    mags: &[MagneticVars],
    psc: &[R; 4],
) -> Vec<EsParams> {
    let c360 = 360.0 * D2R;
    points
        .iter()
        .zip(mags)
        .map(|(pt, mag)| {
            let clg = if pt.lon < 0.0 { c360 + pt.lon } else { pt.lon };
            let gob = pt.lat.cos();
            let gamma1 = versy(ab, &set.ikim, 1, mag.gmdip, clg, gob);
            let gamma2 = versy(ab, &set.ikim, 2, mag.gmdip, clg, gob);
            let gamma3 = versy(ab, &set.ikim, 3, mag.gmdip, clg, gob);
            EsParams {
                fs: [gamma3 * psc[3], gamma2 * psc[3], gamma1 * psc[3]],
                hs: 110.0,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cngtim_wraps_and_reports_day_changes() {
        // 90 degrees east is six hours ahead: 20 UT is 02 local next day.
        let mut t: R = 20.0;
        let day = cngtim(&mut t, 90.0, 1);
        assert_eq!(t, 2.0);
        assert_eq!(day, 1.0);
        // The inverse conversion returns to UT with the day retarded.
        let day = cngtim(&mut t, 90.0, -1);
        assert_eq!(t, 20.0);
        assert_eq!(day, -1.0);
        // Longitude zero changes nothing.
        let mut t: R = 5.0;
        assert_eq!(cngtim(&mut t, 0.0, 1), 0.0);
        assert_eq!(t, 5.0);
    }

    #[test]
    fn geotim_ut_card_keeps_gmt_and_derives_local_times() {
        // Transmitter at 75 W (5 hours behind), receiver at 0.
        let times = geotim(12, 1, -75.0 * D2R, 0.0);
        assert_eq!(times.gmt, 12.0);
        assert_eq!(times.lmt_tx, 7.0);
        assert_eq!(times.gmtr, 12.0);
    }

    #[test]
    fn virtim_is_periodic_in_time() {
        // With a synthetic constant coefficient set, only the constant
        // term of each map survives at any hour.
        let cof = vec![1.0 as R; 2604];
        let mut ikim = [[0i32; 10]; 6];
        for row in ikim.iter_mut() {
            row[8] = 0; // one spatial term
            row[9] = 1; // one harmonic
        }
        let noon = virtim(&cof, &ikim, 12.0);
        let noon_next = virtim(&cof, &ikim, 36.0);
        for (a, b) in noon.iter().zip(&noon_next) {
            assert!((a - b).abs() < 1e-5);
        }
    }

    #[test]
    fn noisy_constant_map_returns_the_constant_plus_normalisation() {
        // A plane whose only nonzero entries are the constant column
        // reduces to the latitude series over that constant.
        let mut plane = [[0.0 as R; 29]; 16];
        for v in plane[15].iter_mut() {
            *v = 0.0;
        }
        let abp = [3.0 as R, 0.0];
        let value = noisy(&plane, &abp, true, 0.0, 0.0);
        assert!((value - 3.0).abs() < 1e-6, "value {value}");
    }
}
