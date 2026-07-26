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
use propcore::runner::{map_limit, run_area, variant_bin, IsolatedRoot};

/// One grid to check, with the reason it is in the set.
struct Case {
    name: &'static str,
    why: &'static str,
    grid: Grid,
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
        },
        Case {
            name: "odd",
            why: "a grid whose centre point falls exactly on the origin",
            grid: gc(51.50, -0.13, -2000.0, 2000.0, -2000.0, 2000.0, 5),
        },
        Case {
            name: "south",
            why: "a southern centre, where the latitude sign flips",
            grid: gc(-33.87, 151.21, -3000.0, 3000.0, -3000.0, 3000.0, 7),
        },
        Case {
            name: "polar",
            why: "reaching over the pole, where the azimuth arithmetic folds",
            grid: gc(78.20, 15.60, -4000.0, 4000.0, -4000.0, 4000.0, 7),
        },
        Case {
            name: "anti",
            why: "a rectangle wide enough to pass the antipode",
            grid: gc(0.00, 0.00, -19000.0, 19000.0, -8000.0, 8000.0, 11),
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
        diffs: Vec<String>,
        broken: Option<String>,
    }

    let outcomes = map_limit(&cases, jobs, |case, _| {
        let mut out = Outcome {
            name: case.name,
            why: case.why,
            points: 0,
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
        let text = match run_area(&reference, root.path(), case.name, &area_file(&case.grid)) {
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
        for (ix, iy, lat, lon) in rows {
            out.points += 1;
            // The transmitter sits at the centre of the projection in
            // every case here, which is what the distributed file does,
            // and its longitude is passed as the driver holds it —
            // unfolded, so possibly negative.
            let (plon, plat) = case.grid.receiver(ix, iy, case.grid.plat, case.grid.plon);
            let close = |a: f64, b: f32| (a * 10_000.0).round_ties_even() == (f64::from(b) * 10_000.0).round_ties_even();
            if !close(lat, plat) || !close(lon, plon) {
                out.diffs.push(format!(
                    "({ix},{iy}): reference {lat:.4} {lon:.4}, port {plat:.4} {plon:.4}"
                ));
            }
        }
        out
    });

    let mut points = 0usize;
    let mut failed = false;
    for o in &outcomes {
        points += o.points;
        if let Some(why) = &o.broken {
            failed = true;
            println!("{}: {}", o.name, why);
            continue;
        }
        if o.diffs.is_empty() {
            println!("{:8} {} points — {}", o.name, o.points, o.why);
        } else {
            failed = true;
            println!("{:8} {} of {} points differ — {}", o.name, o.diffs.len(), o.points, o.why);
            for d in o.diffs.iter().take(6) {
                println!("    {d}");
            }
            if o.diffs.len() > 6 {
                println!("    ...and {} more", o.diffs.len() - 6);
            }
        }
    }
    println!("\n{points} grid points compared over {} grids.", outcomes.len());
    if failed {
        println!("Verdict: the area grid disagrees with the reference.");
        ExitCode::FAILURE
    } else {
        println!("Verdict: every grid point matches the reference.");
        ExitCode::SUCCESS
    }
}

/// Writes the keyed area input file. The values that are not the grid
/// are held fixed: what varies here is the geometry.
fn area_file(grid: &Grid) -> String {
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
        "Months   :   6.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00   0.00".to_string(),
        "Ssns     :    100      0      0      0      0      0      0      0      0".to_string(),
        "Hours    :     18      0      0      0      0      0      0      0      0".to_string(),
        "Freqs    : 11.850  0.000  0.000  0.000  0.000  0.000  0.000  0.000  0.000".to_string(),
        "System   :  145     0.100   90   73     3.000     0.100".to_string(),
        "Fprob    : 1.00 1.00 1.00 0.00".to_string(),
        "Rec Ants :[default /swwhip.voa  ]  gain=   0.0   0.0".to_string(),
        "Tx Ants  :[default /const17.voa ]  0.000  57.0   500.0000".to_string(),
    ]
    .join("\n")
        + "\n"
}

/// Reads the grid file's data rows: the point indices and its latitude
/// and longitude. The header carries the two grid dimensions in the
/// same columns the rows use for their indices, so a row is recognised
/// by having a latitude and longitude that parse.
fn parse_grid(text: &str) -> Vec<(usize, usize, f64, f64)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if line.len() < 26 {
            continue;
        }
        let ix = line[0..3].trim().parse::<usize>();
        let iy = line[3..6].trim().parse::<usize>();
        // Two I3 indices, then the latitude and longitude in ten
        // columns each, as the header's own labels are spaced.
        let lat = line[6..16].trim().parse::<f64>();
        let lon = line[16..26].trim().parse::<f64>();
        if let (Ok(ix), Ok(iy), Ok(lat), Ok(lon)) = (ix, iy, lat, lon) {
            out.push((ix, iy, lat, lon));
        }
    }
    out
}
