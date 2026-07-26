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
//! Usage: `porttest [--cases N] [--only ID] [--seed N] [--fuzz N [--from N]]`

use std::path::PathBuf;
use std::process::ExitCode;

use propcore::deck::build_deck;
use propcore::engine::coefficients::{redmap, FoF2Model};
use propcore::engine::con::MagneticPole;
use propcore::engine::ionosphere::{cofion, esind, layer_parameters, virtim, LayerParams};
use propcore::engine::con::D2R;
use propcore::engine::ionogram::{alosfv, fobby, genion, sang, selmod};
use propcore::engine::ionosphere::{alatd, geotim, ground_constants};
use propcore::engine::noise::{anois1, genois};
use propcore::engine::modes::{
    es_slots, luffy_freq_loop, luffy_smooth, outbod_sentinels, setlng, AllModes, DeckParams,
    FreqDebug, Geog, HourSaves, ModeLoopState, PassCtx, Son, Zon,
};
use propcore::engine::muf::{curmuf, ionset, lecden, IonoState};
use propcore::engine::sigdis::sigdis;
use propcore::engine::geometry::{path_geometry, PathGeometry};
use propcore::engine::magnetic::magvar;
use propcore::runner::{run_deck_with_env, variant_bin, IsolatedRoot};
use propcore::fuzz::fuzz_cases;
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
    /// Ground constants (conductivity, dielectric) per point.
    grounds: Vec<[f64; 2]>,
    alatd: f64,
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
                    grounds: Vec::new(),
                    alatd: f64::NAN,
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
            Some(&"GC") if fields.len() == 3 => {
                if let Some(current) = out.last_mut() {
                    let parse = |i: usize| fields[i].parse::<f64>().unwrap_or(f64::NAN);
                    current.grounds.push([parse(1), parse(2)]);
                }
            }
            Some(&"AL") if fields.len() == 2 => {
                if let Some(current) = out.last_mut() {
                    current.alatd = fields[1].parse().unwrap_or(f64::NAN);
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
    /// Second to fourth header fields where the dump has them (area
    /// index, angle count, row count and similar), NaN otherwise.
    h2: f64,
    h3: f64,
    h4: f64,
    values: Vec<f64>,
    points: Vec<Vec<f64>>,
}

fn parse_hour_traces(text: &str, header: &str) -> Vec<HourTrace> {
    let mut out: Vec<HourTrace> = Vec::new();
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        match fields.first() {
            Some(&h) if h == header => {
                let field = |i: usize| {
                    fields
                        .get(i)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(f64::NAN)
                };
                out.push(HourTrace {
                    gmt: field(1),
                    h2: field(2),
                    h3: field(3),
                    h4: field(4),
                    values: Vec::new(),
                    points: Vec::new(),
                });
            }
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

/// The dump code of a `LAYTYP` two-character label for a layer index
/// (zero: the label bytes are still NUL, the program-start state).
fn laytyp_code(layer: i32) -> f64 {
    match layer {
        1 => (32 * 256 + 69) as f64,  // " E"
        2 => (70 * 256 + 49) as f64,  // "F1"
        3 => (70 * 256 + 50) as f64,  // "F2"
        4 => (69 * 256 + 83) as f64,  // "ES"
        5 => (32 * 256 + 78) as f64,  // " N"
        6 => (78 * 256 + 65) as f64,  // "NA", the OUTBOD sentinel
        _ => 0.0,
    }
}

/// The float fields of a `/SON/` slot in dump order (NHP, NREL and the
/// layer labels are compared as integers).
fn son_fields(son: &Son) -> [f64; 24] {
    [
        son.angle,
        son.angler,
        son.cprob,
        son.dblos,
        son.dblosl,
        son.dblosu,
        son.dbu,
        son.delay,
        son.dbw,
        son.xnynois,
        son.probmp,
        son.reliab,
        son.sndb,
        son.snpr,
        son.snrlw,
        son.snrup,
        son.sprob,
        son.vhigh,
        son.rneff,
        son.snxx,
        son.gaint,
        son.gainr,
        son.du_nois,
        son.dl_nois,
    ]
    .map(f64::from)
}

/// Dump indices of the [`son_fields`] floats (NHP sits at 9).
const SON_IDX: [usize; 24] = [
    0, 1, 2, 3, 4, 5, 6, 7, 8, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24,
];

/// One `/ZON/` slot's float fields in dump order plus its mode integer.
fn zon_row(z: &Zon, i: usize) -> ([f64; 22], i32) {
    (
        [
            z.abps[i], z.crel[i], z.eff[i], z.fldst[i], z.grlos[i], z.hn[i], z.hp[i], z.prob[i],
            z.rely[i], z.rgain[i], z.sigpow[i], z.sn[i], z.spro[i], z.tgain[i], z.timed[i],
            z.tloss[i], z.b[i], z.fslos[i], z.adv[i], z.obf[i], z.tllow[i], z.tlhgh[i],
        ]
        .map(f64::from),
        z.nmode[i],
    )
}

/// One accumulated-mode slot's float fields in dump order plus its mode.
fn amd_row(a: &AllModes, i: usize) -> ([f64; 22], i32) {
    (
        [
            a.abps[i], a.crel[i], a.eff[i], a.fldst[i], a.grlos[i], a.hn[i], a.hp[i], a.prob[i],
            a.rely[i], a.rgain[i], a.sigpow[i], a.sn[i], a.spro[i], a.tgain[i], a.timed[i],
            a.tloss[i], a.b[i], a.fslos[i], a.adv[i], a.obf[i], a.tllow[i], a.tlhgh[i],
        ]
        .map(f64::from),
        a.nmode[i],
    )
}

/// Everything [`compare_freq_debug`] needs: the frequency's Rust
/// intermediates, the parsed dump streams with their cursors, and the
/// worst-trackers.
struct CompareFreq<'a> {
    dbg: FreqDebug,
    expected_pass: i32,
    gmt: f64,
    case_id: &'a str,
    rfxs: &'a [HourTrace],
    zons_t: &'a [HourTrace],
    amds_t: &'a [HourTrace],
    loss_t: &'a [HourTrace],
    sons_t: &'a [HourTrace],
    rfx_index: &'a mut usize,
    zon_index: &'a mut usize,
    amd_index: &'a mut usize,
    los_index: &'a mut usize,
    son_index: &'a mut usize,
    rfx_worst: &'a mut [Worst],
    zon_worst: &'a mut [Worst],
    amd_worst: &'a mut [Worst],
    los_worst: &'a mut [Worst],
    son_worst: &'a mut [Worst],
    rfx_calls: &'a mut usize,
    zon_calls: &'a mut usize,
    amd_calls: &'a mut usize,
    los_calls: &'a mut usize,
    son_calls: &'a mut usize,
    structural: &'a mut usize,
}

/// Compares one frequency slot's mode-loop intermediates against the
/// dumps: the reflectrix (RFX), the per-hop mode slots (ZON), the
/// accumulated modes (AMD), the long-path loss tables (LOS) and the
/// final /SON/ slot (SON).
fn compare_freq_debug(c: CompareFreq) {
    let CompareFreq {
        dbg,
        expected_pass,
        gmt,
        case_id,
        rfxs,
        zons_t,
        amds_t,
        loss_t,
        sons_t,
        rfx_index,
        zon_index,
        amd_index,
        los_index,
        son_index,
        rfx_worst,
        zon_worst,
        amd_worst,
        los_worst,
        son_worst,
        rfx_calls,
        zon_calls,
        amd_calls,
        los_calls,
        son_calls,
        structural,
    } = c;
    for snap in &dbg.rfx {
        let Some(rt) = rfxs.get(*rfx_index) else {
            eprintln!("{case_id}: ran out of RFX dumps");
            *structural += 1;
            break;
        };
        *rfx_index += 1;
        if rt.gmt != gmt
            || rt.h2 as i32 != snap.khz
            || rt.h3 as usize != snap.k + 1
            || rt.h4 as usize != snap.rows.len()
        {
            eprintln!(
                "{case_id}: RFX dump gmt {} area {} rows {} where Rust has {} {} {}",
                rt.gmt,
                rt.h3,
                rt.h4,
                snap.k + 1,
                snap.rows.len(),
                snap.khz,
            );
            *structural += 1;
            continue;
        }
        let expect_len = snap.rows.len() * 7 + 2 + if snap.skip.is_some() { 5 } else { 0 } + 3;
        if rt.values.len() != expect_len {
            eprintln!("{case_id}: malformed RFX dump");
            *structural += 1;
            continue;
        }
        *rfx_calls += 1;
        for (ri, row) in snap.rows.iter().enumerate() {
            let base = ri * 7;
            for j in 0..6 {
                rfx_worst[j].update((f64::from(row[j]) - rt.values[base + j]).abs(), case_id);
            }
            if row[6] as i32 != rt.values[base + 6] as i32 {
                *structural += 1;
            }
        }
        let mut base = snap.rows.len() * 7;
        rfx_worst[6].update((f64::from(snap.dskpkm) - rt.values[base]).abs(), case_id);
        rfx_worst[7].update((f64::from(snap.dmaxkm) - rt.values[base + 1]).abs(), case_id);
        base += 2;
        if let Some(skip) = snap.skip {
            for j in 0..4 {
                rfx_worst[8 + j].update((f64::from(skip[j]) - rt.values[base + j]).abs(), case_id);
            }
            if skip[4] as i32 != rt.values[base + 4] as i32 {
                *structural += 1;
            }
            base += 5;
        }
        for j in 0..3 {
            rfx_worst[12].update(
                (f64::from(snap.delpen[j]) - rt.values[base + j]).abs(),
                case_id,
            );
        }
    }
    for (hopid, zon) in &dbg.zons {
        let Some(zt) = zons_t.get(*zon_index) else {
            eprintln!("{case_id}: ran out of ZON dumps");
            *structural += 1;
            break;
        };
        *zon_index += 1;
        if zt.gmt != gmt || zt.h2 as i32 != dbg.khz || zt.h3 as i32 != *hopid {
            eprintln!(
                "{case_id}: ZON dump hop {} at {} kHz where Rust ran hop {} at {} kHz",
                zt.h3, zt.h2, hopid, dbg.khz
            );
            *structural += 1;
            continue;
        }
        if zt.values.len() != 7 * 23 {
            eprintln!("{case_id}: malformed ZON dump");
            *structural += 1;
            continue;
        }
        *zon_calls += 1;
        for i in 0..7 {
            let (f, nm) = zon_row(zon, i);
            let base = i * 23;
            for j in 0..20 {
                zon_worst[j].update((f[j] - zt.values[base + j]).abs(), case_id);
            }
            zon_worst[20].update((f[20] - zt.values[base + 21]).abs(), case_id);
            zon_worst[21].update((f[21] - zt.values[base + 22]).abs(), case_id);
            if nm != zt.values[base + 20] as i32 {
                *structural += 1;
            }
        }
    }
    if let Some(all) = &dbg.amd {
        let Some(at) = amds_t.get(*amd_index) else {
            eprintln!("{case_id}: ran out of AMD dumps");
            *structural += 1;
            return;
        };
        *amd_index += 1;
        if at.gmt != gmt || at.h2 as i32 != dbg.khz || at.h3 as usize != all.nmmod {
            eprintln!(
                "{case_id}: AMD dump has {} modes at {} kHz where Rust has {} at {}",
                at.h3, at.h2, all.nmmod, dbg.khz
            );
            *structural += 1;
        } else {
            let ndmp = at.h4 as usize;
            if at.values.len() != ndmp * 23 {
                eprintln!("{case_id}: malformed AMD dump");
                *structural += 1;
            } else {
                *amd_calls += 1;
                for i in 0..ndmp {
                    let (f, nm) = amd_row(all, i);
                    let base = i * 23;
                    for j in 0..20 {
                        amd_worst[j].update((f[j] - at.values[base + j]).abs(), case_id);
                    }
                    amd_worst[20].update((f[20] - at.values[base + 21]).abs(), case_id);
                    amd_worst[21].update((f[21] - at.values[base + 22]).abs(), case_id);
                    if nm != at.values[base + 20] as i32 {
                        *structural += 1;
                    }
                }
            }
        }
    }
    if let Some(los) = &dbg.los {
        let Some(lt) = loss_t.get(*los_index) else {
            eprintln!("{case_id}: ran out of LOS dumps");
            *structural += 1;
            return;
        };
        *los_index += 1;
        if lt.gmt != gmt
            || lt.h2 as i32 != dbg.khz
            || lt.h3 as i32 != los.ltxrgm[0]
            || lt.h4 as i32 != los.ltxrgm[1]
        {
            eprintln!(
                "{case_id}: SELTXR chose rows {} {} where Rust chose {} {}",
                lt.h3, lt.h4, los.ltxrgm[0], los.ltxrgm[1]
            );
            *structural += 1;
        } else if lt.values.len() != 450 {
            eprintln!("{case_id}: malformed LOS dump");
            *structural += 1;
        } else {
            *los_calls += 1;
            for (jj, (_, rows)) in los.ends.iter().enumerate() {
                for (i, row) in rows.iter().enumerate() {
                    let base = jj * 225 + i * 5;
                    for j in 0..5 {
                        los_worst[j].update((f64::from(row[j]) - lt.values[base + j]).abs(), case_id);
                    }
                }
            }
        }
    }
    {
        let Some(st) = sons_t.get(*son_index) else {
            eprintln!("{case_id}: ran out of SON dumps");
            *structural += 1;
            return;
        };
        *son_index += 1;
        if st.gmt != gmt
            || st.h2 as i32 != dbg.khz
            || st.h3 as i32 != expected_pass
            || st.values.len() != 28
        {
            eprintln!(
                "{case_id}: SON dump pass {} at {} kHz where Rust ran pass {} at {}",
                st.h3, st.h2, expected_pass, dbg.khz
            );
            *structural += 1;
            return;
        }
        *son_calls += 1;
        let f = son_fields(&dbg.son);
        for (j, w) in son_worst.iter_mut().enumerate() {
            w.update((f[j] - st.values[SON_IDX[j]]).abs(), case_id);
        }
        if dbg.son.nhp != st.values[9] as i32
            || dbg.nrel as i32 != st.values[25] as i32
            || laytyp_code(dbg.son.mode_layer) != st.values[26]
            || laytyp_code(dbg.son.moder_layer) != st.values[27]
        {
            eprintln!(
                "{case_id}: SON integer mismatch at {} kHz pass {expected_pass}: nhp {} vs {}, nrel {} vs {}, mode {} vs {}, moder {} vs {}",
                dbg.khz,
                dbg.son.nhp,
                st.values[9],
                dbg.nrel,
                st.values[25],
                laytyp_code(dbg.son.mode_layer),
                st.values[26],
                laytyp_code(dbg.son.moder_layer),
                st.values[27],
            );
            *structural += 1;
        }
    }
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

    let only = argv
        .iter()
        .position(|a| a == "--only")
        .and_then(|i| argv.get(i + 1))
        .cloned();
    // `fuzz` reports a case index when the whole-engine listings
    // disagree, but the listing only says which cell is wrong. Running
    // that same case through the stage traces says which stage made it
    // wrong, so the index is accepted here too.
    let number = |name: &str| -> Option<u64> {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .and_then(|v| v.parse::<u64>().ok())
    };
    let seed = number("--seed");
    // `--fuzz N` runs N generated cases through the stage traces. The
    // whole-engine check cannot see a difference the listing does not
    // print, so the generated corpus needs a pass at this level too.
    let fuzz_count = number("--fuzz");
    let from = number("--from").unwrap_or(0);
    let cases: Vec<_> = match (seed, fuzz_count) {
        (Some(index), _) => fuzz_cases(index, 1),
        (None, Some(n)) => fuzz_cases(from, n),
        (None, None) => sweep_cases()
            .into_iter()
            .filter(|c| only.as_ref().is_none_or(|o| c.id.contains(o.as_str())))
            .take(case_limit)
            .collect(),
    };
    match (seed, fuzz_count) {
        (Some(index), _) => println!("# Port stage check: generated case {index}\n"),
        (None, Some(n)) => println!(
            "# Port stage check: {n} generated cases, indices {from}..{}\n",
            from + n - 1
        ),
        (None, None) => println!("# Port stage check: {} sweep cases\n", cases.len()),
    }

    let mut worst = [
        Worst::new("distance (km)"),
        Worst::new("bearing to receiver (deg)"),
        Worst::new("bearing to transmitter (deg)"),
        Worst::new("control point distance (rad)"),
        Worst::new("control point latitude (deg)"),
        Worst::new("control point longitude (deg)"),
        Worst::new("geomagnetic latitude (deg)"),
        Worst::new("ground conductivity (mhos/m)"),
        Worst::new("ground dielectric"),
        Worst::new("path latitude ALATD (deg)"),
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
    let mut noi_worst: Vec<Worst> = [
        "combined noise (dBW)",
        "noise upper decile",
        "noise lower decile",
        "noise error median",
        "noise error upper",
        "noise error lower",
        "atmospheric (dBW)",
        "galactic (dBW)",
        "man-made (dBW)",
        "3 MHz noise (ZNOISE)",
        "receiver efficiency",
        "1 MHz block noise (ATNU)",
        "1 MHz neighbour noise (ATNY)",
    ]
    .iter()
    .map(|n| Worst::new(n))
    .collect();
    let mut sig_worst: Vec<Worst> = [
        "DSL",
        "ASM",
        "DSU",
        "AGLAT",
        "ACAV",
        "FEAV",
        "AFE",
        "BFE",
        "HNU",
        "HTLOSS",
        "XNUZ",
        "XVE",
        "ADJ",
        "SU",
        "SL",
        "ADS",
        "SUS",
        "SLS",
        "ABIY",
        "ARTIC",
    ]
    .iter()
    .map(|n| Worst::new(n))
    .collect();
    let mut ion_worst = [
        Worst::new("sounding frequency (MHz)"),
        Worst::new("ionogram virtual height (km)"),
        Worst::new("ionogram true height (km)"),
        Worst::new("deviative loss factor"),
        Worst::new("reflectrix frequency (kHz)"),
    ];
    const ZON_NAMES: [&str; 22] = [
        "ABPS", "CREL", "EFF", "FLDST", "GRLOS", "HN", "HP", "PROB", "RELY", "RGAIN", "SIGPOW",
        "SN", "SPRO", "TGAIN", "TIMED", "TLOSS", "B", "FSLOS", "ADV", "OBF", "TLLOW", "TLHGH",
    ];
    const SON_NAMES: [&str; 24] = [
        "ANGLE", "ANGLER", "CPROB", "DBLOS", "DBLOSL", "DBLOSU", "DBU", "DELAY", "DBW",
        "XNYNOIS", "PROBMP", "RELIAB", "SNDB", "SNPR", "SNRLW", "SNRUP", "SPROB", "VHIGH",
        "RNEFF", "SNXX", "GAINT", "GAINR", "DU_NOIS", "DL_NOIS",
    ];
    let mut rfx_worst: Vec<Worst> = [
        "takeoff angle (deg)",
        "virtual height (km)",
        "true height (km)",
        "ground distance (km)",
        "vertical frequency (MHz)",
        "loss factor",
        "skip distance (km)",
        "maximum distance (km)",
        "skip angle (deg)",
        "skip virtual height (km)",
        "skip true height (km)",
        "skip vertical frequency (MHz)",
        "penetration angle (deg)",
    ]
    .iter()
    .map(|n| Worst::new(n))
    .collect();
    let mut zon_worst: Vec<Worst> = ZON_NAMES.iter().map(|n| Worst::new(n)).collect();
    let mut amd_worst: Vec<Worst> = ZON_NAMES.iter().map(|n| Worst::new(n)).collect();
    let mut los_worst: Vec<Worst> = ["ANDVX", "ADVX", "AOFX", "GRLOSX", "TGAINX"]
        .iter()
        .map(|n| Worst::new(n))
        .collect();
    let mut son_worst: Vec<Worst> = SON_NAMES.iter().map(|n| Worst::new(n)).collect();
    let mut smo_worst: Vec<Worst> = SON_NAMES.iter().map(|n| Worst::new(n)).collect();
    let mut structural = 0usize;
    let mut compared = 0usize;
    let mut mag_points = 0usize;
    let mut red_points = 0usize;
    let mut ab_points = 0usize;
    let mut iono_points = 0usize;
    let mut es_points = 0usize;
    let mut lec_points = 0usize;
    let mut muf_hours = 0usize;
    let mut ion_calls = 0usize;
    let mut sig_calls = 0usize;
    let mut noi_calls = 0usize;
    let mut rfx_calls = 0usize;
    let mut zon_calls = 0usize;
    let mut amd_calls = 0usize;
    let mut los_calls = 0usize;
    let mut son_calls = 0usize;
    let mut smo_calls = 0usize;

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
        let ion_dump = std::fs::read_to_string(trace_dir.join("ionogram.txt")).unwrap_or_default();
        let ions = parse_hour_traces(&ion_dump, "ION");
        if only.is_some() {
            let heads: Vec<String> = ions
                .iter()
                .take(8)
                .map(|t| format!("(gmt {} area {})", t.gmt, t.h2))
                .collect();
            eprintln!("{}: {} ION dumps: {}", case.id, ions.len(), heads.join(" "));
        }
        let fob_dump = std::fs::read_to_string(trace_dir.join("fobby.txt")).unwrap_or_default();
        let fobs = parse_hour_traces(&fob_dump, "FOB");
        let sig_dump = std::fs::read_to_string(trace_dir.join("sigdis.txt")).unwrap_or_default();
        let sigs = parse_hour_traces(&sig_dump, "SIG");
        let noi_dump = std::fs::read_to_string(trace_dir.join("genois.txt")).unwrap_or_default();
        let nois = parse_hour_traces(&noi_dump, "NOI");
        let rfx_dump = std::fs::read_to_string(trace_dir.join("findf.txt")).unwrap_or_default();
        let rfxs = parse_hour_traces(&rfx_dump, "RFX");
        let zon_dump = std::fs::read_to_string(trace_dir.join("zon.txt")).unwrap_or_default();
        let zons_t = parse_hour_traces(&zon_dump, "ZON");
        let amd_dump = std::fs::read_to_string(trace_dir.join("amd.txt")).unwrap_or_default();
        let amds_t = parse_hour_traces(&amd_dump, "AMD");
        let los_dump = std::fs::read_to_string(trace_dir.join("los.txt")).unwrap_or_default();
        let loss_t = parse_hour_traces(&los_dump, "LOS");
        let son_dump = std::fs::read_to_string(trace_dir.join("son.txt")).unwrap_or_default();
        let sons_t = parse_hour_traces(&son_dump, "SON");
        let smo_dump = std::fs::read_to_string(trace_dir.join("smo.txt")).unwrap_or_default();
        let smos_t = parse_hour_traces(&smo_dump, "SMO");
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
        // Ground constants and path latitude, dumped with the geometry.
        let grounds = ground_constants(&set, &rust.points, &mags);
        if fortran.grounds.len() == grounds.len() {
            for ((sig, eps), t) in grounds.iter().zip(&fortran.grounds) {
                worst[7].update((f64::from(*sig) - t[0]).abs(), &case.id);
                worst[8].update((f64::from(*eps) - t[1]).abs(), &case.id);
            }
        } else {
            eprintln!(
                "{}: {} ground-constant dumps for {} points",
                case.id,
                fortran.grounds.len(),
                grounds.len()
            );
            structural += 1;
        }
        worst[9].update(
            (f64::from(alatd(&rust.points)) - fortran.alatd).abs(),
            &case.id,
        );
        let clats: Vec<f32> = rust.points.iter().map(|p| p.lat).collect();
        // FSECV lives in a COMMON block and carries across hours; the
        // ionogram chain's lecden calls update it after curmuf's.
        let mut fsecv_carry = [0.0f32; 3];
        let mut ion_index = 0usize;
        let mut sig_index = 0usize;
        let mut noi_index = 0usize;
        let mut rfx_index = 0usize;
        let mut zon_index = 0usize;
        let mut amd_index = 0usize;
        let mut los_index = 0usize;
        let mut son_index = 0usize;
        let mut smo_index = 0usize;
        let nang = sang(rust.gcd_km, 0.1);
        // The mode loop's persistent COMMON blocks, one per case.
        let mut lp = ModeLoopState::default();
        // The sweep is isotropic at both ends; the fuzz corpus covers
        // directional antennas through the whole-engine check instead.
        let ants = propcore::engine::antenna::AntennaSet::isotropes(case.watts as f32);
        let deck_params = DeckParams {
            amind: 0.1,
            rsn: case.required_snr_db as f32,
            lufp: 90,
            pmp: 3.0,
            dmp: 0.1,
        };
        let mut base_frel = [0.0f32; 12];
        for (slot, f) in base_frel.iter_mut().zip(&case.freqs_mhz) {
            *slot = *f as f32;
        }
        // The FPROB card's critical-frequency multipliers. The fourth
        // is sporadic E, and driving it from the case is what makes an
        // Es-off deck comparable: with the multiplier at zero every
        // control point has foEs = 0, so CURMUF skips them all and
        // zeroes the Es layer.
        let psc = [
            1.0,
            1.0,
            1.0,
            if case.sporadic_e { 1.0 } else { 0.0 },
        ];
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
                &psc,
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

            let es = esind(&set, &ab, &rust.points, &mags, &psc);
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
            for (index, (layer, traced)) in hour.layers.iter().zip(&mufh.points).enumerate() {
                if traced.len() != 11 {
                    structural += 1;
                    continue;
                }
                if traced[7] as i32 != layer.nhopmf {
                    // Layers are 1 E, 2 F1, 3 F2, 4 Es, as in /MUFS/.
                    eprintln!(
                        "{}: hour {} layer {} hop count {} vs {}",
                        case.id,
                        vir.gmt,
                        index + 1,
                        traced[7],
                        layer.nhopmf
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

            // Receiver-site noise, needed by the mode loop and the
            // per-frequency GENOIS comparison below.
            let times = geotim(
                vir.gmt as i32,
                1,
                case.from_lon as f32 * D2R,
                case.to_lon as f32 * D2R,
            );
            let an = anois1(
                &set,
                times.gmtr,
                case.to_lat as f32 * D2R,
                case.to_lon as f32 * D2R,
                case.to_lon as f32,
            );
            let fof2_end = state.fi[state.kfx - 1][2];
            let noise_for = |f: f32| {
                genois(
                    ants.gain(2, 0.0, f).1,
                    &set,
                    &an,
                    f,
                    case.to_lat as f32 * D2R,
                    fof2_end,
                    case.noise_dbw as i32,
                )
            };

            // The LUFFY passes for method 30 (MSPEC=121): the short-path
            // model below 7000 km; short then long (and the smoothing
            // blend) from 7000 to 10000 km; the long-path model alone
            // beyond. Each pass recomputes the areas the previous pass
            // did not cover, replicates the sample-area arrays (SETLNG),
            // runs SIGDIS and then the frequency loop.
            let jmode = selmod(&state);
            struct PassPlan {
                long: bool,
                areas: Vec<usize>,
            }
            let plans: Vec<PassPlan> = if rust.gcd_km > 10000.0 {
                vec![PassPlan {
                    long: true,
                    areas: if state.kfx > 1 {
                        vec![0, state.kfx - 1]
                    } else {
                        vec![0]
                    },
                }]
            } else if rust.gcd_km >= 7000.0 {
                let mut p2 = Vec::new();
                if jmode != 0 {
                    p2.push(0);
                }
                if state.kfx > 1 && jmode != state.kfx - 1 {
                    p2.push(state.kfx - 1);
                }
                vec![
                    PassPlan {
                        long: false,
                        areas: vec![jmode],
                    },
                    PassPlan {
                        long: true,
                        areas: p2,
                    },
                ]
            } else {
                vec![PassPlan {
                    long: false,
                    areas: vec![jmode],
                }]
            };
            let (mut fs, mut hs) = es_slots(&es_state);
            let mut geog = Geog::from_points(&params, &mags, &grounds);
            let mut hour_m = hour.clone();
            let mut saves = HourSaves::default();
            let glats: Vec<f32> = rust.points.iter().map(|p| p.gmlat).collect();
            let mut frel = base_frel;
            frel[11] = hour.allmuf;
            let mut sd_last = None;
            for plan in &plans {
                for &k in &plan.areas {
                    let (Some(ionh), Some(fobh)) = (ions.get(ion_index), fobs.get(ion_index))
                    else {
                        eprintln!("{}: ran out of ionogram dumps", case.id);
                        structural += 1;
                        break;
                    };
                    ion_index += 1;
                    if ionh.h2 as usize != k + 1
                        || fobh.h2 as usize != k + 1
                        || fobh.h3 as usize != nang
                        || ionh.values.len() != 120
                        || fobh.points.len() != nang
                    {
                        eprintln!(
                            "{}: ionogram dump for area {} angles {} where Rust ran {} {}",
                            case.id,
                            ionh.h2,
                            fobh.h3,
                            k + 1,
                            nang
                        );
                        structural += 1;
                        continue;
                    }
                    lecden(&mut state, k);
                    let mut ion = genion(&state, k);
                    let table = fobby(&ion, nang);
                    alosfv(&state, k, &mut ion, &hour.layers);
                    ion_calls += 1;
                    for i in 0..30 {
                        ion_worst[0]
                            .update((f64::from(ion.fvert[i]) - ionh.values[i]).abs(), &case.id);
                        ion_worst[1].update(
                            (f64::from(ion.hprim[i]) - ionh.values[30 + i]).abs(),
                            &case.id,
                        );
                        ion_worst[2].update(
                            (f64::from(ion.htrue[i]) - ionh.values[60 + i]).abs(),
                            &case.id,
                        );
                        ion_worst[3].update(
                            (f64::from(ion.afac[i]) - ionh.values[90 + i]).abs(),
                            &case.id,
                        );
                    }
                    for (row, traced_row) in table.iter().zip(&fobh.points) {
                        for (r, t) in row.iter().zip(traced_row) {
                            ion_worst[4].update((f64::from(*r) - t).abs(), &case.id);
                        }
                    }
                    lp.areas[k].update(ion, &table);
                }
                setlng(&mut state, &mut fs, &mut hs, &mut geog, &mut lp.areas);
                sd_last = Some(sigdis(
                    &set,
                    &state,
                    &hour,
                    &lp.areas[jmode].ion,
                    &glats,
                    &clcks,
                    jmode,
                    rust.gcd_km,
                ));
                let sd = sd_last.as_ref().expect("just set");
                if let Some(sigh) = sigs.get(sig_index) {
                    sig_index += 1;
                    let kfx = sigh.h2 as usize;
                    if kfx != state.kfx || sigh.values.len() != 18 + 2 * kfx {
                        eprintln!("{}: malformed SIGDIS dump", case.id);
                        structural += 1;
                    } else {
                        sig_calls += 1;
                        let fields = [
                            f64::from(sd.dsl),
                            f64::from(sd.asm),
                            f64::from(sd.dsu),
                            f64::from(sd.aglat),
                            f64::from(sd.acav),
                            f64::from(sd.feav),
                            f64::from(sd.afe),
                            f64::from(sd.bfe),
                            f64::from(sd.hnu),
                            f64::from(sd.htloss),
                            f64::from(sd.xnuz),
                            f64::from(sd.xve),
                            f64::from(sd.adj),
                            f64::from(sd.su),
                            f64::from(sd.sl),
                            f64::from(sd.ads),
                            f64::from(sd.sus),
                            f64::from(sd.sls),
                        ];
                        for (worst, (r, t)) in
                            sig_worst.iter_mut().zip(fields.iter().zip(&sigh.values))
                        {
                            worst.update((r - t).abs(), &case.id);
                        }
                        for k in 0..kfx {
                            sig_worst[18].update(
                                (f64::from(sd.abiy[k]) - sigh.values[18 + k]).abs(),
                                &case.id,
                            );
                            sig_worst[19].update(
                                (f64::from(sd.artic[k]) - sigh.values[18 + kfx + k]).abs(),
                                &case.id,
                            );
                        }
                    }
                } else {
                    eprintln!("{}: ran out of SIGDIS dumps", case.id);
                    structural += 1;
                }
                geog.apply_sigdis(sd);
                let ctx = PassCtx {
                    state: &state,
                    ants: &ants,
                    fs: &fs,
                    hs: &hs,
                    geog: &geog,
                    sig: sd,
                    deck: deck_params,
                    gcd: rust.gcd,
                    gcdkm: rust.gcd_km,
                    jmode,
                    nang,
                    long: plan.long,
                };
                let dbgs = luffy_freq_loop(&mut lp, &ctx, &mut hour_m, &noise_for, &frel, &mut saves);
                let expected_pass = if plan.long { 2 } else { 1 };
                for dbg in dbgs.into_iter().flatten() {
                    compare_freq_debug(CompareFreq {
                        dbg,
                        expected_pass,
                        gmt: vir.gmt,
                        case_id: &case.id,
                        rfxs: &rfxs,
                        zons_t: &zons_t,
                        amds_t: &amds_t,
                        loss_t: &loss_t,
                        sons_t: &sons_t,
                        rfx_index: &mut rfx_index,
                        zon_index: &mut zon_index,
                        amd_index: &mut amd_index,
                        los_index: &mut los_index,
                        son_index: &mut son_index,
                        rfx_worst: &mut rfx_worst,
                        zon_worst: &mut zon_worst,
                        amd_worst: &mut amd_worst,
                        los_worst: &mut los_worst,
                        son_worst: &mut son_worst,
                        rfx_calls: &mut rfx_calls,
                        zon_calls: &mut zon_calls,
                        amd_calls: &mut amd_calls,
                        los_calls: &mut los_calls,
                        son_calls: &mut son_calls,
                        structural: &mut structural,
                    });
                }
            }
            if plans.len() == 2 {
                let sd = sd_last.as_ref().expect("two passes ran");
                let ctx = PassCtx {
                    state: &state,
                    ants: &ants,
                    fs: &fs,
                    hs: &hs,
                    geog: &geog,
                    sig: sd,
                    deck: deck_params,
                    gcd: rust.gcd,
                    gcdkm: rust.gcd_km,
                    jmode,
                    nang,
                    long: true,
                };
                let smos = luffy_smooth(&mut lp, &ctx, &noise_for, &frel, &saves);
                for sm in smos.into_iter().flatten() {
                    let Some(st) = smos_t.get(smo_index) else {
                        eprintln!("{}: ran out of SMO dumps", case.id);
                        structural += 1;
                        break;
                    };
                    smo_index += 1;
                    if st.gmt != vir.gmt || st.h2 as i32 != sm.khz || st.values.len() != 27 {
                        eprintln!("{}: SMO dump mismatch at {} kHz", case.id, sm.khz);
                        structural += 1;
                        continue;
                    }
                    if st.h3 as i32 != i32::from(sm.son.mdl) {
                        eprintln!(
                            "{}: smoothing chose model {} where Rust chose {}",
                            case.id, st.h3, sm.son.mdl as char
                        );
                        structural += 1;
                        continue;
                    }
                    smo_calls += 1;
                    let f = son_fields(&sm.son);
                    for (j, w) in smo_worst.iter_mut().enumerate() {
                        w.update((f[j] - st.values[SON_IDX[j]]).abs(), &case.id);
                    }
                    if sm.son.nhp != st.values[9] as i32
                        || laytyp_code(sm.son.mode_layer) != st.values[25]
                        || laytyp_code(sm.son.moder_layer) != st.values[26]
                    {
                        eprintln!("{}: SMO integer mismatch at {} kHz", case.id, sm.khz);
                        structural += 1;
                    }
                }
            }
            // OUTBOD's high-MUF sentinels, applied after the hour's
            // output; the next hour's stale reads see them.
            outbod_sentinels(&mut lp.son, hour.allmuf);
            fsecv_carry = state.fsecv;

            // The noise stage: GENOIS runs per frequency (at least
            // twice each) in the LUFFY loop; every dump of this hour is
            // compared at its own dumped frequency.
            while noi_index < nois.len() && nois[noi_index].gmt == vir.gmt {
                let noih = &nois[noi_index];
                noi_index += 1;
                if noih.h2 as usize != an.kj
                    || noih.h3 as usize != an.jk
                    || noih.values.len() != 16
                {
                    eprintln!(
                        "{}: GENOIS blocks {} {} where Rust chose {} {}",
                        case.id, noih.h2, noih.h3, an.kj, an.jk
                    );
                    structural += 1;
                    continue;
                }
                noi_calls += 1;
                let freq = noih.values[0] as f32;
                let nr = genois(
                    ants.gain(2, 0.0, freq).1,
                    &set,
                    &an,
                    freq,
                    case.to_lat as f32 * D2R,
                    state.fi[state.kfx - 1][2],
                    case.noise_dbw as i32,
                );
                let fields = [
                    f64::from(nr.rcnse),
                    f64::from(nr.du),
                    f64::from(nr.dl),
                    f64::from(nr.sigm),
                    f64::from(nr.sygu),
                    f64::from(nr.sygl),
                    f64::from(nr.atnos),
                    f64::from(nr.gnos),
                    f64::from(nr.xnois),
                    f64::from(nr.znoise),
                    f64::from(nr.eff),
                    f64::from(an.atnu),
                    f64::from(an.atny),
                ];
                for (worst, (r, t)) in noi_worst
                    .iter_mut()
                    .zip(fields.iter().zip(&noih.values[1..14]))
                {
                    worst.update((r - t).abs(), &case.id);
                }
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

    println!("\n## Stage: ionogram tables (sang, selmod, genion, fobby, alosfv)\n");
    println!("Compared {ion_calls} area calls.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &ion_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: signal distribution (syssy, prbmuf, sigdis)\n");
    println!("Compared {sig_calls} calls.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &sig_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: noise (anois1, genfam, genois)\n");
    println!("Compared {noi_calls} calls.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &noi_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: mode loop (penang, findf, fdist, inmuf, regmod, esmod, esreg, allmodes)\n");
    println!(
        "Compared {rfx_calls} reflectrix builds, {zon_calls} hop-mode dumps and {amd_calls} accumulated-mode dumps.\n"
    );
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &rfx_worst {
        println!("| reflectrix {} | {:.2e} | {} |", w.name, w.value, w.case);
    }
    for w in &zon_worst {
        println!("| ZON {} | {:.2e} | {} |", w.name, w.value, w.case);
    }
    for w in &amd_worst {
        println!("| modes {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: long path (gmloss, settxr, seltxr, lngpat)\n");
    println!("Compared {los_calls} two-end loss tables.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &los_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    println!("\n## Stage: reliability and output fields (relbil, serprb, mpath, smoothing)\n");
    println!("Compared {son_calls} frequency slots and {smo_calls} smoothed slots.\n");
    println!("| field | worst difference | case |");
    println!("| --- | --: | --- |");
    for w in &son_worst {
        println!("| {} | {:.2e} | {} |", w.name, w.value, w.case);
    }
    for w in &smo_worst {
        println!("| smoothed {} | {:.2e} | {} |", w.name, w.value, w.case);
    }

    if structural > 0 {
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
