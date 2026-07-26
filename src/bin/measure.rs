//! Measures how far a VOACAP listing moves when only the arithmetic evaluation
//! order changes.
//!
//! Every variant binary is the same Fortran compiled with different
//! optimisation flags. A difference between their listings is therefore not
//! physics — it is the model's sensitivity to how its floating-point arithmetic
//! was evaluated. A port to another language introduces exactly that kind of
//! difference, so this spread is the floor for a per-field port tolerance:
//! tighter than this and the suite would fail runs that are, by the engine's
//! own standard, the same answer.
//!
//! Usage: `cargo run --release --bin measure -- [--out report.json]`

use std::collections::BTreeMap;
use std::fs;
use std::process::ExitCode;
use std::time::Instant;

use propcore::compare::{compare_listings, Comparison, FieldStats};
use propcore::deck::build_deck;
use propcore::listing::{parse_listing, ParsedListing};
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};
use propcore::sweep::{sweep_cases, PATHS};

/// `-O2` matches how the vendored and installed binaries are built.
const REFERENCE: &str = "O2";

/// Variants compared against the reference.
///
/// `-ffast-math` is included on purpose even though it is expected to
/// misbehave: it is the control case showing what happens when arithmetic is
/// reassociated freely, which is precisely what an idiomatic rewrite does.
const COMPARED: &[&str] = &["O0", "O1", "O3", "fastmath"];

/// Variants that keep IEEE semantics. Only these set the tolerance; a build
/// that abandons the standard says nothing about what a correct port may do.
const IEEE_CONFORMANT: &[&str] = &["O0", "O1", "O3"];

/// The host has 16 cores but under 3 GB of RAM, and each process reads a share
/// of the coefficient tree. Sizing this from core count gets runs OOM-killed.
const CONCURRENCY: usize = 4;

struct CaseOutcome {
    listings: BTreeMap<String, ParsedListing>,
    failures: Vec<(String, String)>,
}

fn main() -> ExitCode {
    let cases = sweep_cases();
    let mut names = vec![REFERENCE.to_string()];
    names.extend(COMPARED.iter().map(|n| n.to_string()));

    for name in &names {
        let bin = variant_bin(name);
        if !bin.is_file() {
            eprintln!("missing variant binary: {}", bin.display());
            eprintln!("run tools/build-variants.sh first");
            return ExitCode::FAILURE;
        }
    }

    let total_runs = cases.len() * names.len();
    eprintln!(
        "running {} cases across {} variants ({total_runs} voacapl runs)",
        cases.len(),
        names.len()
    );
    let started = Instant::now();

    // Each case runs every variant, so compared listings always come from
    // identical input. Cases are independent, so they parallelise.
    let outcomes = map_limit(&cases, CONCURRENCY, |case, index| {
        let deck = match build_deck(case) {
            Ok(d) => d,
            Err(e) => {
                return CaseOutcome {
                    listings: BTreeMap::new(),
                    failures: vec![("deck".to_string(), e.to_string())],
                }
            }
        };

        // Every run needs its own itshfbc tree: the engine names its antenna
        // scratch files gain01.dat and gain02.dat regardless of the caller, so
        // concurrent runs sharing a tree corrupt each other.
        let root = match IsolatedRoot::create(&index.to_string()) {
            Ok(r) => r,
            Err(e) => {
                return CaseOutcome {
                    listings: BTreeMap::new(),
                    failures: vec![("isolate".to_string(), e.to_string())],
                }
            }
        };

        let mut listings = BTreeMap::new();
        let mut failures = Vec::new();
        for name in &names {
            match run_deck(&variant_bin(name), root.path(), &deck) {
                Ok(text) => {
                    listings.insert(name.clone(), parse_listing(&text));
                }
                Err(e) => failures.push((name.clone(), e.to_string())),
            }
        }
        CaseOutcome { listings, failures }
    });

    let elapsed = started.elapsed();

    let mut failure_counts: BTreeMap<String, usize> = BTreeMap::new();
    let mut first_failure: BTreeMap<String, String> = BTreeMap::new();
    for outcome in &outcomes {
        for (name, message) in &outcome.failures {
            *failure_counts.entry(name.clone()).or_default() += 1;
            first_failure
                .entry(name.clone())
                .or_insert_with(|| message.clone());
        }
    }

    let reference_cells: usize = outcomes
        .iter()
        .filter_map(|o| o.listings.get(REFERENCE))
        .map(|l| l.numeric.len())
        .sum();

    eprintln!(
        "finished in {:.1}s; {reference_cells} reference cells parsed",
        elapsed.as_secs_f64()
    );

    let mut merged: BTreeMap<String, Comparison> = BTreeMap::new();
    let mut compared_cases: BTreeMap<String, usize> = BTreeMap::new();

    for name in COMPARED {
        let mut acc = Comparison::default();
        let mut count = 0usize;
        for outcome in &outcomes {
            let (Some(reference), Some(other)) =
                (outcome.listings.get(REFERENCE), outcome.listings.get(*name))
            else {
                continue;
            };
            acc.merge(compare_listings(reference, other));
            count += 1;
        }
        merged.insert((*name).to_string(), acc);
        compared_cases.insert((*name).to_string(), count);
    }

    print_report(
        &cases.len(),
        reference_cells,
        elapsed.as_secs_f64(),
        &failure_counts,
        &first_failure,
        &compared_cases,
        &merged,
    );

    if let Some(path) = out_path() {
        match fs::write(&path, json_report(&merged, &failure_counts, cases.len())) {
            Ok(()) => eprintln!("wrote {path}"),
            Err(e) => {
                eprintln!("could not write {path}: {e}");
                return ExitCode::FAILURE;
            }
        }
    }

    ExitCode::SUCCESS
}

fn out_path() -> Option<String> {
    let argv: Vec<String> = std::env::args().collect();
    let index = argv.iter().position(|a| a == "--out")?;
    argv.get(index + 1).cloned()
}

fn fmt_num(value: f64) -> String {
    if value == 0.0 {
        "0".to_string()
    } else if value.fract() == 0.0 && value.abs() < 1e6 {
        format!("{value:.0}")
    } else if value.abs() < 0.001 {
        format!("{value:.1e}")
    } else {
        format!("{value:.4}")
    }
}

#[allow(clippy::too_many_arguments)]
fn print_report(
    case_count: &usize,
    reference_cells: usize,
    elapsed_secs: f64,
    failure_counts: &BTreeMap<String, usize>,
    first_failure: &BTreeMap<String, String>,
    compared_cases: &BTreeMap<String, usize>,
    merged: &BTreeMap<String, Comparison>,
) {
    println!("# VOACAP evaluation-order sensitivity\n");
    println!(
        "Reference build `{REFERENCE}`. {case_count} sweep cases, \
         {reference_cells} numeric cells parsed from the reference, \
         measured in {elapsed_secs:.1}s.\n"
    );
    println!(
        "Every variant is the same Fortran source compiled with different \
         optimisation flags, so any difference below is the model's sensitivity \
         to floating-point evaluation order, not to physics.\n"
    );

    if failure_counts.is_empty() {
        println!("All variants completed every case.\n");
    } else {
        println!("## Runs that did not complete\n");
        println!("| variant | failed cases | first failure |");
        println!("| --- | --: | --- |");
        for (name, count) in failure_counts {
            let message = first_failure
                .get(name)
                .map(|m| m.replace('|', "\\|"))
                .unwrap_or_default();
            println!("| `{name}` | {count} | {message} |");
        }
        println!();
    }

    for name in COMPARED {
        let Some(comparison) = merged.get(*name) else {
            continue;
        };
        let cases = compared_cases.get(*name).copied().unwrap_or(0);
        println!("## `{REFERENCE}` vs `{name}`\n");
        if comparison.is_empty() {
            println!("No cases completed on both builds, so nothing was compared.\n");
            continue;
        }
        println!("Compared over {cases} cases.\n");
        println!(
            "| field | samples | differing | % | max abs | p95 abs | p99 abs | max rel | only in one |"
        );
        println!("| --- | --: | --: | --: | --: | --: | --: | --: | --: |");
        for f in comparison.stats() {
            let pct = if f.samples == 0 {
                0.0
            } else {
                100.0 * f.differing as f64 / f.samples as f64
            };
            println!(
                "| {} | {} | {} | {:.2} | {} | {} | {} | {} | {} |",
                f.row,
                f.samples,
                f.differing,
                pct,
                fmt_num(f.max_abs),
                fmt_num(f.p95_abs),
                fmt_num(f.p99_abs),
                fmt_num(f.max_rel),
                f.only_in_one
            );
        }
        let m = comparison.modes;
        let mode_pct = if m.compared == 0 {
            0.0
        } else {
            100.0 * m.mismatched as f64 / m.compared as f64
        };
        println!(
            "\nMODE: {} compared, {} mismatched ({mode_pct:.2}%), {} present in only one listing.\n",
            m.compared,
            m.mismatched,
            m.only_in_a + m.only_in_b
        );
    }

    print_tolerance_table(merged);

    println!("\n## Path regimes\n");
    for p in PATHS {
        println!("- `{}` — {}", p.id, p.regime);
    }
}

/// The deliverable: a per-field tolerance taken from the widest disagreement
/// between IEEE-conformant builds of the same source.
fn print_tolerance_table(merged: &BTreeMap<String, Comparison>) {
    let mut worst: BTreeMap<String, (f64, usize)> = BTreeMap::new();

    for name in IEEE_CONFORMANT {
        let Some(comparison) = merged.get(*name) else {
            continue;
        };
        for f in comparison.stats() {
            let entry = worst.entry(f.row.clone()).or_insert((0.0, 0));
            entry.0 = entry.0.max(f.max_abs);
            entry.1 += f.only_in_one;
        }
    }

    println!("## Derived tolerance\n");
    if worst.is_empty() {
        println!("No IEEE-conformant variant completed, so no tolerance could be derived.\n");
        return;
    }
    println!(
        "Widest disagreement between IEEE-conformant builds ({}). A port that \
         stays inside these bounds is no further from the reference than the \
         reference is from itself under a different optimisation level.\n",
        IEEE_CONFORMANT.join(", ")
    );
    println!("| field | observed max abs | structural disagreements |");
    println!("| --- | --: | --: |");
    for (row, (max_abs, only)) in &worst {
        println!("| {row} | {} | {only} |", fmt_num(*max_abs));
    }
    println!();
}

fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out
}

fn field_json(f: &FieldStats) -> String {
    format!(
        "{{\"row\":\"{}\",\"samples\":{},\"differing\":{},\"maxAbs\":{},\
         \"p50Abs\":{},\"p95Abs\":{},\"p99Abs\":{},\"maxRel\":{},\"onlyInOne\":{}}}",
        escape(&f.row),
        f.samples,
        f.differing,
        f.max_abs,
        f.p50_abs,
        f.p95_abs,
        f.p99_abs,
        f.max_rel,
        f.only_in_one
    )
}

fn json_report(
    merged: &BTreeMap<String, Comparison>,
    failures: &BTreeMap<String, usize>,
    case_count: usize,
) -> String {
    let comparisons: Vec<String> = merged
        .iter()
        .map(|(name, comparison)| {
            let fields: Vec<String> = comparison.stats().iter().map(field_json).collect();
            let m = comparison.modes;
            format!(
                "\"{}\":{{\"fields\":[{}],\"modes\":{{\"compared\":{},\"mismatched\":{},\
                 \"onlyInOne\":{}}}}}",
                escape(name),
                fields.join(","),
                m.compared,
                m.mismatched,
                m.only_in_a + m.only_in_b
            )
        })
        .collect();

    let failed: Vec<String> = failures
        .iter()
        .map(|(name, count)| format!("\"{}\":{count}", escape(name)))
        .collect();

    format!(
        "{{\"reference\":\"{REFERENCE}\",\"cases\":{case_count},\
         \"failedRuns\":{{{}}},\"comparisons\":{{{}}}}}\n",
        failed.join(","),
        comparisons.join(",")
    )
}
