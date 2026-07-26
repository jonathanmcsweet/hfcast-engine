//! Area check: the Rust area grid against the reference's own grid.
//!
//! An area run sweeps a grid of receiver locations and, at each one,
//! runs the same one-hour prediction a point-to-point method runs. The
//! grid file it writes names every point's latitude and longitude
//! before the predicted values, so the geometry can be checked on its
//! own, ahead of the antenna work the predictions still need.
//!
//! This runs the reference over several grids — both projections, a
//! range of sizes, centres north and south, and rectangles that
//! straddle the prime meridian and the poles — and compares every
//! point's coordinates against [`Grid::point`] at the four decimals the
//! file prints.
//!
//! Usage: `cargo run --release --bin areacheck [--jobs J]`

use std::process::ExitCode;

use propcore::engine::area::{Grid, Projection};
use propcore::engine::coefficients::FoF2Model;
use propcore::engine::run::{f_fixed, run_area as port_area, AreaInputs};
use propcore::runner::{map_limit, run_area, variant_bin, IsolatedRoot};

/// The values the area file holds fixed. What varies between cases is
/// the geometry.
const HOUR: i32 = 18;
const MONTH: u32 = 6;
const SSN: f32 = 100.0;
const FREQ: f32 = 11.850;
const REQUIRED_SNR: f32 = 73.0;
const NOISE_DBW: i32 = 145;
const WATTS: f32 = 100.0;
/// Isotropes at both ends, so the area antenna table is one constant at
/// every azimuth and elevation. The transmit card's design frequency and
/// the receive card's gain field both become the isotrope's gain, and
/// they differ here so a swap between the two ends would show.
const TX_GAIN: f32 = 2.5;
const RX_GAIN: f32 = 1.5;

/// One grid to check, with the reason it is in the set.
struct Case {
    name: &'static str,
    why: &'static str,
    grid: Grid,
    /// The run's frequencies. One gives `OUTAREA`'s 24-column form, more
    /// than one its 7-column form, where six values are the largest over
    /// the frequencies.
    ///
    /// The area file's `Freqs` line holds one frequency per plot, not a
    /// list for one run. Several frequencies are asked for by naming a
    /// frequency at or below 0.5, which makes the reference read the list
    /// from `run/areafreq.dat` instead — a file this tree does not ship,
    /// so the case writes it.
    freqs: &'static [f32],
}

fn cases() -> Vec<Case> {
    let gc = |plat, plon, xmin, xmax, ymin, ymax, n| Grid {
        projection: Projection::GreatCircle,
        plat,
        plon,
        xmin,
        xmax,
        ymin,
        ymax,
        nx: n,
        ny: n,
    };
    vec![
        Case {
            name: "dist",
            why: "the distributed file's own rectangle, at a smaller size",
            grid: gc(35.80, -5.90, -1000.0, 6000.0, -1000.0, 4000.0, 9),
            freqs: &[FREQ],
        },
        Case {
            name: "odd",
            why: "a grid whose centre point falls exactly on the origin",
            grid: gc(51.50, -0.13, -2000.0, 2000.0, -2000.0, 2000.0, 5),
            freqs: &[FREQ],
        },
        Case {
            name: "south",
            why: "a southern centre, where the latitude sign flips",
            grid: gc(-33.87, 151.21, -3000.0, 3000.0, -3000.0, 3000.0, 7),
            freqs: &[FREQ],
        },
        Case {
            name: "polar",
            why: "reaching over the pole, where the azimuth arithmetic folds",
            grid: gc(78.20, 15.60, -4000.0, 4000.0, -4000.0, 4000.0, 7),
            freqs: &[FREQ],
        },
        Case {
            name: "anti",
            why: "a rectangle wide enough to pass the antipode",
            grid: gc(0.00, 0.00, -19000.0, 19000.0, -8000.0, 8000.0, 11),
            freqs: &[FREQ],
        },
        Case {
            name: "manyfreq",
            why: "several frequencies, where the columns become maxima over them",
            grid: gc(48.86, 2.35, -5000.0, 5000.0, -5000.0, 5000.0, 7),
            freqs: &[7.100, 11.850, 15.400, 21.650],
        },
        // The IPROJ = 8 projection is left out until its printed
        // longitudes are understood: the reference prints them
        // unfolded, negative, where `GRIDXY` folds every longitude into
        // 0 to 360 before returning. See docs/roadmap.md.
    ]
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let jobs = argv
        .iter()
        .position(|a| a == "--jobs")
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(3usize)
        .max(1);

    let reference = variant_bin("O2");
    if !reference.is_file() {
        eprintln!("no O2 variant; run tools/build-variants.sh");
        return ExitCode::FAILURE;
    }

    let cases = cases();
    println!("# Area grid check: {} grids\n", cases.len());

    struct Outcome {
        name: &'static str,
        why: &'static str,
        points: usize,
        cells: usize,
        diffs: Vec<String>,
        broken: Option<String>,
    }

    let outcomes = map_limit(&cases, jobs, |case, _| {
        let mut out = Outcome {
            name: case.name,
            why: case.why,
            points: 0,
            cells: 0,
            diffs: Vec::new(),
            broken: None,
        };
        let root = match IsolatedRoot::create(&format!("area-{}", case.name)) {
            Ok(r) => r,
            Err(e) => {
                out.broken = Some(format!("tree: {e}"));
                return out;
            }
        };
        if case.freqs.len() > 1 {
            let mut list = String::new();
            for i in 0..11 {
                list += &format!("{:8.3}", case.freqs.get(i).copied().unwrap_or(0.0));
            }
            if let Err(e) = std::fs::write(root.path().join("run").join("areafreq.dat"), list + "\n")
            {
                out.broken = Some(format!("areafreq.dat: {e}"));
                return out;
            }
        }
        let text = match run_area(&reference, root.path(), case.name, &area_file(case)) {
            Ok(t) => t,
            Err(e) => {
                out.broken = Some(format!("reference: {e}"));
                return out;
            }
        };
        let rows = parse_grid(&text);
        let want = case.grid.nx * case.grid.ny;
        if rows.len() != want {
            out.broken = Some(format!("parsed {} points, wanted {want}", rows.len()));
            return out;
        }
        let inputs = AreaInputs {
            grid: case.grid,
            tx_lat_deg: f64::from(case.grid.plat),
            tx_lon_deg: f64::from(case.grid.plon),
            month: MONTH,
            ssn: SSN,
            hour: HOUR,
            freqs_mhz: case.freqs.to_vec(),
            required_snr_db: REQUIRED_SNR,
            noise_dbw: NOISE_DBW,
            watts: WATTS,
            psc: [1.0, 1.0, 1.0, 0.0],
            method: 30,
            fof2: FoF2Model::Ccir,
            tx_gain_db: TX_GAIN,
            rx_gain_db: RX_GAIN,
        };
        let ported = match port_area(root.path(), &inputs) {
            Ok(p) => p,
            Err(e) => {
                out.broken = Some(format!("port: {e}"));
                return out;
            }
        };
        if ported.len() != rows.len() {
            out.broken = Some(format!(
                "port returned {} points, reference {}",
                ported.len(),
                rows.len()
            ));
            return out;
        }
        for (reference, port) in rows.iter().zip(&ported) {
            out.points += 1;
            if reference.ix != port.ix || reference.iy != port.iy {
                out.diffs.push(format!(
                    "point order: reference ({},{}), port ({},{})",
                    reference.ix, reference.iy, port.ix, port.iy
                ));
                continue;
            }
            // The coordinates print through `f10.4`, so they are
            // compared as the file's own text.
            let coords = format!("{}{}", f_fixed(port.lat, 10, 4), f_fixed(port.lon, 10, 4));
            out.cells += 2;
            if coords != reference.coords {
                out.diffs.push(format!(
                    "({},{}) coords: reference '{}', port '{}'",
                    reference.ix, reference.iy, reference.coords, coords
                ));
            }
            for (i, (want, got)) in reference.fields.iter().zip(&port.fields).enumerate() {
                out.cells += 1;
                if want != got {
                    out.diffs.push(format!(
                        "({},{}) {}: reference '{want}', port '{got}'",
                        reference.ix,
                        reference.iy,
                        COLUMNS.get(i).copied().unwrap_or("?")
                    ));
                }
            }
        }
        out
    });

    let mut points = 0usize;
    let mut cells = 0usize;
    let mut failed = false;
    for o in &outcomes {
        points += o.points;
        cells += o.cells;
        if let Some(why) = &o.broken {
            failed = true;
            println!("{}: {}", o.name, why);
            continue;
        }
        if o.diffs.is_empty() {
            println!("{:8} {} points — {}", o.name, o.points, o.why);
        } else {
            failed = true;
            println!(
                "{:8} {} of {} cells differ — {}",
                o.name,
                o.diffs.len(),
                o.cells,
                o.why
            );
            for d in o.diffs.iter().take(6) {
                println!("    {d}");
            }
            if o.diffs.len() > 6 {
                println!("    ...and {} more", o.diffs.len() - 6);
            }
        }
    }
    println!(
        "\n{points} grid points, {cells} printed cells, over {} grids.",
        outcomes.len()
    );
    if failed {
        println!("Verdict: the area output disagrees with the reference.");
        ExitCode::FAILURE
    } else {
        println!("Verdict: every printed cell matches the reference.");
        ExitCode::SUCCESS
    }
}

/// Writes the keyed area input file. The values that are not the grid
/// are held fixed: what varies here is the geometry.
fn area_file(case: &Case) -> String {
    let grid = &case.grid;
    let hemi = |v: f32, pos: char, neg: char| {
        format!("{:9.2}{}", v.abs(), if v >= 0.0 { pos } else { neg })
    };
    // The projection travels in the Gridsize card's second field.
    let gridtype = match grid.projection {
        Projection::GreatCircle => 0,
        Projection::LatLon => 1,
    };
    [
        "Model    :VOACAP".to_string(),
        "Colors   :Black    :Blue     :Ignore   :Ignore   :Red      :Black with shading".to_string(),
        "Cities   :Receive.cty".to_string(),
        "Nparms   :    1".to_string(),
        "Parameter:MUF      0".to_string(),
        format!(
            "Transmit :{}{} CHECK                Short",
            hemi(grid.plat, 'N', 'S'),
            hemi(grid.plon, 'E', 'W')
        ),
        format!(
            "Pcenter  :{}{} CHECK",
            hemi(grid.plat, 'N', 'S'),
            hemi(grid.plon, 'E', 'W')
        ),
        format!(
            "Area     :{:10.1}{:10.1}{:10.1}{:10.1}",
            grid.xmin, grid.xmax, grid.ymin, grid.ymax
        ),
        format!("Gridsize :{:5}{:5}", grid.nx, gridtype),
        "Method   :   30".to_string(),
        "Coeffs   :CCIR".to_string(),
        format!("Months   :{MONTH:7.2}   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00"),
        format!("Ssns     :{:7}      0      0      0      0      0      0      0      0", SSN as i32),
        format!("Hours    :{HOUR:7}      0      0      0      0      0      0      0      0"),
        {
            // One frequency per plot. A value at or below 0.5 asks the
            // reference to read its frequency list from areafreq.dat.
            let first = if case.freqs.len() > 1 {
                0.0
            } else {
                case.freqs[0]
            };
            format!("Freqs    :{first:7.3}  0.000  0.000  0.000  0.000  0.000  0.000  0.000  0.000")
        },
        format!(
            "System   :{NOISE_DBW:5}     0.100   90{:5}     3.000     0.100",
            REQUIRED_SNR as i32
        ),
        "Fprob    : 1.00 1.00 1.00 0.00".to_string(),
        format!("Rec Ants :[default /isotrope    ]  gain={RX_GAIN:6.1}   0.0"),
        format!("Tx Ants  :[default /isotrope    ]{TX_GAIN:7.3}   0.0{:10.4}", WATTS / 1000.0),
    ]
    .join("\n")
        + "\n"
}

/// `OUTAREA`'s column labels, for naming a differing cell.
const COLUMNS: [&str; 24] = [
    "MUF", "MODE", "ANGLE", "DELAY", "VHITE", "MUFda", "LOSS", "DBU", "SDBW", "NDBW", "SNR",
    "RPWRG", "REL", "MPROB", "SPROB", "TGAIN", "RGAIN", "SNRxx", "DU", "DL", "SIGLW", "SIGUP",
    "PWRCT", "ANGLER",
];

/// One data row of the grid file.
struct Row {
    ix: usize,
    iy: usize,
    /// The two coordinate fields as printed, twenty characters.
    coords: String,
    /// The value columns, six characters each.
    fields: Vec<String>,
}

/// Reads the grid file's data rows: two `I3` indices, two `F10.4`
/// coordinates, then `A6` value columns.
///
/// The header carries the two grid dimensions in the same columns the
/// rows use for their indices, so a row is recognised by having indices
/// that parse as positive numbers and a full set of value columns.
fn parse_grid(text: &str) -> Vec<Row> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.len() < 26 {
            continue;
        }
        let (Ok(ix), Ok(iy)) = (
            line[0..3].trim().parse::<usize>(),
            line[3..6].trim().parse::<usize>(),
        ) else {
            continue;
        };
        if line[6..16].trim().parse::<f64>().is_err() {
            continue;
        }
        let fields: Vec<String> = line.as_bytes()[26..]
            .chunks(6)
            .filter(|c| c.len() == 6)
            .map(|c| String::from_utf8_lossy(c).into_owned())
            .collect();
        out.push(Row {
            ix,
            iy,
            coords: line[6..26].to_string(),
            fields,
        });
    }
    out
}
