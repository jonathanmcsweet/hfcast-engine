//! Characterisation harness for the HF propagation engine.
//!
//! The engine in use today is `voacapl`, the maintained Unix build of the ITS
//! Fortran VOACAP program. Porting it to Rust needs an acceptance criterion,
//! and this crate exists to derive that criterion from measurement rather than
//! from opinion.
//!
//! The pieces:
//!
//! - [`api`] is the public face of the ported engine: structured
//!   requests in, structured reports or a reference-identical listing
//!   out. Library callers start here.
//! - [`deck`] writes VOACAP's fixed-width input deck.
//! - [`listing`] reads every numeric field back out of a method 30 listing.
//! - [`sweep`] enumerates input cases covering the model's regimes.
//! - [`fuzz`] generates valid decks from a seed, for combinations the
//!   hand-built sweep does not contain.
//! - [`runner`] drives a chosen `voacapl` binary.
//! - [`compare`] measures how far two listings differ, field by field.
//! - [`spread`] pairs the engine's day-to-day spread claims with measured days.
//! - [`geomag`] reads the GFZ Kp record, so days can be split by storm state.
//! - [`irtam`] converts real-time IRTAM foF2 maps into VOACAP coefficient files.
//!
//! All of it survives the port: the same parser and comparator that measure
//! compiler-to-compiler spread today will measure Rust-to-Fortran spread later.

pub mod api;
pub mod compare;
pub mod deck;
pub mod engine;
pub mod fuzz;
pub mod geomag;
pub mod irtam;
pub mod itu;
pub mod listing;
pub mod runner;
pub mod spread;
pub mod stats;
pub mod sweep;
pub mod wspr;
