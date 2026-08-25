# Is the reliability number honest?

VOACAP claims a day-to-day spread for every hour: 10% of days fall more
than `SNR LW` dB below the hour's monthly median, 10% rise more than
`SNR UP` above it. The app's "chance of rain" is computed from those
claims, so this checks them against the WSPR record, day by day. All
comparisons are deviations from each path-hour's own median, which no
unknown antenna can shift.

Fitted on 2025-06: the engine's lower decile is 2.51 times too wide
(2122 path-hours), the upper 1.70 times (2239 path-hours). Scale factors
below 40% mean the engine overstates how much days differ from each
other.

Fitted spread scales: lower 0.399, upper 0.587.

## Tested on 2025-07 (2427 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       38.8% |             24.2% |             24.0% |       2331 |
| 6 dB below  |       28.6% |              9.4% |              9.9% |       2135 |
| 10 dB below |       17.3% |              2.3% |              3.6% |       1676 |
| 15 dB below |        8.2% |              0.4% |              1.5% |       1045 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       31.2% |             20.6% |             22.8% |       2425 |
| 6 dB above  |       17.0% |              6.2% |              8.1% |       2416 |
| 10 dB above |        6.5% |              1.1% |              2.1% |       2373 |
| 15 dB above |        1.9% |              0.2% |              0.4% |       2217 |

## Tested on 2022-09 (2151 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       38.7% |             24.1% |             22.1% |       2097 |
| 6 dB below  |       28.5% |              9.4% |             10.0% |       1977 |
| 10 dB below |       17.6% |              2.4% |              4.6% |       1582 |
| 15 dB below |        9.2% |              0.4% |              2.1% |        988 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       30.4% |             19.2% |             19.2% |       2138 |
| 6 dB above  |       15.5% |              4.7% |              5.5% |       2121 |
| 10 dB above |        5.0% |              0.5% |              1.1% |       2035 |
| 15 dB above |        1.0% |              0.1% |              0.2% |       1869 |

## Tested on 2024-12 (2052 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       36.6% |             19.9% |             21.3% |       2023 |
| 6 dB below  |       24.9% |              5.3% |              9.2% |       1940 |
| 10 dB below |       13.2% |              0.8% |              3.9% |       1707 |
| 15 dB below |        5.4% |              0.2% |              1.5% |       1255 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       30.1% |             19.0% |             17.4% |       2023 |
| 6 dB above  |       15.3% |              4.9% |              4.7% |       2016 |
| 10 dB above |        5.2% |              0.8% |              1.1% |       1945 |
| 15 dB above |        1.3% |              0.2% |              0.3% |       1675 |

## Tested on 2019-12 (1594 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       37.2% |             21.0% |             20.6% |       1559 |
| 6 dB below  |       26.0% |              6.5% |              8.0% |       1382 |
| 10 dB below |       14.8% |              1.2% |              3.0% |       1059 |
| 15 dB below |        6.3% |              0.1% |              1.5% |        571 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       31.6% |             21.1% |             18.1% |       1580 |
| 6 dB above  |       17.5% |              6.7% |              4.6% |       1571 |
| 10 dB above |        7.1% |              1.4% |              1.0% |       1540 |
| 15 dB above |        2.2% |              0.2% |              0.3% |       1470 |

## Tested on 2015-03 (608 spread records)

Days falling BELOW the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB below  |       39.7% |             26.1% |             20.1% |        604 |
| 6 dB below  |       30.3% |             11.3% |              8.3% |        575 |
| 10 dB below |       20.1% |              3.4% |              3.4% |        492 |
| 15 dB below |       11.4% |              0.6% |              2.2% |        312 |

Days rising ABOVE the hour's median:

| deviation   | engine says | with fitted scale | actually happened | path-hours |
| ----------- | ----------: | ----------------: | ----------------: | ---------: |
| 3 dB above  |       31.5% |             20.8% |             16.7% |        597 |
| 6 dB above  |       17.1% |              5.9% |              4.5% |        597 |
| 10 dB above |        6.3% |              0.8% |              1.0% |        568 |
| 15 dB above |        1.4% |              0.1% |              0.2% |        499 |

## Storm days need a wider spread: measured, and by how much

The calibration above left one defect on the record: beyond 10 dB, bad
days happen two to four times more often than the calibrated spread
predicts. The suspected cause was geomagnetic storms. This section
confirms that, quantifies it, and turns it into a rule the server can
apply when it knows current conditions.

Full program output: [storm-output.md](storm-output.md). Reproduce with
`tools/fetch-kp.sh` then `storm --kp data/kp_daily.txt --cache
data/cache --fit data/2025-06 --test <the other seven months>`.

### Method

Every measured day-hour (about 592,000 across the eight validation
months) is tagged with the highest Kp index of its preceding 24 hours,
from the GFZ Potsdam record. Kp measures how disturbed the Earth's
magnetic field is; storm effects on the ionosphere outlast the
disturbance itself, which is why the tag looks back a day rather than
only at the current 3-hour block.

Each day's deviation from its path-hour's monthly median is divided by
the spread the calibrated model claims for that path-hour, giving a z
value. If the calibration is honest for a group of day-hours, 10% of its
z values fall below -1.28 (the definition of a decile). The ratio
between a group's measured 10% point and -1.28 is the widening that
group needs. Pooling z values across path-hours is what makes storm
hours measurable at all: any single path-hour sees only a few of them.
Censoring is handled as above, so quantiles only use path-hours whose
median sits far enough above the decode floor that a 15 dB fade was
still visible.

### The calibration holds exactly when it is quiet

On the seven test months, day-hours with no storm in the last 24 hours
need a widening of 1.1 to 1.2 (June 2025: 0.95 to 1.05), so the shipped
spread scales are confirmed by data they were not fitted on. The upward
side needs no change under any condition (0.97 to 1.23 everywhere):
storms suppress signals, they do not boost them.

### The widening grows with storm strength, and the gradient repeats

Widening needed on the downward side, by highest Kp of the last 24
hours:

| Kp (24h max) | June 2025 (fit) | seven test months |
| ------------ | --------------: | ----------------: |
| below 5      |     x 0.95–1.05 |       x 1.09–1.20 |
| 5–6          |          x 1.51 |            x 1.27 |
| 6–7          |          x 2.15 |            x 1.78 |
| 7 and above  |          x 2.53 |            x 2.34 |

The same staircase appears in a severe-storm month (June 2025), in the
March 2015 St. Patrick's Day storm, and in the moderate storms of 2022
to 2025. A single flat "storm factor" is wrong: x 2 fitted on June's
severe storm over-widens the far more common Kp 5 to 6 case badly.

### The rule that ships

`widening = 1 + 0.5 × (Kp24 − 4.75)`, at least 1, at most 2.5, applied
to the downward spread only. The line is drawn through the band tables
of all eight months; the evidence that it generalises is that the two
independent columns above agree on the staircase. Checked against
measured frequencies on the seven test months:

| condition | deviation | calibrated model | with the rule | actually happened |
| --------- | --------- | ---------------: | ------------: | ----------------: |
| Kp 5–6    | 6 dB      |             9.8% |         14.7% |             11.1% |
| Kp 5–6    | 10 dB     |             2.4% |          5.2% |              5.3% |
| Kp 6–7    | 6 dB      |            10.0% |         21.8% |             18.4% |
| Kp 6–7    | 10 dB     |             2.7% |         10.9% |             10.3% |
| Kp 7+     | 10 dB     |             2.6% |         18.6% |             11.3% |
| Kp 7+     | 15 dB     |             0.4% |          9.4% |             12.4% |

In the 6 to 10 dB range that decides most reliability values, the rule
turns a 3 to 7 times under-prediction into approximate agreement. Its
known imperfections: it over-warns somewhat at 3 dB during storms, and
the deepest Kp 7+ fades (15 dB and beyond) still exceed what any bell
curve can say. During a severe storm, a shown reliability should be read
as a lower bound of doubt, not an exact probability.

This widening acts on the *spread* around a finished prediction, and the
server applies it. It is a separate thing from HFcast Truecast's storm
table, which shifts foF2 inside the engine before a prediction is made.

### Where it applies

Only a request that knows the recent Kp can widen, fed from the NOAA
K-index feed. Predictions for other days describe a typical day of the
month and keep the quiet-day calibration, including the honest
limitation, now confirmed rather than suspected, that a small share of
days (storm days) will be worse than a typical-day forecast can know.
