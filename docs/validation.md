# Both engines against measured WSPR reports

2025-06, smoothed sunspot number 124.7. 150 paths used of 150 fetched.

Each path is a fixed pair of stations on a fixed band, so its antennas and its local noise are unknown but constant. One offset per path is fitted and removed, which is why this measures how well a model tracks the **daily shape** of a circuit rather than its absolute level.

The flat baseline predicts every hour as that path's own median. It contains no physics. An engine that does not beat it is adding nothing.

## All paths

| predictor     | path-hours | median error | RMS error | correlation | slope | error after gain fit |
| ------------- | ---------: | -----------: | --------: | ----------: | ----: | -------------------: |
| VOACAP        |       3481 |          4.0 |      14.9 |       +0.76 | +0.22 |                  1.5 |
| ITU-R P.533   |       3481 |          3.3 |       6.0 |       +0.58 | +0.32 |                  2.0 |
| flat baseline |       3481 |          2.5 |       4.4 |           — |     — |                    — |

Errors are in dB. Correlation and slope come from fitting `observed = a + b * predicted` per path: correlation says whether the peaks and troughs land in the right places, slope says whether the model swings by the right amount, and the last column is what is left once both are fitted.

## Paths that never approach the decoder's floor

27 paths whose weakest hour stays above -15 dB. WSPR cannot report what it fails to decode, so weak hours read higher than they were or vanish, which flattens the measured daily swing. On these paths that cannot be happening, so anything that survives here is the models rather than the measurement.

| predictor     | path-hours | median error | RMS error | correlation | slope | error after gain fit |
| ------------- | ---------: | -----------: | --------: | ----------: | ----: | -------------------: |
| VOACAP        |        630 |          5.0 |      13.2 |       +0.73 | +0.16 |                  1.3 |
| ITU-R P.533   |        630 |          3.4 |       5.7 |       +0.58 | +0.24 |                  1.6 |
| flat baseline |        630 |          2.0 |       3.7 |           — |     — |                    — |

## What was left out

- 0 paths had fewer than 8 usable hours after dropping observations within -4 dB of the decoder's floor.
- 0 paths failed to run.

Per-path detail written to `data/validation-per-path.csv`.
