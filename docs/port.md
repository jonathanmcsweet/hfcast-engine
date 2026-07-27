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
   requires the two engines to write the same listing file, byte for
   byte — banner, echoed deck, header blocks, page breaks and body rows. The sweep only
   holds combinations somebody chose; this covers the rest, cycles
   through six distance bands so short and near-antipodal paths are
   always represented, and reports a case index that reproduces any
   failure exactly (`--seed N`). Refusing the same case counts as
   agreement: the reference stops on some inputs and the port stops on
   the same ones. **Result (2026-07-27): 600 cases identical as text —
   434,116 printed lines, holding 2,031,840 cells and 100,872 mode
   labels.** `porttest --seed N` runs one
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

The other limit is file descriptors: 4096 per process, soft and hard,
so it cannot be raised. A harness run holds them while it builds
private trees. `fuzz --cases 200 --jobs 4` exhausted them, and the
failure does not look like a resource problem — the report says the
harness could not run 145 of the cases and ends with "the port and the
reference disagree", while the shell itself starts failing to load
shared libraries. Keep to `--jobs 2` above about a hundred cases, and
read a mass failure with no printed difference as this rather than as
a regression.

The same limit stops `cargo test` from linking at all, with pages of
`rust-lld: error: cannot open ...rcgu.o: Too many open files`. The dev
profile splits the crate into 256 codegen units and the linker opens
every one at once. Build with one unit instead:

```
CARGO_PROFILE_DEV_CODEGEN_UNITS=1 CARGO_PROFILE_TEST_CODEGEN_UNITS=1 \
  ~/.cargo/bin/cargo test --jobs 2
```

Compilation is slower and linking succeeds. This is worth reaching for
straight away: the failure is intermittent, so retrying looks like it
is working and then fails again minutes later.

A foreground `sleep` is blocked. Wait on a condition instead.

## The harnesses

Two builds of the reference are needed once, into
`vendor/voacapl-variants/`:

```
propcore/tools/build-variants.sh   # O0 O1 O2 O3 fastmath — O2 is the reference
propcore/tools/build-trace.sh      # the instrumented build the stage traces read
```

Then, from `propcore/`, with `cargo` as above:

| harness       | what it proves                                        | flags                                                                                                                                          |
| ------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `porttest`    | each stage's intermediates against the trace build    | `--cases N` `--only ID` `--seed N` `--fuzz N [--from N]`                                                                                       |
| `portcheck`   | whole listings over the 96 sweep cases                | `--cases N`                                                                                                                                    |
| `fuzz`        | whole listing files over generated decks              | `--cases N` `--from N` `--jobs J` `--seed N` `--show N` `--method M` `--coeffs URSI88` `--fprob a,b,c,d` `--botlines a,b,c` `--toplines a,b,c` |
| `antcheck`    | antenna gain tables against the reference's own files | `--only NAME` `--verbose`                                                                                                                      |
| `lufcheck`    | `OUTMUF`'s table from a method-26 deck                | `--cases N` `--from N` `--jobs J`                                                                                                              |
| `mufcheck`    | methods 1, 3 and 7 tables                             | `--method 1\|3\|7` `--cases N` `--from N` `--jobs J`                                                                                           |
| `areacheck`   | area coverage rows and antennas against the grid file | `--jobs J`                                                                                                                                     |
| `paritycheck` | the fields the server reads, both production paths    | `--jobs J`                                                                                                                                     |

`porttest --fuzz` is not currently usable: it reports stage mismatches
on generated decks where `fuzz` finds the finished listings identical,
so the fault is in how the harness pairs its dumps, not in the engine.
`porttest` over the 96 sweep cases is clean and is the mode to trust.

`fuzz`'s `--method`, `--coeffs`, `--fprob`, `--botlines` and
`--toplines` are applied
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

| stage                                 | Fortran                                                                                                   | Rust                               | verified against trace                            |
| ------------------------------------- | --------------------------------------------------------------------------------------------------------- | ---------------------------------- | ------------------------------------------------- |
| constants, magnetic pole              | `blkdat`, `set_magnetic_pole`                                                                             | `engine::con`                      | via geometry                                      |
| path geometry, control points         | `geom.for`                                                                                                | `engine::geometry`                 | worst 3e-4 km / 1.3e-5 deg over 96 cases          |
| magnetic field at control points      | `magvar.for`, `magfin.for`                                                                                | `engine::magnetic`                 | worst 5e-8 over 408 control points                |
| coefficient loading                   | `redmap.for`                                                                                              | `engine::coefficients`             | 819k elements, worst at print precision           |
| map evaluation, layer parameters      | `geotim`, `virtim`, `versy`, `noisy`, `ef1var`, `timvar`, `f2var`                                         | `engine::ionosphere`               | 733k AB values, 9.8k point-hours                  |
| sporadic E parameters                 | `esind`                                                                                                   | `engine::ionosphere`               | 9.8k point-hours                                  |
| sporadic E losses                     | `esreg`, `esmod`                                                                                          | `engine::modes`                    | with the mode loop below                          |
| MUF                                   | `ionset`, `lecden`, `gethp`, `f2dis`, `curmuf`                                                            | `engine::muf`                      | 2.3k hours, 20 fields + profiles                  |
| ionogram, reflectrix, deviative loss  | `sang`, `selmod`, `genion`, `fobby`, `alosfv`                                                             | `engine::ionogram`                 | 4.6k area calls incl. exact reflectrix            |
| signal distribution, absorption       | `syssy`, `xlin`, `prbmuf`, `sigdis`                                                                       | `engine::sigdis`                   | 3.2k calls, 20 fields                             |
| noise                                 | `anois1`, `genfam`, `genois`                                                                              | `engine::noise`                    | 70k calls, 13 fields                              |
| ground constants, path latitude       | `geom.for` land-mass lookup                                                                               | `engine::ionosphere`               | identical sea/land at every point                 |
| mode loop (raysets, losses, Es modes) | `penang`, `findf`, `fdist`, `inmuf`, `regmod`, `esmod`, `esreg`                                           | `engine::modes`                    | 46k reflectrix, 49k hop, 32k mode dumps           |
| long-path model                       | `gmloss`, `settxr`, `seltxr`, `lngpat` and helpers                                                        | `engine::modes`                    | 14.4k two-end loss tables, exact rows             |
| reliability, per-frequency outputs    | `relbil`, `serprb`, `mpath`, `setlng`, the smoothing blend                                                | `engine::modes`                    | 31.7k slots + 8.6k smoothed, 24 fields            |
| output fields, whole engine           | `setluf`, `outbod` listing body, `hfmufs` hour loop                                                       | `engine::run`                      | listing bit-identical over 96 cases               |
| listing text: banner, header, paging  | `listin`, `outtop`, `setout`, `outlin` page breaks                                                        | `engine::output`                   | whole file identical over 600 cases               |
| every card method's output routine    | `outpar`, `oution`/`ionplt`, `outmuf`, `outlay`, `outgph`/`gphbod`, `outant`, `outtab`/`tabbod`, `outall` | `engine::tables`, `engine::graphs` | whole file identical, methods 1-30, 40 cases each |

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

## The listing text (`engine::output`)

The comparison used to be the printed cells a parser could recover.
Now it is the whole file, byte for byte: the banner, the echoed input
deck, every header block and every body row. That closes three gaps at
once — the header was never checked, the page breaks were never checked,
and the long model's `RANGLE` row was rendered nowhere.

Three routines write it.

- `LISTIN` writes the banner, the version line and a column ruler, then
  echoes each input card with one leading space and its trailing blanks
  dropped. The first character of the banner is a form-feed flag, blank
  for a point-to-point run.
- `OUTTOP` writes a header block: a page banner carrying the coefficient
  set, the method, the model name, the version and the page number, then
  up to seven lines describing the deck. Two of those seven are the
  antenna lines, one per card, so a deck with several cards per end
  prints a longer block.
- `HFMUFS` writes one end-of-run line.

The version is not a constant. The reference reads
`database/version.w32` and takes the eight characters after `Version`,
so the port reads the same file from the same tree and a tree with a
different version file changes both engines together.

### Which lines print

`SETOUT` sets `NTOP`, turns on header lines 1 to `NTOP`, and stores the
count in `LINTOP(15)`. A `TOPLINES` card replaces the selection with an
arbitrary set, and — like `BOTLINES` — it does so for **any** method, not
only method 23, because the jump that was meant to confine it to method
23 is commented out. Its count is the number of accepted fields, so a
card naming the same line twice counts two. Lines 8 to 14 may be named
and counted although `OUTTOP` prints nothing for them.

Method 23 without cards is the interesting case. `SETOUT` clears
`LINTOP` and `LINBOT` to -1 and then jumps past the statements that would
set both counts, so the page arithmetic runs on -1: the run prints no
header at its first hour, one header at its second, and then none for the
rest of the day, because each hour charges one line where it prints two.

### Where the page breaks

`OUTLIN` compares the row count of one hour against the lines left on
the page and calls `OUTTOP` when the next hour would not fit. Three
details decide the answer and all three are pinned by probes:

- `SETOUT` leaves the counter at the page limit, so the first hour always
  breaks a page.
- After a header the counter is set to `LINTOP(15)` plus the antenna
  lines printed — not to the number of lines the header actually wrote.
  The two happen to agree for every method's own selection, and diverge
  only under a `TOPLINES` card.
- An hour with no mode in any slot prints its frequency line alone and
  charges three lines.

A `BOTLINES` card makes the counts disagree with each other. `SETOUT`
accepts fields up to 25 and counts those; `OUTBOD` recounts as it prints
and has no upper bound. So the first hour is charged `SETOUT`'s count and
every hour after it `OUTBOD`'s, and a card naming line 26 raises the
second without the first.

Two defects fall out of the reading and are documented where they live.
The long model prints `RANGLE` after `TANGLE`, and nothing counts it, so
a long-path page runs one line over the limit for every hour on it. And
the counter's value after a header is smaller than the block it printed
whenever a `TOPLINES` card turns lines off.

### What the header prints about an antenna

`ANTMODEL` builds a ten-character model label from the antenna file's
type number — `+  0.0 dBi` for an isotrope, carrying its gain, and
`IONCAP #21`, `HFMUFES#37`, `NOSC-95#48` and the rest for the families.
`ANTCALC` writes it as the first field of `gainNN.dat` and `DECRED` reads
it back, so it reaches the header the same way the gain table does.

The main beam bearing and the off-azimuth travel through that file's
`f7.2`, and the header then prints one decimal of them. The off-azimuth
is a computed bearing, so the rounding is visible and the port applies
it. The main beam bearing comes from a card field five columns wide with
one decimal, so `f7.2` can never change it: that rounding is
unobservable by construction rather than untested.

The transmit line ends with the card's power in kilowatts, after
`DECRED` turns a non-positive power into one kilowatt. The receive line
stops at the off-azimuth: `OUTTOP` passes one value fewer to the same
format, and the record ends where the values run out.

### Output formats still unported

Methods 1 to 15 and 24 to 29 print through routines this stage does not
cover — `OUTPAR`, `OUTION`/`IONPLT`, `OUTMUF`, `OUTLAY`, `OUTTAB` and
`OUTGPH`, and method 25's `OUTALL`. `fuzz --method M` on any of them now
reports differing lines, which is honest: the values behind those tables
are checked through `mufcheck` and `lufcheck`, but the text is not
written yet. It used to report zero cells compared, which read like a
pass.

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

The listing text was checked the same way. Over 40 cases: printing the
first page's tilde on every page breaks 40, numbering every page 1 breaks
40, giving the receive antenna line a power column breaks 40, omitting
the long model's `RANGLE` row breaks 19, mislabelling every antenna as an
isotrope breaks 29, numbering the `IONCAP` and `HFMUFES` labels from one
breaks 19, keeping the echoed deck's trailing blanks breaks 40, and not
defaulting a non-positive transmit power to one kilowatt breaks 1 — the
corpus draws a power that rounds to zero about that often. Over 24 method
23 cases, charging the page zero rows instead of -1 breaks all 24.

Two of these needed a card to become visible at all. Charging the page
the lines the header printed rather than `LINTOP(15)` plus the antenna
lines breaks nothing on any method's own selection, because the two are
equal there; with `--toplines 1,1,1,1,1,1,1,1,1,1,1,1,1,1` it breaks 40
of 40. Counting a repeated `TOPLINES` field once instead of twice needs
the same card, and breaks 24 of 24 with it.

Two details stay unobservable. Rounding the main beam bearing through the
gain file's `f7.2` cannot change it, because the card field it comes from
has one decimal. And for method 23 without cards, taking the header's own
count as zero rather than -1 changes nothing, because either value leaves
the counter far below the page limit for all 24 hours.

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
applies it, and so does `output::f` — the listing needed it too, but
only card methods 13 and 15 exposed it, because `OUTANT` is the one
routine that prints a whole antenna pattern and so prints thousands of
gains, some of which round to zero from below.

**`to_degrees()` is not `R2D`.** Rust's conversion factor is
`180.0f32 / PI`, which is one `f32` step from the `57.295779513` that
`blkdat.for` puts in `/CON/`. One step is enough to move an
interpolated antenna gain across a rounding boundary, so every
conversion in the engine goes through `con::R2D` and `con::D2R`.

**Card method 25 prints two decimals, and that is enough to expose the
last bit.** Every other method prints a gain or a loss to one decimal,
where a difference in the last `f32` bit is invisible. `OUTALL` prints
`F9.2`, so a value sitting within about 5e-6 of a half-hundredth
rounds the other way. Two have been seen, both inside the tolerance
envelope. Only the second is in the current corpus — adding a fuzzer
draw shifts every case after it — but the first is worth keeping
because it was traced all the way down:

- 27.93 MHz at 15 UT, `R. GAIN` -12.32 against -12.33. Traced
  to `samples/sample.43`'s 28 MHz / 9 degree pattern value, which the
  reference writes to `run/gainNN.dat` with `F7.3` and reads back: the
  port's -11.8015003 and the reference's differ in the last bit, and
  the file's three decimals turn that into a whole 0.001.
- Case 2, 28.53 MHz at 2 UT, `TRAN.LOSS` 227.69 against 227.70, with
  `SIG. POW.` following it. The printed gains agree, so the difference
  is below print precision in one of the loss terms.

Every IEEE-conformant build of the reference agrees with itself on
both, so this is the port's own last bit rather than build noise.

**State survives between hours and between calls.** `/SON/`, `/REFLX/`,
`/ZON/`, `/allMODE/` and `/MODES/` persist across hours and are read
stale. `FSECV` carries from each hour into the next, which is why an area
run's single hour is not the same computation as that hour inside a
24-hour run. Some antenna locals survive between calls on gfortran's
stack without being in a `SAVE` statement.

## Every card method's own output routine

`HFMUFS` dispatches on `JTOUT(METHOD)`, and `engine::output::render`
mirrors that table. What each routine writes, and where it lives:

| `ITOUT` | card methods     | routine           | module           |
| ------: | ---------------- | ----------------- | ---------------- |
|       1 | 1                | `OUTPAR`          | `engine::tables` |
|       2 | 2                | `OUTION`/`IONPLT` | `engine::graphs` |
|       3 | 3, 26            | `OUTMUF`          | `engine::tables` |
|       4 | 4-6, 8-11, 27-29 | `OUTGPH`/`GPHBOD` | `engine::graphs` |
|       5 | 12               | nothing           | —                |
|       6 | 13-15            | `OUTANT`          | `engine::graphs` |
|       7 | 16-23, 30        | `OUTLIN`/`OUTBOD` | `engine::output` |
|       8 | 24               | `OUTTAB`/`TABBOD` | `engine::tables` |
|       9 | 25               | `OUTALL`          | `engine::tables` |
|      10 | 7                | `OUTLAY`          | `engine::tables` |

Slot 11 of `JTOUT` (card method 30) is dead: `DECRED` rewrites method 30
to `METHOD = 20, MSPEC = 121` before the table is read.

Three of these do not fit the pattern the others follow.

- Card method 12 computes a MUF and then leaves the hour loop with no
  output option matching, so the run prints its preamble, the echoed
  deck and the end-of-run line and nothing else.
- Card methods 13 to 15 print through `OUTANT` _before_ `SETOUT` runs,
  so they get no header block and no page arithmetic — each antenna
  card's pattern starts its own page with a form feed and a banner of
  its own.
- Card method 25 prints from inside `LUFFY`'s frequency loop, which is
  why its header carries the tilde on every hour rather than only the
  first, and why a deck whose first frequency slot is empty never gets
  the ionospheric parameter table: `OUTALL` calls `OUTPAR` when its
  argument is 1, and the loop skips an empty slot before reaching the
  call.

`OUTALL` has one input it cannot print. Its formats are built at run
time with the mode count as a repeat count, and a repeat count of zero
is not a legal Fortran format, so a frequency with no modes at all stops
the reference with a runtime error part way through the file. The port
refuses the same run rather than invent an output for it.

## The OUTGRAPH card

`OUTGRAPH` names up to twelve further card methods whose MUF table or
diurnal graph is printed after the run's own output, from the arrays the
run already filled. `HFMUFS` ignores the card unless the run computed
MUFs or LUFs (`ITRUN` 3, 4, 7 or 8), ignores a request whose own output
is not `ITOUT` 3 or 4, and ignores a LUF table or LUF graph unless the
run itself computed a LUF.

`SETOUT` does not run again, so a requested method prints the original
method's header lines under its own method number — except on a card
method 30 deck, where `OUTTOP` prints a literal 30 because `MSPEC` is
121. The values are whatever the original method left in the arrays: a
nomogram run asked for method 10's graph plots the takeoff angle
`NOMMUF` never wrote, which is the -1 `SETOUT` cleared the array to.

A negative request writes to a second output unit the driver never
opens, so those pages land in a stray `fort.16`. They still advance the
page number, which is why the port counts them without printing them.

## The INTEGRATE card

`IEDP` decides how layer heights are obtained. Its program-start value
is -1 and only an `INTEGRATE` card raises it, including in the card's
own `OFF` form: `DECRED` assigns 1 before it tests for `OFF` and never
restores -1, so `INTEGRATE OFF` selects the fast path rather than
turning it off.

At zero or above, three places change.

- `CURMUF` gives the E layer the fixed pair a 110 km, 20 km parabolic
  layer would produce (`HTE = 104.25`, `HPE = 125.30`) instead of
  reading the profile.
- `CURMUF` gives the F2 layer a parabolic true height and a virtual
  height from `BENDY` plus `PEN`, but only when there is no F1 layer;
  with one it falls back to `GETHP` for both. The parabolic true height
  reads `XT2` after the short-distance `BETA` scaling, so that store,
  which is dead on the default path, is live here.
- `GENION` takes the E layer's ten points from a table, the F layer's
  true heights from a profile lookup, and its virtual heights from the
  same `BENDY` and `PEN` — again falling back to `GETHP` where an F1
  layer exists.

`IONPLT` also prints `MODEL SEG` instead of `GAUSSIAN` in its heading
when the point has no F1 layer and `IEDP` is not negative.

## KRUN and the EFVAR, ESVAR and EDP cards

The `EXECUTE` card's fifth-to-tenth column carries `KRUN`, which says
how much of the ionosphere each hour recomputes:

| `KRUN` | `VIRTIM` | `TIMVAR`, `F2VAR` | `ESIND` |
| -----: | -------- | ----------------- | ------- |
|      0 | yes      | yes               | yes     |
|      1 | yes      | yes               | no      |
|      2 | yes      | no                | yes     |
|     3+ | no       | no                | no      |

`GEOTIM` runs every hour whatever the field says. It writes each
control point's local mean time, and `TIMVAR` writes it again from a
different expression, so a run that skips `TIMVAR` prints `GEOTIM`'s
value rather than `TIMVAR`'s.

`EFVAR` and `ESVAR` put the layer and sporadic-E parameters straight
into the arrays, which is only useful with a `KRUN` above zero, because
otherwise the first hour overwrites them. What they do not replace
starts at `blkdat`'s presets, and those are written as though `FI`,
`YI` and `HI` were `(5,3)` rather than `(3,5)` — 110 km for every
point's E layer, nothing for F1, 300 km for F2 is the intent, and the
values land in the wrong slots. The comment above them calls it an
"effective elimination of layers".

The port has to model the arrays as state that survives the hour,
because the routines that read them also write them: `IONSET` reorders
`FI`, `YI` and `HI` in place, `SETLNG` replicates them into the slots
above the control point count, and `CURMUF` rewrites one sporadic-E
lower decile. With `TIMVAR` and `F2VAR` running, each hour overwrites
all of that before it is read again and none of it shows. With them
skipped, nothing puts the arrays back, so `IONSET` reorders values it
has already reordered and the ionosphere drifts from hour to hour with
no map behind it. The port reproduces the drift; `IonoCarry` is where
the arrays live.

`EDP` supplies the electron density profile directly. `LECDEN` then
returns without building one — and it tests all three slots of the
`IELECT` flag rather than the one for the area it was asked about, so a
single card suppresses the profile for every area.

One `IONPLT` detail belongs here: a sporadic-E lower decile below about
-0.15 MHz puts the first column of its segment at or below zero, and
the source writes there, outside its own `IX` array. Nothing of that
reaches the plot, so the port drops those columns rather than write out
of bounds.

## Cards that reach no computation

Four of the input cards set a variable nothing reads, or write values
something else overwrites before they are used. The deck builder can
write all of them, and the fuzzer draws them, so "no effect" is a
measured result rather than a reading of the source.

- `FREEFORM` sets `ITYPE`. Nothing reads it; the source's own comment
  says the free-form input analyser was never developed.
- `ANTOUT` sets `IANTOU`. Nothing reads it, although `OUTANT`'s header
  comment claims the card makes it write an antenna file.
- `SAMPLE` writes the control point coordinates, gyrofrequency,
  magnetic dip, local time, excess loss and ground constants. `GEOM`
  and the `MAGVAR` call inside it overwrite the first six at every
  `EXECUTE`, `GEOTIM` overwrites the local time every hour, and
  `SIGDIS` overwrites the excess loss every hour.
- `COMMENT` reaches nothing either, but it is not invisible: `LISTIN`
  drops a card whose first fifteen characters are `COMMENT   GROUP`
  from the echoed deck, and `DECRED` writes it back when it reads it,
  which is after the whole deck has been echoed. So that one spelling
  moves to the end of the deck listing and is padded to the 75
  characters its `CHARACTER*85` buffer holds.

`MONTHLOOP` and `NEXT` are not usable at all: the reference reaches a
`pause` and a `stop` on both.

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

## The public API (`propcore::api`)

The port's own interface, for callers who are not writing card decks.
`predict` returns data, `listing` returns the reference's text, `deck`
returns the cards a request resolves to. Both entry points go through
one `Request` to `DeckCase` conversion, so the data and the text
cannot describe two different runs.

`Task` separates what a run computes from what it prints. The card
number conflates the two — `JTRUN` and `JTOUT` are two tables indexed
by the same number — so a caller who wants MUFs has to know that 7 is
the method that computes them from the profile and 3 the one that uses
the nomogram. Seven tasks name the distinct products. The other card
methods are reachable as before, through `deck::DeckCase` and
`engine::output::render`; the API is a face on the engine, not a fence
around it.

### Inputs are put on the card grid first

A card column carries a fixed number of decimals, and the listing
prints its header from those columns, so no listing can show a finer
value than a card can carry. Running the engine on a caller's
unrounded value while echoing a rounded deck prints a listing that
contradicts itself — with a transmitter at 35.8765 N the header says
`35.88 N` and the bearing beside it, 254.90, is the bearing from
35.8765; the reference given that same deck prints 254.91. Twenty-five
lines differed on the first request tried.

So `DeckCase::as_written` puts every field on the grid its own column
holds, and the API applies it before anything runs. That is what makes
"byte-identical to the reference" true of every request instead of
only of card-expressible ones. The grid is 0.01 degree of latitude and
longitude, 0.01 MHz, 0.1 W, 0.1 dB of required SNR, 1 dB of noise, 1
sunspot, and 0.01 MHz / 0.1 km on the layer override cards — every
step far below what the model resolves.

`as_written` mirrors `build_deck`'s format for each field and has to
be edited with it. What proves the two agree is
`tests/api_reference.rs`: a request finer than every column, run
through the reference and compared as text. Removing the rounding from
any one field makes it fail, which was checked field by field rather
than assumed.

One card field takes no fraction at all: man-made noise is written as
the value followed by a point, so 145.42 would produce `145.42.` and
overflow the five-column field rather than round. `as_written` rounds
it to a whole number, which is the only thing the card can say.

## Serving predictions: the `predict` binary

`predict` is the interface between the TypeScript server and the
engine. It reads one request object as JSON on stdin and writes the
prediction as JSON on stdout — a process boundary rather than a
binding, which is the least machinery that takes the Fortran
toolchain out of the deployment.

What it removes from the server: writing a fixed-width card deck,
running `voacapl`, parsing a printed listing, and giving every
concurrent run a private copy of the `itshfbc` tree. That last one
was needed because the Fortran names its antenna scratch files from a
global counter, so two runs sharing a tree overwrite each other. The
port holds no such state, so runs are independent and the tree is
read only.

### Why it renders a listing and reads it back

The server has always consumed _printed_ values — reliability to two
decimals, SNR to the nearest dB, the deciles to one — and its
correction factors were fitted against exactly those numbers. So
`predict` renders the listing with the verified formatter and parses
it with `listing::parse_listing`. That makes the values identical to
the reference's by construction, rather than by a second
implementation of `OUTBOD`'s rounding, its at-the-MUF column, and its
rule for how many frequency slots print at all.

The raw `f32` values are richer and are what a later tier should use.
Reaching for them changes the numbers the fitted corrections were
built on, so it is a deliberate change to measure rather than a side
effect of moving off Fortran.

### `paritycheck`

Narrower than `portcheck` on purpose: not every printed cell, but
exactly the four fields `server/src/voacap/parse.ts` reads —
reliability, SNR and the two SNR deciles — plus the MUF, over eight
request shapes the server actually sends. Both sides run the whole
production chain: the Fortran through `voacapl` and the printed
listing, the Rust through the `predict` binary as a subprocess so the
JSON boundary is under test too.

**Result (2026-07-27): 7104 fields over 8 shapes, 0 differing.**

The harness was checked by breaking it: adding 1 dB to the Rust
side's SNR makes all 1728 SNR fields differ and the verdict flip. A
green run with no such check would say nothing, because both sides
end in the same parser.

### A stale fixture, worth knowing about

`server/test/fixtures/seattle-tokyo-jul2026-ssn68.out` was generated
with `FPROB 1.00 1.00 1.00 0.00` — sporadic E off. The server turned
it on in 0.2.0 on the evidence in `accuracy.md`, so that fixture is a
listing for a deck the server no longer sends. It stays because the
parser tests use it as known text. Comparing engine output against it
compares two different questions, which cost a confusing test failure
once; the `-es` fixture beside it is the current deck's listing.
