//! Antenna check: this port's gain tables against the reference's.
//!
//! For every antenna definition file in the tree, a deck is built that
//! names it at the transmitter, the reference engine runs, and the
//! `run/gain01.dat` it wrote is compared with
//! [`point_to_point_table`]'s answer — 30 frequencies by 91 elevation
//! angles, plus the per-frequency efficiency and the header line.
//!
//! Because the reference writes that file itself, no instrumented build
//! is needed: the comparison is against the shipped engine's own output
//! at the 0.001 dB its `f7.3` fields carry.
//!
//! Families this port does not compute yet are counted as pending
//! rather than passing, so the report doubles as the remaining work
//! list.
//!
//! Usage: `cargo run --release --bin antcheck [--only NAME] [--verbose]`

use std::path::Path;
use std::process::ExitCode;

use propcore::engine::antenna::{
    dazel0, point_to_point_table, read_antenna, AntennaEnd, AntennaSetup, GainTable, ELEVS, FREQS,
};
use propcore::engine::model::Model;
use propcore::runner::{itshfbc_dir, run_deck, variant_bin, IsolatedRoot};

/// The circuit every probe deck uses. Only the azimuth from these two
/// points reaches the pattern, so one circuit is enough. Held as `f32`
/// because that is how the engine reads them off the card.
const FROM: (f32, f32) = (35.8, -5.9);
const TO: (f32, f32) = (44.9, 20.5);

/// The card's frequency range, in whole MHz.
const MIN_FREQ: i32 = 2;
const MAX_FREQ: i32 = 30;

/// Power in kW on the transmitter card.
const POWER_KW: f64 = 0.1;

struct Outcome {
    name: String,
    jant: i32,
    verdict: Verdict,
}

enum Verdict {
    /// Every cell agreed.
    Matched { cells: usize },
    /// The port answered and the answer differs.
    Differed {
        worst: f64,
        cells: usize,
        differing: usize,
        first: String,
    },
    /// The family is not ported yet.
    Pending(String),
    /// The harness could not get a reference answer.
    Broken(String),
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let only = argv
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| argv.get(i + 1))
        .cloned();
    let verbose = argv.iter().any(|a| a == "--verbose");

    let reference = variant_bin("O2");
    if !reference.is_file() {
        eprintln!("no O2 variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }
    let tree = itshfbc_dir();
    let files = match antenna_files(&tree) {
        Ok(f) => f,
        Err(e) => {
            eprintln!("listing antennas: {e}");
            return ExitCode::FAILURE;
        }
    };
    let files: Vec<String> = files
        .into_iter()
        .filter(|f| only.as_ref().is_none_or(|o| f.contains(o.as_str())))
        .collect();

    println!("# Antenna check: {} definition files\n", files.len());

    let mut outcomes = Vec::new();
    for name in &files {
        outcomes.push(check_one(&reference, &tree, name));
    }

    let matched: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| matches!(o.verdict, Verdict::Matched { .. }))
        .collect();
    let differed: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| matches!(o.verdict, Verdict::Differed { .. }))
        .collect();
    let pending: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| matches!(o.verdict, Verdict::Pending(_)))
        .collect();
    let broken: Vec<&Outcome> = outcomes
        .iter()
        .filter(|o| matches!(o.verdict, Verdict::Broken(_)))
        .collect();

    let cells: usize = outcomes
        .iter()
        .map(|o| match o.verdict {
            Verdict::Matched { cells } | Verdict::Differed { cells, .. } => cells,
            _ => 0,
        })
        .sum();

    println!("| outcome | files |");
    println!("| --- | --: |");
    println!("| identical | {} |", matched.len());
    println!("| differing | {} |", differed.len());
    println!("| not ported yet | {} |", pending.len());
    println!("| harness failure | {} |", broken.len());
    println!("\n{cells} table cells compared.\n");

    if !differed.is_empty() {
        println!("## Differences\n");
        println!("| antenna | type | differing | worst | first differing cell |");
        println!("| --- | --: | --: | --: | --- |");
        for o in &differed {
            if let Verdict::Differed {
                worst,
                differing,
                first,
                ..
            } = &o.verdict
            {
                println!(
                    "| {} | {} | {} | {} | {} |",
                    o.name, o.jant, differing, worst, first
                );
            }
        }
        println!();
    }

    if !broken.is_empty() {
        println!("## Harness failures\n");
        for o in &broken {
            if let Verdict::Broken(why) = &o.verdict {
                println!("- {} (type {}): {}", o.name, o.jant, why);
            }
        }
        println!();
    }

    if verbose && !matched.is_empty() {
        println!("## Verified\n");
        for o in &matched {
            println!("- {} (type {})", o.name, o.jant);
        }
        println!();
    }

    if !pending.is_empty() {
        println!("## Families still to port\n");
        let mut by_family: std::collections::BTreeMap<String, Vec<&str>> = Default::default();
        for o in &pending {
            if let Verdict::Pending(family) = &o.verdict {
                by_family
                    .entry(family.clone())
                    .or_default()
                    .push(o.name.as_str());
            }
        }
        println!("| family | files |");
        println!("| --- | --: |");
        for (family, names) in &by_family {
            println!("| {} | {} |", family, names.len());
        }
        println!();
    }

    if differed.is_empty() && broken.is_empty() {
        println!("Verdict: every ported family matches the reference.");
        ExitCode::SUCCESS
    } else {
        println!("Verdict: the port and the reference disagree.");
        ExitCode::FAILURE
    }
}

/// Every definition file under `<tree>/antennas`, as a path relative to
/// that directory.
fn antenna_files(tree: &Path) -> std::io::Result<Vec<String>> {
    let root = tree.join("antennas");
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if let Ok(rel) = path.strip_prefix(&root) {
                out.push(rel.to_string_lossy().into_owned());
            }
        }
    }
    out.sort();
    Ok(out)
}

fn check_one(reference: &Path, tree: &Path, name: &str) -> Outcome {
    let jant = read_antenna(tree, name).map(|f| f.jant()).unwrap_or(-1);
    let mut outcome = Outcome {
        name: name.to_string(),
        jant,
        verdict: Verdict::Broken("not run".into()),
    };

    // The bracketed field is exactly 21 columns wide; a longer name
    // cannot be expressed on the card at all.
    if name.len() > 21 {
        outcome.verdict = Verdict::Broken("name does not fit the card's 21 columns".into());
        return outcome;
    }

    let file = match read_antenna(tree, name) {
        Ok(f) => f,
        Err(e) => {
            outcome.verdict = Verdict::Broken(e);
            return outcome;
        }
    };

    let (azimuth, _) = dazel0(FROM.0, FROM.1, TO.0, TO.1);
    let ported = match point_to_point_table(&AntennaSetup {
        file: &file,
        end: AntennaEnd::Transmit,
        min_freq: MIN_FREQ,
        max_freq: MAX_FREQ,
        design_freq: 0.0,
        beam_deg: 0.0,
        power_field: POWER_KW as f32,
        azimuth_deg: azimuth,
        // The reference is the oracle here, so the only tier this
        // harness can judge is the one that reproduces it.
        model: Model::Compatible,
    }) {
        Ok(t) => t,
        Err(e) => {
            outcome.verdict = Verdict::Pending(e.family.to_string());
            return outcome;
        }
    };

    let root = match IsolatedRoot::create(&format!("ant-{}", name.replace('/', "-"))) {
        Ok(r) => r,
        Err(e) => {
            outcome.verdict = Verdict::Broken(format!("private tree: {e}"));
            return outcome;
        }
    };
    if let Err(e) = run_deck(reference, root.path(), &probe_deck(name)) {
        outcome.verdict = Verdict::Broken(format!("reference failed: {e}"));
        return outcome;
    }
    let text = match std::fs::read_to_string(root.path().join("run/gain01.dat")) {
        Ok(t) => t,
        Err(e) => {
            outcome.verdict = Verdict::Broken(format!("reading gain01.dat: {e}"));
            return outcome;
        }
    };
    let expected = match parse_gain_file(&text) {
        Ok(t) => t,
        Err(e) => {
            outcome.verdict = Verdict::Broken(e);
            return outcome;
        }
    };

    outcome.verdict = compare(&expected, &ported);
    outcome
}

/// A deck naming `antenna` at the transmitter.
///
/// The bracketed antenna field is exactly 21 columns; the fields around
/// it are the same fixed widths `propcore::deck` writes.
fn probe_deck(antenna: &str) -> String {
    let lat = |v: f32, width: usize| {
        format!(
            "{:>width$.2}{}",
            v.abs(),
            if v >= 0.0 { "N" } else { "S" },
            width = width
        )
    };
    let lon = |v: f32| {
        format!(
            "{:>9.2}{}",
            v.abs(),
            if v >= 0.0 { "E" } else { "W" }
        )
    };
    let circuit = format!(
        "CIRCUIT   {}{}{}{}  S     0",
        lat(FROM.0, 5),
        lon(FROM.1),
        lat(TO.0, 9),
        lon(TO.1)
    );
    let padded = format!("{antenna:<21}");
    format!(
        "LINEMAX      55       number of lines-per-page\n\
         COEFFS    CCIR\n\
         TIME          1    1    1    1\n\
         MONTH      202607.00\n\
         SUNSPOT    100.\n\
         LABEL     antcheck            probe               \n\
         {circuit}\n\
         SYSTEM       1. 145. 0.10  90. 24.0 3.00 0.10\n\
         FPROB      1.00 1.00 1.00 1.00\n\
         ANTENNA       1    1{MIN_FREQ:5}{MAX_FREQ:5}     0.000[{padded}]  0.0{POWER_KW:10.4}\n\
         ANTENNA       2    2    2   30     0.000[default/isotrope     ]  0.0    0.0000\n\
         FREQUENCY 14.20 0.00 0.00 0.00 0.00 0.00 0.00 0.00 0.00 0.00 0.00\n\
         METHOD       30    0\n\
         EXECUTE\n\
         QUIT\n"
    )
}

/// Reads a `gainNN.dat` written by the reference.
fn parse_gain_file(text: &str) -> Result<GainTable, String> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return Err("gain file too short".into());
    }
    let header = lines[1];
    let mut table = GainTable {
        fs: field(header, 0, 5),
        fe: field(header, 5, 5),
        beam_main: field(header, 10, 7),
        offazim: field(header, 17, 7),
        cond: field(header, 24, 10),
        diel: field(header, 34, 10),
        ..Default::default()
    };

    let mut index = 2usize;
    for ifreq in 0..FREQS {
        let line = lines
            .get(index)
            .ok_or_else(|| format!("gain file ended at frequency {}", ifreq + 1))?;
        index += 1;
        table.eff[ifreq] = field(line, 2, 6);
        let mut row = [0.0f32; ELEVS];
        for (i, slot) in row.iter_mut().enumerate().take(10) {
            *slot = field(line, 9 + i * 7, 7);
        }
        let mut filled = 10usize;
        while filled < ELEVS {
            let cont = lines
                .get(index)
                .ok_or_else(|| format!("gain file ended inside frequency {}", ifreq + 1))?;
            index += 1;
            for i in 0..10 {
                if filled < ELEVS {
                    row[filled] = field(cont, 9 + i * 7, 7);
                    filled += 1;
                }
            }
        }
        table.gains[ifreq] = row;
    }
    Ok(table)
}

fn field(line: &str, start: usize, width: usize) -> f32 {
    let chars: Vec<char> = line.chars().collect();
    if start >= chars.len() {
        return 0.0;
    }
    let end = (start + width).min(chars.len());
    let text: String = chars[start..end].iter().collect();
    text.trim().parse().unwrap_or(0.0)
}

/// Rounds as the gain file's format does, so the port is judged on what
/// the reference could have written rather than on digits the file never
/// carried. The header's `f7.2` prints 57.406578 as 57.41.
fn round_to(v: f64, decimals: i32) -> f64 {
    let scale = 10f64.powi(decimals);
    (v * scale).round() / scale
}

fn compare(expected: &GainTable, ported: &GainTable) -> Verdict {
    let mut worst = 0.0f64;
    let mut first: Option<String> = None;
    let mut cells = 0usize;
    let differing = std::cell::Cell::new(0usize);

    let note = |label: String,
                a: f32,
                b: f32,
                decimals: i32,
                worst: &mut f64,
                first: &mut Option<String>| {
        // Both sides are rounded: the reference's own digits came back
        // through a 32-bit float, so 57.41 reads as 57.40999985.
        let reference = round_to(f64::from(a), decimals);
        let port = round_to(f64::from(b), decimals);
        let diff = (reference - port).abs();
        if diff > *worst {
            *worst = diff;
        }
        if diff != 0.0 {
            differing.set(differing.get() + 1);
            if first.is_none() {
                *first = Some(format!("{label}: reference {reference}, port {port}"));
            }
        }
    };

    // The second header line is (2f5.0, 2f7.2, 2f10.5).
    for (name, a, b, decimals) in [
        ("fs", expected.fs, ported.fs, 0),
        ("fe", expected.fe, ported.fe, 0),
        ("beam", expected.beam_main, ported.beam_main, 2),
        ("offazim", expected.offazim, ported.offazim, 2),
        ("conductivity", expected.cond, ported.cond, 5),
        ("dielectric", expected.diel, ported.diel, 5),
    ] {
        cells += 1;
        note(name.to_string(), a, b, decimals, &mut worst, &mut first);
    }

    // The rows are (i2, f6.2, (t10, 10f7.3)).
    for ifreq in 0..FREQS {
        cells += 1;
        note(
            format!("eff at {} MHz", ifreq + 1),
            expected.eff[ifreq],
            ported.eff[ifreq],
            2,
            &mut worst,
            &mut first,
        );
        for ielev in 0..ELEVS {
            cells += 1;
            note(
                format!("{} MHz, {} deg", ifreq + 1, ielev),
                expected.gains[ifreq][ielev],
                ported.gains[ifreq][ielev],
                3,
                &mut worst,
                &mut first,
            );
        }
    }

    match first {
        None => Verdict::Matched { cells },
        Some(first) => Verdict::Differed {
            worst,
            cells,
            differing: differing.get(),
            first,
        },
    }
}
