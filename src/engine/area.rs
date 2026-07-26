//! Area coverage: the grid an area run sweeps.
//!
//! An area run does not read a card deck. It reads a keyed text file
//! naming a centre point, a rectangle and a grid size, then runs the
//! same one-hour prediction as a point-to-point method at every grid
//! point and writes the results to its own file. This module is the
//! geometry half: where the grid points are.
//!
//! `GRIDXY` offers two projections. A plain latitude and longitude mesh
//! takes the rectangle's coordinates as degrees. The great-circle mesh
//! takes them as kilometres east and north of the centre, turns each
//! into an azimuth and a distance, and asks `DAZEL1` for the point that
//! far along that bearing. Longitudes come back folded into 0 to 360.

use super::con::R;

/// Which projection the grid uses.
///
/// The `AREA` card's last field decides: zero gives `IPROJ = 7`, the
/// great-circle mesh, and anything else `IPROJ = 8`, which the source
/// calls "lat/lon for GRIB format". `GRIDXY` tests only for 7, so 8
/// takes its plain branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Projection {
    /// The rectangle is in degrees of longitude and latitude.
    LatLon,
    /// The rectangle is in kilometres east and north of the centre.
    GreatCircle,
}

/// The `/RGRID/` block: the projection, its centre, the rectangle and
/// the number of points along each side.
#[derive(Debug, Clone, Copy)]
pub struct Grid {
    pub projection: Projection,
    /// Centre of the projection, degrees.
    pub plat: R,
    pub plon: R,
    pub xmin: R,
    pub xmax: R,
    pub ymin: R,
    pub ymax: R,
    pub nx: usize,
    pub ny: usize,
}

impl Grid {
    /// The receiver location the driver uses at grid point
    /// `(ix, iy)`: [`Grid::point`], then `HFAREA`'s two corrections.
    ///
    /// A grid point that lands on the transmitter would be a
    /// zero-length path, so the driver moves it a twentieth of a degree
    /// east — wrapping past 360 — when the point is within 0.05 degrees
    /// of the transmitter in latitude and longitude both. And within
    /// 0.1 degrees of either pole the longitude is forced to zero,
    /// where every meridian meets.
    ///
    /// `tlon` is the transmitter's longitude as the driver holds it,
    /// which is the value the input file gave and so may be negative,
    /// while the grid point's longitude has already been folded into
    /// 0 to 360. The comparison is therefore between an unfolded number
    /// and a folded one: a transmitter at 0.13 degrees west differs
    /// from its own grid point by a full 360 degrees and never
    /// triggers the offset, so that run computes a zero-length path at
    /// its centre instead. A bug, kept as written.
    pub fn receiver(&self, ix: usize, iy: usize, tlat: R, tlon: R) -> (R, R) {
        let (mut lon, lat) = self.point(ix, iy);
        if (lat - tlat).abs() < 0.05 && (lon - tlon).abs() <= 0.05 {
            lon = tlon + 0.05;
            if lon >= 360.0 {
                lon -= 360.0;
            }
        }
        if lat.abs() > 89.9 {
            lon = 0.0;
        }
        (lon, lat)
    }

    /// Port of `GRIDXY`: the longitude and latitude of grid point
    /// `(ix, iy)`, both 1-based as the Fortran counts them.
    ///
    /// The azimuth is scaled by the literal `.0174533` the source
    /// writes rather than by `/CON/`'s degree conversion, which is a
    /// different number in its last digits.
    pub fn point(&self, ix: usize, iy: usize) -> (R, R) {
        let x = self.xmin + (ix as R - 1.0) * (self.xmax - self.xmin) / (self.nx as R - 1.0);
        let y = self.ymin + (iy as R - 1.0) * (self.ymax - self.ymin) / (self.ny as R - 1.0);
        let (mut lon, lat) = match self.projection {
            Projection::LatLon => (x, y),
            Projection::GreatCircle => {
                if x != 0.0 || y != 0.0 {
                    let mut az = 90.0 - y.atan2(x) / 0.0174533;
                    if az < 0.0 {
                        az += 360.0;
                    }
                    let dgc = (x * x + y * y).sqrt();
                    let (rlat, rlon) = dazel1(self.plat, self.plon, az, dgc);
                    (rlon, rlat)
                } else {
                    // The point is the centre of the projection.
                    (self.plon, self.plat)
                }
            }
        };
        if lon < 0.0 {
            lon += 360.0;
        }
        (lon, lat)
    }
}

/// Port of `DAZEL1`'s endpoint arithmetic: the latitude and longitude
/// `dgc` kilometres from `(tlat, tlon)` along azimuth `taz`.
///
/// The routine computes in double precision but its arguments and
/// results live in a single-precision COMMON block, so the inputs are
/// rounded to `f32` before widening and the answer is rounded back.
/// Its own Earth radius is 6370 km, and it decides the sign of the
/// longitude change from whether the azimuth exceeds 180 degrees rather
/// than from the arithmetic.
pub fn dazel1(tlat: R, tlon: R, taz: R, dgc: R) -> (R, R) {
    const PI: f64 = std::f64::consts::PI;
    const RERTH: f64 = 6370.0;
    const DTOR: f64 = 0.01745329252;
    const RTOD: f64 = 57.29577951;
    let (ztlat, ztlon, ztaz) = (f64::from(tlat), f64::from(tlon), f64::from(taz));
    let tlatr = ztlat * DTOR;
    let tazr = ztaz * DTOR;
    let gc = f64::from(dgc) / RERTH;
    let colat = PI / 2.0 - tlatr;
    let cosco = colat.cos();
    let sinco = colat.sin();
    let cosgc = gc.cos();
    let singc = gc.sin();
    let cosb = cosco * cosgc + sinco * singc * tazr.cos();
    let arg = (1.0 - cosb * cosb).max(0.0);
    let b = arg.sqrt().atan2(cosb);
    let arc = (cosgc - cosco * cosb) / (sinco * b.sin());
    let arg = (1.0 - arc * arc).max(0.0);
    let rdlon = arg.sqrt().atan2(arc);
    let rlat = (PI / 2.0 - b.abs()) * RTOD;
    // DSIGN takes the magnitude of the first argument and the sign of
    // the second.
    let rlat = rlat.abs() * if cosb < 0.0 { -1.0 } else { 1.0 };
    let rlon = if ztaz > 180.0 {
        ztlon - rdlon.abs() * RTOD
    } else {
        ztlon + rdlon.abs() * RTOD
    };
    (rlat as R, rlon as R)
}

/// `XLIMIT6`: clamps a value so it fits an `F6.i` field, `i` 1 to 4.
pub fn xlimit6(x: R, i: usize) -> R {
    const XMIN: [R; 4] = [-999.9, -99.99, -9.999, -0.9999];
    const XMAX: [R; 4] = [9999.9, 999.99, 99.999, 9.9999];
    let (lo, hi) = (XMIN[i - 1], XMAX[i - 1]);
    if x < lo {
        lo
    } else if x > hi {
        hi
    } else {
        x
    }
}

/// `PWRCUT`: the fraction of transmit power that could be cut, by George
/// Lane's algorithm — the area under the assumed normal distribution of
/// signal-to-noise ratios over the days of the month.
///
/// The eleven-point distribution is built from the median and the two
/// decile deviations, then the fraction of days exceeding each of the
/// half-power and quarter-power limits is interpolated within it.
pub fn pwrcut(snr50: R, snr_lw: R, snr_up: R, snr88: R, snr91: R) -> R {
    const FACT: [R; 4] = [1.28, 0.84, 0.525, 0.255];
    let std_lw = snr_lw / 1.28;
    let std_up = snr_up / 1.28;
    let mut snr = [0.0 as R; 11];
    snr[10] = snr50 - FACT[0] * std_lw * 2.0;
    snr[9] = snr50 - FACT[0] * std_lw;
    snr[8] = snr50 - FACT[1] * std_lw;
    snr[7] = snr50 - FACT[2] * std_lw;
    snr[6] = snr50 - FACT[3] * std_lw;
    snr[5] = snr50;
    snr[4] = snr50 + FACT[3] * std_up;
    snr[3] = snr50 + FACT[2] * std_up;
    snr[2] = snr50 + FACT[1] * std_up;
    snr[1] = snr50 + FACT[0] * std_up;
    snr[0] = snr50 + FACT[0] * std_up * 2.0;
    let day3db = dayinterp(&snr, snr88);
    let day6db = dayinterp(&snr, snr91);
    1.0 - (1.0 - day3db) - (day3db - day6db) / 2.0 - day6db / 4.0
}

/// `DAYINTERP`: the fraction of days whose signal-to-noise ratio exceeds
/// `snrx`, interpolated in the eleven-point distribution.
fn dayinterp(snr: &[R; 11], snrx: R) -> R {
    if snrx > snr[0] {
        return 0.0;
    }
    for i in 0..10 {
        if snrx <= snr[i] && snrx >= snr[i + 1] {
            return ((i as R) + (snr[i] - snrx) / (snr[i] - snr[i + 1])) / 10.0;
        }
    }
    1.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xlimit6_clamps_to_its_field() {
        assert_eq!(xlimit6(12.5, 1), 12.5);
        assert_eq!(xlimit6(-2000.0, 1), -999.9);
        assert_eq!(xlimit6(1000.0, 2), 999.99);
        assert_eq!(xlimit6(-1.0, 4), -0.9999);
    }

    /// A median far above both limits cuts the most power the algorithm
    /// allows, and one far below cuts none.
    #[test]
    fn the_power_cut_spans_its_documented_range() {
        assert!((pwrcut(200.0, 10.0, 10.0, 88.0, 91.0) - 0.75).abs() < 1e-6);
        assert_eq!(pwrcut(10.0, 10.0, 10.0, 88.0, 91.0), 0.0);
    }

    /// The reference's own first grid point for the distributed area
    /// file: a 1414 km diagonal southwest of Tangier.
    #[test]
    fn the_first_grid_point_matches_the_reference() {
        let grid = Grid {
            projection: Projection::GreatCircle,
            plat: 35.80,
            plon: -5.90,
            xmin: -1000.0,
            xmax: 6000.0,
            ymin: -1000.0,
            ymax: 4000.0,
            nx: 9,
            ny: 9,
        };
        let (lon, lat) = grid.point(1, 1);
        assert!((lat - 26.3797).abs() < 0.0002, "lat {lat}");
        assert!((lon - 344.0913).abs() < 0.0002, "lon {lon}");
    }

    #[test]
    fn a_lat_lon_grid_is_its_own_rectangle() {
        let grid = Grid {
            projection: Projection::LatLon,
            plat: 0.0,
            plon: 0.0,
            xmin: -10.0,
            xmax: 10.0,
            ymin: 40.0,
            ymax: 50.0,
            nx: 3,
            ny: 3,
        };
        assert_eq!(grid.point(1, 1), (350.0, 40.0));
        assert_eq!(grid.point(3, 3), (10.0, 50.0));
        assert_eq!(grid.point(2, 2), (0.0, 45.0));
    }
}
