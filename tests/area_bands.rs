//! Several bands in one area run must equal several runs of one band.
//!
//! Needs `embedded-coefficients`: these runs ask for `<embedded>`, and the
//! coefficients are not compiled in by default. `cargo test --all-features`
//! runs them, which is what CI does.
//!
//! This is the whole contract of `freqsMhz`, and it is not a contract
//! the reference can be asked about: `HFAREA` answers several
//! frequencies by printing the maximum over them, so there is no
//! reference output for "each band, separately, from one pass". The
//! check is therefore against the engine's own single-band answer, which
//! *is* covered against the reference by `areacheck`.
//!
//! It has already caught three faults. Rounding reliability with
//! arithmetic instead of through the printed `F6.3` disagreed on 46
//! percent of points; the take-off angle was read from `ANGLER` when an
//! area run reports `ANGLE`, which moved it by up to 64 degrees; and
//! carrying the values as `f32` put 3e-8 between two numbers that should
//! be the same number.
//!
//! Needs no reference binary and no data tree: the coefficient files are
//! compiled in, so this runs anywhere.

#![cfg(feature = "embedded-coefficients")]

use hfcast::json::{self, Json};

/// Three bands far enough apart to sit in different windows, and low,
/// middle and high so a lookup that picked the wrong table would show.
const BANDS: [f64; 3] = [3.6, 14.1, 28.1];

/// A coarse grid. The property is about every point, not about many of
/// them, and 192 points keep the test quick.
fn request(bands: &str) -> String {
    format!(
        r#"{{"itshfbc":"<embedded>","mode":"area",
           "fromLat":33.75,"fromLon":-84.39,
           "month":8,"year":2026,"ssn":60,"watts":100,
           "requiredSnrDb":-24,"noiseDbw":-145,"hour":18,
           "latStep":15,"lonStep":22.5,{bands}}}"#
    )
}

fn answer(body: &str) -> Json {
    let text = hfcast::service::run(body).expect("the run failed");
    let parsed = json::parse(&text).expect("the answer is not JSON");
    assert!(parsed.get("error").is_none(), "engine error: {text}");
    parsed
}

fn points(a: &Json) -> Vec<Json> {
    a.get("points")
        .and_then(Json::as_array)
        .expect("no points")
        .to_vec()
}

/// One value, as the comparison sees it: a number, or absent.
fn slot(p: &Json, key: &str, at: Option<usize>) -> Option<f64> {
    let field = p.get(key).expect("missing field");
    match at {
        None => field.as_f64(),
        Some(i) => field
            .as_array()
            .expect("expected an array a band long")
            .get(i)
            .expect("band index past the end")
            .as_f64(),
    }
}

#[test]
fn a_batch_of_bands_equals_the_same_bands_run_one_at_a_time() {
    let list = BANDS
        .iter()
        .map(|f| f.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let together = answer(&request(&format!("\"freqsMhz\":[{list}]")));
    let batch = points(&together);

    // The frequencies are echoed so a caller cannot line the arrays up
    // against the wrong band. Check that before trusting the order.
    let echoed = together
        .get("freqsMhz")
        .and_then(Json::as_array)
        .expect("the batch does not say which bands it answered");
    assert_eq!(echoed.len(), BANDS.len());
    for (i, band) in BANDS.iter().enumerate() {
        let said = echoed[i].as_f64().expect("a frequency");
        assert!((said - band).abs() < 1e-4, "band {i}: {said} not {band}");
    }

    for (index, band) in BANDS.iter().enumerate() {
        let alone = points(&answer(&request(&format!("\"freqMhz\":{band}"))));
        assert_eq!(alone.len(), batch.len(), "{band} MHz: point count");

        for (i, one) in alone.iter().enumerate() {
            let many = &batch[i];
            // Same place, or the comparison below is meaningless.
            for axis in ["lat", "lon"] {
                let a = slot(one, axis, None).expect(axis);
                let b = slot(many, axis, None).expect(axis);
                assert!((a - b).abs() < 1e-9, "{band} MHz point {i}: {axis}");
            }
            // Exactly equal, not nearly. Both sides read the same
            // printed column, so there is no rounding left to allow for.
            //
            // The signal level and its two deciles are here for the same
            // reason as the rest: the map corrects a cell from them, and
            // a band read out of a batch that disagreed with the same
            // band run alone would correct one map differently from
            // another for no reason a reader could see.
            for key in [
                "reliability",
                "takeoffAngleDeg",
                "snr",
                "snrLowDecile",
                "snrUpDecile",
            ] {
                assert_eq!(
                    slot(one, key, None),
                    slot(many, key, Some(index)),
                    "{band} MHz, point {i}, {key}",
                );
            }
        }
    }
}

#[test]
fn one_band_asked_for_as_a_list_is_still_a_plain_answer() {
    // A single-entry list is one band, so it takes the ordinary shape
    // rather than arrays of length one. Callers that build the list
    // programmatically then need no special case for it.
    let listed = answer(&request("\"freqsMhz\":[14.1]"));
    let plain = answer(&request("\"freqMhz\":14.1"));
    assert_eq!(listed.write(), plain.write());
}

#[test]
fn a_band_list_that_could_not_be_split_into_windows_is_refused() {
    // Each band gets its own antenna table in a window cut halfway to
    // its neighbours. Repeats and reversals cannot produce windows that
    // hold exactly one band each, so they are refused rather than
    // silently answered from whichever table the lookup reached first.
    for bad in [
        "\"freqsMhz\":[]",
        "\"freqsMhz\":[14.1,14.1]",
        "\"freqsMhz\":[21.1,7.1]",
        "\"freqsMhz\":[7.1,0]",
        "\"freqsMhz\":\"7.1\"",
    ] {
        let out = hfcast::service::run(&request(bad));
        assert!(out.is_err(), "accepted {bad}: {out:?}");
    }
}
