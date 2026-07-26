//! MUF check: the Rust MUF-only run against the reference's method-7
//! table.
//!
//! Card methods 7 to 11 are `ITRUN = 4`: the engine computes the hour's
//! MUFs from the full ionosphere and stops, with no systems model
//! after it. Method 7 prints `OUTLAY`'s table — two lines per hour
//! carrying, for each of the four layers, the lower decile, median and
//! upper decile of the MUF, the takeoff angle, the virtual and true
//! heights and the equivalent vertical frequency.
//!
//! That is a wider view of `CURMUF` than any systems method prints:
//! method 30's listing shows only the circuit MUF, FOT and HPF. Each
//! case runs the reference with a method-7 deck, parses the table by
//! column and compares every field at the decimals it prints.
//!
//! Usage: `cargo run --release --bin mufcheck [--cases N] [--from N]
//! [--jobs J]`

use std::process::ExitCode;

use propcore::deck::build_deck;
use propcore::engine::run::{run_muf, run_par, RunInputs};
use propcore::fuzz::fuzz_cases;
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};

/// Comparisons round half to even, the way the Fortran runtime's
/// formatted output does: a value that lands exactly on a printing
/// boundary, such as 327.25 in an `F7.1` field, prints as 327.2.
///
/// One hour: GMT, LMT, then seven fields for each of four layers.
#[derive(Debug, Clone, Copy)]
struct Row {
    gmt: f64,
    lmt: f64,
    layer: [[f64; 7]; 4],
}

/// The seven per-layer columns, in the order `OUTLAY` prints them.
const FIELDS: [&str; 7] = ["FOT", "MUF", "HPF", "ANGLE", "VIRTL", "TRUE", "FVERT"];

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
    // Method 7 prints `OUTLAY`'s per-layer table; method 3 computes
    // the same hour by the nomogram method and prints `OUTMUF`'s
    // four-column one; method 1 prints `OUTPAR`'s ionospheric
    // parameters, one line per control point per hour.
    let method = number("--method", 7) as u32;

    let reference = variant_bin("O2");
    if !reference.is_file() {
        eprintln!("no O2 variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }

    // The MUF methods read no frequencies and no antennas; the corpus
    // is reused for geometry, month and sunspot number.
    let mut cases = fuzz_cases(from, count);
    for case in cases.iter_mut() {
        case.method = method;
    }
    println!(
        "# MUF check: {} method-{method} cases, indices {from}..\n",
        cases.len()
    );

    struct Outcome {
        index: u64,
        cells: usize,
        diffs: Vec<String>,
        broken: Option<String>,
    }

    let outcomes = map_limit(&cases, jobs, |case, offset| {
        let index = from + offset as u64;
        let mut out = Outcome {
            index,
            cells: 0,
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
        let root = match IsolatedRoot::create(&format!("muf-{}", case.id)) {
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
        let inputs = RunInputs::from(case);
        if method == 1 {
            let want = parse_outpar(&text);
            let got = match run_par(root.path(), &inputs) {
                Ok(r) => r,
                Err(e) => {
                    out.broken = Some(format!("port: {e}"));
                    return out;
                }
            };
            if want.is_empty() || want.len() != got.len() {
                out.broken = Some(format!("parsed {} rows, port has {}", want.len(), got.len()));
                return out;
            }
            for (row, p) in want.iter().zip(&got) {
                // The printed fields, with the decimals each carries.
                let fields: [(&str, f32, f64); 21] = [
                    ("LAT", p.lat.abs(), 10.0),
                    ("LONG", p.lon.abs(), 10.0),
                    ("LMT", p.lmt, 10.0),
                    ("UT", p.gmt, 10.0),
                    ("E", p.fe, 100.0),
                    ("F1", p.f1, 10.0),
                    ("Y1", p.y1, 10.0),
                    ("H1", p.h1, 10.0),
                    ("FH/2", p.fh2, 10.0),
                    ("F2Z", p.f2z, 10.0),
                    ("Y2", p.y2, 10.0),
                    ("H2", p.h2, 10.0),
                    ("ES", p.es, 10.0),
                    ("MED", p.med, 10.0),
                    ("HI", p.esu, 10.0),
                    ("M3000", p.m3000, 100.0),
                    ("HPF2", p.hpf2, 10.0),
                    ("RAT", p.rat, 10.0),
                    ("ZEN", p.zen, 10.0),
                    ("ZMAX", p.zmax, 10.0),
                    ("MAGL", p.magl.abs(), 10.0),
                ];
                for (i, (name, got, decimals)) in fields.into_iter().enumerate() {
                    out.cells += 1;
                    let want = row[i];
                    if (want * decimals).round_ties_even() != (f64::from(got) * decimals).round_ties_even() {
                        out.diffs.push(format!(
                            "UT {} point {}: {name} reference {want}, port {got}",
                            row[3], row[0]
                        ));
                    }
                }
            }
            return out;
        }
        let expected = if method == 7 {
            parse_outlay(&text)
        } else {
            parse_outmuf(&text)
        };
        if expected.len() != 24 {
            out.broken = Some(format!("parsed {} rows, wanted 24", expected.len()));
            return out;
        }
        let hours = match run_muf(root.path(), &inputs) {
            Ok(h) => h,
            Err(e) => {
                out.broken = Some(format!("port: {e}"));
                return out;
            }
        };
        for (row, hour) in expected.iter().zip(&hours) {
            let mut check = |name: &str, want: f64, got: f32, decimals: f64| {
                let got = f64::from(got);
                out.cells += 1;
                if (want * decimals).round_ties_even() != (got * decimals).round_ties_even() {
                    out.diffs
                        .push(format!("hour {} {}: reference {}, port {}", row.gmt, name, want, got));
                }
            };
            check("GMT", row.gmt, hour.gmt, 10.0);
            check("LMT", row.lmt, hour.lmt, 10.0);
            if method != 7 {
                // `OUTMUF`: the four summary columns, at F7.2.
                check("FOT", row.layer[0][0], hour.fot, 100.0);
                check("HPF", row.layer[0][1], hour.hpf, 100.0);
                check("ES MUF", row.layer[0][2], hour.esmuf, 100.0);
                check("MUF", row.layer[0][3], hour.allmuf, 100.0);
                continue;
            }
            for (l, layer) in hour.layers.iter().enumerate() {
                // The heights print F6.0, everything else F6.1.
                let values = [
                    (layer.yfot, 10.0),
                    (layer.ymuf, 10.0),
                    (layer.yhpf, 10.0),
                    (layer.delmuf, 10.0),
                    (layer.hpmuf, 1.0),
                    (layer.htmuf, 1.0),
                    (layer.fvmuf, 10.0),
                ];
                for (f, (got, decimals)) in values.into_iter().enumerate() {
                    check(
                        &format!("layer {} {}", l + 1, FIELDS[f]),
                        row.layer[l][f],
                        got,
                        decimals,
                    );
                }
            }
        }
        out
    });

    let mut cells = 0usize;
    let mut failed = false;
    for o in &outcomes {
        cells += o.cells;
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
    println!("\n{} cells compared over {} cases.", cells, outcomes.len());
    if failed {
        println!("Verdict: the MUF table disagrees with the reference.");
        ExitCode::FAILURE
    } else {
        println!("Verdict: every table cell matches the reference.");
        ExitCode::SUCCESS
    }
}

/// Reads one fixed-width field, or `None` when it is blank.
fn field(line: &str, from: usize, to: usize) -> Option<f64> {
    let bytes = line.as_bytes();
    if bytes.len() < to {
        return None;
    }
    line[from..to].trim().parse().ok()
}

/// Pulls `OUTPAR`'s lines out of a method-1 listing.
///
/// The line is `2(1X,F5.1,A1),2F6.1,F6.2,2F6.1,F7.1,3F6.1,F7.1,3F5.1,
/// F6.2,2(F7.1,F6.1),F6.1,A1` — 21 numbers, read by column. The two
/// hemisphere letters and the sign they carry are dropped: the check
/// compares magnitudes, which is what the line prints.
fn parse_outpar(text: &str) -> Vec<[f64; 21]> {
    // (start, end) of each printed field.
    const COLS: [(usize, usize); 21] = [
        (1, 6),
        (8, 13),
        (14, 20),
        (20, 26),
        (26, 32),
        (32, 38),
        (38, 44),
        (44, 51),
        (51, 57),
        (57, 63),
        (63, 69),
        (69, 76),
        (76, 81),
        (81, 86),
        (86, 91),
        (91, 97),
        (97, 104),
        (104, 110),
        (110, 117),
        (117, 123),
        (123, 129),
    ];
    let mut out = Vec::new();
    for line in text.lines() {
        let mut row = [0.0f64; 21];
        let mut ok = true;
        for (i, (from, to)) in COLS.into_iter().enumerate() {
            match field(line, from, to) {
                Some(v) => row[i] = v,
                None => ok = false,
            }
        }
        // The UT column identifies a data line.
        if ok && (1.0..=24.0).contains(&row[3]) {
            out.push(row);
        }
    }
    out
}

/// Pulls the 24 rows out of an `OUTMUF` table: GMT, LMT, FOT, HPF, the
/// sporadic-E MUF and the circuit MUF, kept in the first layer slot.
///
/// `OUTMUF` builds its format as `(1H ,2X,2F6.1,` then one `F7.2` per
/// tabulated column, so the fields are read by column: a MUF of
/// 1000.00 fills its field completely and leaves no space before the
/// next one.
fn parse_outmuf(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for line in text.lines() {
        let (Some(gmt), Some(lmt)) = (field(line, 3, 9), field(line, 9, 15)) else {
            continue;
        };
        if !(1.0..=24.0).contains(&gmt) {
            continue;
        }
        let mut layer = [[0.0; 7]; 4];
        let mut ok = true;
        for (c, slot) in layer[0].iter_mut().take(4).enumerate() {
            match field(line, 15 + c * 7, 22 + c * 7) {
                Some(v) => *slot = v,
                None => ok = false,
            }
        }
        if ok {
            out.push(Row { gmt, lmt, layer });
        }
    }
    out
}

/// Pulls the 24 hour-pairs out of a method-7 listing.
///
/// `OUTLAY` writes each hour as two lines: `(' ',F4.1,F6.1,2(4F6.1,
/// 2F6.0,F6.1,2X))` for layers 1 and 2, then `(11X,2(4F6.1,2F6.0,
/// F6.1,2X))` for layers 3 and 4 in the same columns. The layout is
/// read by column rather than by splitting, because a full field
/// leaves no space between values.
fn parse_outlay(text: &str) -> Vec<Row> {
    // Where each layer group starts; the seven fields are six wide.
    const GROUPS: [usize; 2] = [11, 55];
    let lines: Vec<&str> = text.lines().collect();
    let mut out = Vec::new();
    let group = |line: &str, start: usize| -> Option<[f64; 7]> {
        let mut v = [0.0; 7];
        for (f, slot) in v.iter_mut().enumerate() {
            *slot = field(line, start + f * 6, start + (f + 1) * 6)?;
        }
        Some(v)
    };
    for (i, line) in lines.iter().enumerate() {
        let (Some(gmt), Some(lmt)) = (field(line, 1, 5), field(line, 5, 11)) else {
            continue;
        };
        if !(1.0..=24.0).contains(&gmt) {
            continue;
        }
        let (Some(l1), Some(l2)) = (group(line, GROUPS[0]), group(line, GROUPS[1])) else {
            continue;
        };
        let Some(next) = lines.get(i + 1) else {
            continue;
        };
        if next.len() < 11 || !next[..11].trim().is_empty() {
            continue;
        }
        let (Some(l3), Some(l4)) = (group(next, GROUPS[0]), group(next, GROUPS[1])) else {
            continue;
        };
        out.push(Row {
            gmt,
            lmt,
            layer: [l1, l2, l3, l4],
        });
    }
    out
}
