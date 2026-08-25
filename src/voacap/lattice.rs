//! A quantity sampled on a fixed latitude and longitude lattice
//! instead of at every control point.
//!
//! A world grid asks the ionosphere chain about 173,000 questions and
//! almost every one is at a different place, so the chain runs almost
//! 173,000 times. Parts of that chain are functions of position and
//! nothing else, and two control points at the same place get the same
//! answer from those parts however different the paths they belong to.
//! So the deviation is to compute such a part on a regular lattice and
//! read it at each control point instead. At 5 degrees the same world
//! grid needs about 2,600 nodes, sixty times fewer.
//!
//! ## What may be put on a lattice, and what may not
//!
//! Only a quantity that is a function of position alone. That has to
//! be checked against the code rather than assumed, and the check has
//! already caught one mistake: the finished per-area ionogram looks
//! like a function of position and is not, because `ionset` builds a
//! slot from two different control points (`muf.rs`, the five-point
//! branch: slot 1 takes its E and F1 layers from point 1 and its F2
//! layer from point 2). A lattice of ionograms would have given slot 1
//! the F2 layer at the wrong place.
//!
//! `layer_parameters` is a function of position: it reads a control
//! point's latitude, longitude and geomagnetic latitude and never its
//! distance along the path, and `magvar` is a function of position
//! too. So the layer parameters go on a lattice and everything after
//! `ionset` stays where it is.
//!
//! ## What this is not
//!
//! It is not free accuracy. Two control points inside one lattice cell
//! now share an answer they would not have shared, and the error that
//! introduces is largest where the ionosphere changes fastest: the
//! sunrise and sunset line, where the critical frequency moves sharply
//! over a few hundred kilometres, and the boundary where the F1 layer
//! appears and disappears. `docs/roadmap.md` records what the deviation
//! is predicted to do before it was scored, so that the scoring is a
//! test rather than a search.
//!
//! ## Reading between nodes
//!
//! [`Interp::Bilinear`] blends the four surrounding nodes, which keeps
//! the answer continuous across a cell boundary. [`Interp::Nearest`]
//! takes the closest node, which is cheaper and leaves a discontinuity
//! at every cell edge. Both are here because the arc's rule is to
//! measure an argument rather than settle it in prose.
//!
//! A payload says how to blend itself, through [`Blend`]. A field that
//! is not continuous in position must not be blended: local mean time
//! runs from 0 to 24 and jumps back, so blending it across the jump
//! gives noon where midnight belongs. Such a field is recomputed at the
//! reading position instead, which for local mean time is exact and
//! costs an add.
//!
//! ## Lifetime and threads
//!
//! A lattice is valid for one month, sunspot index, hour and
//! coefficient set, because that is what the closure passed to
//! [`Lattice::at`] captures. It holds no context of its own and so
//! cannot go stale against one: a caller that changes the hour builds a
//! new lattice or gets the wrong answer from its own closure, and the
//! borrow checker keeps the two together. Each worker thread owns one,
//! which costs a duplicate build per thread and buys freedom from locks
//! and from any dependence on which thread reached a node first.

use std::collections::HashMap;

use super::con::{D2R, R};

/// How a payload is blended between two nodes.
pub trait Blend {
    /// This value `f` of the way to `other`, where `f` runs 0 to 1.
    ///
    /// A field that is not continuous in position must be left out and
    /// recomputed by the caller at the reading position.
    fn blend(&self, other: &Self, f: R) -> Self;
}

/// Thirty values blended, `f` of the way from `a` to `b`.
pub fn blend30(a: &[R; 30], b: &[R; 30], f: R) -> [R; 30] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
}

/// Three values blended, `f` of the way from `a` to `b`.
pub fn blend3(a: &[R; 3], b: &[R; 3], f: R) -> [R; 3] {
    std::array::from_fn(|i| a[i] + (b[i] - a[i]) * f)
}

/// One value blended, `f` of the way from `a` to `b`.
pub fn blend1(a: R, b: R, f: R) -> R {
    a + (b - a) * f
}

/// How a read between nodes is answered.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Interp {
    /// The closest node, discontinuous at every cell edge.
    Nearest,
    /// The four surrounding nodes blended, continuous everywhere the
    /// payload is.
    Bilinear,
}

/// Payloads on a regular latitude and longitude lattice, built on
/// demand.
///
/// Rows run from the south pole to the north pole inclusive, so a
/// spacing has to divide 180. Columns run east from 180 west and wrap,
/// so a spacing has to divide 360.
#[derive(Debug, Clone)]
pub struct Lattice<T> {
    step_deg: R,
    n_lat: usize,
    n_lon: usize,
    interp: Interp,
    nodes: HashMap<u32, T>,
    hits: u64,
    misses: u64,
}

impl<T: Blend + Clone> Lattice<T> {
    /// A lattice of the given spacing in degrees, holding nothing yet.
    ///
    /// The spacing must divide both 180 and 360, which 1, 2.5 and 5 all
    /// do. A spacing that does not would put no row on a pole and leave
    /// the wrap at 180 degrees between two nodes rather than on one.
    pub fn new(step_deg: R, interp: Interp) -> Self {
        debug_assert!(step_deg > 0.0, "lattice spacing must be positive");
        let rows = 180.0 / step_deg;
        let cols = 360.0 / step_deg;
        debug_assert!(
            (rows - rows.round()).abs() < 1e-4 && (cols - cols.round()).abs() < 1e-4,
            "lattice spacing {step_deg} does not divide 180 and 360"
        );
        Self {
            step_deg,
            n_lat: rows.round() as usize + 1,
            n_lon: cols.round() as usize,
            interp,
            nodes: HashMap::new(),
            hits: 0,
            misses: 0,
        }
    }

    /// The payload at a position in radians, building whatever nodes
    /// the read needs.
    ///
    /// `build` takes a node's latitude and longitude in radians and
    /// returns what the unmodified chain would produce there. It is
    /// called once per node over the lattice's life and never for a
    /// node already held.
    pub fn at(&mut self, lat_rad: R, lon_rad: R, build: impl Fn(R, R) -> T + Copy) -> T {
        let (y, x) = self.node_coords(lat_rad, lon_rad);
        match self.interp {
            Interp::Nearest => {
                let i = (y.round() as usize).min(self.n_lat - 1);
                let j = (x.round() as i64).rem_euclid(self.n_lon as i64) as usize;
                self.node(i, j, build)
            }
            Interp::Bilinear => {
                // The southern row of the cell, held one row below the
                // north pole so that a read exactly on the pole still
                // has a row above it to blend with.
                let i0 = (y.floor().max(0.0) as usize).min(self.n_lat - 2);
                let fy = (y - i0 as R).clamp(0.0, 1.0);
                let j0 = (x.floor() as i64).rem_euclid(self.n_lon as i64) as usize;
                let fx = x - x.floor();
                let j1 = (j0 + 1) % self.n_lon;
                let south = self
                    .node(i0, j0, build)
                    .blend(&self.node(i0, j1, build), fx);
                let north = self
                    .node(i0 + 1, j0, build)
                    .blend(&self.node(i0 + 1, j1, build), fx);
                south.blend(&north, fy)
            }
        }
    }

    /// Nodes answered from the lattice, and nodes built.
    ///
    /// Counted because two caches on this hot path silently measured
    /// nothing before anybody noticed: one was keyed past an early
    /// return and one was placed on a chain the grid never ran. A cache
    /// that cannot report its own hits is a cache nobody has checked.
    pub fn counts(&self) -> (u64, u64) {
        (self.hits, self.misses)
    }

    /// The lattice position of a point, in units of the spacing: rows
    /// north from the south pole, columns east from 180 west.
    fn node_coords(&self, lat_rad: R, lon_rad: R) -> (R, R) {
        let lat_deg = (lat_rad / D2R).clamp(-90.0, 90.0);
        let lon_deg = (lon_rad / D2R + 180.0).rem_euclid(360.0);
        ((lat_deg + 90.0) / self.step_deg, lon_deg / self.step_deg)
    }

    /// One node, built on the first read of it.
    fn node(&mut self, i: usize, j: usize, build: impl Fn(R, R) -> T) -> T {
        let key = (i * self.n_lon + j) as u32;
        if let Some(held) = self.nodes.get(&key) {
            self.hits += 1;
            return held.clone();
        }
        self.misses += 1;
        let lat = (-90.0 + i as R * self.step_deg) * D2R;
        let lon = (-180.0 + j as R * self.step_deg) * D2R;
        let built = build(lat, lon);
        self.nodes.insert(key, built.clone());
        built
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A payload that carries where it was built, so a read can be
    /// checked against the position it should have come from.
    #[derive(Debug, Clone, Copy, PartialEq)]
    struct Marker {
        lat_deg: R,
        lon_deg: R,
    }

    impl Blend for Marker {
        fn blend(&self, other: &Self, f: R) -> Self {
            Marker {
                lat_deg: blend1(self.lat_deg, other.lat_deg, f),
                lon_deg: blend1(self.lon_deg, other.lon_deg, f),
            }
        }
    }

    fn marker(lat: R, lon: R) -> Marker {
        Marker {
            lat_deg: lat / D2R,
            lon_deg: lon / D2R,
        }
    }

    fn near(a: R, b: R) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_node_read_at_its_own_position_is_itself() {
        let mut lat = Lattice::new(5.0, Interp::Bilinear);
        let got = lat.at(45.0 * D2R, 20.0 * D2R, marker);
        assert!(near(got.lat_deg, 45.0), "{}", got.lat_deg);
        assert!(near(got.lon_deg, 20.0), "{}", got.lon_deg);
    }

    #[test]
    fn the_middle_of_a_cell_is_the_average_of_its_corners() {
        let mut lat = Lattice::new(5.0, Interp::Bilinear);
        let got = lat.at(42.5 * D2R, 22.5 * D2R, marker);
        assert!(near(got.lat_deg, 42.5), "{}", got.lat_deg);
        assert!(near(got.lon_deg, 22.5), "{}", got.lon_deg);
        assert_eq!(lat.counts(), (0, 4), "four corners, each built once");
    }

    #[test]
    fn a_second_read_of_the_same_cell_builds_nothing() {
        let mut lat = Lattice::new(5.0, Interp::Bilinear);
        lat.at(42.5 * D2R, 22.5 * D2R, marker);
        lat.at(41.0 * D2R, 21.0 * D2R, marker);
        assert_eq!(lat.counts(), (4, 4), "the second read is four hits");
    }

    #[test]
    fn longitude_wraps_at_the_date_line() {
        let mut lat = Lattice::new(5.0, Interp::Nearest);
        let west = lat.at(0.0, -180.0 * D2R, marker);
        let east = lat.at(0.0, 180.0 * D2R, marker);
        assert_eq!(west, east, "180 east and 180 west are one column");
        assert_eq!(lat.counts(), (1, 1), "and it is one node, not two");
    }

    #[test]
    fn a_cell_spanning_the_date_line_blends_both_sides() {
        let mut lat = Lattice::new(5.0, Interp::Bilinear);
        // 177.5E sits halfway between the nodes at 175E and 180E, and
        // the node at 180E is the one at 180W, column zero.
        let got = lat.at(0.0, 177.5 * D2R, marker);
        assert!(near(got.lon_deg, (175.0 + -180.0) / 2.0), "{}", got.lon_deg);
        assert_eq!(lat.counts().1, 4, "two columns, two rows");
    }

    #[test]
    fn a_read_on_the_north_pole_stays_inside_the_lattice() {
        let mut lat = Lattice::new(5.0, Interp::Bilinear);
        let got = lat.at(90.0 * D2R, 20.0 * D2R, marker);
        assert!(near(got.lat_deg, 90.0), "{}", got.lat_deg);
    }

    #[test]
    fn a_read_on_the_south_pole_stays_inside_the_lattice() {
        let mut lat = Lattice::new(5.0, Interp::Bilinear);
        let got = lat.at(-90.0 * D2R, 20.0 * D2R, marker);
        assert!(near(got.lat_deg, -90.0), "{}", got.lat_deg);
    }

    #[test]
    fn nearest_takes_the_closer_node() {
        let mut lat = Lattice::new(5.0, Interp::Nearest);
        assert!(near(lat.at(41.0 * D2R, 0.0, marker).lat_deg, 40.0));
        assert!(near(lat.at(44.0 * D2R, 0.0, marker).lat_deg, 45.0));
        assert_eq!(lat.counts(), (0, 2), "two different nodes");
    }

    #[test]
    fn every_spacing_the_scoring_uses_puts_a_row_on_each_pole() {
        // 1, 2.5 and 5 degrees are what `docs/roadmap.md` says to score.
        [1.0, 2.5, 5.0].into_iter().for_each(|step| {
            let mut lat = Lattice::new(step, Interp::Bilinear);
            let north = lat.at(90.0 * D2R, 0.0, marker);
            let south = lat.at(-90.0 * D2R, 0.0, marker);
            assert!(near(north.lat_deg, 90.0), "{step} north {}", north.lat_deg);
            assert!(near(south.lat_deg, -90.0), "{step} south {}", south.lat_deg);
        });
    }
}
