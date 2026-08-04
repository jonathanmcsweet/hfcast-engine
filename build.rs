//! Says why `embedded-coefficients` cannot work from the published
//! package, instead of letting the build stop at a bare "file not found".
//!
//! The package excludes `embedded/coeffs/**`: part of that data is CCIR
//! Report 322 and 340 material rather than NTIA/ITS work, so this crate
//! does not redistribute it. See NOTICE and docs/licence.md.
//!
//! With the feature on, `src/voacap/data.rs` reads the maps with
//! `include_bytes!`. From crates.io those files are absent, and the
//! compiler reports only the path it could not open. That says what
//! happened and not why, and it does not say what to do instead.
//!
//! This script has no dependencies, and it does nothing at all unless the
//! feature is on.

use std::path::Path;

// One of the twelve monthly maps. If this one is here they all are: they
// are excluded and restored as a set.
const A_MAP: &str = "embedded/coeffs/coeff01w.bin";

fn main() {
    println!("cargo:rerun-if-changed=embedded/coeffs");

    // Cargo names an enabled feature in the environment of a build
    // script. `cfg!(feature = ...)` here would read the build script's own
    // features, which is not the same thing.
    if std::env::var_os("CARGO_FEATURE_EMBEDDED_COEFFICIENTS").is_none() {
        return;
    }

    if Path::new(A_MAP).exists() {
        return;
    }

    panic!(
        "\n\
        \n\
        The `embedded-coefficients` feature needs the ionospheric\n\
        coefficient maps, and they are not in the published package.\n\
        \n\
        Part of that data is CCIR Report 322 and 340 material rather than\n\
        NTIA/ITS work, so this crate does not redistribute it.\n\
        \n\
        Use the crate without the feature. It then reads the maps from a\n\
        real `itshfbc` tree, which is how the reference engine has always\n\
        found them:\n\
        \n\
            hfcast = \"*\"                      # no features\n\
            HFCAST_ITSHFBC=$HOME/itshfbc      # or pass the root yourself\n\
        \n\
        `makeitshfbc`, from voacapl, writes that tree.\n\
        \n\
        The feature works only in a checkout of the repository, which has\n\
        the files. See NOTICE and docs/licence.md.\n\
        \n"
    );
}
