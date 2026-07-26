//! Randomized whole-engine check: the Rust engine against the
//! reference Fortran over generated decks.
//!
//! [`portcheck`](../portcheck) judges the 96 hand-picked sweep cases.
//! This asks the same question of inputs nobody picked. Each case is a
//! function of its index, so a failure reports an index that reproduces
//! the deck exactly: `--seed 4217` reruns that one case and prints the
//! deck, the differing cells and both engines' values.
//!
//! Both engines are allowed to refuse a case — the Fortran stops, the
//! port panics where the Fortran stops — and refusing the *same* cases
//! counts as agreement. One engine refusing while the other answers is
//! a difference, and is reported as one.
//!
//! Usage: `cargo run --release --bin fuzz [--cases N] [--from N]
//! [--jobs J] [--seed N] [--show N] [--method M] [--coeffs URSI88]
//! [--fprob a,b,c,d] [--botlines a,b,c]`

use std::panic::{catch_unwind, AssertUnwindSafe};
use std::process::ExitCode;

use propcore::deck::{build_deck, DeckCase};
use propcore::engine::run::{body_lines, listing_text, run, RunInputs};
use propcore::fuzz::{band_for, fuzz_cases};
use propcore::listing::{parse_listing, ParsedListing};
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};

/// Concurrent runs. Each one copies the `itshfbc` tree, so this trades
/// disk and memory for wall clock; four is what the other harness
/// binaries use.
const CONCURRENCY: usize = 4;

/// One printed cell that the two engines disagree about. `None` means
/// that engine did not print the cell at all.
#[derive(Debug, Clone)]
struct CellDiff {
    hour: u8,
    row: String,
    slot: i8,
    reference: Option<f64>,
    ported: Option<f64>,
}

#[derive(Debug, Clone)]
struct ModeDiff {
    hour: u8,
    slot: i8,
    reference: Option<String>,
    ported: Option<String>,
}

enum Outcome {
    /// Every printed cell and mode label agreed.
    Matched { cells: usize, modes: usize },
    Differed {
        cells: Vec<CellDiff>,
        modes: Vec<ModeDiff>,
        compared: usize,
    },
    /// Both engines declined the case.
    BothRefused,
    /// One answered and the other did not, which is itself a difference.
    OneRefused { which: &'static str, why: String },
    /// The harness could not even build the deck.
    Broken(String),
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    };
    let number = |name: &str, default: u64| -> u64 {
        flag(name)
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(default)
    };

    let single = flag("--seed").and_then(|v| v.parse::<u64>().ok());
    let from = number("--from", 0);
    let count = if single.is_some() {
        1
    } else {
        number("--cases", 200)
    };
    let jobs = number("--jobs", CONCURRENCY as u64).max(1) as usize;
    let show = number("--show", 12) as usize;

    let reference = variant_bin("O2");
    if !reference.is_file() {
        eprintln!("no O2 variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }

    // A panic in the port is a result here, not a crash: the Fortran
    // stops on some inputs and the port stops on the same ones. The
    // default hook would print a traceback per case and bury the
    // report, so it is silenced — except for a single-case run, where
    // the traceback is the thing being looked for.
    if single.is_none() {
        std::panic::set_hook(Box::new(|_| {}));
    }

    let mut cases = match single {
        Some(index) => fuzz_cases(index, 1),
        None => fuzz_cases(from, count),
    };
    // `--method M` asks the same question of another systems method:
    // the same decks with a different `METHOD` card, so the model that
    // runs and the lines that print change but the corpus does not.
    let method = number("--method", 30) as u32;
    // `--coeffs URSI88` asks the same question of the other foF2 map
    // set; the default is the CCIR set the corpus was built on.
    let ursi = flag("--coeffs").map(|v| v.starts_with("URSI")).unwrap_or(false);
    // `--fprob a,b,c,d` sets the whole card, for the critical-frequency
    // multipliers the sporadic-E switch cannot express.
    let fprob: Option<[f64; 4]> = flag("--fprob").and_then(|v| {
        let parts: Vec<f64> = v.split(',').filter_map(|t| t.trim().parse().ok()).collect();
        (parts.len() == 4).then(|| [parts[0], parts[1], parts[2], parts[3]])
    });
    // `--botlines a,b,c` writes a BOTLINES card, which selects the
    // body lines and their order for any method.
    let botlines: Option<Vec<u32>> = flag("--botlines").map(|v| {
        v.split(',').filter_map(|t| t.trim().parse().ok()).collect()
    });
    for case in cases.iter_mut() {
        case.method = method;
        case.ursi = ursi;
        case.fprob = fprob;
        case.botlines = botlines.clone();
    }
    let cases = cases;

    if let Some(index) = single {
        return single_case(&reference, &cases[0], index, show);
    }

    println!(
        "# Randomized whole-engine check: {} cases, method {}, {} coefficients, indices {}..{}\n",
        cases.len(),
        method,
        if ursi { "URSI88" } else { "CCIR" },
        from,
        from + count - 1
    );

    let outcomes = map_limit(&cases, jobs, |case, _| check_case(&reference, case));

    let mut matched = 0usize;
    let mut cells = 0usize;
    let mut modes = 0usize;
    let mut both_refused = 0usize;
    let mut failures: Vec<(u64, &Outcome)> = Vec::new();
    for (offset, outcome) in outcomes.iter().enumerate() {
        let index = from + offset as u64;
        match outcome {
            Outcome::Matched { cells: c, modes: m } => {
                matched += 1;
                cells += c;
                modes += m;
            }
            Outcome::BothRefused => both_refused += 1,
            _ => failures.push((index, outcome)),
        }
    }

    println!("| outcome | cases |");
    println!("| --- | --: |");
    println!("| identical | {matched} |");
    println!("| both engines refused | {both_refused} |");
    println!("| differing | {} |", failures.len());
    println!("\n{cells} printed cells and {modes} mode labels compared.\n");

    if failures.is_empty() {
        println!("Verdict: identical on every case the reference answered.");
        return ExitCode::SUCCESS;
    }

    println!("## Differences\n");
    for (index, outcome) in failures.iter().take(show) {
        let (_, _, band) = band_for(*index);
        println!("### case {index} ({band})\n");
        println!("Reproduce with `--seed {index}`.\n");
        match outcome {
            Outcome::Differed {
                cells,
                modes,
                compared,
            } => {
                println!(
                    "{} of {compared} cells differ, {} mode labels.\n",
                    cells.len(),
                    modes.len()
                );
                print_cells(cells, show);
                print_modes(modes, show);
            }
            Outcome::OneRefused { which, why } => println!("only the {which} refused it: {why}\n"),
            Outcome::Broken(why) => println!("the harness could not run it: {why}\n"),
            Outcome::Matched { .. } | Outcome::BothRefused => unreachable!("not a failure"),
        }
    }
    if failures.len() > show {
        println!("...and {} more cases.", failures.len() - show);
    }
    println!("\nVerdict: the port and the reference disagree.");
    ExitCode::FAILURE
}

/// Runs one case verbosely, for chasing down a reported index.
fn single_case(reference: &std::path::Path, case: &DeckCase, index: u64, show: usize) -> ExitCode {
    let (_, _, band) = band_for(index);
    println!("# case {index} ({band})\n");
    println!(
        "{:.2},{:.2} to {:.2},{:.2}  month {}  ssn {}  {} W  snr {} dB  noise {} dBW  es {}",
        case.from_lat,
        case.from_lon,
        case.to_lat,
        case.to_lon,
        case.month,
        case.ssn,
        case.watts,
        case.required_snr_db,
        case.noise_dbw,
        {
            let p = case.fprob();
            format!("fprob {:.2} {:.2} {:.2} {:.2}", p[0], p[1], p[2], p[3])
        }
    );
    println!("frequencies: {:?}\n", case.freqs_mhz);
    match build_deck(case) {
        Ok(deck) => println!("```\n{deck}```\n"),
        Err(e) => {
            println!("the deck does not build: {e}");
            return ExitCode::FAILURE;
        }
    }

    match check_case(reference, case) {
        Outcome::Matched { cells, modes } => {
            println!("Identical: {cells} cells and {modes} mode labels agree.");
            ExitCode::SUCCESS
        }
        Outcome::BothRefused => {
            println!("Both engines refused this case.");
            ExitCode::SUCCESS
        }
        Outcome::Differed {
            cells,
            modes,
            compared,
        } => {
            println!(
                "{} of {compared} cells differ, {} mode labels.\n",
                cells.len(),
                modes.len()
            );
            print_cells(&cells, show.max(40));
            print_modes(&modes, show.max(40));
            ExitCode::FAILURE
        }
        Outcome::OneRefused { which, why } => {
            println!("Only the {which} refused it: {why}");
            ExitCode::FAILURE
        }
        Outcome::Broken(why) => {
            println!("The harness could not run it: {why}");
            ExitCode::FAILURE
        }
    }
}

fn print_cells(diffs: &[CellDiff], show: usize) {
    if diffs.is_empty() {
        return;
    }
    println!("| hour | row | slot | reference | port |");
    println!("| --: | --- | --: | --: | --: |");
    for d in diffs.iter().take(show) {
        println!(
            "| {} | {} | {} | {} | {} |",
            d.hour,
            d.row.trim(),
            d.slot,
            d.reference
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into()),
            d.ported
                .map(|v| v.to_string())
                .unwrap_or_else(|| "-".into())
        );
    }
    if diffs.len() > show {
        println!("\n...and {} more cells.", diffs.len() - show);
    }
    println!();
}

fn print_modes(diffs: &[ModeDiff], show: usize) {
    if diffs.is_empty() {
        return;
    }
    println!("| hour | slot | reference mode | port mode |");
    println!("| --: | --: | --- | --- |");
    for d in diffs.iter().take(show) {
        println!(
            "| {} | {} | {} | {} |",
            d.hour,
            d.slot,
            d.reference.clone().unwrap_or_else(|| "-".into()),
            d.ported.clone().unwrap_or_else(|| "-".into())
        );
    }
    if diffs.len() > show {
        println!("\n...and {} more labels.", diffs.len() - show);
    }
    println!();
}

fn check_case(reference: &std::path::Path, case: &DeckCase) -> Outcome {
    let deck = match build_deck(case) {
        Ok(d) => d,
        Err(e) => return Outcome::Broken(format!("deck: {e}")),
    };
    let root = match IsolatedRoot::create(&format!("fz-{}", case.id)) {
        Ok(r) => r,
        Err(e) => return Outcome::Broken(format!("private tree: {e}")),
    };

    let fortran = run_deck(reference, root.path(), &deck);
    let inputs = RunInputs::from(case);
    // The port panics where the engine stops, so a panic is caught and
    // compared against the Fortran's refusal rather than ending the run.
    let ported = match catch_unwind(AssertUnwindSafe(|| run(root.path(), &inputs))) {
        Ok(Ok(hours)) => Ok(hours),
        Ok(Err(e)) => Err(e),
        Err(payload) => Err(panic_message(payload)),
    };

    match (fortran, ported) {
        (Err(fe), Err(_)) => {
            let _ = fe;
            Outcome::BothRefused
        }
        (Err(fe), Ok(_)) => Outcome::OneRefused {
            which: "reference",
            why: fe.to_string(),
        },
        (Ok(_), Err(pe)) => Outcome::OneRefused {
            which: "port",
            why: pe,
        },
        (Ok(text), Ok(hours)) => {
            let a = parse_listing(&text);
            let b = parse_listing(&listing_text(&hours, &body_lines(case.method, case.botlines.as_deref())));
            let (cells, compared) = cell_diffs(&a, &b);
            let modes = mode_diffs(&a, &b);
            if cells.is_empty() && modes.is_empty() {
                Outcome::Matched {
                    cells: compared,
                    modes: a.modes.len(),
                }
            } else {
                Outcome::Differed {
                    cells,
                    modes,
                    compared,
                }
            }
        }
    }
}

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "panicked".to_string()
    }
}

/// Every differing cell, and how many were compared.
///
/// [`propcore::compare`] keeps per-row aggregates, which is what a
/// tolerance needs; chasing one failing case needs the cell's hour,
/// row and slot, so the two listings are walked directly here.
fn cell_diffs(a: &ParsedListing, b: &ParsedListing) -> (Vec<CellDiff>, usize) {
    use std::collections::HashMap;
    let ported: HashMap<(u8, &str, i8), f64> =
        b.numeric.iter().map(|s| (s.key(), s.value)).collect();
    let seen: std::collections::HashSet<(u8, &str, i8)> =
        a.numeric.iter().map(|s| s.key()).collect();

    let mut diffs = Vec::new();
    let mut compared = 0usize;
    for sa in &a.numeric {
        match ported.get(&sa.key()) {
            Some(&value) => {
                compared += 1;
                if value != sa.value {
                    diffs.push(CellDiff {
                        hour: sa.hour,
                        row: sa.row.clone(),
                        slot: sa.slot,
                        reference: Some(sa.value),
                        ported: Some(value),
                    });
                }
            }
            None => diffs.push(CellDiff {
                hour: sa.hour,
                row: sa.row.clone(),
                slot: sa.slot,
                reference: Some(sa.value),
                ported: None,
            }),
        }
    }
    for sb in &b.numeric {
        if !seen.contains(&sb.key()) {
            diffs.push(CellDiff {
                hour: sb.hour,
                row: sb.row.clone(),
                slot: sb.slot,
                reference: None,
                ported: Some(sb.value),
            });
        }
    }
    diffs.sort_by(|x, y| (x.hour, &x.row, x.slot).cmp(&(y.hour, &y.row, y.slot)));
    (diffs, compared)
}

fn mode_diffs(a: &ParsedListing, b: &ParsedListing) -> Vec<ModeDiff> {
    use std::collections::HashMap;
    let ported: HashMap<(u8, i8), &str> =
        b.modes.iter().map(|m| (m.key(), m.mode.as_str())).collect();
    let seen: std::collections::HashSet<(u8, i8)> = a.modes.iter().map(|m| m.key()).collect();

    let mut diffs = Vec::new();
    for ma in &a.modes {
        match ported.get(&ma.key()) {
            Some(&other) if other == ma.mode => {}
            Some(&other) => diffs.push(ModeDiff {
                hour: ma.hour,
                slot: ma.slot,
                reference: Some(ma.mode.clone()),
                ported: Some(other.to_string()),
            }),
            None => diffs.push(ModeDiff {
                hour: ma.hour,
                slot: ma.slot,
                reference: Some(ma.mode.clone()),
                ported: None,
            }),
        }
    }
    for mb in &b.modes {
        if !seen.contains(&mb.key()) {
            diffs.push(ModeDiff {
                hour: mb.hour,
                slot: mb.slot,
                reference: None,
                ported: Some(mb.mode.clone()),
            });
        }
    }
    diffs.sort_by_key(|d| (d.hour, d.slot));
    diffs
}
