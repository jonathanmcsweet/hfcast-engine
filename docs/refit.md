# The whole-archive refit — storm table and absorption edge

The first fits of the storm table (`src/stormfit.rs`) and the
absorption-edge level (`truecast::api`) were made on the eight original
validation months. The 2026-08 backfill put every month from 2015-01
to 2026-08 on disk — about 130 months, a full solar cycle — so both
constants were refitted on the archive (2026-08-15). This document
records the held-out design, the method, and the verdicts. Fit
programs: `sonde --fit-storm` and `sonde --fit-edge`; both print their
result as the Rust source that ships.

## The held-out set was designed before the fit

Eight months are named in the `sonde` binary (`HELD_OUT`) and are
excluded from every fit and shown in every verdict. They were chosen
by rule — from the Kp record and the solar cycle, not by looking at
scores — so the verdict covers every stratum the models claim to
serve:

| month | why |
| --- | --- |
| 2015-03 | the original held-out pair, kept held out forever |
| 2022-09 | the original held-out pair, kept held out forever |
| 2024-05 | peak Kp 9.0 — the strongest month in the whole record |
| 2018-08 | peak Kp 7.3 — the only severe month of the deep minimum |
| 2019-03 | quiet March at minimum — the edge season verdict's low end |
| 2024-03 | severe March at maximum — season and severe verdict at once |
| 2020-05 | peak Kp 3.3 — the quietest month in the record |
| 2024-01 | the quietest solar-maximum month, winter |

The fit set is every other month except the current live one
(2026-08): 131 months. The last observed smoothed sunspot number is
2026-01 and later months carry predicted values, but both fits are
independent of that number by construction — the daily index is
fitted from the soundings themselves, the storm ratio is observed
over that index's prediction, and Kp is observed — so those months
fit honestly. The phantom day 31 (roadmap) is filtered from fit
samples the same way `--daily` filters it.

## Storm table: refit on 1.63 million samples — ships on provenance

The archive fit (`sonde --fit-storm`, every non-held-out month with a
Kp record) replaces a six-month fit with a 131-month one. What the
extra data bought is structure, not headline skill: every severe
mid-latitude bin now stands on 400 to 1100 of its own season's
samples where the first fit could only repeat one season-pooled row
across summer, equinox and winter.

Held-out storm-hour foF2 MAE (MHz), the embedded table before and
after:

| month | first fit | archive fit |
| --- | ---: | ---: |
| 2015-03 | 0.706 | 0.727 |
| 2018-08 | 0.458 | 0.452 |
| 2022-09 | 0.600 | 0.581 |
| 2024-03 | 1.095 | 1.051 |
| 2024-05 | 0.881 | 0.895 |

Sample-weighted, that is a wash (0.739 both ways); RMS improves in
four of five and holds in the fifth, and day-to-day correlation
rises on both original held-out months (2015-03 +0.395 to +0.403,
2022-09 +0.452 to +0.474). The two quiet held-out months are
untouched to the third decimal — the quiet-identity contract holds.
The archive table ships because it says the same thing with far more
evidence behind every bin, not because it scores higher.

## Absorption edge: one ratio was hiding two structures

The first calibration shipped a single level, `EDGE_FMIN_RATIO =
1.6138`. Refitting it per month across the archive showed the level
is not one number: it runs about 1.3 near solar minimum and past 2.0
at solar maximum, and swings with the calendar season on top. The
"March residual" the first calibration left open measured as mostly
the index effect — a quiet March at minimum (2019-03) has no March
residual at all.

What ships (`truecast::api::edge_fmin_ratio`): ln(ratio) linear in
the day's index plus the first two calendar harmonics, six
constants, fitted by weighted least squares on each station-day's
median ratio (66,352 station-days) so no single ionogram can steer
the level. The index is clamped to the archive's measured span
outside it.

Held-out verdict, bias / MAE in MHz against observed fmin:

| month | single archive ratio | fitted model |
| --- | --- | --- |
| 2015-03 | +0.64 / 1.07 | +0.08 / 0.81 |
| 2018-08 | −0.42 / 0.66 | −0.13 / 0.62 |
| 2019-03 | −0.00 / 0.67 | −0.08 / 0.66 |
| 2020-05 | −0.53 / 0.70 | −0.26 / 0.63 |
| 2022-09 | +0.07 / 0.77 | −0.17 / 0.73 |
| 2024-01 | +0.41 / 0.90 | −0.10 / 0.70 |
| 2024-03 | +0.75 / 1.08 | +0.00 / 0.73 |
| 2024-05 | −0.36 / 1.02 | −0.58 / 1.04 |

MAE improves in seven of eight months, and the March structure is
gone on Marches the fit never saw. The one miss is the 2024-05
superstorm month: its measured level (1.42) sits far below what its
index and season predict (about 1.75), consistent with storm-driven
absorption raising observed fmin. A storm term for the edge level is
recorded on the roadmap; it needs care because the effect is largest
exactly where months are fewest.

Model-selection notes, all judged on the held-out months only:

- Index alone and season alone each improve on the constant; together
  they halve the held-out miss of the constant. Both terms earn their
  place independently.
- Mirroring the season by hemisphere (a southern station's March as
  September) was measured and rejected: a northern-stations-only fit
  scored on southern stations hinted at the mirror, but the held-out
  verdict preferred the calendar both overall and on the southern
  points themselves. Six southern stations reaching only −34° cannot
  settle the physics; a deeper southern network could reopen this.

