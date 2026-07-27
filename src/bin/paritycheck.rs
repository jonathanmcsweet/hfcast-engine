//! Does the Rust engine give the server the same numbers the Fortran
//! binary gave it?
//!
//! This is the check that has to pass before the server's Fortran path
//! can be removed. It is narrower than [`portcheck`](../portcheck) on
//! purpose: not every printed cell, but exactly the four fields
//! `server/src/voacap/parse.ts` reads — reliability, SNR and the two
//! SNR deciles — plus the MUF, over the request shapes the server
//! actually sends.
//!
//! Both sides run the whole production chain:
//!
//! - **Fortran**: the deck the server writes, through `voacapl`,
//!   parsed by [`propcore::listing`].
//! - **Rust**: the same request as JSON, through the `predict` binary
//!   as a subprocess, so the JSON boundary is under test too.
//!
//! Usage: `cargo run --release --bin paritycheck -- [--jobs J]`

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use propcore::deck::{build_deck, DeckCase};
use propcore::json::{self, num, obj, str_of, Json};
use propcore::listing::{parse_listing, MUF_ROW, MUF_SLOT};
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};

const VOACAP_VARIANT: &str = "O2";
const CONCURRENCY: usize = 2;

/// The nine amateur bands, ascending, as `server/src/types.ts` lists
/// them and the deck must order them.
const BANDS_MHZ: [f64; 9] = [1.84, 3.75, 7.1, 10.12, 14.2, 18.1, 21.2, 24.94, 28.4];

/// The server's defaults, from `server/src/index.ts`.
const WATTS: f64 = 100.0;
const REQUIRED_SNR_DB: f64 = 24.0;
const NOISE_DBW: f64 = 145.0;

/// One request the server could serve: a real path, a month and a
/// sunspot number. Chosen to span the regimes the model behaves
/// differently in — short and antipodal, equator and high latitude,
/// solar maximum and minimum, solstice and equinox.
struct Case {
    id: &'static str,
    from: (f64, f64, &'static str),
    to: (f64, f64, &'static str),
    month: u32,
    year: u32,
    ssn: f64,
}

const CASES: &[Case] = &[
    Case {
        id: "seattle-tokyo-jun-max",
        from: (47.6062, -122.3321, "SEATTLE"),
        to: (35.6762, 139.6503, "TOKYO"),
        month: 6,
        year: 2025,
        ssn: 124.7,
    },
    Case {
        id: "seattle-tokyo-dec-min",
        from: (47.6062, -122.3321, "SEATTLE"),
        to: (35.6762, 139.6503, "TOKYO"),
        month: 12,
        year: 2019,
        ssn: 4.4,
    },
    Case {
        id: "london-sydney-antipodal",
        from: (51.5072, -0.1276, "LONDON"),
        to: (-33.8688, 151.2093, "SYDNEY"),
        month: 3,
        year: 2015,
        ssn: 54.8,
    },
    Case {
        id: "nairobi-singapore-equator",
        from: (-1.2921, 36.8219, "NAIROBI"),
        to: (1.3521, 103.8198, "SINGAPORE"),
        month: 9,
        year: 2022,
        ssn: 82.0,
    },
    Case {
        id: "reykjavik-tromso-polar-short",
        from: (64.1466, -21.9426, "REYKJAVIK"),
        to: (69.6492, 18.9553, "TROMSO"),
        month: 1,
        year: 2024,
        ssn: 130.0,
    },
    Case {
        id: "santiago-madrid-cross-equator",
        from: (-33.4489, -70.6693, "SANTIAGO"),
        to: (40.4168, -3.7038, "MADRID"),
        month: 7,
        year: 2026,
        ssn: 100.0,
    },
    Case {
        id: "denver-boulder-very-short",
        from: (39.7392, -104.9903, "DENVER"),
        to: (40.015, -105.2705, "BOULDER"),
        month: 4,
        year: 2021,
        ssn: 30.0,
    },
    Case {
        id: "perth-reykjavik-long",
        from: (-31.9523, 115.8613, "PERTH"),
        to: (64.1466, -21.9426, "REYKJAVIK"),
        month: 10,
        year: 2014,
        ssn: 113.0,
    },
];

/// The deck the server writes for a case.
///
/// `server/src/voacap/deck.ts` and `propcore::deck` write the same
/// cards for this shape — isotropes at both ends, sporadic E on,
/// CCIR, method 30 — so building it here rather than shelling out to
/// Node keeps the check to one language without changing the
/// question.
fn deck_case(case: &Case) -> DeckCase {
    DeckCase {
        id: case.from.2.to_string(),
        rx_label: case.to.2.to_string(),
        from_lat: case.from.0,
        from_lon: case.from.1,
        to_lat: case.to.0,
        to_lon: case.to.1,
        method: 30,
        ursi: false,
        month: case.month,
        year: case.year,
        ssn: case.ssn,
        watts: WATTS,
        required_snr_db: REQUIRED_SNR_DB,
        noise_dbw: NOISE_DBW,
        freqs_mhz: BANDS_MHZ.to_vec(),
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        sporadic_e: true,
        fprob: Some([1.0, 1.0, 1.0, 1.0]),
        botlines: None,
        toplines: None,
        krun: 0,
        efvar: Vec::new(),
        esvar: Vec::new(),
        edp: None,
        extra_cards: Vec::new(),
        comment: None,
        integrate: None,
        outgraph: None,
    }
    .as_written()
}

fn request_json(case: &Case, tree: &std::path::Path) -> String {
    obj([
        ("fromLat", num(case.from.0)),
        ("fromLon", num(case.from.1)),
        ("fromLabel", str_of(case.from.2)),
        ("toLat", num(case.to.0)),
        ("toLon", num(case.to.1)),
        ("toLabel", str_of(case.to.2)),
        ("month", num(f64::from(case.month))),
        ("year", num(f64::from(case.year))),
        ("ssn", num(case.ssn)),
        ("watts", num(WATTS)),
        ("requiredSnrDb", num(REQUIRED_SNR_DB)),
        ("noiseDbw", num(NOISE_DBW)),
        (
            "bands",
            Json::Arr(BANDS_MHZ.iter().copied().map(num).collect()),
        ),
        ("itshfbc", str_of(&tree.display().to_string())),
    ])
    .write()
}

/// One value the server reads: `(hour, slot, field)`.
type Fields = BTreeMap<(u8, i8, &'static str), f64>;

/// The fields the server consumes, out of a listing.
fn fields_from_listing(text: &str) -> Fields {
    let parsed = parse_listing(text);
    let mut out = Fields::new();
    for s in &parsed.numeric {
        if s.row == MUF_ROW && s.slot == MUF_SLOT {
            out.insert((s.hour, MUF_SLOT, "muf"), s.value);
            continue;
        }
        let name = match s.row.as_str() {
            "REL" => "reliability",
            "SNR" => "snr",
            "SNR LW" => "snrLowDecile",
            "SNR UP" => "snrUpDecile",
            _ => continue,
        };
        // The server drops slots past the bands it asked for.
        if (s.slot as usize) < BANDS_MHZ.len() {
            out.insert((s.hour, s.slot, name), s.value);
        }
    }
    out
}

/// The same fields out of the `predict` binary's JSON.
fn fields_from_json(text: &str) -> Result<Fields, String> {
    let v = json::parse(text)?;
    if let Some(e) = v.get("error").and_then(Json::as_str) {
        return Err(format!("predict reported: {e}"));
    }
    let mut out = Fields::new();

    let muf = v
        .get("mufByHour")
        .and_then(Json::as_array)
        .ok_or("no mufByHour")?;
    for (hour, value) in muf.iter().enumerate() {
        if let Some(n) = value.as_f64() {
            out.insert((hour as u8, MUF_SLOT, "muf"), n);
        }
    }

    let cells = v.get("cells").and_then(Json::as_array).ok_or("no cells")?;
    for cell in cells {
        let hour = cell.number("hour")? as u8;
        let freq = cell.number("freqMhz")?;
        let slot = BANDS_MHZ
            .iter()
            .position(|b| (b - freq).abs() < 1e-9)
            .ok_or_else(|| format!("cell at {freq} MHz is not a requested band"))?
            as i8;
        out.insert((hour, slot, "reliability"), cell.number("reliability")?);
        out.insert((hour, slot, "snr"), cell.number("snr")?);
        for key in ["snrLowDecile", "snrUpDecile"] {
            // Null means the listing printed no value, which the
            // Fortran side records by having no entry either.
            if let Some(n) = cell.get(key).and_then(Json::as_f64) {
                out.insert((hour, slot, leak(key)), n);
            }
        }
    }
    Ok(out)
}

/// The field names are a fixed set, so a `&'static str` key is honest
/// rather than a leak that grows.
fn leak(key: &str) -> &'static str {
    match key {
        "snrLowDecile" => "snrLowDecile",
        "snrUpDecile" => "snrUpDecile",
        other => panic!("unknown field {other}"),
    }
}

struct Outcome {
    id: &'static str,
    compared: usize,
    differing: Vec<String>,
    failure: Option<String>,
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let jobs = argv
        .iter()
        .position(|a| a == "--jobs")
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(CONCURRENCY)
        .max(1);

    let reference = variant_bin(VOACAP_VARIANT);
    if !reference.is_file() {
        eprintln!("no {VOACAP_VARIANT} variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }
    let predict = predict_binary();
    if !predict.is_file() {
        eprintln!("no predict binary at {}", predict.display());
        eprintln!("build it with: cargo build --release --bin predict");
        return ExitCode::FAILURE;
    }

    eprintln!("comparing {} request shapes through both paths", CASES.len());
    let outcomes = map_limit(CASES, jobs, |case, index| {
        run_case(case, index, &reference, &predict)
    });

    let mut total = 0usize;
    let mut bad = 0usize;
    println!("| case | fields | differing |");
    println!("| --- | --: | --: |");
    for o in &outcomes {
        total += o.compared;
        bad += o.differing.len();
        let note = match &o.failure {
            Some(e) => format!(" ({e})"),
            None => String::new(),
        };
        println!(
            "| {}{} | {} | {} |",
            o.id,
            note,
            o.compared,
            o.differing.len()
        );
    }
    println!();

    let failures: Vec<&Outcome> = outcomes.iter().filter(|o| o.failure.is_some()).collect();
    for o in &outcomes {
        for line in o.differing.iter().take(10) {
            println!("{}: {line}", o.id);
        }
    }

    println!();
    println!("{total} fields compared, {bad} differing.");
    if !failures.is_empty() {
        println!("{} case(s) could not run.", failures.len());
        return ExitCode::FAILURE;
    }
    if bad == 0 {
        println!("Verdict: the Rust engine gives the server the same numbers.");
        ExitCode::SUCCESS
    } else {
        println!("Verdict: the two paths disagree — do not cut over.");
        ExitCode::FAILURE
    }
}

/// The `predict` binary beside this one, whichever profile built it.
fn predict_binary() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join("predict")))
        .unwrap_or_else(|| PathBuf::from("predict"))
}

fn run_case(
    case: &Case,
    index: usize,
    reference: &std::path::Path,
    predict: &std::path::Path,
) -> Outcome {
    let mut out = Outcome {
        id: case.id,
        compared: 0,
        differing: Vec::new(),
        failure: None,
    };

    let root = match IsolatedRoot::create(&format!("parity{index}")) {
        Ok(r) => r,
        Err(e) => {
            out.failure = Some(format!("tree: {e}"));
            return out;
        }
    };

    let deck = match build_deck(&deck_case(case)) {
        Ok(d) => d,
        Err(e) => {
            out.failure = Some(format!("deck: {e}"));
            return out;
        }
    };
    let listing = match run_deck(reference, root.path(), &deck) {
        Ok(t) => t,
        Err(e) => {
            out.failure = Some(format!("voacapl: {e}"));
            return out;
        }
    };
    let fortran = fields_from_listing(&listing);

    let rust = match run_predict(predict, &request_json(case, root.path())) {
        Ok(text) => match fields_from_json(&text) {
            Ok(f) => f,
            Err(e) => {
                out.failure = Some(e);
                return out;
            }
        },
        Err(e) => {
            out.failure = Some(e);
            return out;
        }
    };

    let mut keys: Vec<_> = fortran.keys().copied().collect();
    keys.extend(rust.keys().copied());
    keys.sort_unstable();
    keys.dedup();

    for key in keys {
        out.compared += 1;
        let (hour, slot, field) = key;
        let band = if slot == MUF_SLOT {
            "MUF".to_string()
        } else {
            format!("{} MHz", BANDS_MHZ[slot as usize])
        };
        match (fortran.get(&key), rust.get(&key)) {
            (Some(a), Some(b)) if a == b => {}
            (Some(a), Some(b)) => out.differing.push(format!(
                "{hour:02}z {band} {field}: fortran {a}, rust {b}"
            )),
            (Some(a), None) => out
                .differing
                .push(format!("{hour:02}z {band} {field}: fortran {a}, rust absent")),
            (None, Some(b)) => out
                .differing
                .push(format!("{hour:02}z {band} {field}: fortran absent, rust {b}")),
            (None, None) => {}
        }
    }
    out
}

fn run_predict(bin: &std::path::Path, request: &str) -> Result<String, String> {
    let mut child = Command::new(bin)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("spawn predict: {e}"))?;
    child
        .stdin
        .as_mut()
        .ok_or("no stdin")?
        .write_all(request.as_bytes())
        .map_err(|e| format!("write to predict: {e}"))?;
    let done = child
        .wait_with_output()
        .map_err(|e| format!("predict: {e}"))?;
    let text = String::from_utf8_lossy(&done.stdout).to_string();
    if !done.status.success() {
        return Err(format!(
            "predict exited {}: {}",
            done.status,
            String::from_utf8_lossy(&done.stderr).trim()
        ));
    }
    Ok(text)
}
