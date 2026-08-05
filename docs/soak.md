# The parity soak

`paritycheck` already shows that the Rust engine and `voacapl` return
the same numbers, over 8 request shapes on one machine at one moment.
The soak turns that into a claim that holds over time, over inputs
nobody chose in advance, and on a machine nobody configured by hand.

This file records what the soak must show **before** it starts, so the
verdict at the end is a check against criteria rather than a
judgement made once the numbers are in.

## What runs

`.github/workflows/soak.yml`. Three events start it, and only the
first writes the record:

- **The schedule**, daily at 06:00 UTC. This is the soak itself. It is
  the only run that appends to `log.tsv`, so the record stays one line
  per day.
- **A push to `main`.** The same sweep, against that day's numbers, so
  a change is measured on real inputs without waiting for the next
  morning. It records nothing; a failing sweep keeps its evidence as a
  run artefact for 30 days.
- **A manual dispatch**, which is how a release is checked. Same rules
  as a push.

## The badge

The README badge reports the verdict of the last sweep and nothing
else. A clean sweep turns it green, with the date it was measured.
Differing fields turn it red, with the count. A run that could not
measure — NOAA unreachable, the reference failing to build, a runner
fault — does not move the badge, because on that run the engine was
not compared. Such a run still fails in the Actions tab and still
notifies; only the badge stays quiet. The date in the green badge is
what shows staleness: if it falls behind, the measurements themselves
have been failing to run.

On the scheduled run:

1. Builds `voacapl` from a pinned commit of `jawatson/voacapl`, with
   the runner's `gfortran`.
2. Builds the Rust engine.
3. Fetches that day's F10.7 flux and planetary K index from NOAA SWPC.
   Both fetches are fatal if they fail. A day that quietly ran on a
   canned value would look live and would prove nothing.
4. Derives the sunspot number from the flux with `spacewx`, and runs
   the 200 paths in `soak-paths.tsv` through both engines with that
   day's month, year and sunspot number.
5. Appends one line to `log.tsv` on the `soak-results` branch, and
   commits the deck and both outputs for any case that disagreed.
6. Fails the workflow, and so notifies, if anything disagreed or could
   not run.

Each path compares 888 fields — reliability, SNR, both SNR deciles and
the MUF, over 24 hours and 9 bands — so a day is about 177,600 fields.

## Why the inputs move

The corpus holds the geography still and lets the live inputs move.
Over a month that samples what a fixed corpus cannot:

- **Space weather.** Each day's flux gives a different sunspot number,
  and the model's behaviour is not linear in it.
- **The calendar.** As the run date walks forward, the month changes
  and with it the coefficient set the engine interpolates.
- **Geomagnetic state.** Storms happen when they happen. The K index
  is recorded with each day so the record can say how many disturbed
  days were covered rather than assume.

## The paths

200 paths, generated once and checked in, spanning the regimes the
model behaves differently in:

| Regime            | Paths |
| ----------------- | ----: |
| Equatorial        |    42 |
| High latitude     |    44 |
| Polar             |    31 |
| Mid latitude      |    32 |
| Trans-equatorial  |    30 |
| Antipodal         |    11 |
| Short             |    10 |

Lengths run from 195 km to 19,353 km. Within each regime the paths are
spread across distance buckets, so a regime is not represented by
several variations of the same hop length.

## Exit criteria

| Criterion         | Required                                            |
| ----------------- | --------------------------------------------------- |
| Duration          | 28 consecutive days minimum                        |
| Clean quiet days  | at least 20                                        |
| Clean disturbed days | at least 1, with `kpmax24h` at or above 5       |
| Differing fields  | zero, on every day                                 |
| Days that failed to run | zero, excluding days a fetch or build broke  |

**Zero is the only acceptable difference.** `paritycheck` already
reaches zero offline, so anything else is a finding rather than noise
to be tolerated.

If a day differs: the run is not a failed soak, it is a defect found by
the mechanism built to find it. The dumped case becomes a `portcheck`
case, the cause is fixed, and **the 28-day clock restarts**. A soak
that counted "mostly clean" would answer a question nobody asked.

If 28 days pass with no disturbed day, the choice is to extend until
one occurs or to state the gap plainly in the verdict. It is not to
quietly count the soak as complete.

## What the soak cannot show

Worth stating, because a clean record is easy to over-read:

- It compares this engine against `voacapl`, so it can only show they
  agree. Neither is measured against reality here; that is what
  [accuracy.md](accuracy.md) is for.
- It covers the fields an application reads and the request shape it
  sends: method 30, isotropes at both ends, sporadic E on, CCIR, nine
  amateur bands. Other card methods and antenna types are covered by
  `portcheck`, `fuzz` and `antcheck` instead, which do not run daily.
- Both engines run on the same runner with the same `gfortran`, so the
  agreement is internally consistent by construction. If a difference
  ever appears, the first question is whether the runner's compiler
  changed — which is why the `gfortran` version and the pinned
  `voacapl` commit are recorded on every line.

## The record

Branch `soak-results`:

- `log.tsv` — one line per day: date, paths, fields, differing,
  verdict, F10.7, sunspot number, Kp, 24-hour Kp maximum, `gfortran`
  version, `voacapl` commit, run id.
- `reports/<date>.md` — that day's full per-case table.
- `dumps/<date>/<case>/` — only for a case that disagreed: the deck,
  the Fortran listing, the request JSON and the Rust output.
