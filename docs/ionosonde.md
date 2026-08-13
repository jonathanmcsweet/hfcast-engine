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
- **essn** (frequency rows only) — one effective sunspot number fitted
  per day from the day's GIRO foF2 readings, always leaving the scored
  station out (`src/essn.rs`). Predicted foF2 is exactly linear in the
  sunspot number, so the fit is a median of closed-form per-sample
  solutions.
- **essn+storm** (frequency rows only) — the essn prediction times the
  embedded storm ratio (`src/stormfit.rs`): median observed-over-essn
  ratios binned by trailing-24-hour Kp class, geomagnetic latitude
  band, season and local-time quarter, fitted on the six fit months
  (2019-06, 2019-12, 2024-12, 2025-03, 2025-06, 2025-07) and scored on
  the two held-out storm months (2015-03, 2022-09). Quiet bins and
  low-latitude bins are the identity by construction.

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

foF2, model minus observed. `essn` is the leave-one-station-out fitted
daily index — the deployable column; `irtam` is the assimilated-map
upper bound.

| month | clim bias / MAE | essn bias / MAE | irtam MAE | day corr: essn / irtam | calls @600, clim → both maps |
| --- | --- | --- | --- | --- | --- |
| 2015-03 | -0.26 / 0.71 | +0.00 / 0.58 | 0.34 | +0.390 / +0.794 | 92.7% → 96.0% |
| 2019-06 | +0.38 / 0.49 | +0.02 / 0.40 | 0.28 | +0.147 / +0.493 | 89.9% → 93.2% |
| 2019-12 | +0.07 / 0.54 | -0.01 / 0.55 | 0.30 | +0.108 / +0.572 | 88.2% → 93.4% |
| 2022-09 | +0.53 / 0.69 | +0.00 / 0.51 | 0.36 | +0.462 / +0.730 | 91.5% → 95.0% |
| 2024-12 | +0.86 / 0.96 | +0.01 / 0.62 | 0.39 | +0.428 / +0.757 | 92.0% → 95.6% |
| 2025-03 | +0.34 / 0.70 | +0.00 / 0.62 | 0.40 | +0.404 / +0.795 | 93.4% → 95.8% |
| 2025-06 | +0.74 / 0.91 | -0.01 / 0.57 | 0.36 | +0.164 / +0.745 | 89.8% → 94.2% |
| 2025-07 | +0.59 / 0.75 | -0.04 / 0.51 | 0.32 | +0.287 / +0.751 | 91.3% → 94.7% |

The climatology day-to-day guard printed +0.000 in every table of every
month. Findings, in order of consequence:

1. **The WSPR ruler was the limit, as suspected.** The same IRTAM input
   that scored about +0.1 day-to-day against WSPR medians scores +0.49
   to +0.80 against ionosonde truth, in every month, weakest at solar
   minimum where daily variance is smallest. The assimilated map does
   know what the ionosphere did that day; WSPR could not see it.
   (One caveat below.)
2. **Climatology's foF2 bias moves month by month, and the fitted daily
   index removes it out of sample.** The bias runs -0.26 to +0.86 MHz
   across the eight months; the leave-one-station-out index brings it
   to zero in every month, improves MAE in seven of eight, and carries
   +0.11 to +0.46 day-to-day skill — strongest exactly where it
   matters, in the storm months. That skill uses no data from the
   scored station: it is what a deployed nowcast could really have.
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
5. **The storm table adds a little beyond essn, only at mid latitudes
   and mostly in severe storms.** foF2 on storm hours (trailing-24-hour
   Kp at or above 5), essn against essn+storm, MAE in MHz:

   | month | storm n | essn | essn+storm | day corr essn → both |
   | --- | ---: | ---: | ---: | --- |
   | 2015-03 (held out) | 3006 | 0.763 | 0.706 | +0.390 → +0.395 |
   | 2022-09 (held out) | 1798 | 0.602 | 0.600 | +0.462 → +0.452 |

   The fit months improve more (2025-06: 0.772 to 0.653) but they are
   in sample and prove nothing. The held-out verdict: a real gain in
   the severe month, nothing in the moderate one, quiet hours untouched
   by construction. A first fit that also learned low-latitude bins
   gained in sample and reversed on 2015-03 (RMS 1.32 to 1.65, day
   correlation +0.390 to +0.245): the equatorial storm response turns
   on penetration-field timing a Kp class cannot carry, so `fit` holds
   the low band at the identity permanently — that exclusion is itself
   a measured result.

   In the NVIS table the full deployable pipeline (`essn+st+dud`)
   holds the essn+dudeney level: MUF MAE ticks down about 0.01 MHz in
   the held-out months and the band-call rates do not move at month
   scale, because storm hours are a minority of cells. The storm
   table's value lives in the storm-hour splits above, not in monthly
   call totals.

## Caveats

- **IRTAM assimilates these same stations.** Its columns are mechanism
  proofs and upper bounds, not deployed-skill claims. The deployable
  number is the essn column, which leaves the scored station out of its
  own fit.
- MUFD came back empty from FastChar for every station; the MUF column
  waits for a DIDBGetValues fetch. The secant-derived NVIS MUF stands in
  meanwhile and is conversion-free at range zero.
- foE is day-side only (a night ionogram has no scalable E trace), and
  the overlay does not touch it — its irtam column matching climatology
  is expected, not a fault.

## The decision this supports

Build the daily conditioning into the new pipeline. The signal is real
(irtam), and enough of it survives honest holdout (essn) to remove the
moving bias and add day-level skill from live soundings alone. The
storm table rides on top as a small, safe increment: identity on quiet
days and at low latitudes, a measured gain on severe storm days at mid
latitudes. The daily-modeling measurements are done; the conditioning
(essn, Kp, the storm table) now has a proven shape for the nowcast
pipeline to consume.
