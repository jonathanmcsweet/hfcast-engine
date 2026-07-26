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
| `src/bin/engines.rs`      | Compares VOACAP against ITU-R P.533                       |
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
