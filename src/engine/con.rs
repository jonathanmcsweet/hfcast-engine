//! Physical constants and configuration the Fortran keeps in `/CON/` and
//! reads at startup (`blkdat.for`, `set_magnetic_pole` in `voacapw.for`).
//!
//! Values are written exactly as the Fortran DATA statements spell them, in
//! `f32`, because the port's job is to be the same model. `PI` here is the
//! engine's `3.1415926`, not Rust's — the difference is real at f32 branch
//! thresholds.

// These constants must be the Fortran DATA values, digit for digit — using
// Rust's own PI or "correcting" the precision would make this a different
// model at f32 branch thresholds. The lints below exist to catch exactly
// what this file does on purpose.
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use std::path::{Path, PathBuf};

use super::model::Model;

/// The working precision of the engine, matching Fortran 4-byte REAL.
pub type R = f32;

pub const D2R: R = 0.01745329251;
pub const R2D: R = 57.295779513;
pub const PI: R = 3.1415926;
pub const PI2: R = 6.283185307;
pub const PIO2: R = 1.570796326;
/// Earth radius, km.
pub const RZ: R = 6370.0;
/// Speed of light, m per ms.
pub const VOFL: R = 299.79246;
/// Euler-Mascheroni constant, used by the distribution code.
pub const GAMA: R = 0.57721566;
pub const DCL: R = 1.28;

/// The geomagnetic north pole the run uses, degrees.
///
/// `set_magnetic_pole`: a `north_pole.txt` in the run directory overrides
/// one in the database directory, which overrides the built-in
/// (78.5, -69.0). Out-of-range values fall through to the next source.
///
/// Bug kept on purpose: the Fortran builds the database path without a
/// separator (`trim(root_directory)//'database'//…`, unlike every other
/// path in the program), producing `<root>database/north_pole.txt`. That
/// file never exists, so the installed database pole (79.5) is silently
/// ignored and the built-in 78.5 is what actually runs — confirmed by the
/// geometry stage trace, which only matches with 78.5. The port reproduces
/// the malformed lookup so it stays wrong in exactly the same way.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MagneticPole {
    pub lat_deg: R,
    pub lon_deg: R,
}

impl Default for MagneticPole {
    fn default() -> Self {
        Self {
            lat_deg: 78.5,
            lon_deg: -69.0,
        }
    }
}

fn pole_from_file(path: &Path) -> Option<MagneticPole> {
    let text = std::fs::read_to_string(path).ok()?;
    let first = text.lines().next()?;
    let mut fields = first.split_whitespace();
    let lat: R = fields.next()?.parse().ok()?;
    let lon: R = fields.next()?.parse().ok()?;
    if !(60.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
        return None;
    }
    Some(MagneticPole {
        lat_deg: lat,
        lon_deg: lon,
    })
}

impl MagneticPole {
    /// The pole for a given `itshfbc` tree, with the Fortran's precedence
    /// (including its broken database path — see the type comment).
    pub fn for_tree(itshfbc: &Path) -> Self {
        Self::for_tree_with(itshfbc, Model::Compatible)
    }

    /// The pole a run uses.
    ///
    /// The reference builds the database path without a separator, so
    /// `<tree>database/north_pole.txt` is what it looks for and the
    /// installed `<tree>/database/north_pole.txt` is never found. Every
    /// run therefore uses the built-in pole, and the file the
    /// distribution ships — which exists precisely so the pole can be
    /// moved — has no effect.
    ///
    /// [`Model::reads_pole_file`] joins the path properly. A
    /// `run/north_pole.txt` still wins, as it does either way, so the
    /// fix only changes runs where the tree has a database file and no
    /// run file.
    pub fn for_tree_with(itshfbc: &Path, model: Model) -> Self {
        let database = if model.reads_pole_file() {
            itshfbc.join("database/north_pole.txt")
        } else {
            PathBuf::from(format!("{}database/north_pole.txt", itshfbc.display()))
        };
        pole_from_file(&itshfbc.join("run/north_pole.txt"))
            .or_else(|| pole_from_file(&database))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pole_default_matches_the_fortran_data() {
        let pole = MagneticPole::default();
        assert_eq!(pole.lat_deg, 78.5);
        assert_eq!(pole.lon_deg, -69.0);
    }

    #[test]
    fn pole_file_precedence_matches_the_broken_fortran_lookup() {
        let base = std::env::temp_dir().join("propcore-pole-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("database")).expect("dirs");
        std::fs::create_dir_all(base.join("run")).expect("dirs");

        // A correctly placed database file is NOT read — the Fortran's
        // malformed path skips it, so the built-in default wins.
        std::fs::write(base.join("database/north_pole.txt"), "79.5 -69.0\nrest\n").expect("write");
        assert_eq!(MagneticPole::for_tree(&base).lat_deg, 78.5);

        // A run file is read; an out-of-range one falls through.
        std::fs::write(base.join("run/north_pole.txt"), "82.0 -82.0\n").expect("write");
        assert_eq!(MagneticPole::for_tree(&base).lat_deg, 82.0);
        std::fs::write(base.join("run/north_pole.txt"), "10.0 -82.0\n").expect("write");
        assert_eq!(MagneticPole::for_tree(&base).lat_deg, 78.5);

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_corrected_tier_reads_the_database_file() {
        let base = std::env::temp_dir().join("propcore-pole-corrected");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("database")).expect("dirs");
        std::fs::create_dir_all(base.join("run")).expect("dirs");
        std::fs::write(base.join("database/north_pole.txt"), "79.5 -69.0\nrest\n").expect("write");

        // The whole defect: the same tree, the same file, read only by
        // the tier that joins the path properly.
        assert_eq!(
            MagneticPole::for_tree_with(&base, Model::Compatible).lat_deg,
            78.5
        );
        assert_eq!(
            MagneticPole::for_tree_with(&base, Model::Corrected).lat_deg,
            79.5
        );

        // A run file still wins on both tiers, so a caller who wants
        // the old pole under `Corrected` can still ask for it.
        std::fs::write(base.join("run/north_pole.txt"), "78.5 -69.0\n").expect("write");
        for model in [Model::Compatible, Model::Corrected] {
            assert_eq!(MagneticPole::for_tree_with(&base, model).lat_deg, 78.5);
        }

        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn constants_match_blkdat() {
        // Spelled exactly as the DATA statements; a typo here would poison
        // every downstream stage.
        assert_eq!(RZ, 6370.0);
        assert_eq!(PI, 3.1415926);
        assert!((D2R * R2D - 1.0).abs() < 1e-6);
    }
}
