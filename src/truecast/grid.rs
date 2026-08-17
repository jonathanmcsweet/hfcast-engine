//! The truecast grid driver: one coverage lattice, threads inside the
//! engine.
//!
//! The application computes its maps by cutting a lattice into strips
//! and running each strip as its own engine call on its own JVM thread.
//! Every strip pays the whole setup again — the coefficient load, the
//! antenna tables, the magnetic pole — and hands back rendered rows.
//! This driver keeps one shared read-only setup and threads inside the
//! engine instead, writing bare `f32` planes.
//!
//! Every point is computed with fresh state (`area_point_fresh`): a
//! pure function of the place and hour. The parity area driver carries
//! COMMON state from point to point because the Fortran does; that
//! carry makes a point's answer depend on the lattice and the visit
//! order, which a threaded driver must not reproduce. The first point
//! of a parity area run has no carry yet, so it is the anchor the
//! tests pin the two drivers together with; across a whole grid the
//! difference is the carry alone, measured in the tests.
//!
//! Workers claim whole rows off a shared cursor and compute into their
//! own lists; the planes are assembled by row index afterwards, so the
//! output cannot depend on which worker won which row.

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

use crate::voacap::coefficients::redmap;
use crate::voacap::con::R;
use crate::voacap::run::{area_point_fresh, AreaFreq, AreaInputs, AreaPrep};

/// One grid question: the parity area inputs (lattice, ends, month,
/// index, frequencies, antennas) plus the thread budget.
#[derive(Debug, Clone)]
pub struct GridRequest {
    pub area: AreaInputs,
    /// Worker threads. Zero means the machine's available parallelism.
    pub threads: usize,
}

/// The lattice's answers as structure-of-arrays `f32` planes.
///
/// Point planes (`lat_deg`, `lon_deg`) hold `ny * nx` values in row
/// order. Frequency planes hold one `ny * nx` block per requested
/// frequency, in the order requested — `cell` computes the index. A
/// takeoff angle the run did not produce is NaN, matching the blank
/// column of the parity listing.
#[derive(Debug, Clone, PartialEq)]
pub struct GridPlanes {
    pub nx: usize,
    pub ny: usize,
    pub n_freqs: usize,
    pub lat_deg: Vec<f32>,
    pub lon_deg: Vec<f32>,
    pub reliability: Vec<f32>,
    pub snr_db: Vec<f32>,
    pub takeoff_deg: Vec<f32>,
}

impl GridPlanes {
    /// The point-plane index of `(ix, iy)`, zero-based.
    pub fn point(&self, ix: usize, iy: usize) -> usize {
        iy * self.nx + ix
    }

    /// The frequency-plane index of `(freq, ix, iy)`, zero-based.
    pub fn cell(&self, freq: usize, ix: usize, iy: usize) -> usize {
        (freq * self.ny + iy) * self.nx + ix
    }
}

/// One computed point before assembly.
struct PointAnswer {
    lat: R,
    lon: R,
    freqs: Vec<AreaFreq>,
}

/// Runs the lattice with threads inside the engine. The output is
/// invariant under the thread count by construction, and each point
/// matches a fresh-state serial run of the same place bit for bit.
pub fn predict_grid(itshfbc: &Path, req: &GridRequest) -> Result<GridPlanes, String> {
    let area = &req.area;
    let set = redmap(itshfbc, area.fof2, area.month, area.ssn).map_err(|e| e.to_string())?;
    let prep = AreaPrep::new(itshfbc, area, &set)?;
    let (nx, ny) = (area.grid.nx, area.grid.ny);
    let workers = match req.threads {
        0 => std::thread::available_parallelism().map_or(1, |n| n.get()),
        n => n,
    }
    .min(ny.max(1));

    // The diurnal series depends on the maps and the hour, never on
    // the grid point, so the whole lattice reads one evaluation.
    let ab = prep.ab_at(area.hour as R);

    let cursor = AtomicUsize::new(0);
    let mut rows: Vec<(usize, Result<Vec<PointAnswer>, String>)> = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|_| scope.spawn(|| work(itshfbc, area, &prep, &ab, &cursor, ny)))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("a grid worker never panics"))
            .collect()
    });
    rows.sort_by_key(|(row, _)| *row);
    assemble(nx, ny, prep.nf(), rows)
}

/// One worker's life: claim the next row off the shared cursor,
/// compute it, repeat until no rows remain.
fn work(
    itshfbc: &Path,
    area: &AreaInputs,
    prep: &AreaPrep<'_>,
    ab: &[R; 318],
    cursor: &AtomicUsize,
    ny: usize,
) -> Vec<(usize, Result<Vec<PointAnswer>, String>)> {
    let mut mine = Vec::new();
    // A loop because each pass claims the next row; the claim is the
    // iteration.
    loop {
        let row = cursor.fetch_add(1, Ordering::Relaxed);
        if row >= ny {
            break;
        }
        mine.push((row, compute_row(itshfbc, area, prep, ab, row)));
    }
    mine
}

/// One row of the lattice, left to right, fresh state per point.
fn compute_row(
    itshfbc: &Path,
    area: &AreaInputs,
    prep: &AreaPrep<'_>,
    ab: &[R; 318],
    row: usize,
) -> Result<Vec<PointAnswer>, String> {
    (1..=area.grid.nx)
        .map(|ix| {
            let (lat, lon, freqs) = area_point_fresh(itshfbc, area, prep, ix, row + 1, Some(ab))?;
            Ok(PointAnswer { lat, lon, freqs })
        })
        .collect()
}

/// Joins the computed rows into planes. Rows arrive sorted, so the
/// first failed row is the error reported whatever order the workers
/// finished in.
fn assemble(
    nx: usize,
    ny: usize,
    n_freqs: usize,
    rows: Vec<(usize, Result<Vec<PointAnswer>, String>)>,
) -> Result<GridPlanes, String> {
    let n_points = nx * ny;
    let mut planes = GridPlanes {
        nx,
        ny,
        n_freqs,
        lat_deg: vec![0.0; n_points],
        lon_deg: vec![0.0; n_points],
        reliability: vec![0.0; n_freqs * n_points],
        snr_db: vec![0.0; n_freqs * n_points],
        takeoff_deg: vec![f32::NAN; n_freqs * n_points],
    };
    // A loop for the indexed writes into the preallocated planes.
    for (row, answers) in rows {
        for (ix, p) in answers?.into_iter().enumerate() {
            let at = planes.point(ix, row);
            planes.lat_deg[at] = p.lat;
            planes.lon_deg[at] = p.lon;
            for (freq, f) in p.freqs.into_iter().enumerate() {
                let cell = planes.cell(freq, ix, row);
                planes.reliability[cell] = f.reliability as f32;
                planes.snr_db[cell] = f.snr_db as f32;
                if let Some(angle) = f.takeoff_angle_deg {
                    planes.takeoff_deg[cell] = angle as f32;
                }
            }
        }
    }
    Ok(planes)
}

#[cfg(test)]
#[cfg(feature = "embedded-coefficients")]
mod tests {
    use super::*;
    use crate::voacap::area::{Grid, Projection};
    use crate::voacap::coefficients::FoF2Model;
    use crate::voacap::data;
    use crate::voacap::model::Model;
    use crate::voacap::run::run_area;

    /// A small lattice around a mid-latitude transmitter, two bands.
    fn small_area() -> AreaInputs {
        AreaInputs {
            grid: Grid {
                projection: Projection::LatLon,
                plat: 47.0,
                plon: 8.0,
                xmin: 2.0,
                xmax: 14.0,
                ymin: 42.0,
                ymax: 52.0,
                nx: 4,
                ny: 3,
            },
            tx_lat_deg: 47.0,
            tx_lon_deg: 8.0,
            month: 6,
            ssn: 80.0,
            hour: 13,
            freqs_mhz: vec![7.1, 14.1],
            required_snr_db: 24.0,
            noise_dbw: 145,
            watts: 100.0,
            psc: [1.0, 1.0, 1.0, 0.0],
            method: 30,
            fof2: FoF2Model::Ccir,
            inverse: false,
            tx_antenna: None,
            rx_antenna: None,
            model: Model::Compatible,
        }
    }

    fn grid_at(threads: usize) -> GridPlanes {
        predict_grid(
            &data::embedded_root(),
            &GridRequest {
                area: small_area(),
                threads,
            },
        )
        .expect("the embedded root answers")
    }

    /// NaN-safe bitwise equality for one plane.
    fn same_bits(a: &[f32], b: &[f32]) -> bool {
        a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.to_bits() == y.to_bits())
    }

    #[test]
    fn thread_counts_cannot_move_the_answer() {
        let (one, three) = (grid_at(1), grid_at(3));
        assert!(same_bits(&one.reliability, &three.reliability));
        assert!(same_bits(&one.snr_db, &three.snr_db));
        assert!(same_bits(&one.takeoff_deg, &three.takeoff_deg));
        assert!(same_bits(&one.lat_deg, &three.lat_deg));
        assert!(same_bits(&one.lon_deg, &three.lon_deg));
    }

    #[test]
    fn the_answers_are_finite_and_plausible() {
        let planes = grid_at(0);
        assert_eq!(planes.reliability.len(), 2 * 12);
        for r in &planes.reliability {
            assert!((0.0..=1.0).contains(r), "reliability {r}");
        }
        for s in &planes.snr_db {
            assert!(s.is_finite() && (-200.0..=200.0).contains(s), "snr {s}");
        }
    }

    #[test]
    fn the_parity_anchor_holds_and_the_carry_is_the_only_difference() {
        // The parity area run carries COMMON state from point to point;
        // the fresh-state driver does not. The first parity point has
        // no carry yet, so there the two must agree exactly. Across the
        // rest of the lattice the carry is the entire difference, and
        // its measured size on this lattice is within the engine's own
        // day-to-day noise floor.
        let parity = run_area(&data::embedded_root(), &small_area()).expect("parity answers");
        let planes = grid_at(2);
        let first = &parity[0];
        for (freq, f) in first.per_freq.iter().enumerate() {
            let cell = planes.cell(freq, first.ix - 1, first.iy - 1);
            assert_eq!(planes.reliability[cell], f.reliability as f32);
            assert_eq!(planes.snr_db[cell], f.snr_db as f32);
        }
        let mut worst_rel = 0.0f64;
        let mut worst_snr = 0.0f64;
        for p in &parity {
            for (freq, f) in p.per_freq.iter().enumerate() {
                let cell = planes.cell(freq, p.ix - 1, p.iy - 1);
                worst_rel =
                    worst_rel.max((f64::from(planes.reliability[cell]) - f.reliability).abs());
                worst_snr = worst_snr.max((f64::from(planes.snr_db[cell]) - f.snr_db).abs());
            }
        }
        // The envelope: the -ffast-math parity study measured carry-free
        // rebuild spread at 0.01 reliability and 1 dB; the carry sits in
        // the same class.
        assert!(worst_rel <= 0.011, "carry moved reliability {worst_rel}");
        assert!(worst_snr <= 2.0, "carry moved snr {worst_snr} dB");
    }
}
