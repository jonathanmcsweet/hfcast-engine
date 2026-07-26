//! Compares VOACAP against the ITU-R P.533 reference implementation.
//!
//! These are two different models, not two implementations of one model, so
//! this measures disagreement rather than error. Neither engine is the truth
//! here. Saying which is more accurate needs measured reception reports, which
//! this program does not have.
//!
//! Only quantities that mean the same thing in both engines are reported as
//! directly comparable:
//!
//! - **MUF** depends on the path and the ionosphere alone. VOACAP prints one
//!   median MUF per hour; P.533 distinguishes a basic MUF from an operational
//!   MUF, so both are compared.
//! - **Dominant mode** is a discrete label in both.
//!
//! Signal power is reported separately and marked indicative. Both runs use
//! isotropic antennas and the same transmit power, but the two engines define
//! their reference points differently. Signal-to-noise ratio and reliability
//! are left out: P.533 takes man-made noise as a named environment over a
//! stated bandwidth, VOACAP takes a number at 3 MHz, and no exact mapping
//! exists between them.
//!
//! Usage: `cargo run --release --bin engines -- [--itu <checkout>]`

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Instant;

use propcore::deck::build_deck;
use propcore::itu::{parse_report, run_case, ItuPaths, ItuRow};
use propcore::listing::{parse_listing, ParsedListing, MUF_ROW, MUF_SLOT};
use propcore::runner::{map_limit, run_deck, variant_bin, IsolatedRoot};
use propcore::sweep::{sweep_cases, AMATEUR_FREQS_MHZ};

/// The VOACAP build used as the comparison point.
const VOACAP_VARIANT: &str = "O2";

const CONCURRENCY: usize = 4;

/// Frequencies are echoed to three decimals, so exact equality will not do.
const FREQ_EPSILON: f64 = 1e-3;

/// Below this, a printed signal power is a dead-path sentinel rather than a
/// measurement. VOACAP prints values like -1982 dBW when nothing propagates,
/// and averaging those with real values produces nonsense.
const PLAUSIBLE_DBW: f64 = -250.0;

/// P.533 prints this when it finds no propagating mode at all.
const NO_MODE: &str = "NONE";

struct Summary {
    n: usize,
    mean: f64,
    median: f64,
    p05: f64,
    p95: f64,
    max_abs: f64,
}

fn summarise(values: &[f64]) -> Summary {
    if values.is_empty() {
        return Summary {
            n: 0,
            mean: 0.0,
            median: 0.0,
            p05: 0.0,
            p95: 0.0,
            max_abs: 0.0,
        };
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).expect("differences are never NaN"));
    let at = |q: f64| -> f64 {
        let rank = (q * sorted.len() as f64).ceil() as usize;
        sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
    };
    Summary {
        n: sorted.len(),
        mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
        median: at(0.50),
        p05: at(0.05),
        p95: at(0.95),
        max_abs: sorted.iter().fold(0.0f64, |m, v| m.max(v.abs())),
    }
}

fn row(label: &str, unit: &str, s: &Summary) -> String {
    format!(
        "| {label} | {} | {:+.2} | {:+.2} | {:+.2} | {:+.2} | {:.2} | {unit} |",
        s.n, s.mean, s.median, s.p05, s.p95, s.max_abs
    )
}

/// VOACAP's median MUF for each hour.
fn voacap_muf_by_hour(listing: &ParsedListing) -> HashMap<u8, f64> {
    listing
        .numeric
        .iter()
        .filter(|s| s.row == MUF_ROW && s.slot == MUF_SLOT)
        .map(|s| (s.hour, s.value))
        .collect()
}

/// VOACAP values for one row label, keyed by hour and frequency slot.
fn voacap_by_hour_slot(listing: &ParsedListing, want: &str) -> HashMap<(u8, i8), f64> {
    listing
        .numeric
        .iter()
        .filter(|s| s.row == want && s.slot >= 0)
        .map(|s| ((s.hour, s.slot), s.value))
        .collect()
}

/// VOACAP modes keyed by hour and frequency slot.
fn voacap_modes(listing: &ParsedListing) -> HashMap<(u8, i8), String> {
    listing
        .modes
        .iter()
        .filter(|m| m.slot >= 0)
        .map(|m| ((m.hour, m.slot), m.mode.clone()))
        .collect()
}

/// Which sweep frequency slot an ITU row belongs to.
fn slot_of(freq_mhz: f64) -> Option<i8> {
    AMATEUR_FREQS_MHZ
        .iter()
        .position(|f| (f - freq_mhz).abs() < FREQ_EPSILON)
        .map(|i| i as i8)
}

/// P.533 prints modes as `1F2`, `2F2`, `1E`; VOACAP pads them. Compare trimmed.
fn same_mode(a: &str, b: &str) -> bool {
    a.trim().eq_ignore_ascii_case(b.trim())
}

struct CaseResult {
    voacap: Option<ParsedListing>,
    itu: Option<Vec<ItuRow>>,
    failure: Option<String>,
}

fn main() -> ExitCode {
    let itu_root = itu_root_arg();
    let paths = ItuPaths::from_checkout(&itu_root);
    if !paths.is_built() {
        eprintln!("no ITURHFProp binary at {}", paths.bin.display());
        eprintln!("build it with: make -C {}/Linux", itu_root.display());
        return ExitCode::FAILURE;
    }
    let voacap_bin = variant_bin(VOACAP_VARIANT);
    if !voacap_bin.is_file() {
        eprintln!("no voacapl binary at {}", voacap_bin.display());
        eprintln!("run tools/build-variants.sh first");
        return ExitCode::FAILURE;
    }

    let cases = sweep_cases();
    eprintln!("running {} cases through both engines", cases.len());
    let started = Instant::now();

    let results = map_limit(&cases, CONCURRENCY, |case, index| {
        let deck = match build_deck(case) {
            Ok(d) => d,
            Err(e) => {
                return CaseResult {
                    voacap: None,
                    itu: None,
                    failure: Some(format!("deck: {e}")),
                }
            }
        };

        let mut failure = None;

        let voacap = match IsolatedRoot::create(&format!("engines-{index}")) {
            Ok(root) => match run_deck(&voacap_bin, root.path(), &deck) {
                Ok(text) => Some(parse_listing(&text)),
                Err(e) => {
                    failure = Some(format!("voacap: {e}"));
                    None
                }
            },
            Err(e) => {
                failure = Some(format!("isolate: {e}"));
                None
            }
        };

        let work = scratch_dir(&format!("propcore-itu-{index}"));
        let itu = match fs::create_dir_all(&work)
            .map_err(|e| e.to_string())
            .and_then(|()| run_case(&paths, case, &work).map_err(|e| e.to_string()))
        {
            Ok(text) => Some(parse_report(&text)),
            Err(e) => {
                failure.get_or_insert(format!("itu: {e}"));
                None
            }
        };
        let _ = fs::remove_dir_all(&work);

        CaseResult {
            voacap,
            itu,
            failure,
        }
    });

    let failures: Vec<&String> = results.iter().filter_map(|r| r.failure.as_ref()).collect();
    let mut basic_muf = Vec::new();
    let mut operational_muf = Vec::new();
    let mut signal = Vec::new();
    let mut distance = Vec::new();
    let mut slots_compared = 0usize;
    let mut itu_no_mode = 0usize;
    let mut signal_skipped = 0usize;
    let mut compared_cases = 0usize;

    for result in &results {
        let (Some(listing), Some(rows)) = (&result.voacap, &result.itu) else {
            continue;
        };
        compared_cases += 1;

        let muf = voacap_muf_by_hour(listing);
        let sdbw = voacap_by_hour_slot(listing, "S DBW");
        let vmodes = voacap_modes(listing);

        // The basic MUF does not vary with frequency, so it is taken once per
        // hour rather than once per printed row.
        let mut hours_seen: HashMap<u8, ()> = HashMap::new();

        for r in rows {
            if hours_seen.insert(r.hour, ()).is_none() {
                if let Some(v) = muf.get(&r.hour) {
                    basic_muf.push(r.bmuf - v);
                    operational_muf.push(r.opmuf - v);
                }
                distance.push(r.distance_km);
            }

            let Some(slot) = slot_of(r.freq_mhz) else {
                continue;
            };
            if let Some(v) = sdbw.get(&(r.hour, slot)) {
                // Both engines print sentinel values for a dead path. Keeping
                // them would swamp the comparison with meaningless magnitudes.
                if *v > PLAUSIBLE_DBW && r.receiver_power > PLAUSIBLE_DBW {
                    signal.push(r.receiver_power - v);
                } else {
                    signal_skipped += 1;
                }
            }
            if vmodes.contains_key(&(r.hour, slot)) {
                slots_compared += 1;
                if r.mode.trim().eq_ignore_ascii_case(NO_MODE) {
                    itu_no_mode += 1;
                }
            }
        }
    }

    eprintln!("finished in {:.1}s", started.elapsed().as_secs_f64());

    if std::env::args().any(|a| a == "--diagnose") {
        diagnose(&results);
        return ExitCode::SUCCESS;
    }

    print_report(
        cases.len(),
        compared_cases,
        &failures,
        &basic_muf,
        &operational_muf,
        &signal,
        &distance,
        slots_compared,
        itu_no_mode,
        signal_skipped,
    );

    ExitCode::SUCCESS
}

/// Checks the two alignment assumptions this comparison rests on: that hour `n`
/// means the same in both reports, and that the quantities being subtracted are
/// on the same scale. A wrong assumption here would look like a disagreement
/// between the models when it is really a bug in this program.
fn diagnose(results: &[CaseResult]) {
    println!("# Alignment diagnostics\n");

    println!("## Hour offset\n");
    println!(
        "Mode agreement and MUF spread if P.533 hour `n` is matched against \
         VOACAP hour `n + offset`. Offset 0 should win if the two agree on what \
         hour 1 means.\n"
    );
    println!("| offset | mode agreement | mean abs MUF difference |");
    println!("| --: | --: | --: |");

    for offset in -2i16..=2 {
        let mut agree = 0usize;
        let mut total = 0usize;
        let mut muf_diffs = Vec::new();

        for result in results {
            let (Some(listing), Some(rows)) = (&result.voacap, &result.itu) else {
                continue;
            };
            let muf = voacap_muf_by_hour(listing);
            let vmodes = voacap_modes(listing);
            let mut seen: HashMap<u8, ()> = HashMap::new();

            for r in rows {
                let shifted = (((r.hour as i16 + offset) % 24 + 24) % 24) as u8;
                if seen.insert(r.hour, ()).is_none() {
                    if let Some(v) = muf.get(&shifted) {
                        muf_diffs.push((r.bmuf - v).abs());
                    }
                }
                let Some(slot) = slot_of(r.freq_mhz) else {
                    continue;
                };
                if let Some(v) = vmodes.get(&(shifted, slot)) {
                    total += 1;
                    if same_mode(v, &r.mode) {
                        agree += 1;
                    }
                }
            }
        }

        let pct = if total == 0 {
            0.0
        } else {
            100.0 * agree as f64 / total as f64
        };
        let mean = if muf_diffs.is_empty() {
            0.0
        } else {
            muf_diffs.iter().sum::<f64>() / muf_diffs.len() as f64
        };
        println!("| {offset:+} | {pct:.1}% | {mean:.2} MHz |");
    }

    println!("\n## Mode labels seen\n");
    let mut voacap_modes_seen: HashMap<String, usize> = HashMap::new();
    let mut itu_modes_seen: HashMap<String, usize> = HashMap::new();
    for result in results {
        if let Some(listing) = &result.voacap {
            for m in &listing.modes {
                *voacap_modes_seen
                    .entry(m.mode.trim().to_string())
                    .or_default() += 1;
            }
        }
        if let Some(rows) = &result.itu {
            for r in rows {
                *itu_modes_seen.entry(r.mode.trim().to_string()).or_default() += 1;
            }
        }
    }
    let show = |name: &str, counts: &HashMap<String, usize>| {
        let mut pairs: Vec<(&String, &usize)> = counts.iter().collect();
        pairs.sort_by(|a, b| b.1.cmp(a.1));
        let listed: Vec<String> = pairs
            .iter()
            .take(8)
            .map(|(m, n)| format!("`{m}` ({n})"))
            .collect();
        println!("- {name}: {}", listed.join(", "));
    };
    show("VOACAP", &voacap_modes_seen);
    show("P.533", &itu_modes_seen);

    println!("\n## Largest signal-power differences\n");
    let mut worst: Vec<(f64, u8, f64, f64, f64)> = Vec::new();
    for result in results {
        let (Some(listing), Some(rows)) = (&result.voacap, &result.itu) else {
            continue;
        };
        let sdbw = voacap_by_hour_slot(listing, "S DBW");
        for r in rows {
            let Some(slot) = slot_of(r.freq_mhz) else {
                continue;
            };
            if let Some(v) = sdbw.get(&(r.hour, slot)) {
                worst.push((
                    (r.receiver_power - v).abs(),
                    r.hour,
                    r.freq_mhz,
                    *v,
                    r.receiver_power,
                ));
            }
        }
    }
    worst.sort_by(|a, b| b.0.partial_cmp(&a.0).expect("no NaN"));
    println!("| difference | hour | MHz | VOACAP S DBW | P.533 Pr |");
    println!("| --: | --: | --: | --: | --: |");
    for (diff, hour, freq, v, i) in worst.iter().take(8) {
        println!("| {diff:.1} | {hour} | {freq:.2} | {v:.1} | {i:.1} |");
    }
}

fn itu_root_arg() -> PathBuf {
    let argv: Vec<String> = std::env::args().collect();
    if let Some(i) = argv.iter().position(|a| a == "--itu") {
        if let Some(v) = argv.get(i + 1) {
            return PathBuf::from(v);
        }
    }
    std::env::var_os("PROPCORE_ITU")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            std::env::var_os("HOME")
                .map(PathBuf::from)
                .unwrap_or_default()
                .join("workspace/vendor/itu-r-hf")
        })
}

fn scratch_dir(name: &str) -> PathBuf {
    std::env::var_os("PROPCORE_SCRATCH")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join(name)
}

#[allow(clippy::too_many_arguments)]
fn print_report(
    total_cases: usize,
    compared_cases: usize,
    failures: &[&String],
    basic_muf: &[f64],
    operational_muf: &[f64],
    signal: &[f64],
    distance: &[f64],
    slots_compared: usize,
    itu_no_mode: usize,
    signal_skipped: usize,
) {
    println!("# VOACAP against ITU-R P.533\n");
    println!("{compared_cases} of {total_cases} sweep cases ran on both engines.\n");
    println!(
        "These are two different models, so this is disagreement, not error. \
         Neither engine is the truth here, and nothing below says which is more \
         accurate. That question needs measured reception reports.\n"
    );

    if !failures.is_empty() {
        println!("## Cases that did not run\n");
        for f in failures.iter().take(10) {
            println!("- {f}");
        }
        if failures.len() > 10 {
            println!("- … and {} more", failures.len() - 10);
        }
        println!();
    }

    println!("## Directly comparable\n");
    println!("Differences are P.533 minus VOACAP.\n");
    println!("| quantity | n | mean | median | 5th pct | 95th pct | max abs | unit |");
    println!("| --- | --: | --: | --: | --: | --: | --: | --- |");
    println!("{}", row("Basic MUF", "MHz", &summarise(basic_muf)));
    println!(
        "{}",
        row("Operational MUF", "MHz", &summarise(operational_muf))
    );

    let d = summarise(distance);
    println!(
        "\nPath distance check: {} hours, mean {:.1} km. Both engines compute \
         this from the same great-circle geometry, so a disagreement here would \
         mean the two runs were not the same circuit.\n",
        d.n, d.mean
    );

    println!("## A real behavioural difference\n");
    let closed = if slots_compared == 0 {
        0.0
    } else {
        100.0 * itu_no_mode as f64 / slots_compared as f64
    };
    println!(
        "Of {slots_compared} hour and frequency combinations, P.533 found no \
         propagating mode at all in {itu_no_mode} ({closed:.1}%). VOACAP named \
         a mode in every one of them. The two engines disagree about how often \
         a band is usable, which matters more to somebody deciding whether to \
         call than any difference of a decibel.\n"
    );

    println!("## Indicative only\n");
    println!(
        "Both engines were run with isotropic antennas and the same transmit \
         power, but they do not define their signal reference points \
         identically, so treat this as a rough check rather than a measurement. \
         {signal_skipped} pairs were left out because at least one engine \
         printed a dead-path sentinel below {PLAUSIBLE_DBW:.0} dBW.\n"
    );
    println!("| quantity | n | mean | median | 5th pct | 95th pct | max abs | unit |");
    println!("| --- | --: | --: | --: | --: | --: | --: | --- |");
    println!("{}", row("Signal power", "dB", &summarise(signal)));

    println!("\n## Not comparable\n");
    println!(
        "- **Propagation mode.** The two use different vocabularies. VOACAP \
         labels the mode mix (`F2F2`, `EF2`, `F2 E`); P.533 names one dominant \
         mode with a hop count (`1F2`, `2E`) or `NONE`. Matching the labels \
         measures nothing.\n\
         - **Signal-to-noise ratio and reliability.** P.533 takes man-made \
         noise as a named environment over a stated bandwidth; VOACAP takes a \
         number at 3 MHz. There is no exact mapping, so any difference would \
         mix the models with the input conversion.\n"
    );
}
