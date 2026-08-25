//! The threaded map driver answers the same question as the serial one.
//!
//! Truecast's one-hour map moved off `run_area`, the serial parity area
//! driver, onto `predict_grid_cells`, which threads inside the engine.
//! The parity engine did not move, because `run_area` carries state
//! from point to point the way `HFAREA` does and that carry is part of
//! what makes its answer match the Fortran.
//!
//! Two things have to hold for the swap to be safe, and neither is
//! obvious from reading either driver.
//!
//! The first point of a parity area run has no carry yet, so there the
//! two drivers must agree exactly. That is the anchor.
//!
//! And the server reads a one-band map's reliability out of the printed
//! column, because its correction factors were fitted against printed
//! values. The threaded driver has no printed columns, so the service
//! rounds the raw value to three decimals instead. That substitution is
//! only sound if the two really are the same number, which is what the
//! second test measures rather than assumes.
//!
//! Needs `embedded-coefficients`: the maps come from `<embedded>`.
#![cfg(feature = "embedded-coefficients")]

use hfcast::truecast::grid::{predict_grid_cells, GridRequest};
use hfcast::voacap::area::{Grid, Projection};
use hfcast::voacap::coefficients::FoF2Model;
use hfcast::voacap::data;
use hfcast::voacap::fastmath::Numerics;
use hfcast::voacap::model::Model;
use hfcast::voacap::run::{run_area, AreaInputs};

/// The reliability column of `OUTAREA`, the field the server parses.
const AREA_RELIABILITY_FIELD: usize = 12;

fn area(numerics: Numerics) -> AreaInputs {
    AreaInputs {
        numerics,
        grid: Grid {
            projection: Projection::LatLon,
            plat: 47.0,
            plon: 8.0,
            xmin: 2.0,
            xmax: 26.0,
            ymin: 36.0,
            ymax: 58.0,
            nx: 6,
            ny: 5,
        },
        tx_lat_deg: 47.0,
        tx_lon_deg: 8.0,
        month: 6,
        ssn: 80.0,
        hour: 13,
        freqs_mhz: vec![14.1],
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

#[test]
fn the_first_point_has_no_carry_so_both_drivers_must_agree_there() {
    let root = data::embedded_root();
    // Reference arithmetic on both sides, so the carry is the only
    // difference left between the two drivers.
    let inputs = area(Numerics::reference());
    let serial = run_area(&root, &inputs).expect("the serial driver answers");
    let threaded = predict_grid_cells(
        &root,
        &GridRequest {
            area: inputs,
            threads: 3,
        },
    )
    .expect("the threaded driver answers");

    assert_eq!(serial.len(), threaded.len(), "same number of points");
    let (first, cell) = (&serial[0], &threaded[0]);
    assert_eq!(first.lat, cell.lat, "same latitude");
    assert_eq!(first.lon, cell.lon, "same longitude");
    let want = first.per_freq.first().expect("one band");
    let got = cell.per_freq.first().expect("one band");
    assert_eq!(want.reliability, got.reliability, "reliability");
    assert_eq!(want.snr_db, got.snr_db, "signal to noise");
    assert_eq!(want.snr_low_decile, got.snr_low_decile, "lower decile");
    assert_eq!(want.snr_up_decile, got.snr_up_decile, "upper decile");
    assert_eq!(want.takeoff_angle_deg, got.takeoff_angle_deg, "takeoff");
}

#[test]
fn the_printed_reliability_is_the_raw_one_rounded_to_three_decimals() {
    let root = data::embedded_root();
    let inputs = area(Numerics::reference());
    let points = run_area(&root, &inputs).expect("the serial driver answers");
    let off = points.iter().find(|p| {
        let printed = p.fields[AREA_RELIABILITY_FIELD]
            .trim()
            .parse::<f64>()
            .unwrap_or(0.0);
        let raw = p.per_freq[0].reliability;
        (printed - (raw * 1000.0).round() / 1000.0).abs() > 1e-9
    });
    assert!(
        off.is_none(),
        "a printed reliability is not the raw one rounded to three decimals: {:?}",
        off.map(|p| (
            p.fields[AREA_RELIABILITY_FIELD].clone(),
            p.per_freq[0].reliability
        )),
    );
}
