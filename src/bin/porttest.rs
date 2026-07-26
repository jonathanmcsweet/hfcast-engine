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
use propcore::engine::coefficients::{redmap, FoF2Model};
use propcore::engine::con::MagneticPole;
use propcore::engine::geometry::{path_geometry, PathGeometry};
use propcore::engine::magnetic::magvar;
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

/// The Fortran side of one REDMAP call, parsed from the trace dump: the
/// header values and each labelled array's elements in storage order.
struct RedmapTrace {
    ssn: f64,
    month: u32,
    arrays: Vec<(String, Vec<f64>)>,
}

/// Parses the first REDMAP dump in the file (later dumps repeat the same
/// month for the deck's other method calls).
fn parse_redmap_trace(text: &str) -> Option<RedmapTrace> {
    let mut trace: Option<RedmapTrace> = None;
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.first() {
            Some(&"REDMAP") if fields.len() == 3 => {
                if trace.is_some() {
                    break; // only the first dump
                }
                trace = Some(RedmapTrace {
                    ssn: fields[1].parse().ok()?,
                    month: fields[2].parse().ok()?,
                    arrays: Vec::new(),
                });
            }
            Some(&"ARR") if fields.len() == 2 => {
                trace
                    .as_mut()?
                    .arrays
                    .push((fields[1].to_string(), Vec::new()));
            }
            Some(_) => {
                if let Some((_, values)) = trace.as_mut().and_then(|t| t.arrays.last_mut()) {
                    for f in &fields {
                        values.push(f.parse().unwrap_or(f64::NAN));
                    }
                }
            }
            None => {}
        }
    }
    trace
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
    let mut mag_worst = [
        Worst::new("gyrofrequency (MHz)"),
        Worst::new("Rawer dip (rad)"),
        Worst::new("east longitude (rad)"),
    ];
    let mut red_worst: Vec<Worst> = Vec::new();
    let mut structural = 0usize;
    let mut compared = 0usize;
    let mut mag_points = 0usize;
    let mut red_points = 0usize;

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

        // The magnetic stage: MAGVAR is called once per control point in
        // the same order, so the dumps line up with the Rust points.
        let mag_dump = std::fs::read_to_string(trace_dir.join("magvar.txt")).unwrap_or_default();
        let mags: Vec<Vec<f64>> = mag_dump
            .lines()
            .filter(|l| l.starts_with("MAG "))
            .map(|l| {
                l.split_whitespace()
                    .skip(1)
                    .map(|t| t.parse().unwrap_or(f64::NAN))
                    .collect()
            })
            .collect();
        for (r, f) in rust.points.iter().zip(&mags) {
            if f.len() != 7 {
                continue;
            }
            let rust_mag = magvar(r.lat, r.lon);
            mag_points += 1;
            mag_worst[0].update((rust_mag.gyz as f64 - f[2]).abs(), &case.id);
            mag_worst[1].update((rust_mag.gmdip as f64 - f[3]).abs(), &case.id);
            mag_worst[2].update((rust_mag.east_lon as f64 - f[1]).abs(), &case.id);
        }

        // The coefficient stage: REDMAP runs once per month group, and the
        // sweep decks have one month each, so the first dump is the one.
        let red_dump = std::fs::read_to_string(trace_dir.join("redmap.txt")).unwrap_or_default();
        let Some(red) = parse_redmap_trace(&red_dump) else {
            eprintln!("{}: no REDMAP trace in the dump", case.id);
            return ExitCode::FAILURE;
        };
        if red.month != case.month || (red.ssn - case.ssn).abs() > 1e-4 {
            eprintln!(
                "{}: REDMAP ran month {} ssn {} but the deck says {} {}",
                case.id, red.month, red.ssn, case.month, case.ssn
            );
            structural += 1;
            continue;
        }
        let set = match redmap(root.path(), FoF2Model::Ccir, red.month, red.ssn as f32) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("{}: coefficient load failed: {e}", case.id);
                return ExitCode::FAILURE;
            }
        };
        let flat = set.flattened();
        if red_worst.is_empty() {
            red_worst = flat.iter().map(|(name, _)| Worst::new(name)).collect();
        }
        if red.arrays.len() != flat.len() {
            eprintln!(
                "{}: {} arrays in the trace, {} in Rust",
                case.id,
                red.arrays.len(),
                flat.len()
            );
            structural += 1;
            continue;
        }
        for (index, ((trace_name, trace_values), (rust_name, rust_values))) in
            red.arrays.iter().zip(&flat).enumerate()
        {
            if trace_name != rust_name || trace_values.len() != rust_values.len() {
                eprintln!(
                    "{}: array {index} is {trace_name}[{}] in the trace, {rust_name}[{}] in Rust",
                    case.id,
                    trace_values.len(),
                    rust_values.len()
                );
                structural += 1;
                continue;
            }
            for (traced, ported) in trace_values.iter().zip(rust_values) {
                red_worst[index].update((ported - traced).abs(), &case.id);
            }
            red_points += trace_values.len();
        }
    }

    println!("## Stage: geometry (geom.for)\n");
    println!("Compared {compared} cases, {structural} structural disagreements.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: coefficient loading (redmap.for)\n");
    println!("Compared {red_points} array elements.\n");
    println!("| array | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &red_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: magnetic field (magvar.for, magfin.for)\n");
    println!("Compared {mag_points} control points.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &mag_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    if structural > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
