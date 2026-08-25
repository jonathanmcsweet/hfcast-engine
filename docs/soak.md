# The recurring daily checks

Two jobs run every day. The parity soak proves HFcast Compatible still
matches the Fortran on live space weather; the live validation loop
points the accuracy harness at the current month. They are separate jobs
with separate failure modes, described in turn.

## The parity soak

`paritycheck` already shows that HFcast Compatible and the Fortran
reference `voacapl` return the same numbers, over 8 request shapes on
one machine at one moment. The soak turns that into a claim that holds
over time, over inputs nobody chose in advance, and on a machine nobody
configured by hand.

This file records what the soak must show **before** it starts, so the
verdict at the end is a check against criteria rather than a judgement
made once the numbers are in.

### What runs

`.github/workflows/soak.yml`. Three events start it, and only the first
writes the record:

- **The schedule**, daily at 06:00 UTC. This is the soak itself. It is
  the only run that appends to `log.tsv`, so the record stays one line
  per day.
- **A push to `main`.** The same sweep, against that day's numbers, so a
  change is measured on real inputs without waiting for the next
  morning. It records nothing; a failing sweep keeps its evidence as a
  run artefact for 30 days.
- **A manual dispatch**, which is how a release is checked. Same rules
  as a push.

### The badge

The README badge reports the verdict of the last sweep and nothing else.
A clean sweep turns it green, with the date it was measured. Differing
fields turn it red, with the count. A run that could not measure,
whether NOAA was unreachable, the reference failed to build, or the
runner faulted, does not move the badge, because on that run HFcast
Compatible was never compared against the reference. Such a run still
fails in the Actions tab and still notifies; only the badge stays quiet.
The date in the green badge is what shows staleness: if it falls behind,
the measurements themselves have been failing to run.

On the scheduled run:

1. Builds `voacapl` from a pinned commit of `jawatson/voacapl`, with the
   runner's `gfortran`. 2. Builds HFcast Compatible. 3. Fetches that
   day's F10.7 flux and planetary K index from NOAA SWPC. Both fetches
   are fatal if they fail. A day that quietly ran on a canned value
   would look live and would prove nothing. 4. Derives the sunspot
   number from the flux with `spacewx`, and runs the 200 paths in
   `soak-paths.tsv` through HFcast Compatible and the reference with
   that day's month, year and sunspot number. 5. Appends one line to
   `log.tsv` on the `soak-results` branch, and commits the deck and both
   outputs for any case that disagreed. 6. Fails the workflow, and so
   notifies, if anything disagreed or could not run.

Each path compares 888 fields: reliability, SNR, both SNR deciles and
the MUF, over 24 hours and 9 bands. A day is therefore about 177,600
fields.

### Why the inputs move

The corpus holds the geography still and lets the live inputs move. Over
a month that samples what a fixed corpus cannot:

- **Space weather.** Each day's flux gives a different sunspot number,
  and the model's behaviour is not linear in it.
- **The calendar.** As the run date walks forward, the month changes and
  with it the coefficient set the engine interpolates.
- **Geomagnetic state.** Storms happen when they happen. The K index is
  recorded with each day so the record can say how many disturbed days
  were covered rather than assume.

### The paths

200 paths, generated once and checked in, spanning the regimes the model
behaves differently in:

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

### Exit criteria

| Criterion         | Required                                            |
| ----------------- | --------------------------------------------------- |
| Duration          | 28 consecutive days minimum                        |
| Clean quiet days  | at least 20                                        |
| Clean disturbed days | at least 1, with `kpmax24h` at or above 5       |
| Differing fields  | zero, on every day                                 |
| Days that failed to run | zero, excluding days a fetch or build broke  |

**Zero is the only acceptable difference.** `paritycheck` already
reaches zero offline, so anything else is a finding rather than noise to
be tolerated.

If a day differs: the run is not a failed soak, it is a defect found by
the mechanism built to find it. The dumped case becomes a `portcheck`
case, the cause is fixed, and **the 28-day clock restarts**. A soak that
counted "mostly clean" would answer a question nobody asked.

If 28 days pass with no disturbed day, the choice is to extend until one
occurs or to state the gap plainly in the verdict. It is not to quietly
count the soak as complete.

### What the soak cannot show

Worth stating, because a clean record is easy to over-read:

- It compares HFcast Compatible against `voacapl`, so it can only show
  that the two agree. Neither is measured against reality here; that is
  what [accuracy.md](accuracy.md) is for.
- It covers the fields an application reads and the request shape it
  sends: method 30, isotropes at both ends, sporadic E on, CCIR, nine
  amateur bands. Other card methods and antenna types are covered by
  `portcheck`, `fuzz` and `antcheck` instead, which do not run daily.
- Both run on the same runner with the same `gfortran`, so the agreement
  is internally consistent by construction. If a difference ever
  appears, the first question is whether the runner's compiler changed,
  which is why the `gfortran` version and the pinned `voacapl` commit
  are recorded on every line.

### The record

Branch `soak-results`:

- `log.tsv`: one line per day, giving date, paths, fields, differing,
  verdict, F10.7, sunspot number, Kp, 24-hour Kp maximum, `gfortran`
  version, `voacapl` commit, run id.
- `reports/<date>.md`: that day's full per-case table.
- `dumps/<date>/<case>/`: only for a case that disagreed, holding the
  deck, the Fortran listing, the request JSON and the Rust output.

## Live daily validation

The eight validation months are archives; this loop points the same
ruler at the present. The current month is an ordinary month bundle
(`data/YYYY-MM`) that is still filling in, so every instrument the
program built runs on live data unchanged: the leave-one-station-out
daily index, the storm conditioning, the ionosonde report, the
absorption edge, and the truecast API replay. Nothing is scored against
a special "live" path, so what is tested daily is exactly what ships.

### The loop

`tools/live-check.sh`, once a day (any scheduler; the script is
self-contained and decides the month from UTC):

1. Refresh Kp (`tools/fetch-kp.sh`) and the month-to-date GIRO
   soundings. The live month's `giro/` directory is removed and
   refetched each run: GIRO revises recent scalings, so a live month is
   not append-only the way an archive month is. The politeness rules
   hold (client identified, spaced requests, about 130 requests per
   run). 2. Fetch any IRTAM daily maps that have appeared. The maps lag
   the present; a missing day is ordinary and its irtam column is empty.
   3. Regather (the month's sonde cache is invalidated, because the
   bundle grew), write the full report to `data/live/report-<date>.txt`,
   and append one line to `data/live/ledger.csv` (`sonde --ledger`): the
   most recent day with samples, scored on its own rows, giving sample
   counts, essn and climatology foF2 bias/MAE, the day's median fitted
   index, and the calibrated lower edge against fmin. 4. Replay the
   truecast point API against the research columns (`sonde --engine
   truecast`). This is the pass/fail gate: a nonzero exit from the
   script means the deployable API and the research harness disagree, or
   a fetch broke.

### Honesty rules

- **The newest day is partial.** During the day the ledger line scores
  the hours that exist so far; the same day firms up over subsequent
  runs. Repeated ledger lines for one day are the day filling in, not a
  fault, and the `run` column says when each was taken.
- **The month's smoothed SSN is predicted.** R12 is a 13-month smooth,
  so a live month cannot have an observed value; SWPC's predicted value
  stands in (`src/wspr.rs`, marked in the table) and is replaced when
  the observed one is published. The daily index fit does not read this
  number. Only the climatology column's own score does.
- **Few stations, few hours is a real state.** A thin day reports its
  small n rather than being padded; the confidence floor and the
  ±30-minute window are the same as for archive months.

### When a month ends

Its final refetch makes it an archive month like the other eight: the
`giro/` directory stops changing, the observed R12 replaces the
predicted entry when SWPC publishes it, and the month can join the
validation set, either as a fit month or a held-out month, and that
choice is recorded in `docs/ionosonde.md` when it is made. The March
lower-edge residual (`docs/roadmap.md`) is the first open question
waiting on this data.
