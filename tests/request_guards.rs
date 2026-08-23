//! A bad request must give an error, not stop the process.
//!
//! Every case here stopped the process or answered with numbers that
//! looked correct before the guard it tests was added. The request comes
//! from a server, so a fault in it is a fault to report, not one to end
//! the run with.
//!
//! Needs `embedded-coefficients`: the runs ask for `<embedded>`, and the
//! coefficients are not compiled in by default. `cargo test
//! --all-features` runs them, which is what CI does.

#![cfg(feature = "embedded-coefficients")]

use hfcast::json;

/// An area request with one field replaced.
fn area_with(field: &str) -> String {
    format!(
        r#"{{"itshfbc":"<embedded>","mode":"area",
           "fromLat":33.75,"fromLon":-84.39,
           "month":8,"year":2026,"ssn":60,"watts":100,
           "requiredSnrDb":-24,"noiseDbw":-145,"hour":18,
           "latStep":45,"lonStep":90,"freqMhz":14.1,{field}}}"#
    )
}

/// A point-to-point request with one field replaced.
fn point_with(field: &str) -> String {
    format!(
        r#"{{"itshfbc":"<embedded>",
           "fromLat":35.8,"fromLon":-5.9,"toLat":44.9,"toLon":20.5,
           "month":8,"year":2026,"ssn":60,"watts":100,
           "requiredSnrDb":-24,"noiseDbw":-145,
           "bands":[14.1],{field}}}"#
    )
}

#[test]
fn an_hour_outside_the_day_is_refused() {
    // Below zero the run answered a different map with no error. At 99
    // it stopped the process: the coefficient tables are indexed by
    // hour and have no bound of their own. `1e999` overflowed the add
    // that turns 0 to 23 into the 1 to 24 the input file names.
    for bad in ["-3", "24", "99", "1e999", "12.5"] {
        let answer = hfcast::service::run(&area_with(&format!(r#""hour":{bad}"#)));
        assert!(answer.is_err(), "hour {bad} was accepted");
    }
    assert!(hfcast::service::run(&area_with(r#""hour":0"#)).is_ok());
    assert!(hfcast::service::run(&area_with(r#""hour":23"#)).is_ok());
}

#[test]
fn a_sunspot_number_outside_the_model_is_refused() {
    // The coefficient maps are held for SSN 0 and SSN 100 and the run
    // mixes between them, so a number far outside cannot be answered.
    for bad in ["-1", "1e999", "100000"] {
        let field = format!(r#""ssn":{bad}"#);
        assert!(
            hfcast::service::run(&area_with(&field)).is_err(),
            "area accepted ssn {bad}"
        );
        assert!(
            hfcast::service::run(&point_with(&field)).is_err(),
            "point accepted ssn {bad}"
        );
    }
}

#[test]
fn a_label_with_multi_byte_characters_does_not_stop_the_run() {
    // The card cuts a label that is too long for its field. The cut was
    // by byte with no check, so a character that takes more than one
    // byte across the cut stopped the process.
    let long = format!("{}é", "A".repeat(19));
    let answer = hfcast::service::run(&point_with(&format!(r#""fromLabel":"{long}""#)));
    assert!(answer.is_ok(), "the run failed: {answer:?}");
}

/// A point-to-point request with no sunspot field of its own, for the
/// engine-selector cases: the parity engine states `"ssn"`, the
/// truecast engine `"essn"`, and the helper bakes in neither.
fn engine_point(fields: &str) -> String {
    format!(
        r#"{{"itshfbc":"<embedded>",
           "fromLat":35.8,"fromLon":-5.9,"toLat":44.9,"toLon":20.5,
           "month":8,"year":2026,"watts":100,
           "requiredSnrDb":-24,"noiseDbw":-145,
           "bands":[14.1],{fields}}}"#
    )
}

#[test]
fn an_unknown_engine_is_refused() {
    let answer = hfcast::service::run(&engine_point(r#""engine":"p533","essn":60"#));
    assert!(answer.is_err());
}

#[test]
fn a_truecast_request_with_a_ssn_beside_the_index_is_refused() {
    // `point_with` carries "ssn":60 of its own, so this request states
    // both numbers — and must be refused, not silently resolved.
    let answer = hfcast::service::run(&point_with(r#""engine":"truecast","essn":60"#));
    assert!(answer.is_err());
}

#[test]
fn every_answer_names_its_engine() {
    let old = hfcast::service::run(&point_with(r#""fromLabel":"A""#)).expect("runs");
    assert!(old.contains(r#""engine":"voacap""#), "{old}");
}

#[test]
fn a_truecast_run_at_a_plain_index_is_the_parity_run_at_that_number() {
    // At an index at or above zero the conditioning changes the input,
    // never the physics, so the two runs answer the same question. They
    // are no longer byte-equal: truecast also takes its own numerics,
    // which evaluate the virtual height instead of approximating it.
    // That is a deliberate second difference and it is small. What this
    // guards is that the conditioning did not smuggle in a third.
    let parity = hfcast::service::run(&engine_point(r#""ssn":60"#)).expect("runs");
    let truecast =
        hfcast::service::run(&engine_point(r#""engine":"truecast","essn":60"#)).expect("runs");

    let numbers = |s: &str| -> Vec<f64> {
        s.split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
            .filter_map(|t| t.parse::<f64>().ok())
            .collect()
    };
    let (a, b) = (numbers(&parity), numbers(&truecast));
    assert_eq!(a.len(), b.len(), "the two answers have different shapes");
    let worst = a
        .iter()
        .zip(&b)
        .map(|(x, y)| (x - y).abs())
        .fold(0.0f64, f64::max);
    // One decibel of signal, or a few tenths of a degree of takeoff
    // angle. Anything larger is a physics change, not the numerics.
    assert!(
        worst <= 1.0,
        "the two runs differ by {worst}, which is too much for numerics alone"
    );
}

#[test]
fn below_the_floor_needs_a_work_directory() {
    let answer = hfcast::service::run(&engine_point(r#""engine":"truecast","essn":-10"#));
    let message = answer.expect_err("must be refused");
    assert!(message.contains("workDir"), "{message}");
}

#[test]
fn below_the_floor_the_run_synthesizes_the_fof2_overlay() {
    let dir = std::env::temp_dir().join("hfcast-engine-floor-test");
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    let field = format!(
        r#""engine":"truecast","essn":-10,"workDir":"{}""#,
        dir.display()
    );
    let at_floor =
        hfcast::service::run(&engine_point(r#""engine":"truecast","essn":0"#)).expect("runs");
    let below = hfcast::service::run(&engine_point(&field)).expect("runs");
    assert!(below.contains(r#""engine":"truecast""#), "{below}");
    // The synthesized file is where the request asked, and the answer
    // moved: foF2 kept following the fitted line below zero.
    let daw = dir
        .join("truecast-fof2-08--10.00")
        .join("coeffs")
        .join("fof2CCIR.daw");
    assert!(daw.exists(), "no synthesized overlay at {}", daw.display());
    assert_ne!(below, at_floor);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_request_nested_too_deep_is_refused() {
    // The parser calls itself for each level, so the text decided how
    // deep the call stack went. This much stopped the process.
    let deep = format!(r#"{{"itshfbc":{}}}"#, "[".repeat(100_000));
    assert!(hfcast::service::run(&deep).is_err());
}

#[test]
fn a_number_too_large_for_f64_is_refused_where_it_is_read() {
    // `1e999` is infinity. It reached the deck as `inf.`, and the
    // listing came back full of NaN. Reading the columns back then
    // moved every value one place, so slot 0 reported its neighbour.
    assert!(json::parse(r#"{"ssn":1e999}"#).is_err());
}
