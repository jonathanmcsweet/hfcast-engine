//! Times the truecast grid driver against the parity area driver.
//!
//! The lattice defaults to the application's fine globe: the whole
//! world at 1.25 by 1.5 degrees, 34,560 points, one band. Needs the
//! embedded coefficients:
//! `cargo run --release --all-features --bin gridbench -- [--nx N]
//!  [--ny N] [--threads N,N,...] [--parity 0|1]`
//!
//! With `HFCAST_PERF` set, a per-stage timing table follows the runs.
//! `--parity 0` skips the parity driver so the table covers the
//! truecast driver alone.

use std::process::ExitCode;
use std::time::Instant;

use hfcast::truecast::grid::{predict_grid, GridRequest};
use hfcast::voacap::area::{Grid, Projection};
use hfcast::voacap::coefficients::FoF2Model;
use hfcast::voacap::data;
use hfcast::voacap::model::Model;
use hfcast::voacap::run::{run_area, AreaInputs};

struct Config {
    nx: usize,
    ny: usize,
    threads: Vec<usize>,
    parity: bool,
}

fn world(nx: usize, ny: usize) -> AreaInputs {
    AreaInputs {
        // The truecast driver, so truecast numerics.
        numerics: hfcast::voacap::fastmath::Numerics::Truecast,
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

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config {
        nx: 240,
        ny: 144,
        threads: vec![1, 2, 4, 8, 0],
        parity: true,
    };
    let mut args = std::env::args().skip(1);
    // A loop because each flag consumes the argument after it.
    while let Some(arg) = args.next() {
        let value = args.next();
        let parse = |v: &Option<String>| v.as_deref().and_then(|s| s.parse().ok());
        match arg.as_str() {
            "--nx" => cfg.nx = parse(&value).unwrap_or(cfg.nx),
            "--ny" => cfg.ny = parse(&value).unwrap_or(cfg.ny),
            "--parity" => cfg.parity = parse(&value).map(|n: usize| n != 0).unwrap_or(cfg.parity),
            "--threads" => {
                cfg.threads = value
                    .as_deref()
                    .map(|list| list.split(',').filter_map(|t| t.parse().ok()).collect())
                    .unwrap_or(cfg.threads);
            }
            other => return Err(format!("unknown argument {other}")),
        }
    }
    Ok(cfg)
}

/// Runs and reports the parity driver once, serially.
fn parity_run(root: &std::path::Path, area: &AreaInputs) -> Result<(), String> {
    let start = Instant::now();
    let points = run_area(root, area)?;
    println!(
        "parity run_area (serial, carried state): {} ms, {} points",
        start.elapsed().as_millis(),
        points.len()
    );
    Ok(())
}

/// Runs the truecast driver at each requested thread count and reports
/// the scaling over one thread.
fn bench_threads(root: &std::path::Path, cfg: &Config) -> Result<(), String> {
    let mut serial_ms = 0u128;
    // A loop to print each measurement as it lands.
    for t in &cfg.threads {
        let req = GridRequest {
            area: world(cfg.nx, cfg.ny),
            threads: *t,
        };
        let start = Instant::now();
        predict_grid(root, &req)?;
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
        println!("truecast predict_grid, {label} thread(s): {ms} ms{scaling}");
    }
    Ok(())
}

fn main() -> ExitCode {
    let cfg = match parse_args() {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let root = data::embedded_root();
    let area = world(cfg.nx, cfg.ny);
    println!(
        "lattice {} x {} = {} points, {} band(s), hour {}",
        cfg.nx,
        cfg.ny,
        cfg.nx * cfg.ny,
        area.freqs_mhz.len(),
        area.hour
    );

    if std::env::var_os("HFCAST_PERF").is_some() {
        hfcast::perf::enable();
    }
    if cfg.parity {
        if let Err(e) = parity_run(&root, &area) {
            eprintln!("run_area: {e}");
            return ExitCode::FAILURE;
        }
    }
    let whole = Instant::now();
    if let Err(e) = bench_threads(&root, &cfg) {
        eprintln!("predict_grid: {e}");
        return ExitCode::FAILURE;
    }
    if hfcast::perf::enabled() {
        eprint!("{}", hfcast::perf::report(whole.elapsed()));
    }
    ExitCode::SUCCESS
}
