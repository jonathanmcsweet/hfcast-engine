# Predictions against ionosonde truth

`docs/irtam.md` closed with three ways its answer could change, and the
largest was ground truth: WSPR daily medians are noisy enough to hide
real skill. This measurement is that better ruler. It scores predicted
ionospheric characteristics against the scaled soundings of the GIRO
ionosonde network — absolute values, in the model's own units, over
known points. Full program output: [ionosonde-output.md](ionosonde-output.md).
Reproduce with `tools/fetch-kp.sh`, `tools/fetch-giro.sh <month>`,
`tools/fetch-irtam.sh <month>`, then
`cargo run --release --all-features --bin sonde -- --kp data/kp_daily.txt data/<month>`.

## Method

For each station in a month bundle, the engine runs one probe path of
about 111 km centered on the station, so the path's single control point
is the station itself. `Task::Parameters` returns the unrounded layer
values per hour. Two model columns so far:

- **climatology** — the engine as shipped, at the month's smoothed
  sunspot number.
- **irtam** — the same, with each day's archived IRTAM map written over
  the coefficient file through the overlay root (`src/irtam.rs`), as in
  the WSPR study. foF2 for the frequency rows; for the height rows the
  hmF2 map goes through the same slot, so the engine's own Jones-Gallet
  evaluator computes it at the station.
- **climatology+dudeney** (heights only) — climatology's own M(3000)F2
  through Dudeney's corrected form instead of the engine's plain
  `1490/M - 176`, separating the formula's error from its input's.

Predicted foF2 is put back on the ionosonde's convention before the
comparison: the engine's F2 working frequency is the extraordinary wave
(the map value plus half the gyrofrequency), and an ionosonde scales the
ordinary wave. Without that step the whole column reads about 0.55 MHz
high, and the error is the magnetic field, not the model.

The decisive day-to-day metric is the one the WSPR study used: the
correlation between predicted and observed deviations from each
station-hour's monthly median. Climatology scores exactly zero by
construction, and the harness prints that guard with every table.

NVIS is scored as its own class: MUF at ground ranges of 0, 300 and
600 km, from foF2 and the mirror-geometry secant at hmF2, plus the
band-call question the app's user actually asks — was 80/60/40/30 m
usable this hour — as hit, miss and false-alarm rates.

## What the eight months say (237,506 samples, 14-22 stations each)

foF2, model minus observed, and the NVIS band calls at 600 km:

| month | clim bias / MAE (MHz) | irtam MAE | day-to-day (irtam) | calls, clim → both maps |
| --- | --- | --- | --- | --- |
| 2015-03 | -0.26 / 0.71 | 0.34 | +0.794 | 92.7% → 96.0% |
| 2019-06 | +0.38 / 0.49 | 0.28 | +0.493 | 89.9% → 93.2% |
| 2019-12 | +0.07 / 0.54 | 0.30 | +0.572 | 88.2% → 93.4% |
| 2022-09 | +0.53 / 0.69 | 0.36 | +0.730 | 91.5% → 95.0% |
| 2024-12 | +0.86 / 0.96 | 0.39 | +0.757 | 92.0% → 95.6% |
| 2025-03 | +0.34 / 0.70 | 0.40 | +0.795 | 93.4% → 95.8% |
| 2025-06 | +0.74 / 0.91 | 0.36 | +0.745 | 89.8% → 94.2% |
| 2025-07 | +0.59 / 0.75 | 0.32 | +0.751 | 91.3% → 94.7% |

The climatology day-to-day guard printed +0.000 in every table of every
month. Findings, in order of consequence:

1. **The WSPR ruler was the limit, as suspected.** The same IRTAM input
   that scored about +0.1 day-to-day against WSPR medians scores +0.49
   to +0.80 against ionosonde truth, in every month, weakest at solar
   minimum where daily variance is smallest. The assimilated map does
   know what the ionosphere did that day; WSPR could not see it.
   (One caveat below.)
2. **Climatology's foF2 bias moves month by month** — from -0.26 to
   +0.86 MHz across the eight months, largest near the cycle 25
   maximum, and larger still on storm days (2025-06: +1.22 MHz). A
   fixed monthly map cannot remove a bias that moves; a per-day
   effective index and a storm mode can, and they are the next phase.
3. **The +61 km height bias decomposes** (2025-06). Dudeney's corrected
   form over climatology's own inputs removes about 19 km; the rest is
   the M(3000)F2 input itself. IRTAM's height map removes nearly all
   of it (+3.5 km bias, MAE 14.9 km). Honest heights need both the
   corrected form and a better height input.
4. **The height matters exactly where geometry says.** At range zero
   the height models are indistinguishable (the secant is 1); at
   600 km, correct daily foF2 over the too-high climatology height
   under-calls the band, and the assimilated height restores it.
   With both maps the band calls sit between 93% and 96% at every
   range in every month.

## Caveats

- **IRTAM assimilates these same stations.** Its columns are mechanism
  proofs and upper bounds, not deployed-skill claims. Deployable skill
  needs the leave-one-station-out effective-index fit, which is the next
  phase's work.
- MUFD came back empty from FastChar for every station; the MUF column
  waits for a DIDBGetValues fetch. The secant-derived NVIS MUF stands in
  meanwhile and is conversion-free at range zero.
- foE is day-side only (a night ionogram has no scalable E trace), and
  the overlay does not touch it — its irtam column matching climatology
  is expected, not a fault.

## The decision this supports

Build the daily conditioning. The ionosonde ruler shows real, large
day-level structure that the engine's monthly input misses and that an
assimilated daily input captures; the open question is no longer
"is there signal" but "how much survives honest holdout" — which is
exactly what the effective-index and storm-mode phases measure next.
