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
//! [`hfcast::listing`], which makes the values identical to the
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

use hfcast::api::{listing, FoF2Model, Ionosphere, Model, Request, Site, Task};
use hfcast::json::{self, num, obj, str_of, Json};
use hfcast::listing::{parse_listing, parse_muf_table, MUF_ROW, MUF_SLOT};
use hfcast::runner::itshfbc_dir;
use hfcast::voacap::area::{Grid, Projection};
use hfcast::voacap::run::{run_area, AreaInputs};

/// The rows the server reads, and the key each becomes in the output.
///
/// `TANGLE` is the transmit take-off angle in degrees. It is here because
/// near-vertical incidence is a property of the angle and nothing else:
/// on a short path the energy leaves steeply, comes back down without a
/// skip zone, and a band well below the MUF works for reasons a
/// reliability figure alone does not explain. Deriving that from distance
/// instead would be inventing a threshold the engine already computes.
const ROWS: [(&str, &str); 5] = [
    ("REL", "reliability"),
    ("SNR", "snr"),
    ("SNR LW", "snrLowDecile"),
    ("SNR UP", "snrUpDecile"),
    ("TANGLE", "takeoffAngleDeg"),
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
    let tree = match req.get("itshfbc").and_then(Json::as_str) {
        Some(path) => PathBuf::from(path),
        None => itshfbc_dir(),
    };

    // Two shapes of answer from one binary. A point-to-point run is the
    // default and stays the bare object it has always been, so nothing
    // that already calls this has to learn a new field.
    if req.get("mode").and_then(Json::as_str) == Some("area") {
        return Ok(area(&tree, &req)?.write());
    }

    let (request, freqs) = build_request(&req)?;
    let text = listing(&tree, &request, Task::Systems)?;
    let mut out = prediction(&text, &freqs);

    // A second run for the operating window. Method 30 prints neither the
    // LUF nor the FOT — `NUMERIC_ROWS` is the full set of rows it has —
    // so there is no way to derive them from the listing above. Method 26
    // is the one that runs the LUF search.
    //
    // It costs a second full pass over the 24 hours, which measures in
    // milliseconds against a server that caches per path, month and SSN.
    let window = listing(&tree, &request, Task::Luf)?;
    if let Json::Obj(fields) = &mut out {
        for (key, value) in operating_window(&window) {
            fields.insert(key.to_string(), value);
        }
    }
    Ok(out.write())
}

/// `OUTAREA`'s reliability column, in the single-frequency row.
///
/// One frequency prints twenty-four fields and several print seven, and
/// reliability sits at a different index in each. An area run here always
/// asks for one frequency, because the map colours one band at a time —
/// several would return the maximum over them, which saturates and says
/// nothing about the band the user chose.
const AREA_RELIABILITY_FIELD: usize = 12;

/// Coverage: one band, one hour, every direction.
///
/// Answers "where can I be heard", which the point-to-point run cannot —
/// that one already knows where the other end is. The grid is in degrees
/// rather than kilometres so the cells tile the whole globe without a
/// projection choice being baked into the numbers; the app draws them
/// through whatever projection it likes.
///
/// Points sit at cell *centres*, half a step in from each edge, so a cell
/// drawn around its point covers exactly its share of the sphere. Putting
/// them on the edges would double-count the poles and leave a seam at the
/// antimeridian.
fn area(tree: &std::path::Path, req: &Json) -> Result<Json, String> {
    let lat_step = req.number("latStep")?;
    let lon_step = req.number("lonStep")?;
    if lat_step <= 0.0 || lon_step <= 0.0 {
        return Err("\"latStep\" and \"lonStep\" must be positive".into());
    }
    let ny = (180.0 / lat_step).round() as usize;
    let nx = (360.0 / lon_step).round() as usize;
    if ny < 2 || nx < 2 {
        return Err("the grid needs at least two points on each side".into());
    }

    let grid = Grid {
        projection: Projection::LatLon,
        plat: req.number("fromLat")? as f32,
        plon: req.number("fromLon")? as f32,
        xmin: (-180.0 + lon_step / 2.0) as f32,
        xmax: (180.0 - lon_step / 2.0) as f32,
        ymin: (-90.0 + lat_step / 2.0) as f32,
        ymax: (90.0 - lat_step / 2.0) as f32,
        nx,
        ny,
    };

    let inputs = AreaInputs {
        grid,
        tx_lat_deg: req.number("fromLat")?,
        tx_lon_deg: req.number("fromLon")?,
        month: req.number("month")? as u32,
        ssn: req.number("ssn")? as f32,
        // `AreaInputs.hour` is the hour the input file names, 1 to 24,
        // while every other interface here counts 0 to 23.
        hour: req.number("hour")? as i32 + 1,
        freqs_mhz: vec![req.number("freqMhz")? as f32],
        required_snr_db: req.number("requiredSnrDb")? as f32,
        noise_dbw: req.number("noiseDbw")? as i32,
        watts: req.number("watts")? as f32,
        psc: [1.0, 1.0, 1.0, 0.0],
        method: 30,
        fof2: FoF2Model::Ccir,
        inverse: false,
        tx_antenna: None,
        rx_antenna: None,
        model: Model::Compatible,
    };

    let points = run_area(tree, &inputs)?;
    let mut out = Vec::with_capacity(points.len());
    for p in points {
        let field = p
            .fields
            .get(AREA_RELIABILITY_FIELD)
            .ok_or("area row is missing its reliability field")?;
        // The field is the reference's own formatting of the number, and
        // asterisks are how Fortran reports one too wide for its column.
        let reliability = field.trim().parse::<f64>().unwrap_or(0.0);
        // Longitudes come back folded into 0..360; the app works in
        // -180..180 like every other coordinate it handles.
        let lon = if p.lon > 180.0 {
            f64::from(p.lon) - 360.0
        } else {
            f64::from(p.lon)
        };
        out.push(obj([
            ("lat", num(f64::from(p.lat))),
            ("lon", num(lon)),
            ("reliability", num(reliability.clamp(0.0, 1.0))),
        ]));
    }

    Ok(obj([
        ("latStep", num(lat_step)),
        ("lonStep", num(lon_step)),
        ("points", Json::Arr(out)),
    ]))
}

/// The frequency window, as three arrays indexed by hour.
///
/// Shaped to match `mufByHour`, which the server already reads, rather
/// than as a list of objects: every consumer wants one curve at a time.
/// The MUF is not repeated here — method 26 prints it too, and the two
/// agree, but one name for one number keeps the contract unambiguous.
///
/// An hour the table did not print stays null rather than becoming zero,
/// and so does a negative LUF, which means the search found no frequency
/// meeting the required reliability rather than a very low one. Zero is a
/// frequency; absent is not.
fn operating_window(text: &str) -> [(&'static str, Json); 3] {
    let mut fot = vec![Json::Null; 24];
    let mut hpf = vec![Json::Null; 24];
    let mut luf = vec![Json::Null; 24];

    for row in parse_muf_table(text) {
        let hour = row.hour() as usize;
        let positive = |v: f64| if v > 0.0 { num(v) } else { Json::Null };
        fot[hour] = positive(row.fot);
        hpf[hour] = positive(row.hpf);
        luf[hour] = positive(row.luf);
    }

    [
        ("fotByHour", Json::Arr(fot)),
        ("hpfByHour", Json::Arr(hpf)),
        ("lufByHour", Json::Arr(luf)),
    ]
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
        // validation put it on; see docs/accuracy.md.
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
