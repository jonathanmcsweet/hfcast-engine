# The nowcast pipeline

`src/nowcast/` is the second prediction pipeline, beside the parity
port. The parity engine's contract is byte-identical agreement with the
Fortran reference, which is why it cannot become more accurate: every
day of a month gets the same answer. The nowcast contract is different:
a change ships when the measured error against ionosonde truth goes
down (`docs/ionosonde.md`), and the parity engine does not move.

## The point API

`nowcast::api::point` (and `day`, its 24-hour form) answers "what is
the ionosphere over this point" in the ionosonde's own conventions:

- ordinary-wave foF2 in MHz (the raw engine value carries half the
  gyrofrequency; the harness measured the difference at ~0.55 MHz),
- foE in MHz,
- hmF2 in km through Dudeney's corrected form (about 19 km of the
  measured +61 km height bias is the plain `1490/M - 176`),
- the M(3000)F2 factor, and MUF by range through the mirror-geometry
  secant (`muf_at`, `muf3000_mhz`).

The conditioning input is the day knob:

- `Conditioning::Climatology { ssn }` — the engine as shipped, at the
  month's smoothed sunspot number.
- `Conditioning::Daily { essn, kp_max24 }` — a daily index fitted from
  live soundings (`src/essn.rs`), plus the trailing-24-hour Kp maximum
  per UT hour. Where Kp is known, foF2 is multiplied by the embedded
  storm ratio (`src/stormfit.rs`); a missing hour gets the identity,
  which is exactly what a device without the feed can honestly do.
  An index below zero is floored for every channel except foF2, which
  follows the fitted line wherever the fit put it: below the map's
  zero-sunspot plane there is no measured state for foE, absorption or
  noise to extrapolate into, and the link study measured that
  extrapolation as the whole solar-minimum cost (`docs/essn-wspr.md`).

Both conditionings were scored held-out against ionosonde truth before
this API existed; the numbers live in `docs/ionosonde.md`. The skeleton
computes through the parity engine's own physics (`probe_hours`), so
the answers inherit the port's correctness while the corrections ride
on top.

## The verification: the API cannot drift from the measurements

`sonde --engine nowcast` replays the point API over every cached cell
of the validation months and compares it with the research columns the
accuracy claims rest on. Over all eight months (2026-08-13):

| comparison | worst disagreement | tolerance |
| --- | ---: | ---: |
| climatology foF2 vs the climatology column | 0.00005 MHz | 0.001 |
| climatology foE vs the climatology column | 0.00005 MHz | 0.001 |
| climatology hmF2 vs the dudeney column | 0.00005 km | 0.001 |
| daily foF2 vs the essn+storm column | 0.028 MHz | 0.05 |

The climatology rows agree to cache rounding: same engine run on both
sides. The daily row crosses two f32 rounding paths — the research
column interpolates the answer line between the two sunspot planes,
the API blends coefficients and then evaluates — which differ most
where the harmonic series cancels at night. The faults the check
exists for are an order larger (a wrong storm bin: ~0.25 MHz on storm
hours; a shifted hour: ~1 MHz), so the mode fails the build well
before a real plumbing error could hide.

## The grid driver

`nowcast::grid::predict_grid` runs one coverage lattice with threads
inside the engine: one shared read-only coefficient set and area setup,
workers claiming rows off a shared cursor, output as structure-of-arrays
`f32` planes (reliability, SNR, takeoff angle). Two properties are
tests, not intentions:

- **Thread counts cannot move the answer.** Every point is computed
  with fresh state — a pure function of the place and hour — and the
  planes are assembled by row index. One thread and three threads
  produce bit-identical planes.
- **The parity anchor holds.** The parity area driver carries COMMON
  state from point to point because the Fortran does. Its first point
  has no carry yet and must match the fresh-state driver exactly; over
  a test lattice, the carry moved reliability by at most 0.011 and SNR
  by at most 2 dB — the class of spread the -ffast-math study measured,
  not signal.

The driver shares everything a grid point cannot change. Beyond the
coefficient set, the antennas and the magnetic pole (shared since the
driver landed), the whole lattice now reads one `COFION` flattening of
the maps and one `VIRTIM` evaluation of the diurnal series — both are
functions of the maps and the hour alone, and both were previously
recomputed at every point. The per-point values are bit-identical
either way; `portcheck` (23,040 cells) stays zero-drift.

Measured with `gridbench` on the application's fine-globe lattice
(34,560 points, one band, 16-core container, 2026-08-13):

| driver | wall |
| --- | ---: |
| parity `run_area`, serial | 1088 ms |
| `predict_grid`, 1 thread | 970 ms |
| `predict_grid`, 4 threads | 249 ms (3.9x) |
| `predict_grid`, 8 threads | 131 ms (7.4x) |

The scaling is near linear to eight threads — the engine parallelizes;
the application's strip sharding and JSON rendering were the loss.
With `HFCAST_PERF` set, `gridbench` prints the per-stage table
(`--parity 0` scopes it to the nowcast driver). Where the remaining
time goes, single thread: the 30-point ionogram and its profile walks
(`genion`/`gethp`) 34%, the systems frequency loop 28%, the per-point
layer parameters (`versy` geography) 18%, per-point setup (geometry,
magnetic field, ground constants) about 10%.

## The packed answer

`nowcast::packed` is HFB1: the grid planes as one little-endian byte
body — a 48-byte header (lattice, counts), the frequencies, then the
reliability, SNR and takeoff planes as raw `f32`, each starting 4-byte
aligned so a JavaScript consumer can view them as `Float32Array`
without parsing. A one-band fine globe is about 405 KB against the
2.2 MB JSON crossing and its 34,560 objects. Point coordinates are
derived from the header, not stored. The encoder and decoder round-trip
bit-exactly including NaN; the JNI and application side of the crossing
is parked with the rest of the app work.

## The lower edge

`nowcast::api::lower_edge` answers the usable window's floor per UT
hour on the ionogram's fmin convention: the absorption-edge probe
(`probe_edge` — a Systems run over a ten-rung ladder, the lowest
frequency within 6 dB of the hour's own SNR plateau) at the
conditioning's floored index, divided by the fitted level
`EDGE_FMIN_RATIO`. `None` is a real answer: the whole ladder sits
within the drop — the night state, where a sounder's fmin is its
instrument floor too. The fit, the held-out verdict (0.79 and
1.11 MHz MAE) and the known March residual are in
`docs/ionosonde.md`; the fit reruns with `sonde --fit-edge`.

## The service selector

The JSON service (`src/service.rs`, the whole application boundary)
takes an `"engine"` field: `"voacap"` — the default, so every existing
request predicts exactly what it always did — or `"nowcast"`, which
runs the same physics conditioned on the fitted daily index. A nowcast
request states `"essn"` in place of `"ssn"` (both at once is refused),
and the run applies the floor: the engine never goes below the map's
zero-sunspot plane, and below it a synthesized coefficient overlay
holds foF2 on the fitted line. The below-floor synthesis needs a
writable `"workDir"` and the compiled-in root; a caller with its own
overlay directory writes `coeffs/fof2CCIR.daw` there itself. Every
answer — point-to-point and area — carries `"engine"` naming the model
behind it, which is the seam an application needs to offer the model
as a user preference. The storm ratio stays at the characteristics
level where it was fitted and scored; no seam carries a per-place,
per-hour foF2 ratio into a listing run yet.

## One physics, by decision

The original batch plan imagined the nowcast pipeline growing its own
structure-of-arrays physics, equivalent to the parity engine within a
tolerance envelope. That plan is closed (2026-08-13, measured): the
pipeline computes through the parity engine's own functions, restructured
only in ways that cannot move a bit — shared evaluations of
position-independent work, and block layouts of independent arithmetic.
Two reasons, in order:

1. The service now proves `"engine":"nowcast"` at an index at or above
   zero answers exactly as the parity engine at that number. A CPU pass
   that drifted within a tolerance would break that proof and put two
   sets of physics behind one API.
2. The measured headroom did not justify the fork. After the shared
   evaluations, the fine globe runs 970 ms single-thread / 131 ms at
   eight threads, and the remainder is pointwise physics that is
   memory- and branch-bound, not arithmetic-bound — restructuring it
   without moving bits was measured at zero gain. The batch plan's
   80 ms target assumed arithmetic the vectorizer could recover;
   there is none to recover. That memory-bound remainder is the
   strongest case the GPU phase has: wide offload, not CPU lanes, is
   where the next factor lives.
