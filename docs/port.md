# The VOACAP port — method and status

The engine is being translated from Fortran 77 (`vendor/voacapl`, the
maintained ITS VOACAP) to Rust, module `propcore::engine`.

The first target was the point-to-point path this app exercises —
method 30, isotropic antennas, single power — and that is finished and
bit-identical. The goal since 2026-07-26 is every code path of the
`voacapl` program, so the port can be a library others depend on;
`docs/roadmap.md` holds the remaining stages. The interactive front
end, plotting, and the sibling engines ICEPAC and REC533 stay out of
scope.

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
   the same ones. **Result (2026-07-26): 600 isotrope cases identical
   (2,031,840 cells), then 300 cases with directional antennas drawn
   from every computable family, also identical (1,011,360 cells).** `porttest --seed N` runs one
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

## Antennas (`engine::antenna`)

The engine never reads an antenna definition file while predicting.
`ANTCALC` runs first, turns each `ANTENNA` card into a table of 30
frequencies by 91 elevation angles, and writes `run/gainNN.dat`;
`DECRED` reads that back and `GAIN` interpolates it per mode. The file
is the interface between the two halves, so it is also the verification
surface — `antcheck` compares this module's table against the one the
shipped engine wrote, needing no instrumented build, at the 0.001 dB
its `f7.3` fields carry. Both sides are rounded to each field's own
format before comparing, because the reference's digits come back
through a 32-bit float.

Status over the 73 definition files in the tree (`antcheck`):

| family       | types          | files | ported |
| ------------ | -------------- | ----: | ------ |
| isotrope     | 0              |     3 | yes    |
| gain tables  | 10, 11, 13, 14 |     8 | yes    |
| NOSC         | 48             |     1 | yes    |
| IONCAP       | 21-30          |    10 | yes    |
| CCIR REC705  | 1-9            |    34 | yes    |
| NTIA curtain | 12             |     1 | yes    |
| HFMUFES      | 31-47          |    14 | no     |
| Harris       | 90+            |     2 | no     |

The ported families match on every cell: 196,386 compared — 71 of
the 73 definition files. The two Harris files (types 90+) cannot be
computed by anyone: the reference shells out to an external
`anttypNN` program that is not in the distribution and STOPs without
it ("Fatal error in subroutine harris"), so both engines refusing
those files is agreement. Unported
families return an error rather than a number, so the report lists
remaining work instead of passing silently.

The HFMUFES family (`engine::hfmufes`) is the deepest: each pattern
computes its input impedance from cosine-integral expressions and
complex mutual-impedance matrices, with Gauss-Jordan inversion for
the Yagi and log-periodic currents and 48-point Gaussian integration
for the tilted dipole and radial-ground monopole. Matching it
bit-for-bit required reproducing the reference's complex arithmetic
exactly: `CABS` is `hypotf`, division is Smith's algorithm as gfortran
inlines it, `CSQRT` is glibc's algorithm. Two Fortran facts are
modelled explicitly: only six locals are in the `SAVE` statement, but
the Yagi and log-periodic currents also survive between calls on
gfortran's stack by accident (`MufesState` carries them honestly);
and the log-periodic's ground-reflection sum reads a DO index one
past its loop, so that term is always zero.

The CCIR family (`engine::ccir`) surfaced the porting rule that cost
the most this stage: Fortran's exponent binds before multiplication,
so `aa * rl**2` is `aa` times the square, and flattening it to
`(aa * rl) * rl` rounds differently — three printed digits flipped
across two log-periodic files until the `ria` fit, the `shf`
polynomial and the curtain's `FZTHR` product were re-associated to the
source's order. Two dead-code findings are recorded in the module:
`antinit2` returns before `parmprec` for type 10, so the monopole's
Bessel and surface-impedance branch is unreachable, and `dirgain` has
no callers. The trig tables also keep their doctored end values —
`b(0)` is `cos(0.005)` in radians, the one entry missing the degree
factor.

The IONCAP family (`engine::ioncap`) added two findings. The curtain
pattern compares its elevation against the integer literal `0001` —
one radian, not the intended `.0001` — so every elevation above about
33 degrees takes the floor gain, plus the elements-per-bay count that
`SOK` still holds on that path. And `ionGAIN2`'s locals are `SAVE`d,
with one read before it is written: the zero-elevation exit jumps to
the efficiency dispatch, whose monopole arm reads the height left over
from the previous call (`IoncapState` carries it). Getting the family
to match also required `DAZEL0` to take `REAL*4` inputs: a coordinate
arrives already rounded to single precision and is then widened, and
passing the exact double instead moved the azimuth one ULP and flipped
borderline printed digits.

Two things the module records. `DAZEL0` is the antenna half's own
geometry — double precision, 6370 km Earth — so the azimuth a pattern
is cut along is deliberately not the path azimuth
`engine::geometry` computes. And the card's last field is transmit
power in kW, except on a receive card where a non-zero value is reused
as the isotrope's gain.

Antennas are wired into the prediction: `AntennaSet` holds what
`DECRED` holds after reading the gain files back — every table value
rounded through the file's `f7.3` and `f6.2` formats, because the
engine computes with the file's decimals, not the unrounded gain. The
mode loop asks it at every `GAIN` call site (`regmod`, `esmod`,
`settxr`, `genois`) and `PWRDB(freq)` picks the matching transmit
card's power. The fuzz corpus draws directional antennas from every
computable family at both ends with random beam headings; 300 cases
are identical to the reference — 1,011,360 printed cells — and the
isotrope sweep is unchanged.

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
30 MHz after each hour's output; `/MODES/` keeps one column per sample
area and `GHOP` as a single shared scalar; and `PWRDB` answers per
frequency from the transmit card whose band covers it.

## Systems methods other than 30 (`ITRUN = 7`)

Card methods 16 to 25 run the same systems chain as method 30 and
differ in which model runs and which lines print. `SETOUT` fills a
26-slot mask from the method number and `OUTBOD` walks lines 1 to 22 in
order, printing the ones the mask selects, so the port renders the same
rows from one `body_lines(method)` mask. Line 22, `DBM`, is the signal
power plus 30 and only method 25 prints it. Method 23 takes its lines
from `TOPLINES` and `BOTLINES` cards instead, and selects nothing
without them.

Which model runs: `DECRED` rewrites card method 30 to method 20 with
`MSPEC = 121`, and that is the only combination that runs both models
between 7000 and 10000 km and blends them. Method 21 forces the long
model at any distance, methods 22 and 25 force the short one, and every
other systems method takes the short model below `GCDLNG` (10000 km)
and the long one at or beyond it.

One computation reads the method directly: `MPATH` returns its floor
value past 7000 km only for `METHOD = 20` with `MSPEC = 121` — card
method 30 — because that is where its two models blend. Every other
systems method computes multipath at any distance. Method 30 hid this:
between 7000 and 10000 km its smoothing restores the multipath
probability from whichever pass it chose, and the long pass never
computes one, so the cell reads zero either way.

`fuzz --method M` runs the corpus with a different `METHOD` card.
Methods 16 to 22 are identical to the reference over 60 cases each.

## Ionospheric parameters (`run_par`)

Card method 1 (`ITRUN = 1`) prints `OUTPAR`'s table and computes
nothing else: one line per control point per hour carrying the E, F1
and F2 critical frequencies, semithicknesses and heights, half the
gyrofrequency, the three sporadic-E deciles, M(3000)F2, the virtual
height at 0.834 of the F2 critical frequency, the height-to-thickness
ratio, the sun zenith angle and the maximum zenith angle at which an
F1 layer exists, and the geomagnetic latitude. They are the layer
parameters as `TIMVAR`, `F2VAR` and `ESIND` leave them — `IONSET` does
not run on this path, so the profile is not yet reshaped.

`mufcheck --method 1` compares all 21 printed fields: 48 cases,
every cell identical.

## The MUF-only methods (`run_muf`)

Card methods 3 to 11 stop at the hour's MUFs and run no systems model.
Methods 7 to 11 (`ITRUN = 4`) take them from the complete
electron-density profile with `CURMUF`, which the systems methods
already use; methods 3 to 6 (`ITRUN = 3`) take them from the manual
nomogram method of NBS Report 7619 instead, which `NOMMUF` computes
from two distance-factor polynomials in great-circle miles, the lowest
E and F2 critical frequencies along the path and, for sporadic E, a
single hop at a 0.5 probability of reflection. There is no separate F1
MUF on that path.

`mufcheck` compares both. Method 7 prints `OUTLAY`'s table — the lower
decile, median and upper decile of the MUF, the takeoff angle, the
virtual and true heights and the equivalent vertical frequency, for
each of four layers — which is a wider view of `CURMUF` than any
systems method prints, since method 30's listing carries only the
circuit MUF, FOT and HPF. Method 3 prints `OUTMUF`'s four summary
columns. 48 cases each, every cell identical.

Both tables are read by column rather than by splitting on spaces: a
MUF of 1000.00, which is what the sporadic-E slot holds when no
control point has a sporadic-E layer, fills its `F7.2` field
completely and leaves no space before the next one. The comparisons
round half to even, which is what the Fortran runtime's formatted
output does: a value landing exactly on a printing boundary, such as
327.25 in an `F7.1` field, prints as 327.2, not 327.3.

## The LUF passes (`run_luf`)

Card methods 26 to 29 are `ITRUN = 8`: instead of the deck's
frequencies, the engine builds its own complement and searches it for
the lowest frequency that meets the required reliability. `FRQCOM`
lays out thirteen slots between 2 and 40 MHz — six cases depending on
where the lower of the E and F2 MUFs and the HPF fall — and puts the
circuit MUF in slot 12 without clamping it, so slot 12 can sit above
40 MHz and, in one case, above a slot the same pass already filled.
`LUFFY` runs the same short or long chain per slot as a systems pass,
stops at the first slot reaching the required reliability and
interpolates the LUF linearly between that slot and the one below it.
The pass is `IPFG` 300 below 10000 km and 400 at or beyond it.

Three things the pass does that the systems passes do not.

A short-path slot whose reflectrix has no reachable distance is
skipped outright, leaving its reliability at zero; `IPFG = 100` instead
forces the single over-the-MUF mode.

When no slot qualifies, the engine reports the negated most reliable
frequency — except that the scan is written

```
IG = 1
REL = RELIAB(1)
DO 160 IF = 2,12
IF(RELIAB(IF).GT.REL) IG = IF
```

with `REL` never reassigned, so it compares every slot against slot 1
and lands on the _last_ slot beating slot 1 rather than on the
maximum. The source carries a comment questioning the test. Kept as
written.

And the electron-density chain ends on the wrong area. It runs for
`K = JMODE`, then the test `IF((IPFG.EQ.100).OR.(K.GT.1))GO TO 87`
decides whether to run again for the long-path receiver area. Only
`IPFG` 100 is named, so the short LUF pass falls through and runs the
second area too, leaving `K = KFX` for the frequency loop. `FINDF` and
`FDIST` take `K` as an argument, but `INMUF`, `REGMOD`, `ESMOD`,
`ESREG` and `SIGDIS` all set `K = JMODE` internally. So when the
controlling area is area 1 and the path has more than one sample area,
the pass builds its reflectrix and raysets from the receiver-end area
and then reads its modes out of the `JMODE` column — which `FDIST`
never wrote. That column holds whatever the last write left there,
which is why the reference's reliabilities in this pass often sit at
the no-modes floor. The port models it: `/MODES/` is three persistent
columns in `ModeLoopState`, `fdist` writes the column `PassCtx::kctl`
names and `inmuf` reads the `jmode` one. A bug, kept as written.

`lufcheck` builds a method-26 deck per fuzz case, parses `OUTMUF`'s
table and compares GMT, LMT, FOT, HPF, the sporadic-E MUF, the circuit
MUF and the LUF at the two decimals the table prints. 96 cases over
all six distance bands — 2304 hours, 16,128 cells — are identical.
