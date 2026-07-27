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
//! A corpus that never reaches a fix's site reports no movement, which
//! looks identical to a fix that changes nothing. `--corpus` chooses
//! the corpus, and every fix names the one that reaches it:
//!
//! | corpus    | cases                           | fixes it can see                 |
//! | --------- | ------------------------------- | -------------------------------- |
//! | `sweep`   | 96 method-30 systems runs       | `pole_file`                      |
//! | `luf`     | fuzz cases rewritten to 26      | `luf_scan_best`, `luf_pass_area` |
//! | `curtain` | sweep paths with a KOP=6 aerial | `curtain_elevation`              |
//! | `area`    | area grids over their centres   | `area_centre_nudge`              |
//!
//! `area_antenna_end` has no corpus, and the reason is recorded in
//! `docs/corrected.md`: no input this crate can build reaches it.
//!
//! Usage: `cargo run --release --bin correctcheck -- [--fix NAME]
//! [--corpus NAME] [--cases N] [--jobs J]`
//!
//! With no `--fix`, every fix is listed with the corpus that reaches
//! it.

use std::collections::BTreeMap;
use std::process::ExitCode;

use propcore::deck::{build_deck, AntennaChoice, DeckCase};
use propcore::engine::area::{Grid, Projection};
use propcore::engine::coefficients::FoF2Model;
use propcore::engine::model::{Fixes, Model};
use propcore::engine::output::render;
use propcore::engine::run::{run_area, AreaInputs};
use propcore::fuzz::fuzz_cases;
use propcore::listing::{parse_listing, ModeSample, Sample};
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

/// Every fix, with the corpus that reaches its site.
const FIX_NAMES: [(&str, &str); 6] = [
    ("pole_file", "sweep"),
    ("curtain_elevation", "curtain"),
    ("luf_scan_best", "luf"),
    ("luf_pass_area", "luf"),
    ("area_centre_nudge", "area"),
    ("area_antenna_end", "no corpus can reach it; see corrected.md"),
];

/// The one sample file in the tree whose antenna type is the IONCAP
/// curtain, `KOP = 6`. `antcheck` verifies its whole gain table against
/// the reference's own `gain01.dat`, which is what makes the compatible
/// half of the curtain corpus trustworthy.
const CURTAIN_FILE: &str = "samples/sample.26";

/// Which cases to run, and how to read what they print.
#[derive(Clone, Copy, PartialEq)]
enum Corpus {
    /// The 96 method-30 systems cases the port is verified on.
    Sweep,
    /// Method-26 decks, the only card methods that run the LUF search.
    /// The paths, seasons and antennas come from the fuzz corpus,
    /// whose frequencies the LUF methods ignore — they sweep their own
    /// complement. This is the corpus `lufcheck` verifies the
    /// compatible tier on against the reference.
    Luf,
    /// The sweep paths with a curtain at both ends, which is the only
    /// way anything reaches the `KOP = 6` pattern: every other corpus
    /// here uses `default/isotrope`, whose gain is a constant.
    ///
    /// Both ends rather than one, because the two ends read the same
    /// table at different elevations and a fix at either end can move
    /// a printed cell.
    Curtain,
    /// Area grids, which have no deck at all. See [`area_cases`].
    Area,
}

impl Corpus {
    fn by_name(name: &str) -> Option<Corpus> {
        match name {
            "sweep" => Some(Corpus::Sweep),
            "luf" => Some(Corpus::Luf),
            "curtain" => Some(Corpus::Curtain),
            "area" => Some(Corpus::Area),
            _ => None,
        }
    }

    fn cases(self, limit: Option<usize>) -> Vec<DeckCase> {
        let mut cases = match self {
            Corpus::Sweep => sweep_cases(),
            Corpus::Luf => {
                let mut cases = fuzz_cases(0, limit.unwrap_or(48) as u64);
                for case in cases.iter_mut() {
                    case.method = 26;
                }
                cases
            }
            Corpus::Curtain => {
                let mut cases = sweep_cases();
                for case in cases.iter_mut() {
                    // Beam bearings left at zero: the pattern is cut at
                    // the path azimuth relative to the beam, so a beam
                    // of zero still exercises every elevation the path
                    // uses.
                    case.tx_antennas = vec![AntennaChoice::whole_band(CURTAIN_FILE, 0.0)];
                    case.rx_antennas = vec![AntennaChoice::whole_band(CURTAIN_FILE, 0.0)];
                }
                cases
            }
            // Not a deck corpus; `area_cases` builds these.
            Corpus::Area => Vec::new(),
        };
        if let Some(n) = limit {
            cases.truncate(n);
        }
        cases
    }

    /// The printed values, as cells that can be aligned across two
    /// runs. A systems listing and a MUF table are different formats,
    /// so each corpus reads its own.
    fn samples(self, text: &str) -> (Vec<Sample>, Vec<ModeSample>) {
        match self {
            Corpus::Sweep | Corpus::Curtain => {
                let parsed = parse_listing(text);
                (parsed.numeric, parsed.modes)
            }
            Corpus::Luf => (outmuf_samples(text), Vec::new()),
            Corpus::Area => (Vec::new(), Vec::new()),
        }
    }
}

/// The data rows of a method-26 table, as cells.
///
/// `OUTMUF` builds its format as `(1H ,2X,2F6.1,` then one `F7.2` per
/// column, so the fields are read by column: a MUF of 1000.00 fills
/// its field and leaves no space before the next. Same reading as
/// `lufcheck`, which is what proves the compatible tier of this corpus
/// against the reference.
fn outmuf_samples(text: &str) -> Vec<Sample> {
    const NAMES: [&str; 5] = ["FOT", "HPF", "ES MUF", "MUF", "LUF"];
    let field = |line: &str, from: usize, to: usize| -> Option<f64> {
        if line.len() < to {
            return None;
        }
        line[from..to].trim().parse().ok()
    };
    let mut out = Vec::new();
    for line in text.lines() {
        let (Some(gmt), Some(_lmt)) = (field(line, 3, 9), field(line, 9, 15)) else {
            continue;
        };
        if !(1.0..=24.0).contains(&gmt) {
            continue;
        }
        let hour = ((gmt.round() as i64).rem_euclid(24)) as u8;
        for (c, name) in NAMES.iter().enumerate() {
            if let Some(v) = field(line, 15 + c * 7, 22 + c * 7) {
                out.push(Sample {
                    hour,
                    row: (*name).to_string(),
                    slot: 0,
                    value: v,
                });
            }
        }
    }
    out
}

/// One area grid to run twice, and why it is here.
struct AreaCase {
    id: &'static str,
    inputs: AreaInputs,
}

/// The area grids, chosen for what `area_centre_nudge` needs: a grid
/// point that lands exactly on the fixed station, at a longitude on
/// each side of zero.
///
/// The nudge that moves a point off the station is what the defect
/// skips, so a grid whose centre point coincides with the station is
/// the only thing that can show it. An odd number of points on each
/// side puts a point at offset zero, which is that centre. The
/// positive-longitude grid is the control: there the comparison
/// already worked, so nothing should move.
fn area_cases() -> Vec<AreaCase> {
    let grid = |plat: f32, plon: f32| Grid {
        projection: Projection::GreatCircle,
        plat,
        plon,
        xmin: -3000.0,
        xmax: 3000.0,
        ymin: -3000.0,
        ymax: 3000.0,
        nx: 5,
        ny: 5,
    };
    let inputs = |plat: f32, plon: f32, inverse: bool| AreaInputs {
        grid: grid(plat, plon),
        tx_lat_deg: f64::from(plat),
        tx_lon_deg: f64::from(plon),
        month: 6,
        ssn: 100.0,
        hour: 18,
        freqs_mhz: vec![11.850],
        required_snr_db: 73.0,
        noise_dbw: 145,
        watts: 100.0,
        psc: [1.0, 1.0, 1.0, 0.0],
        method: 30,
        fof2: FoF2Model::Ccir,
        inverse,
        tx_antenna: None,
        rx_antenna: None,
        model: Model::Compatible,
    };
    vec![
        AreaCase {
            id: "west-of-greenwich",
            inputs: inputs(35.8, -5.9, false),
        },
        AreaCase {
            id: "east-of-greenwich",
            inputs: inputs(35.8, 5.9, false),
        },
        AreaCase {
            id: "west-of-greenwich-inverse",
            inputs: inputs(35.8, -5.9, true),
        },
        AreaCase {
            id: "far-west",
            inputs: inputs(40.0, -105.0, false),
        },
    ]
}

/// Runs one area grid twice and reports which printed fields moved.
///
/// The grid file prints each point's coordinates before its values, so
/// the coordinates are compared too: with this fix they are the first
/// thing to move, and a grid point that changed place changes every
/// value under it.
fn area_outcome(case: &AreaCase, fixes: Fixes, index: usize) -> Outcome {
    let mut out = Outcome {
        id: case.id.to_string(),
        compared: 0,
        moved: 0,
        structural: 0,
        by_row: BTreeMap::new(),
        failure: None,
    };
    let root = match IsolatedRoot::create(&format!("correct-area{index}")) {
        Ok(r) => r,
        Err(e) => {
            out.failure = Some(format!("tree: {e}"));
            return out;
        }
    };
    let run = |model: Model| {
        let mut inputs = case.inputs.clone();
        inputs.model = model;
        run_area(root.path(), &inputs)
    };
    let base = match run(Model::Compatible) {
        Ok(p) => p,
        Err(e) => {
            out.failure = Some(format!("compatible: {e}"));
            return out;
        }
    };
    let fixed = match run(Model::from_fixes(fixes)) {
        Ok(p) => p,
        Err(e) => {
            out.failure = Some(format!("fixed: {e}"));
            return out;
        }
    };
    if base.len() != fixed.len() {
        out.failure = Some(format!("{} points against {}", base.len(), fixed.len()));
        return out;
    }

    let note = |row: &str, delta: f64, out: &mut Outcome| {
        out.moved += 1;
        let e = out.by_row.entry(row.to_string()).or_insert((0, 0.0));
        e.0 += 1;
        e.1 = e.1.max(delta);
    };
    for (a, b) in base.iter().zip(&fixed) {
        out.compared += 2;
        if a.lat != b.lat {
            note("LAT", f64::from((a.lat - b.lat).abs()), &mut out);
        }
        if a.print_lon != b.print_lon {
            note(
                "LON",
                f64::from((a.print_lon - b.print_lon).abs()),
                &mut out,
            );
        }
        for (i, (x, y)) in a.fields.iter().zip(&b.fields).enumerate() {
            out.compared += 1;
            if x == y {
                continue;
            }
            let row = AREA_COLUMNS.get(i).copied().unwrap_or("?");
            // The fields are text, and some of them are not numbers,
            // so the size of the change is only reported where both
            // sides parse.
            let delta = match (x.trim().parse::<f64>(), y.trim().parse::<f64>()) {
                (Ok(p), Ok(q)) => (p - q).abs(),
                _ => 0.0,
            };
            note(row, delta, &mut out);
        }
    }
    out
}

/// `OUTAREA`'s columns, in the order it writes them.
const AREA_COLUMNS: [&str; 24] = [
    "MUF", "MODE", "ANGLE", "DELAY", "VHITE", "MUFda", "LOSS", "DBU", "SDBW", "NDBW", "SNR",
    "RPWRG", "REL", "MPROB", "SPROB", "TGAIN", "RGAIN", "SNRxx", "DU", "DL", "SIGLW", "SIGUP",
    "PWRCT", "ANGLER",
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
        println!("Fixes, by the name --fix takes, and the corpus that reaches each:");
        for (n, corpus) in FIX_NAMES {
            println!("  {n:<18} {corpus}");
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
    let corpus_name = flag("--corpus").unwrap_or_else(|| "sweep".to_string());
    let Some(corpus) = Corpus::by_name(&corpus_name) else {
        eprintln!("unknown corpus {corpus_name:?}; try sweep, luf, curtain or area");
        return ExitCode::FAILURE;
    };

    // An area run has no deck and prints a grid file rather than a
    // listing, so it takes its own path from here.
    if corpus == Corpus::Area {
        let cases = area_cases();
        eprintln!("running {} area grids with only {name} on", cases.len());
        let outcomes = map_limit(&cases, jobs, |case, index| area_outcome(case, fixes, index));
        return report(&name, &corpus_name, &outcomes);
    }

    let cases = corpus.cases(limit);
    eprintln!(
        "running {} {corpus_name} cases with only {name} on",
        cases.len()
    );

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

        compare(corpus, &base, &fixed, &mut out);
        out
    });

    report(&name, &corpus_name, &outcomes)
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

fn compare(corpus: Corpus, base: &str, fixed: &str, out: &mut Outcome) {
    let (a_numeric, a_modes) = corpus.samples(base);
    let (b_numeric, b_modes) = corpus.samples(fixed);

    let key = |s: &Sample| (s.hour, s.row.clone(), s.slot);
    let left: BTreeMap<_, f64> = a_numeric.iter().map(|s| (key(s), s.value)).collect();
    let right: BTreeMap<_, f64> = b_numeric.iter().map(|s| (key(s), s.value)).collect();

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
    let modes_a: BTreeMap<_, _> = a_modes.iter().map(|m| (m.key(), &m.mode)).collect();
    let modes_b: BTreeMap<_, _> = b_modes.iter().map(|m| (m.key(), &m.mode)).collect();
    for (k, v) in &modes_a {
        out.compared += 1;
        if modes_b.get(k) != Some(v) {
            out.moved += 1;
            let entry = out.by_row.entry("MODE".to_string()).or_insert((0, 0.0));
            entry.0 += 1;
        }
    }
}

fn report(name: &str, corpus: &str, outcomes: &[Outcome]) -> ExitCode {
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
        "{} of {} {corpus} cases touched; {moved} of {compared} cells moved, {structural} structural.",
        touched,
        outcomes.len()
    );
    println!();
    if rows.is_empty() {
        println!(
            "No printed cell changes on the {corpus} corpus. That is only \
             evidence about the fix if this corpus reaches its site."
        );
    } else {
        println!("| row | cells moved | worst change |");
        println!("| --- | --: | --: |");
        let mut sorted: Vec<&Moved> = rows.values().collect();
        sorted.sort_by(|a, b| b.cells.cmp(&a.cells).then(a.row.cmp(&b.row)));
        for m in sorted {
            println!("| {} | {} | {:.2} |", m.row, m.cells, m.worst);
        }
    }

    // Which cases moved, not only how many: a fix that bites in a
    // minority of cases needs a named case before anything can be read
    // back from it.
    let names: Vec<&str> = outcomes
        .iter()
        .filter(|o| o.moved + o.structural > 0)
        .map(|o| o.id.as_str())
        .collect();
    if !names.is_empty() {
        println!();
        println!("Cases touched: {}", names.join(", "));
    }

    if !failures.is_empty() {
        println!();
        println!("{} case(s) could not run.", failures.len());
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
