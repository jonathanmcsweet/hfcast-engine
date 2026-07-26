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
use propcore::engine::ionosphere::{cofion, esind, layer_parameters, virtim, LayerParams};
use propcore::engine::muf::{curmuf, ionset, IonoState};
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

/// One dumped hour: the header value(s) and the numbers that follow, as
/// written by the VIRTIM trace (`VIR gmt` + 318 values) or the F2VAR
/// trace (`F2V gmt km` + one `PT` line of 16 values per control point).
struct HourTrace {
    gmt: f64,
    values: Vec<f64>,
    points: Vec<Vec<f64>>,
}

fn parse_hour_traces(text: &str, header: &str) -> Vec<HourTrace> {
    let mut out: Vec<HourTrace> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.first() {
            Some(&h) if h == header => out.push(HourTrace {
                gmt: fields.get(1).and_then(|v| v.parse().ok()).unwrap_or(f64::NAN),
                values: Vec::new(),
                points: Vec::new(),
            }),
            Some(&"PT") => {
                if let Some(current) = out.last_mut() {
                    current
                        .points
                        .push(fields[1..].iter().map(|v| v.parse().unwrap_or(f64::NAN)).collect());
                }
            }
            Some(_) => {
                if let Some(current) = out.last_mut() {
                    for f in &fields {
                        current.values.push(f.parse().unwrap_or(f64::NAN));
                    }
                }
            }
            None => {}
        }
    }
    out
}

/// The 16 dumped layer fields in trace order.
fn layer_fields(p: &LayerParams) -> [f64; 16] {
    [
        p.fi[0], p.fi[1], p.fi[2], p.yi[0], p.yi[1], p.yi[2], p.hi[0], p.hi[1], p.hi[2], p.f2m3,
        p.hpf2, p.rat, p.abiy, p.clck, p.zenang, p.zenmax,
    ]
    .map(f64::from)
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
    let mut ab_worst = Worst::new("AB time-evaluated coefficient");
    let mut iono_worst = [
        Worst::new("foE (MHz)"),
        Worst::new("foF1 (MHz)"),
        Worst::new("foF2 (MHz)"),
        Worst::new("E semithickness (km)"),
        Worst::new("F1 semithickness (km)"),
        Worst::new("F2 semithickness (km)"),
        Worst::new("E height (km)"),
        Worst::new("F1 height (km)"),
        Worst::new("F2 height (km)"),
        Worst::new("M(3000)F2"),
        Worst::new("hpF2 (km)"),
        Worst::new("height/semithickness ratio"),
        Worst::new("absorption index"),
        Worst::new("local time (h)"),
        Worst::new("zenith angle (deg)"),
        Worst::new("max F1 zenith (deg)"),
    ];
    let mut es_worst = [
        Worst::new("Es lower decile (MHz)"),
        Worst::new("Es median (MHz)"),
        Worst::new("Es upper decile (MHz)"),
        Worst::new("Es height (km)"),
    ];
    // FSECV is dumped too but not compared: its value on no-F1 hours is
    // whatever the previous hour's LUFFY lecden calls left behind (not
    // yet ported), and nothing in the method-30 path ever reads it — the
    // only reader is the ionogram plotter (ITRUN = 2).
    let mut lec_worst = [
        Worst::new("F1 critical after lecden (MHz)"),
        Worst::new("F1 semithickness after lecden (km)"),
        Worst::new("F1 height after lecden (km)"),
        Worst::new("profile height (km)"),
        Worst::new("profile density (MHz^2)"),
    ];
    let mut muf_worst = [
        Worst::new("E MUF (MHz)"),
        Worst::new("F1 MUF (MHz)"),
        Worst::new("F2 MUF (MHz)"),
        Worst::new("Es MUF (MHz)"),
        Worst::new("circuit MUF (MHz)"),
        Worst::new("FOT (MHz)"),
        Worst::new("HPF (MHz)"),
        Worst::new("MUF takeoff angle (deg)"),
    ];
    let mut muf_layer_worst = [
        Worst::new("layer sig lower"),
        Worst::new("layer sig upper"),
        Worst::new("layer angle (deg)"),
        Worst::new("layer virtual height (km)"),
        Worst::new("layer true height (km)"),
        Worst::new("layer vertical frequency (MHz)"),
        Worst::new("layer loss factor"),
        Worst::new("layer MUF lower decile (MHz)"),
        Worst::new("layer MUF median (MHz)"),
        Worst::new("layer MUF upper decile (MHz)"),
    ];
    let mut structural = 0usize;
    let mut compared = 0usize;
    let mut mag_points = 0usize;
    let mut red_points = 0usize;
    let mut ab_points = 0usize;
    let mut iono_points = 0usize;
    let mut es_points = 0usize;
    let mut lec_points = 0usize;
    let mut muf_hours = 0usize;

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

        // The ionosphere stage: one VIRTIM dump and one F2VAR dump per
        // hour, in the engine's hour order.
        let vir_dump = std::fs::read_to_string(trace_dir.join("virtim.txt")).unwrap_or_default();
        let f2_dump = std::fs::read_to_string(trace_dir.join("f2var.txt")).unwrap_or_default();
        let virs = parse_hour_traces(&vir_dump, "VIR");
        let f2s = parse_hour_traces(&f2_dump, "F2V");
        if virs.is_empty() || virs.len() != f2s.len() {
            eprintln!(
                "{}: {} VIRTIM dumps but {} F2VAR dumps",
                case.id,
                virs.len(),
                f2s.len()
            );
            structural += 1;
            continue;
        }
        let es_dump = std::fs::read_to_string(trace_dir.join("esind.txt")).unwrap_or_default();
        let ess = parse_hour_traces(&es_dump, "ESI");
        if ess.len() != virs.len() {
            eprintln!(
                "{}: {} VIRTIM dumps but {} ESIND dumps",
                case.id,
                virs.len(),
                ess.len()
            );
            structural += 1;
            continue;
        }
        let lec_dump = std::fs::read_to_string(trace_dir.join("lecden.txt")).unwrap_or_default();
        let lecs_all = parse_hour_traces(&lec_dump, "LEC");
        // LUFFY calls LECDEN again later in the hour; the first dump per
        // hour is the CURMUF one this stage compares.
        let mut lecs: Vec<&HourTrace> = Vec::new();
        for l in &lecs_all {
            if lecs.last().map(|p| p.gmt) != Some(l.gmt) {
                lecs.push(l);
            }
        }
        let muf_dump = std::fs::read_to_string(trace_dir.join("curmuf.txt")).unwrap_or_default();
        let mufs = parse_hour_traces(&muf_dump, "MUF");
        if mufs.len() != virs.len() || lecs.len() != virs.len() {
            eprintln!(
                "{}: {} VIRTIM dumps but {} CURMUF and {} LECDEN dumps",
                case.id,
                virs.len(),
                mufs.len(),
                lecs.len()
            );
            structural += 1;
            continue;
        }
        let cof = cofion(&set);
        let mags: Vec<_> = rust.points.iter().map(|p| magvar(p.lat, p.lon)).collect();
        let clats: Vec<f32> = rust.points.iter().map(|p| p.lat).collect();
        // FSECV lives in a COMMON block and carries across hours. The
        // carry here is partial — LUFFY's own lecden calls (next stage)
        // also write it — which is one reason FSECV is not compared.
        let mut fsecv_carry = [0.0f32; 3];
        for (((vir, f2h), esh), (lech, mufh)) in virs
            .iter()
            .zip(&f2s)
            .zip(&ess)
            .zip(lecs.iter().zip(&mufs))
        {
            let gmt = vir.gmt as f32;
            let ab = virtim(&cof, &set.ikim, gmt);
            for (ported, traced) in ab.iter().zip(&vir.values) {
                ab_worst.update((f64::from(*ported) - traced).abs(), &case.id);
            }
            ab_points += vir.values.len();

            let params = layer_parameters(
                &set,
                &ab,
                &rust.points,
                &mags,
                case.month,
                red.ssn as f32,
                f2h.gmt as f32,
                &[1.0, 1.0, 1.0, 1.0],
            );
            if f2h.points.len() != params.len() {
                eprintln!(
                    "{}: F2VAR dumped {} points, Rust computed {}",
                    case.id,
                    f2h.points.len(),
                    params.len()
                );
                structural += 1;
                continue;
            }
            for (ported, traced) in params.iter().zip(&f2h.points) {
                if traced.len() != 16 {
                    structural += 1;
                    continue;
                }
                iono_points += 1;
                for (worst, (r, t)) in iono_worst
                    .iter_mut()
                    .zip(layer_fields(ported).iter().zip(traced))
                {
                    worst.update((r - t).abs(), &case.id);
                }
            }

            let es = esind(&set, &ab, &rust.points, &mags, &[1.0, 1.0, 1.0, 1.0]);
            if esh.points.len() != es.len() {
                eprintln!(
                    "{}: ESIND dumped {} points, Rust computed {}",
                    case.id,
                    esh.points.len(),
                    es.len()
                );
                structural += 1;
                continue;
            }
            for (ported, traced) in es.iter().zip(&esh.points) {
                if traced.len() != 4 {
                    structural += 1;
                    continue;
                }
                es_points += 1;
                let fields = [
                    f64::from(ported.fs[0]),
                    f64::from(ported.fs[1]),
                    f64::from(ported.fs[2]),
                    f64::from(ported.hs),
                ];
                for (worst, (r, t)) in es_worst.iter_mut().zip(fields.iter().zip(traced)) {
                    worst.update((r - t).abs(), &case.id);
                }
            }

            // The MUF stage: ionset + curmuf (which runs lecden inside).
            let mut state = IonoState::from_layers(&params);
            state.fsecv = fsecv_carry;
            ionset(&mut state);
            let mut es_state = es.clone();
            let clcks: Vec<f32> = params.iter().map(|p| p.clck).collect();
            let hour = curmuf(
                &mut state,
                &mut es_state,
                &set.f2d,
                &clats,
                &clcks,
                rust.gcd,
                rust.gcd_km,
                0.1,
                red.ssn as f32,
            );
            fsecv_carry = state.fsecv;
            let scalars = &mufh.values;
            if scalars.len() != 10 || mufh.points.len() != 4 {
                eprintln!("{}: malformed CURMUF dump", case.id);
                structural += 1;
                continue;
            }
            if scalars[9] as usize != hour.ks + 1 || scalars[8] as i32 != hour.modmuf {
                eprintln!(
                    "{}: CURMUF chose ks {} mode {} but Rust chose {} {}",
                    case.id,
                    scalars[9],
                    scalars[8],
                    hour.ks + 1,
                    hour.modmuf
                );
                structural += 1;
                continue;
            }
            muf_hours += 1;
            let muf_fields = [
                f64::from(hour.emuf),
                f64::from(hour.f1muf),
                f64::from(hour.f2muf),
                f64::from(hour.esmuf),
                f64::from(hour.allmuf),
                f64::from(hour.fot),
                f64::from(hour.hpf),
                f64::from(hour.angmuf),
            ];
            for (worst, (r, t)) in muf_worst.iter_mut().zip(muf_fields.iter().zip(scalars)) {
                worst.update((r - t).abs(), &case.id);
            }
            for (layer, traced) in hour.layers.iter().zip(&mufh.points) {
                if traced.len() != 11 {
                    structural += 1;
                    continue;
                }
                if traced[7] as i32 != layer.nhopmf {
                    eprintln!(
                        "{}: layer hop count {} vs {}",
                        case.id, traced[7], layer.nhopmf
                    );
                    structural += 1;
                    continue;
                }
                let fields = [
                    f64::from(layer.sigl),
                    f64::from(layer.sigu),
                    f64::from(layer.delmuf),
                    f64::from(layer.hpmuf),
                    f64::from(layer.htmuf),
                    f64::from(layer.fvmuf),
                    f64::from(layer.afmuf),
                    f64::from(layer.yfot),
                    f64::from(layer.ymuf),
                    f64::from(layer.yhpf),
                ];
                let traced_fields = [
                    traced[0], traced[1], traced[2], traced[3], traced[4], traced[5], traced[6],
                    traced[8], traced[9], traced[10],
                ];
                for (worst, (r, t)) in muf_layer_worst
                    .iter_mut()
                    .zip(fields.iter().zip(&traced_fields))
                {
                    worst.update((r - t).abs(), &case.id);
                }
            }

            // The profile from the CURMUF-time LECDEN call.
            let lv = &lech.values;
            if lv.len() != 104 {
                eprintln!("{}: malformed LECDEN dump ({} values)", case.id, lv.len());
                structural += 1;
                continue;
            }
            lec_points += 1;
            let ks = hour.ks;
            let f1_fields = [
                f64::from(state.fi[ks][1]),
                f64::from(state.yi[ks][1]),
                f64::from(state.hi[ks][1]),
            ];
            for (worst, (r, t)) in lec_worst.iter_mut().zip(f1_fields.iter().zip(&lv[0..3])) {
                worst.update((r - t).abs(), &case.id);
            }
            for i in 0..50 {
                lec_worst[3].update((f64::from(state.htr[i]) - lv[4 + i]).abs(), &case.id);
                lec_worst[4].update((f64::from(state.fnsq[i]) - lv[54 + i]).abs(), &case.id);
            }
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

    println!("\n## Stage: ionosphere (virtim, versy, ef1var, timvar, f2var)\n");
    println!(
        "Compared {ab_points} AB coefficients and {iono_points} control-point-hours.\n"
    );
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    println!(
        "| {} | {:.2e} | {} |",
        ab_worst.name, ab_worst.value, ab_worst.case
    );
    for w in &iono_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: sporadic E parameters (esind.for)\n");
    println!("Compared {es_points} control-point-hours.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &es_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: MUF (ionset, lecden, gethp, f2dis, curmuf)\n");
    println!("Compared {muf_hours} hours and {lec_points} density profiles.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in muf_worst.iter().chain(&muf_layer_worst).chain(&lec_worst) {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    if structural > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
