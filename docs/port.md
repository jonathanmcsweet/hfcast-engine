# The VOACAP port — method and status

The engine is being translated from Fortran 77 (`vendor/voacapl`, the
maintained ITS VOACAP) to Rust, module `propcore::engine`. Scope: the
point-to-point prediction path the app exercises — method 30, isotropic
antennas, single power, sporadic-E on. Area coverage, antenna pattern files
and the interactive front end are out of scope.

Method 30 internally means `MSPEC = 121` (`decred.for`): the short-path
systems model below 7000 km, the long-path model beyond 10000 km, and
**both models run and smoothed together between 7000 and 10000 km** — so
the systems-model stage must port both paths and the smoothing blend.

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

| stage                                 | Fortran                                                           | Rust                   | verified against trace                   |
| ------------------------------------- | ----------------------------------------------------------------- | ---------------------- | ---------------------------------------- |
| constants, magnetic pole              | `blkdat`, `set_magnetic_pole`                                     | `engine::con`          | via geometry                             |
| path geometry, control points         | `geom.for`                                                        | `engine::geometry`     | worst 3e-4 km / 1.3e-5 deg over 96 cases |
| magnetic field at control points      | `magvar.for`, `magfin.for`                                        | `engine::magnetic`     | worst 5e-8 over 408 control points       |
| coefficient loading                   | `redmap.for`                                                      | `engine::coefficients` | 819k elements, worst at print precision  |
| map evaluation, layer parameters      | `geotim`, `virtim`, `versy`, `noisy`, `ef1var`, `timvar`, `f2var` | `engine::ionosphere`   | 733k AB values, 9.8k point-hours         |
| sporadic E parameters                 | `esind`                                                           | `engine::ionosphere`   | 9.8k point-hours                         |
| sporadic E losses                     | `esreg`, `esmod`                                                  | —                      | (with the systems model)                 |
| MUF                                   | `ionset`, `lecden`, `gethp`, `f2dis`, `curmuf`                    | `engine::muf`          | 2.3k hours, 20 fields + profiles         |
| ionogram, reflectrix, deviative loss  | `sang`, `selmod`, `genion`, `fobby`, `alosfv`                     | `engine::ionogram`     | 4.6k area calls incl. exact reflectrix   |
| signal distribution, absorption       | `syssy`, `xlin`, `prbmuf`, `sigdis`                               | `engine::sigdis`       | 3.2k calls, 20 fields                    |
| noise                                 | `anois1`, `genfam`, `genois`                                      | `engine::noise`        | 70k calls, 13 fields                     |
| ground constants, path latitude       | `geom.for` land-mass lookup                                       | `engine::ionosphere`   | identical sea/land at every point        |
| systems model (modes, losses, signal) | `setlng`, `luffy` and relatives                                   | —                      |                                          |
| output fields                         | `setluf`, `outlin`                                                | —                      |                                          |

Working order is data flow, top to bottom. Each stage lands with its trace
instrumentation, its `porttest` comparison, and unit tests.

## Mode-loop notes (read, not yet ported)

The short-path per-frequency chain inside `LUFFY` is: `PENANG`
(penetration angles per layer) → `FINDF` (reflectrix search building the
`/REFLX/` tables with cusp inserts, skip and maximum distances, plus a
Martyn correction per entry) → per hop `FDIST` (up to six raysets by
distance interpolation) → `INMUF` (inserts over-the-MUF or
zero-distance modes; temporarily rescales the layer MUF distributions
for higher hop counts, restoring them after) → `REGMOD` (per-mode
losses: free space, D-E absorption with the collision-frequency term,
deviative loss, Es obscuration via `PRBMUF`, ground loss between hops
via `GAIN`'s Fresnel branch using the per-point ground constants,
over-the-MUF loss, the 2006 low-MUFday extra loss; produces `/ZON/`) →
`ESMOD`/`ESREG` (sporadic-E modes) → `ALLMODES` (accumulates into
`/allMODE/`, up to 20 modes) → `GENOIS` → `RELBIL` (combined
reliability) → `SERPRB`, `MPATH`, `OUTALL`. Inputs still to port:
ground constants from `geom.for` (`NOISY` plane 7 land-mass map: sea
5/80, land 0.001/4) and `PWRDB` (transmit power in dBW per antenna,
30 dBW default). The long-path chain (`GMLOSS`, `SELTXR`, `SETTXR`,
`LNGPAT`) and the 7000-10000 km smoothing blend at the end of `LUFFY`
complete the stage.
