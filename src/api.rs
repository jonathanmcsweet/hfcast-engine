//! The public face of the ported engine: structured inputs in,
//! structured results out.
//!
//! The engine underneath is a bug-compatible port of `voacapl` (ITS
//! VOACAP, Fortran 77), proven byte-identical to the reference over the
//! whole card surface. That proof is stated in card terms because the
//! reference can only be asked questions a card deck can express. This
//! module is for callers who are not writing card decks:
//!
//! - [`predict`] runs the engine and returns data.
//! - [`listing`] runs the engine and returns the text the reference
//!   program would print for the same question, byte for byte,
//!   including the echoed input deck.
//!
//! ## Inputs are quantised to the card grid
//!
//! A card column holds a fixed number of decimals, and the listing
//! prints its header from those same columns, so no listing can show
//! a finer value than a card can carry. Every request is therefore
//! put on the card grid before it runs
//! ([`crate::deck::DeckCase::as_written`]), which is what makes the
//! byte-identical guarantee hold for every request rather than for
//! carefully chosen ones. The grid:
//!
//! | Input | Grid |
//! | --- | --- |
//! | Latitude, longitude | 0.01 degree, about 1 km |
//! | Frequency | 0.01 MHz |
//! | Transmit power | 0.1 W |
//! | Required SNR | 0.1 dB |
//! | Man-made noise | 1 dB |
//! | Sunspot number | 1 |
//! | Antenna beam bearing | 0.1 degree |
//! | Layer frequencies, heights | 0.01 MHz, 0.1 km |
//!
//! Every step is far below what the model resolves, so this changes
//! no answer that meant anything. It is done openly rather than
//! silently: [`deck`] returns the cards a request resolves to, so the
//! values actually used are always visible.
//!
//! ## The card-stream cards
//!
//! `AUXIN`, `AUXOUT` and `PROCEDURE`/`END` are deliberately not here.
//! They redirect where cards are read from and where the listing is
//! written to; a caller who passes a [`Request`] and receives a
//! `String` has nothing for them to do, and they never touch the
//! model. `docs/roadmap-progress.md` records the decision.

use std::path::Path;

use crate::deck::{build_deck, DeckCase};
use crate::engine::output::render;
use crate::engine::run::{run_ion, run_listing, run_luf, run_muf, run_par, RunInputs};

// Everything a caller needs to build a request or read a report, so
// nothing forces them into the harness or engine modules.
pub use crate::deck::{AntennaChoice, Edp, EfVar, EsVar, FREQ_SLOTS};
pub use crate::engine::coefficients::FoF2Model;
pub use crate::engine::model::Model;
pub use crate::engine::modes::{AllModesOut, Son};
pub use crate::engine::run::{
    HourPrediction, IonPlot, MufHourOut, ParRow, PathReport, Prediction,
};

/// One end of the path.
#[derive(Debug, Clone)]
pub struct Site {
    /// Printed in the listing header; at most twenty characters reach
    /// the page.
    pub name: String,
    /// Degrees, north positive.
    pub lat_deg: f64,
    /// Degrees, east positive.
    pub lon_deg: f64,
}

/// How deep a run goes, and therefore what a [`Report`] holds.
///
/// The card deck says this with a method number that also selects a
/// print format; here the two are separate, and [`listing`] maps each
/// task back to one canonical card method.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Task {
    /// The ionospheric layer parameters at each control point, and
    /// nothing after them. Card method 1.
    Parameters,
    /// The vertical ionograms at each control point. Card method 2.
    Ionograms,
    /// MUF and FOT by the manual nomogram method, which fills no
    /// per-layer detail. Card method 3.
    MufNomogram,
    /// MUF, FOT and HPF from the full electron-density profile. Card
    /// method 7.
    Muf,
    /// The MUF computation plus the LUF search. Card method 26.
    Luf,
    /// The complete systems prediction: modes, signal, noise, SNR and
    /// reliability at every frequency. Card method 30.
    Systems,
    /// The systems prediction with the all-modes detail kept: every
    /// mode's distribution at every frequency, plus the layer
    /// parameters each hour. Card method 25.
    AllModes,
}

impl Task {
    /// The card method [`listing`] prints this task as.
    pub fn method(self) -> u32 {
        match self {
            Task::Parameters => 1,
            Task::Ionograms => 2,
            Task::MufNomogram => 3,
            Task::Muf => 7,
            Task::Luf => 26,
            Task::Systems => 30,
            Task::AllModes => 25,
        }
    }
}

/// How much of the per-hour ionosphere is recomputed: the `EXECUTE`
/// card's `KRUN` field.
///
/// Anything other than [`Recompute::Everything`] keeps some of the
/// previous hour's arrays, which is what lets the values in
/// [`Ionosphere::layers`] and [`Ionosphere::sporadic_e`] stand instead
/// of being overwritten. The reference has no code to restore what the
/// readers of those arrays change in place, so under any keep option
/// the ionosphere drifts hour to hour; the port reproduces the drift.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Recompute {
    /// `KRUN 0`: everything, every hour.
    #[default]
    Everything,
    /// `KRUN 1`: keep the sporadic-E parameters, recompute the rest.
    KeepSporadicE,
    /// `KRUN 2`: keep the E, F1 and F2 layer parameters too.
    KeepLayers,
    /// `KRUN 3`: keep everything, including the virtual-height work.
    KeepEverything,
}

impl Recompute {
    fn krun(self) -> i32 {
        match self {
            Recompute::Everything => 0,
            Recompute::KeepSporadicE => 1,
            Recompute::KeepLayers => 2,
            Recompute::KeepEverything => 3,
        }
    }
}

/// Where layer heights come from: the `INTEGRATE` card.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Heights {
    /// No card: every height is read off the electron-density profile.
    #[default]
    FromProfile,
    /// With the card: the E layer takes fixed heights and a profile
    /// with no F1 layer takes parabolic segments.
    Integrated,
}

/// The ionosphere controls, all defaulting to a plain run.
#[derive(Debug, Clone, Default)]
pub struct Ionosphere {
    pub recompute: Recompute,
    pub heights: Heights,
    /// Per control point, the E, F1 and F2 critical frequency,
    /// semithickness and height: the `EFVAR` cards. Meaningful with a
    /// [`Recompute`] that keeps them.
    pub layers: Vec<EfVar>,
    /// Per control point, the sporadic-E deciles and reflection
    /// height: the `ESVAR` cards.
    pub sporadic_e: Vec<EsVar>,
    /// A fixed electron-density profile: the `EDP` card. The engine
    /// then leaves the profile alone instead of building one.
    pub profile: Option<Edp>,
}

/// One prediction request. Fields mirror the physical question, not
/// the card deck; [`predict`] and [`listing`] resolve them to the same
/// engine inputs, so the data and the text always describe one run.
#[derive(Debug, Clone)]
pub struct Request {
    pub tx: Site,
    pub rx: Site,
    /// 1 to 12.
    pub month: u32,
    /// Printed in the listing header; no computation reads it.
    pub year: u32,
    /// Smoothed sunspot number.
    pub ssn: f64,
    /// Transmit power in watts, taken by any transmit antenna card
    /// that does not carry its own kilowatts field.
    pub power_watts: f64,
    /// At most [`FREQ_SLOTS`] frequencies, in MHz.
    pub freqs_mhz: Vec<f64>,
    /// Signal-to-noise ratio the service needs, in dB.
    pub required_snr_db: f64,
    /// Man-made noise at 3 MHz, as a positive number of dB below 1 W.
    pub noise_dbw: f64,
    /// Which foF2 coefficient set to read.
    pub fof2: FoF2Model,
    /// The E, F1, F2 and sporadic-E critical-frequency multipliers:
    /// the `FPROB` card. `[1.0, 1.0, 1.0, 0.0]` is standard practice
    /// (sporadic E off); a fourth value above zero turns it on.
    pub layer_multipliers: [f64; 4],
    /// The antennas at each end, in search order: the first card whose
    /// frequency range holds the frequency serves it. Empty is one
    /// isotrope over 2 to 30 MHz.
    pub tx_antennas: Vec<AntennaChoice>,
    pub rx_antennas: Vec<AntennaChoice>,
    pub ionosphere: Ionosphere,
    /// Whether the run reproduces VOACAP's documented defects or
    /// fixes them. [`Model::Compatible`] by default, which is the
    /// only tier proven identical to the reference.
    pub model: Model,
}

/// What a [`Task`] computed.
#[derive(Debug, Clone)]
pub enum Report {
    /// One row per control point per hour, in hour order.
    Parameters(Vec<ParRow>),
    /// One ionogram per control point, per hour.
    Ionograms(Vec<Vec<IonPlot>>),
    /// One entry per hour, 0100 to 2400 UT. [`Task::Luf`] fills the
    /// LUF slot; the MUF tasks leave it at -1.
    Muf(Vec<MufHourOut>),
    /// The full systems run: 24 hours plus the path description the
    /// listing header prints.
    Systems(Prediction),
}

/// Runs the engine against the data tree at `itshfbc` and returns the
/// task's data.
pub fn predict(itshfbc: &Path, req: &Request, task: Task) -> Result<Report, String> {
    let case = deck_case(req, task)?;
    let mut inp = RunInputs::from(&case);
    inp.model = req.model;
    let inp = inp;
    match task {
        Task::Parameters => run_par(itshfbc, &inp).map(Report::Parameters),
        Task::Ionograms => run_ion(itshfbc, &inp).map(Report::Ionograms),
        Task::MufNomogram | Task::Muf => run_muf(itshfbc, &inp).map(Report::Muf),
        Task::Luf => run_luf(itshfbc, &inp).map(Report::Muf),
        Task::Systems | Task::AllModes => run_listing(itshfbc, &inp).map(Report::Systems),
    }
}

/// Runs the engine and returns the listing the reference program would
/// print for this request, byte for byte — the echoed deck, the
/// header pages and the task's table or graph.
pub fn listing(itshfbc: &Path, req: &Request, task: Task) -> Result<String, String> {
    let case = deck_case(req, task)?;
    let deck = build_deck(&case).map_err(|e| e.to_string())?;
    render(itshfbc, &case, &deck, req.model)
}

/// The card deck this request resolves to: what the reference program
/// would be given to ask the same question. This is what [`listing`]
/// echoes, and what a caller who still drives a `voacapl` binary
/// feeds it.
pub fn deck(req: &Request, task: Task) -> Result<String, String> {
    let case = deck_case(req, task)?;
    build_deck(&case).map_err(|e| e.to_string())
}

/// Resolves a request to the card deck that asks the same question.
///
/// Both entry points go through this one conversion, so the data and
/// the text cannot come from two different descriptions of the run.
fn deck_case(req: &Request, task: Task) -> Result<DeckCase, String> {
    validate(req)?;
    Ok(DeckCase {
        // Quantised at the end of this function, so the deck text and
        // the numbers the engine runs on are one description.
        id: req.tx.name.clone(),
        rx_label: req.rx.name.clone(),
        from_lat: req.tx.lat_deg,
        from_lon: req.tx.lon_deg,
        to_lat: req.rx.lat_deg,
        to_lon: req.rx.lon_deg,
        method: task.method(),
        ursi: req.fof2 == FoF2Model::Ursi,
        month: req.month,
        year: req.year,
        ssn: req.ssn,
        watts: req.power_watts,
        required_snr_db: req.required_snr_db,
        noise_dbw: req.noise_dbw,
        freqs_mhz: req.freqs_mhz.clone(),
        tx_antennas: req.tx_antennas.clone(),
        rx_antennas: req.rx_antennas.clone(),
        sporadic_e: req.layer_multipliers[3] > 0.0,
        fprob: Some(req.layer_multipliers),
        botlines: None,
        toplines: None,
        krun: req.ionosphere.recompute.krun(),
        efvar: req.ionosphere.layers.clone(),
        esvar: req.ionosphere.sporadic_e.clone(),
        edp: req.ionosphere.profile.clone(),
        extra_cards: Vec::new(),
        comment: None,
        integrate: match req.ionosphere.heights {
            Heights::FromProfile => None,
            Heights::Integrated => Some(1),
        },
        outgraph: None,
    }
    .as_written())
}

/// The structural checks — the limits that are the engine's rather
/// than the card's, so a caller learns about them before a run rather
/// than through a field-width error.
///
/// Eleven frequencies is one of them: `COMMON/FRQ/` dimensions the
/// array at 29, but the driver overwrites slot 12 with the MUF every
/// hour and reads slot 14 as a flag, so eleven is what the engine
/// supports and not merely what the card has room for.
fn validate(req: &Request) -> Result<(), String> {
    if !(1..=12).contains(&req.month) {
        return Err(format!("month {} is not 1 to 12", req.month));
    }
    if req.freqs_mhz.is_empty() || req.freqs_mhz.len() > FREQ_SLOTS {
        return Err(format!(
            "{} frequencies given; 1 to {FREQ_SLOTS} are supported",
            req.freqs_mhz.len()
        ));
    }
    for e in &req.ionosphere.layers {
        if !(1..=5).contains(&e.area) {
            return Err(format!("EFVAR control point {} is not 1 to 5", e.area));
        }
    }
    for e in &req.ionosphere.sporadic_e {
        if !(1..=5).contains(&e.area) {
            return Err(format!("ESVAR control point {} is not 1 to 5", e.area));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::output::{itout, itrun};

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
            freqs_mhz: vec![7.2, 14.2],
            required_snr_db: 24.0,
            noise_dbw: 145.0,
            fof2: FoF2Model::Ccir,
            layer_multipliers: [1.0, 1.0, 1.0, 0.0],
            tx_antennas: Vec::new(),
            rx_antennas: Vec::new(),
            ionosphere: Ionosphere::default(),
            model: Model::default(),
        }
    }

    #[test]
    fn each_task_maps_to_a_method_that_runs_and_prints_what_it_claims() {
        // `ITRUN` says what the method computes, `ITOUT` what it
        // prints; the task must pick a method whose two entries match
        // the report variant and table it documents.
        assert_eq!(itrun(Task::Parameters.method()), 1);
        assert_eq!(itrun(Task::Ionograms.method()), 2);
        assert_eq!(itrun(Task::MufNomogram.method()), 3);
        assert_eq!(itrun(Task::Muf.method()), 4);
        assert_eq!(itrun(Task::Luf.method()), 8);
        assert_eq!(itrun(Task::Systems.method()), 7);
        assert_eq!(itrun(Task::AllModes.method()), 7);
        assert_eq!(itout(Task::Luf.method()), 3);
        assert_eq!(itout(Task::AllModes.method()), 9);
    }

    #[test]
    fn a_request_resolves_to_a_writable_deck() {
        let case = deck_case(&a_request(), Task::Systems).expect("case");
        let deck = build_deck(&case).expect("deck");
        assert!(deck.contains("LABEL     TANGIER             BELGRADE"));
        assert!(deck.contains("METHOD       30"));
    }

    #[test]
    fn both_entry_points_share_one_description_of_the_run() {
        let mut req = a_request();
        req.ionosphere.recompute = Recompute::KeepLayers;
        req.ionosphere.heights = Heights::Integrated;
        let case = deck_case(&req, Task::Muf).expect("case");
        assert_eq!(case.krun, 2);
        assert_eq!(case.integrate, Some(1));
        let inp = RunInputs::from(&case);
        assert_eq!(inp.krun, 2);
        assert_eq!(inp.iedp, 1);
        assert_eq!(inp.method, 7);
    }

    #[test]
    fn structural_errors_are_reported_before_the_engine_runs() {
        let mut req = a_request();
        req.month = 13;
        assert!(predict_error(&req).contains("month"));

        let mut req = a_request();
        req.freqs_mhz = vec![7.1; FREQ_SLOTS + 1];
        assert!(predict_error(&req).contains("frequencies"));

        let mut req = a_request();
        req.ionosphere.layers.push(EfVar {
            area: 6,
            fi: [3.0; 3],
            yi: [20.0; 3],
            hi: [110.0; 3],
        });
        assert!(predict_error(&req).contains("control point"));
    }

    fn predict_error(req: &Request) -> String {
        deck_case(req, Task::Systems).expect_err("should fail validation")
    }
}
