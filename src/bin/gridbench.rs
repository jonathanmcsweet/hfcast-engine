//! Times the nowcast grid driver against the parity area driver.
//!
//! The lattice defaults to the application's fine globe: the whole
//! world at 1.25 by 1.5 degrees, 34,560 points, one band. Needs the
//! embedded coefficients:
//! `cargo run --release --all-features --bin gridbench -- [--nx N]
//!  [--ny N] [--threads N,N,...]`

use std::process::ExitCode;
use std::time::Instant;

use hfcast::nowcast::grid::{predict_grid, GridRequest};
use hfcast::voacap::area::{Grid, Projection};
use hfcast::voacap::coefficients::FoF2Model;
use hfcast::voacap::data;
use hfcast::voacap::model::Model;
use hfcast::voacap::run::{run_area, AreaInputs};

fn world(nx: usize, ny: usize) -> AreaInputs {
    AreaInputs {
        grid: Grid {
            projection: Projection::LatLon,
            plat: 47.0,
            plon: 8.0,
            xmin: -180.0,
            xmax: 180.0,
            ymin: -90.0,
            ymax: 90.0,
            nx,
            ny,
        },
        tx_lat_deg: 47.0,
        tx_lon_deg: 8.0,
        month: 6,
        ssn: 80.0,
        hour: 13,
        freqs_mhz: vec![7.1],
        required_snr_db: 24.0,
        noise_dbw: 145,
        watts: 100.0,
        psc: [1.0, 1.0, 1.0, 0.0],
        method: 30,
        fof2: FoF2Model::Ccir,
        inverse: false,
        tx_antenna: None,
        rx_antenna: None,
        model: Model::Compatible,
    }
}

fn main() -> ExitCode {
    let mut nx = 240usize;
    let mut ny = 144usize;
    let mut threads = vec![1usize, 2, 4, 8, 0];
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next();
        let parse = |v: &Option<String>| v.as_deref().and_then(|s| s.parse().ok());
        match arg.as_str() {
            "--nx" => nx = parse(&value).unwrap_or(nx),
            "--ny" => ny = parse(&value).unwrap_or(ny),
            "--threads" => {
                threads = value
                    .as_deref()
                    .map(|list| list.split(',').filter_map(|t| t.parse().ok()).collect())
                    .unwrap_or(threads);
            }
            other => {
                eprintln!("unknown argument {other}");
                return ExitCode::FAILURE;
            }
        }
    }

    let root = data::embedded_root();
    let area = world(nx, ny);
    println!(
        "lattice {nx} x {ny} = {} points, {} band(s), hour {}",
        nx * ny,
        area.freqs_mhz.len(),
        area.hour
    );

    let start = Instant::now();
    let parity = run_area(&root, &area);
    let parity_ms = start.elapsed().as_millis();
    match parity {
        Ok(points) => println!(
            "parity run_area (serial, carried state): {parity_ms} ms, {} points",
            points.len()
        ),
        Err(e) => {
            eprintln!("run_area: {e}");
            return ExitCode::FAILURE;
        }
    }

    let mut serial_ms = 0u128;
    // A loop to print each measurement as it lands.
    for t in &threads {
        let req = GridRequest {
            area: world(nx, ny),
            threads: *t,
        };
        let start = Instant::now();
        match predict_grid(&root, &req) {
            Ok(_) => {
                let ms = start.elapsed().as_millis();
                if *t == 1 {
                    serial_ms = ms;
                }
                let scaling = if *t != 1 && serial_ms > 0 {
                    format!(", {:.1}x over one thread", serial_ms as f64 / ms as f64)
                } else {
                    String::new()
                };
                let label = if *t == 0 {
                    "all".to_string()
                } else {
                    t.to_string()
                };
                println!("nowcast predict_grid, {label} thread(s): {ms} ms{scaling}");
            }
            Err(e) => {
                eprintln!("predict_grid: {e}");
                return ExitCode::FAILURE;
            }
        }
    }
    ExitCode::SUCCESS
}
