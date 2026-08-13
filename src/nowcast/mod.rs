//! The second prediction pipeline: measured accuracy, not parity.
//!
//! The pipeline under `src/voacap/` is a faithful port whose contract is
//! byte-identical agreement with the Fortran reference. That contract is
//! why it cannot become more accurate: every day of a month gets the
//! same answer, and no number may move. This module tree is the other
//! pipeline. Its contract is the ionosonde harness (`src/sonde.rs`,
//! results in `docs/ionosonde.md`): a change ships when the measured
//! error goes down, and the parity engine is untouched beside it.
//!
//! The skeleton answers point questions through the parity engine's own
//! physics, then applies the corrections the harness proved:
//!
//! - the ordinary-wave convention for foF2 (the raw engine value
//!   carries half the gyrofrequency),
//! - Dudeney's corrected height form (about 19 km of the measured
//!   +61 km hmF2 bias is the plain formula),
//! - a fitted daily index in place of the monthly smoothed sunspot
//!   number ([`api::Conditioning::Daily`]), and
//! - the Kp-conditioned storm table (`src/stormfit.rs`) on top of it.
//!
//! `sonde --engine nowcast` checks this plumbing against the research
//! columns of the harness cache, so the deployable API and the measured
//! tables cannot drift apart silently. Later phases replace the inner
//! physics with a batch-shaped form; the API and the ruler stay.

pub mod api;
pub mod grid;
pub mod packed;
