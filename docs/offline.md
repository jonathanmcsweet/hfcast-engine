# The offline forecast — how a device with no network beats climatology

Some devices run the app with zero network access, some forever. The
engine still gives them a better forecast than the parity engine's
climatology through two conditionings in `nowcast::api`:
`Conditioning::offline` (nothing but the install) and
`Conditioning::offline_synced` (the install plus a baked snapshot).
This page explains how both work and the measurements behind them.
The fits are reproduced by `sonde --fit-offline` and
`sonde --fit-sync`.

## What a never-online device has

Both engines need one solar-activity input. VOACAP's is the month's
smoothed sunspot number, and offline devices already carry it: the
predictions are published years ahead and ship inside the app. So the
question is not "can we predict with nothing" but "can we turn the
shipped calendar into a better daily number than the month-flat one".

## Why not a plain daily average

The direct form — for each calendar day, the mean fitted index across
the 2015-2026 archive, no sunspot input — was measured
(leave-one-year-out): median miss 48.6 index units where the sunspot
number misses by 12.5. Four times worse. The reason is the solar
cycle: the same calendar day spans an index near 5 (2020) and near
150 (2024), and their average is wrong in every year at once. An
offline model has to ride the shipped sunspot series; what the
archive adds is everything that series gets wrong.

## `Conditioning::offline`: the day-of-year correction curve

The fitted daily index sits systematically away from the smoothed
sunspot number — about 11 index units low on average, more in active
years, with a repeatable seasonal swing. That structure survives
leave-one-year-out scoring, so it ships as a smooth curve over the
day of year (mean plus two harmonics, five constants —
`OFFLINE_ANOMALY_MODEL`): every calendar day gets its own correction,
no monthly plateaus, no step at a month boundary. The conditioning is
the shipped sunspot number plus the curve's value for the date, with
no storm correction (a device without a Kp feed honestly has none).

Fit: each archive day's median fitted index minus its month's R12,
equal weight per day; the eight held-out months and months whose
table entry is itself a prediction (`wspr::SSN_PREDICTED_FROM`) are
excluded. 3,802 day medians.

Scored in MHz of foF2 error against the ionosonde archive, every
sample rescored on its own index line, leave-one-year-out (the table
never contains the scored year), 1.5 million samples:

| year | climatology | offline | live-data ceiling |
| --- | ---: | ---: | ---: |
| 2015 | 0.845 | 0.880 | — |
| 2016 | 0.684 | 0.637 | — |
| 2017 | 0.650 | 0.601 | — |
| 2018 | 0.617 | 0.582 | — |
| 2019 | 0.602 | 0.566 | — |
| 2020 | 0.599 | 0.583 | — |
| 2021 | 0.730 | 0.623 | — |
| 2022 | 0.888 | 0.795 | — |
| 2023 | 0.895 | 0.851 | — |
| 2024 | 1.017 | 0.924 | — |
| 2025 | 1.006 | 0.927 | — |
| 2026 | 0.949 | 0.896 | — |
| all | 0.766 | 0.717 | 0.652 |

Better in eleven of twelve years, and by the most exactly where
climatology is worst — the active years. Overall the correction
recovers about two fifths of the full live-data advantage. The
exception is 2015, the one year whose offset sits on the other side
of the archive's; it is in the table, not hidden.

The held-out months (never in any fit; full table in the
`sonde --fit-offline` output):

| month | climatology bias / MAE | offline bias / MAE |
| --- | --- | --- |
| 2015-03 | −0.18 / 0.87 | −0.37 / 0.93 |
| 2018-08 | +0.16 / 0.56 | +0.04 / 0.53 |
| 2019-03 | +0.32 / 0.62 | +0.11 / 0.54 |
| 2020-05 | +0.16 / 0.59 | −0.05 / 0.56 |
| 2022-09 | +0.60 / 0.87 | +0.34 / 0.78 |
| 2024-01 | +0.10 / 0.82 | −0.16 / 0.79 |
| 2024-03 | +0.55 / 0.91 | +0.37 / 0.84 |
| 2024-05 | +0.71 / 1.19 | +0.51 / 1.11 |

Seven of eight improve. The seven months whose sunspot entries are
themselves predictions — the exact condition a never-online device
lives in — improve in six of seven, because the correction's sign
also leans against the predictions' habit of running hot.

A month-of-year table and a single constant both score within
0.001 MHz of the curve overall; the curve ships because day
granularity costs nothing.

## `Conditioning::offline_synced`: the baked snapshot

A build can carry a snapshot of the real measured index, so even an
APK carried to an air-gapped device on foot starts from a measured
day rather than the calendar. `sonde --sync-record data/<month>`
prints the JSON to bake — the last measured day's index and its
anomaly against the embedded sunspot table (the app must ship the
same table version) — and `Conditioning::offline_synced` consumes it.

The record's value beyond the offline curve decays by the fitted
weight `SYNC_DECAY`: `w(N) = 0.575 exp(-N / 24 days) + 0.05`. Half
the head start is gone in about seventeen days; the small floor is
the slow memory of the cycle's current regime, measured real in the
two-to-twelve-month staleness buckets. Because the record's
contribution multiplies by `w` and adds to the curve, an aged record
converges onto plain `Conditioning::offline` and can never fall below
it: the never-online numbers above are the worst case at every age.

Held-out verdict, foF2 MAE in MHz with the record aged along a
ladder (full table in the `sonde --fit-sync` output):

| month | climatology | offline curve | fresh record | 7 days | 30 days | 90 days |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 2018-08 | 0.591 | 0.544 | 0.528 | 0.549 | 0.548 | 0.545 |
| 2019-03 | 0.611 | 0.536 | 0.523 | 0.540 | 0.540 | 0.542 |
| 2020-05 | 0.580 | 0.548 | 0.541 | 0.550 | 0.547 | 0.548 |
| 2022-09 | 0.880 | 0.782 | 0.655 | 0.743 | 0.762 | 0.789 |
| 2024-01 | 0.836 | 0.800 | 0.740 | 0.770 | 0.818 | 0.803 |
| 2024-03 | 0.940 | 0.863 | 0.798 | 0.850 | 0.864 | 0.851 |
| 2024-05 | 1.211 | 1.127 | 0.939 | 1.107 | 1.082 | 1.125 |

Every rung of every row beats climatology; a fresh record beats the
offline curve in all eight held-out months (2015-03 included: 0.787
against the curve's 0.954), and by 90 days the answer has settled
onto the curve.
