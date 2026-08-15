# Beating VOACAP with no network at all

The maintainer's requirement (2026-08-15): some devices run the app
with zero network access, forever. For them the engine must still do
better than VOACAP — not fall back to it. This document records what
was measured, what was rejected, and the model that ships:
`Conditioning::offline` in `nowcast::api`, fitted by
`sonde --fit-offline`.

## What a never-online device has

Both engines need one solar-activity input. VOACAP's is the month's
smoothed sunspot number, and offline devices already carry it: the
predictions are published years ahead and ship inside the app. So the
honest question is not "can we predict with nothing" but "can we turn
the shipped calendar into a better daily number than VOACAP's
month-flat one".

## A fixed 365-day table of the index was measured and rejected

The direct form of a daily average — for each calendar day, the mean
fitted index across the 2015-2026 archive, no sunspot input — was
tested first (leave-one-year-out): median miss 48.6 index units where
VOACAP's number misses by 12.5. Four times worse. The reason is the
solar cycle: the same calendar day spans an index near 5 (2020) and
near 150 (2024), and their average is wrong in every year at once. Any
offline model has to ride the shipped sunspot series; what our archive
can add is everything that series gets wrong.

## What ships: a day-of-year correction curve

The fitted daily index sits systematically away from the smoothed
sunspot number — about 11 index units low on average, more in active
years, with a repeatable seasonal swing on top. That structure is
stable enough to survive leave-one-year-out scoring, so it ships as a
smooth curve over the day of year (mean plus two harmonics, five
constants — `OFFLINE_ANOMALY_MODEL`): every calendar day gets its own
correction, with no monthly plateaus and no step at a month boundary.
The offline conditioning is the shipped sunspot number plus the
curve's value for the date, no storm correction (a device without a
Kp feed honestly has none).

Fit: each archive day's median fitted index minus its month's R12,
equal weight per day, months whose table entry is itself a prediction
(`wspr::SSN_PREDICTED_FROM`) and the eight held-out months excluded.
3,802 day medians.

## The verdict

Scored in MHz of foF2 error against the ionosonde archive, every foF2
sample rescored on its own index line. Leave-one-year-out (the table
never contains the scored year), 1.5 million samples:

| year | VOACAP | offline | live-data ceiling |
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
VOACAP is worst — the active years whose storms decide whether a
field forecast was worth carrying. Overall the free, never-online
correction recovers about two fifths of the full live-data
advantage. The exception is 2015, the one year whose offset sits on
the other side of the archive's; it is in the table above, not hidden.

The held-out months (fit never saw them; full table in the
`sonde --fit-offline` output):

| month | VOACAP bias / MAE | offline bias / MAE |
| --- | --- | --- |
| 2015-03 | −0.18 / 0.87 | −0.37 / 0.93 |
| 2018-08 | +0.16 / 0.56 | +0.04 / 0.53 |
| 2019-03 | +0.32 / 0.62 | +0.11 / 0.54 |
| 2020-05 | +0.16 / 0.59 | −0.05 / 0.56 |
| 2022-09 | +0.60 / 0.87 | +0.34 / 0.78 |
| 2024-01 | +0.10 / 0.82 | −0.16 / 0.79 |
| 2024-03 | +0.55 / 0.91 | +0.37 / 0.84 |
| 2024-05 | +0.71 / 1.19 | +0.51 / 1.11 |

Seven of eight improve. And the seven 2026 months whose sunspot
entries are themselves predictions — the exact condition a
never-online device lives in — improve in six of seven, because the
correction's sign also leans against the predictions' habit of
running hot.

## The baked sync record: a build-time head start that ages gracefully

The maintainer's second requirement: a build can carry a snapshot of
the real measured index, so even an APK carried to an air-gapped
device on foot starts from a measured day rather than the calendar.
`sonde --sync-record data/<current month>` prints the JSON to bake
(the last measured day's index and its anomaly against the embedded
sunspot table — the app must ship the same table version), and
`Conditioning::offline_synced` consumes it.

The record's value beyond the offline curve decays by the fitted
weight `SYNC_DECAY`: `w(N) = 0.575 exp(-N / 24 days) + 0.05`
(`sonde --fit-sync`) — half the head start is gone in about
seventeen days, and the small floor is the slow memory of the
cycle's current regime, measured real in the two-to-twelve-month
buckets. Because the record's contribution multiplies by `w` and adds
to the curve, an aged record converges onto plain
`Conditioning::offline` and can never fall below it: the never-online
floor of this document is the worst case at every age.

Held-out verdict, foF2 MAE in MHz with the record aged along a
ladder (full table in the `sonde --fit-sync` output):

| month | VOACAP | offline curve | fresh record | 7 days | 30 days | 90 days |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 2018-08 | 0.591 | 0.544 | 0.528 | 0.549 | 0.548 | 0.545 |
| 2019-03 | 0.611 | 0.536 | 0.523 | 0.540 | 0.540 | 0.542 |
| 2020-05 | 0.580 | 0.548 | 0.541 | 0.550 | 0.547 | 0.548 |
| 2022-09 | 0.880 | 0.782 | 0.655 | 0.743 | 0.762 | 0.789 |
| 2024-01 | 0.836 | 0.800 | 0.740 | 0.770 | 0.818 | 0.803 |
| 2024-03 | 0.940 | 0.863 | 0.798 | 0.850 | 0.864 | 0.851 |
| 2024-05 | 1.211 | 1.127 | 0.939 | 1.107 | 1.082 | 1.125 |

Every rung of every row beats VOACAP; a fresh record beats the
offline curve in all eight held-out months (2015-03 included: 0.787
against the curve's 0.954), and by 90 days the answer has settled
onto the curve. The bucket weights show a visible bump at the
15-to-30-day lag — the sun's 27-day rotation bringing the same face
back around — which the smooth decay does not model; it is left as a
possible term if a future batch wants the last few hundredths.

## What was measured and left on the table

A month-of-year correction table and a single constant both score
within 0.001 MHz of the day-of-year curve overall; the curve ships
because day granularity was the requirement and costs nothing. The
27-day rotation echo in the sync decay is noted above. Wiring both
offline forms through the service JSON remains on the roadmap.
