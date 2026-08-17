# Live daily validation

The eight validation months are archives; this loop points the same
ruler at the present. The current month is an ordinary month bundle
(`data/YYYY-MM`) that is still filling in, so every instrument the
program built — the leave-one-station-out daily index, the storm
conditioning, the ionosonde report, the absorption edge, the truecast
API replay — runs on live data unchanged. Nothing is scored against a
special "live" path: what is tested daily is exactly what ships.

## The loop

`tools/live-check.sh`, once a day (any scheduler; the script is
self-contained and decides the month from UTC):

1. Refresh Kp (`tools/fetch-kp.sh`) and the month-to-date GIRO
   soundings. The live month's `giro/` directory is removed and
   refetched each run: GIRO revises recent scalings, so a live month
   is not append-only the way an archive month is. The politeness
   rules hold (client identified, spaced requests, about 130 requests
   per run).
2. Fetch any IRTAM daily maps that have appeared. The maps lag the
   present; a missing day is ordinary and its irtam column is empty.
3. Regather (the month's sonde cache is invalidated — the bundle
   grew), write the full report to `data/live/report-<date>.txt`, and
   append one line to `data/live/ledger.csv` (`sonde --ledger`): the
   most recent day with samples, scored on its own rows — sample
   counts, essn and climatology foF2 bias/MAE, the day's median
   fitted index, and the calibrated lower edge against fmin.
4. Replay the truecast point API against the research columns
   (`sonde --engine truecast`). This is the pass/fail gate: a nonzero
   exit from the script means the deployable API and the research
   harness disagree, or a fetch broke.

## Honesty rules

- **The newest day is partial.** During the day the ledger line
  scores the hours that exist so far; the same day firms up over
  subsequent runs. Repeated ledger lines for one day are the day
  filling in, not a fault — the `run` column says when each was
  taken.
- **The month's smoothed SSN is predicted.** R12 is a 13-month
  smooth, so a live month cannot have an observed value; SWPC's
  predicted value stands in (`src/wspr.rs`, marked in the table) and
  is replaced when the observed one is published. The daily index fit
  does not read this number — only the climatology column's own score
  does.
- **Few stations, few hours is a real state.** A thin day reports its
  small n rather than being padded; the confidence floor and the
  ±30-minute window are the same as for archive months.

## When a month ends

Its final refetch makes it an archive month like the other eight: the
`giro/` directory stops changing, the observed R12 replaces the
predicted entry when SWPC publishes it, and the month can join the
validation set (as a fit month or a held-out month — the choice is
recorded in `docs/ionosonde.md` when made). The March lower-edge
residual (`docs/roadmap.md`) is the first open question waiting on
this data.
