//! One prediction, requested and answered as JSON.
//!
//! This is the whole interface between an application and the engine.
//! The server reaches it through the `predict` binary over a pipe; an
//! application with the engine compiled in calls [`run`] directly. Both
//! go through this one function, so the two cannot drift — a second
//! implementation of the request shape is exactly the kind of thing
//! that disagrees quietly.
//!
//! The request may name its data root in an `itshfbc` field, which is
//! how a caller with no tree asks for the compiled-in files:
//! `"itshfbc": "<embedded>"`, or `"<embedded>+/some/cache"` to have one
//! directory searched first. See [`crate::voacap::data`].
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
use std::path::PathBuf;

use crate::api::{listing, FoF2Model, Ionosphere, Model, Request, Site, Task};
use crate::deck::AntennaChoice;
use crate::json::{self, num, obj, Json};
use crate::listing::{parse_listing, parse_muf_table, MUF_ROW, MUF_SLOT};
use crate::runner::itshfbc_dir;
use crate::voacap::area::{Grid, Projection};
use crate::voacap::con::R;
use crate::voacap::run::{run_area, AntennaCardSpec, AreaInputs};

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

pub fn run(input: &str) -> Result<String, String> {
    let req = json::parse(input)?;
    let tree = match req.get("itshfbc").and_then(Json::as_str) {
        Some(path) => PathBuf::from(path),
        None => itshfbc_dir(),
    };

    // Two shapes of answer from one binary. A point-to-point run is the
    // default and stays the bare object it has always been, so nothing
    // that already calls this has to learn a new field.
    if req.get("mode").and_then(Json::as_str) == Some("area") {
        let tree_json = area(&tree, &req)?;
        let _perf = crate::perf::Step::new(crate::perf::WRITE);
        return Ok(tree_json.write());
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

/// One `txAntenna` or `rxAntenna` object as the request states it.
///
/// The card's last field is kept optional rather than defaulted here,
/// because the two paths below disagree about what it means. A
/// point-to-point deck leaves it empty and writes the case's own power;
/// an area run has already turned the power into kilowatts and puts it
/// in this field, so defaulting it to zero there would predict a
/// transmitter with no power.
struct AntennaRequest {
    file: String,
    design_freq: f64,
    beam_deg: f64,
    min_freq: i32,
    max_freq: i32,
    last_field: Option<f64>,
}

/// Reads one antenna object, if the request has it.
///
/// Absent, the caller gets the isotrope both paths used before antennas
/// were wired in, so a request written against the older interface
/// predicts exactly what it always did.
///
/// `file` is a path under `<itshfbc>/antennas` and is the only required
/// field. The rest carry the card's defaults: the whole 2 to 30 MHz
/// range, no design frequency, and a beam pointing at 0 degrees, which
/// only the directional families read.
fn antenna(req: &Json, key: &str) -> Result<Option<AntennaRequest>, String> {
    let Some(spec) = req.get(key) else {
        return Ok(None);
    };
    let file = spec
        .get("file")
        .and_then(Json::as_str)
        .ok_or(format!("\"{key}\" must have a \"file\""))?;
    if file.trim().is_empty() {
        return Err(format!("\"{key}.file\" must name an antenna"));
    }
    // The card holds the path in 21 columns, so a longer one would be
    // truncated into the name of a file that does not exist.
    if file.chars().count() > 21 {
        return Err(format!(
            "\"{key}.file\" is longer than the card's 21 columns"
        ));
    }
    let number = |field: &str, fallback: f64| -> f64 {
        spec.get(field).and_then(Json::as_f64).unwrap_or(fallback)
    };
    let min_freq = number("minFreq", 2.0) as i32;
    let max_freq = number("maxFreq", 30.0) as i32;
    if min_freq > max_freq {
        return Err(format!("\"{key}.minFreq\" is above its \"maxFreq\""));
    }
    Ok(Some(AntennaRequest {
        file: file.to_string(),
        design_freq: number("designFreq", 0.0),
        beam_deg: number("beamDeg", 0.0),
        min_freq,
        max_freq,
        last_field: spec.get("powerField").and_then(Json::as_f64),
    }))
}

impl AntennaRequest {
    /// The point-to-point form. `None` in the last field leaves the deck
    /// to write the case's own power.
    fn choice(self) -> AntennaChoice {
        AntennaChoice {
            file: self.file,
            design_freq: self.design_freq,
            beam_deg: self.beam_deg,
            min_freq: self.min_freq,
            max_freq: self.max_freq,
            last_field: self.last_field,
        }
    }

    /// The area form, where the last field is already a number the run
    /// reads: kilowatts on the transmit card, a gain on the receive one.
    fn card(self, default_last_field: f64) -> AntennaCardSpec {
        AntennaCardSpec {
            file: self.file,
            design_freq: self.design_freq as R,
            beam_deg: self.beam_deg as R,
            min_freq: self.min_freq,
            max_freq: self.max_freq,
            power_field: self.last_field.unwrap_or(default_last_field) as R,
        }
    }
}

/// `OUTAREA`'s reliability column, in the single-frequency row.
///
/// One frequency prints twenty-four fields and several print seven, and
/// reliability sits at a different index in each. An area run here always
/// asks for one frequency, because the map colours one band at a time —
/// several would return the maximum over them, which saturates and says
/// nothing about the band the user chose.
const AREA_RELIABILITY_FIELD: usize = 12;

/// `OUTAREA`'s take-off angle column, in the same row.
///
/// The same quantity the point-to-point listing prints as `TANGLE`, and
/// the reason it is worth carrying: near-vertical incidence is a property
/// of the angle alone. A short path leaves steeply and comes back down
/// without a skip zone, which is why a low band works at midday over a few
/// hundred kilometres, and a reliability figure on its own does not say
/// so. Reading it here means a map can tell that region from a long
/// low-angle hop instead of inferring one from distance.
const AREA_TAKEOFF_FIELD: usize = 2;

/// The rectangle a grid covers, in degrees.
///
/// The whole world unless the request names all four edges. The default is
/// what every caller written before this asks for, so it is the value the
/// reader of this file should assume.
#[derive(Debug)]
struct Bounds {
    lat_min: f64,
    lat_max: f64,
    lon_min: f64,
    lon_max: f64,
}

/// The rectangle the request asks for, or `None` for the whole world.
///
/// All four edges together or none of them. A request naming only
/// `latMin` would otherwise be answered over every longitude, which is a
/// far larger and slower answer than it asked for and arrives looking like
/// a correct one.
fn bounds(req: &Json) -> Result<Option<Bounds>, String> {
    const KEYS: [&str; 4] = ["latMin", "latMax", "lonMin", "lonMax"];
    let stated = KEYS.iter().filter(|key| req.get(key).is_some()).count();
    if stated == 0 {
        return Ok(None);
    }
    if stated < KEYS.len() {
        return Err(format!(
            "an area request states all of {} or none of them",
            KEYS.join(", ")
        ));
    }
    let box_ = Bounds {
        lat_min: req.number("latMin")?,
        lat_max: req.number("latMax")?,
        lon_min: req.number("lonMin")?,
        lon_max: req.number("lonMax")?,
    };
    if box_.lat_min < -90.0 || box_.lat_max > 90.0 {
        return Err("\"latMin\" and \"latMax\" are degrees, -90 to 90".into());
    }
    if box_.lon_min < -180.0 || box_.lon_max > 180.0 {
        return Err("\"lonMin\" and \"lonMax\" are degrees, -180 to 180".into());
    }
    if box_.lat_min >= box_.lat_max {
        return Err("\"latMin\" must be below \"latMax\"".into());
    }
    if box_.lon_min >= box_.lon_max {
        // A rectangle across the antimeridian is written this way round —
        // 170 to -170 — and is refused rather than guessed at. The grid
        // counts its points eastward from `lonMin`, so reading it as a
        // crossing would need the counting to change; asking for the two
        // halves separately gives the same cells with nothing implied.
        return Err(
            "\"lonMin\" must be below \"lonMax\": a rectangle crossing the \
             antimeridian has to be asked for as two"
                .into(),
        );
    }
    Ok(Some(box_))
}

/// One axis of the grid: its first point, its last, and how many.
#[derive(Debug)]
struct Axis {
    min: R,
    max: R,
    n: usize,
}

/// One axis of the whole-world grid, written as it has always been
/// written.
///
/// `edge` is where the world starts on this axis and `span` how far it
/// runs: -90 and 180 for latitude, -180 and 360 for longitude.
///
/// Kept as its own two expressions rather than as the bounded form below
/// with the world for a rectangle, because the two agree only where the
/// step divides the world evenly. At a step of 7 degrees this puts 26
/// points between -86.5 and 86.5, which are 6.92 degrees apart — the
/// inset is half of the step asked for and the spacing is not, so it is
/// not a lattice of that step at all. Every step the callers use does
/// divide evenly, so nothing meets the difference; reproducing it exactly
/// costs four lines and removes the question.
fn world_axis(step: f64, edge: f64, span: f64) -> Result<Axis, String> {
    let n = (span / step).round() as usize;
    if n < 2 {
        return Err("the grid needs at least two points on each side".into());
    }
    Ok(Axis {
        min: (edge + step / 2.0) as R,
        max: (edge + span - step / 2.0) as R,
        n,
    })
}

/// The cell centres on one axis that fall inside `lo` to `hi`.
///
/// The world is divided into bands of the step asked for — or of the
/// nearest width that divides it evenly, so the bands tile — and the
/// points are the centres of the bands inside the rectangle. A bounded
/// grid is therefore a window on the same lattice the whole-world run
/// uses rather than a second grid beside it: its cells line up with the
/// coarse ones under them, and two rectangles side by side join with no
/// seam and no overlap.
fn part_axis(
    lo: f64,
    hi: f64,
    step: f64,
    edge: f64,
    span: f64,
    name: &str,
) -> Result<Axis, String> {
    let bands = (span / step).round().max(1.0);
    let width = span / bands;
    let index = |degrees: f64| (degrees - edge) / width - 0.5;
    let first = index(lo).ceil().max(0.0);
    let last = index(hi).floor().min(bands - 1.0);
    let n = (last - first + 1.0).max(0.0) as usize;
    if n < 2 {
        return Err(format!(
            "the {name} range holds {n} grid point(s) at a step of {step}, \
             and a grid needs at least two on each side"
        ));
    }
    Ok(Axis {
        min: (edge + (first + 0.5) * width) as R,
        max: (edge + (last + 0.5) * width) as R,
        n,
    })
}

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
///
/// The rectangle is the whole world unless the request names one. A
/// smaller one costs what its own points cost, which is what makes a fine
/// grid near the station affordable: the same step over the whole globe
/// would be a hundred times the work to answer a question about one
/// region.
/// Longitudes come back folded into 0..360; every caller works in
/// -180..180 like every other coordinate they handle.
fn unfold(lon: R) -> f64 {
    if lon > 180.0 {
        f64::from(lon) - 360.0
    } else {
        f64::from(lon)
    }
}

/// The frequencies an area run covers: one, or several in one pass.
///
/// `freqMhz` is the original single-band form and stays. `freqsMhz` is
/// an array, and the reason it exists is that almost everything an area
/// run does before it reaches a frequency — the coefficient
/// interpolation and the ionogram built from it — is the same for all of
/// them. Measured over a 3,072-point grid: one band 129 ms, eight bands
/// in one pass 216 ms, eight bands run separately 1,032 ms.
///
/// The frequencies must be distinct and increasing. Each gets its own
/// area antenna table in a window bracketing only that band, and windows
/// cut halfway between neighbours cannot be formed from a list that
/// repeats or goes backwards.
fn area_freqs(req: &Json) -> Result<Vec<f32>, String> {
    let Some(many) = req.get("freqsMhz") else {
        return Ok(vec![req.number("freqMhz")? as f32]);
    };
    let list = many
        .as_array()
        .ok_or("\"freqsMhz\" must be an array of frequencies in MHz")?;
    if list.is_empty() {
        return Err("\"freqsMhz\" must name at least one frequency".into());
    }
    // `SON` holds twelve slots, which is what the frequency loop walks
    // and what an area point reports from. A thirteenth would be
    // silently dropped, so it is refused instead.
    if list.len() > 12 {
        return Err("\"freqsMhz\" may name at most 12 frequencies".into());
    }
    let freqs: Vec<f32> = list
        .iter()
        .map(|f| {
            f.as_f64()
                .filter(|v| *v > 0.0)
                .map(|v| v as f32)
                .ok_or_else(|| "every entry in \"freqsMhz\" must be a frequency in MHz".to_string())
        })
        .collect::<Result<_, _>>()?;
    if freqs.windows(2).any(|pair| pair[1] <= pair[0]) {
        return Err("\"freqsMhz\" must be increasing, with no repeats".into());
    }
    Ok(freqs)
}

/// The smoothed sunspot number, checked against the range the model can
/// answer for.
///
/// The coefficient maps are held for SSN 0 and SSN 100 and the run mixes
/// between them, so a number far outside that range gives a result the
/// mix cannot support. The card also writes the number in five columns.
/// The highest monthly smoothed sunspot number on record is near 285, so
/// 400 leaves room above every real value while it still refuses a
/// number that came from a fault.
fn ssn(req: &Json) -> Result<f64, String> {
    let value = req.number("ssn")?;
    if !(0.0..=400.0).contains(&value) {
        return Err(format!("\"ssn\" must be from 0 to 400, not {value}"));
    }
    Ok(value)
}

/// The hour of the day, as `AreaInputs` counts it.
///
/// The point path runs every hour and never takes one from the caller,
/// so an area run is the only place an hour is read. Without this check
/// an hour below zero answered a different map and gave no error, and
/// one far above 23 stopped the process: the coefficient tables are
/// indexed by hour and have no bound of their own.
///
/// `AreaInputs.hour` counts 1 to 24, the way the input file names the
/// hours, while every other interface here counts 0 to 23.
fn hour(req: &Json) -> Result<i32, String> {
    let value = req.number("hour")?;
    if !(0.0..=23.0).contains(&value) || value.fract() != 0.0 {
        return Err(format!(
            "\"hour\" must be a whole number from 0 to 23, not {value}"
        ));
    }
    Ok(value as i32 + 1)
}

fn area(tree: &std::path::Path, req: &Json) -> Result<Json, String> {
    let lat_step = req.number("latStep")?;
    let lon_step = req.number("lonStep")?;
    if lat_step <= 0.0 || lon_step <= 0.0 {
        return Err("\"latStep\" and \"lonStep\" must be positive".into());
    }
    let (rows, columns) = match bounds(req)? {
        None => (
            world_axis(lat_step, -90.0, 180.0)?,
            world_axis(lon_step, -180.0, 360.0)?,
        ),
        Some(box_) => (
            part_axis(
                box_.lat_min,
                box_.lat_max,
                lat_step,
                -90.0,
                180.0,
                "latitude",
            )?,
            part_axis(
                box_.lon_min,
                box_.lon_max,
                lon_step,
                -180.0,
                360.0,
                "longitude",
            )?,
        ),
    };
    let watts = req.number("watts")?;
    let hour = hour(req)?;

    let grid = Grid {
        projection: Projection::LatLon,
        plat: req.number("fromLat")? as f32,
        plon: req.number("fromLon")? as f32,
        xmin: columns.min,
        xmax: columns.max,
        ymin: rows.min,
        ymax: rows.max,
        nx: columns.n,
        ny: rows.n,
    };

    let inputs = AreaInputs {
        grid,
        tx_lat_deg: req.number("fromLat")?,
        tx_lon_deg: req.number("fromLon")?,
        month: req.number("month")? as u32,
        ssn: ssn(req)? as f32,
        hour,
        freqs_mhz: area_freqs(req)?,
        required_snr_db: req.number("requiredSnrDb")? as f32,
        noise_dbw: req.number("noiseDbw")? as i32,
        watts: watts as f32,
        psc: [1.0, 1.0, 1.0, 0.0],
        method: 30,
        fof2: FoF2Model::Ccir,
        inverse: false,
        // An area transmit card carries the power itself, so the default
        // is the run's own watts in kilowatts rather than zero.
        tx_antenna: antenna(req, "txAntenna")?.map(|a| a.card(watts / 1000.0)),
        rx_antenna: antenna(req, "rxAntenna")?.map(|a| a.card(0.0)),
        model: Model::Compatible,
    };

    let points = {
        let _perf = crate::perf::Step::new(crate::perf::AREA_POINTS);
        run_area(tree, &inputs)?
    };
    let _answer = crate::perf::Step::new(crate::perf::ANSWER);

    // Several bands asked for together are answered together: one row
    // per grid point carrying every band, rather than one row per point
    // per band. The coordinates are the larger half of a row — 31 bytes
    // of the 69 a single-band point costs — so repeating them once per
    // band would spend more on saying where a point is than on the
    // answer there. Measured over a 3,072-point grid: 212,569 bytes for
    // one band, 684,671 for eight together, 1,700,552 for eight apart.
    if inputs.freqs_mhz.len() > 1 {
        let rows = points
            .into_iter()
            .map(|p| {
                let reliability = p
                    .per_freq
                    .iter()
                    .map(|f| num(f.reliability.clamp(0.0, 1.0)))
                    .collect();
                let angles = p
                    .per_freq
                    .iter()
                    .map(|f| f.takeoff_angle_deg.map_or(Json::Null, num))
                    .collect();
                obj([
                    ("lat", num(f64::from(p.lat))),
                    ("lon", num(unfold(p.lon))),
                    ("reliability", Json::Arr(reliability)),
                    ("takeoffAngleDeg", Json::Arr(angles)),
                ])
            })
            .collect();
        return Ok(obj([
            ("latStep", num(lat_step)),
            ("lonStep", num(lon_step)),
            ("latMin", num(f64::from(grid.ymin))),
            ("latMax", num(f64::from(grid.ymax))),
            ("lonMin", num(f64::from(grid.xmin))),
            ("lonMax", num(f64::from(grid.xmax))),
            // Echoed so a caller reading the arrays above cannot line
            // them up against the wrong band.
            (
                "freqsMhz",
                Json::Arr(
                    inputs
                        .freqs_mhz
                        .iter()
                        .map(|f| num(f64::from(*f)))
                        .collect(),
                ),
            ),
            ("points", Json::Arr(rows)),
        ]));
    }

    let mut out = Vec::with_capacity(points.len());
    for p in points {
        let field = p
            .fields
            .get(AREA_RELIABILITY_FIELD)
            .ok_or("area row is missing its reliability field")?;
        // The field is the reference's own formatting of the number, and
        // asterisks are how Fortran reports one too wide for its column.
        let reliability = field.trim().parse::<f64>().unwrap_or(0.0);
        // Absent rather than zero when the column did not print a number.
        // Zero is an angle — a signal leaving along the horizon — so
        // reporting one where none was computed would be a measurement
        // rather than a gap, and a map would draw it as a real answer.
        let takeoff = p
            .fields
            .get(AREA_TAKEOFF_FIELD)
            .and_then(|f| f.trim().parse::<f64>().ok());
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
            ("takeoffAngleDeg", takeoff.map_or(Json::Null, num)),
        ]));
    }

    // The rectangle echoed back is the grid that ran, not the one asked
    // for: the request states any rectangle it likes and the points are
    // the lattice centres inside it, so these are the first and last
    // point on each axis. A caller drawing cells adds half a step.
    Ok(obj([
        ("latStep", num(lat_step)),
        ("lonStep", num(lon_step)),
        ("latMin", num(f64::from(grid.ymin))),
        ("latMax", num(f64::from(grid.ymax))),
        ("lonMin", num(f64::from(grid.xmin))),
        ("lonMax", num(f64::from(grid.xmax))),
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
        ssn: ssn(req)?,
        power_watts: req.number("watts")?,
        freqs_mhz: freqs.clone(),
        required_snr_db: req.number("requiredSnrDb")?,
        noise_dbw: req.number("noiseDbw")?,
        fof2: FoF2Model::Ccir,
        // The fourth value enables sporadic E. Eight months of WSPR
        // validation put it on; see docs/accuracy.md.
        layer_multipliers: [1.0, 1.0, 1.0, 1.0],
        // One card per end at most. Several would let an operator split
        // the bands between antennas, which the app has no way to ask
        // for and the card order would have to be part of the interface.
        tx_antennas: antenna(req, "txAntenna")?
            .map(|a| vec![a.choice()])
            .unwrap_or_default(),
        rx_antennas: antenna(req, "rxAntenna")?
            .map(|a| vec![a.choice()])
            .unwrap_or_default(),
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
                by_row
                    .get(&(hour, row))
                    .and_then(|m| m.get(&(slot as i8)))
                    .copied()
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
        (
            "mufByHour",
            Json::Arr(muf.iter().copied().map(num).collect()),
        ),
        ("cells", Json::Arr(cells)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parsed(text: &str) -> Json {
        json::parse(text).expect("valid json")
    }

    #[test]
    fn a_request_without_an_antenna_asks_for_none() {
        // The whole compatibility claim: every caller written before
        // antennas existed keeps predicting against the isotrope.
        let req = parsed(r#"{"watts":100}"#);
        assert!(antenna(&req, "txAntenna").expect("parses").is_none());
    }

    #[test]
    fn an_antenna_needs_only_its_file() {
        let req = parsed(r#"{"txAntenna":{"file":"hfcast/a1.voa"}}"#);
        let a = antenna(&req, "txAntenna")
            .expect("parses")
            .expect("an antenna");
        assert_eq!(a.file, "hfcast/a1.voa");
        assert_eq!(a.min_freq, 2);
        assert_eq!(a.max_freq, 30);
        assert_eq!(a.beam_deg, 0.0);
        assert_eq!(a.last_field, None);
    }

    #[test]
    fn the_two_paths_default_the_last_field_differently() {
        // An area transmit card carries the power itself, so a missing
        // last field there must become the run's kilowatts. The
        // point-to-point deck writes its own power and wants None. Getting
        // this the same way round in both places would predict a
        // transmitter running at zero watts over the whole map.
        let req = parsed(r#"{"txAntenna":{"file":"hfcast/a1.voa"}}"#);
        let read = || {
            antenna(&req, "txAntenna")
                .expect("parses")
                .expect("antenna")
        };
        assert_eq!(read().choice().last_field, None);
        assert_eq!(read().card(0.1).power_field, 0.1);
    }

    #[test]
    fn a_stated_last_field_wins_over_either_default() {
        let req = parsed(r#"{"txAntenna":{"file":"a.voa","powerField":1.5}}"#);
        let read = || {
            antenna(&req, "txAntenna")
                .expect("parses")
                .expect("antenna")
        };
        assert_eq!(read().choice().last_field, Some(1.5));
        assert_eq!(read().card(0.1).power_field, 1.5);
    }

    #[test]
    fn every_card_field_can_be_stated() {
        let req = parsed(
            r#"{"rxAntenna":{"file":"a.voa","beamDeg":135,"designFreq":14.1,
                "minFreq":7,"maxFreq":21}}"#,
        );
        let a = antenna(&req, "rxAntenna")
            .expect("parses")
            .expect("an antenna");
        assert_eq!(a.beam_deg, 135.0);
        assert_eq!(a.design_freq, 14.1);
        assert_eq!((a.min_freq, a.max_freq), (7, 21));
    }

    #[test]
    fn a_path_too_long_for_the_card_is_refused() {
        // The card holds 21 columns. Truncating instead would name a file
        // that does not exist, and the failure would come from the
        // antenna reader with nothing pointing back at the request.
        let long = "hfcast/aaaaaaaaaaaaaaaaaa.voa";
        assert!(long.len() > 21);
        let req = parsed(&format!(r#"{{"txAntenna":{{"file":"{long}"}}}}"#));
        assert!(antenna(&req, "txAntenna").is_err());
    }

    #[test]
    fn an_antenna_without_a_file_is_refused() {
        let req = parsed(r#"{"txAntenna":{"beamDeg":90}}"#);
        assert!(antenna(&req, "txAntenna").is_err());
        let empty = parsed(r#"{"txAntenna":{"file":"   "}}"#);
        assert!(antenna(&empty, "txAntenna").is_err());
    }

    #[test]
    fn an_inverted_frequency_range_is_refused() {
        // GAIN takes the first card whose range holds the frequency, so an
        // inverted range serves nothing and the end silently loses its
        // antenna rather than reporting anything.
        let req = parsed(r#"{"txAntenna":{"file":"a.voa","minFreq":21,"maxFreq":7}}"#);
        assert!(antenna(&req, "txAntenna").is_err());
    }

    #[test]
    fn the_whole_world_is_still_the_grid_it_always_was() {
        // The compatibility claim, checked rather than argued. A grid a
        // thousandth of a degree off would move every point of every map
        // and nothing on screen would say so.
        for (step, edge, span) in [(15.0, -90.0, 180.0), (22.5, -180.0, 360.0)] {
            let a = world_axis(step, edge, span).expect("a grid");
            assert_eq!(a.min, (edge + step / 2.0) as R);
            assert_eq!(a.max, (edge + span - step / 2.0) as R);
            assert_eq!(a.n, (span / step).round() as usize);
        }
        let rows = world_axis(15.0, -90.0, 180.0).expect("a grid");
        assert_eq!((rows.min, rows.max, rows.n), (-82.5, 82.5, 12));
        let columns = world_axis(22.5, -180.0, 360.0).expect("a grid");
        assert_eq!((columns.min, columns.max, columns.n), (-168.75, 168.75, 16));
    }

    #[test]
    fn a_rectangle_over_the_whole_world_is_the_whole_world() {
        // Only where the step divides the world evenly, which is every
        // step the callers use. `world_axis` says why it is kept separate.
        for (step, edge, span) in [
            (15.0, -90.0, 180.0),
            (22.5, -180.0, 360.0),
            (1.25, -90.0, 180.0),
            (1.5, -180.0, 360.0),
        ] {
            let whole = world_axis(step, edge, span).expect("a grid");
            let asked = part_axis(edge, edge + span, step, edge, span, "axis").expect("a grid");
            assert_eq!(
                (whole.min, whole.max, whole.n),
                (asked.min, asked.max, asked.n),
                "at a step of {step}"
            );
        }
    }

    #[test]
    fn a_rectangle_holds_the_lattice_centres_inside_it() {
        // Denver, plus and minus ten degrees, at the patch's own step.
        let rows = part_axis(29.74, 49.74, 1.25, -90.0, 180.0, "latitude").expect("a grid");
        assert_eq!(rows.n, 16);
        assert_eq!((rows.min, rows.max), (30.625, 49.375));
        // Every point is inside what was asked for, and one more step
        // either way would leave it.
        assert!(rows.min >= 29.74 && rows.max <= 49.74);
        assert!(rows.min - 1.25 < 29.74 && rows.max + 1.25 > 49.74);
    }

    #[test]
    fn a_rectangle_lands_on_the_same_lattice_as_the_whole_world() {
        // What makes a patch a window on the coarse grid rather than a
        // second grid beside it. Without this the fine cells would sit
        // half over one coarse cell and half over its neighbour, and the
        // map would show a seam that means nothing.
        let part = part_axis(-40.0, 40.0, 15.0, -90.0, 180.0, "latitude").expect("a grid");
        assert_eq!((part.min, part.max, part.n), (-37.5, 37.5, 6));
    }

    #[test]
    fn a_rectangle_never_reaches_past_the_pole() {
        // A patch around a station at 85 degrees north asks for ten
        // degrees of latitude that do not exist. The lattice is clamped to
        // the world rather than continued past it, so the grid stops at
        // the last real band instead of running a prediction to a
        // latitude of 95.
        let rows = part_axis(75.0, 95.0, 1.25, -90.0, 180.0, "latitude").expect("a grid");
        assert!(rows.max <= 90.0, "{} is past the pole", rows.max);
        assert_eq!(rows.max, 89.375);
        assert_eq!(rows.n, 12);
    }

    #[test]
    fn a_rectangle_stated_in_part_is_refused() {
        // The dangerous case: answered over every longitude, which is a
        // hundred times the work and arrives looking correct.
        let req = parsed(r#"{"latMin":30,"latMax":50}"#);
        assert!(bounds(&req).is_err());
        assert!(bounds(&parsed("{}")).is_ok());
    }

    #[test]
    fn an_empty_or_inverted_rectangle_is_refused() {
        let refused = |text: &str| bounds(&parsed(text)).is_err();
        assert!(refused(
            r#"{"latMin":50,"latMax":30,"lonMin":-10,"lonMax":10}"#
        ));
        assert!(refused(
            r#"{"latMin":30,"latMax":30,"lonMin":-10,"lonMax":10}"#
        ));
        assert!(refused(
            r#"{"latMin":30,"latMax":50,"lonMin":10,"lonMax":10}"#
        ));
        assert!(refused(
            r#"{"latMin":-91,"latMax":50,"lonMin":-10,"lonMax":10}"#
        ));
    }

    #[test]
    fn a_rectangle_across_the_antimeridian_says_so() {
        // Refused rather than guessed at, and the message has to name the
        // reason: 170 to -170 is how somebody writes a real rectangle,
        // not a typing mistake.
        let req = parsed(r#"{"latMin":-10,"latMax":10,"lonMin":170,"lonMax":-170}"#);
        let message = bounds(&req).expect_err("refused");
        assert!(message.contains("antimeridian"), "{message}");
    }

    #[test]
    fn a_rectangle_smaller_than_its_own_step_is_refused() {
        // `Grid::point` divides by the number of points less one, so a
        // single-point axis is a division by zero rather than a small
        // answer.
        let message = part_axis(30.0, 31.0, 15.0, -90.0, 180.0, "latitude").expect_err("refused");
        assert!(message.contains("latitude"), "{message}");
    }
}
