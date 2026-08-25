# Predictions against ionosonde truth

`docs/ionosonde.md` closed with three ways its answer could change, and the
largest was ground truth: WSPR daily medians are noisy enough to hide
real skill. This measurement is that better ruler. It scores predicted
ionospheric characteristics against the scaled soundings of the GIRO
ionosonde network — absolute values, in the model's own units, over
known points. Full program output: [ionosonde-output.md](ionosonde-output.md).
Reproduce with `tools/fetch-kp.sh`, `tools/fetch-giro.sh <month>`,
`tools/fetch-irtam.sh <month>`, then
`cargo run --release --all-features --bin sonde -- --kp data/kp_daily.txt data/<month>`.

## Method

For each station in a month bundle, the engine runs one probe path of
about 111 km centered on the station, so the path's single control point
is the station itself. `Task::Parameters` returns the unrounded layer
values per hour. Two model columns so far:

- **climatology** — the engine as shipped, at the month's smoothed
  sunspot number.
- **irtam** — the same, with each day's archived IRTAM map written over
  the coefficient file through the overlay root (`src/irtam.rs`), as in
  the WSPR study. foF2 for the frequency rows; for the height rows the
  hmF2 map goes through the same slot, so the engine's own Jones-Gallet
  evaluator computes it at the station.
- **climatology+dudeney** (heights only) — climatology's own M(3000)F2
  through Dudeney's corrected form instead of the engine's plain
  `1490/M - 176`, separating the formula's error from its input's.
- **essn** (frequency rows only) — one effective sunspot number fitted
  per day from the day's GIRO foF2 readings, always leaving the scored
  station out (`src/essn.rs`). Predicted foF2 is exactly linear in the
  sunspot number, so the fit is a median of closed-form per-sample
  solutions.
- **essn+storm** (frequency rows only) — the essn prediction times the
  embedded storm ratio (`src/stormfit.rs`): median observed-over-essn
  ratios binned by trailing-24-hour Kp class, geomagnetic latitude
  band, season and local-time quarter, fitted on the six fit months
  (2019-06, 2019-12, 2024-12, 2025-03, 2025-06, 2025-07) and scored on
  the two held-out storm months (2015-03, 2022-09). Quiet bins and
  low-latitude bins are the identity by construction.
- **absorption edge** (fmin rows) — the lower edge of the usable
  window, against the ionogram's fmin. The model is the engine's own
  SNR-versus-frequency curve over the probe path
  (`truecast::api::probe_edge`): the lowest frequency within 6 dB of
  the hour's plateau, interpolated on a ten-rung ladder from 2 to
  10.3 MHz. Relative to the plateau rather than absolute, the way
  fmin is relative to the sounder's own echo strength; the remaining
  per-station constants motivate the offset-removed rows in the fmin
  table. Hours whose whole ladder sits within the drop have no edge —
  the usual night state, where observed fmin is the instrument floor.
  The engine's LUF task cannot serve here: its scan floors at 2 MHz,
  its usable-budget window on a 111 km path is about 4 dB wide, and
  outside it the answer flips sign and tracks the best frequency near
  the MUF — a different edge (measured 2026-08-13). The essn variant
  runs the same probe at the day's holdout index, floored at zero as
  the conditioning floors it. The level between the probe edge and the
  ionogram convention is one fitted multiplicative constant
  (`truecast::api::EDGE_FMIN_RATIO`, 1.6138): the median of edge over
  fmin across the six fit months, fitted by `sonde --fit-edge` with
  the two storm months held out. Multiplicative because a fixed
  decibel budget crosses the absorption wall at a fixed frequency
  ratio; the additive form was measured and transfers worse
  (held-out MAE 1.13 and 1.64 MHz against 0.79 and 1.11).

Predicted foF2 is put back on the ionosonde's convention before the
comparison: the engine's F2 working frequency is the extraordinary wave
(the map value plus half the gyrofrequency), and an ionosonde scales the
ordinary wave. Without that step the whole column reads about 0.55 MHz
high, and the error is the magnetic field, not the model.

The decisive day-to-day metric is the one the WSPR study used: the
correlation between predicted and observed deviations from each
station-hour's monthly median. Climatology scores exactly zero by
construction, and the harness prints that guard with every table.

NVIS is scored as its own class: MUF at ground ranges of 0, 300 and
600 km, from foF2 and the mirror-geometry secant at hmF2, plus the
band-call question the app's user actually asks — was 80/60/40/30 m
usable this hour — as hit, miss and false-alarm rates.

## What the eight months say (237,506 samples, 14-22 stations each)

foF2, model minus observed. `essn` is the leave-one-station-out fitted
daily index — the deployable column; `irtam` is the assimilated-map
upper bound.

| month | clim bias / MAE | essn bias / MAE | irtam MAE | day corr: essn / irtam | calls @600, clim → both maps |
| --- | --- | --- | --- | --- | --- |
| 2015-03 | -0.26 / 0.71 | +0.00 / 0.58 | 0.34 | +0.390 / +0.794 | 92.7% → 96.0% |
| 2019-06 | +0.38 / 0.49 | +0.02 / 0.40 | 0.28 | +0.147 / +0.493 | 89.9% → 93.2% |
| 2019-12 | +0.07 / 0.54 | -0.01 / 0.55 | 0.30 | +0.108 / +0.572 | 88.2% → 93.4% |
| 2022-09 | +0.53 / 0.69 | +0.00 / 0.51 | 0.36 | +0.462 / +0.730 | 91.5% → 95.0% |
| 2024-12 | +0.86 / 0.96 | +0.01 / 0.62 | 0.39 | +0.428 / +0.757 | 92.0% → 95.6% |
| 2025-03 | +0.34 / 0.70 | +0.00 / 0.62 | 0.40 | +0.404 / +0.795 | 93.4% → 95.8% |
| 2025-06 | +0.74 / 0.91 | -0.01 / 0.57 | 0.36 | +0.164 / +0.745 | 89.8% → 94.2% |
| 2025-07 | +0.59 / 0.75 | -0.04 / 0.51 | 0.32 | +0.287 / +0.751 | 91.3% → 94.7% |

The climatology day-to-day guard printed +0.000 in every table of every
month. Findings, in order of consequence:

1. **The WSPR ruler was the limit, as suspected.** The same IRTAM input
   that scored about +0.1 day-to-day against WSPR medians scores +0.49
   to +0.80 against ionosonde truth, in every month, weakest at solar
   minimum where daily variance is smallest. The assimilated map does
   know what the ionosphere did that day; WSPR could not see it.
   (One caveat below.)
2. **Climatology's foF2 bias moves month by month, and the fitted daily
   index removes it out of sample.** The bias runs -0.26 to +0.86 MHz
   across the eight months; the leave-one-station-out index brings it
   to zero in every month, improves MAE in seven of eight, and carries
   +0.11 to +0.46 day-to-day skill — strongest exactly where it
   matters, in the storm months. That skill uses no data from the
   scored station: it is what a deployed truecast could really have.
3. **The +61 km height bias decomposes** (2025-06). Dudeney's corrected
   form over climatology's own inputs removes about 19 km; the rest is
   the M(3000)F2 input itself. IRTAM's height map removes nearly all
   of it (+3.5 km bias, MAE 14.9 km). Honest heights need both the
   corrected form and a better height input.
4. **The height matters exactly where geometry says.** At range zero
   the height models are indistinguishable (the secant is 1); at
   600 km, correct daily foF2 over the too-high climatology height
   under-calls the band, and the assimilated height restores it.
   With both maps the band calls sit between 93% and 96% at every
   range in every month.
5. **The storm table adds a little beyond essn, only at mid latitudes
   and mostly in severe storms.** foF2 on storm hours (trailing-24-hour
   Kp at or above 5), essn against essn+storm, MAE in MHz:

   | month | storm n | essn | essn+storm | day corr essn → both |
   | --- | ---: | ---: | ---: | --- |
   | 2015-03 (held out) | 3006 | 0.763 | 0.727 | +0.390 → +0.403 |
   | 2022-09 (held out) | 1798 | 0.602 | 0.581 | +0.462 → +0.474 |

   (Table numbers are the embedded table as shipped — since 2026-08-15
   the whole-archive refit, whose wider held-out verdict is in
   `ionosonde.md`.) The fit months improve more but they are
   in sample and prove nothing. The held-out verdict: a real gain in
   the severe month, a small one in the moderate month, quiet hours
   untouched by construction. A first fit that also learned low-latitude bins
   gained in sample and reversed on 2015-03 (RMS 1.32 to 1.65, day
   correlation +0.390 to +0.245): the equatorial storm response turns
   on penetration-field timing a Kp class cannot carry, so `fit` holds
   the low band at the identity permanently — that exclusion is itself
   a measured result.

   In the NVIS table the full deployable pipeline (`essn+st+dud`)
   holds the essn+dudeney level: MUF MAE ticks down about 0.01 MHz in
   the held-out months and the band-call rates do not move at month
   scale, because storm hours are a minority of cells. The storm
   table's value lives in the storm-hour splits above, not in monthly
   call totals.

6. **The absorption edge tracks fmin's shape and its storm response;
   one fitted ratio closes most of its level** (fmin rows, added
   2026-08-13). With each station's constant removed, the probe edge
   sits 0.42 to 0.91 MHz from observed fmin — a fraction of fmin's
   own day-night swing, so the diurnal shape is real. The raw offset
   is large and moves month to month (+0.7 to +2.6 MHz). The
   day-conditioned edge is where the signal concentrates: in the
   severe storm month the storm-hour bias falls from +2.44 to
   +1.13 MHz, and the day-to-day correlations, though small (up to
   +0.052), are largest in the storm months. Day-level absorption
   skill beyond the diurnal shape is thin on quiet months —
   consistent with absorption being driven by the solar zenith angle
   first and activity second.

   The level calibration (`sonde --fit-edge`, 2026-08-13): the fitted
   ratio 1.6138 brings the held-out months from +1.71/1.80 and
   +2.51/2.55 bias/MAE to +0.13/0.79 and +0.71/1.11 — the moderate
   storm month lands inside the within-month offset-removed band
   (0.43 to 0.91 MHz), the severe one just above it. Two forms were
   tried against the same held-out months: per-station constants
   fitted on the fit months transfer WORSE than the one global ratio
   (MAE 1.17 and 1.69 against 1.13 and 1.64 for the additive global
   form), so the station-to-station part of the offset is not stable
   across months and is not worth carrying. What remains after the
   ratio is a residual that moves with the month — within 0.35 MHz of
   zero in six of eight months, about +0.7 MHz in the two March
   months (2025-03 in the fit, 2015-03 held out) — an equinox-shaped
   structure that a season term could carry once more months exist to
   fit it honestly.

   That refit happened (2026-08-15, the whole 2015-2026 archive;
   method and verdict in `ionosonde.md`). The single ratio turned out to
   be two real structures: the level rises with the day's index
   (about 1.3 near solar minimum, past 2.0 at maximum) and swings
   with the calendar season. The shipped level is now the fitted
   model `truecast::api::edge_fmin_ratio` (ln ratio linear in index
   plus two season harmonics), and with it the "March residual"
   above measured as mostly the index term: held-out Marches at
   minimum and maximum both land within 0.08 MHz of zero. Held-out
   MAE improves in seven of eight months; the exception is the
   2024-05 superstorm month, where storm absorption depresses the
   level in a way index and season cannot carry (roadmap).

## Caveats

- **IRTAM assimilates these same stations.** Its columns are mechanism
  proofs and upper bounds, not deployed-skill claims. The deployable
  number is the essn column, which leaves the scored station out of its
  own fit.
- MUFD came back empty from FastChar for every station; the MUF column
  waits for a DIDBGetValues fetch. The secant-derived NVIS MUF stands in
  meanwhile and is conversion-free at range zero.
- foE is day-side only (a night ionogram has no scalable E trace), and
  the overlay does not touch it — its irtam column matching climatology
  is expected, not a fault.

## The decision this supports

Build the daily conditioning into the new pipeline. The signal is real
(irtam), and enough of it survives honest holdout (essn) to remove the
moving bias and add day-level skill from live soundings alone. The
storm table rides on top as a small, safe increment: identity on quiet
days and at low latitudes, a measured gain on severe storm days at mid
latitudes. The daily-modeling measurements are done; the conditioning
(essn, Kp, the storm table) now has a proven shape for the truecast
pipeline to consume.

For the lower edge: ship it on the ionogram convention.
`truecast::api::lower_edge` answers the usable window's floor per hour —
the probe edge at the conditioning's floored index, divided by the
fitted `EDGE_FMIN_RATIO`. The held-out months put its error at 0.79
and 1.11 MHz MAE with the bias near zero outside the equinox months —
honest numbers for a window floor whose observed truth swings several
megahertz between night and noon. The known residual (about +0.7 MHz
in March months) is documented above and stays open until enough
months exist to fit a season term without memorizing the two that
showed it.

## The whole-archive refit: storm table and absorption edge

The first fits of the storm table (`src/stormfit.rs`) and the
absorption-edge level (`truecast::api`) were made on the eight original
validation months. The 2026-08 backfill put every month from 2015-01
to 2026-08 on disk, about 130 months and a full solar cycle, so both
constants were refitted on the archive (2026-08-15). This document
records the held-out design, the method, and the verdicts. Fit
programs: `sonde --fit-storm` and `sonde --fit-edge`; both print their
result as the Rust source that ships.

### The held-out set was designed before the fit

Eight months are named in the `sonde` binary (`HELD_OUT`) and are
excluded from every fit and shown in every verdict. They were chosen
by rule, from the Kp record and the solar cycle rather than by looking
at scores, so the verdict covers every stratum the models claim to
serve:

| month | why |
| --- | --- |
| 2015-03 | the original held-out pair, kept held out forever |
| 2022-09 | the original held-out pair, kept held out forever |
| 2024-05 | peak Kp 9.0, the strongest month in the whole record |
| 2018-08 | peak Kp 7.3, the only severe month of the deep minimum |
| 2019-03 | quiet March at minimum, the edge season verdict's low end |
| 2024-03 | severe March at maximum, season and severe verdict at once |
| 2020-05 | peak Kp 3.3, the quietest month in the record |
| 2024-01 | the quietest solar-maximum month, winter |

The fit set is every other month except the current live one
(2026-08): 131 months. The last observed smoothed sunspot number is
2026-01 and later months carry predicted values, but both fits are
independent of that number by construction: the daily index is
fitted from the soundings themselves, the storm ratio is observed
over that index's prediction, and Kp is observed. So those months
fit honestly. The phantom day 31 (roadmap) is filtered from fit
samples the same way `--daily` filters it.

### Storm table: refit on 1.63 million samples, shipped on provenance

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
untouched to the third decimal, so the quiet-identity contract holds.
The archive table ships because it says the same thing with far more
evidence behind every bin, not because it scores higher.

### Absorption edge: one ratio was hiding two structures

The first calibration shipped a single level, `EDGE_FMIN_RATIO =
1.6138`. Refitting it per month across the archive showed the level
is not one number: it runs about 1.3 near solar minimum and past 2.0
at solar maximum, and swings with the calendar season on top. The
"March residual" the first calibration left open measured as mostly
the index effect: a quiet March at minimum (2019-03) has no March
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


## Assimilated real-time maps, tested against measured radio


The biggest open idea was to replace VOACAP's monthly-climatology
input with IRTAM, the real-time ionosphere model that refits the global foF2
map every 15 minutes from about 50 ionosonde stations. The premise: the
engine's physics is fine, its input is blind to the actual day. This
measurement tests the premise end to end before any real-time plumbing is
built. Reproduce
with `tools/fetch-irtam.sh <month>` then `irtam_validate data/<month>`.

### Method

For each of the eight validation months, VOACAP ran twice per path: once as
shipped, and once per day with that day's archived IRTAM foF2 map written
into the run's private data tree (`src/irtam.rs` converts the published
ASCII coefficients into the binary file `redmap.for` reads; the layout was
verified value-by-value against VOACAP's own coefficient file). About
41,000 engine runs, scored against 522,000 per-day WSPR medians.

The decisive metric is day-to-day: the correlation between predicted and
observed deviations from each path-hour's monthly median. Climatology
scores exactly zero on it by construction, since every day of a month gets
the same forecast, so any positive value is information the real-time input
added. This also guards against a silent failure: if the coefficient patch
did not take, the correlation would be exactly zero, not small.

### What the eight months say

| month   | overall error, climatology → IRTAM | day-to-day correlation |
| ------- | ---------------------------------- | ---------------------: |
| 2025-06 | 5.00 → 5.00 dB                     |                 +0.146 |
| 2025-07 | 4.50 → **4.00** dB                 |                 +0.142 |
| 2025-03 | 4.00 → 4.00 dB                     |                 +0.066 |
| 2024-12 | 3.50 → 3.50 dB                     |                 +0.098 |
| 2022-09 | 4.50 → 4.50 dB                     |                 +0.110 |
| 2015-03 | 3.50 → 3.50 dB                     |                 +0.135 |
| 2019-06 | 3.50 → 3.50 dB                     |                 +0.061 |
| 2019-12 | 4.00 → 4.00 dB                     |                 +0.086 |

- **The mechanism works.** All 41,000 patched runs completed; the
  correlation is positive in every month; and it concentrates where foF2
  physics says it should: higher on the bands near the maximum usable
  frequency than on the low bands ruled by absorption (June 2025: +0.195
  against +0.071).
- **The value is small.** A correlation near +0.1 explains about one
  percent of the day-to-day variance. Overall error improves in one month
  of eight, by half a decibel.
- **Storms are the one bright spot.** In the two strong equinox storm
  months the storm-day correlation is clearly higher: +0.262 in March 2015
  (the St. Patrick's Day storm) and +0.179 in September 2022. During a
  major storm the assimilated map genuinely knows which way the ionosphere
  went.

### Why the numbers are an upper bound

The end-of-day file was used for each day, so each map was fitted from that
whole day's soundings, which is hindsight a deployed system would not have. A live
system, always holding only the trailing window, can only do the same or
worse. The measured value therefore does not understate deployment value.

Two things make that correlation look smaller than it really is. The
observed daily medians are noisy, being built from a handful of reports
per path-hour-day, and only foF2 was replaced, so the layer heights and
every other input stayed climatological. Both mean the true foF2 signal
is probably somewhat larger than +0.1. The noise applies equally to
every model compared here, though, so it doesn't favour one over
another.

### The decision this supports

**Do not build the real-time IRTAM plumbing now.** The measured day-to-day
benefit on these paths is too small to justify a second upstream
dependency, a registration with the data provider, and a per-request
coefficient pipeline. The validated corrections already shipped (sporadic-E
on, swing 0.25, spread scales, Kp storm widening) each moved measured
accuracy more than this would.

What could change the answer, in order of promise:

- **Storm-time only.** The one measured strength. A future Kp-triggered
  IRTAM mode would target exactly the days the storm widening currently
  covers statistically. Worth revisiting if users need storm-day accuracy
  rather than storm-day honesty.
- **hmF2 as well as foF2.** IRTAM also publishes layer-height maps; VOACAP
  consumes M3000F2 instead, so using them needs a conversion step the
  format check did not cover.
- **Better ground truth.** WSPR daily medians cap how much skill any model
  can show. Validation against ionosonde-measured MUFs would separate
  "IRTAM adds little" from "WSPR cannot see what it adds".
  **Now measured**, in [ionosonde.md](ionosonde.md): against ionosonde
  truth the same IRTAM input scores +0.745 day-to-day where WSPR saw
  +0.1. It was the ruler.
