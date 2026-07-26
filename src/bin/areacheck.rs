//! Area check: the Rust area grid against the reference's own grid.
//!
//! An area run sweeps a grid of receiver locations and, at each one,
//! runs the same one-hour prediction a point-to-point method runs. The
//! grid file it writes names every point's latitude and longitude
//! before the predicted values, so both the geometry and the prediction
//! are compared, as text, in the formats the file prints.
//!
//! Two things vary across the cases. The geometry: a range of sizes,
//! centres north and south, rectangles that straddle the prime meridian
//! and the poles, and one and several frequencies. And the antennas: one
//! case per family `ANTCALC`'s area branch computes, at both ends, over a
//! grid whose points reach every quadrant — so the 360-azimuth table is
//! read at a different bearing at every point.
//!
//! Usage: `cargo run --release --bin areacheck [--jobs J]`

use std::process::ExitCode;

use propcore::engine::area::{Grid, Projection};
use propcore::engine::coefficients::FoF2Model;
use propcore::engine::run::{f_fixed, run_area as port_area, AntennaCardSpec, AreaInputs};
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
const TX_GAIN: f32 = 2.5;
const RX_GAIN: f32 = 1.5;

/// One antenna, as the area input file's `Tx Ants` or `Rec Ants` line
/// carries it.
struct Ant {
    /// The directory and name, as the bracketed card field wants them.
    dir: &'static str,
    name: &'static str,
    /// The transmit line's design frequency, or the receive line's gain.
    /// Both end up in the same card field, and for an isotrope both
    /// become its gain.
    value: f32,
    /// The main beam bearing.
    beam: f32,
}

/// The isotropes the geometry cases use, so the area table is one
/// constant at every azimuth and elevation. The transmit card's design
/// frequency and the receive card's gain field both become the isotrope's
/// gain, and they differ here so a swap between the two ends would show.
const TX_ISO: Ant = Ant {
    dir: "default",
    name: "isotrope",
    value: TX_GAIN,
    beam: 0.0,
};
const RX_ISO: Ant = Ant {
    dir: "default",
    name: "isotrope",
    value: RX_GAIN,
    beam: 0.0,
};

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
    tx: Ant,
    rx: Ant,
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
    // A grid that reaches every quadrant from its centre, so the 25
    // points cut the antenna pattern at 25 different bearings.
    let spread = gc(40.00, 10.00, -3000.0, 3000.0, -3000.0, 3000.0, 5);
    // The antenna pairs put a different family at each end, so a swap
    // between the two ends changes every row.
    let ant = |dir, name, value, beam| Ant {
        dir,
        name,
        value,
        beam,
    };
    vec![
        Case {
            name: "dist",
            why: "the distributed file's own rectangle, at a smaller size",
            grid: gc(35.80, -5.90, -1000.0, 6000.0, -1000.0, 4000.0, 9),
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "odd",
            why: "a grid whose centre point falls exactly on the origin",
            grid: gc(51.50, -0.13, -2000.0, 2000.0, -2000.0, 2000.0, 5),
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "south",
            why: "a southern centre, where the latitude sign flips",
            grid: gc(-33.87, 151.21, -3000.0, 3000.0, -3000.0, 3000.0, 7),
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "polar",
            why: "reaching over the pole, where the azimuth arithmetic folds",
            grid: gc(78.20, 15.60, -4000.0, 4000.0, -4000.0, 4000.0, 7),
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "anti",
            why: "a rectangle wide enough to pass the antipode",
            grid: gc(0.00, 0.00, -19000.0, 19000.0, -8000.0, 8000.0, 11),
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "manyfreq",
            why: "several frequencies, where the columns become maxima over them",
            grid: gc(48.86, 2.35, -5000.0, 5000.0, -5000.0, 5000.0, 7),
            freqs: &[7.100, 11.850, 15.400, 21.650],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        // From here the antenna is what varies: one case per family the
        // area branch of `ANTCALC` computes, at both ends.
        Case {
            name: "ccir",
            why: "the CCIR patterns, types 2 and 6",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.02", 0.0, 45.0),
            rx: ant("samples", "sample.06", 0.0, 200.0),
        },
        Case {
            name: "rhombic",
            why: "a non-terminated rhombic, whose table is built over folded azimuths",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.07", 0.0, -40.0),
            rx: ant("samples", "sample.09", 0.0, 130.0),
        },
        Case {
            name: "tables",
            why: "the measured tables, over 360 azimuths and over 30 frequencies",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.13", 0.0, 70.0),
            rx: ant("samples", "sample.14", 0.0, 250.0),
        },
        Case {
            name: "curtain",
            why: "an NTIA curtain array, and a gain table in elevation only",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.12", 0.0, 90.0),
            rx: ant("samples", "sample.11", 0.0, 0.0),
        },
        Case {
            name: "ioncap",
            why: "the IONCAP patterns, types 21 and 25",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.21", 0.0, 20.0),
            // Type 25's efficiency varies with elevation, so which of
            // the branch's calls leaves the stored value behind matters.
            rx: ant("samples", "sample.25", 0.0, 300.0),
        },
        Case {
            name: "mufes",
            why: "the HFMUFES patterns, types 31 and 44",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.31", 0.0, 160.0),
            rx: ant("samples", "sample.44", 0.0, 10.0),
        },
        Case {
            name: "nosc",
            why: "the NOSC inverted cone, and a vertical monopole",
            grid: spread,
            freqs: &[FREQ],
            tx: ant("samples", "sample.48", 0.0, 0.0),
            rx: ant("samples", "sample.10", 0.0, 0.0),
        },
        Case {
            name: "multiant",
            why: "several frequencies with a beam, where the table is cut along one bearing",
            grid: spread,
            freqs: &[7.100, 11.850, 15.400],
            tx: ant("samples", "sample.02", 0.0, 45.0),
            rx: ant("samples", "sample.06", 0.0, 200.0),
        },
        // The latitude and longitude projection, whose rectangle is in
        // degrees and whose printed longitudes come back unfolded.
        Case {
            name: "latlon",
            why: "the degree mesh with a negative western edge, reaching the pole",
            grid: Grid {
                projection: Projection::LatLon,
                plat: 40.00,
                plon: 10.00,
                xmin: -20.0,
                xmax: 20.0,
                ymin: 50.0,
                ymax: 90.0,
                nx: 5,
                ny: 5,
            },
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "latlonwest",
            why: "a western edge past -180, where the first column alone is unfolded",
            grid: Grid {
                projection: Projection::LatLon,
                plat: 40.00,
                plon: 10.00,
                xmin: -200.0,
                xmax: -160.0,
                ymin: 20.0,
                ymax: 50.0,
                nx: 5,
                ny: 5,
            },
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
        Case {
            name: "latlon180",
            why: "the degree mesh past 180, where the edge is not negative and nothing is unfolded",
            grid: Grid {
                projection: Projection::LatLon,
                plat: 40.00,
                plon: 10.00,
                xmin: 170.0,
                xmax: 200.0,
                ymin: 20.0,
                ymax: 50.0,
                nx: 5,
                ny: 5,
            },
            freqs: &[FREQ],
            tx: TX_ISO,
            rx: RX_ISO,
        },
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
            // The transmit card carries a design frequency and the
            // receive card a gain, in the same field.
            tx_antenna: Some(spec(&case.tx, case.tx.value)),
            rx_antenna: Some(spec(&case.rx, 0.0)),
            rx_gain_field: case.rx.value,
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
            let coords = format!(
                "{}{}",
                f_fixed(port.lat, 10, 4),
                f_fixed(port.print_lon, 10, 4)
            );
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

/// The card as the port reads it, from the same fields the input file
/// carries. `AREAMAP` writes the transmit card's design frequency from
/// the input file and the receive card's as zero.
fn spec(ant: &Ant, design_freq: f32) -> AntennaCardSpec {
    AntennaCardSpec {
        file: format!("{}/{}", ant.dir, ant.name),
        design_freq,
        beam_deg: ant.beam,
        min_freq: 2,
        max_freq: 30,
    }
}

/// Writes the keyed area input file. The values that are not the grid
/// are held fixed: what varies here is the geometry and the antennas.
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
        format!(
            "Rec Ants :[{:<8}/{:<12}]  gain={:6.1}{:6.1}",
            case.rx.dir, case.rx.name, case.rx.value, case.rx.beam
        ),
        format!(
            "Tx Ants  :[{:<8}/{:<12}]{:7.3}{:6.1} {:10.4}",
            case.tx.dir,
            case.tx.name,
            case.tx.value,
            case.tx.beam,
            WATTS / 1000.0
        ),
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
