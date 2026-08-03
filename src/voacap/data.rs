//! Where the engine's data comes from.
//!
//! The reference reads an `itshfbc` tree from disk, and so did this port:
//! every function that needs a data file takes the tree's root as a `&Path`.
//! That is right for a server and impossible on a phone, where there is no
//! tree and a user cannot be asked to build one.
//!
//! So the root is now a specification rather than only a directory. Three
//! forms, and the plain one is unchanged:
//!
//! | root                  | where a file comes from                     |
//! | --------------------- | ------------------------------------------- |
//! | `/home/me/itshfbc`    | that directory, and nowhere else            |
//! | `<embedded>`          | the bytes compiled into this binary         |
//! | `<embedded>+/var/gen` | that directory first, then the compiled-in bytes |
//!
//! Keeping the signatures as `&Path` is deliberate. Every harness and both
//! production paths pass a root today, and threading a new context type
//! through the whole engine to express three cases would be a wide change to
//! the code the parity harnesses cover, for no gain in what it can express.
//!
//! The third form exists for the application. It generates an antenna
//! definition per station and needs the engine to read it, so it writes that
//! one file to its own cache directory and leaves the 653 KB of coefficients
//! compiled in. A plain directory root never falls back: a server with an
//! incomplete tree must fail loudly rather than quietly predict from
//! something else.
//!
//! What is embedded, and why each part is here, is in
//! `docs/licence.md`. Briefly: the coefficients originate in CCIR Report 340
//! and URSI publications, the antennas and the version file are NTIA/ITS.

use std::borrow::Cow;
use std::io::{Error, ErrorKind, Result};
use std::path::{Path, PathBuf};

/// A root meaning "use the bytes compiled into this binary".
pub const EMBEDDED: &str = "<embedded>";

/// Separates the embedded marker from a directory searched before it.
const OVERLAY: char = '+';

macro_rules! embedded_files {
    ($name:ident, $($rel:literal),* $(,)?) => {
        static $name: &[(&str, &[u8])] = &[
            $(($rel, include_bytes!(concat!("../../embedded/", $rel))),)*
        ];
    };
}

// The files NTIA/ITS wrote, which are a work of the US Government and
// carry no restriction on redistribution. Always compiled in.
embedded_files!(
    FREE_FILES,
    "antennas/default/ccir.000",
    "antennas/default/ccir.001",
    "antennas/default/ccir.002",
    "antennas/default/ccir.003",
    "antennas/default/ccir.004",
    "antennas/default/ccir.005",
    "antennas/default/ccir.006",
    "antennas/default/ccir.007",
    "antennas/default/ccir.008",
    "antennas/default/ccir.009",
    "antennas/default/ccir.010",
    "antennas/default/ccir.011",
    "antennas/default/ccir.012",
    "antennas/default/ccir.013",
    "antennas/default/ccir.014",
    "antennas/default/ccir.015",
    "antennas/default/ccir.016",
    "antennas/default/ccir.017",
    "antennas/default/ccir.018",
    "antennas/default/ccir.019",
    "antennas/default/ccir.020",
    "antennas/default/ccir.021",
    "antennas/default/ccir.022",
    "antennas/default/ccir.023",
    "antennas/default/ccir.024",
    "antennas/default/ccir.025",
    "antennas/default/ccir.026",
    "antennas/default/const17.voa",
    "antennas/default/isotrope",
    "antennas/default/swwhip.voa",
    "database/version.w32",
);

// The ionospheric coefficients: 544 KB of the 560 in the repository.
//
// Two bodies wrote them. About 210 KB is NTIA/ITS work — sporadic E, the E
// region, F1 and the prediction-error tables. The rest comes from CCIR
// Report 322 (atmospheric noise) and CCIR Report 340 (the foF2 and
// M(3000)F2 maps), which the ITU publishes itself in its P.372 and P.533
// reference software. See `NOTICE` and `docs/licence.md`.
//
// They are behind a feature that is off by default, so the published crate
// carries none of them. A build from a source checkout can turn the feature
// on; a build without it reads the files from a real `itshfbc` root.
//
// `fof2URSI.daw` is deliberately absent. The URSI-88 maps are the one part
// with no ITU publication behind them, and no caller here selects them, so
// `COEFFS URSI88` needs a real `itshfbc` root.
#[cfg(feature = "embedded-coefficients")]
embedded_files!(
    COEFF_FILES,
    "coeffs/coeff01w.bin",
    "coeffs/coeff02w.bin",
    "coeffs/coeff03w.bin",
    "coeffs/coeff04w.bin",
    "coeffs/coeff05w.bin",
    "coeffs/coeff06w.bin",
    "coeffs/coeff07w.bin",
    "coeffs/coeff08w.bin",
    "coeffs/coeff09w.bin",
    "coeffs/coeff10w.bin",
    "coeffs/coeff11w.bin",
    "coeffs/coeff12w.bin",
    "coeffs/fof2CCIR.daw",
);

/// Empty without the feature, so every lookup falls through to the message in
/// [`compiled`] rather than to a wrong answer.
#[cfg(not(feature = "embedded-coefficients"))]
static COEFF_FILES: &[(&str, &[u8])] = &[];

/// Every file compiled into this build, by its path relative to an `itshfbc`
/// root. What is here depends on the `embedded-coefficients` feature.
fn files() -> impl Iterator<Item = &'static (&'static str, &'static [u8])> {
    FREE_FILES.iter().chain(COEFF_FILES.iter())
}

/// The root that reads only compiled-in bytes.
pub fn embedded_root() -> PathBuf {
    PathBuf::from(EMBEDDED)
}

/// A root that reads `dir` first and falls back to the compiled-in bytes.
pub fn overlay_root(dir: &Path) -> PathBuf {
    PathBuf::from(format!("{EMBEDDED}{OVERLAY}{}", dir.display()))
}

/// Which of the three forms a root is.
enum Source<'a> {
    Tree(&'a Path),
    Embedded,
    Overlay(&'a Path),
}

fn source(root: &Path) -> Source<'_> {
    let Some(text) = root.to_str() else {
        return Source::Tree(root);
    };
    match text.strip_prefix(EMBEDDED) {
        None => Source::Tree(root),
        Some("") => Source::Embedded,
        Some(rest) => match rest.strip_prefix(OVERLAY) {
            Some(dir) if !dir.is_empty() => Source::Overlay(Path::new(dir)),
            // "<embedded>something" is a typo, not a directory called that.
            _ => Source::Embedded,
        },
    }
}

fn compiled(rel: &str) -> Result<Cow<'static, [u8]>> {
    if let Some((_, bytes)) = files().find(|(name, _)| *name == rel) {
        return Ok(Cow::Borrowed(bytes));
    }
    // The URSI-88 maps are in no build, with the feature or without it, so
    // the message must not send the caller to turn a feature on.
    if rel == "coeffs/fof2URSI.daw" {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "{rel} is not compiled into any build of this crate. The URSI-88 \
                 foF2 maps are not in the repository: unlike the CCIR maps, the \
                 ITU does not publish them itself. Give a real itshfbc root in \
                 place of \"{EMBEDDED}\", or use the default CCIR maps."
            ),
        ));
    }
    // A coefficient file asked for in a build without them is the one
    // failure a caller can fix, so it says how rather than reporting a
    // count. Without this the message reads as a corrupt build.
    if cfg!(not(feature = "embedded-coefficients")) && rel.starts_with("coeffs/") {
        return Err(Error::new(
            ErrorKind::NotFound,
            format!(
                "{rel} is not compiled into this build: the `embedded-coefficients` \
                 feature is off. Part of that data comes from CCIR Reports 322 and \
                 340, so the published crate does not carry it. Either build with \
                 the feature from a source checkout, or give a real itshfbc root in \
                 place of \"{EMBEDDED}\"."
            ),
        ));
    }
    Err(Error::new(
        ErrorKind::NotFound,
        format!(
            "{rel} is not one of the {} embedded files",
            files().count()
        ),
    ))
}

/// Reads a data file named relative to the root, as `coeffs/coeff01w.bin`.
pub fn read(root: &Path, rel: &str) -> Result<Cow<'static, [u8]>> {
    match source(root) {
        Source::Tree(dir) => std::fs::read(dir.join(rel)).map(Cow::Owned),
        Source::Embedded => compiled(rel),
        // Errors from the directory are discarded on purpose: an absent file
        // is the ordinary case here, since the caller writes only the one or
        // two files it generates.
        Source::Overlay(dir) => match std::fs::read(dir.join(rel)) {
            Ok(bytes) => Ok(Cow::Owned(bytes)),
            Err(_) => compiled(rel),
        },
    }
}

/// The same, for a file the engine parses as text.
pub fn read_to_string(root: &Path, rel: &str) -> Result<String> {
    let bytes = read(root, rel)?;
    String::from_utf8(bytes.into_owned())
        .map_err(|e| Error::new(ErrorKind::InvalidData, format!("{rel}: {e}")))
}

/// Where a file would be looked for, for an error message.
pub fn describe(root: &Path, rel: &str) -> String {
    match source(root) {
        Source::Tree(dir) => dir.join(rel).display().to_string(),
        Source::Embedded => format!("{EMBEDDED}/{rel}"),
        Source::Overlay(dir) => format!("{} or {EMBEDDED}, for {rel}", dir.display()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_embedded_file_is_reachable_by_its_name() {
        // The list is written by hand, so a name that does not match its
        // path would compile and then fail at run time on one month only.
        let missing: Vec<&str> = files()
            .filter(|(rel, _)| read(&embedded_root(), rel).is_err())
            .map(|(rel, _)| *rel)
            .collect();
        assert!(missing.is_empty(), "unreachable: {missing:?}");
    }

    #[test]
    #[cfg(feature = "embedded-coefficients")]
    fn the_twelve_months_and_the_ccir_maps_are_present() {
        // A missing month is a prediction that fails in one month of the
        // year, which is the kind of gap a smoke test does not find.
        for month in 1..=12 {
            let rel = format!("coeffs/coeff{month:02}w.bin");
            assert!(read(&embedded_root(), &rel).is_ok(), "{rel}");
        }
        assert!(read(&embedded_root(), "coeffs/fof2CCIR.daw").is_ok());
        assert!(read(&embedded_root(), "database/version.w32").is_ok());
    }

    #[test]
    fn the_ursi_maps_are_in_no_build_and_say_so() {
        // Turning the feature on does not bring these back, so the message
        // must not name the feature: it would send the reader in a circle.
        let error = read(&embedded_root(), "coeffs/fof2URSI.daw")
            .expect_err("URSI-88 is not embedded in any build");
        let text = error.to_string();
        assert!(text.contains("not in the repository"), "{text}");
        assert!(!text.contains("embedded-coefficients"), "{text}");
    }

    #[test]
    fn a_plain_directory_never_falls_back_to_the_compiled_bytes() {
        // A server with a broken tree has to fail, not predict from
        // something the operator did not install.
        let root = Path::new("/nonexistent-itshfbc");
        assert!(read(root, "coeffs/coeff01w.bin").is_err());
    }

    #[test]
    fn an_overlay_prefers_its_directory_and_falls_back() {
        let dir = std::env::temp_dir().join("hfcast-overlay-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("coeffs")).expect("dirs");
        std::fs::write(dir.join("coeffs/coeff01w.bin"), b"overridden").expect("write");
        let root = overlay_root(&dir);
        assert_eq!(&*read(&root, "coeffs/coeff01w.bin").expect("read"), b"overridden");
        // Not written there, so it comes from the binary — when the binary
        // has it. Without the feature the fall-through is the error instead,
        // which is the same code path.
        if cfg!(feature = "embedded-coefficients") {
            assert!(read(&root, "coeffs/coeff02w.bin").expect("read").len() > 1000);
        } else {
            assert!(read(&root, "coeffs/coeff02w.bin").is_err());
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_antenna_files_are_always_compiled_in() {
        // NTIA/ITS wrote these and they carry no redistribution question,
        // so they are present whether the coefficient feature is on or not.
        assert!(read(&embedded_root(), "antennas/default/isotrope").is_ok());
        assert!(read(&embedded_root(), "antennas/default/ccir.000").is_ok());
        assert_eq!(FREE_FILES.len(), 31);
    }

    #[test]
    #[cfg(not(feature = "embedded-coefficients"))]
    fn a_missing_coefficient_says_which_feature_supplies_it() {
        // The one failure a caller can act on. Without this it reads as a
        // corrupt build rather than as a build made a particular way.
        let err = read(&embedded_root(), "coeffs/coeff01w.bin").expect_err("no coeffs");
        let text = err.to_string();
        assert!(text.contains("embedded-coefficients"), "{text}");
        assert!(text.contains("CCIR"), "{text}");
        assert!(text.contains("itshfbc"), "{text}");
    }

    #[test]
    fn reads_the_version_file_as_text() {
        let text = read_to_string(&embedded_root(), "database/version.w32").expect("text");
        assert!(text.starts_with("Version"), "{text:?}");
    }
}
