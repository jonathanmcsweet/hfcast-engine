# How accurate is VOACAP: measured over eight months of real radio

Three configurations are measured here: VOACAP as standard practice
runs it, VOACAP with its sporadic-E term switched on, and VOACAP with
a calibration applied to its output. ITU-R P.533 appears once, in the
decision at the end. The numbers come from the Fortran reference,
which HFcast Compatible reproduces exactly, so they describe
that engine too.

HFcast Truecast is not measured on this page. It is scored against
ionosonde soundings in [comparison.md](comparison.md) and
[ionosonde.md](ionosonde.md), because the WSPR ruler used here is too
noisy to show the day-level skill that Truecast is built for.

Every claim here is measured against WSPR reception reports: 150 paths
per month, over eight months chosen to cover the extremes of season and
solar cycle. The method fits one offset per path, since antennas and
local receiver noise are unknown but constant, so what is scored is the
**daily shape** of each circuit — when it opens, peaks and closes —
rather than its absolute level. The flat baseline predicts every hour
as that path's own measured median. It contains no physics and needs a
month of data for the exact path, which makes it a reference rather
than a competitor anything could ship.

| month   | season                   | smoothed sunspot number |
| ------- | ------------------------ | ----------------------: |
| 2025-06 | summer                   |                   124.7 |
| 2025-07 | summer                   |                   122.5 |
| 2025-03 | equinox                  |                   135.9 |
| 2024-12 | winter                   |                   151.2 |
| 2019-06 | summer, solar minimum    |                     3.7 |
| 2019-12 | winter, solar minimum    |                     1.8 |
| 2022-09 | equinox, rising cycle    |                    96.5 |
| 2015-03 | equinox, declining cycle |                    82.1 |

Slope is from fitting `observed = a + b × predicted` per path: 1.0 means the
model predicts the right amount of daily variation, 0.2 means reality swings a
fifth as much as predicted.

## Finding 1: as configured today, accuracy collapses at solar minimum

Standard practice runs VOACAP with its sporadic-E layer disabled, because that
part of the model is considered unreliable. With that configuration:

| month   | VOACAP error | VOACAP correlation | VOACAP slope |
| ------- | -----------: | -----------------: | -----------: |
| 2025-06 |       4.0 dB |              +0.76 |         0.22 |
| 2025-07 |       4.0 dB |              +0.73 |         0.25 |
| 2025-03 |       3.5 dB |              +0.55 |         0.20 |
| 2024-12 |       3.5 dB |              +0.67 |         0.20 |
| 2019-06 |       5.0 dB |              +0.28 |     **0.04** |
| 2019-12 |       7.0 dB |              +0.54 |     **0.09** |

At high solar activity the exaggeration is a stable factor of four to five. At
solar minimum the predictions become close to uncorrelated with reality: the
model predicts enormous swings on circuits that barely moved.

## Finding 2: the disabled sporadic-E layer was a large part of the problem

Turning VOACAP's sporadic-E term on (the `FPROB` card's fourth value):

| month   | error off → on   | slope off → on |
| ------- | ---------------- | -------------- |
| 2025-06 | 4.0 → 3.0 dB     | 0.22 → 0.31    |
| 2024-12 | 3.5 → 3.0 dB     | 0.20 → 0.33    |
| 2019-06 | 5.0 → **2.0** dB | 0.04 → 0.34    |
| 2019-12 | 7.0 → **3.0** dB | 0.09 → 0.27    |

Better in every regime, dramatically so at solar minimum, and — the important
part — the slope becomes one number across all six months (0.20–0.36) instead
of collapsing. Sporadic-E is what keeps real circuits alive when the main-layer
forecast says they should be dead, and the standard advice to disable it costs
real accuracy against measured radio. The remaining factor of ~3 is still
unexplained; the leading suspect is that the models follow one propagation
route per hour while reality sums several.

An earlier experiment had already ruled out the two measurement-side
explanations, and its tables are below: the effect is stronger on paths
that never approach the WSPR decoder's floor, and holding the noise
models out barely changes the slope.

### The single-month experiment behind that

2025-06, smoothed sunspot number 124.7, 150 paths of 150 fetched.
Errors are in dB. Correlation and slope come from fitting
`observed = a + b * predicted` per path: correlation says whether the
peaks and troughs land in the right places, slope says whether the
model swings by the right amount, and the last column is what is left
once both are fitted.

| predictor           | path-hours | median error | RMS error | correlation | slope | error after gain fit |
| ------------------- | ---------: | -----------: | --------: | ----------: | ----: | -------------------: |
| VOACAP              |       3477 |          4.0 |      14.5 |       +0.76 | +0.22 |                  1.5 |
| ITU-R P.533         |       3477 |          3.3 |       5.9 |       +0.59 | +0.32 |                  2.0 |
| VOACAP, signal only |       3477 |          4.0 |      15.1 |       +0.77 | +0.20 |                  1.4 |
| P.533, signal only  |       3477 |          3.1 |       5.9 |       +0.71 | +0.39 |                  1.6 |
| flat baseline       |       3477 |          2.5 |       4.4 |         n/a |   n/a |                  n/a |

The signal-only rows score each model's predicted received signal with
its noise prediction left out, as if the receiver's noise were constant
through the day. The models' noise swings strongly between day and
night, while a typical WSPR receiver's noise is set by local
interference and barely moves. The gap between a model's row and its
signal-only row is the part of the exaggerated swing that comes from
the noise half of the prediction.

Restricted to the 27 paths whose weakest hour stays above -15 dB, so
the decoder's floor cannot be flattening the measured daily swing:

| predictor           | path-hours | median error | RMS error | correlation | slope | error after gain fit |
| ------------------- | ---------: | -----------: | --------: | ----------: | ----: | -------------------: |
| VOACAP              |        630 |          5.0 |      13.2 |       +0.73 | +0.16 |                  1.3 |
| ITU-R P.533         |        630 |          3.4 |       5.7 |       +0.58 | +0.24 |                  1.6 |
| VOACAP, signal only |        630 |          5.0 |      13.6 |       +0.66 | +0.16 |                  1.2 |
| P.533, signal only  |        630 |          3.5 |       5.8 |       +0.63 | +0.25 |                  1.5 |
| flat baseline       |        630 |          2.0 |       3.7 |         n/a |   n/a |                  n/a |

No path had fewer than 8 usable hours after dropping observations
within -4 dB of the decoder's floor, and none failed to run. Per-path
detail is written to `data/validation-per-path.csv`.

## Finding 3: one calibration factor survives out of sample

The calibration is `calibrated = centre + k × (predicted − centre)`, where
`centre` is the prediction's own daily median, everything known at prediction
time. Fitted per month on the sporadic-E-on configuration, `k` is stable:

| fitted on       | 2025-06 | 2025-07 | 2025-03 | 2024-12 | 2019-06 | 2019-12 |
| --------------- | ------: | ------: | ------: | ------: | ------: | ------: |
| VOACAP global k |   0.248 |   0.286 |   0.212 |   0.234 |   0.396 |   0.242 |

Fitting on June 2025 alone (k = 0.248) and applying that single number,
unchanged, to the five months it never saw:

| tested on | raw VOACAP | calibrated | flat baseline |
| --------- | ---------: | --------: | ------------: |
| 2025-07   |       3.00 |  **1.88** |          3.00 |
| 2025-03   |       3.50 |  **1.97** |          2.00 |
| 2024-12   |       3.00 |  **2.50** |          3.00 |
| 2019-06   |       2.00 |  **1.86** |          2.00 |
| 2019-12   |       3.00 |  **2.00** |          2.50 |
| 2022-09   |       3.50 |  **1.74** |          2.00 |
| 2015-03   |       3.75 |      2.63 |          2.50 |

Median absolute error in dB. Calibrated VOACAP beats or matches the
with-hindsight baseline on six of the seven unseen months, across ten years
and the full solar cycle; the one near-miss is 2015-03, 0.13 dB behind, and
equinox months are consistently the weakest. Per-band factors improve this by
only ~0.1 dB and wobble between months, so the global factor is the one to
ship.

## Finding 4: the day-to-day spread is overstated too

Full write-up in [reliability.md](reliability.md). The app's reliability
number — the chance a band works on a given day — rests on the engine's
day-to-day spread claims, and those were the last unvalidated input. Checked
against per-day WSPR records, offset-free (deviations from each path-hour's
own median, which no unknown antenna can shift):

- The engine claims 25–30% of days fall 6 dB or more below an hour's median.
  Measured: 5–10%.
- Scaling the deciles — lower × 0.40, upper × 0.59, fitted on June 2025 —
  reproduces the measured frequencies in the 3–10 dB range on five test
  months spanning 2015–2025. Per-month fits across all eight months stay
  within 0.30–0.45 and 0.43–0.59, so the factors are stable.
- Beyond 10 dB the scaled model under-predicts: rare bad days (magnetic
  storms) are worse than a bell curve allows. Reliability shown near 100%
  should be read as "9 in 10", not certainty.

## Finding 5: the missing bad days are storm days, and Kp finds them

Full write-up in [reliability.md](reliability.md). This rule widens the *spread*
around a prediction, and the server applies it to finished numbers. It
is a separate thing from HFcast Truecast's storm table, which shifts
foF2 inside the engine before a prediction is made; the two work at
different layers and neither replaces the other. Tagging every measured
day-hour with the highest Kp of its preceding 24 hours (GFZ Potsdam
record):

- With no recent storm, the calibrated spread is confirmed on data it was
  not fitted on (widening needed: 1.0–1.2).
- After a storm the downward spread must widen with storm strength — about
  1.4 times for Kp 5–6, 2 for Kp 6–7, 2.5 for Kp 7+ — and the same
  staircase appears independently in the fit month and in the seven test
  months. The upward side never changes: storms only suppress.
- The graded rule `1 + 0.5 × (Kp24 − 4.75)`, capped at 2.5, brings
  predicted frequencies into approximate agreement with measured ones in
  the decision-relevant 6–10 dB range. The server applies it only to
  requests that know the current Kp.

## The two models against each other, with no measured radio

Everything above scores a model against real reception reports. This
section does something different: it runs VOACAP and ITU-R P.533 over
the same 96 sweep cases and reports where they disagree. Neither is the
truth here, and nothing in this section says which is more accurate.
VOACAP means HFcast Compatible, which reproduces the Fortran exactly.

All 96 of 96 cases ran on both. Differences below are P.533 minus
VOACAP.

| quantity        |    n |  mean | median | 5th pct | 95th pct | max abs | unit |
| --------------- | ---: | ----: | -----: | ------: | -------: | ------: | ---- |
| Basic MUF       | 2304 | -2.69 |  -2.61 |   -6.63 |    +0.52 |   11.71 | MHz  |
| Operational MUF | 2304 | +1.47 |  +1.31 |   -2.11 |    +5.91 |   12.15 | MHz  |

As a check that both ran the same circuit: 2304 hours, mean path length
7370.4 km. Both compute this from the same great-circle geometry, so any
disagreement there would mean the two runs were not the same path.

The difference that matters most is not a decibel figure. Of 20,736
hour and frequency combinations, P.533 found no propagating mode at all
in 12,996 of them (62.7%), while VOACAP named a mode in every one. The
two disagree about how often a band is usable at all, which matters
more to somebody deciding whether to call than any difference in signal
strength.

Signal power can only be compared roughly. Both were run with isotropic
antennas and the same transmit power, but they do not define their
signal reference points in the same way, so treat this as an indication
rather than a measurement. 4974 pairs were left out because at least
one of the two printed a dead-path value below -250 dBW.

| quantity     |     n |  mean | median | 5th pct | 95th pct | max abs | unit |
| ------------ | ----: | ----: | -----: | ------: | -------: | ------: | ---- |
| Signal power | 15762 | +9.45 |  +5.23 |  -17.81 |   +53.33 |  122.30 | dB   |

Two quantities cannot be compared at all:

- **Propagation mode.** The two use different vocabularies. VOACAP
  labels the mode mix (`F2F2`, `EF2`, `F2 E`), while P.533 names one
  dominant mode with a hop count (`1F2`, `2E`) or `NONE`. Matching the
  labels would measure nothing.
- **Signal-to-noise ratio and reliability.** P.533 takes man-made noise
  as a named environment over a stated bandwidth; VOACAP takes a number
  at 3 MHz. There is no exact way to convert between them, so any
  difference would mix the models' own disagreement with the error of
  converting the input.

## The decision this supports

**VOACAP, with its sporadic-E term enabled, its daily swing shrunk by
k = 0.25, and its spread deciles scaled by 0.40 (lower) and 0.59 (upper).**
Calibrated P.533 loses to calibrated VOACAP on every test month (2.08–3.09 dB
against 1.74–2.63 dB). The full signal-to-noise prediction and the
signal-only variant perform identically, so the server keeps reading the
field it already reads.

## What this does not establish

- **Absolute level.** The per-path offset removes it; predicting it needs
  known antennas.
- **Storm-day tails on predictions that cannot know the Kp.** A forecast
  for a future day has no way to know that day's Kp, so it keeps the
  quiet-day calibration, and the small share of days that turn out
  stormy will be worse than it says. Where the current Kp is known the
  storm widening covers this, except the very deepest Kp 7+ fades,
  which exceed any bell curve.
- **Geography.** WSPR receivers cluster in North America and Europe.
- **The remaining factor of ~3.** Sporadic-E closed most of the gap at solar
  minimum, but reality still swings only about a third of what
  calibrated VOACAP predicts. Multi-route filling is the open suspect.

Reproduce with `tools/fetch-wspr.sh`, `validate --es --dump`, and
`tools/calibration-matrix.sh`. Full outputs are in `docs/calibration-matrix.md`
(standard configuration) and `docs/calibration-matrix-es.md` (sporadic-E on).
