//! Does this engine give the same answer on another architecture?
//!
//! Every other harness compares the port against the reference Fortran, and
//! all of them have only ever run on x86-64. The reference cannot easily be
//! built for a phone, so the claim is chained instead: `portcheck` establishes
//! that the port matches the reference on x86-64, and this establishes that
//! the port matches *itself* elsewhere. Both green means the port matches the
//! reference on the other architecture too.
//!
//! What is actually at risk is the maths library. IEEE-754 fixes add, multiply
//! and divide, and Rust does not contract expressions into fused multiply-add,
//! so plain arithmetic is safe. `sin`, `cos`, `exp`, `pow` and `log` are not
//! guaranteed identical between platforms or libm versions, and this engine
//! calls them throughout the geometry and absorption paths. A last-place
//! difference there can move a rounded listing field.
//!
//! No Fortran, no isolated root per case, no temporary trees: it renders the
//! same listing text `portcheck` compares and prints a digest per case. That
//! keeps it runnable under an emulator, where process spawning and disk copies
//! are what make the other harnesses impractical.
//!
//! Usage: `cargo run --release --bin archcheck [--cases N] [--full]`
//!
//! `--full` prints every listing instead of digests, for diffing a case that
//! disagrees.

use std::process::ExitCode;

use hfcast::sweep::sweep_cases;
use hfcast::voacap::run::{body_lines, listing_text, run, RunInputs};

/// A digest small enough to read in a table and wide enough not to collide
/// across a hundred listings. FNV-1a over the bytes: the point is detecting
/// difference, not resisting an adversary, and a hand-written hash keeps this
/// harness free of dependencies that might themselves differ per platform.
fn digest(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let full = args.iter().any(|a| a == "--full");
    let limit = args
        .iter()
        .position(|a| a == "--cases")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse::<usize>().ok());

    let all = sweep_cases();
    let cases = match limit {
        Some(n) => &all[..n.min(all.len())],
        None => &all[..],
    };

    // The data files are read from the tree the other harnesses use. Under an
    // emulator this is the host's own path, so nothing is copied.
    let root = std::path::PathBuf::from(
        std::env::var("HFCAST_ITSHFBC").unwrap_or_else(|_| "itshfbc".to_string()),
    );
    if !root.is_dir() {
        eprintln!("no itshfbc tree at {}", root.display());
        eprintln!("set HFCAST_ITSHFBC to one");
        return ExitCode::FAILURE;
    }

    println!(
        "# archcheck: {} sweep cases on {} {}",
        cases.len(),
        std::env::consts::ARCH,
        std::env::consts::OS,
    );

    // Sequential on purpose: this runs under an emulator, where a thread pool
    // buys nothing and memory is the scarce thing.
    for case in cases {
        let inputs = RunInputs::from(case);
        let hours = match run(&root, &inputs) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("{}: engine failed: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let text = listing_text(
            &hours,
            &body_lines(case.method, case.botlines.as_deref()),
        );
        if full {
            println!("=== {}\n{text}", case.id);
        } else {
            println!("{:016x}  {}", digest(&text), case.id);
        }
    }

    ExitCode::SUCCESS
}
