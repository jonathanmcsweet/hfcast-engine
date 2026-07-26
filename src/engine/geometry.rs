//! Path geometry: great-circle distance, bearings and control points.
//!
//! Port of `geom.for`. The control points ("sample areas") are where the
//! ionosphere gets evaluated: 1, 3 or 5 of them depending on path length,
//! at fixed distances from each end, plus the midpoint. Each carries its
//! geomagnetic latitude against the run's magnetic pole.
//!
//! The Fortran's numeric quirks are kept on purpose: its own PI constant,
//! clamped ACOS arguments, the 31.85-metre minimum distance, the nudge that
//! separates a receiver sitting on top of the transmitter, and the special
//! cases for endpoints within 1e-7 of a pole. Arithmetic follows the source
//! expression by expression in `f32`.

use super::con::{MagneticPole, D2R, PI, PI2, PIO2, R, R2D, RZ};

const EPSLON: R = 1.0e-7;
/// Closest the receiver may sit to the transmitter, degrees.
const DDX: R = 0.03;

/// Fortran `SIGN(a, b)`: the magnitude of `a` with the sign of `b`.
fn sign(a: R, b: R) -> R {
    if b >= 0.0 {
        a.abs()
    } else {
        -a.abs()
    }
}

/// One reflection sample area along the path.
#[derive(Debug, Clone, Copy)]
pub struct ControlPoint {
    /// Distance from the transmitter along the path, radians.
    pub rd: R,
    /// Geographic latitude, radians.
    pub lat: R,
    /// Geographic longitude, radians.
    pub lon: R,
    /// Geomagnetic latitude, radians.
    pub gmlat: R,
}

#[derive(Debug, Clone)]
pub struct PathGeometry {
    /// Great-circle distance, radians.
    pub gcd: R,
    /// Great-circle distance, km.
    pub gcd_km: R,
    /// Bearing transmitter to receiver, radians clockwise from north.
    pub btr: R,
    /// Bearing receiver to transmitter, radians clockwise from north.
    pub brt: R,
    pub points: Vec<ControlPoint>,
}

/// Computes the path geometry. `long_path` is the Fortran's `NPSL = 1`:
/// the great-circle route the long way round.
pub fn path_geometry(
    tlat_deg: R,
    tlong_deg: R,
    rlat_deg: R,
    rlong_deg: R,
    long_path: bool,
    pole: MagneticPole,
) -> PathGeometry {
    let glt = pole.lat_deg * D2R;
    let glg = pole.lon_deg * D2R;

    let tlatd = tlat_deg;
    let tlongd = tlong_deg;
    let mut rlatd = rlat_deg;
    let rlongd = rlong_deg;
    // Move the receiver away from a coincident transmitter a little.
    if (tlatd - rlatd).abs() <= DDX && (tlongd - rlongd).abs() <= DDX {
        let dd = if rlatd < 0.0 { -DDX } else { DDX };
        rlatd = tlatd - dd;
    }

    let tlat = tlatd * D2R;
    let mut tlong = tlongd * D2R;
    let rlat = rlatd * D2R;
    let mut rlong = rlongd * D2R;
    // At the poles longitude is meaningless; the Fortran forces zero.
    if rlatd.abs() > 89.9 {
        rlong = 0.0;
    }
    if tlatd.abs() > 89.9 {
        tlong = 0.0;
    }

    let mut dlong = tlong - rlong;
    if dlong.abs() > PI {
        dlong -= sign(PI2, dlong);
    }
    if long_path {
        dlong -= sign(PI2, dlong);
    }

    let mut qcos = tlat.sin() * rlat.sin() + tlat.cos() * rlat.cos() * dlong.cos();
    if qcos.abs() > 1.0 {
        qcos = sign(1.0, qcos);
    }
    let mut gcd = qcos.acos();
    // Minimum distance is 31.85 metres.
    if gcd < 0.000001 {
        gcd = 0.000001;
    }
    if long_path {
        gcd = PI2 - gcd;
    }
    let gcd_km = gcd * RZ;

    // Bearing transmitter to receiver, with the near-pole special case.
    let mut btr = if tlat.cos() - EPSLON <= 0.0 {
        if tlat <= 0.0 {
            0.0
        } else {
            PI
        }
    } else {
        let mut q = (rlat.sin() - tlat.sin() * gcd.cos()) / (tlat.cos() * gcd.sin());
        if q.abs() > 1.0 {
            q = sign(1.0, q);
        }
        q.acos()
    };
    if dlong > 0.0 {
        btr = PI2 - btr;
    }

    // Bearing receiver to transmitter.
    let mut brt = if rlat.cos() - EPSLON <= 0.0 {
        if rlat <= 0.0 {
            0.0
        } else {
            PI
        }
    } else {
        let mut q = (tlat.sin() - rlat.sin() * gcd.cos()) / (rlat.cos() * gcd.sin());
        if q.abs() > 1.0 {
            q = sign(1.0, q);
        }
        q.acos()
    };
    if dlong < 0.0 {
        brt = PI2 - brt;
    }

    // Sample areas in order: RD(1) is E layer, RD(2) F layer, RD(3) all
    // layers, RD(4) F layer, RD(5) E layer.
    let rd: Vec<R> = if gcd_km <= 2000.01 {
        vec![gcd / 2.0]
    } else if gcd_km <= 4000.0 {
        let r1 = 1000.0 / RZ;
        vec![r1, gcd / 2.0, gcd - r1]
    } else {
        let r1 = 1000.0 / RZ;
        let r2 = r1 + r1;
        vec![r1, r2, gcd / 2.0, gcd - r2, gcd - r1]
    };

    let points = rd
        .iter()
        .map(|&drf| {
            let ctlat = tlat.cos();
            let (rflt, rflg) = if ctlat - EPSLON < 0.0 {
                // Transmitter near a pole: walk straight down its meridian.
                let mut rflt = tlat - sign(drf, tlat);
                if rflt.abs() > PIO2 {
                    rflt = PIO2 * sign(1.0, rflt);
                }
                (rflt, rlong)
            } else {
                let mut q = drf.cos() * tlat.sin() + drf.sin() * tlat.cos() * btr.cos();
                if q.abs() > 1.0 {
                    q = sign(1.0, q);
                }
                let rflt = PIO2 - q.acos();
                let rflg = if rflt.cos() - EPSLON <= 0.0 {
                    // The sample area itself is near a pole.
                    tlong
                } else {
                    let mut q = (drf.cos() - rflt.sin() * tlat.sin()) / (rflt.cos() * tlat.cos());
                    if q.abs() > 1.0 {
                        q = sign(1.0, q);
                    }
                    let mut rflg = q.acos();
                    if drf >= PI {
                        rflg = PI2 - rflg;
                    }
                    rflg = tlong - sign(rflg, dlong);
                    if rflg.abs() > PI {
                        rflg -= sign(PI2, rflg);
                    }
                    rflg
                };
                (rflt, rflg)
            };

            let mut q = glt.sin() * rflt.sin() + glt.cos() * rflt.cos() * (rflg - glg).cos();
            if q.abs() > 1.0 {
                q = sign(1.0, q);
            }
            let gmlat = PIO2 - q.acos();
            ControlPoint {
                rd: drf,
                lat: rflt,
                lon: rflg,
                gmlat,
            }
        })
        .collect();

    PathGeometry {
        gcd,
        gcd_km,
        btr,
        brt,
        points,
    }
}

impl PathGeometry {
    pub fn btr_deg(&self) -> R {
        self.btr * R2D
    }

    pub fn brt_deg(&self) -> R {
        self.brt * R2D
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pole() -> MagneticPole {
        MagneticPole {
            lat_deg: 79.5,
            lon_deg: -69.0,
        }
    }

    #[test]
    fn short_path_has_one_midpoint() {
        // London to Paris, ~343 km.
        let g = path_geometry(51.5, -0.13, 48.86, 2.35, false, pole());
        assert!((g.gcd_km - 343.0).abs() < 3.0, "gcd_km {}", g.gcd_km);
        assert_eq!(g.points.len(), 1);
        let mid = &g.points[0];
        assert!((mid.rd - g.gcd / 2.0).abs() < 1e-7);
        // The midpoint sits between the endpoints.
        let lat_deg = mid.lat * R2D;
        assert!(lat_deg > 48.86 && lat_deg < 51.5, "midpoint lat {lat_deg}");
    }

    #[test]
    fn long_path_has_five_points_at_fixed_offsets() {
        // Seattle to Tokyo, ~7700 km.
        let g = path_geometry(47.6, -122.33, 35.68, 139.65, false, pole());
        assert!((g.gcd_km - 7700.0).abs() < 100.0, "gcd_km {}", g.gcd_km);
        assert_eq!(g.points.len(), 5);
        assert!((g.points[0].rd * RZ - 1000.0).abs() < 0.5);
        assert!((g.points[4].rd * RZ - (g.gcd_km - 1000.0)).abs() < 0.5);
        // Westward across the Pacific: bearing is west of north.
        let b = g.btr_deg();
        assert!((270.0..360.0).contains(&b), "bearing {b}");
    }

    #[test]
    fn the_long_way_round_is_the_complement() {
        let short = path_geometry(-33.87, 151.21, 51.5, -0.13, false, pole());
        let long = path_geometry(-33.87, 151.21, 51.5, -0.13, true, pole());
        let total = short.gcd_km + long.gcd_km;
        assert!((total - PI2 * RZ).abs() < 1.0, "total {total}");
    }

    #[test]
    fn coincident_stations_are_separated() {
        let g = path_geometry(40.0, -75.0, 40.0, -75.0, false, pole());
        // The nudge is 0.03 degrees of latitude, about 3.3 km.
        assert!(g.gcd_km > 3.0 && g.gcd_km < 4.0, "gcd_km {}", g.gcd_km);
    }

    #[test]
    fn polar_transmitter_uses_the_meridian_rule() {
        // Pole to 50 N is about 4,450 km, above the five-point threshold.
        let g = path_geometry(90.0, 0.0, 50.0, 10.0, false, pole());
        assert_eq!(g.btr, PI);
        assert_eq!(g.points.len(), 5);
        for p in &g.points {
            assert!(p.lat <= PIO2);
        }
    }
}
