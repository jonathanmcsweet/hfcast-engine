# The fitted daily index against monthly climatology, on real links

Same engine, same configuration, one change: the sunspot number is the day's fitted index from GIRO soundings instead of the month's smoothed value. Scored against per-day WSPR medians.

## 2015-03 (37164 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 3.59 |
| essn | 3.57 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 36210 | +0.091 | 0.16 | 2.00 |
| quiet (Kp < 3) | 5787 | -0.003 | 0.07 | 2.00 |
| unsettled (3-5) | 24187 | +0.065 | 0.14 | 2.00 |
| storm (Kp >= 5) | 6236 | +0.158 | 0.60 | 2.50 |

## 2019-06 (67226 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 3.64 |
| essn | 3.68 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 66972 | +0.025 | 0.02 | 2.50 |
| quiet (Kp < 3) | 60967 | +0.028 | 0.02 | 2.50 |
| unsettled (3-5) | 3345 | -0.015 | 0.01 | 2.50 |
| storm (Kp >= 5) | 2660 | -0.001 | 0.01 | 2.50 |

## 2019-12 (57682 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 3.87 |
| essn | 3.90 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 56704 | +0.021 | 0.07 | 2.00 |
| quiet (Kp < 3) | 52553 | +0.020 | 0.06 | 2.00 |
| unsettled (3-5) | 4151 | +0.026 | 0.25 | 2.00 |
| storm (Kp >= 5) | 0 | n/a | 0.00 | 0.00 |

## 2022-09 (69804 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 4.37 |
| essn | 4.20 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 69083 | +0.078 | 0.26 | 2.00 |
| quiet (Kp < 3) | 21882 | +0.048 | 0.24 | 2.00 |
| unsettled (3-5) | 37269 | +0.049 | 0.22 | 2.00 |
| storm (Kp >= 5) | 9932 | +0.166 | 0.68 | 2.75 |

## 2024-12 (67910 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 3.58 |
| essn | 3.57 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 67197 | +0.027 | 0.16 | 2.00 |
| quiet (Kp < 3) | 32283 | +0.020 | 0.17 | 2.00 |
| unsettled (3-5) | 32166 | +0.032 | 0.17 | 2.00 |
| storm (Kp >= 5) | 2748 | +0.051 | 0.03 | 2.00 |

## 2025-03 (73451 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 3.85 |
| essn | 3.81 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 73009 | +0.056 | 0.27 | 2.00 |
| quiet (Kp < 3) | 13975 | +0.088 | 0.15 | 2.00 |
| unsettled (3-5) | 43226 | +0.046 | 0.28 | 2.00 |
| storm (Kp >= 5) | 15808 | +0.066 | 0.39 | 2.00 |

## 2025-06 (73578 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 4.76 |
| essn | 4.36 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 73179 | +0.024 | 0.34 | 2.50 |
| quiet (Kp < 3) | 13237 | +0.062 | 0.27 | 2.25 |
| unsettled (3-5) | 41127 | +0.026 | 0.29 | 2.50 |
| storm (Kp >= 5) | 18815 | +0.054 | 0.70 | 3.25 |

## 2025-07 (78662 path-day-hours)

Absolute error, one offset per path (median absolute error, dB):

| model | error |
| --- | --: |
| climatology | 4.51 |
| essn | 4.23 |

Day-to-day deviations from each path-hour's monthly median
(climatology guard: +0.000, must be +0.000 — a model that never varies by day cannot correlate):

| condition | day-hours | correlation | predicted size (dB) | observed size (dB) |
| --- | --: | --: | --: | --: |
| all days | 78330 | +0.059 | 0.34 | 2.50 |
| quiet (Kp < 3) | 24840 | +0.088 | 0.46 | 2.50 |
| unsettled (3-5) | 48952 | +0.043 | 0.27 | 2.50 |
| storm (Kp >= 5) | 4538 | +0.149 | 0.69 | 2.50 |

