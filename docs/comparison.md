# The parity engine and the new model, side by side

Two prediction models live in this crate. The **parity engine** is the
faithful VOACAP port: its contract is byte-equivalence with the
Fortran (`portcheck`, 23,040 cells), and within a month only the hour
changes its answer. The **new model** (`src/truecast/`) is the same
physics conditioned on live data: a daily effective sunspot index
fitted from ionosonde readings, a Kp storm table, the corrected height
form, and a calibrated absorption edge. This page puts the two side by
side over the eight validation months. Every number is copied from a
generated source named in its section; regenerate those and this page
is stale until the copies are refreshed.

The two are compatible by construction, and the compatibility is a
test, not a claim: a truecast run at an index at or above zero answers
**exactly** what the parity engine answers at that number
(`tests/request_guards.rs`, string equality), and every service answer
names the engine behind it. The comparison below is therefore about
what the conditioning adds on real days, not about two divergent
physics.

## Day by day, across the archive

The monthly tables below aggregate; the daily view does not.
`sonde --daily` prints one CSV line per day, with HFcast Compatible and
HFcast Truecast both scored against that day's own soundings (sample
count, bias and MAE for the
fitted-index model and for climatology, the day's fitted index, the
calibrated lower edge against fmin, and the day's peak Kp).
`tools/backfill.sh 2015-01 <now>` builds the month bundles across the
archive and writes the combined file to `data/daily-comparison.csv`;
`tools/live-check.sh` extends the same comparison forward every day.
The span starts at 2015 because the curated station list and the R12
table (`src/wspr.rs`, now monthly since 2015-01) are solid from
there; the tooling takes any range, so the span can be extended
backward by running the same script for earlier years.

## foF2 against ionosonde truth

237,506 samples, 14-22 stations per month, model minus observed, MHz.
The parity engine is the `climatology` column; the new model's
deployable column is `essn` (the daily index fitted while always
leaving the scored station out). Source: `docs/ionosonde.md` and
`docs/ionosonde-output.md` (`sonde` over the eight bundles).

| month | parity bias / MAE | new bias / MAE | day-to-day corr, new |
| --- | --- | --- | ---: |
| 2015-03 | -0.26 / 0.71 | +0.00 / 0.58 | +0.390 |
| 2019-06 | +0.38 / 0.49 | +0.02 / 0.40 | +0.147 |
| 2019-12 | +0.07 / 0.54 | -0.01 / 0.55 | +0.108 |
| 2022-09 | +0.53 / 0.69 | +0.00 / 0.51 | +0.462 |
| 2024-12 | +0.86 / 0.96 | +0.01 / 0.62 | +0.428 |
| 2025-03 | +0.34 / 0.70 | +0.00 / 0.62 | +0.404 |
| 2025-06 | +0.74 / 0.91 | -0.01 / 0.57 | +0.164 |
| 2025-07 | +0.59 / 0.75 | -0.04 / 0.51 | +0.287 |

The parity engine's bias moves month to month (-0.26 to +0.86 MHz)
because a monthly median map cannot follow the solar cycle's error
within a month; the fitted index removes it out of sample in every
month and improves MAE in seven of eight. The day-to-day column is the
skill climatology cannot have: the parity engine's day-to-day
correlation is exactly zero **by construction** (the same answer every
day), and the harness prints that guard with every table.

## Storm hours, held out

foF2 on hours with trailing-24-hour Kp at or above 5, MAE in MHz. The
new model adds the embedded storm table on top of the index
(`essn+storm`); both storm months were excluded from every fit.
Source: `docs/ionosonde.md`, finding 5.

| month | n | parity | new (essn) | new (essn+storm) |
| --- | ---: | ---: | ---: | ---: |
| 2015-03 | 3006 | 0.888 | 0.763 | 0.727 |
| 2022-09 | 1798 | 1.164 | 0.602 | 0.581 |

(The parity column is climatology on the same storm hours, from the
same report tables; its storm-hour bias is +0.42 and +1.11 MHz where
the new model's is within 0.09 of zero. The essn+storm column is the
2026-08-15 whole-archive table; the wider held-out verdict, including
three more storm months, is in `ionosonde.md`.)

## NVIS band calls: an honest null

MUF at 600 km ground range, and the band-call question the
application asks (was 80/60/40/30 m usable this hour). The new model's
full deployable pipeline is `essn+st+dud` (daily index, storm ratio,
Dudeney height). Source: `docs/ionosonde-output.md`, NVIS tables.

| month | parity MAE / calls | new MAE / calls |
| --- | --- | --- |
| 2015-03 | 1.241 / 92.7% | 0.906 / 93.7% |
| 2019-06 | 0.727 / 89.9% | 0.791 / 88.9% |
| 2019-12 | 0.833 / 88.2% | 0.850 / 87.9% |
| 2022-09 | 0.953 / 91.5% | 0.910 / 91.6% |
| 2024-12 | 1.046 / 92.0% | 1.045 / 93.0% |
| 2025-03 | 1.087 / 93.4% | 1.088 / 94.1% |
| 2025-06 | 0.960 / 89.8% | 0.937 / 89.7% |
| 2025-07 | 0.797 / 91.3% | 0.816 / 91.1% |

At month scale the two models call bands at the same rate outside the
severe storm month. This is the program's honest null: the new
model's value at NVIS ranges is day-level and storm-hour, the hours
a monthly average gets wrong, rather than the monthly totals, where
climatology is already calibrated. (The upper-bound columns using
assimilated maps, in the source tables, show 94-96% is reachable with
better height input; that is measurement, not deployment.)

## Heights

The parity engine's hmF2 ran +61.5 km high in 2025-06. The corrected
Dudeney form over the engine's own M(3000)F2, which is what the new
model's point answers use, removes about 19 km of it; the rest is the
M(3000)F2 input itself, and closing it needs an assimilated height
source (IRTAM hmF2 measured +3.5 km bias as the upper bound). Source:
`docs/ionosonde.md`, finding 3.

## The usable window's lower edge

The parity engine has no deployable counterpart: its LUF task floors
at 2 MHz, has a usable-budget window of about 4 dB on an NVIS probe,
and flips sign outside it (measured, `docs/ionosonde.md`). The new
model answers `truecast::api::lower_edge`, the absorption-edge probe
behind a fitted level that follows the day's index and the season
(2026-08-15 refit), with held-out error 0.62 to 1.04 MHz MAE
against ionogram fmin across eight months spanning quiet minimum to
the 2024-05 superstorm, bias within 0.26 MHz of zero in seven of
the eight. Source: `docs/ionosonde.md` and `sonde --fit-edge`.

## Fully offline, no network ever

The new model does not need the daily feed to beat the parity
engine. A never-online device runs `Conditioning::offline`: the
shipped sunspot calendar plus a fitted day-of-year correction curve
(`docs/offline.md`, 2026-08-15). Scored leave-one-year-out over the
whole archive it improves foF2 MAE from 0.766 to 0.717 MHz, about
two fifths of the live daily index's whole advantage and for free, and
it wins in eleven of twelve years, all but 2015.

## Link level, real radio paths

WSPR reception reports, 150 paths per month, about 525,000
path-day-hours: each model's predicted SNR against the day's median
report, offset-adjusted MAE in dB, and the correlation between
predicted and observed day-to-day movement. Two engine runs per
path-day question, identical but for one number: the sunspot number is
either the month's smoothed value or the day's fitted index from GIRO
soundings (`sonde::essn_series`, the all-station fit, since the WSPR
paths are independent of the ionosonde network). Reproduce with
`tools/fetch-wspr.sh <month> data/<month>` then `essn_validate`.

| month | parity MAE | new MAE | day corr, new | storm-day corr, new |
| --- | ---: | ---: | ---: | ---: |
| 2015-03 | 3.59 | 3.57 | +0.091 | +0.158 |
| 2019-06 | 3.64 | 3.68 | +0.025 | -0.001 |
| 2019-12 | 3.87 | 3.90 | +0.021 | (no storm days) |
| 2022-09 | 4.37 | 4.20 | +0.078 | +0.166 |
| 2024-12 | 3.58 | 3.57 | +0.027 | +0.051 |
| 2025-03 | 3.85 | 3.81 | +0.056 | +0.066 |
| 2025-06 | 4.76 | 4.36 | +0.024 | +0.054 |
| 2025-07 | 4.51 | 4.23 | +0.059 | +0.149 |

Active months improve 0.17-0.40 dB, on a ruler where the whole observed
day-to-day spread is about 2.5 dB and most of it is not ionospheric.
The two deep-solar-minimum months cost 0.03-0.04 dB, which is the
ruler's own resolution. The storm-day correlations are where the daily
index shows on real links, and the parity engine's day-to-day link
correlation is zero by construction, as with foF2.

That solar-minimum cost was the engine below its map, not the fit. The
first run of this study (0.79.0) found both 2019 months 0.03 to 0.07 dB
worse and blamed fit noise. Diagnosed 2026-08-13: the solar-minimum fit
is the best-sampled of all eight months (up to 496 samples per day,
standard error 0.7 index units), and shrinking its day-to-day movement
recovered nothing. The cost sat in the persistent offset, which ran the
whole engine at an index near -17, below the map's zero-sunspot plane,
where foE, absorption and noise have no measured state to extrapolate
into. The floor, where foF2 follows the fitted line while every other
channel stops at zero, removed half the 2019-06 cost and raised its day
correlation from +0.018 to +0.025 while leaving every non-negative
index untouched, so the regenerated cache for an active month is
byte-identical. The floor has no fitted constant, so there is nothing
to overfit and nothing to re-tune.

Day-level link skill is small and lives on storm days, +0.15 to +0.17
where storms happened and near zero on quiet months. That sits against
a ceiling measured on these same paths: the daily residuals carry a
lag-1 autocorrelation of +0.340, so even a perfect next-day predictor
would shrink a typical 1.29 dB residual to 1.21 dB, a gain of 0.08 dB.
What structure there is concentrates on disturbed days, +0.424 after a
day reaching Kp 5 against +0.328 after a quiet one, and the IRTAM map
scores +0.1 on the same ruler. This is why the day-level case rests on
ionosonde truth rather than on link reports. Link-level SNR error is
dominated by station factors the engine cannot know; the calibration
phase (WSPR and RBN offsets) remains the tool for that layer.

## Performance

The same physics serves both models, so per-point cost is identical.
The new model's grid driver threads inside the engine over one shared
setup: the application's fine globe (34,560 points, one band) in
131 ms at eight threads against 1088 ms for the serial parity area
driver, with bit-identical answers and thread-count invariance. Source:
`docs/truecast.md`, `gridbench`.

## Reproduction

- foF2 / storm / NVIS / edge: `tools/fetch-kp.sh`,
  `tools/fetch-giro.sh <month>`, `tools/fetch-irtam.sh <month>`, then
  `cargo run --release --all-features --bin sonde -- --kp
  data/kp_daily.txt data/<month> ...` (the committed
  `docs/ionosonde-output.md` is the cache-loaded report).
- Link level: `tools/fetch-wspr.sh <month>`, then
  `cargo run --release --all-features --bin essn_validate`.
- The compatibility proof: `cargo test --all-features
  request_guards`.
- The live continuation of this comparison: `tools/live-check.sh`
  (`docs/soak.md`), which appends the same essn-versus-climatology
  scoring to `data/live/ledger.csv` daily.
