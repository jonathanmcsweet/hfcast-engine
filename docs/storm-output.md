# Do storm days need a wider spread?

Every measured day-hour is tagged with the highest Kp of the preceding 24 hours. z is the day's deviation divided by the calibrated model's claimed spread for that path-hour; if the calibration holds for a group, its z at 10% is -1.28, and the widening column is how much wider the spread must be to make that true.

## Day-hours per condition (fit months)

| month   | quiet | unsettled | storm | no record |
| ------- | ----: | --------: | ----: | --------: |
| 2025-06 | 10367 |     32479 | 15177 |         0 |

## Pooled z quantiles (fit months)

Days below the median:

| condition       | day-hours | z at 10% | z at 5% | z at 2% | widening needed |
| --------------- | --------: | -------: | ------: | ------: | --------------: |
| quiet (Kp < 3)  |      4532 |    -1.22 |   -1.75 |   -2.58 |          x 0.95 |
| unsettled (3-5) |     14568 |    -1.31 |   -1.92 |   -2.81 |          x 1.02 |
| storm (Kp >= 5) |      6982 |    -2.67 |   -3.49 |   -4.64 |          x 2.08 |

Days above the median:

| condition       | day-hours | z at 90% | z at 95% | z at 98% | widening needed |
| --------------- | --------: | -------: | -------: | -------: | --------------: |
| quiet (Kp < 3)  |      9459 |     1.55 |     2.17 |     3.00 |          x 1.21 |
| unsettled (3-5) |     29631 |     1.58 |     2.17 |     2.88 |          x 1.23 |
| storm (Kp >= 5) |     13781 |     1.46 |     2.14 |     2.88 |          x 1.14 |

## Widening by storm strength, below side (fit months)

| Kp (24h max) | day-hours | z at 10% | widening needed |
| ------------ | --------: | -------: | --------------: |
| Kp < 2       |         0 |  too few |                 |
| 2-3          |      4532 |    -1.22 |          x 0.95 |
| 3-4          |      8633 |    -1.28 |          x 1.00 |
| 4-5          |      5935 |    -1.34 |          x 1.05 |
| 5-6          |      3015 |    -1.93 |          x 1.51 |
| 6-7          |      1743 |    -2.76 |          x 2.15 |
| Kp >= 7      |      2224 |    -3.24 |          x 2.53 |

Storm widening fitted on the fit months (below side): x 2.08

## Day-hours per condition (test months)

| month   | quiet | unsettled | storm | no record |
| ------- | ----: | --------: | ----: | --------: |
| 2015-03 |  2192 |      9302 |  2585 |         0 |
| 2022-09 | 17702 |     30985 |  8438 |         0 |
| 2024-12 | 27592 |     26915 |  2183 |         0 |
| 2025-03 | 12106 |     37895 | 14347 |         0 |
| 2025-07 | 20465 |     41464 |  3977 |         0 |
| 2019-06 | 43294 |      2357 |  1949 |         0 |
| 2019-12 | 38402 |      2936 |     0 |         0 |

## Pooled z quantiles (test months)

Days below the median:

| condition       | day-hours | z at 10% | z at 5% | z at 2% | widening needed |
| --------------- | --------: | -------: | ------: | ------: | --------------: |
| quiet (Kp < 3)  |     72135 |    -1.50 |   -2.28 |   -3.37 |          x 1.17 |
| unsettled (3-5) |     78781 |    -1.45 |   -2.21 |   -3.33 |          x 1.13 |
| storm (Kp >= 5) |     17611 |    -1.70 |   -2.65 |   -3.92 |          x 1.33 |

Days above the median:

| condition       | day-hours | z at 90% | z at 95% | z at 98% | widening needed |
| --------------- | --------: | -------: | -------: | -------: | --------------: |
| quiet (Kp < 3)  |    142858 |     1.26 |     1.81 |     2.60 |          x 0.98 |
| unsettled (3-5) |    129583 |     1.24 |     1.76 |     2.53 |          x 0.97 |
| storm (Kp >= 5) |     28331 |     1.25 |     1.76 |     2.43 |          x 0.98 |

## Widening by storm strength, below side (test months)

| Kp (24h max) | day-hours | z at 10% | widening needed |
| ------------ | --------: | -------: | --------------: |
| Kp < 2       |     35277 |    -1.54 |          x 1.20 |
| 2-3          |     36858 |    -1.44 |          x 1.13 |
| 3-4          |     45163 |    -1.39 |          x 1.09 |
| 4-5          |     33618 |    -1.50 |          x 1.17 |
| 5-6          |     15844 |    -1.63 |          x 1.27 |
| 6-7          |      1526 |    -2.28 |          x 1.78 |
| Kp >= 7      |       241 |    -3.00 |          x 2.34 |

## Frequencies by condition, test months (storm widening x 2.08)

quiet (Kp < 3) — days below the median:

| deviation   | calibrated model says | with storm widening | actually happened |   days |
| ----------- | --------------------: | ------------------: | ----------------: | -----: |
| 3 dB below  |                 23.3% |               23.3% |             21.7% | 157038 |
| 6 dB below  |                  8.6% |                8.6% |              8.9% | 145600 |
| 10 dB below |                  2.0% |                2.0% |              3.3% | 118243 |
| 15 dB below |                  0.3% |                0.3% |              1.5% |  72135 |

unsettled (3-5) — days below the median:

| deviation   | calibrated model says | with storm widening | actually happened |   days |
| ----------- | --------------------: | ------------------: | ----------------: | -----: |
| 3 dB below  |                 24.0% |               24.0% |             21.7% | 148635 |
| 6 dB below  |                  9.1% |                9.1% |              9.0% | 140384 |
| 10 dB below |                  2.2% |                2.2% |              3.6% | 117344 |
| 15 dB below |                  0.4% |                0.4% |              1.6% |  78781 |

storm (Kp >= 5) — days below the median:

| deviation   | calibrated model says | with storm widening | actually happened |  days |
| ----------- | --------------------: | ------------------: | ----------------: | ----: |
| 3 dB below  |                 24.8% |               37.0% |             24.2% | 32810 |
| 6 dB below  |                  9.8% |               25.5% |             11.9% | 31161 |
| 10 dB below |                  2.5% |               14.2% |              5.8% | 26315 |
| 15 dB below |                  0.4% |                6.3% |              3.5% | 17611 |

quiet (Kp < 3) — days above the median:

| deviation   | calibrated model says | with storm widening | actually happened |   days |
| ----------- | --------------------: | ------------------: | ----------------: | -----: |
| 3 dB above  |                 20.2% |               20.2% |             19.8% | 160218 |
| 6 dB above  |                  5.8% |                5.8% |              6.1% | 159353 |
| 10 dB above |                  1.0% |                1.0% |              1.6% | 155214 |
| 15 dB above |                  0.2% |                0.2% |              0.4% | 142858 |

unsettled (3-5) — days above the median:

| deviation   | calibrated model says | with storm widening | actually happened |   days |
| ----------- | --------------------: | ------------------: | ----------------: | -----: |
| 3 dB above  |                 20.2% |               20.2% |             18.7% | 150460 |
| 6 dB above  |                  5.6% |                5.6% |              5.4% | 149589 |
| 10 dB above |                  0.9% |                0.9% |              1.3% | 144548 |
| 15 dB above |                  0.1% |                0.1% |              0.3% | 129583 |

storm (Kp >= 5) — days above the median:

| deviation   | calibrated model says | with storm widening | actually happened |  days |
| ----------- | --------------------: | ------------------: | ----------------: | ----: |
| 3 dB above  |                 20.2% |               34.3% |             19.2% | 33145 |
| 6 dB above  |                  5.5% |               21.2% |              5.5% | 32934 |
| 10 dB above |                  0.8% |                9.7% |              1.1% | 31651 |
| 15 dB above |                  0.1% |                3.1% |              0.3% | 28331 |

## Graded rule on the test months: widening 1 + 0.5 x (Kp24 - 4.75), capped at 2.5

4-5 — days below the median:

| deviation   | calibrated model says | with graded rule | actually happened |  days |
| ----------- | --------------------: | ---------------: | ----------------: | ----: |
| 3 dB below  |                 24.5% |            24.5% |             23.0% | 64738 |
| 6 dB below  |                  9.7% |             9.7% |              9.9% | 60878 |
| 10 dB below |                  2.4% |             2.4% |              4.1% | 50574 |
| 15 dB below |                  0.4% |             0.4% |              2.0% | 33618 |

5-6 — days below the median:

| deviation   | calibrated model says | with graded rule | actually happened |  days |
| ----------- | --------------------: | ---------------: | ----------------: | ----: |
| 3 dB below  |                 24.8% |            29.2% |             23.3% | 29211 |
| 6 dB below  |                  9.8% |            14.7% |             11.1% | 27740 |
| 10 dB below |                  2.4% |             5.2% |              5.3% | 23514 |
| 15 dB below |                  0.4% |             1.4% |              3.2% | 15844 |

6-7 — days below the median:

| deviation   | calibrated model says | with graded rule | actually happened | days |
| ----------- | --------------------: | ---------------: | ----------------: | ---: |
| 3 dB below  |                 24.7% |            34.5% |             32.0% | 3107 |
| 6 dB below  |                 10.0% |            21.8% |             18.4% | 2945 |
| 10 dB below |                  2.7% |            10.9% |             10.3% | 2404 |
| 15 dB below |                  0.5% |             4.5% |              5.0% | 1526 |

Kp >= 7 — days below the median:

| deviation   | calibrated model says | with graded rule | actually happened | days |
| ----------- | --------------------: | ---------------: | ----------------: | ---: |
| 3 dB below  |                 25.2% |            39.1% |             29.5% |  492 |
| 6 dB below  |                 10.2% |            29.2% |             17.0% |  476 |
| 10 dB below |                  2.6% |            18.6% |             11.3% |  397 |
| 15 dB below |                  0.4% |             9.4% |             12.4% |  241 |
