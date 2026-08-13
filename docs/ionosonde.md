# Predictions against ionosonde truth

`docs/irtam.md` closed with three ways its answer could change, and the
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

## What one month says (2025-06, 15 stations, 26,396 samples)

| quantity | climatology | with the day's IRTAM maps |
| --- | --- | --- |
| foF2 bias / MAE | +0.74 / 0.91 MHz | **-0.01 / 0.36 MHz** |
| foF2 storm-day bias | +1.22 MHz | +0.09 MHz |
| foF2 day-to-day correlation | +0.000 (guard) | **+0.745** |
| hmF2 bias / MAE | +61.5 / 62.2 km | **+3.5 / 14.9 km** |
| hmF2 day-to-day correlation | +0.000 (guard) | +0.533 |
| NVIS band calls right, overhead | 86.6% | 94.4% |
| NVIS MUF(600 km) MAE | 0.96 MHz | **0.54 MHz** (both maps) |

Four findings, in order of consequence:

1. **The WSPR ruler was the limit, as suspected.** The same IRTAM input
   that scored +0.1 day-to-day against WSPR medians scores +0.745
   against ionosonde truth. The assimilated map does know what the
   ionosphere did that day; WSPR could not see it. (One caveat below.)
2. **Climatology ran high in 2025-06.** +0.74 MHz median foF2 bias at
   these stations, rising to +1.22 MHz on storm days. This is the
   error a per-day effective index and a storm mode exist to remove.
3. **The +61 km height bias decomposes.** The corrected Dudeney form
   over climatology's own inputs removes about 19 km (bias +42 km);
   the rest is the M(3000)F2 input itself. IRTAM's assimilated height
   map removes nearly all of it (+3.5 km). An engine that wants honest
   heights needs both the corrected form and a better height input.
4. **The height matters exactly where geometry says.** At range zero
   the height models are indistinguishable (the secant is 1); at
   600 km, correct daily foF2 over the too-high climatology height
   under-calls the band (-0.84 MHz bias), and adding the assimilated
   height brings the error to -0.10 MHz and band calls to 94.2%.

## Caveats

- **IRTAM assimilates these same stations.** Its columns are mechanism
  proofs and upper bounds, not deployed-skill claims. Deployable skill
  needs the leave-one-station-out effective-index fit, which is the next
  phase's work.
- One month so far. The other seven validation months score the same
  way once their bundles are fetched.
- MUFD came back empty from FastChar for every station; the MUF column
  waits for a DIDBGetValues fetch. The secant-derived NVIS MUF stands in
  meanwhile and is conversion-free at range zero.
- foE is day-side only (a night ionogram has no scalable E trace), and
  the overlay does not touch it — its irtam column matching climatology
  is expected, not a fault.

## The decision this supports

Build the daily conditioning. The ionosonde ruler shows real, large
day-level structure that the engine's monthly input misses and that an
assimilated daily input captures; the open question is no longer
"is there signal" but "how much survives honest holdout" — which is
exactly what the effective-index and storm-mode phases measure next.
