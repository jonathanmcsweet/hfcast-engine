//! Which behaviour the engine reproduces: VOACAP's, or VOACAP's with
//! its defects fixed.
//!
//! ## Why this file exists at all
//!
//! The port is bug-compatible on purpose. That is what makes
//! "identical to the reference" a checkable claim rather than an
//! opinion, and it is the only property the whole verification method
//! rests on. But several of the reproduced behaviours are plainly
//! defects, documented at their sites and in `docs/roadmap.md`, and
//! the point of having a readable engine is to be able to fix them.
//!
//! So both live in one engine, chosen at run time. Not two
//! codebases: a fork would double the maintenance and, worse, would
//! let the two drift apart everywhere rather than only where a fix
//! was intended. Not a Cargo feature: features are additive, so two
//! dependents of this crate could silently change each other's
//! numbers, and compile-time selection would make it impossible to
//! run both behaviours in one process — which is exactly what
//! measuring a fix requires.
//!
//! ## The rules this file keeps
//!
//! 1. **Every divergence is named here.** Engine code never asks
//!    "am I corrected?"; it asks a question about one defect, like
//!    [`Model::reads_pole_file`]. The methods below are therefore the
//!    complete list of ways the two tiers can differ. Counting them
//!    counts the divergence.
//! 2. **Two configurations are public**: all-off and all-on. The
//!    per-defect field is deliberately not public, because the 2^n
//!    combinations in between are a measurement tool, not a promise
//!    anyone should build on.
//! 3. **Point defects only.** A fix that is one branch at one
//!    documented site belongs here. Anything pervasive — `f32` to
//!    `f64`, evaluation order, the stale state that persists between
//!    hours and between calls — does not, because a flag cannot
//!    honestly describe it and the result would not be VOACAP with a
//!    fix but a different model. Those belong to a later tier's
//!    structural work.
//!
//! ## Adding a fix
//!
//! Add a field to [`Fixes`], a method that reads it, and take the
//! branch at the one site the defect lives at. Then measure it: a
//! differential test recording exactly which outputs move, and a run
//! of the WSPR validation with only that fix on. `docs/corrected.md`
//! holds the results, including for any fix that measured worse and
//! was therefore left off.

/// Which of the documented defects are fixed.
///
/// Not public: a caller chooses [`Model::Compatible`] or
/// [`Model::Corrected`], and the combinations exist only so a
/// measurement can attribute a change to one fix.
///
/// Every field here is unused until the fix it names is implemented
/// and takes its branch at the defect's own site. That is why the
/// allow is on the whole type rather than on individual fields: a
/// field with no reader yet is the expected state, and the allow
/// comes off when the last fix lands.
#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) struct Fixes {
    /// `MagneticPole::for_tree` builds the database path without a
    /// separator, so the installed `database/north_pole.txt` is never
    /// read and every run uses the built-in pole.
    pub(crate) pole_file: bool,
    /// The IONCAP curtain compares an elevation against `0001` — one
    /// radian, where `.0001` was meant — so every elevation above
    /// about 33 degrees takes the floor gain.
    pub(crate) curtain_elevation: bool,
    /// The no-LUF-found scan never reassigns its running best, so it
    /// compares every slot against slot 1 and returns the last slot
    /// beating slot 1 rather than the most reliable one.
    pub(crate) luf_scan_best: bool,
    /// The short LUF pass reads its modes from whichever column was
    /// written last, because `FINDF` and `FDIST` take the area as an
    /// argument while the mode routines set it internally.
    pub(crate) luf_pass_area: bool,
    /// An area run's nudge off the transmitter compares a folded
    /// longitude against an unfolded one, so a transmitter at a
    /// negative longitude computes a zero-length path at the grid's
    /// own centre.
    pub(crate) area_centre_nudge: bool,
    /// `GAIN`'s area branch picks the path bearing from the antenna's
    /// position in the list rather than from the end it serves.
    pub(crate) area_antenna_end: bool,
}

#[allow(dead_code)]
impl Fixes {
    const NONE: Fixes = Fixes {
        pole_file: false,
        curtain_elevation: false,
        luf_scan_best: false,
        luf_pass_area: false,
        area_centre_nudge: false,
        area_antenna_end: false,
    };

    const ALL: Fixes = Fixes {
        pole_file: true,
        curtain_elevation: true,
        luf_scan_best: true,
        luf_pass_area: true,
        area_centre_nudge: true,
        area_antenna_end: true,
    };
}

/// Which behaviour a run reproduces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Model {
    /// VOACAP as it is, defects included. Byte-identical to the
    /// reference, and the only tier any of the harnesses can judge.
    #[default]
    Compatible,
    /// VOACAP with its documented defects fixed. Deliberately not
    /// identical to the reference; `docs/corrected.md` records what
    /// each fix changes and what it measured against real radio.
    Corrected,
}

/// The accessors below have no callers until their fix is
/// implemented. Each loses this allow when its site starts reading
/// it.
#[allow(dead_code)]
impl Model {
    pub(crate) fn fixes(self) -> Fixes {
        match self {
            Model::Compatible => Fixes::NONE,
            Model::Corrected => Fixes::ALL,
        }
    }

    /// Reads the magnetic pole from the tree's `database` directory,
    /// which the reference's malformed path never finds.
    pub(crate) fn reads_pole_file(self) -> bool {
        self.fixes().pole_file
    }

    /// Compares a curtain elevation against `.0001` radians rather
    /// than `0001`.
    pub(crate) fn curtain_elevation_threshold(self) -> bool {
        self.fixes().curtain_elevation
    }

    /// Keeps the running best while scanning for the most reliable
    /// frequency when no LUF was found.
    pub(crate) fn luf_scan_reassigns(self) -> bool {
        self.fixes().luf_scan_best
    }

    /// Reads the short LUF pass's modes from the area it built its
    /// raysets for.
    pub(crate) fn luf_pass_reads_own_area(self) -> bool {
        self.fixes().luf_pass_area
    }

    /// Compares both longitudes the same way when nudging a grid
    /// point off the transmitter.
    pub(crate) fn area_nudge_compares_alike(self) -> bool {
        self.fixes().area_centre_nudge
    }

    /// Aims an area antenna by the end it serves rather than by its
    /// position in the card list.
    pub(crate) fn area_antenna_by_end(self) -> bool {
        self.fixes().area_antenna_end
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_fixes_nothing_and_corrected_fixes_everything() {
        let c = Model::Compatible;
        assert!(!c.reads_pole_file());
        assert!(!c.curtain_elevation_threshold());
        assert!(!c.luf_scan_reassigns());
        assert!(!c.luf_pass_reads_own_area());
        assert!(!c.area_nudge_compares_alike());
        assert!(!c.area_antenna_by_end());

        let f = Model::Corrected;
        assert!(f.reads_pole_file());
        assert!(f.curtain_elevation_threshold());
        assert!(f.luf_scan_reassigns());
        assert!(f.luf_pass_reads_own_area());
        assert!(f.area_nudge_compares_alike());
        assert!(f.area_antenna_by_end());
    }

    #[test]
    fn the_default_is_the_behaviour_the_harnesses_can_judge() {
        // Every harness compares against the Fortran reference, so a
        // default of anything else would make them all fail and mean
        // a caller who said nothing got unverified numbers.
        assert_eq!(Model::default(), Model::Compatible);
    }

    /// `ALL` must set every field. A fix added to [`Fixes`] but not to
    /// `ALL` would be dead in `Corrected` and nothing else would say
    /// so.
    #[test]
    fn every_fix_is_on_in_corrected() {
        // Field-by-field rather than a struct comparison, so adding a
        // field without adding it here fails to compile rather than
        // passing quietly.
        let Fixes {
            pole_file,
            curtain_elevation,
            luf_scan_best,
            luf_pass_area,
            area_centre_nudge,
            area_antenna_end,
        } = Model::Corrected.fixes();
        assert!(pole_file);
        assert!(curtain_elevation);
        assert!(luf_scan_best);
        assert!(luf_pass_area);
        assert!(area_centre_nudge);
        assert!(area_antenna_end);
    }
}
