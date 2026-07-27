//! Does the Rust engine give a calling application the same numbers
//! the Fortran binary gave it?
//!
//! This is the check that has to pass before an application can drop
//! its Fortran path. It is narrower than [`portcheck`](../portcheck)
//! on purpose: not every printed cell, but exactly the four fields an
//! application reads out of a listing — reliability, SNR and the two
//! SNR deciles — plus the MUF, over the request shapes it sends.
//!
//! Both sides run the whole production chain:
//!
//! - **Fortran**: the deck an application writes, through `voacapl`,
//!   parsed by [`hfcast::listing`].
//! - **Rust**: the same request as JSON, through the `predict` binary
//!   as a subprocess, so the JSON boundary is under test too.
//!
//! Usage: `cargo run --release --bin paritycheck -- [--jobs J]`
//!
//! For a live soak, `--paths FILE --month M --year Y --ssn S` runs a
//! path list against one day's inputs, and `--dump DIR` writes the
//! deck and both outputs for any case that disagrees.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::process::{Command, ExitCode, Stdio};

use hfcast::deck::{build_deck, DeckCase};
use hfcast::json::{self, num, obj, str_of, Json};
use hfcast::listing::{parse_listing, MUF_ROW, MUF_SLOT};
use hfcast::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};

const VOACAP_VARIANT: &str = "O2";
const CONCURRENCY: usize = 2;

/// The nine amateur bands, ascending, which is the order the deck
/// must list them in.
const BANDS_MHZ: [f64; 9] = [1.84, 3.75, 7.1, 10.12, 14.2, 18.1, 21.2, 24.94, 28.4];

/// The defaults a calling application sends when it says nothing.
const WATTS: f64 = 100.0;
const REQUIRED_SNR_DB: f64 = 24.0;
const NOISE_DBW: f64 = 145.0;

/// One request an application could serve: a real path, a month and a
/// sunspot number. Chosen to span the regimes the model behaves
/// differently in — short and antipodal, equator and high latitude,
/// solar maximum and minimum, solstice and equinox.
struct Case {
    id: String,
    from: (f64, f64, String),
    to: (f64, f64, String),
    month: u32,
    year: u32,
    ssn: f64,
}

/// The same, as a compile-time constant, so the built-in set needs no
/// allocation to declare.
struct StaticCase {
    id: &'static str,
    from: (f64, f64, &'static str),
    to: (f64, f64, &'static str),
    month: u32,
    year: u32,
    ssn: f64,
}

impl StaticCase {
    fn owned(&self) -> Case {
        Case {
            id: self.id.to_string(),
            from: (self.from.0, self.from.1, self.from.2.to_string()),
            to: (self.to.0, self.to.1, self.to.2.to_string()),
            month: self.month,
            year: self.year,
            ssn: self.ssn,
        }
    }
}

const CASES: &[StaticCase] = &[
    StaticCase {
        id: "seattle-tokyo-jun-max",
        from: (47.6062, -122.3321, "SEATTLE"),
        to: (35.6762, 139.6503, "TOKYO"),
        month: 6,
        year: 2025,
        ssn: 124.7,
    },
    StaticCase {
        id: "seattle-tokyo-dec-min",
        from: (47.6062, -122.3321, "SEATTLE"),
        to: (35.6762, 139.6503, "TOKYO"),
        month: 12,
        year: 2019,
        ssn: 4.4,
    },
    StaticCase {
        id: "london-sydney-antipodal",
        from: (51.5072, -0.1276, "LONDON"),
        to: (-33.8688, 151.2093, "SYDNEY"),
        month: 3,
        year: 2015,
        ssn: 54.8,
    },
    StaticCase {
        id: "nairobi-singapore-equator",
        from: (-1.2921, 36.8219, "NAIROBI"),
        to: (1.3521, 103.8198, "SINGAPORE"),
        month: 9,
        year: 2022,
        ssn: 82.0,
    },
    StaticCase {
        id: "reykjavik-tromso-polar-short",
        from: (64.1466, -21.9426, "REYKJAVIK"),
        to: (69.6492, 18.9553, "TROMSO"),
        month: 1,
        year: 2024,
        ssn: 130.0,
    },
    StaticCase {
        id: "santiago-madrid-cross-equator",
        from: (-33.4489, -70.6693, "SANTIAGO"),
        to: (40.4168, -3.7038, "MADRID"),
        month: 7,
        year: 2026,
        ssn: 100.0,
    },
    StaticCase {
        id: "denver-boulder-very-short",
        from: (39.7392, -104.9903, "DENVER"),
        to: (40.015, -105.2705, "BOULDER"),
        month: 4,
        year: 2021,
        ssn: 30.0,
    },
    StaticCase {
        id: "perth-reykjavik-long",
        from: (-31.9523, 115.8613, "PERTH"),
        to: (64.1466, -21.9426, "REYKJAVIK"),
        month: 10,
        year: 2014,
        ssn: 113.0,
    },
];

/// The deck a calling application writes for a case: isotropes at
/// both ends, sporadic E on, CCIR, method 30.
fn deck_case(case: &Case) -> DeckCase {
    DeckCase {
        id: case.from.2.clone(),
        rx_label: case.to.2.clone(),
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
        ("fromLabel", str_of(&case.from.2)),
        ("toLat", num(case.to.0)),
        ("toLon", num(case.to.1)),
        ("toLabel", str_of(&case.to.2)),
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

/// One value an application reads: `(hour, slot, field)`.
type Fields = BTreeMap<(u8, i8, &'static str), f64>;

/// The fields an application consumes, out of a listing.
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
        // A caller drops slots past the bands it asked for.
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
    id: String,
    compared: usize,
    differing: Vec<String>,
    failure: Option<String>,
}

/// Reads a path list: one case per line, tab-separated, as
/// `id  tx_lat  tx_lon  TX_LABEL  rx_lat  rx_lon  RX_LABEL`. Blank
/// lines and lines starting with `#` are skipped.
///
/// The month, year and sunspot number come from the command line
/// rather than the file, because the point of a path list is to hold
/// the geography still while the live inputs move.
fn load_paths(file: &std::path::Path, month: u32, year: u32, ssn: f64) -> Result<Vec<Case>, String> {
    let text = std::fs::read_to_string(file).map_err(|e| format!("{}: {e}", file.display()))?;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let f: Vec<&str> = line.split('\t').map(str::trim).collect();
        if f.len() != 7 {
            return Err(format!(
                "{}:{}: expected 7 tab-separated fields, found {}",
                file.display(),
                n + 1,
                f.len()
            ));
        }
        let numeric = |i: usize| -> Result<f64, String> {
            f[i].parse::<f64>()
                .map_err(|_| format!("{}:{}: {:?} is not a number", file.display(), n + 1, f[i]))
        };
        out.push(Case {
            id: f[0].to_string(),
            from: (numeric(1)?, numeric(2)?, f[3].to_string()),
            to: (numeric(4)?, numeric(5)?, f[6].to_string()),
            month,
            year,
            ssn,
        });
    }
    if out.is_empty() {
        return Err(format!("{}: no cases", file.display()));
    }
    Ok(out)
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
    let dump = flag("--dump").map(PathBuf::from);

    // A path list holds the geography still; the month, year and
    // sunspot number are what a soak run varies, so all three must be
    // given together and none of them has a default. Guessing one
    // would produce a run that looks live and is not.
    let cases: Vec<Case> = match flag("--paths") {
        Some(file) => {
            let (month, year, ssn) = match (flag("--month"), flag("--year"), flag("--ssn")) {
                (Some(m), Some(y), Some(s)) => {
                    match (m.parse::<u32>(), y.parse::<u32>(), s.parse::<f64>()) {
                        (Ok(m), Ok(y), Ok(s)) => (m, y, s),
                        _ => {
                            eprintln!("--month, --year and --ssn must be numbers");
                            return ExitCode::FAILURE;
                        }
                    }
                }
                _ => {
                    eprintln!("--paths needs --month, --year and --ssn as well");
                    return ExitCode::FAILURE;
                }
            };
            match load_paths(&PathBuf::from(file), month, year, ssn) {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::FAILURE;
                }
            }
        }
        None => CASES.iter().map(StaticCase::owned).collect(),
    };

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
    if let Some(dir) = &dump {
        if let Err(e) = std::fs::create_dir_all(dir) {
            eprintln!("{}: {e}", dir.display());
            return ExitCode::FAILURE;
        }
    }

    eprintln!("comparing {} request shapes through both paths", cases.len());
    let outcomes = map_limit(&cases, jobs, |case, index| {
        run_case(case, index, &reference, &predict, dump.as_deref())
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
        println!("Verdict: the Rust engine gives the same numbers.");
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
    dump: Option<&std::path::Path>,
) -> Outcome {
    let mut out = Outcome {
        id: case.id.clone(),
        compared: 0,
        differing: Vec::new(),
        failure: None,
    };

    // Kept so a case that disagrees can be written out whole. A
    // difference is only useful if it reproduces, and it reproduces
    // from the deck and the two outputs, not from a summary line.
    let mut artifacts: Vec<(&str, String)> = Vec::new();

    let root = match IsolatedRoot::create(&format!("parity{index}")) {
        Ok(r) => r,
        Err(e) => {
            out.failure = Some(format!("tree: {e}"));
            write_dump(dump, case, &out, &artifacts);
            return out;
        }
    };

    let deck = match build_deck(&deck_case(case)) {
        Ok(d) => d,
        Err(e) => {
            out.failure = Some(format!("deck: {e}"));
            write_dump(dump, case, &out, &artifacts);
            return out;
        }
    };
    artifacts.push(("deck.txt", deck.clone()));

    let listing = match run_deck(reference, root.path(), &deck) {
        Ok(t) => t,
        Err(e) => {
            out.failure = Some(format!("voacapl: {e}"));
            write_dump(dump, case, &out, &artifacts);
            return out;
        }
    };
    artifacts.push(("fortran.txt", listing.clone()));
    let fortran = fields_from_listing(&listing);

    let request = request_json(case, root.path());
    artifacts.push(("request.json", request.clone()));
    let rust = match run_predict(predict, &request) {
        Ok(text) => {
            artifacts.push(("rust.json", text.clone()));
            match fields_from_json(&text) {
                Ok(f) => f,
                Err(e) => {
                    out.failure = Some(e);
                    write_dump(dump, case, &out, &artifacts);
                    return out;
                }
            }
        }
        Err(e) => {
            out.failure = Some(e);
            write_dump(dump, case, &out, &artifacts);
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
    write_dump(dump, case, &out, &artifacts);
    out
}

/// Writes everything a disagreeing case needs to be reproduced, into
/// `<dir>/<case id>/`. Silent for a case that agreed: a clean soak
/// should leave nothing behind.
fn write_dump(
    dir: Option<&std::path::Path>,
    case: &Case,
    out: &Outcome,
    artifacts: &[(&str, String)],
) {
    let Some(dir) = dir else { return };
    if out.differing.is_empty() && out.failure.is_none() {
        return;
    }
    let case_dir = dir.join(&case.id);
    if std::fs::create_dir_all(&case_dir).is_err() {
        return;
    }
    let mut report = format!(
        "case: {}\nfrom: {} {} {}\nto: {} {} {}\nmonth: {}\nyear: {}\nssn: {}\n",
        case.id,
        case.from.0,
        case.from.1,
        case.from.2,
        case.to.0,
        case.to.1,
        case.to.2,
        case.month,
        case.year,
        case.ssn,
    );
    if let Some(e) = &out.failure {
        report.push_str(&format!("failure: {e}\n"));
    }
    for line in &out.differing {
        report.push_str(&format!("differs: {line}\n"));
    }
    let _ = std::fs::write(case_dir.join("case.txt"), report);
    for (name, body) in artifacts {
        let _ = std::fs::write(case_dir.join(name), body);
    }
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
