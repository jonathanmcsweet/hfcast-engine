//! Compares each ported engine stage against the instrumented Fortran.
//!
//! `tools/build-trace.sh` builds the `trace` variant: the reference engine
//! plus dump statements at every ported stage boundary, active only when
//! `PROPCORE_TRACE` names a directory. This program runs that binary over
//! the sweep cases, parses the dumps, computes the same stage in Rust, and
//! reports the worst disagreement per field. A port error shows up here as
//! a disagreement in the *first* stage that contains it, not as a mystery
//! at the end of the pipeline.
//!
//! Usage: `porttest [--cases N]`

use std::path::PathBuf;
use std::process::ExitCode;

use propcore::deck::build_deck;
use propcore::engine::con::MagneticPole;
use propcore::engine::geometry::{path_geometry, PathGeometry};
use propcore::runner::{run_deck_with_env, variant_bin, IsolatedRoot};
use propcore::sweep::sweep_cases;

/// One worst-case tracker per compared field.
struct Worst {
    name: &'static str,
    value: f64,
    case: String,
}

impl Worst {
    fn new(name: &'static str) -> Self {
        Self {
            name,
            value: 0.0,
            case: String::new(),
        }
    }

    fn update(&mut self, difference: f64, case: &str) {
        if difference > self.value {
            self.value = difference;
            self.case = case.to_string();
        }
    }
}

/// The Fortran side of one GEOM call, parsed from the trace dump.
struct GeomTrace {
    gcd_km: f64,
    btr_deg: f64,
    brt_deg: f64,
    points: Vec<[f64; 4]>,
}

fn parse_geom_trace(text: &str) -> Vec<GeomTrace> {
    let mut out: Vec<GeomTrace> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.first() {
            Some(&"GEOM") if fields.len() == 5 => {
                let parse = |i: usize| fields[i].parse::<f64>().unwrap_or(f64::NAN);
                out.push(GeomTrace {
                    gcd_km: parse(1),
                    btr_deg: parse(2),
                    brt_deg: parse(3),
                    points: Vec::new(),
                });
            }
            Some(&"CP") if fields.len() == 5 => {
                if let Some(current) = out.last_mut() {
                    let parse = |i: usize| fields[i].parse::<f64>().unwrap_or(f64::NAN);
                    current
                        .points
                        .push([parse(1), parse(2), parse(3), parse(4)]);
                }
            }
            _ => {}
        }
    }
    out
}

fn main() -> ExitCode {
    let argv: Vec<String> = std::env::args().collect();
    let case_limit = argv
        .iter()
        .position(|a| a == "--cases")
        .and_then(|i| argv.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(usize::MAX);

    let trace_bin = variant_bin("trace");
    if !trace_bin.is_file() {
        eprintln!("no trace variant; run tools/build-trace.sh");
        return ExitCode::FAILURE;
    }

    let cases: Vec<_> = sweep_cases().into_iter().take(case_limit).collect();
    println!("# Port stage check: {} sweep cases\n", cases.len());

    let mut worst = [
        Worst::new("distance (km)"),
        Worst::new("bearing to receiver (deg)"),
        Worst::new("bearing to transmitter (deg)"),
        Worst::new("control point distance (rad)"),
        Worst::new("control point latitude (deg)"),
        Worst::new("control point longitude (deg)"),
        Worst::new("geomagnetic latitude (deg)"),
    ];
    let mut structural = 0usize;
    let mut compared = 0usize;

    for case in &cases {
        let deck = match build_deck(case) {
            Ok(d) => d,
            Err(e) => {
                eprintln!("{}: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let root = match IsolatedRoot::create(&format!("pt-{}", case.id)) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("{}: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let trace_dir = root.path().join("trace-out");
        if let Err(e) = std::fs::create_dir_all(&trace_dir) {
            eprintln!("{}: {e}", case.id);
            return ExitCode::FAILURE;
        }
        let trace_path: PathBuf = trace_dir.clone();
        if let Err(e) = run_deck_with_env(
            &trace_bin,
            root.path(),
            &deck,
            &[("PROPCORE_TRACE", &trace_path.to_string_lossy())],
        ) {
            eprintln!("{}: engine failed: {e}", case.id);
            return ExitCode::FAILURE;
        }
        let dump = std::fs::read_to_string(trace_dir.join("geom.txt")).unwrap_or_default();
        let traces = parse_geom_trace(&dump);
        let Some(fortran) = traces.first() else {
            eprintln!("{}: no GEOM trace in the dump", case.id);
            return ExitCode::FAILURE;
        };

        let pole = MagneticPole::for_tree(root.path());
        let rust: PathGeometry = path_geometry(
            case.from_lat as f32,
            case.from_lon as f32,
            case.to_lat as f32,
            case.to_lon as f32,
            false,
            pole,
        );

        if rust.points.len() != fortran.points.len() {
            eprintln!(
                "{}: control point count {} vs {}",
                case.id,
                rust.points.len(),
                fortran.points.len()
            );
            structural += 1;
            continue;
        }
        compared += 1;

        worst[0].update((rust.gcd_km as f64 - fortran.gcd_km).abs(), &case.id);
        worst[1].update((rust.btr_deg() as f64 - fortran.btr_deg).abs(), &case.id);
        worst[2].update((rust.brt_deg() as f64 - fortran.brt_deg).abs(), &case.id);
        for (r, f) in rust.points.iter().zip(&fortran.points) {
            worst[3].update((r.rd as f64 - f[0]).abs(), &case.id);
            worst[4].update((r.lat as f64 * 57.295779513 - f[1]).abs(), &case.id);
            // Longitude differences wrap at the date line.
            let mut dlon = (r.lon as f64 * 57.295779513 - f[2]).abs();
            if dlon > 180.0 {
                dlon = 360.0 - dlon;
            }
            worst[5].update(dlon, &case.id);
            worst[6].update((r.gmlat as f64 * 57.295779513 - f[3]).abs(), &case.id);
        }
    }

    println!("## Stage: geometry (geom.for)\n");
    println!("Compared {compared} cases, {structural} structural disagreements.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    if structural > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
