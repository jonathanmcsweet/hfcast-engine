# The VOACAP port — method and status

The engine is being translated from Fortran 77 (`vendor/voacapl`, the
maintained ITS VOACAP) to Rust, module `propcore::engine`. Scope: the
point-to-point prediction path the app exercises — method 30, isotropic
antennas, single power, sporadic-E on. Area coverage, antenna pattern files
and the interactive front end are out of scope.

## Why the port

Not accuracy — a faithful port produces the same numbers by definition, and
the corrected model built on those numbers is already validated
(`accuracy.md`). The port buys: no Fortran toolchain, no fixed-width card
decks, no per-run private-directory workaround for the engine's shared
scratch files, deployment anywhere Rust runs, and code a person can read.

## Correctness method

1. **Stage traces.** `tools/build-trace.sh` builds the `trace` variant:
   the reference source with the instrumented files in `trace/` copied in.
   Each ported stage dumps its intermediates when `PROPCORE_TRACE` names a
   directory. The `porttest` binary runs that engine over the 96 sweep
   cases and compares every dumped value with the Rust stage, reporting the
   worst difference per field. A porting mistake surfaces in the first
   stage that contains it.
2. **The tolerance envelope.** The finished engine must stay inside
   `sensitivity.md` on the full sweep: no further from the `-O2` reference
   than IEEE-conformant rebuilds are from each other (worst case 1 dB SNR,
   zero structural disagreements).

## Precision policy

The Fortran computes in 4-byte REAL, so the port uses `f32` (`con::R`) on
purpose, and writes expressions in the source's order. Double precision
would be a _different_ model — branch decisions near thresholds can flip —
and the tolerance envelope covers evaluation-order noise, not precision
changes. Upgrading is a deliberate post-port step.

Bugs are kept, not fixed, and documented where they live. First example:
`set_magnetic_pole` builds its database path without a separator, so the
installed `database/north_pole.txt` (79.5) is silently ignored and the
built-in pole (78.5, -69.0) is what every run actually uses. The port
reproduces the malformed lookup (`con::MagneticPole::for_tree`); the
geometry trace only matches this way.

## Stage status

| stage                                 | Fortran                               | Rust               | verified against trace                   |
| ------------------------------------- | ------------------------------------- | ------------------ | ---------------------------------------- |
| constants, magnetic pole              | `blkdat`, `set_magnetic_pole`         | `engine::con`      | via geometry                             |
| path geometry, control points         | `geom.for`                            | `engine::geometry` | worst 3e-4 km / 1.3e-5 deg over 96 cases |
| magnetic field at control points      | `magvar.for`                          | —                  |                                          |
| coefficient loading                   | `redmap.for`                          | —                  |                                          |
| map evaluation (foF2, M3000, etc.)    | `geotim`, `virtim`, `timvar`, `f2var` | —                  |                                          |
| sporadic E                            | `esind`, `esreg`, `esmod`             | —                  |                                          |
| MUF                                   | `ionset`, `curmuf`                    | —                  |                                          |
| systems model (modes, losses, signal) | `luffy` and relatives                 | —                  |                                          |
| noise                                 | `noisy`, `genois`, `anois1`           | —                  |                                          |
| output fields                         | `setluf`, `outlin`                    | —                  |                                          |

Working order is data flow, top to bottom. Each stage lands with its trace
instrumentation, its `porttest` comparison, and unit tests.
