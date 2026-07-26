# propcore

Characterisation harness for porting the HF propagation engine off Fortran.

The engine in use today is `voacapl`, the maintained Unix build of the ITS
VOACAP program: about 22,800 lines of FORTRAN 77 across 195 files, with 783
`GOTO` statements, no `IMPLICIT NONE` anywhere, and almost all state passed
through `COMMON` blocks rather than arguments.

A port needs an acceptance criterion before it needs any code. This crate exists
to derive that criterion from measurement.

## Layout

| Path                      | What it does                                              |
| ------------------------- | --------------------------------------------------------- |
| `src/deck.rs`             | Writes VOACAP's fixed-width input deck                    |
| `src/listing.rs`          | Reads every numeric field back out of a method 30 listing |
| `src/sweep.rs`            | Enumerates input cases covering the model's regimes       |
| `src/runner.rs`           | Drives a chosen `voacapl` binary in an isolated tree      |
| `src/compare.rs`          | Measures how far two listings differ, field by field      |
| `src/itu.rs`              | Drives the ITU-R P.533 reference implementation           |
| `src/bin/measure.rs`      | The compiler-variant experiment below                     |
| `src/wspr.rs`             | Reads aggregated WSPR reception reports                   |
| `src/bin/engines.rs`      | Compares VOACAP against ITU-R P.533                       |
| `src/bin/validate.rs`     | Compares both engines against measured radio              |
| `tools/fetch-wspr.sh`     | Downloads a month of WSPR reports, pre-aggregated         |
| `tools/build-variants.sh` | Builds `voacapl` at several optimisation levels           |

None of this is throwaway. The same parser and comparator that measure
compiler-to-compiler spread today will measure Rust-to-Fortran spread later.

There are no external dependencies, on purpose: this crate is the reference a
port gets judged against, so its own supply chain is kept empty.

## Running it

```sh
tools/build-variants.sh                 # ~1 minute, writes to vendor/voacapl-variants
cargo test
cargo run --release --bin measure -- --out docs/sensitivity.json > docs/sensitivity.md
```

`measure` needs a built `itshfbc` tree; it reads `$HFCAST_ITSHFBC`, defaulting
to `~/itshfbc`.

## The experiment

Compile the same Fortran five ways — `-O0`, `-O1`, `-O2`, `-O3` and
`-O2 -ffast-math` — then run all five over 96 sweep cases and compare every
printed number. Any difference is not physics. It is the model's sensitivity to
how its floating-point arithmetic was evaluated, which is exactly the kind of
difference a port introduces.

96 cases is 8 paths (short, medium, long east-west, long north-south, polar,
equatorial, near-antipodal, western hemisphere) crossed with 4 months and 3
sunspot numbers. Each run prints 4,824 numbers, so a full sweep compares about
463,000 cells per variant.

## What it found

Full output is in [docs/sensitivity.md](docs/sensitivity.md).

| Comparison            | Cells differing (of 463,104) | Largest difference                              |
| --------------------- | ---------------------------: | ----------------------------------------------- |
| `O2` vs `O0`          |                            0 | —                                               |
| `O2` vs `O1`          |                            0 | —                                               |
| `O2` vs `O3`          |                            6 | 1 dB loss, 1 km virtual height                  |
| `O2` vs `-ffast-math` |                         ~180 | 1 dB SNR, 0.01 reliability, 4 km virtual height |

In every comparison, the discrete outputs were identical: no propagation mode
(`1F2`, `2F2`, `1E` …) changed, and there was no case where one build printed a
value and another printed a dash. The builds never disagreed about whether a
path existed — only, very occasionally, about its last digit.

### The important caveat

The first three rows are a weaker result than they look. `gfortran` does not
reassociate floating-point arithmetic at any `-O` level; that is what
`-ffast-math` unlocks. So `-O0` through `-O3` all evaluate the same operations
in the same order, and on x86-64 they were always going to agree. Those zeros
mostly say that `gfortran` is faithful to IEEE 754, not that the model is
numerically insensitive.

The `-ffast-math` row is the one that carries information. It is the only build
that genuinely reorders arithmetic, which is what an idiomatic rewrite in
another language does. It moved 0.04% of cells, and never by more than 1 dB.

### The criterion this suggests

Do not set a tolerance by taste. Use the measured envelope:

- **Continuous fields** — accept within the `-ffast-math` envelope: 1 dB on
  signal, noise, loss and SNR; 0.01 on reliability and probability; 4 km on
  virtual height; 0.2 degrees on take-off angle.
- **Discrete outputs** — require exact equality. Propagation mode never moved
  under any build, so a port that changes one has changed the model, and no
  tolerance should hide that.
- **Structure** — require exact agreement on which cells are printed at all. A
  band being open in one implementation and closed in the other is a defect,
  not a rounding difference.

## The other engine

`src/itu.rs` and `src/bin/engines.rs` drive the ITU-R Study Group 3 reference
implementation of Recommendation P.533, which is a candidate starting point for
a port instead of VOACAP.

```sh
git clone --depth 1 https://github.com/ITU-R-Study-Group-3/ITU-R-HF.git vendor/itu-r-hf
make -C vendor/itu-r-hf/Linux -j4
cargo run --release --bin engines > docs/engines.md
cargo run --release --bin engines -- --diagnose   # alignment checks
```

It built on the first attempt with no changes.

### As a thing to port

|                    | VOACAP          | ITU-R P.533           |
| ------------------ | --------------- | --------------------- |
| Language           | FORTRAN 77      | C99                   |
| Lines in the model | 22,800          | 13,300                |
| `GOTO` / `goto`    | 783             | 0                     |
| Precision          | single          | double                |
| State              | `COMMON` blocks | arguments and structs |

The C is markedly easier to port. It is also about 40% smaller, and double
precision removes the problem of reproducing single-precision arithmetic
exactly.

### As a model

Full output is in [docs/engines.md](docs/engines.md). These are two different
models, so this is disagreement, not error, and none of it says which is right.

- **MUF.** VOACAP's single median MUF sits between P.533's two. P.533's basic
  MUF runs about 2.6 MHz lower (median) and its operational MUF about 1.3 MHz
  higher. The spread is wide, reaching 12 MHz on some hours.
- **How often a band works.** Of 20,736 hour and frequency combinations, P.533
  found no propagating mode at all in 62.7%. VOACAP named a mode in every one.
  For somebody deciding whether to call, this difference matters far more than
  a decibel.

Two things turned out not to be comparable, and the first version of this
comparison reported both as if they were. Propagation mode uses different
vocabularies: VOACAP labels the mode mix (`F2F2`, `EF2`), P.533 names one
dominant mode (`1F2`, `2E`) or `NONE`. And signal power carries dead-path
sentinel values — VOACAP prints around -1982 dBW where nothing propagates — so
averaging raw differences produced a meaningless 1,259 dB. Both are now
excluded, with the exclusions counted in the report.

`--diagnose` exists because of that mistake. It checks the assumptions the
comparison rests on: that hour `n` means the same thing to both engines (it
does; offset 0 gives the smallest MUF spread), and what the mode vocabularies
and value ranges actually look like.

## Against reality

The two experiments above compare code with code. This one compares both
engines with measured radio.

```sh
tools/fetch-wspr.sh 2025-06                 # aggregated reception reports
cargo run --release --bin validate > docs/validation.md
```

WSPR is the only large public source that records both ends of the experiment:
every report carries the transmit power, the measured signal-to-noise ratio,
both locations and a timestamp. June 2025 holds about 162 million of them.

### Method

A path is a fixed pair of stations on a fixed band, which holds the two unknown
quantities — their antennas and the receiver's local noise — constant. One
offset per path is fitted and removed, so what is measured is how well a model
tracks the **daily shape** of a circuit, not its absolute level. Absolute level
cannot be tested without knowing the antennas.

A flat baseline runs alongside: predict every hour as that path's own median.
It contains no physics, and an engine that cannot beat it is adding nothing.

### What it found

Full output in [docs/validation.md](docs/validation.md). 150 paths, 3,481
path-hours.

| predictor           | median error | correlation | slope | error after gain fit |
| ------------------- | -----------: | ----------: | ----: | -------------------: |
| VOACAP              |       4.0 dB |       +0.76 | +0.22 |               1.5 dB |
| ITU-R P.533         |       3.3 dB |       +0.59 | +0.32 |               2.0 dB |
| VOACAP, signal only |       4.0 dB |       +0.77 | +0.20 |               1.4 dB |
| P.533, signal only  |       3.1 dB |       +0.71 | +0.39 |               1.6 dB |
| flat baseline       |       2.5 dB |           — |     — |                    — |

Read the columns together, because separately they mislead:

- **Both engines lose to a flat line on raw error.** Taken alone that reads as
  the models being useless.
- **But VOACAP correlates +0.76 with the truth.** It puts the peaks and troughs
  in the right places.
- **The slope explains the contradiction.** At +0.22, the real daily swing is
  about a fifth of what VOACAP predicts. It gets the timing right and the
  amplitude badly wrong, and the raw error is dominated by that overshoot.
- **Scaled down, VOACAP is the best predictor here.** 1.5 dB residual against
  the baseline's 2.5 dB.

So the useful summary is that both models exaggerate how much a circuit varies
through the day, VOACAP more than P.533, while VOACAP tracks _when_ things
happen distinctly better.

### Is that real, or an artefact?

WSPR cannot report what it fails to decode, so weak hours read higher than they
were or vanish entirely, which would flatten the measured swing and produce a
low slope even from a perfect model. The report therefore repeats everything on
the 27 paths whose weakest hour never drops below -15 dB, comfortably clear of
the roughly -29 dB decode floor. The effect gets _stronger_ there, not weaker:
slope +0.16, residual 1.3 dB. Censoring is not what is causing it.

### Which half of the prediction is wrong

Both engines predict the received signal and the background noise separately
and subtract. Those halves can be scored apart, because VOACAP prints `S DBW`
and P.533 prints `Pr` alongside their ratios. The signal-only rows in the table
score each engine as if the receiver's noise were constant through the day —
which, for a typical WSPR receiver limited by its own local interference, it
roughly is.

The result is one-sided. Removing the noise barely changes the swing: VOACAP's
slope moves from 0.22 to 0.20, P.533's from 0.32 to 0.39. **The exaggeration
lives in the predicted signal itself**, in both engines. The noise model is not
the cause. One side finding: P.533's timing improves when its noise is removed
(correlation +0.59 to +0.71), so its noise model's daily pattern is actively
hurting its predictions on these paths.

What remains open is why the predicted signal swings too much. Two candidates
point the same way. The models follow one dominant propagation route per hour,
while the real ionosphere usually offers several at once — so when the
dominant route fades, reality does not drop as far as the model says. And June
is the peak of the sporadic-E season, a summer layer that keeps paths alive
when the main-layer prediction says they should fade; standard VOACAP practice,
followed here, disables its sporadic-E term because that term is considered
unreliable. A winter month, where sporadic-E largely disappears, would separate
these: if the exaggeration shrinks in winter, the missing mechanism is found.

### Limits

One month, one solar level (smoothed sunspot number 125), and a receiver
population concentrated in North America and Europe. Both engines are run at a
fixed one watt, because signal-to-noise is linear in transmit power and the
P.533 reference implementation rejects anything below a watt outright
(`RTN_ERRTXPOWER`), which excludes most WSPR beacons.

## Concurrency, and a bug this turned up

`voacapl` cannot be run concurrently against one `itshfbc` tree. `decred.for`
builds its antenna scratch filename from the antenna index alone:

```fortran
write(gainfile,'(4hgain,i2.2,4h.dat)') iantr
```

so every run writes `<root>/run/gain01.dat` and `gain02.dat` under those fixed
names. Two runs sharing a tree overwrite each other's gain files, and a run that
reads one mid-write dies with a Fortran end-of-file fault. Giving each run
unique deck filenames does not help, because these names come from the engine,
not from the caller.

The first version of this harness made exactly that mistake, and the failures it
produced looked convincingly like compiler bugs. `src/runner.rs` now gives each
run a private tree. The tree is mostly symbolic links into the installed share
directory, so a private copy is cheap as long as the links are recreated rather
than followed.

`server/src/voacap/run.ts` had the same mistake. It now keeps a small pool of
private trees and hands one to each run, because copying a tree takes longer
than a run does.
