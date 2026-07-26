# Cross-month calibration matrix

# Amplitude correction fitted on 2025-06/hours

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                              |
| ------------------- | -------: | --------------------------------------- |
| VOACAP              |    0.150 | 7 MHz: 0.27, 10 MHz: 0.26, 14 MHz: 0.08 |
| ITU-R P.533         |    0.332 | 7 MHz: 0.52, 10 MHz: 0.34, 14 MHz: 0.23 |
| VOACAP, signal only |    0.148 | 7 MHz: 0.24, 10 MHz: 0.26, 14 MHz: 0.08 |
| P.533, signal only  |    0.355 | 7 MHz: 0.51, 10 MHz: 0.39, 14 MHz: 0.23 |

## Tested on 2025-07/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.12 |       1.82 |
| ITU-R P.533                                | 3.34 |     2.31 |       2.13 |
| VOACAP, signal only                        | 4.00 |     2.08 |       1.82 |
| P.533, signal only                         | 3.09 |     2.01 |       1.81 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.77 |       1.58 |
| ITU-R P.533                                | 3.08 |     2.09 |       1.97 |
| VOACAP, signal only                        | 4.00 |     1.71 |       1.58 |
| P.533, signal only                         | 2.97 |     1.95 |       1.80 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     2.35 |       2.33 |
| ITU-R P.533                                | 3.85 |     2.36 |       2.40 |
| VOACAP, signal only                        | 3.50 |     2.34 |       2.30 |
| P.533, signal only                         | 3.80 |     2.29 |       2.29 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 5.00 |     2.50 |       2.33 |
| ITU-R P.533                                | 5.02 |     2.83 |       2.87 |
| VOACAP, signal only                        | 5.00 |     2.48 |       2.27 |
| P.533, signal only                         | 3.98 |     2.67 |       2.62 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 7.00 |     2.48 |       2.99 |
| ITU-R P.533                                | 7.54 |     3.11 |       3.82 |
| VOACAP, signal only                        | 7.50 |     2.45 |       2.82 |
| P.533, signal only                         | 7.09 |     2.98 |       3.61 |
| flat baseline (needs the month's own data) | 2.25 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2025-07/hours

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                              |
| ------------------- | -------: | --------------------------------------- |
| VOACAP              |    0.149 | 7 MHz: 0.29, 10 MHz: 0.25, 14 MHz: 0.07 |
| ITU-R P.533         |    0.363 | 7 MHz: 0.58, 10 MHz: 0.34, 14 MHz: 0.22 |
| VOACAP, signal only |    0.148 | 7 MHz: 0.26, 10 MHz: 0.24, 14 MHz: 0.07 |
| P.533, signal only  |    0.372 | 7 MHz: 0.54, 10 MHz: 0.38, 14 MHz: 0.21 |

## Tested on 2025-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.93 |       1.63 |
| ITU-R P.533                                | 3.26 |     2.14 |       1.95 |
| VOACAP, signal only                        | 4.00 |     1.85 |       1.61 |
| P.533, signal only                         | 3.12 |     1.86 |       1.71 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.78 |       1.57 |
| ITU-R P.533                                | 3.08 |     2.10 |       1.96 |
| VOACAP, signal only                        | 4.00 |     1.71 |       1.56 |
| P.533, signal only                         | 2.97 |     1.96 |       1.82 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     2.35 |       2.32 |
| ITU-R P.533                                | 3.85 |     2.39 |       2.40 |
| VOACAP, signal only                        | 3.50 |     2.34 |       2.30 |
| P.533, signal only                         | 3.80 |     2.29 |       2.29 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 5.00 |     2.51 |       2.29 |
| ITU-R P.533                                | 5.02 |     2.95 |       2.91 |
| VOACAP, signal only                        | 5.00 |     2.48 |       2.24 |
| P.533, signal only                         | 3.98 |     2.69 |       2.59 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 7.00 |     2.48 |       3.00 |
| ITU-R P.533                                | 7.54 |     3.29 |       4.00 |
| VOACAP, signal only                        | 7.50 |     2.43 |       2.84 |
| P.533, signal only                         | 7.09 |     3.06 |       3.69 |
| flat baseline (needs the month's own data) | 2.25 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2025-03/hours

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                                            |
| ------------------- | -------: | ----------------------------------------------------- |
| VOACAP              |    0.188 | 7 MHz: 0.31, 10 MHz: 0.31, 14 MHz: 0.09, 18 MHz: 0.13 |
| ITU-R P.533         |    0.292 | 7 MHz: 0.56, 10 MHz: 0.32, 14 MHz: 0.13, 18 MHz: 0.15 |
| VOACAP, signal only |    0.186 | 7 MHz: 0.29, 10 MHz: 0.29, 14 MHz: 0.09, 18 MHz: 0.13 |
| P.533, signal only  |    0.301 | 7 MHz: 0.49, 10 MHz: 0.33, 14 MHz: 0.14, 18 MHz: 0.14 |

## Tested on 2025-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.81 |       1.62 |
| ITU-R P.533                                | 3.26 |     2.17 |       1.98 |
| VOACAP, signal only                        | 4.00 |     1.74 |       1.58 |
| P.533, signal only                         | 3.12 |     1.88 |       1.74 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.05 |       1.79 |
| ITU-R P.533                                | 3.34 |     2.33 |       2.07 |
| VOACAP, signal only                        | 4.00 |     2.01 |       1.78 |
| P.533, signal only                         | 3.09 |     2.11 |       1.82 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     2.35 |       2.25 |
| ITU-R P.533                                | 3.85 |     2.32 |       2.54 |
| VOACAP, signal only                        | 3.50 |     2.33 |       2.24 |
| P.533, signal only                         | 3.80 |     2.26 |       2.43 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 5.00 |     2.59 |       2.41 |
| ITU-R P.533                                | 5.02 |     2.66 |       2.72 |
| VOACAP, signal only                        | 5.00 |     2.54 |       2.31 |
| P.533, signal only                         | 3.98 |     2.54 |       2.42 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 7.00 |     2.74 |       3.10 |
| ITU-R P.533                                | 7.54 |     2.84 |       3.92 |
| VOACAP, signal only                        | 7.50 |     2.62 |       2.95 |
| P.533, signal only                         | 7.09 |     2.70 |       3.44 |
| flat baseline (needs the month's own data) | 2.25 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2024-12/hours

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                                           |
| ------------------- | -------: | ---------------------------------------------------- |
| VOACAP              |    0.154 | 3 MHz: 0.38, 7 MHz: 0.36, 10 MHz: 0.22, 14 MHz: 0.11 |
| ITU-R P.533         |    0.296 | 3 MHz: 0.45, 7 MHz: 0.34, 10 MHz: 0.27, 14 MHz: 0.25 |
| VOACAP, signal only |    0.154 | 3 MHz: 0.36, 7 MHz: 0.34, 10 MHz: 0.22, 14 MHz: 0.11 |
| P.533, signal only  |    0.288 | 3 MHz: 0.40, 7 MHz: 0.32, 10 MHz: 0.29, 14 MHz: 0.24 |

## Tested on 2025-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.92 |       1.61 |
| ITU-R P.533                                | 3.26 |     2.17 |       2.10 |
| VOACAP, signal only                        | 4.00 |     1.85 |       1.58 |
| P.533, signal only                         | 3.12 |     1.90 |       1.84 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.11 |       1.78 |
| ITU-R P.533                                | 3.34 |     2.32 |       2.29 |
| VOACAP, signal only                        | 4.00 |     2.08 |       1.78 |
| P.533, signal only                         | 3.09 |     2.12 |       2.05 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.77 |       1.55 |
| ITU-R P.533                                | 3.08 |     2.05 |       2.01 |
| VOACAP, signal only                        | 4.00 |     1.70 |       1.54 |
| P.533, signal only                         | 2.97 |     1.92 |       1.85 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 5.00 |     2.53 |       2.47 |
| ITU-R P.533                                | 5.02 |     2.67 |       2.59 |
| VOACAP, signal only                        | 5.00 |     2.50 |       2.33 |
| P.533, signal only                         | 3.98 |     2.48 |       2.46 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 7.00 |     2.50 |       3.00 |
| ITU-R P.533                                | 7.54 |     2.86 |       2.96 |
| VOACAP, signal only                        | 7.50 |     2.46 |       2.89 |
| P.533, signal only                         | 7.09 |     2.63 |       2.72 |
| flat baseline (needs the month's own data) | 2.25 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2019-06/hours

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                              |
| ------------------- | -------: | --------------------------------------- |
| VOACAP              |    0.033 | 7 MHz: 0.22, 10 MHz: 0.01, 14 MHz: 0.01 |
| ITU-R P.533         |    0.085 | 7 MHz: 0.12, 10 MHz: 0.04, 14 MHz: 0.11 |
| VOACAP, signal only |    0.037 | 7 MHz: 0.25, 10 MHz: 0.01, 14 MHz: 0.01 |
| P.533, signal only  |    0.116 | 7 MHz: 0.22, 10 MHz: 0.03, 14 MHz: 0.10 |

## Tested on 2025-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.40 |       1.95 |
| ITU-R P.533                                | 3.26 |     2.37 |       2.35 |
| VOACAP, signal only                        | 4.00 |     2.37 |       1.89 |
| P.533, signal only                         | 3.12 |     2.21 |       2.11 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.51 |       2.03 |
| ITU-R P.533                                | 3.34 |     2.51 |       2.52 |
| VOACAP, signal only                        | 4.00 |     2.48 |       1.99 |
| P.533, signal only                         | 3.09 |     2.37 |       2.32 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.93 |       1.80 |
| ITU-R P.533                                | 3.08 |     1.96 |       1.96 |
| VOACAP, signal only                        | 4.00 |     1.93 |       1.71 |
| P.533, signal only                         | 2.97 |     1.93 |       1.92 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     2.80 |       2.80 |
| ITU-R P.533                                | 3.85 |     2.74 |       2.68 |
| VOACAP, signal only                        | 3.50 |     2.78 |       2.75 |
| P.533, signal only                         | 3.80 |     2.61 |       2.59 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 7.00 |     2.07 |       2.42 |
| ITU-R P.533                                | 7.54 |     2.10 |       2.13 |
| VOACAP, signal only                        | 7.50 |     2.05 |       2.41 |
| P.533, signal only                         | 7.09 |     2.05 |       2.24 |
| flat baseline (needs the month's own data) | 2.25 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

# Amplitude correction fitted on 2019-12/hours

Correction: `corrected = centre + k * (predicted - centre)`, with
`centre` the prediction's own daily median. Factors:

| predictor           | global k | per-band k                                           |
| ------------------- | -------: | ---------------------------------------------------- |
| VOACAP              |    0.065 | 3 MHz: 0.44, 7 MHz: 0.10, 10 MHz: 0.04, 14 MHz: 0.06 |
| ITU-R P.533         |    0.119 | 3 MHz: 0.32, 7 MHz: 0.12, 10 MHz: 0.09, 14 MHz: 0.17 |
| VOACAP, signal only |    0.066 | 3 MHz: 0.44, 7 MHz: 0.10, 10 MHz: 0.04, 14 MHz: 0.06 |
| P.533, signal only  |    0.134 | 3 MHz: 0.31, 7 MHz: 0.14, 10 MHz: 0.09, 14 MHz: 0.16 |

## Tested on 2025-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.24 |       2.20 |
| ITU-R P.533                                | 3.26 |     2.30 |       2.27 |
| VOACAP, signal only                        | 4.00 |     2.20 |       2.16 |
| P.533, signal only                         | 3.12 |     2.18 |       2.16 |
| flat baseline (needs the month's own data) | 2.50 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-07/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     2.37 |       2.35 |
| ITU-R P.533                                | 3.34 |     2.44 |       2.49 |
| VOACAP, signal only                        | 4.00 |     2.36 |       2.30 |
| P.533, signal only                         | 3.09 |     2.34 |       2.38 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2025-03/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 4.00 |     1.90 |       1.89 |
| ITU-R P.533                                | 3.08 |     1.97 |       1.98 |
| VOACAP, signal only                        | 4.00 |     1.89 |       1.86 |
| P.533, signal only                         | 2.97 |     1.92 |       1.92 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2024-12/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 3.50 |     2.67 |       2.59 |
| ITU-R P.533                                | 3.85 |     2.65 |       2.55 |
| VOACAP, signal only                        | 3.50 |     2.67 |       2.58 |
| P.533, signal only                         | 3.80 |     2.55 |       2.54 |
| flat baseline (needs the month's own data) | 3.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.

## Tested on 2019-06/hours

| predictor                                  |  raw | global k | per-band k |
| ------------------------------------------ | ---: | -------: | ---------: |
| VOACAP                                     | 5.00 |     2.10 |       2.00 |
| ITU-R P.533                                | 5.02 |     2.01 |       2.01 |
| VOACAP, signal only                        | 5.00 |     2.10 |       1.99 |
| P.533, signal only                         | 3.98 |     2.03 |       2.00 |
| flat baseline (needs the month's own data) | 2.00 |        — |          — |

Numbers are median absolute error in dB after per-path offset removal.
