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
    ($($rel:literal),* $(,)?) => {
        /// Every file compiled in, by its path relative to an `itshfbc` root.
        static FILES: &[(&str, &[u8])] = &[
            $(($rel, include_bytes!(concat!("../../embedded/", $rel))),)*
        ];
    };
}

embedded_files!(
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
    "coeffs/fof2URSI.daw",
    "database/version.w32",
);

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
    FILES
        .iter()
        .find(|(name, _)| *name == rel)
        .map(|(_, bytes)| Cow::Borrowed(*bytes))
        .ok_or_else(|| {
            Error::new(
                ErrorKind::NotFound,
                format!("{rel} is not one of the {} embedded files", FILES.len()),
            )
        })
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
        let missing: Vec<&str> = FILES
            .iter()
            .filter(|(rel, _)| read(&embedded_root(), rel).is_err())
            .map(|(rel, _)| *rel)
            .collect();
        assert!(missing.is_empty(), "unreachable: {missing:?}");
    }

    #[test]
    fn the_twelve_months_and_both_models_are_present() {
        // A missing month is a prediction that fails in one month of the
        // year, which is the kind of gap a smoke test does not find.
        for month in 1..=12 {
            let rel = format!("coeffs/coeff{month:02}w.bin");
            assert!(read(&embedded_root(), &rel).is_ok(), "{rel}");
        }
        assert!(read(&embedded_root(), "coeffs/fof2CCIR.daw").is_ok());
        assert!(read(&embedded_root(), "coeffs/fof2URSI.daw").is_ok());
        assert!(read(&embedded_root(), "database/version.w32").is_ok());
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
        // Not written there, so it comes from the binary.
        assert!(read(&root, "coeffs/coeff02w.bin").expect("read").len() > 1000);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reads_the_version_file_as_text() {
        let text = read_to_string(&embedded_root(), "database/version.w32").expect("text");
        assert!(text.starts_with("Version"), "{text:?}");
    }
}
