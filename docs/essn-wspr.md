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

A fitted index below zero is floored for the run: the engine goes no
lower than the map's own zero-sunspot plane, and a synthesized
coefficient overlay (`irtam::ccir_at`) pins foF2 alone to the fitted
line. This is the same rule `Conditioning::Daily` applies, and the
section below is what measured it in.

## What the eight months say (525,000 path-day-hours)

Offset-adjusted median absolute error, dB, and the day-to-day
correlation of the essn run (climatology guard read +0.000 in every
month):

| month | clim MAE | essn MAE | day corr | storm-day corr |
| --- | ---: | ---: | ---: | ---: |
| 2015-03 | 3.59 | 3.57 | +0.091 | +0.158 |
| 2019-06 | 3.64 | 3.68 | +0.025 | -0.001 |
| 2019-12 | 3.87 | 3.90 | +0.021 | (no storm days) |
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
2. **The solar-minimum cost was the engine below its map, not the
   fit.** The first run of this study (0.79.0) found both 2019 months
   0.03 to 0.07 dB worse and blamed fit noise. Diagnosed 2026-08-13:
   the solar-minimum fit is the best-sampled of all eight months (up
   to 496 samples per day, standard error 0.7 index units), and
   shrinking its day-to-day movement recovered nothing — the cost sat
   in the persistent offset, which ran the whole engine at an index
   near -17, below the map's zero-sunspot plane, where foE, absorption
   and noise have no measured state to extrapolate into. The floor
   (foF2 follows the fitted line, every other channel stops at zero)
   removed half the 2019-06 cost and raised its day correlation from
   +0.018 to +0.025 while leaving every non-negative index untouched
   — the regenerated cache for an active month is byte-identical. The
   remaining 0.03 dB is at the ruler's own resolution.
3. **Day-level link skill is small and lives on storm days**, +0.15 to
   +0.17 where storms happened, near zero on quiet months — consistent
   with `docs/daily.md`, which measured the ceiling for any daily
   model on these paths (lag-1 autocorrelation +0.34 means even a
   perfect one recovers little of the 2.5 dB day spread), and with the
   IRTAM map's +0.1 on the same ruler.

## The decision this supports

Ship the daily conditioning with the floor, for the whole solar cycle:
active months and storm days carry the gains, and at solar minimum the
floor holds the cost to the ruler's resolution while the foF2
correction — the part ionosondes verify directly — stays whole. The
floor has no fitted constant, so there is nothing to overfit and
nothing to re-tune; both solar-minimum months in the archive are fit
months, which a fitted constant would have had to worry about.
Link-level SNR error is dominated by station factors the engine cannot
know; the calibration phase (WSPR and RBN offsets) remains the tool
for that layer.
