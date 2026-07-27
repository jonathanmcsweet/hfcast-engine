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

## Host limits and toolchain

Nothing is on `PATH`. Cargo is `~/.cargo/bin/cargo`, run from
`propcore/`. `dprint` is `./node_modules/.bin/dprint` at the repository
root. There is no `node`, `npm` or `pnpm` on `PATH`: the only Node is the
editor server's, at `/home/dev/.vscodium-server/bin/<hash>/node`, and it
needs `unset ELECTRON_RUN_AS_NODE VSCODE_ESM_ENTRYPOINT` first or it
starts the extension host instead of running the script.

The host has **2 GB of RAM, no swap, and 16 CPUs**. Any tool that sizes a
pool from core count is killed by the kernel, reporting a bare `Killed`
or exit code 137 and nothing else — this has cost time in both the
Fortran builds and the Node bundler. Give every parallel step an explicit
count: `--jobs 3` to the harnesses, `JOBS=4` to the build scripts. Three
concurrent harness jobs is the measured comfortable figure; each one runs
a Fortran binary and copies a tree.

A foreground `sleep` is blocked. Wait on a condition instead.

## The harnesses

Two builds of the reference are needed once, into
`vendor/voacapl-variants/`:

```
propcore/tools/build-variants.sh   # O0 O1 O2 O3 fastmath — O2 is the reference
propcore/tools/build-trace.sh      # the instrumented build the stage traces read
```

Then, from `propcore/`, with `cargo` as above:

| harness     | what it proves                                        | flags                                                                                                                       |
| ----------- | ----------------------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `porttest`  | each stage's intermediates against the trace build    | `--cases N` `--only ID` `--seed N` `--fuzz N [--from N]`                                                                    |
| `portcheck` | whole listings over the 96 sweep cases                | `--cases N`                                                                                                                 |
| `fuzz`      | whole listings over generated decks                   | `--cases N` `--from N` `--jobs J` `--seed N` `--show N` `--method M` `--coeffs URSI88` `--fprob a,b,c,d` `--botlines a,b,c` |
| `antcheck`  | antenna gain tables against the reference's own files | `--only NAME` `--verbose`                                                                                                   |
| `lufcheck`  | `OUTMUF`'s table from a method-26 deck                | `--cases N` `--from N` `--jobs J`                                                                                           |
| `mufcheck`  | methods 1, 3 and 7 tables                             | `--method 1\|3\|7` `--cases N` `--from N` `--jobs J`                                                                        |
| `areacheck` | area coverage rows and antennas against the grid file | `--jobs J`                                                                                                                  |

`fuzz`'s `--method`, `--coeffs`, `--fprob` and `--botlines` are applied
after a case is generated, so the corpus is the same set of paths with
one card changed. That is deliberate: a difference is then attributable
to the card and not to a different path.

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
| HFMUFES      | 31-47          |    14 | yes    |
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
computable family at both ends with random beam headings; 600 cases
are identical to the reference — 2,031,840 printed cells — and the
isotrope sweep is unchanged.

### Several `ANTENNA` cards per end

A deck may carry up to twenty cards, and `/cantenna/` is one flat table
of twenty slots. The card's second field is the slot, so the slots are
numbered across both ends together, not per end: transmit cards first,
then receive. Reusing a slot overwrites it, which is why the deck
builder numbers from one and keeps counting through the receive cards.

`GAIN` then walks slots 1 to `numants` and takes the **first** whose end
matches and whose `[minfreq, maxfreq]` holds the frequency. Three
consequences, all of them exercised by the corpus:

- Bands that meet split the frequencies between cards.
- Bands that leave a gap leave some frequency with no antenna at all,
  and `GAIN` answers zero gain and zero efficiency. `PWRDB` does the
  same and returns its default 30 dBW, which is one kilowatt however
  much the cards asked for.
- Bands that overlap are resolved by position: the earlier card wins.

The frequency range comes from the card, not the gain file's own
computed extent — `ANTCALC` clears the whole 30-row table and fills only
`minfreq` to `maxfreq`, then writes all 30 rows and the range in the
header. Power is per card too, so a deck can transmit at a different
power on each band.

Card order, the frequency range, the gap rule, the per-frequency power
lookup and the slot numbering are each pinned by a deliberate-breakage
run (see "Traps"). One detail is **not** observable and is right by
transcription only: the zero rows outside a card's range. A matching
card has integer range bounds, so `I = FMC` and `I+1` can never index
past `maxfreq` with a non-zero weight — filling the whole table instead
changes nothing in 300 cases.

The deck writer and the engine's inputs both come from
`DeckCase::antenna_cards`, so the number written in a column and the
number the port is given cannot drift apart. There is no separate
transmit-power input any more: the reference has none either, since
`PWRDB` reads power out of the antenna table.

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

A `BOTLINES` card overrides that selection, for any method and not
only method 23: the jump that would skip `SETOUT`'s card block is
commented out, so the card applies to whatever the method chose. The
lines then print in the order the card lists them rather than in
numeric order, because `OUTBOD` walks the card for this path. A card
may also name a line past the 22 `OUTBOD2` knows: `SETOUT` lets values
up to 25 through, and the computed `GO TO` falls out of its label list
into the statement after it, which prints the MODE row. The port
matches on both counts.

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

## Traps: how a passing or failing verdict has been wrong

Every one of these produced a confident wrong answer at least once. Check
them before recording a verdict.

**A green verdict from a deck that cannot reach the branch proves
nothing.** `MPATH` returns its floor value past 7000 km only for card
method 30, and every other systems method computes multipath at any
distance. Method 30 could not expose the port applying that cutoff
everywhere, because between 7000 and 10000 km its smoothing takes the
multipath probability from whichever pass it chose and the long pass
never computes one — so the cell reads zero whether the port is right or
wrong. The port was wrong there through several passing sweeps. Before
trusting a result, ask what input would make the branch visible in a
printed cell, and whether the corpus contains it. The cheap way to answer
that is to break the new code on purpose and re-run: a case that stays
green under a deliberate error never reached the code. That is how the
area antenna cases were checked, and it is how the one detail no printed
cell can distinguish — the `.0174533` elevation constant — was found.

The several-cards-per-end stage was checked the same way, and the counts
are worth keeping because they say how much of the corpus reaches each
rule. Over 300 cases: reversing the card order within an end breaks 30,
taking the last matching card instead of the first breaks the same 30,
falling back to the end's first card when none matches breaks 32,
ignoring the frequency in `PWRDB` breaks 40, ignoring the receive card's
gain column breaks 5, giving every card the whole 2 to 30 MHz range
breaks 93, and renumbering each end's slots from one breaks 296 — that
last one collides even with a single card per end, since the receive card
would take slot 1 from the transmit card. Filling the whole gain table
regardless of the card's range breaks nothing, and that is recorded as
unobservable rather than verified.

**The listing does not print everything.** A difference in a value the
listing never shows is invisible to `portcheck` and `fuzz`. That is what
`porttest --seed N` and `porttest --fuzz N` are for, and how the
sporadic-E-off disagreement was found: the two engines printed the same
table while disagreeing about the Es layer's MUF hop count.

**Formatted output rounds half to even.** The Fortran runtime's `F`
editing rounds a value landing exactly on a printing boundary to the even
digit: 327.25 in an `F7.1` field prints as 327.2, not 327.3. A comparison
rounding half away from zero reports a difference that is not there.
Use `round_ties_even`. Note that `ANINT` inside the model does round half
away from zero, so the two rules coexist in one program.

**Split-on-whitespace parsing breaks on a full field.** A MUF of
1000.00, which is what the sporadic-E slot holds when no control point
has an Es layer, fills its `F7.2` field completely and leaves no space
before the next. `parse_outmuf` silently returned zero rows for method 3
this way. Read these tables by column. The formats are:

```
OUTMUF  (1H ,2X,2F6.1,   then F7.2 per column
OUTLAY  (' ',F4.1,F6.1,2(4F6.1,2F6.0,F6.1,2X))  continued (11X,2(...))
OUTPAR  2(1X,F5.1,A1),2F6.1,F6.2,2F6.1,F7.1,3F6.1,F7.1,3F5.1,F6.2,
        2(F7.1,F6.1),F6.1,A1
```

**Concurrent harnesses shared their scratch trees.** Each harness copies
the `itshfbc` tree per case so runs cannot see each other's files, but
the copy was named after the case alone, so two harnesses working the
same corpus picked the same directory — and `IsolatedRoot::create` starts
by deleting it. It did not fail: it truncated one run's reference listing
and the missing cells were reported as differences. A concurrent sweep of
eight methods showed false differences in three that vanished when each
ran alone. The tree name now carries the process id. Verdicts recorded
before that fix came from harnesses run one at a time, which was never
affected.

**Fortran binds an exponent before a multiplication.** `aa * rl**2` is
`aa` times the square; flattening it to `(aa * rl) * rl` rounds
differently and moved three printed digits in the CCIR antenna family.
Keep the source's association.

**Single-precision arguments are widened, not passed exact.** A routine
computing in double precision whose arguments live in a single-precision
COMMON block receives a value already rounded to `f32`. Passing the exact
double instead moved an azimuth by one unit in the last place and flipped
borderline digits. `DAZEL0` and `DAZEL1` both need the round-then-widen.

**Values that travel through a file carry the file's decimals.** The
engine computes with the gain it read back from `gainNN.dat`, not the
gain it computed, so `AntennaSet` rounds every table value through the
file's `f7.3` and `f6.2` formats.

**The reference suppresses the sign of a value that prints as zero.**
Every source file is compiled with `-fno-sign-zero`
(`src/*/Makefile.am`), so a negative value that rounds to zero in its
field prints without a minus: a latitude of -1.6e-10 in an `F10.4` field
is `0.0000`, not `-0.0000`. The listing comparisons could never catch
this, because they parse the numbers back and `-0.0` equals `0.0`; it
surfaced the first time an output was compared as text. `run::f_fixed`
applies it.

**State survives between hours and between calls.** `/SON/`, `/REFLX/`,
`/ZON/`, `/allMODE/` and `/MODES/` persist across hours and are read
stale. `FSECV` carries from each hour into the next, which is why an area
run's single hour is not the same computation as that hour inside a
24-hour run. Some antenna locals survive between calls on gfortran's
stack without being in a `SAVE` statement.

## The COEFFS and FPROB cards

`COEFFS` chooses the foF2 map set: `URSI88` instead of the default
CCIR, which `REDMAP` reads as a different `.daw` file. The port's
coefficient reader already had both; the card now selects between
them, and `fuzz --coeffs URSI88` runs the corpus through the URSI maps
— 60 cases, identical.

`FPROB` multiplies each layer's critical frequency: E, F1, F2 and
sporadic E. The deck builder wrote only the sporadic-E switch, all
ones with the fourth at one or zero. A case can now carry the whole
card, and `fuzz --fprob a,b,c,d` runs the corpus with arbitrary
multipliers — 60 cases at 0.90, 1.10, 1.05, 0.70, identical. Note that
the engine's own default, when a deck has no `FPROB` card at all, is
1, 1, 1, 0.7 rather than all ones.

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

## Area coverage: how to drive the reference

An area run is the program's **second invocation mode** and does not read
a card deck at all, which is why it looked untestable at first. It is:

```
voacapl <itshfbc-root> area calc default/<name>.voa
```

run with the working directory set to the root. The input is a keyed text
file — one `Keyword :values` line per setting, not fixed-width columns —
placed at `<root>/areadata/default/<name>.voa`. The reference writes its
results beside it as `<name>.vg1`. `runner::run_area` does all of this and
returns the grid file's text; `src/bin/areacheck.rs::area_file` shows a
complete working input file. A 9 by 9 grid runs in 0.03 seconds, so the
comparison loop is fast and a grid can be large.

The grid file prints each point's own latitude and longitude before the
predicted columns, which is what let the geometry be verified ahead of
everything else: two `I3` indices, then latitude in columns 6 to 16 and
longitude in 16 to 26. Its header carries the grid dimensions in the same
columns the rows use for their indices, so a data row is recognised by
having coordinates that parse.

`engine::area` is `GRIDXY` and `DAZEL1` plus the driver's two corrections
to each receiver point.

The input file's two antenna lines are keyed but their values are still
read from fixed columns, so a misplaced space silently changes an antenna.
`Tx Ants  :` is a bracketed 21-character name, then the design frequency
in columns 34 to 40, the main beam bearing in 41 to 46, and the power in
kilowatts in 48 to 57. `Rec Ants :` is the name, seven ignored characters,
then the gain in 41 to 46 and the bearing in 47 to 52. `AREAMAP` turns
these into two `ANTENNA` cards, and writes the transmit card's design
frequency from the file while the receive card's is always zero — which is
why the receive line's gain field is what reaches a receive isotrope.

## The area driver and its output columns (`run_area`)

`HFAREA` runs the same one-hour prediction at every grid point. The port
shares the hour body with the point-to-point driver: `hour_setup` holds
what one hour reads and does not change, and `hour_body` is the hour
itself, so `run`, `run_hour` and `run_area` cannot drift apart.

Two things the driver does that a reading of the roadmap's note would
miss. The mode-loop state and `FSECV` carry from one grid point to the
next, exactly as they carry from hour to hour: `HFAREA` does not reset
them, so only the first point starts from the program-start zero. And it
compares the path length against `GCDLNG` with `.GT.` where `HFMUFS` uses
`.GE.`, which changes the model at exactly 10000 km.

`OUTAREA` prints **24** value columns, not 27, and only when the run has
one frequency; with more it prints 7 — the MUF and the six values that
are maxima over the frequencies. Those six are the largest value over the
frequencies rather than the first frequency's in both forms, because the
reference walks them overwriting slot 1 — and the power cut, which reads
the same slot, therefore sees a maximised median against unmaximised
decile deviations.

Asking for several frequencies is not obvious: the area file's `Freqs`
line holds one frequency **per plot**, not a list for one run. A value at
or below 0.5 makes `GETFREQS` read the list from `run/areafreq.dat`
instead, up to eleven frequencies, which is the only way to reach the
seven-column form. The distributed tree ships no such file, so
`areacheck` writes one. The reference prints a warning on that path that
the transmit antenna must be non-directional, which fits: with more than
one frequency `ANTCALC` builds an ordinary point-to-point table instead
of the 360-azimuth one. `PWRCUT` is George Lane's algorithm: an eleven-point
normal distribution of signal-to-noise ratios built from the median and
the two deciles, interpolated at the half-power and quarter-power limits.

Three things about this mode are recorded at the code and matter to the
rest of the stage. The `AREA` card's last field picks the projection:
zero gives the great-circle mesh (`IPROJ = 7`) and anything else the
latitude and longitude mesh (`IPROJ = 8`), which `GRIDXY` does not test
for and so takes its plain branch. The azimuth is scaled by the literal
`.0174533` the source writes rather than by `/CON/`'s degree conversion,
which differs in its last digits. And this driver compares the path
length against `GCDLNG` with `.GT.` where the point-to-point driver uses
`.GE.`.

The printed longitude is not always the one the prediction used. Under the
latitude and longitude projection (`IPROJ = 8`), a grid whose western edge
is negative reads better unfolded, so `OUTAREA` subtracts 360 again from
the first column and from any value past 180 — `GRIDXY` having folded
every longitude into 0 to 360 on the way in. It is a rendering
adjustment, not a different mesh. The pole needs the rule after it: the
driver forces the longitude to zero within a tenth of a degree of either
pole, so the first column there would print -360, and the source answers
zero instead.

`areacheck` compares the reference's rows as text, field by field, which
is stricter than parsing them back: 17,791 printed cells over 21 grids are
identical.

## Inverse area coverage

The program's third invocation word. `voacapl <root> inv calc
default/<name>.voa` reads its input from `area_inv/default` rather than
`areadata/default` and writes its `.vg1` beside it. The input file has the
same keys.

The grid supplies the **transmitter** and the file's `Transmit` line
becomes the fixed receiver — `HFAREA` swaps the roles rather than the
file, so that line keeps its name and the port's field names follow it.
Three consequences:

- The output row still names the grid point, which is now the transmitter.
- The transmit antenna's beam is re-aimed at the fixed station from every
  grid point, replacing whatever the card asked for. This is a lookup-time
  change only, so a multi-frequency inverse run is unaffected: its table
  was already cut along one bearing and no longer consults the beam.
- The nudge that separates a grid point from the station compares against
  the station's coordinates after a round trip through the degree
  conversion, because the driver holds that end in radians by then. The
  round trip is not exact in single precision. Reaching a printed
  difference would need a grid point within about 20 cm of the nudge's
  0.05-degree boundary, so this is right by transcription and not by
  verification — the same standing as the `.0174533` constant above.

`areacheck` covers this with four cases: an isotrope, where only the
geometry reverses; a directional transmitter, where the re-aiming shows; a
multi-frequency run, where it does not; and a grid straddling its own
station at the nudge's boundary. Removing the swap breaks all four,
removing the re-aiming breaks only the directional one, and removing the
nudge breaks all four again.

## The area antenna table (`area_table`, `area_gain_lookup`)

`ANTCALC` has a second branch for area coverage: one frequency, 360
azimuths by 91 elevation angles. It takes that branch only when the run
asks for a **single** frequency (`freqarea(2)` is zero) and the input file
asks for area coverage; with several frequencies an area run uses the
ordinary point-to-point table, cut along the transmitter-to-plot-centre
bearing and fixed for the whole grid, because the deck `AREAMAP` writes
names the plot centre as the receiver.

The table never travels through `gainNN.dat`. The area branch writes only
the two header lines and stores the numbers straight into a COMMON, and
`DECRED`'s read-back is commented out — so the values carry
`NINT(gain*100)` in an `INTEGER*2` and none of the file's `f7.3`
rounding. The `-999` the header's off-azimuth field holds is the whole
flag: it is what makes `GAIN` interpolate in bearing instead of frequency.

Four details of the branch are easy to miss:

- The pattern models are initialised **once**, at the one frequency,
  rather than per frequency as point-to-point does.
- The elevation angles are converted with `.0174533` where the
  point-to-point branch writes `.01745329`. Both are in the source. The
  difference is far below the table's hundredth of a decibel, so no
  printed cell can distinguish them — the constant is right by
  transcription, not by verification.
- A non-terminated rhombic (type 7 with a negative beam) is symmetric
  about the broadside line, so the table is built at `180 - a` for
  azimuths 91 to 180 and at `540 - a` for 181 to 269.
- The transmit lookup takes the **magnitude** of the beam bearing, for
  that same rhombic; the receive lookup does not.

`areacheck` covers this with one case per family, a different family at
each end, over a grid whose 25 points reach every quadrant — so the table
is read at 25 different bearings. Six of those cases go dark the moment
the two bearings are swapped, and the isotrope, inverted-cone and
multi-frequency cases do not, which is what says each case reaches the
branch it is meant to.
