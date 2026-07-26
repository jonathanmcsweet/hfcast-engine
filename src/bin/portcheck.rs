//! The whole-engine check: the Rust engine against the reference
//! Fortran, judged by the tolerance envelope of `docs/sensitivity.md`.
//!
//! Per sweep case the reference `-O2` binary runs the deck and its
//! listing is parsed; the Rust engine (`engine::run`) computes the same
//! prediction and renders the same listing body. The two parsed
//! listings are compared cell by cell. The port passes when every field
//! stays within the envelope — no further from the reference than
//! IEEE-conformant rebuilds of the reference are from each other — with
//! zero structural disagreements (cells or modes present in only one).
//!
//! Usage: `cargo run --release --bin portcheck [--cases N]`

use std::process::ExitCode;

use propcore::compare::{compare_listings, Comparison};
use propcore::deck::build_deck;
use propcore::engine::run::{listing_text, run, RunInputs};
use propcore::listing::parse_listing;
use propcore::runner::{run_deck, variant_bin, IsolatedRoot};
use propcore::sweep::sweep_cases;

/// `docs/sensitivity.md`, "Derived tolerance": the widest disagreement
/// between IEEE-conformant builds of the reference.
const ENVELOPE: &[(&str, f64)] = &[
    ("DBU", 0.0),
    ("DELAY", 0.1),
    ("LOSS", 0.0),
    ("MPROB", 0.0),
    ("MUF", 0.0),
    ("MUFday", 0.0),
    ("N DBW", 0.0),
    ("REL", 0.0),
    ("RGAIN", 0.0),
    ("RPWRG", 0.0),
    ("S DBW", 0.0),
    ("S PRB", 0.0),
    ("SIG LW", 0.0),
    ("SIG UP", 0.1),
    ("SNR", 1.0),
    ("SNR LW", 0.0),
    ("SNR UP", 0.1),
    ("SNRxx", 0.0),
    ("TANGLE", 0.1),
    ("TGAIN", 0.0),
    ("V HITE", 1.0),
];

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let case_limit = argv
        .iter()
        .position(|a| a == "--cases")
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(usize::MAX);
    let reference = variant_bin("O2");
    if !reference.is_file() {
        eprintln!("no O2 variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }
    let cases: Vec<_> = sweep_cases().into_iter().take(case_limit).collect();
    println!(
        "# Whole-engine check: Rust engine vs the -O2 reference, {} sweep cases\n",
        cases.len()
    );

    let mut merged = Comparison::default();
    let mut compared = 0usize;
    for case in &cases {
        let deck = match build_deck(case) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{}: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let root = match IsolatedRoot::create(&format!("pc-{}", case.id)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let fortran = match run_deck(&reference, root.path(), &deck) {
            Ok(text) => parse_listing(&text),
            Err(e) => {
                eprintln!("{}: reference failed: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let inputs = RunInputs::from(case);
        let hours = match run(root.path(), &inputs) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("{}: Rust engine failed: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let ported = parse_listing(&listing_text(&hours));
        merged.merge(compare_listings(&fortran, &ported));
        compared += 1;
    }

    println!("Compared {compared} cases.\n");
    println!("| field | samples | differing | max abs | only in one | envelope | verdict |");
    println!("| --- | --: | --: | --: | --: | --: | --- |");
    let stats = merged.stats();
    let mut failed = false;
    for s in &stats {
        let limit = ENVELOPE
            .iter()
            .find(|(name, _)| *name == s.row)
            .map(|(_, l)| *l);
        let (limit_text, verdict) = match limit {
            Some(l) => {
                let ok = s.max_abs <= l && s.only_in_one == 0;
                if !ok {
                    failed = true;
                }
                (format!("{l}"), if ok { "inside" } else { "OUTSIDE" })
            }
            None => {
                failed = true;
                ("?".to_string(), "unknown row")
            }
        };
        println!(
            "| {} | {} | {} | {} | {} | {} | {} |",
            s.row, s.samples, s.differing, s.max_abs, s.only_in_one, limit_text, verdict
        );
    }
    let m = merged.modes;
    let modes_ok = m.mismatched == 0 && m.only_in_a == 0 && m.only_in_b == 0;
    if !modes_ok {
        failed = true;
    }
    println!(
        "\nMODE: {} compared, {} mismatched, {} present in only one — {}.",
        m.compared,
        m.mismatched,
        m.only_in_a + m.only_in_b,
        if modes_ok { "inside" } else { "OUTSIDE" }
    );
    if failed {
        println!("\nVerdict: OUTSIDE the envelope.");
        ExitCode::FAILURE
    } else {
        println!("\nVerdict: inside the envelope — the port is equivalent to the reference.");
        ExitCode::SUCCESS
    }
}
