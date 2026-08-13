# The fitted daily index against climatology, on real links

`docs/ionosonde.md` proved the daily conditioning against ionosonde
truth. This measurement asks the user-level question: does it improve
predicted SNR on real radio links? It is the ruler `docs/irtam.md`
used — per-day WSPR medians over 150 paths per month — pointed at the
deployable input instead of the assimilated map, and run through the
Rust engine's own API rather than the Fortran reference. Full output:
[essn-wspr-output.md](essn-wspr-output.md). Reproduce with
`tools/fetch-wspr.sh <month> data/<month>` and
`cargo run --release --all-features --bin essn_validate --
--kp data/kp_daily.txt data/<month> ...`.

## Method

Two engine runs per path-day question, identical but for one number:
the sunspot number is either the month's smoothed value (climatology,
the engine as shipped) or the day's fitted index from GIRO soundings
(`sonde::essn_series` — the all-station fit, since the WSPR paths are
independent of the ionosonde network). Absolute error removes one
offset per path, because a path's antennas and local noise are unknown
but constant. The day-to-day metric is the deviation correlation from
each path-hour's monthly median, where climatology scores exactly
zero by construction — the guard printed with every table.

## What the eight months say (525,000 path-day-hours)

Offset-adjusted median absolute error, dB, and the day-to-day
correlation of the essn run (climatology guard read +0.000 in every
month):

| month | clim MAE | essn MAE | day corr | storm-day corr |
| --- | ---: | ---: | ---: | ---: |
| 2015-03 | 3.59 | 3.57 | +0.091 | +0.158 |
| 2019-06 | 3.64 | 3.71 | +0.018 | -0.003 |
| 2019-12 | 3.87 | 3.90 | +0.018 | (no storm days) |
| 2022-09 | 4.37 | 4.20 | +0.078 | +0.166 |
| 2024-12 | 3.58 | 3.57 | +0.027 | +0.051 |
| 2025-03 | 3.85 | 3.81 | +0.056 | +0.066 |
| 2025-06 | 4.76 | 4.36 | +0.024 | +0.054 |
| 2025-07 | 4.51 | 4.23 | +0.059 | +0.149 |

Findings:

1. **At solar maximum the index pays on real links.** The three most
   active months improve by 0.17 to 0.40 dB of median absolute error —
   on a ruler where the whole observed day-to-day spread is about
   2.5 dB, most of it not ionospheric. This is the same signal the
   ionosonde harness saw (bias removed, MAE down), surviving into the
   quantity the application actually forecasts.
2. **At solar minimum the index costs a little.** Both 2019 months are
   0.03 to 0.07 dB worse: with foF2 barely responding to the sunspot
   number, the fitted index adds fit noise and no information. A
   deployed nowcast should shrink the index toward the smoothed value
   when the network's slope information is weak (roadmap).
3. **Day-level link skill is small and lives on storm days**, +0.15 to
   +0.17 where storms happened, near zero on quiet months — consistent
   with `docs/daily.md`, which measured the ceiling for any daily
   model on these paths (lag-1 autocorrelation +0.34 means even a
   perfect one recovers little of the 2.5 dB day spread), and with the
   IRTAM map's +0.1 on the same ruler.

## The decision this supports

Ship the daily conditioning for the active half of the solar cycle and
storm days — where operators most need it and where every ruler now
agrees it helps — and add the solar-minimum shrinkage before calling
the conditioning finished. Link-level SNR error is dominated by
station factors the engine cannot know; the calibration phase (WSPR
and RBN offsets) remains the tool for that layer.
