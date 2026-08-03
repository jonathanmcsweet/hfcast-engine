# HFcast Engine

[![CI](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/ci.yml)
[![Parity soak](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/soak.yml/badge.svg)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/soak.yml)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue)](LICENSE)
[![No dependencies](https://img.shields.io/badge/dependencies-none-brightgreen)](Cargo.toml)

Built for, each checked on its own:

[![linux x86_64](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/linux-x86-64.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![linux aarch64](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/linux-aarch64.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android arm64-v8a](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-arm64-v8a.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android armeabi-v7a](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-armeabi-v7a.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android x86_64](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-x86-64.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android x86](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-x86.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)

The two Linux badges run the whole test suite on that architecture. The
four Android ones are library builds: a runner cannot execute them.

A radio propagation engine in Rust, and the tests that prove it is
correct. No dependencies: everything here is `std`.

```sh
cargo add hfcast
```

## What this is

High frequency radio signals travel long distances because the
ionosphere reflects them. How well they travel changes with the hour,
the season, and the activity of the sun. VOACAP is the model that
predicts this. The US Institute for Telecommunication Sciences wrote it,
and much of the world still uses it.

VOACAP is approximately 22,800 lines of FORTRAN 77 in 195 files. It has
783 `GOTO` statements. It does not use `IMPLICIT NONE`. Almost all of
its data moves through `COMMON` blocks and not through arguments. It is
correct, and it is very difficult to change.

This is a translation of that model into Rust. The translation gives the
same answer as the original, to the last printed character. Then it
gives a second answer, with the defects of the original corrected.

The application that uses it is
[HFcast](https://github.com/jonathanmcsweet/hfcast), which puts the engine on a
telephone.

## The proof

A translation is only useful if you can show it is faithful. Each test
below runs the original Fortran and this engine on the same input, and
compares the output character by character.

| Test | What it compares | Result |
| --- | --- | --- |
| `portcheck` | 463,104 printed cells and 23,040 mode labels, over 96 paths | 0 differ |
| `fuzz` | 600 generated inputs, 434,116 lines of output | identical |
| `areacheck` | 749 area points and 17,791 cells | identical |
| `lufcheck` | 1,152 rows of the lowest usable frequency table | identical |
| `antcheck` | each antenna type, against the gain files of the original | identical |
| `paritycheck` | 7,104 fields that the application reads | 0 differ |
| `archcheck` | this engine against itself on a different processor | identical |

Plus 203 unit tests and 40 harness and integration tests.

A [daily job](docs/soak.md) runs 200 paths through both engines with the
space weather of that day. It fails if one number is different.

## The two behaviours

Select with `api::Request::model`:

- **`Model::Compatible`** is the default. It is VOACAP as it is, with
  the defects included. The tests above can judge only this behaviour,
  which is why it is the default. A caller who says nothing gets
  numbers that are proved.
- **`Model::Corrected`** is VOACAP with six recorded defects corrected.
  It is not the same as the original, on purpose.

`src/voacap/model.rs` has one method for each defect. It is the complete
list of the ways the two can be different.
[docs/corrected.md](docs/corrected.md) records what each correction
moves, and says which corrections have no measurement of accuracy behind
them.

A change that touches everything — `f32` to `f64`, the order of
arithmetic, state that stays between hours — is **not** behind that
switch. A flag cannot describe such a change honestly. The result would
be a different model, not VOACAP with a correction.

## Why keep the defects

If the engine copies the defects, then "the same as the original" is
something you can test. If it does not, it is an opinion. That test is
what the whole method depends on.

Corrections then live in one named place, where each one can be measured
alone.

## How accurate is it

The engine gives the same answers as VOACAP, so it is exactly as
accurate as VOACAP. That is a separate question, and this repository
measures it against real radio reports:

VOACAP puts the good hours and the bad hours in the correct places
(correlation +0.76 against measured WSPR reports). It exaggerates the
difference between them by approximately four and a half times (slope
+0.22). [docs/accuracy.md](docs/accuracy.md) has the measurements, and
[docs/validation.md](docs/validation.md) has the comparison with ITU-R
P.533.

## Layout

| Path | What it does |
| --- | --- |
| `src/voacap/` | The translated engine |
| `src/api.rs` | The public interface: a request in, a report out |
| `src/deck.rs` | Writes the fixed-width input file VOACAP reads |
| `src/listing.rs` | Reads each number back out of the output |
| `src/sweep.rs` | Makes input cases that cover the model's regimes |
| `src/fuzz.rs` | Makes valid inputs from a seed |
| `src/runner.rs` | Operates a chosen `voacapl` binary in a separate tree |
| `src/compare.rs` | Measures how far two outputs differ, field by field |
| `src/wspr.rs` | Reads collected WSPR reception reports |
| `src/itu.rs` | Operates the ITU-R P.533 reference implementation |
| `src/bin/` | The tests, `predict`, and `spacewx` |
| `embedded/` | The 653 KB of data the engine needs, compiled in |

**There are no dependencies, on purpose.** This crate is the reference
that a translation is judged against, so its own supply chain is empty.
Everything is `std`.

## Operate it

You need a Rust toolchain, `gfortran`, and a copy of `voacapl` in
`vendor/voacapl`.

```sh
tools/build-variants.sh    # builds the original at five optimisation levels
cargo test
cargo run --release --bin portcheck
```

`voacapl` needs an installed `itshfbc` data tree. The tests read
`$HFCAST_ITSHFBC`, and use `~/itshfbc` if it is not set.

`cargo test` runs the default build, which reads the coefficients from
that tree. `cargo test --all-features` also runs the tests that ask for
the compiled-in copy. CI runs both.

Hooks run the checks for you: formatting and clippy before a commit,
both test builds and the analysis gates before a push. Turn them on once
per clone, because git does not enable a hook directory by itself:

```sh
git config core.hooksPath .githooks
```

To make one prediction:

```sh
echo '{"fromLat":47.6,"fromLon":-122.3,"toLat":51.5,"toLon":-0.1,
       "month":8,"year":2026,"ssn":60,"watts":100}' |
  cargo run --release --bin predict
```

[docs/port.md](docs/port.md) has the complete list of tests, the options
each one takes, and a "Traps" section that records each way a result has
been wrong here before. Read it before you trust a result.

## Static analysis

```sh
tools/analyze.sh          # clippy, complexity, duplication, coverage
tools/analyze.sh --gate   # the same, but it fails on a broken gate
```

[docs/analysis.md](docs/analysis.md) explains which warnings must never
be applied. Some of them would change the arithmetic and break the
agreement with the original.

## Documents

| Document | What it covers |
| --- | --- |
| [port.md](docs/port.md) | How the translation is proved, and the traps |
| [analysis.md](docs/analysis.md) | The static analysis suite |
| [corrected.md](docs/corrected.md) | Each corrected defect and what it moves |
| [sensitivity.md](docs/sensitivity.md) | The measured tolerance |
| [accuracy.md](docs/accuracy.md) | Both engines against measured radio |
| [validation.md](docs/validation.md) | The scores against WSPR reports |
| [engines.md](docs/engines.md) | VOACAP against ITU-R P.533 |
| [storm.md](docs/storm.md) | Geomagnetic storm widening |
| [daily.md](docs/daily.md) | Whether a daily forecast is possible |
| [irtam.md](docs/irtam.md) | Real-time ionospheric maps, measured |
| [licence.md](docs/licence.md) | Where the code and the data come from |
| [soak.md](docs/soak.md) | The daily parity job |

## Data files

The engine needs 560 KB of data: the ionospheric maps and noise tables for
each month, the antenna files, and one version string. Where they come
from decides how they are shipped.

| Part | Size | Origin | In the published crate |
| --- | --: | --- | --- |
| Antenna files, version string | 16 KB | NTIA/ITS | yes |
| Sporadic E, E, F1, prediction error | 195 KB | NTIA/ITS | **no** |
| Atmospheric noise | 216 KB | CCIR Report 322 | **no** |
| foF2 and M(3000)F2 maps | 134 KB | CCIR Report 340 | **no** |

The URSI-88 foF2 maps are in no build. They are the one part the ITU does
not publish itself, and nothing here selects them, so a `COEFFS URSI88`
card needs a real `itshfbc` root.

The coefficients are behind the `embedded-coefficients` feature, which is
off by default, and the files are excluded from the package. A build from
crates.io reads them from an `itshfbc` tree, which is how the reference
engine has always found them:

```rust
// From a tree on disk. No feature needed.
let answer = hfcast::service::run(r#"{"itshfbc": "/home/you/itshfbc", ...}"#)?;
```

A build from this repository can compile them in instead, which is what
the telephone application does, because a telephone has no tree to read:

```sh
cargo build --features embedded-coefficients
```

Asking for `"<embedded>"` without the feature fails with a message saying
so, rather than quietly giving a wrong answer.

[NOTICE](NOTICE) records what is inside `embedded/coeffs/`, array by
array, and [docs/licence.md](docs/licence.md) records how that was
measured and what it does and does not settle. In short: ITU-R Study
Group 3 publishes the CCIR Report 322 and 340 data itself, for
implementers, free from copyright assertions, in its P.372 and P.533
reference software. The crate does not rely on that — it carries none of
it — but the repository and the telephone application do.

## Licence

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

The translated model comes from work that is not subject to copyright
protection in the United States, and from changes released under CC0.
[docs/licence.md](docs/licence.md) records where it comes from in full,
and the limits of that finding.
