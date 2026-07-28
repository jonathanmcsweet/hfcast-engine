# Is a daily forecast possible?

VOACAP is monthly climatology: every day of a month gets the same answer.
Before building anything that predicts a particular day, this measures
whether the thing such a model would have to predict carries any
structure at all.

Run on 2026-07-28 over the eight validation months spanning 2015 to
2025, 150 WSPR paths each. Full output: [daily-output.md](daily-output.md).
Reproduce with `cargo run --release --bin daily`.

## Method

The residual is one number per path per day: how far that day sat from a
typical day of the month.

1. For each path and UTC hour, take the median reported SNR across the
   days of the month. That centre absorbs the two unknowns that would
   otherwise dominate — the stations' antennas and local noise, constant
   within a path, and the shape of the diurnal curve, constant within an
   hour.
2. The residual for one path-day-hour is that day's median minus the
   centre. A day needs at least 8 hours present before it is used: a day
   represented by two hours mostly reports which hours it appeared in.
3. Average the residuals over the hours of the day, giving one figure in
   dB per path per day.
4. Standardise within each path, so a path with a wide spread does not
   dominate, then correlate day *d* with day *d+k* over every pair where
   both days exist and are exactly *k* apart.

**No engine run is involved, and that is the point.** Within one month
the pipeline's prediction for a path-hour is the same number every day,
and the corrections on top of it vary with hour and path but not with
day. Subtracting a constant from a series leaves its autocorrelation
unchanged, because autocorrelation is taken about the series' own mean.
So the lag-*k* autocorrelation of the residual equals that of the
observations about their monthly centre, and the model cancels. The
result is a statement about the radio, not about this port, and it
bounds *any* daily model — learned, physical, or borrowed.

## What it found

There is real structure. Pooled over 29,668 day pairs:

| lag | correlation |
| --- | --: |
| 1 day | +0.340 |
| 2 days | +0.172 |
| 3 days | +0.080 |

Every month agrees, from +0.275 to +0.412, so this is not one month's
accident. The decay is close to halving per day: a memory of about a day,
slightly longer than a pure one-day process would give.

Disturbed days carry more of it than quiet ones — lag 1 of +0.424 after a
day reaching Kp 5, against +0.328 after a quiet day, and +0.726 in
2022-09. That is the expected shape, since recurrent solar wind streams
and storm recovery persist across days while quiet-day variation does
not.

## The reading

**The signal is real and too small to be worth predicting for its own
sake.**

A correlation of 0.340 explains 11.6% of the daily variance, which leaves
94% of the deviation standing. The typical daily residual is 1.29 dB, so
a *perfect* next-day predictor built on this correlation would shrink it
to 1.21 dB — a gain of 0.08 dB. For comparison, the port's whole
tolerance envelope is 1 dB of SNR, and the correction constants already
shipped address errors an order of magnitude larger.

So the honest conclusion is not the one this measurement was set up to
find. It is not "no daily structure exists" — there plainly is some, and
consistently. It is that the coherent day-level component of these paths
is about a decibel, and a tenth of a decibel of it is recoverable from
yesterday. Nobody would notice.

Two things follow, and they point in opposite directions from a
median-focused daily model:

- **The disturbed-day result is the useful part.** A correlation of
  +0.424 on days after Kp 5, reaching +0.726 in a storm month, is
  materially stronger than the quiet-day figure, and it lands exactly
  where the shipped storm-spread widening already operates. That
  supports widening uncertainty from a Kp forecast — which is an
  existing roadmap item — rather than shifting the median.
- **Some individual paths are far more predictable than the pool.**
  Ten paths exceed lag-1 of +0.86. If a daily model is ever attempted,
  those are where it should be tested first, though with the caveat below
  that a high per-path figure is also what a station changing its own
  setup for a few days would produce.

## What would deflate or inflate this number

**Deflating: WSPR noise.** A daily median from a few hundred reports
carries its own error, and measurement noise pushes a correlation toward
zero. So +0.340 is a floor for the true day-to-day persistence, and a
low figure here would read as "not visible with this ruler" rather than
"absent". Separating the two needs absolute ground truth, which is what
the ionosonde-grade validation item in [roadmap.md](roadmap.md) is for.

**Inflating: station behaviour, not ionosphere.** This measurement cannot
tell ionospheric persistence from station persistence. A receiver whose
local noise floor rises for three days, an antenna left in the wrong
position over a weekend, or a run of days with fewer reporting stations
would all produce exactly this autocorrelation without any propagation
content. That makes +0.340 an *upper* bound on the ionospheric signal as
well as a lower bound on the measured one — the two caveats work in
opposite directions, and neither is resolved here.

Taken together: the true recoverable gain is bounded above by something
close to the 0.08 dB computed above, and is plausibly less.
