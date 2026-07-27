# HFcast Engine

An HF radio propagation engine in Rust: a verified port of VOACAP,
together with the harness that verifies it.

VOACAP is the ITS ionospheric propagation model, about 22,800 lines of
FORTRAN 77 across 195 files, with 783 `GOTO` statements, no `IMPLICIT
NONE` anywhere, and almost all state passed through `COMMON` blocks
rather than arguments. `voacapl` is its maintained Unix build.

This crate reproduces that model exactly, and then offers a second
behaviour with its documented defects fixed.

## Status

The port is complete. It reproduces the reference bit for bit at the
listing level, over every corpus there is an oracle for:

| Check          | Result                                                                    |
| -------------- | ------------------------------------------------------------------------- |
| `portcheck`    | 463,104 printed cells and 23,040 mode labels over 96 sweep cases, 0 differ |
| `fuzz`         | 600 generated decks identical as text — 434,116 lines, 2,031,840 cells    |
| `areacheck`    | 749 area points and 17,791 cells matching                                 |
| `lufcheck`     | 1,152 `OUTMUF` rows matching                                              |
| `antcheck`     | every antenna family matching the reference's own gain files             |
| `paritycheck`  | 7,104 fields over both production paths, 0 differing                     |

Plus 172 unit tests and 18 harness and integration tests.

## The two behaviours

Choose with `api::Request::model`:

- **`Model::Compatible`** (the default) — VOACAP as it is, defects
  included. Byte-identical to the reference. This is the only
  behaviour any of the harnesses can judge, which is why it is the
  default: a caller who says nothing gets verified numbers.
- **`Model::Corrected`** — VOACAP with six documented defects fixed.
  Deliberately not identical to the reference.

`src/voacap/model.rs` names one method per defect and is the complete
list of ways the two can differ. [docs/corrected.md](docs/corrected.md)
records what each fix moves, and states plainly which fixes have no
accuracy measurement and why.

Anything pervasive — `f32` to `f64`, evaluation order, state that
persists between hours — is deliberately **not** behind that switch. A
flag cannot honestly describe those, and the result would be a
different model rather than VOACAP with a fix.

## Why bug-compatible on purpose

Reproducing the defects is what makes "identical to the reference" a
checkable claim rather than an opinion, and that claim is what the
whole verification method rests on. Fixes then live in one named place
where each can be measured on its own.

## Layout

| Path            | What it does                                              |
| --------------- | --------------------------------------------------------- |
| `src/voacap/`   | The ported engine                                         |
| `src/api.rs`    | The public face: structured requests in, reports out      |
| `src/deck.rs`   | Writes VOACAP's fixed-width input deck                    |
| `src/listing.rs`| Reads every numeric field back out of a method 30 listing |
| `src/sweep.rs`  | Enumerates input cases covering the model's regimes       |
| `src/fuzz.rs`   | Generates valid decks from a seed                         |
| `src/runner.rs` | Drives a chosen `voacapl` binary in an isolated tree      |
| `src/compare.rs`| Measures how far two listings differ, field by field      |
| `src/wspr.rs`   | Reads aggregated WSPR reception reports                   |
| `src/itu.rs`    | Drives the ITU-R P.533 reference implementation           |
| `src/bin/`      | The harnesses, `predict`, and `spacewx`                   |
| `trace/`        | Instrumented copies of reference routines, for stage traces |
| `soak-paths.tsv`| The 200 paths the daily parity soak runs                  |

There are no external dependencies, on purpose: this crate is the
reference a port gets judged against, so its own supply chain is kept
empty. Everything is `std`.

Several comments say "the server". This engine was extracted from an
application that consumes it; the term means that calling application,
whose source is not part of this repository.

## Running it

Needs a Rust toolchain, `gfortran`, and a `voacapl` checkout in
`vendor/voacapl`.

```sh
tools/build-variants.sh    # builds the reference at O0 O1 O2 O3 fastmath
cargo test
cargo run --release --bin portcheck
```

`voacapl` needs an installed `itshfbc` data tree. The harnesses read
`$HFCAST_ITSHFBC`, defaulting to `~/itshfbc`.

[docs/port.md](docs/port.md) has the full harness list, the flags each
takes, and a "Traps" section recording every way a verdict has been
wrong here before. Read it before trusting a result.

## Documentation

| Document                                       | What it covers                                   |
| ---------------------------------------------- | ------------------------------------------------ |
| [port.md](docs/port.md)                         | How the port is verified, and the traps          |
| [corrected.md](docs/corrected.md)               | Each fixed defect, what it moves, what it proves |
| [sensitivity.md](docs/sensitivity.md)           | The measured tolerance envelope                  |
| [accuracy.md](docs/accuracy.md)                 | Both engines against measured radio              |
| [engines.md](docs/engines.md)                   | VOACAP against ITU-R P.533                       |
| [storm.md](docs/storm.md)                       | Geomagnetic storm widening                       |
| [irtam.md](docs/irtam.md)                       | Real-time ionospheric maps, measured             |
| [licence.md](docs/licence.md)                   | Provenance of the code and the data files        |
| [soak.md](docs/soak.md)                         | The live parity soak and its exit criteria       |
| [roadmap.md](docs/roadmap.md)                   | Open work                                        |

## Data files

The engine reads about 760 KB from an `itshfbc` tree at run time. The
crate ships none of it. Whether it should embed those files is an open
decision, recorded in [docs/licence.md](docs/licence.md): most of them
are pure NTIA/ITS with no question over them, but the 637 KB of
ionospheric coefficients originate in ITU publications.

## Licence

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

The ported model derives from work that is not subject to copyright
protection in the United States, and from changes released under CC0.
[docs/licence.md](docs/licence.md) records the provenance in full,
including the limits of that finding.
