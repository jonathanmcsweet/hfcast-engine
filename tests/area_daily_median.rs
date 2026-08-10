//! The middle of the day, at every point of an area grid.
//!
//! The empirical swing correction shrinks each hour toward the middle of
//! that place's own day. A map is drawn on a fine lattice and the middle
//! is computed on a coarse one, so the number has to be a property of
//! the place and not of the lattice it was computed on. That is the
//! property these tests hold: the same place must answer the same,
//! whatever else was in the grid with it.
//!
//! Needs `embedded-coefficients`, like every other test that asks for
//! `<embedded>`. `cargo test --all-features` runs them, which is what CI
//! does.

#![cfg(feature = "embedded-coefficients")]

use hfcast::json::{self, Json};

fn answer(body: &str) -> Json {
    let text = hfcast::service::run(body).expect("the run failed");
    let parsed = json::parse(&text).expect("the answer is not JSON");
    assert!(parsed.get("error").is_none(), "engine error: {text}");
    parsed
}

fn failure(body: &str) -> String {
    let text = hfcast::service::run(body).unwrap_or_else(|e| e);
    let parsed = json::parse(&text).ok();
    parsed
        .as_ref()
        .and_then(|p| p.get("error"))
        .and_then(Json::as_str)
        .map_or(text, str::to_string)
}

fn points(a: &Json) -> Vec<Json> {
    a.get("points")
        .and_then(Json::as_array)
        .expect("no points")
        .to_vec()
}

/// A grid over one rectangle, at one lattice step.
fn grid(bands: &str, lat_step: f64, lon_step: f64, box_: &str) -> String {
    format!(
        r#"{{"itshfbc":"<embedded>","mode":"area","dailyMedian":true,
           "fromLat":33.75,"fromLon":-84.39,
           "month":8,"year":2026,"ssn":60,"watts":100,
           "requiredSnrDb":-24,"noiseDbw":-145,
           "latStep":{lat_step},"lonStep":{lon_step},{box_}{bands}}}"#
    )
}

/// The middle at one place, out of the smallest grid that holds it.
///
/// A grid needs at least two points on each side, so the smallest is two
/// by two. That is still a place with three neighbours instead of 191,
/// and three of them in different directions, which is enough for
/// carried state to show.
fn alone(bands: &str, lat: f64, lon: f64) -> Json {
    let box_ = format!(
        "\"latMin\":{},\"latMax\":{},\"lonMin\":{},\"lonMax\":{},",
        lat - 1.0,
        lat + 16.0,
        lon - 1.0,
        lon + 23.5,
    );
    let few = points(&answer(&grid(bands, 15.0, 22.5, &box_)));
    assert_eq!(
        few.len(),
        4,
        "the small rectangle gave {} points",
        few.len()
    );
    let at = |p: &Json, k: &str| p.get(k).and_then(Json::as_f64).expect(k);
    few.iter()
        .find(|p| (at(p, "lat") - lat).abs() < 1e-6 && (at(p, "lon") - lon).abs() < 1e-6)
        .expect("the place asked for is not in its own rectangle")
        .clone()
}

#[test]
fn a_place_answers_the_same_whatever_lattice_it_was_computed_on() {
    // The lattice the map's coarse grid uses, and the finer one the
    // correction's anchor lattice would use. Every point of the coarse
    // one is also a point of the finer one, because 15 is three times 5
    // and 22.5 is three times 7.5 — so the two must agree point for
    // point where they meet.
    let coarse = points(&answer(&grid("\"freqMhz\":14.1", 15.0, 22.5, "")));
    let fine = points(&answer(&grid("\"freqMhz\":14.1", 5.0, 7.5, "")));

    let key = |p: &Json| {
        let at = |k: &str| (p.get(k).and_then(Json::as_f64).expect(k) * 1e6).round() as i64;
        (at("lat"), at("lon"))
    };
    let middle = |p: &Json| {
        p.get("medianSnr")
            .and_then(Json::as_f64)
            .expect("medianSnr")
    };

    let finer: std::collections::HashMap<(i64, i64), f64> =
        fine.iter().map(|p| (key(p), middle(p))).collect();

    let mut shared = 0;
    for p in &coarse {
        let Some(other) = finer.get(&key(p)) else {
            continue;
        };
        shared += 1;
        assert_eq!(
            middle(p),
            *other,
            "the same place answered differently on the two lattices",
        );
    }
    assert!(
        shared >= 100,
        "the lattices only met at {shared} places, which is too few to prove anything",
    );
}

#[test]
fn a_place_answers_the_same_alone_as_it_does_in_a_crowd() {
    // The strongest form of the same property: a grid of one point has
    // no neighbours at all to carry state from.
    let whole = points(&answer(&grid("\"freqMhz\":14.1", 15.0, 22.5, "")));
    let middle = |p: &Json| {
        p.get("medianSnr")
            .and_then(Json::as_f64)
            .expect("medianSnr")
    };

    // Four places spread across the grid rather than all of them: each
    // one costs a whole day of prediction.
    for index in [0usize, 37, 96, 150] {
        let p = &whole[index];
        let lat = p.get("lat").and_then(Json::as_f64).expect("lat");
        let lon = p.get("lon").and_then(Json::as_f64).expect("lon");
        assert_eq!(
            middle(p),
            middle(&alone("\"freqMhz\":14.1", lat, lon)),
            "point {index} at {lat}, {lon} answered differently on its own",
        );
    }
}

#[test]
fn a_band_read_from_a_batch_is_the_band_run_alone() {
    // The same contract `area_bands.rs` holds for a one-hour run. A map
    // takes every band's middle from one pass, and reading one out of it
    // has to give what asking for that band alone would.
    let bands = [3.6f64, 14.1, 28.1];
    let list = bands
        .iter()
        .map(f64::to_string)
        .collect::<Vec<_>>()
        .join(",");
    let together = points(&answer(&grid(
        &format!("\"freqsMhz\":[{list}]"),
        15.0,
        22.5,
        "",
    )));

    for (index, band) in bands.iter().enumerate() {
        let one = points(&answer(&grid(
            &format!("\"freqMhz\":{band}"),
            15.0,
            22.5,
            "",
        )));
        assert_eq!(one.len(), together.len(), "{band} MHz: point count");
        for (i, single) in one.iter().enumerate() {
            let batched = together[i]
                .get("medianSnr")
                .and_then(Json::as_array)
                .expect("a batch answers with one middle a band")
                .get(index)
                .and_then(Json::as_f64)
                .expect("band index past the end");
            let solo = single
                .get("medianSnr")
                .and_then(Json::as_f64)
                .expect("medianSnr");
            assert_eq!(solo, batched, "{band} MHz, point {i}");
        }
    }
}

#[test]
fn the_middle_is_the_average_of_the_two_middle_hours() {
    // 24 is an even count, so the middle is between the twelfth and
    // thirteenth values of the sorted day. That is the rule the
    // application's own correction uses over the band table, and the two
    // have to centre on the same number — which shows up as a half
    // decibel, because the hourly values are whole numbers.
    let whole = points(&answer(&grid("\"freqMhz\":14.1", 15.0, 22.5, "")));
    let halves = whole
        .iter()
        .filter_map(|p| p.get("medianSnr").and_then(Json::as_f64))
        .filter(|v| (v * 2.0).fract() == 0.0 && v.fract() != 0.0)
        .count();
    assert!(
        halves > 0,
        "no middle landed between two hours, so the even-count rule is untested",
    );
    // And every value is either whole or a half, because it is either
    // one hour's whole-decibel value or the average of two of them.
    for p in &whole {
        let v = p
            .get("medianSnr")
            .and_then(Json::as_f64)
            .expect("medianSnr");
        assert_eq!(
            (v * 2.0).fract(),
            0.0,
            "{v} is neither a whole decibel nor a half",
        );
    }
}

#[test]
fn an_hour_cannot_be_asked_for_alongside_the_whole_day() {
    // A caller that sent an hour believes the answer is about that hour.
    // It is not, so the request is refused rather than answered.
    let text = failure(&grid("\"freqMhz\":14.1,\"hour\":18", 15.0, 22.5, ""));
    assert!(
        text.contains("hour") && text.contains("whole day"),
        "unhelpful refusal: {text}",
    );
}

#[test]
fn a_whole_day_answer_carries_no_hourly_values() {
    // There is no hour for a reliability or an angle to be about, so
    // neither is reported. A caller reading one would be reading a
    // number that means nothing.
    let whole = points(&answer(&grid("\"freqMhz\":14.1", 15.0, 22.5, "")));
    for p in &whole {
        for absent in ["reliability", "takeoffAngleDeg", "snr", "snrLowDecile"] {
            assert!(
                p.get(absent).is_none(),
                "a whole-day point reported {absent}",
            );
        }
    }
}
