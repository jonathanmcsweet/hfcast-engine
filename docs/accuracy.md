# How accurate are the engines — measured, eight months of real radio

Every claim here is measured against WSPR reception reports: 150 paths per
month, eight months chosen to cover the extremes of season and solar cycle. The method fits one offset per path (antennas and local receiver noise
are unknown but constant), so what is scored is the **daily shape** of each
circuit — when it opens, peaks and closes — not absolute level. The flat
baseline predicts every hour as that path's own measured median; it contains no
physics and needs a month of data for the exact path, so it is a reference, not
a shippable competitor.

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

## Finding 1 — as configured today, accuracy collapses at solar minimum

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
solar minimum the predictions become close to uncorrelated with reality — the
model predicts enormous swings on circuits that barely moved.

## Finding 2 — the disabled sporadic-E layer was a large part of the problem

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

An earlier experiment (`docs/validation.md`) had already ruled out the two
measurement-side explanations: the effect is stronger on paths that never
approach the WSPR decoder's floor, and holding the engines' noise models out
barely changes the slope.

## Finding 3 — one correction factor survives out of sample

The correction is `corrected = centre + k × (predicted − centre)`, where
`centre` is the prediction's own daily median — everything known at prediction
time. Fitted per month on the sporadic-E-on configuration, `k` is stable:

| fitted on       | 2025-06 | 2025-07 | 2025-03 | 2024-12 | 2019-06 | 2019-12 |
| --------------- | ------: | ------: | ------: | ------: | ------: | ------: |
| VOACAP global k |   0.248 |   0.286 |   0.212 |   0.234 |   0.396 |   0.242 |

Fitting on June 2025 alone (k = 0.248) and applying that single number,
unchanged, to the five months it never saw:

| tested on | raw VOACAP | corrected | flat baseline |
| --------- | ---------: | --------: | ------------: |
| 2025-07   |       3.00 |  **1.88** |          3.00 |
| 2025-03   |       3.50 |  **1.97** |          2.00 |
| 2024-12   |       3.00 |  **2.50** |          3.00 |
| 2019-06   |       2.00 |  **1.86** |          2.00 |
| 2019-12   |       3.00 |  **2.00** |          2.50 |
| 2022-09   |       3.50 |  **1.74** |          2.00 |
| 2015-03   |       3.75 |      2.63 |          2.50 |

Median absolute error in dB. The corrected model beats or matches the
with-hindsight baseline on six of the seven unseen months, across ten years
and the full solar cycle; the one near-miss is 2015-03, 0.13 dB behind, and
equinox months are consistently the weakest. Per-band factors improve this by
only ~0.1 dB and wobble between months, so the global factor is the one to
ship.

## Finding 4 — the day-to-day spread is overstated too

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

## The decision this supports

**VOACAP, with its sporadic-E term enabled, its daily swing shrunk by
k = 0.25, and its spread deciles scaled by 0.40 (lower) and 0.59 (upper).**
Corrected P.533 loses to corrected VOACAP on every test month (2.08–3.09 dB
against 1.74–2.63 dB). The full signal-to-noise prediction and the
signal-only variant perform identically, so the server keeps reading the
field it already reads.

## What this does not establish

- **Absolute level.** The per-path offset removes it; predicting it needs
  known antennas.
- **Storm-day tails.** Deviations beyond 10 dB happen more often than the
  scaled spread predicts; the reliability number is honest in the common
  range and optimistic about rare deep fades.
- **Geography.** WSPR receivers cluster in North America and Europe.
- **The remaining factor of ~3.** Sporadic-E closed most of the gap at solar
  minimum, but reality still swings about a third of the corrected-model
  prediction basis. Multi-route filling is the open suspect.

Reproduce with `tools/fetch-wspr.sh`, `validate --es --dump`, and
`tools/calibration-matrix.sh` — full outputs in `docs/calibration-matrix.md`
(standard configuration) and `docs/calibration-matrix-es.md` (sporadic-E on).
