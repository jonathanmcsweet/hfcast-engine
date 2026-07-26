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
2. **Randomized decks.** `fuzz` generates valid decks from a seed and
   requires the two engines to print identical listings. The sweep only
   holds combinations somebody chose; this covers the rest, cycles
   through six distance bands so short and near-antipodal paths are
   always represented, and reports a case index that reproduces any
   failure exactly (`--seed N`). Refusing the same case counts as
   agreement: the reference stops on some inputs and the port stops on
   the same ones. **Result (2026-07-26): 600 cases identical, 2,031,840
   cells and 100,872 mode labels.** `porttest --seed N` runs one
   generated case through the stage traces and `--fuzz N` runs a batch,
   because a difference the listing does not print is invisible to the
   whole-engine check: that is how the sporadic-E-off disagreement
   below was found.
3. **The tolerance envelope.** The finished engine must stay inside
   `sensitivity.md` on the full sweep: no further from the `-O2` reference
   than IEEE-conformant rebuilds are from each other (worst case 1 dB SNR,
   zero structural disagreements). **Result (2026-07-26, `portcheck`): the
   port is bit-identical at the listing level — 463,104 printed cells and
   23,040 mode labels over the 96 sweep cases with zero differences.**

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
| sporadic E losses                     | `esreg`, `esmod`                                                  | `engine::modes`        | with the mode loop below                 |
| MUF                                   | `ionset`, `lecden`, `gethp`, `f2dis`, `curmuf`                    | `engine::muf`          | 2.3k hours, 20 fields + profiles         |
| ionogram, reflectrix, deviative loss  | `sang`, `selmod`, `genion`, `fobby`, `alosfv`                     | `engine::ionogram`     | 4.6k area calls incl. exact reflectrix   |
| signal distribution, absorption       | `syssy`, `xlin`, `prbmuf`, `sigdis`                               | `engine::sigdis`       | 3.2k calls, 20 fields                    |
| noise                                 | `anois1`, `genfam`, `genois`                                      | `engine::noise`        | 70k calls, 13 fields                     |
| ground constants, path latitude       | `geom.for` land-mass lookup                                       | `engine::ionosphere`   | identical sea/land at every point        |
| mode loop (raysets, losses, Es modes) | `penang`, `findf`, `fdist`, `inmuf`, `regmod`, `esmod`, `esreg`   | `engine::modes`        | 46k reflectrix, 49k hop, 32k mode dumps  |
| long-path model                       | `gmloss`, `settxr`, `seltxr`, `lngpat` and helpers                | `engine::modes`        | 14.4k two-end loss tables, exact rows    |
| reliability, per-frequency outputs    | `relbil`, `serprb`, `mpath`, `setlng`, the smoothing blend        | `engine::modes`        | 31.7k slots + 8.6k smoothed, 24 fields   |
| output fields, whole engine           | `setluf`, `outbod` listing body, `hfmufs` hour loop               | `engine::run`          | listing bit-identical over 96 cases      |

Working order is data flow, top to bottom. Each stage lands with its trace
instrumentation, its `porttest` comparison, and unit tests.

## Sporadic E off

The `FPROB` card multiplies each critical frequency, and its fourth
value is sporadic E. At zero every control point has `foEs = 0`, so
`CURMUF` skips all of them, leaves its running minimum at the initial
1000 and takes the branch that zeroes the whole Es layer. The engine
honours this; the stage harness used to pass all four multipliers as 1
regardless of the deck, so an Es-off case compared two different
questions and disagreed about the Es layer's MUF hop count. The sweep
fixes the flag on, which is why only generated cases exposed it.

## Mode-loop notes (`engine::modes`)

The `LUFFY` frequency loop is one Rust module. The short-path chain per
frequency: `penang` (penetration angles) → `findf` (the reflectrix with
cusp inserts and the Martyn spherical correction) → per hop `fdist` (up
to six raysets) → `inmuf` (over-the-MUF and zero-distance inserts, with
the temporary layer-MUF rescale) → `regmod` (per-mode losses into the
`/ZON/` slots) → `esmod` (two sporadic-E hops) → `esreg` (slot presets
only — its mixed-mode body is dead code behind an unconditional
`RETURN`) → `allmodes` accumulation → `relbil`/`serprb`/`mpath`. The
long-path chain is `findf` at both ends → `gmloss`/`settxr` → `seltxr`
→ `lngpat` (with `convh`, `gettop`, `tabs`, `babs`). Between 7000 and
10000 km both passes run and `luffy_smooth` applies the VOA-memo blend.

Fortran facts the module preserves: `/SON/`, `/REFLX/`, `/ZON/` and
`/allMODE/` persist across hours and are read stale (a frequency with
no modes keeps the previous values — `ModeLoopState` carries them per
case); `OUTBOD` overwrites the MUF slot with "NA" sentinels above
30 MHz after each hour's output; antennas are the isotrope so `GAIN`
reduces to constants except its Fresnel ground-reflection branch; and
`PWRDB` is the single deck power in dBW.
