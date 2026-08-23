//! Does this engine give the same answer on another architecture?
//!
//! Every other harness compares the port against the reference Fortran, and
//! all of them have only ever run on x86-64. The reference cannot easily be
//! built for a phone, so the claim is chained instead: `portcheck` establishes
//! that the port matches the reference on x86-64, and this establishes that
//! the port matches *itself* elsewhere. Both green means the port matches the
//! reference on the other architecture too.
//!
//! What is actually at risk is the maths library. IEEE-754 fixes add, multiply
//! and divide, and Rust does not contract expressions into fused multiply-add,
//! so plain arithmetic is safe. `sin`, `cos`, `exp`, `pow` and `log` are not
//! guaranteed identical between platforms or libm versions, and this engine
//! calls them throughout the geometry and absorption paths. A last-place
//! difference there can move a rounded listing field.
//!
//! No Fortran, no isolated root per case, no temporary trees: it renders the
//! same listing text `portcheck` compares and prints a digest per case. That
//! keeps it runnable under an emulator, where process spawning and disk copies
//! are what make the other harnesses impractical.
//!
//! Both shapes of run are covered. The point-to-point sweep is the bulk of it;
//! the area grids at the end are here because the app draws its coverage map
//! from one, and an area run reaches the same maths from a different direction
//! — one hour over hundreds of bearings, rather than one bearing over a day.
//! A libm difference in the azimuth path could show in the map and not in the
//! forecast.
//!
//! Usage: `cargo run --release --bin archcheck [--cases N] [--full]`
//!
//! `--full` prints every listing instead of digests, for diffing a case that
//! disagrees. `--cases N` limits the point-to-point sweep only; the area cases
//! are few and always run.

use std::process::ExitCode;

use hfcast::sweep::sweep_cases;
use hfcast::voacap::area::{Grid, Projection};
use hfcast::voacap::coefficients::FoF2Model;
use hfcast::voacap::data::EMBEDDED;
use hfcast::voacap::model::Model;
use hfcast::voacap::run::{
    body_lines, f_fixed, listing_text, run, run_area, AntennaCardSpec, AreaInputs, RunInputs,
};

/// A digest small enough to read in a table and wide enough not to collide
/// across a hundred listings. FNV-1a over the bytes: the point is detecting
/// difference, not resisting an adversary, and a hand-written hash keeps this
/// harness free of dependencies that might themselves differ per platform.
fn digest(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

/// One area grid to check, with the reason it is in the set.
struct AreaCase {
    id: &'static str,
    inputs: AreaInputs,
}

/// The grid the app's coverage map runs on: whole-sphere, points at cell
/// centres, one frequency. 15 by 22.5 degrees is 12 rows of 16.
fn whole_sphere(plat: f32, plon: f32) -> Grid {
    let (lat_step, lon_step) = (15.0, 22.5);
    Grid {
        projection: Projection::LatLon,
        plat,
        plon,
        xmin: -180.0 + lon_step / 2.0,
        xmax: 180.0 - lon_step / 2.0,
        ymin: -90.0 + lat_step / 2.0,
        ymax: 90.0 - lat_step / 2.0,
        nx: 16,
        ny: 12,
    }
}

fn area_cases() -> Vec<AreaCase> {
    let base = |id: &'static str, grid: Grid, beam_deg: f32, hour: i32| AreaCase {
        id,
        inputs: AreaInputs {
            arith: Default::default(),
            grid,
            tx_lat_deg: f64::from(grid.plat),
            tx_lon_deg: f64::from(grid.plon),
            month: 7,
            ssn: 92.3,
            hour,
            freqs_mhz: vec![14.2],
            required_snr_db: 24.0,
            noise_dbw: -145,
            watts: 100.0,
            psc: [1.0, 1.0, 1.0, 0.0],
            method: 30,
            fof2: FoF2Model::Ccir,
            inverse: false,
            // A directional card, because a beam is what makes the answer
            // depend on the azimuth to each point, and the azimuth is where
            // the trigonometry that could differ per platform lives.
            tx_antenna: Some(AntennaCardSpec {
                file: "default/swwhip.voa".to_string(),
                design_freq: 14.2,
                beam_deg,
                min_freq: 2,
                max_freq: 30,
                power_field: 0.1,
            }),
            rx_antenna: Some(AntennaCardSpec::isotrope(0.0)),
            model: Model::Compatible,
        },
    };

    vec![
        // A mid-latitude station, which is the ordinary case.
        base("area/seattle-14mhz-03z", whole_sphere(47.6, -122.3), 0.0, 4),
        // Southern hemisphere and east of Greenwich, so both coordinate
        // signs are exercised, at an hour on the other side of the day.
        base(
            "area/sydney-14mhz-15z",
            whole_sphere(-33.9, 151.2),
            90.0,
            16,
        ),
        // A transmitter inside the polar cap, where the paths to most of the
        // grid cross the auroral zone and the bearings converge.
        base("area/polar-14mhz-12z", whole_sphere(78.2, 15.6), 180.0, 13),
    ]
}

/// The area answer as comparable text: one line per point, coordinates
/// through the reference's own `f10.4` and every printed field after them.
///
/// Formatted rather than digested from the numbers directly, so a difference
/// too small to move a printed field is not reported as a disagreement — the
/// listing is what a caller sees, and it is what `areacheck` compares.
fn area_text(points: &[hfcast::voacap::run::AreaPoint]) -> String {
    points
        .iter()
        .map(|p| {
            format!(
                "{:3} {:3} {}{} {}\n",
                p.ix,
                p.iy,
                f_fixed(p.lat, 10, 4),
                f_fixed(p.print_lon, 10, 4),
                p.fields.join(" "),
            )
        })
        .collect()
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let full = args.iter().any(|a| a == "--full");
    let limit = args
        .iter()
        .position(|a| a == "--cases")
        .and_then(|i| args.get(i + 1))
        .and_then(|n| n.parse::<usize>().ok());

    let all = sweep_cases();
    let cases = match limit {
        Some(n) => &all[..n.min(all.len())],
        None => &all[..],
    };

    // The data files are read from the tree the other harnesses use. Under an
    // emulator this is the host's own path, so nothing is copied.
    let root = std::path::PathBuf::from(
        std::env::var("HFCAST_ITSHFBC").unwrap_or_else(|_| "itshfbc".to_string()),
    );
    // `<embedded>` is a root too, and is not a directory. Checking for a
    // directory first gives a clearer failure than a missing coefficient file
    // twelve stages later.
    let embedded = root.to_string_lossy().starts_with(EMBEDDED);
    if !embedded && !root.is_dir() {
        eprintln!("no itshfbc tree at {}", root.display());
        eprintln!("set HFCAST_ITSHFBC to one, or to {EMBEDDED}");
        return ExitCode::FAILURE;
    }

    let areas = area_cases();
    println!(
        "# archcheck: {} sweep cases and {} area grids on {} {}, data from {}",
        cases.len(),
        areas.len(),
        std::env::consts::ARCH,
        std::env::consts::OS,
        root.display(),
    );

    // Sequential on purpose: this runs under an emulator, where a thread pool
    // buys nothing and memory is the scarce thing.
    for case in cases {
        let inputs = RunInputs::from(case);
        let hours = match run(&root, &inputs) {
            Ok(h) => h,
            Err(e) => {
                eprintln!("{}: engine failed: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let text = listing_text(&hours, &body_lines(case.method, case.botlines.as_deref()));
        if full {
            println!("=== {}\n{text}", case.id);
        } else {
            println!("{:016x}  {}", digest(&text), case.id);
        }
    }

    for case in &areas {
        let points = match run_area(&root, &case.inputs) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("{}: area run failed: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let want = case.inputs.grid.nx * case.inputs.grid.ny;
        if points.len() != want {
            eprintln!("{}: {} points, wanted {want}", case.id, points.len());
            return ExitCode::FAILURE;
        }
        let text = area_text(&points);
        if full {
            println!("=== {}\n{text}", case.id);
        } else {
            println!("{:016x}  {}", digest(&text), case.id);
        }
    }

    ExitCode::SUCCESS
}
