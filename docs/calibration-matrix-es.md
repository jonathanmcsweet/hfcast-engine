# Cross-month calibration matrix

# Amplitude correction fitted on 2025-06/hours-es

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                              |
| ------------------- | -------: | --------------------------------------- |
| VOACAP              |    0.248 | 7 MHz: 0.29, 10 MHz: 0.34, 14 MHz: 0.21 |
| ITU-R P.533         |    0.331 | 7 MHz: 0.52, 10 MHz: 0.34, 14 MHz: 0.23 |
| VOACAP, signal only |    0.236 | 7 MHz: 0.26, 10 MHz: 0.33, 14 MHz: 0.21 |
| P.533, signal only  |    0.353 | 7 MHz: 0.51, 10 MHz: 0.39, 14 MHz: 0.23 |

## Tested on 2025-07/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.88 |       1.77 |
| ITU-R P.533                                | 3.35 |     2.31 |       2.13 |
| VOACAP, signal only                        | 3.00 |     1.85 |       1.80 |
| P.533, signal only                         | 3.10 |     2.01 |       1.81 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     1.97 |       1.84 |
| ITU-R P.533                                | 3.08 |     2.08 |       1.97 |
| VOACAP, signal only                        | 3.50 |     1.84 |       1.79 |
| P.533, signal only                         | 2.97 |     1.94 |       1.80 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.50 |       2.38 |
| ITU-R P.533                                | 3.85 |     2.36 |       2.41 |
| VOACAP, signal only                        | 3.50 |     2.47 |       2.42 |
| P.533, signal only                         | 3.82 |     2.29 |       2.29 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 2.00 |     1.86 |       1.81 |
| ITU-R P.533                                | 5.05 |     2.82 |       2.86 |
| VOACAP, signal only                        | 2.25 |     1.82 |       1.80 |
| P.533, signal only                         | 4.01 |     2.66 |       2.62 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.00 |       2.03 |
| ITU-R P.533                                | 7.69 |     3.09 |       3.83 |
| VOACAP, signal only                        | 3.00 |     2.00 |       2.06 |
| P.533, signal only                         | 7.26 |     3.00 |       3.61 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2025-07/hours-es

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                              |
| ------------------- | -------: | --------------------------------------- |
| VOACAP              |    0.286 | 7 MHz: 0.33, 10 MHz: 0.41, 14 MHz: 0.21 |
| ITU-R P.533         |    0.363 | 7 MHz: 0.58, 10 MHz: 0.34, 14 MHz: 0.22 |
| VOACAP, signal only |    0.270 | 7 MHz: 0.29, 10 MHz: 0.39, 14 MHz: 0.22 |
| P.533, signal only  |    0.372 | 7 MHz: 0.54, 10 MHz: 0.38, 14 MHz: 0.21 |

## Tested on 2025-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.65 |       1.61 |
| ITU-R P.533                                | 3.26 |     2.13 |       1.95 |
| VOACAP, signal only                        | 3.00 |     1.61 |       1.56 |
| P.533, signal only                         | 3.12 |     1.85 |       1.71 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     1.93 |       1.88 |
| ITU-R P.533                                | 3.08 |     2.10 |       1.96 |
| VOACAP, signal only                        | 3.50 |     1.83 |       1.81 |
| P.533, signal only                         | 2.97 |     1.96 |       1.82 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.43 |       2.39 |
| ITU-R P.533                                | 3.85 |     2.40 |       2.41 |
| VOACAP, signal only                        | 3.50 |     2.45 |       2.40 |
| P.533, signal only                         | 3.82 |     2.29 |       2.29 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 2.00 |     1.84 |       1.81 |
| ITU-R P.533                                | 5.05 |     2.94 |       2.91 |
| VOACAP, signal only                        | 2.25 |     1.80 |       1.80 |
| P.533, signal only                         | 4.01 |     2.69 |       2.58 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.00 |       2.05 |
| ITU-R P.533                                | 7.69 |     3.32 |       4.02 |
| VOACAP, signal only                        | 3.00 |     2.03 |       2.09 |
| P.533, signal only                         | 7.26 |     3.11 |       3.70 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2025-03/hours-es

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                                            |
| ------------------- | -------: | ----------------------------------------------------- |
| VOACAP              |    0.212 | 7 MHz: 0.39, 10 MHz: 0.32, 14 MHz: 0.12, 18 MHz: 0.13 |
| ITU-R P.533         |    0.292 | 7 MHz: 0.56, 10 MHz: 0.32, 14 MHz: 0.13, 18 MHz: 0.15 |
| VOACAP, signal only |    0.211 | 7 MHz: 0.37, 10 MHz: 0.31, 14 MHz: 0.11, 18 MHz: 0.13 |
| P.533, signal only  |    0.301 | 7 MHz: 0.49, 10 MHz: 0.33, 14 MHz: 0.14, 18 MHz: 0.14 |

## Tested on 2025-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.79 |       1.62 |
| ITU-R P.533                                | 3.26 |     2.17 |       1.96 |
| VOACAP, signal only                        | 3.00 |     1.68 |       1.61 |
| P.533, signal only                         | 3.12 |     1.88 |       1.74 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.97 |       1.81 |
| ITU-R P.533                                | 3.35 |     2.33 |       2.07 |
| VOACAP, signal only                        | 3.00 |     1.92 |       1.77 |
| P.533, signal only                         | 3.10 |     2.11 |       1.81 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.50 |       2.39 |
| ITU-R P.533                                | 3.85 |     2.32 |       2.54 |
| VOACAP, signal only                        | 3.50 |     2.48 |       2.39 |
| P.533, signal only                         | 3.82 |     2.27 |       2.43 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 2.00 |     1.85 |       1.74 |
| ITU-R P.533                                | 5.05 |     2.66 |       2.72 |
| VOACAP, signal only                        | 2.25 |     1.83 |       1.71 |
| P.533, signal only                         | 4.01 |     2.54 |       2.41 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.06 |       2.00 |
| ITU-R P.533                                | 7.69 |     2.84 |       3.92 |
| VOACAP, signal only                        | 3.00 |     2.06 |       2.00 |
| P.533, signal only                         | 7.26 |     2.70 |       3.45 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2024-12/hours-es

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                                           |
| ------------------- | -------: | ---------------------------------------------------- |
| VOACAP              |    0.234 | 3 MHz: 0.52, 7 MHz: 0.41, 10 MHz: 0.33, 14 MHz: 0.19 |
| ITU-R P.533         |    0.297 | 3 MHz: 0.45, 7 MHz: 0.34, 10 MHz: 0.27, 14 MHz: 0.25 |
| VOACAP, signal only |    0.233 | 3 MHz: 0.49, 7 MHz: 0.39, 10 MHz: 0.32, 14 MHz: 0.19 |
| P.533, signal only  |    0.289 | 3 MHz: 0.40, 7 MHz: 0.32, 10 MHz: 0.29, 14 MHz: 0.24 |

## Tested on 2025-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.72 |       1.58 |
| ITU-R P.533                                | 3.26 |     2.17 |       2.10 |
| VOACAP, signal only                        | 3.00 |     1.65 |       1.56 |
| P.533, signal only                         | 3.12 |     1.90 |       1.84 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.90 |       1.75 |
| ITU-R P.533                                | 3.35 |     2.32 |       2.29 |
| VOACAP, signal only                        | 3.00 |     1.87 |       1.73 |
| P.533, signal only                         | 3.10 |     2.12 |       2.05 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     1.91 |       1.81 |
| ITU-R P.533                                | 3.08 |     2.06 |       2.01 |
| VOACAP, signal only                        | 3.50 |     1.85 |       1.71 |
| P.533, signal only                         | 2.97 |     1.92 |       1.85 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 2.00 |     1.83 |       1.75 |
| ITU-R P.533                                | 5.05 |     2.67 |       2.59 |
| VOACAP, signal only                        | 2.25 |     1.82 |       1.71 |
| P.533, signal only                         | 4.01 |     2.48 |       2.45 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.01 |       2.00 |
| ITU-R P.533                                | 7.69 |     2.87 |       2.98 |
| VOACAP, signal only                        | 3.00 |     2.02 |       2.00 |
| P.533, signal only                         | 7.26 |     2.63 |       2.73 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2019-06/hours-es

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                              |
| ------------------- | -------: | --------------------------------------- |
| VOACAP              |    0.396 | 7 MHz: 0.49, 10 MHz: 0.12, 14 MHz: 0.37 |
| ITU-R P.533         |    0.087 | 7 MHz: 0.12, 10 MHz: 0.04, 14 MHz: 0.12 |
| VOACAP, signal only |    0.380 | 7 MHz: 0.47, 10 MHz: 0.07, 14 MHz: 0.33 |
| P.533, signal only  |    0.117 | 7 MHz: 0.22, 10 MHz: 0.03, 14 MHz: 0.11 |

## Tested on 2025-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.68 |       1.88 |
| ITU-R P.533                                | 3.26 |     2.36 |       2.36 |
| VOACAP, signal only                        | 3.00 |     1.64 |       1.89 |
| P.533, signal only                         | 3.12 |     2.20 |       2.12 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.77 |       1.90 |
| ITU-R P.533                                | 3.35 |     2.51 |       2.51 |
| VOACAP, signal only                        | 3.00 |     1.76 |       1.90 |
| P.533, signal only                         | 3.10 |     2.36 |       2.31 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     2.06 |       2.00 |
| ITU-R P.533                                | 3.08 |     1.96 |       1.97 |
| VOACAP, signal only                        | 3.50 |     1.93 |       1.98 |
| P.533, signal only                         | 2.97 |     1.93 |       1.92 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.40 |       2.53 |
| ITU-R P.533                                | 3.85 |     2.74 |       2.70 |
| VOACAP, signal only                        | 3.50 |     2.40 |       2.53 |
| P.533, signal only                         | 3.82 |     2.60 |       2.59 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.11 |       2.06 |
| ITU-R P.533                                | 7.69 |     2.11 |       2.14 |
| VOACAP, signal only                        | 3.00 |     2.08 |       2.04 |
| P.533, signal only                         | 7.26 |     2.07 |       2.25 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2019-12/hours-es

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                                           |
| ------------------- | -------: | ---------------------------------------------------- |
| VOACAP              |    0.242 | 3 MHz: 0.69, 7 MHz: 0.35, 10 MHz: 0.18, 14 MHz: 0.14 |
| ITU-R P.533         |    0.120 | 3 MHz: 0.32, 7 MHz: 0.12, 10 MHz: 0.09, 14 MHz: 0.16 |
| VOACAP, signal only |    0.249 | 3 MHz: 0.78, 7 MHz: 0.36, 10 MHz: 0.18, 14 MHz: 0.15 |
| P.533, signal only  |    0.134 | 3 MHz: 0.31, 7 MHz: 0.14, 10 MHz: 0.09, 14 MHz: 0.15 |

## Tested on 2025-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.71 |       1.64 |
| ITU-R P.533                                | 3.26 |     2.31 |       2.29 |
| VOACAP, signal only                        | 3.00 |     1.63 |       1.63 |
| P.533, signal only                         | 3.12 |     2.18 |       2.18 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     1.88 |       1.85 |
| ITU-R P.533                                | 3.35 |     2.44 |       2.46 |
| VOACAP, signal only                        | 3.00 |     1.76 |       1.78 |
| P.533, signal only                         | 3.10 |     2.34 |       2.38 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     1.93 |       1.83 |
| ITU-R P.533                                | 3.08 |     1.98 |       1.98 |
| VOACAP, signal only                        | 3.50 |     1.87 |       1.75 |
| P.533, signal only                         | 2.97 |     1.92 |       1.93 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.00 |     2.52 |       2.46 |
| ITU-R P.533                                | 3.85 |     2.65 |       2.58 |
| VOACAP, signal only                        | 3.50 |     2.50 |       2.55 |
| P.533, signal only                         | 3.82 |     2.55 |       2.55 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours-es

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 2.00 |     1.83 |       1.78 |
| ITU-R P.533                                | 5.05 |     2.03 |       2.01 |
| VOACAP, signal only                        | 2.25 |     1.75 |       1.72 |
| P.533, signal only                         | 4.01 |     2.03 |       2.00 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.
