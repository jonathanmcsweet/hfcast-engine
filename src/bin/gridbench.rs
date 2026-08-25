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
use hfcast::voacap::fastmath::Numerics;
use hfcast::voacap::model::Model;
use hfcast::voacap::run::{run_area, AreaInputs};

struct Config {
    nx: usize,
    ny: usize,
    threads: Vec<usize>,
    parity: bool,
    /// Which deviations from the reference's arithmetic the run takes.
    /// One at a time is what sizes a deviation on the device, which is
    /// the only machine whose maths library makes them worth having.
    numerics: Numerics,
    /// The set to compare against, cell by cell, instead of timing.
    against: Option<Numerics>,
    month: u32,
    hour: i32,
    ssn: f32,
}

fn world(cfg: &Config, numerics: Numerics) -> AreaInputs {
    AreaInputs {
        numerics,
        grid: Grid {
            projection: Projection::LatLon,
            plat: 47.0,
            plon: 8.0,
            xmin: -180.0,
            xmax: 180.0,
            ymin: -90.0,
            ymax: 90.0,
            nx: cfg.nx,
            ny: cfg.ny,
        },
        tx_lat_deg: 47.0,
        tx_lon_deg: 8.0,
        month: cfg.month,
        ssn: cfg.ssn,
        hour: cfg.hour,
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

/// One command-line value as whatever type the caller wants.
fn number<T: std::str::FromStr>(value: &Option<String>) -> Option<T> {
    value.as_deref().and_then(|s| s.parse().ok())
}

fn parse_args() -> Result<Config, String> {
    let mut cfg = Config {
        nx: 240,
        ny: 144,
        threads: vec![1, 2, 4, 8, 0],
        parity: true,
        numerics: Numerics::shipping(),
        against: None,
        month: 6,
        hour: 13,
        ssn: 80.0,
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
            "--numerics" => {
                let list = value.unwrap_or_default();
                cfg.numerics = Numerics::from_names(&list).map_err(|name| {
                    format!(
                        "unknown deviation {name:?}, one of: {}, {}",
                        Numerics::NAMES.join(", "),
                        Numerics::LATTICE_NAMES,
                    )
                })?;
            }
            "--against" => {
                let list = value.unwrap_or_default();
                cfg.against = Some(Numerics::from_names(&list).map_err(|name| {
                    format!(
                        "unknown deviation {name:?}, one of: {}, {}",
                        Numerics::NAMES.join(", "),
                        Numerics::LATTICE_NAMES,
                    )
                })?);
            }
            // Parsed here rather than through `parse`, whose one closure
            // takes a single type and is already fixed to `usize`.
            "--month" => cfg.month = number(&value).unwrap_or(cfg.month),
            "--hour" => cfg.hour = number(&value).unwrap_or(cfg.hour),
            "--ssn" => cfg.ssn = number(&value).unwrap_or(cfg.ssn),
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

/// The reliability contours a coverage overlay is drawn at.
///
/// A cell that lands on the other side of one of these is drawn in a
/// different band, which is the error an operator acts on: the map is
/// read to find an area that is open when the wanted one is not, so an
/// area on the wrong side of a contour sends somebody to call into a
/// band that is shut, or keeps them off one that is working.
const CONTOURS: [f64; 5] = [0.10, 0.25, 0.50, 0.75, 0.90];

/// How far apart two sets of cells landed.
///
/// The median and the 99th are here because the distribution has a long
/// tail: a mean or a root mean square over 34,560 cells is set by a
/// handful of them and says nothing about the map a person looks at.
struct Spread {
    p50: f64,
    p99: f64,
    worst: f64,
}

fn spread(diffs: impl Iterator<Item = f64>) -> Spread {
    let mut sorted: Vec<f64> = diffs.collect();
    sorted.sort_by(f64::total_cmp);
    let at = |q: f64| match sorted.len() {
        0 => 0.0,
        n => sorted[((n - 1) as f64 * q).round() as usize],
    };
    Spread {
        p50: at(0.50),
        p99: at(0.99),
        worst: at(1.0),
    }
}

/// What the two overlays disagree about.
struct Coverage {
    /// Cells whose reliability differs once printed to the two decimals
    /// the server consumes. The strictest reading of "the map changed".
    moved: f64,
    /// Cells that changed side at each contour, and at any of them.
    crossed: Vec<f64>,
    any: f64,
}

fn coverage(base: &[f32], taken: &[f32]) -> Coverage {
    let n = base.len().max(1) as f64;
    let printed = |v: f32| (f64::from(v) * 100.0).round();
    let side = |v: f32, t: f64| f64::from(v) >= t;
    let share = |count: usize| 100.0 * count as f64 / n;
    let pairs = || base.iter().zip(taken);
    Coverage {
        moved: share(
            pairs()
                .filter(|(a, b)| printed(**a) != printed(**b))
                .count(),
        ),
        crossed: CONTOURS
            .iter()
            .map(|t| {
                share(
                    pairs()
                        .filter(|(a, b)| side(**a, *t) != side(**b, *t))
                        .count(),
                )
            })
            .collect(),
        any: share(
            pairs()
                .filter(|(a, b)| CONTOURS.iter().any(|t| side(**a, *t) != side(**b, *t)))
                .count(),
        ),
    }
}

/// Runs the same grid under two sets of arithmetic and reports how far
/// apart the overlays land.
///
/// This is the ruler a map deviation has. The WSPR and ionosonde
/// harnesses both ask point-to-point questions, so neither reaches this
/// driver, and neither would answer this question if it did: a coverage
/// map is read for which areas are open, so it has to be judged on
/// areas, not on how close one cell's signal number is.
fn compare_run(
    root: &std::path::Path,
    cfg: &Config,
    against: Numerics,
    header: bool,
) -> Result<(), String> {
    let run = |numerics| {
        predict_grid(
            root,
            &GridRequest {
                area: world(cfg, numerics),
                threads: 0,
            },
        )
    };
    let (taken, base) = (run(cfg.numerics)?, run(against)?);
    let cover = coverage(&base.reliability, &taken.reliability);
    let rel = spread(
        base.reliability
            .iter()
            .zip(&taken.reliability)
            .map(|(a, b)| f64::from(a - b).abs()),
    );
    // Signal strength is read where there is a signal to read. A cell
    // the map draws as dead carries whatever the mode search left
    // behind, and nobody looks at it.
    let live = spread(
        base.reliability
            .iter()
            .zip(&base.snr_db)
            .zip(&taken.snr_db)
            .filter(|((r, _), _)| f64::from(**r) >= 0.10)
            .map(|((_, a), b)| f64::from(a - b).abs()),
    );
    if header {
        println!(
            "\n| month | hour | cells moved | rel p50 | rel p99 | rel worst | \
             {} | any contour | live dB p99 | live dB worst |",
            CONTOURS
                .iter()
                .map(|t| format!("cross {t:.2}"))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        println!("|{}", " --- |".repeat(9 + CONTOURS.len()));
    }
    println!(
        "| {} | {} | {:.2}% | {:.4} | {:.4} | {:.4} | {} | {:.2}% | {:.2} | {:.2} |",
        cfg.month,
        cfg.hour,
        cover.moved,
        rel.p50,
        rel.p99,
        rel.worst,
        cover
            .crossed
            .iter()
            .map(|c| format!("{c:.2}%"))
            .collect::<Vec<_>>()
            .join(" | "),
        cover.any,
        live.p99,
        live.worst,
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
            area: world(cfg, cfg.numerics),
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
    let area = world(&cfg, cfg.numerics);
    let taken = cfg.numerics.names();
    println!(
        "lattice {} x {} = {} points, {} band(s), hour {}, deviations: {}",
        cfg.nx,
        cfg.ny,
        cfg.nx * cfg.ny,
        area.freqs_mhz.len(),
        area.hour,
        if taken.is_empty() {
            "none, the reference's arithmetic".to_string()
        } else {
            taken.join(", ")
        }
    );

    if let Some(against) = cfg.against {
        let baseline = against.names();
        println!(
            "against: {}",
            if baseline.is_empty() {
                "none, the reference's arithmetic".to_string()
            } else {
                baseline.join(", ")
            }
        );
        if let Err(e) = compare_run(&root, &cfg, against, true) {
            eprintln!("predict_grid: {e}");
            return ExitCode::FAILURE;
        }
        return ExitCode::SUCCESS;
    }

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
