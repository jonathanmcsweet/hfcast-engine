//! What does a fix change?
//!
//! Runs the sweep corpus twice — `Model::Compatible` and the same
//! cases with one fix on — and reports exactly which printed cells
//! move and by how much. That change set is part of a fix's
//! documentation: "it is fixed" says nothing without "and here is
//! what it did".
//!
//! This is not an accuracy measurement. It says what moved, not
//! whether the movement is an improvement; only the WSPR pipeline can
//! say that, and only for defects that touch the point-to-point
//! systems path. `docs/corrected.md` records both.
//!
//! Usage: `cargo run --release --bin correctcheck -- [--fix NAME]
//! [--cases N] [--jobs J]`
//!
//! With no `--fix`, every fix is listed with the cases it touches.

use std::collections::BTreeMap;
use std::process::ExitCode;

use propcore::deck::build_deck;
use propcore::engine::model::{Fixes, Model};
use propcore::engine::output::render;
use propcore::listing::{parse_listing, Sample};
use propcore::runner::{map_limit, IsolatedRoot};
use propcore::sweep::sweep_cases;

const CONCURRENCY: usize = 2;

/// Every fix, by the name the command line uses.
fn fix_by_name(name: &str) -> Option<Fixes> {
    let mut f = Fixes::default();
    match name {
        "pole_file" => f.pole_file = true,
        "curtain_elevation" => f.curtain_elevation = true,
        "luf_scan_best" => f.luf_scan_best = true,
        "luf_pass_area" => f.luf_pass_area = true,
        "area_centre_nudge" => f.area_centre_nudge = true,
        "area_antenna_end" => f.area_antenna_end = true,
        _ => return None,
    }
    Some(f)
}

const FIX_NAMES: [&str; 6] = [
    "pole_file",
    "curtain_elevation",
    "luf_scan_best",
    "luf_pass_area",
    "area_centre_nudge",
    "area_antenna_end",
];

/// One row's worth of movement.
struct Moved {
    row: String,
    cells: usize,
    worst: f64,
}

struct Outcome {
    id: String,
    compared: usize,
    moved: usize,
    /// Structural: a cell one side printed and the other did not.
    structural: usize,
    by_row: BTreeMap<String, (usize, f64)>,
    failure: Option<String>,
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    };
    let jobs = flag("--jobs")
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(CONCURRENCY)
        .max(1);
    let limit = flag("--cases").and_then(|v| v.parse::<usize>().ok());

    let Some(name) = flag("--fix") else {
        println!("Fixes, by the name --fix takes:");
        for n in FIX_NAMES {
            println!("  {n}");
        }
        println!();
        println!("Model::Corrected turns on all of them at once. This binary");
        println!("runs one at a time, so a change can be attributed.");
        return ExitCode::SUCCESS;
    };
    let Some(fixes) = fix_by_name(&name) else {
        eprintln!("unknown fix {name:?}; run with no --fix to list them");
        return ExitCode::FAILURE;
    };

    let mut cases = sweep_cases();
    if let Some(n) = limit {
        cases.truncate(n);
    }
    eprintln!("running {} sweep cases with only {name} on", cases.len());

    let outcomes = map_limit(&cases, jobs, |case, index| {
        let mut out = Outcome {
            id: case.id.clone(),
            compared: 0,
            moved: 0,
            structural: 0,
            by_row: BTreeMap::new(),
            failure: None,
        };
        let root = match IsolatedRoot::create(&format!("correct{index}")) {
            Ok(r) => r,
            Err(e) => {
                out.failure = Some(format!("tree: {e}"));
                return out;
            }
        };
        let deck = match build_deck(case) {
            Ok(d) => d,
            Err(e) => {
                out.failure = Some(format!("deck: {e}"));
                return out;
            }
        };

        let base = match render(root.path(), case, &deck, Model::Compatible) {
            Ok(t) => t,
            Err(e) => {
                out.failure = Some(format!("compatible: {e}"));
                return out;
            }
        };
        let fixed = match render_with(root.path(), case, &deck, fixes) {
            Ok(t) => t,
            Err(e) => {
                out.failure = Some(format!("fixed: {e}"));
                return out;
            }
        };

        compare(&base, &fixed, &mut out);
        out
    });

    report(&name, &outcomes)
}

/// Renders with an arbitrary fix set.
///
/// `Model` exposes only all-off and all-on, on purpose. Isolating one
/// fix is a measurement, so it lives here in the harness rather than
/// in the public API.
fn render_with(
    root: &std::path::Path,
    case: &propcore::deck::DeckCase,
    deck: &str,
    fixes: Fixes,
) -> Result<String, String> {
    render(root, case, deck, Model::from_fixes(fixes))
}

fn compare(base: &str, fixed: &str, out: &mut Outcome) {
    let a = parse_listing(base);
    let b = parse_listing(fixed);

    let key = |s: &Sample| (s.hour, s.row.clone(), s.slot);
    let left: BTreeMap<_, f64> = a.numeric.iter().map(|s| (key(s), s.value)).collect();
    let right: BTreeMap<_, f64> = b.numeric.iter().map(|s| (key(s), s.value)).collect();

    let mut keys: Vec<_> = left.keys().cloned().collect();
    keys.extend(right.keys().cloned());
    keys.sort();
    keys.dedup();

    for k in keys {
        out.compared += 1;
        let row = k.1.clone();
        match (left.get(&k), right.get(&k)) {
            (Some(x), Some(y)) if x == y => {}
            (Some(x), Some(y)) => {
                out.moved += 1;
                let delta = (y - x).abs();
                let entry = out.by_row.entry(row).or_insert((0, 0.0));
                entry.0 += 1;
                entry.1 = entry.1.max(delta);
            }
            (None, None) => {}
            _ => {
                out.structural += 1;
                let entry = out.by_row.entry(row).or_insert((0, 0.0));
                entry.0 += 1;
            }
        }
    }

    // A propagation mode is discrete: it matches or it does not.
    let modes_a: BTreeMap<_, _> = a.modes.iter().map(|m| (m.key(), &m.mode)).collect();
    let modes_b: BTreeMap<_, _> = b.modes.iter().map(|m| (m.key(), &m.mode)).collect();
    for (k, v) in &modes_a {
        out.compared += 1;
        if modes_b.get(k) != Some(v) {
            out.moved += 1;
            let entry = out.by_row.entry("MODE".to_string()).or_insert((0, 0.0));
            entry.0 += 1;
        }
    }
}

fn report(name: &str, outcomes: &[Outcome]) -> ExitCode {
    let failures: Vec<&Outcome> = outcomes.iter().filter(|o| o.failure.is_some()).collect();
    for o in &failures {
        eprintln!("{}: {}", o.id, o.failure.as_deref().unwrap_or(""));
    }

    let compared: usize = outcomes.iter().map(|o| o.compared).sum();
    let moved: usize = outcomes.iter().map(|o| o.moved).sum();
    let structural: usize = outcomes.iter().map(|o| o.structural).sum();
    let touched = outcomes.iter().filter(|o| o.moved + o.structural > 0).count();

    let mut rows: BTreeMap<String, Moved> = BTreeMap::new();
    for o in outcomes {
        for (row, (cells, worst)) in &o.by_row {
            let e = rows.entry(row.clone()).or_insert(Moved {
                row: row.clone(),
                cells: 0,
                worst: 0.0,
            });
            e.cells += cells;
            e.worst = e.worst.max(*worst);
        }
    }

    println!("# What `{name}` changes");
    println!();
    println!(
        "{} of {} sweep cases touched; {moved} of {compared} cells moved, {structural} structural.",
        touched,
        outcomes.len()
    );
    println!();
    if rows.is_empty() {
        println!("No printed cell changes on this corpus.");
    } else {
        println!("| row | cells moved | worst change |");
        println!("| --- | --: | --: |");
        let mut sorted: Vec<&Moved> = rows.values().collect();
        sorted.sort_by(|a, b| b.cells.cmp(&a.cells).then(a.row.cmp(&b.row)));
        for m in sorted {
            println!("| {} | {} | {:.2} |", m.row, m.cells, m.worst);
        }
    }

    if !failures.is_empty() {
        println!();
        println!("{} case(s) could not run.", failures.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
