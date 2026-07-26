//! LUF check: the Rust LUF search against the reference's method-26
//! table.
//!
//! Card method 26 is `ITRUN = 8`: the engine sweeps a frequency
//! complement per hour for the lowest frequency meeting the required
//! reliability, and `OUTMUF` prints GMT, LMT, FOT, HPF, the sporadic-E
//! and circuit MUFs, and the LUF (negative when nothing qualified).
//! Each case runs the reference with a method-26 deck and parses those
//! rows, then runs [`run_luf`] and compares every column at the
//! table's two printed decimals.
//!
//! Usage: `cargo run --release --bin lufcheck [--cases N] [--from N]
//! [--jobs J]`

use std::process::ExitCode;

use propcore::deck::build_deck;
use propcore::engine::run::{run_luf, RunInputs};
use propcore::fuzz::fuzz_cases;
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};

/// One parsed table row: GMT, LMT, FOT, HPF, ES MUF, MUF, LUF.
type Row = [f64; 7];

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let number = |name: &str, default: u64| -> u64 {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .and_then(|v| v.parse().ok())
            .unwrap_or(default)
    };
    let count = number("--cases", 48);
    let from = number("--from", 0);
    let jobs = number("--jobs", 4).max(1) as usize;

    let reference = variant_bin("O2");
    if !reference.is_file() {
        eprintln!("no O2 variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }

    // The LUF methods keep their own frequency sweep, so the deck's
    // antennas and system cards still matter but its frequencies do
    // not. Reuse the fuzz corpus for geometry, season and antennas.
    let mut cases = fuzz_cases(from, count);
    for case in cases.iter_mut() {
        case.method = 26;
    }
    println!("# LUF check: {} method-26 cases, indices {from}..\n", cases.len());

    struct Outcome {
        index: u64,
        rows: usize,
        diffs: Vec<String>,
        broken: Option<String>,
    }

    let outcomes = map_limit(&cases, jobs, |case, offset| {
        let index = from + offset as u64;
        let mut out = Outcome {
            index,
            rows: 0,
            diffs: Vec::new(),
            broken: None,
        };
        let deck = match build_deck(case) {
            Ok(d) => d,
            Err(e) => {
                out.broken = Some(format!("deck: {e}"));
                return out;
            }
        };
        let root = match IsolatedRoot::create(&format!("luf-{}", case.id)) {
            Ok(r) => r,
            Err(e) => {
                out.broken = Some(format!("tree: {e}"));
                return out;
            }
        };
        let text = match run_deck(&reference, root.path(), &deck) {
            Ok(t) => t,
            Err(e) => {
                out.broken = Some(format!("reference: {e}"));
                return out;
            }
        };
        let expected = parse_outmuf(&text);
        if expected.len() != 24 {
            out.broken = Some(format!("parsed {} rows, wanted 24", expected.len()));
            return out;
        }
        let inputs = RunInputs::from(case);
        let hours = match run_luf(root.path(), &inputs) {
            Ok(h) => h,
            Err(e) => {
                out.broken = Some(format!("port: {e}"));
                return out;
            }
        };
        for (row, hour) in expected.iter().zip(&hours) {
            out.rows += 1;
            let ported: Row = [
                f64::from(hour.gmt),
                f64::from(hour.lmt),
                f64::from(hour.fot),
                f64::from(hour.hpf),
                f64::from(hour.esmuf),
                f64::from(hour.allmuf),
                f64::from(hour.xluf),
            ];
            const NAMES: [&str; 7] = ["GMT", "LMT", "FOT", "HPF", "ES MUF", "MUF", "LUF"];
            for c in 0..7 {
                // GMT and LMT print F6.1, the rest F7.2.
                let decimals = if c < 2 { 10.0 } else { 100.0 };
                let want = (row[c] * decimals).round_ties_even();
                let got = (ported[c] * decimals).round_ties_even();
                if want != got {
                    out.diffs.push(format!(
                        "hour {} {}: reference {}, port {}",
                        row[0], NAMES[c], row[c], ported[c]
                    ));
                }
            }
        }
        out
    });

    let mut rows = 0usize;
    let mut failed = false;
    for o in &outcomes {
        rows += o.rows;
        if let Some(why) = &o.broken {
            failed = true;
            println!("case {}: {}", o.index, why);
        }
        if !o.diffs.is_empty() {
            failed = true;
            println!("case {} (reproduce with fuzz --seed {}):", o.index, o.index);
            for d in o.diffs.iter().take(8) {
                println!("  {d}");
            }
            if o.diffs.len() > 8 {
                println!("  ...and {} more", o.diffs.len() - 8);
            }
        }
    }
    println!("\n{} hour-rows compared over {} cases.", rows, outcomes.len());
    if failed {
        println!("Verdict: the LUF search disagrees with the reference.");
        ExitCode::FAILURE
    } else {
        println!("Verdict: every table cell matches the reference.");
        ExitCode::SUCCESS
    }
}

/// Pulls the 24 data rows out of a method-26 listing.
///
/// `OUTMUF` builds its format as `(1H ,2X,2F6.1,` then one `F7.2` per
/// tabulated column, so the fields are read by column rather than by
/// splitting: a MUF of 1000.00 fills its field completely and leaves
/// no space before the next one.
fn parse_outmuf(text: &str) -> Vec<Row> {
    let field = |line: &str, from: usize, to: usize| -> Option<f64> {
        if line.len() < to {
            return None;
        }
        line[from..to].trim().parse().ok()
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let (Some(gmt), Some(lmt)) = (field(line, 3, 9), field(line, 9, 15)) else {
            continue;
        };
        if !(1.0..=24.0).contains(&gmt) {
            continue;
        }
        let mut row: Row = [gmt, lmt, 0.0, 0.0, 0.0, 0.0, 0.0];
        let mut ok = true;
        for c in 0..5 {
            match field(line, 15 + c * 7, 22 + c * 7) {
                Some(v) => row[2 + c] = v,
                None => ok = false,
            }
        }
        if ok {
            out.push(row);
        }
    }
    out
}
