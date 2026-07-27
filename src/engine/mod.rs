//! The VOACAP port, stage by stage.
//!
//! This is a translation of the point-to-point prediction path of `voacapl`
//! (ITS VOACAP, Fortran 77) into Rust, scoped to what the app exercises:
//! method 30, isotropic antennas, single transmit power, sporadic-E on. The
//! area-coverage code, antenna pattern files and the interactive front end
//! are out of scope.
//!
//! ## Correctness method
//!
//! Two layers of tests, both against the real engine rather than opinion:
//!
//! 1. **Stage traces.** `tools/build-trace.sh` builds the Fortran with the
//!    patches in `trace/*.patch`, which dump each ported stage's
//!    intermediate values when `PROPCORE_TRACE` names a directory. Each
//!    Rust stage is compared against those dumps over the sweep cases
//!    (`porttest` binary). This localises any disagreement to the stage
//!    that caused it — indispensable in a program built from global state
//!    and 783 GOTOs.
//! 2. **The tolerance envelope.** The finished engine must stay inside
//!    `docs/sensitivity.md` on the full sweep: no further from the `-O2`
//!    reference than IEEE-conformant rebuilds of the reference are from
//!    each other (worst case 1 dB of SNR), with zero structural
//!    disagreements.
//!
//! ## Precision
//!
//! The Fortran computes in 4-byte REAL throughout, so this port uses `f32`
//! as its working type on purpose. Porting to `f64` would be "better"
//! arithmetic but a *different* model — branch decisions (mode selection,
//! layer choice) can flip near thresholds, and the tolerance envelope was
//! derived for evaluation-order noise, not precision changes. Upgrading
//! precision is a deliberate post-port change to make once the f32 port is
//! proven equivalent.
//!
//! ## Stage map (Fortran → module)
//!
//! | Fortran | module | ported |
//! | --- | --- | --- |
//! | `blkdat.for` constants, `north_pole.txt` | [`con`] | yes |
//! | `geom.for` path geometry, control points | [`geometry`] | yes |
//! | `magvar.for` magnetic field at control points | [`magnetic`] | yes |
//! | `redmap.for` coefficient loading | [`coefficients`] | yes |
//! | `geotim/virtim/versy/noisy/ef1var/timvar/f2var/esind` layer parameters | [`ionosphere`] | yes |
//! | `esreg/esmod` sporadic E losses | [`modes`] | yes |
//! | `ionset/lecden/gethp/f2dis/curmuf` MUF | [`muf`] | yes |
//! | `sang/selmod/genion/fobby/alosfv` ionogram tables | [`ionogram`] | yes |
//! | `syssy/xlin/prbmuf/sigdis` signal distribution | [`sigdis`] | yes |
//! | `luffy` mode loop, long path, smoothing | [`modes`] | yes |
//! | `anois1/genfam/genois` noise | [`noise`] | yes |
//! | `setluf/outlin` output fields | [`modes`] `setluf` | line output open |

pub mod antenna;
pub mod area;
pub mod ccir;
pub mod coefficients;
pub mod con;
pub mod geometry;
pub mod graphs;
pub mod hfmufes;
pub mod ioncap;
pub mod ionogram;
pub mod ionosphere;
pub mod magnetic;
pub mod model;
pub mod modes;
pub mod muf;
pub mod noise;
pub mod output;
pub mod run;
pub mod sigdis;
pub mod tables;
