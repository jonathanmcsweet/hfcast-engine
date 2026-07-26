# Real-time ionosphere data, tested against measured radio

The roadmap's biggest open idea was to replace VOACAP's monthly-climatology
input with IRTAM, the real-time ionosphere model that refits the global foF2
map every 15 minutes from about 50 ionosonde stations. The premise: the
engine's physics is fine, its input is blind to the actual day. This
measurement tests the premise end to end before any real-time plumbing is
built. Full program output: [irtam-output.md](irtam-output.md). Reproduce
with `tools/fetch-irtam.sh <month>` then `irtam_validate data/<month>`.

## Method

For each of the eight validation months, VOACAP ran twice per path: once as
shipped, and once per day with that day's archived IRTAM foF2 map written
into the run's private data tree (`src/irtam.rs` converts the published
ASCII coefficients into the binary file `redmap.for` reads; the layout was
verified value-by-value against VOACAP's own coefficient file). About
41,000 engine runs, scored against 522,000 per-day WSPR medians.

The decisive metric is day-to-day: the correlation between predicted and
observed deviations from each path-hour's monthly median. Climatology
scores exactly zero on it by construction — every day of a month gets the
same forecast — so any positive value is information the real-time input
added. This also guards against a silent failure: if the coefficient patch
did not take, the correlation would be exactly zero, not small.

## What the eight months say

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
  physics says it should — higher on the bands near the maximum usable
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

## Why the numbers are an upper bound

The end-of-day file was used for each day, so each map was fitted from that
whole day's soundings — hindsight a deployed now-cast would not have. A live
system, always holding only the trailing window, can only do the same or
worse. The measured value therefore does not understate deployment value.

Two honest attenuators of the correlation: the observed daily medians are
themselves noisy (a handful of reports per path-hour-day), and only foF2 was
replaced — the layer heights and every other input stayed climatological.
Both mean the true foF2 signal may be somewhat larger than +0.1, but the
first applies equally to every model compared here.

## The decision this supports

**Do not build the real-time IRTAM plumbing now.** The measured day-to-day
benefit on these paths is too small to justify a second upstream
dependency, a registration with the data provider, and a per-request
coefficient pipeline. The validated corrections already shipped (sporadic-E
on, swing 0.25, spread scales, Kp storm widening) each moved measured
accuracy more than this would.

What could change the answer, in order of promise:

- **Storm-time only.** The one measured strength. A future Kp-triggered
  IRTAM mode would target exactly the days the storm widening currently
  covers statistically — worth revisiting if users need storm-day accuracy
  rather than storm-day honesty.
- **hmF2 as well as foF2.** IRTAM also publishes layer-height maps; VOACAP
  consumes M3000F2 instead, so using them needs a conversion step the
  format check did not cover.
- **Better ground truth.** WSPR daily medians cap how much skill any model
  can show. Validation against ionosonde-measured MUFs would separate
  "IRTAM adds little" from "WSPR cannot see what it adds".
