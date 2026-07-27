//! The API against the real reference binary, byte for byte.
//!
//! For every [`Task`], the deck the request resolves to is run through
//! the reference `voacapl` build and the text is compared with
//! [`propcore::api::listing`]'s. The unit tests prove the request
//! resolves consistently; this proves the whole answer is the
//! reference's answer.
//!
//! The tests skip (and say so) on a machine without the reference
//! binary and data tree, so `cargo test` stays runnable anywhere.

use propcore::api::{
    deck, listing, predict, EfVar, EsVar, FoF2Model, Heights, Ionosphere, Model, Recompute, Report,
    Request, Site, Task,
};
use propcore::runner::{itshfbc_dir, run_deck, variant_bin, IsolatedRoot};

const ALL_TASKS: [Task; 7] = [
    Task::Parameters,
    Task::Ionograms,
    Task::MufNomogram,
    Task::Muf,
    Task::Luf,
    Task::Systems,
    Task::AllModes,
];

fn a_request() -> Request {
    Request {
        tx: Site {
            name: "TANGIER".to_string(),
            lat_deg: 35.8,
            lon_deg: -5.9,
        },
        rx: Site {
            name: "BELGRADE".to_string(),
            lat_deg: 44.9,
            lon_deg: 20.5,
        },
        month: 6,
        year: 2026,
        ssn: 100.0,
        power_watts: 100.0,
        freqs_mhz: vec![7.2, 14.2, 21.2],
        required_snr_db: 24.0,
        noise_dbw: 145.0,
        fof2: FoF2Model::Ccir,
        layer_multipliers: [1.0, 1.0, 1.0, 1.0],
        tx_antennas: Vec::new(),
        rx_antennas: Vec::new(),
        ionosphere: Ionosphere::default(),
        // Every test in this file compares against the reference, so
        // the compatible tier is the only one they can judge.
        model: Model::Compatible,
    }
}

/// A request exercising the ionosphere controls: kept layer values,
/// integrated heights. Every value is card-expressible on purpose —
/// that is the set the byte-identical claim covers.
fn an_override_request() -> Request {
    let mut req = a_request();
    req.ionosphere = Ionosphere {
        recompute: Recompute::KeepLayers,
        heights: Heights::Integrated,
        layers: (1..=5)
            .map(|point| EfVar {
                area: point,
                fi: [3.0, 4.5, 8.0],
                yi: [20.0, 40.0, 80.0],
                hi: [110.0, 200.0, 300.0],
            })
            .collect(),
        sporadic_e: (1..=5)
            .map(|point| EsVar {
                area: point,
                fs: [2.0, 3.0, 4.0],
                hs: 110.0,
            })
            .collect(),
        profile: None,
    };
    req
}

/// `None` where the reference is not installed; the tests then skip.
///
/// The tag must be unique per test: the tests share one process, an
/// [`IsolatedRoot`] is keyed by process id and tag, and creating one
/// begins by deleting that directory — a shared tag would erase
/// another test's tree while it runs.
fn reference(tag: &str) -> Option<(std::path::PathBuf, IsolatedRoot)> {
    let bin = variant_bin("O2");
    if !bin.is_file() || !itshfbc_dir().is_dir() {
        eprintln!("skipped: no reference voacapl build on this machine");
        return None;
    }
    let root = IsolatedRoot::create(tag).expect("isolated itshfbc tree");
    Some((bin, root))
}

#[test]
fn every_task_prints_the_reference_listing() {
    let Some((bin, root)) = reference("api-tasks") else {
        return;
    };
    let req = a_request();
    for task in ALL_TASKS {
        let cards = deck(&req, task).expect("deck");
        let fortran = run_deck(&bin, root.path(), &cards).expect("reference run");
        let ported = listing(root.path(), &req, task).expect("ported run");
        assert_eq!(ported, fortran, "task {task:?} listing differs");
    }
}

#[test]
fn the_ionosphere_controls_print_the_reference_listing() {
    let Some((bin, root)) = reference("api-iono") else {
        return;
    };
    let req = an_override_request();
    for task in [Task::Muf, Task::Systems] {
        let cards = deck(&req, task).expect("deck");
        let fortran = run_deck(&bin, root.path(), &cards).expect("reference run");
        let ported = listing(root.path(), &req, task).expect("ported run");
        assert_eq!(ported, fortran, "task {task:?} listing differs");
    }
}

/// A request finer than every card column can carry.
///
/// This is what proves `DeckCase::as_written` covers every field: a
/// field it misses is a field where the engine runs on the caller's
/// value while the deck echoes a rounded one, and the reference —
/// which has only the deck — then disagrees.
#[test]
fn a_request_finer_than_the_cards_still_prints_the_reference_listing() {
    let Some((bin, root)) = reference("api-precision") else {
        return;
    };
    let mut req = a_request();
    req.tx.lat_deg = 35.876_54;
    req.tx.lon_deg = -5.931_27;
    req.rx.lat_deg = 44.918_36;
    req.rx.lon_deg = 20.512_49;
    req.ssn = 103.71;
    req.power_watts = 137.049_1;
    req.required_snr_db = 24.37;
    req.noise_dbw = 145.42;
    req.freqs_mhz = vec![7.213_4, 14.267_9, 21.184_2];
    req.layer_multipliers = [1.014, 0.987, 1.003, 0.996];
    req.ionosphere = Ionosphere {
        recompute: Recompute::KeepSporadicE,
        heights: Heights::Integrated,
        layers: (1..=5)
            .map(|point| EfVar {
                area: point,
                fi: [3.017_4, 4.523_9, 8.061_2],
                yi: [20.34, 40.71, 80.26],
                hi: [110.83, 200.17, 300.62],
            })
            .collect(),
        sporadic_e: (1..=5)
            .map(|point| EsVar {
                area: point,
                fs: [2.013, 3.047, 4.092],
                hs: 110.37,
            })
            .collect(),
        profile: None,
    };

    for task in [Task::Muf, Task::Systems, Task::Parameters] {
        let cards = deck(&req, task).expect("deck");
        let fortran = run_deck(&bin, root.path(), &cards).expect("reference run");
        let ported = listing(root.path(), &req, task).expect("ported run");
        assert_eq!(ported, fortran, "task {task:?} listing differs");
    }
}

/// `Compatible` is the reference's own output; `Corrected` is not.
///
/// Both halves matter. The first is the guarantee the whole port rests
/// on and must never break. The second says the corrected tier is
/// actually reaching the engine — a `Corrected` run that still matched
/// the reference byte for byte would mean the fixes were not wired up,
/// which is a silent failure a passing test would otherwise hide.
///
/// What the difference consists of is recorded in `docs/corrected.md`,
/// measured by `correctcheck` rather than asserted here.
#[test]
fn compatible_matches_the_reference_and_corrected_does_not() {
    let Some((bin, root)) = reference("api-model") else {
        return;
    };
    // A high-latitude path, because the implemented fix moves the
    // magnetic pole and a mid-latitude path barely notices.
    let mut req = a_request();
    req.tx = Site {
        name: "REYKJAVIK".to_string(),
        lat_deg: 64.15,
        lon_deg: -21.94,
    };
    req.rx = Site {
        name: "TROMSO".to_string(),
        lat_deg: 69.65,
        lon_deg: 18.96,
    };

    let cards = deck(&req, Task::Systems).expect("deck");
    let fortran = run_deck(&bin, root.path(), &cards).expect("reference run");

    let compatible = listing(
        root.path(),
        &Request {
            model: Model::Compatible,
            ..req.clone()
        },
        Task::Systems,
    )
    .expect("compatible run");
    assert_eq!(compatible, fortran, "Compatible must be the reference");

    let corrected = listing(
        root.path(),
        &Request {
            model: Model::Corrected,
            ..req.clone()
        },
        Task::Systems,
    )
    .expect("corrected run");
    assert_ne!(
        corrected, fortran,
        "Corrected should differ — the fixes are not reaching the engine"
    );
}

#[test]
fn predict_returns_the_shape_each_task_documents() {
    let Some((_, root)) = reference("api-shapes") else {
        return;
    };
    let req = a_request();
    match predict(root.path(), &req, Task::Parameters).expect("parameters") {
        // One row per control point per hour; this path has three
        // control points.
        Report::Parameters(rows) => assert_eq!(rows.len() % 24, 0),
        other => panic!("wrong report: {other:?}"),
    }
    match predict(root.path(), &req, Task::Muf).expect("muf") {
        Report::Muf(hours) => {
            assert_eq!(hours.len(), 24);
            assert!(hours.iter().all(|h| h.allmuf > 0.0));
            // The MUF task computes no LUF; the slot keeps its -1.
            assert!(hours.iter().all(|h| h.xluf < 0.0));
        }
        other => panic!("wrong report: {other:?}"),
    }
    match predict(root.path(), &req, Task::Systems).expect("systems") {
        Report::Systems(p) => {
            assert_eq!(p.hours.len(), 24);
            assert!(p.path.gcd_km > 0.0);
            // Method 30 keeps no all-modes detail.
            assert!(p.hours.iter().all(|h| h.allmodes.is_empty()));
        }
        other => panic!("wrong report: {other:?}"),
    }
    match predict(root.path(), &req, Task::AllModes).expect("all modes") {
        Report::Systems(p) => {
            assert!(p.hours.iter().all(|h| !h.allmodes.is_empty()));
            assert!(p.hours.iter().all(|h| !h.par.is_empty()));
        }
        other => panic!("wrong report: {other:?}"),
    }
}
