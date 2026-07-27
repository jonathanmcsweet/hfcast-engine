//! The prediction the server consumes, as JSON on stdout.
//!
//! Reads one request object from stdin and writes one prediction
//! object to stdout. This is the whole interface between the
//! TypeScript server and the Rust engine: a process boundary rather
//! than a binding, because it is the least machinery that removes the
//! Fortran toolchain from the deployment.
//!
//! ```text
//! echo '{"fromLat":47.6,...}' | predict
//! ```
//!
//! ## Why it renders a listing and reads it back
//!
//! The server has always consumed *printed* values: reliability to
//! two decimals, SNR to the nearest dB, the deciles to one. Its
//! correction factors were fitted against those numbers. So this
//! renders the listing with the verified formatter and parses it with
//! [`propcore::listing`], which makes the values identical to the
//! reference's by construction rather than by a second implementation
//! of `OUTBOD`'s rounding, its at-the-MUF column and its rule for
//! which frequency slots print at all.
//!
//! The raw `f32` values are richer and are what a later tier should
//! use. Reaching for them is a deliberate change to measure, not a
//! side effect of moving off Fortran, so it is not done here.

use std::collections::BTreeMap;
use std::io::{self, Read, Write};
use std::path::PathBuf;
use std::process::ExitCode;

use propcore::api::{listing, FoF2Model, Ionosphere, Model, Request, Site, Task};
use propcore::json::{self, num, obj, str_of, Json};
use propcore::listing::{parse_listing, MUF_ROW, MUF_SLOT};
use propcore::runner::itshfbc_dir;

/// The rows the server reads, and the key each becomes in the output.
const ROWS: [(&str, &str); 4] = [
    ("REL", "reliability"),
    ("SNR", "snr"),
    ("SNR LW", "snrLowDecile"),
    ("SNR UP", "snrUpDecile"),
];

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        return fail(&format!("could not read stdin: {e}"));
    }
    match run(&input) {
        Ok(text) => {
            let mut out = io::stdout().lock();
            if writeln!(out, "{text}").is_err() {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

/// Errors go to stdout as JSON too, so the caller has one thing to
/// parse whatever happened, and to stderr for a human reading logs.
fn fail(message: &str) -> ExitCode {
    let body = obj([("error", str_of(message))]).write();
    println!("{body}");
    eprintln!("predict: {message}");
    ExitCode::FAILURE
}

fn run(input: &str) -> Result<String, String> {
    let req = json::parse(input)?;
    let (request, freqs) = build_request(&req)?;
    let tree = match req.get("itshfbc").and_then(Json::as_str) {
        Some(path) => PathBuf::from(path),
        None => itshfbc_dir(),
    };

    let text = listing(&tree, &request, Task::Systems)?;
    Ok(prediction(&text, &freqs).write())
}

/// The request, and the frequencies in the slot order the listing
/// prints them, so a cell can be labelled by the band that asked for
/// it.
fn build_request(req: &Json) -> Result<(Request, Vec<f64>), String> {
    let bands = req
        .get("bands")
        .and_then(Json::as_array)
        .ok_or("field \"bands\" must be an array")?;
    let mut freqs = Vec::with_capacity(bands.len());
    for band in bands {
        freqs.push(
            band.as_f64()
                .ok_or("every entry in \"bands\" must be a frequency in MHz")?,
        );
    }

    let request = Request {
        tx: Site {
            name: req.string("fromLabel").unwrap_or_default(),
            lat_deg: req.number("fromLat")?,
            lon_deg: req.number("fromLon")?,
        },
        rx: Site {
            name: req.string("toLabel").unwrap_or_default(),
            lat_deg: req.number("toLat")?,
            lon_deg: req.number("toLon")?,
        },
        month: req.number("month")? as u32,
        year: req.number("year")? as u32,
        ssn: req.number("ssn")?,
        power_watts: req.number("watts")?,
        freqs_mhz: freqs.clone(),
        required_snr_db: req.number("requiredSnrDb")?,
        noise_dbw: req.number("noiseDbw")?,
        fof2: FoF2Model::Ccir,
        // The fourth value enables sporadic E. Eight months of WSPR
        // validation put it on; see propcore/docs/accuracy.md.
        layer_multipliers: [1.0, 1.0, 1.0, 1.0],
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        ionosphere: Ionosphere::default(),
        // `"model": "corrected"` asks for VOACAP with its documented
        // defects fixed. Absent, a request gets the behaviour proven
        // identical to the reference, which is what the server wants
        // and what every harness can judge.
        model: match req.get("model").and_then(Json::as_str) {
            None | Some("compatible") => Model::Compatible,
            Some("corrected") => Model::Corrected,
            Some(other) => return Err(format!("unknown model {other:?}")),
        },
    };
    Ok((request, freqs))
}

/// Reshapes the parsed listing into the object the server reads.
fn prediction(text: &str, freqs: &[f64]) -> Json {
    let parsed = parse_listing(text);

    // (hour, row) -> slot -> value, so a cell can ask for the four
    // rows it needs without walking the samples four times.
    let mut by_row: BTreeMap<(u8, &str), BTreeMap<i8, f64>> = BTreeMap::new();
    let mut muf = [0.0f64; 24];
    for s in &parsed.numeric {
        if s.row == MUF_ROW && s.slot == MUF_SLOT {
            muf[s.hour as usize] = s.value;
            continue;
        }
        by_row
            .entry((s.hour, s.row.as_str()))
            .or_default()
            .insert(s.slot, s.value);
    }

    let mut cells = Vec::new();
    for hour in 0..24u8 {
        for (slot, freq) in freqs.iter().enumerate() {
            let at = |row: &str| -> Option<f64> {
                by_row.get(&(hour, row)).and_then(|m| m.get(&(slot as i8))).copied()
            };
            // The server drops a cell without both of these, because
            // the listing prints no slot past the last with a mode.
            let (Some(rel), Some(snr)) = (at("REL"), at("SNR")) else {
                continue;
            };
            let mut fields = vec![
                ("hour", num(f64::from(hour))),
                ("freqMhz", num(*freq)),
                // Reliability is a probability however the listing
                // rounded it.
                ("reliability", num(rel.clamp(0.0, 1.0))),
                ("snr", num(snr)),
            ];
            for (row, key) in ROWS {
                if key == "reliability" || key == "snr" {
                    continue;
                }
                fields.push((key, at(row).map(num).unwrap_or(Json::Null)));
            }
            cells.push(Json::Obj(
                fields
                    .into_iter()
                    .map(|(k, v)| (k.to_string(), v))
                    .collect::<BTreeMap<_, _>>(),
            ));
        }
    }

    obj([
        ("mufByHour", Json::Arr(muf.iter().copied().map(num).collect())),
        ("cells", Json::Arr(cells)),
    ])
}
