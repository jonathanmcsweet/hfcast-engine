//! Each implemented fix reaches the engine.
//!
//! `docs/corrected.md` says what every fix moves, measured by
//! `correctcheck`. These tests are the much smaller claim underneath
//! that: the switch is wired to the site at all. A fix that silently
//! stopped taking its branch — a renamed accessor left unread, a
//! refactor that dropped the model on its way down — would make
//! `correctcheck` report "no movement", which reads exactly like a fix
//! that changes nothing, and no other test would notice.
//!
//! One test per corpus rather than per fix: the corpus is what makes a
//! fix visible, and a fix with no corpus that reaches it cannot be
//! guarded here at all.
//!
//! The tests skip (and say so) without the data tree, so `cargo test`
//! stays runnable anywhere.

use propcore::deck::{build_deck, AntennaChoice, DeckCase};
use propcore::engine::model::{Fixes, Model};
use propcore::engine::output::render;
use propcore::fuzz::fuzz_cases;
use propcore::runner::{itshfbc_dir, IsolatedRoot};
use propcore::sweep::sweep_cases;

/// The first few method-26 cases from the fuzz corpus: the decks that
/// run the LUF search. Their frequencies are ignored — the LUF methods
/// sweep their own complement — so the corpus contributes geometry,
/// season and antennas.
fn luf_cases() -> Vec<DeckCase> {
    let mut cases = fuzz_cases(0, 6);
    for case in cases.iter_mut() {
        case.method = 26;
    }
    cases
}

/// One sweep case with a `KOP = 6` curtain at both ends, the only way
/// to reach the IONCAP curtain pattern. The short European path,
/// because a short path uses the high elevations the defect floors.
fn curtain_case() -> Vec<DeckCase> {
    let mut cases = sweep_cases();
    cases.truncate(1);
    for case in cases.iter_mut() {
        let curtain = AntennaChoice::whole_band("samples/sample.26", 0.0);
        case.tx_antennas = vec![curtain.clone()];
        case.rx_antennas = vec![curtain];
    }
    cases
}

/// Renders every case twice and returns how many printed differently.
fn cases_moved_by(cases: Vec<DeckCase>, fixes: Fixes, tag: &str) -> Option<usize> {
    if !itshfbc_dir().is_dir() {
        eprintln!("skipped: no itshfbc data tree on this machine");
        return None;
    }
    let root = IsolatedRoot::create(tag).expect("isolated itshfbc tree");
    let mut moved = 0;
    for case in cases {
        let deck = build_deck(&case).expect("deck");
        let base = render(root.path(), &case, &deck, Model::Compatible).expect("compatible run");
        let fixed = render(root.path(), &case, &deck, Model::from_fixes(fixes))
            .expect("run with the fix on");
        if base != fixed {
            moved += 1;
        }
    }
    Some(moved)
}

#[test]
fn the_luf_scan_fix_reaches_the_luf_search() {
    let fixes = Fixes {
        luf_scan_best: true,
        ..Fixes::default()
    };
    let Some(moved) = cases_moved_by(luf_cases(), fixes, "fix-luf-scan") else {
        return;
    };
    assert!(
        moved > 0,
        "no method-26 case moved: the luf_scan_best branch is not being taken"
    );
}

#[test]
fn the_luf_pass_area_fix_reaches_the_luf_search() {
    let fixes = Fixes {
        luf_pass_area: true,
        ..Fixes::default()
    };
    let Some(moved) = cases_moved_by(luf_cases(), fixes, "fix-luf-area") else {
        return;
    };
    // This fix bites only where the electron-density chain left a
    // second area behind, which is a minority of cases; `corrected.md`
    // records how small a minority. Case `fz000003` is in the first
    // six, which is why six is the number rendered here.
    assert!(
        moved > 0,
        "no method-26 case moved: the luf_pass_area branch is not being taken"
    );
}

#[test]
fn the_curtain_fix_reaches_the_ioncap_pattern() {
    let fixes = Fixes {
        curtain_elevation: true,
        ..Fixes::default()
    };
    let Some(moved) = cases_moved_by(curtain_case(), fixes, "fix-curtain") else {
        return;
    };
    assert!(
        moved > 0,
        "the curtain case did not move: the curtain_elevation branch is not being taken"
    );
}
