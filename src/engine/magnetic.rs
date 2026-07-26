//! The geomagnetic field at a control point: gyrofrequency and dip angle.
//!
//! Port of `magvar.for` and `magfin.for`. The field model is a degree-6
//! spherical-harmonic expansion with the Jensen-Cain 1960 coefficients
//! (Journal of Geophysical Research 67(9), 1962) — VOACAP has carried this
//! epoch-1960 field ever since, and matching the engine means keeping it.
//! Evaluation height is fixed at 300 km.
//!
//! The coefficient tables below are the Fortran DATA statements re-laid
//! row-major: `G[n][m]` here is `G(n+1, m+1)` there. The absurd-looking
//! Rawer dip formula — an arctangent of an arctangent divided by the square
//! root of a cosine — is exactly what the source computes.

use super::con::{D2R, R};

/// Fortran `SIGN` is not needed here, but the polar clamp is: latitudes
/// beyond ±89.9 degrees are pinned and their longitude zeroed.
const RD: R = 1.56905124;
/// Reference radius, metres.
const HC: R = 6371200.0;

/// Gauss-normalised recurrence constants CT(N,M).
const CT: [[R; 7]; 7] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.33333333, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.26666667, 0.2, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.25714286, 0.22857142, 0.14285714, 0.0, 0.0, 0.0, 0.0],
    [
        0.25396825, 0.23809523, 0.19047619, 0.11111111, 0.0, 0.0, 0.0,
    ],
    [
        0.25252525, 0.24242424, 0.21212121, 0.16161616, 0.09090909, 0.0, 0.0,
    ],
];

/// Jensen-Cain 1960 G coefficients, G[n][m] = Fortran G(n+1, m+1).
const G: [[R; 7]; 7] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.304112, 0.021474, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.024035, -0.051253, -0.013381, 0.0, 0.0, 0.0, 0.0],
    [-0.031518, 0.062130, -0.024898, -0.006496, 0.0, 0.0, 0.0],
    [
        -0.041794, -0.045298, -0.021795, 0.007008, -0.002044, 0.0, 0.0,
    ],
    [
        0.016256, -0.034407, -0.019447, -0.000608, 0.002775, 0.000697, 0.0,
    ],
    [
        -0.019523, -0.004853, 0.003212, 0.021413, 0.001051, 0.000227, 0.001115,
    ],
];

/// Jensen-Cain 1960 H coefficients, H[n][m] = Fortran H(n+1, m+1).
const H: [[R; 7]; 7] = [
    [0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, -0.057989, 0.0, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.033124, -0.001579, 0.0, 0.0, 0.0, 0.0],
    [0.0, 0.014870, -0.004075, 0.00021, 0.0, 0.0, 0.0],
    [0.0, -0.011825, 0.010006, 0.00043, 0.001385, 0.0, 0.0],
    [0.0, -0.000796, -0.002, 0.004597, 0.002421, -0.001218, 0.0],
    [
        0.0, -0.005758, -0.008735, -0.003406, -0.000118, -0.001116, -0.000325,
    ],
];

/// The magnetic field vector at a position, in the Fortran's left-handed
/// frame: `[up, north, east]`, in gauss.
///
/// `lat` in radians, `east_lon` in radians 0..2*PI, `height_m` in metres.
pub fn magfin(lat: R, east_lon: R, height_m: R) -> [R; 3] {
    let mut p1 = lat;
    let mut p2 = east_lon;
    if p1 > RD {
        p1 = RD;
        p2 = 0.0;
    } else if p1 < -RD {
        p1 = -RD;
        p2 = 0.0;
    }
    let phi = p2;
    let ar = HC / (HC + height_m);
    let c = p1.sin();
    let s = (1.0 - c * c).sqrt();

    // Fortran P(N,M) → p[n-1][m-1], same for DP; SP/CP/AOR are 1-based
    // there and 0-based here.
    let mut p = [[0.0 as R; 7]; 7];
    let mut dp = [[0.0 as R; 7]; 7];
    p[0][0] = 1.0;
    let mut sp = [0.0 as R; 7];
    let mut cp = [0.0 as R; 7];
    sp[0] = 0.0;
    cp[0] = 1.0;
    sp[1] = phi.sin();
    cp[1] = phi.cos();
    let mut aor = [0.0 as R; 7];
    aor[0] = ar * ar;
    aor[1] = aor[0] * ar;
    for m in 2..7 {
        sp[m] = sp[1] * cp[m - 1] + cp[1] * sp[m - 1];
        cp[m] = cp[1] * cp[m - 1] - sp[1] * sp[m - 1];
        aor[m] = ar * aor[m - 1];
    }

    let mut bv: R = 0.0;
    let mut bn: R = 0.0;
    let mut bphi: R = 0.0;
    for n in 1..7 {
        // n is Fortran N-1: Fortran N runs 2..=7.
        let fn_ = (n + 1) as R;
        let mut sumr: R = 0.0;
        let mut sumt: R = 0.0;
        let mut sump: R = 0.0;
        for m in 0..=n {
            // m is Fortran M-1: Fortran M runs 1..=N.
            if n == m {
                p[n][n] = s * p[n - 1][n - 1];
                dp[n][n] = s * dp[n - 1][n - 1] + c * p[n - 1][n - 1];
            } else if n == 1 {
                // Fortran N=2, M=1: the explicit seed.
                p[1][0] = c;
                dp[1][0] = -s;
            } else {
                p[n][m] = c * p[n - 1][m] - CT[n][m] * p[n - 2][m];
                dp[n][m] = c * dp[n - 1][m] - s * p[n - 1][m] - CT[n][m] * dp[n - 2][m];
            }
            let fm = m as R;
            let ts = G[n][m] * cp[m] + H[n][m] * sp[m];
            sumr += p[n][m] * ts;
            sumt += dp[n][m] * ts;
            sump += fm * p[n][m] * (-G[n][m] * sp[m] + H[n][m] * cp[m]);
        }
        bv += aor[n] * fn_ * sumr;
        bn -= aor[n] * sumt;
        bphi -= aor[n] * sump;
    }

    [-bv, bn, -bphi / s]
}

/// Gyrofrequency and Rawer dip angle at a control point.
#[derive(Debug, Clone, Copy)]
pub struct MagneticVars {
    /// Gyrofrequency, MHz.
    pub gyz: R,
    /// Rawer magnetic dip angle, radians.
    pub gmdip: R,
    /// The point's longitude converted to east longitude 0..2*PI, which
    /// the Fortran hands on to the noise model.
    pub east_lon: R,
}

/// Port of `MAGVAR`: evaluates the field at 300 km over a control point.
///
/// `lat` and `lon` in radians, longitude negative west as elsewhere.
pub fn magvar(lat: R, lon: R) -> MagneticVars {
    let c360 = 360.0 * D2R;
    let east_lon = if lon < 0.0 { c360 + lon } else { lon };
    let une = magfin(lat, east_lon, 300000.0);
    let hhm = (une[0] * une[0] + une[1] * une[1] + une[2] * une[2]).sqrt();
    let tmp = une[1] * une[1] + une[2] * une[2];
    let mut gob = lat.cos();
    if gob <= 0.000001 {
        gob = 0.000001;
    }
    let gmdip = ((-une[0] / tmp.sqrt()).atan() / gob.sqrt()).atan();
    MagneticVars {
        gyz: 2.8 * hhm,
        gmdip,
        east_lon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::con::PIO2;

    #[test]
    fn the_field_is_dipole_like() {
        // Stronger and steeper near the magnetic pole than at the equator.
        let polar = magvar(70.0 * D2R, -70.0 * D2R);
        let equatorial = magvar(0.0, 100.0 * D2R);
        assert!(
            polar.gyz > equatorial.gyz,
            "polar {} vs equatorial {}",
            polar.gyz,
            equatorial.gyz
        );
        assert!(polar.gmdip.abs() > equatorial.gmdip.abs());
        // Earth's field: gyrofrequency runs about 0.7 to 1.7 MHz.
        assert!(polar.gyz > 1.0 && polar.gyz < 2.0, "gyz {}", polar.gyz);
        assert!(
            equatorial.gyz > 0.5 && equatorial.gyz < 1.2,
            "gyz {}",
            equatorial.gyz
        );
    }

    #[test]
    fn dip_signs_follow_the_hemispheres() {
        let north = magvar(50.0 * D2R, 10.0 * D2R);
        let south = magvar(-50.0 * D2R, 10.0 * D2R);
        assert!(north.gmdip > 0.0);
        assert!(south.gmdip < 0.0);
        assert!(north.gmdip.abs() < PIO2);
    }

    #[test]
    fn west_longitudes_convert_to_east() {
        let v = magvar(40.0 * D2R, -75.0 * D2R);
        let east_deg = v.east_lon / D2R;
        assert!((east_deg - 285.0).abs() < 0.01, "east {east_deg}");
    }
}
