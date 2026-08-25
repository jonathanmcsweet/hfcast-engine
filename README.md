# HFcast Engine

[![CI](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/ci.yml/badge.svg)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/ci.yml)
[![Parity soak](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/soak.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/soak.yml)
[![Licence](https://img.shields.io/badge/licence-Apache--2.0-blue)](LICENSE)
[![No dependencies](https://img.shields.io/badge/dependencies-none-brightgreen)](Cargo.toml)

Built for these platforms, each checked on its own:

[![linux x86_64](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/linux-x86-64.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![linux aarch64](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/linux-aarch64.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android arm64-v8a](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-arm64-v8a.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android armeabi-v7a](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-armeabi-v7a.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android x86_64](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-x86-64.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)
[![android x86](https://img.shields.io/endpoint?url=https://raw.githubusercontent.com/jonathanmcsweet/hfcast-engine/badges/android-x86.json)](https://github.com/jonathanmcsweet/hfcast-engine/actions/workflows/arch.yml)

## What this is

High frequency radio signals (HF) travel long distances under differeing conditions making their reach hard to predict: it changes with the hour, the season, and the activity of the sun. 

This libary offers three models to predict HF propagation: 

1. A faithful port of VOACAP, built for the Voice of America from the US Institute for Telecommunication Sciences' IONCAP
2. VOACAP Corrected, a VOACAP implementation with defects fixed,
3. Truecast, which runs VOACAP's physics against a more granular daily average, the effective sunspot index, a geomagnetic storm table, a corrected
layer height, and a lower edge of the usable window that the original
cannot give.

## Quick start

No dependencies: everything here is `std`.

```sh
cargo add hfcast
```

## Our engines

Two of the three are forms of the port, chosen with
`api::Request::model`. The third is a separate engine, chosen by the
request itself.

### VOACAP `Model::Compatible`

VOACAP is approximately 22,800 lines of FORTRAN 77 in 195 files. It has
783 `GOTO` statements. It does not use `IMPLICIT NONE`. Almost all of
its data moves through `COMMON` blocks and not through arguments. 

This is that model translated into Rust, defects included, and it gives
the same answer as the original to the last printed character.

### VOACAP with defects fixed `Model::Corrected`

The same engine with six recorded defects corrected, and nothing else
changed. `src/voacap/model.rs` has one method per defect, which is the
complete list of ways the two can differ.
[docs/corrected.md](docs/corrected.md) records what each correction
moves, and says which ones have no measurement of accuracy behind them.

### Truecast `"engine": "truecast"`

The third model lives in `src/truecast/`. It's chosen by the request
rather than by `Model`, because it's a second engine rather than a
variant of the port.

VOACAP predicts a monthly median, so every day of a month gets the same
answer. Truecast conditions that same climatology on the day itself:

- a **daily effective sunspot index**, fitted from ionosonde soundings,
  replaces the monthly smoothed number when the caller has one
- a **geomagnetic storm table** widens the forecast when the measured
  Kp says the ionosphere is disturbed
- with **no network at all** the engine derives its own index for the
  date, from the embedded sunspot table and a fitted day-of-year
  correction, so a device that never goes online still beats the
  monthly median.

Fitted on an eleven-year ionosonde archive. The verdict comes from
eight months held back before any fit ran, chosen by rule to cover the
record's worst storms, its quietest spells and its seasonal edges.

[docs/comparison.md](docs/comparison.md) puts the two models side by
side; [docs/offline.md](docs/offline.md) is the measured case that the
offline form beats the monthly median on individual days.

### Why keep the defects?

If the engine copies the defects, then "the same as the original" is
something you can test and verify.

## The proof for the orignial VOACAP model

Each test below runs the original Fortran and this engine on the same input, and compares the output character by character.

| Test | What it compares | Result |
| --- | --- | --- |
| `portcheck` | 463,104 printed cells and 23,040 mode labels, over 96 paths | 0 differ |
| `fuzz` | 600 generated inputs, 434,116 lines of output | identical |
| `areacheck` | 749 area points and 17,791 cells | 19 of 21 grids identical, 2 differ [by design](docs/port.md) |
| `lufcheck` | 1,152 rows of the lowest usable frequency table | identical |
| `antcheck` | each antenna type, against the gain files of the original | identical |
| `paritycheck` | 7,104 fields the [HFcast](https://github.com/jonathanmcsweet/hfcast) app reads | 0 differ |
| `archcheck` | this engine against itself on a different processor | identical |

Plus 307 unit tests and 61 harness and integration tests.

A [daily job](docs/soak.md) runs 200 paths through HFcast Compatible and
the Fortran reference with the space weather of that day. It fails if one
number is different.

## How accurate is it

The port gives the same answers as VOACAP, so it is exactly as accurate
as VOACAP. That is a separate question, and this repository measures it
against real radio reports: VOACAP puts the good hours and the bad
hours in the correct places (correlation +0.76 against measured WSPR
reports), and exaggerates the difference between them by approximately
four and a half times (slope +0.22).
[docs/accuracy.md](docs/accuracy.md) has the measurements, including
the comparison with ITU-R P.533.

Truecast is measured against ionosonde soundings, which observe the
ionosphere directly where the WSPR record can only infer it. Over the
eight held-out months it removes the port's month-to-month bias and
improves foF2 error in seven of the eight; on storm hours it improves
on the port by 0.16 and 0.58 MHz. Fully offline, with no reading of any
kind, it still improves on the port in eleven of twelve years.
[docs/comparison.md](docs/comparison.md) has the tables.

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
| `src/truecast/` | The second engine: climatology conditioned on the day |
| `src/giro.rs` | Reads GIRO ionosonde soundings, the ground truth |
| `src/essn.rs` | Fits the daily effective sunspot index from them |
| `src/stormfit.rs` | The fitted geomagnetic storm table |
| `src/bin/` | The tests, `predict`, `sonde`, and `spacewx` |
| `embedded/` | The 560 KB of data the engine needs, compiled in |

There are no dependencies, on purpose because this crate faithfully replicates the reference VOACAP model

Everything is `std`.

## Testing

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
       "month":8,"year":2026,"ssn":60,"watts":100,
       "bands":[7.1,14.1,21.1],"requiredSnrDb":24,"noiseDbw":-145}' |
  cargo run --release --bin predict
```

Each field above is necessary. `predict` reads the data tree named in
the request, or `$HFCAST_ITSHFBC`, or `~/itshfbc`. A build with
`--features embedded-coefficients` accepts `"itshfbc":"<embedded>"` and
needs no tree.

The same request selects Truecast by swapping `"ssn"` for
`"engine":"truecast"`. A live daily index is passed as `"essn"`. With
no index at all the engine derives its own for the date — the offline
form, which also takes an optional `"day"` (the 15th if absent) and an
optional baked `"sync"` record:

```sh
echo '{"fromLat":47.6,"fromLon":-122.3,"toLat":51.5,"toLon":-0.1,
       "month":8,"year":2026,"day":17,"engine":"truecast","watts":100,
       "bands":[7.1,14.1,21.1],"requiredSnrDb":24,"noiseDbw":-145}' |
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

The script's own header says what each step is for, and the comment
above `parity_allows` records which clippy suggestions must never be
applied. Some of them would change the arithmetic and break the
agreement with the original.

## Documents

| Document | What it covers |
| --- | --- |
| [port.md](docs/port.md) | How the translation is proved, and the traps |
| [corrected.md](docs/corrected.md) | Each corrected defect and what it moves |
| [sensitivity.md](docs/sensitivity.md) | The measured tolerance |
| [accuracy.md](docs/accuracy.md) | VOACAP against measured radio, and against P.533 |
| [reliability.md](docs/reliability.md) | The day-to-day spread, and storm days |
| [truecast.md](docs/truecast.md) | The second pipeline and its contract |
| [ionosonde.md](docs/ionosonde.md) | Truecast against ionosonde truth, and the fits |
| [comparison.md](docs/comparison.md) | The two models, side by side |
| [offline.md](docs/offline.md) | The forecast with no network at all |
| [soak.md](docs/soak.md) | The recurring daily checks |
| [licence.md](docs/licence.md) | Where the code and the data come from |

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
the [HFcast](https://github.com/jonathanmcsweet/hfcast) phone app does:

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
reference software.

## Licence

Apache-2.0. See [LICENSE](LICENSE) and [NOTICE](NOTICE).

The translated model comes from work that is not subject to copyright
protection in the United States, and from changes released under CC0.
[docs/licence.md](docs/licence.md) records where it comes from in full,
and the limits of that finding.
