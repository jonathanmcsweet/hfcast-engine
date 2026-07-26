# Storm days need a wider spread — measured, and by how much

The reliability calibration (`reliability.md`) left one defect on the record:
beyond 10 dB, bad days happen two to four times more often than the calibrated
spread predicts. The suspected cause was geomagnetic storms. This measurement
confirms that, quantifies it, and turns it into a rule the server can apply
when it knows current conditions.

Full program output: [storm-output.md](storm-output.md). Reproduce with
`tools/fetch-kp.sh` then `storm --kp data/kp_daily.txt --cache data/cache
--fit data/2025-06 --test <the other seven months>`.

## Method

Every measured day-hour (about 592,000 across the eight validation months) is
tagged with the highest Kp index of its preceding 24 hours, from the GFZ
Potsdam record. Kp measures how disturbed the Earth's magnetic field is;
storm effects on the ionosphere outlast the disturbance itself, which is why
the tag looks back a day rather than only at the current 3-hour block.

Each day's deviation from its path-hour's monthly median is divided by the
spread the calibrated model claims for that path-hour, giving a z value. If
the calibration is honest for a group of day-hours, 10% of its z values fall
below -1.28 (the definition of a decile). The ratio between a group's
measured 10% point and -1.28 is the widening that group needs. Pooling z
values across path-hours is what makes storm hours measurable at all: any
single path-hour sees only a few of them. Censoring is handled as in the
reliability check — quantiles only use path-hours whose median sits far
enough above the decode floor that a 15 dB fade was still visible.

## Finding 1 — the calibration holds exactly when it is quiet

On the seven test months, day-hours with no storm in the last 24 hours need
a widening of 1.1–1.2 (June 2025: 0.95–1.05) — the shipped spread scales are
confirmed by data they were not fitted on. The upward side needs no change
under any condition (0.97–1.23 everywhere): storms suppress signals, they do
not boost them.

## Finding 2 — the widening grows with storm strength, and the gradient repeats

Widening needed on the downward side, by highest Kp of the last 24 hours:

| Kp (24h max) | June 2025 (fit) | seven test months |
| ------------ | --------------: | ----------------: |
| below 5      |     x 0.95–1.05 |       x 1.09–1.20 |
| 5–6          |          x 1.51 |            x 1.27 |
| 6–7          |          x 2.15 |            x 1.78 |
| 7 and above  |          x 2.53 |            x 2.34 |

The same staircase appears in a severe-storm month (June 2025), in the
March 2015 St. Patrick's Day storm, and in the moderate storms of 2022–2025.
A single flat "storm factor" is wrong: x 2 fitted on June's severe storm
over-widens the far more common Kp 5–6 case badly.

## The rule that ships

`widening = 1 + 0.5 × (Kp24 − 4.75)`, at least 1, at most 2.5, applied to
the downward spread only. The line is drawn through the band tables of all
eight months; the evidence that it generalises is that the two independent
columns above agree on the staircase. Checked against measured frequencies
on the seven test months:

| condition | deviation | calibrated model | with the rule | actually happened |
| --------- | --------- | ---------------: | ------------: | ----------------: |
| Kp 5–6    | 6 dB      |             9.8% |         14.7% |             11.1% |
| Kp 5–6    | 10 dB     |             2.4% |          5.2% |              5.3% |
| Kp 6–7    | 6 dB      |            10.0% |         21.8% |             18.4% |
| Kp 6–7    | 10 dB     |             2.7% |         10.9% |             10.3% |
| Kp 7+     | 10 dB     |             2.6% |         18.6% |             11.3% |
| Kp 7+     | 15 dB     |             0.4% |          9.4% |             12.4% |

In the 6–10 dB range that decides most reliability values, the rule turns a
3–7 times under-prediction into approximate agreement. Its known
imperfections: it over-warns somewhat at 3 dB during storms, and the deepest
Kp 7+ fades (15 dB and beyond) still exceed what any bell curve can say —
during a severe storm, a shown reliability should be read as a lower bound
of doubt, not an exact probability.

## Where the server applies it

Only a now-cast knows the recent Kp, so only now-casts widen
(`server/src/voacap/correct.ts`, fed from the NOAA K-index feed by
`spaceweather.ts`). Predictions for other days describe a typical day of the
month and keep the quiet-day calibration — including the honest limitation,
now confirmed rather than suspected, that a small share of days (storm days)
will be worse than a typical-day forecast can know.
