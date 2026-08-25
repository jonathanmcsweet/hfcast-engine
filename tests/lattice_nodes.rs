//! A lattice node has to be exactly what the engine would have computed
//! at that place.
//!
//! The lattice rests on one claim: the layer parameters at a control
//! point are a function of its position and of nothing else. Not of how
//! far along a path it sits, and not of which other points share the
//! call. If that is wrong anywhere, a node stands in for a calculation
//! it is not, and the engine goes quietly wrong rather than loudly.
//!
//! So the test hands the engine a set of control points at chosen
//! places, with deliberately different distances along their path, and
//! checks every field against what the node builder produces from
//! position alone.
//!
//! Needs `embedded-coefficients`: the maps come from `<embedded>`.
#![cfg(feature = "embedded-coefficients")]

use hfcast::voacap::coefficients::{redmap, FoF2Model};
use hfcast::voacap::con::{MagneticPole, D2R, R};
use hfcast::voacap::data;
use hfcast::voacap::geometry::{geomagnetic_latitude, ControlPoint};
use hfcast::voacap::ionosphere::{
    cofion, layer_parameters, layer_parameters_at, virtim, LayerInputs, LayerParams,
};
use hfcast::voacap::magnetic::magvar;

/// Places chosen to reach the parts of the maps that behave
/// differently: the magnetic equator, both auroral zones, a pole, the
/// date line, and the prime meridian.
const PLACES: [(R, R); 8] = [
    (47.6, -122.3),
    (51.5, -0.1),
    (0.0, 0.0),
    (-33.9, 151.2),
    (68.0, 18.0),
    (-70.0, -60.0),
    (90.0, 0.0),
    (12.0, 180.0),
];

/// The month, index and hour the rest of the harness uses for a
/// single-case check.
const MONTH: u32 = 6;
const SSN: R = 60.0;
const GMT: R = 13.0;

fn fields(p: &LayerParams) -> Vec<(&'static str, R)> {
    [
        ("fi0", p.fi[0]),
        ("fi1", p.fi[1]),
        ("fi2", p.fi[2]),
        ("yi0", p.yi[0]),
        ("yi1", p.yi[1]),
        ("yi2", p.yi[2]),
        ("hi0", p.hi[0]),
        ("hi1", p.hi[1]),
        ("hi2", p.hi[2]),
        ("f2m3", p.f2m3),
        ("hpf2", p.hpf2),
        ("rat", p.rat),
        ("abiy", p.abiy),
        ("clck", p.clck),
        ("zenang", p.zenang),
        ("zenmax", p.zenmax),
    ]
    .into_iter()
    .collect()
}

#[test]
fn a_node_is_what_the_engine_computes_at_that_place() {
    let set = redmap(&data::embedded_root(), FoF2Model::Ccir, MONTH, SSN)
        .expect("the embedded coefficients load");
    let cof = cofion(&set);
    let ab = virtim(&cof, &set.ikim, GMT);
    let pole = MagneticPole::default();
    let psc = [0.0 as R; 4];

    // Distances along a path that are all different and none zero, so
    // that a chain reading `rd` would give a different answer here from
    // the node builder, which sets it to zero.
    let points: Vec<ControlPoint> = PLACES
        .iter()
        .enumerate()
        .map(|(i, (lat_deg, lon_deg))| {
            let (lat, lon) = (lat_deg * D2R, lon_deg * D2R);
            ControlPoint {
                rd: 0.05 + 0.1 * i as R,
                lat,
                lon,
                gmlat: geomagnetic_latitude(pole, lat, lon),
            }
        })
        .collect();
    let mags: Vec<_> = points.iter().map(|p| magvar(p.lat, p.lon)).collect();

    let live = layer_parameters(&set, &ab, &points, &mags, MONTH, SSN, GMT, &psc);
    assert_eq!(live.len(), PLACES.len(), "one set of parameters per point");

    let inputs = LayerInputs {
        set: &set,
        month: MONTH,
        ssn: SSN,
        gmt: GMT,
        psc: &psc,
    };
    live.iter().zip(&points).for_each(|(want, pt)| {
        let node = layer_parameters_at(inputs, &ab, pole, pt.lat, pt.lon);
        fields(want)
            .into_iter()
            .zip(fields(&node))
            .for_each(|((name, want), (_, got))| {
                assert_eq!(
                    want.to_bits(),
                    got.to_bits(),
                    "{name} at {:.1}, {:.1}: engine {want} against node {got}",
                    pt.lat / D2R,
                    pt.lon / D2R
                );
            });
    });
}

#[test]
fn a_point_answers_the_same_whoever_it_shares_the_call_with() {
    let set = redmap(&data::embedded_root(), FoF2Model::Ccir, MONTH, SSN)
        .expect("the embedded coefficients load");
    let cof = cofion(&set);
    let ab = virtim(&cof, &set.ikim, GMT);
    let pole = MagneticPole::default();
    let psc = [0.0 as R; 4];
    let (lat, lon) = (47.6 * D2R, -122.3 * D2R);
    let alone = ControlPoint {
        rd: 0.0,
        lat,
        lon,
        gmlat: geomagnetic_latitude(pole, lat, lon),
    };

    // The same place, first on its own and then as the last of five.
    let one = layer_parameters(
        &set,
        &ab,
        &[alone],
        &[magvar(lat, lon)],
        MONTH,
        SSN,
        GMT,
        &psc,
    );
    let crowd: Vec<ControlPoint> = PLACES
        .iter()
        .take(4)
        .map(|(lat_deg, lon_deg)| {
            let (lat, lon) = (lat_deg * D2R, lon_deg * D2R);
            ControlPoint {
                rd: 0.3,
                lat,
                lon,
                gmlat: geomagnetic_latitude(pole, lat, lon),
            }
        })
        .chain(std::iter::once(alone))
        .collect();
    let mags: Vec<_> = crowd.iter().map(|p| magvar(p.lat, p.lon)).collect();
    let many = layer_parameters(&set, &ab, &crowd, &mags, MONTH, SSN, GMT, &psc);

    fields(&one[0])
        .into_iter()
        .zip(fields(&many[4]))
        .for_each(|((name, want), (_, got))| {
            assert_eq!(
                want.to_bits(),
                got.to_bits(),
                "{name} depends on its company"
            );
        });
}
