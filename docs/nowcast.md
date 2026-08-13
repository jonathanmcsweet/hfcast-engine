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

Measured with `gridbench` on the application's fine-globe lattice
(34,560 points, one band, 16-core container, 2026-08-13):

| driver | wall |
| --- | ---: |
| parity `run_area`, serial | 1088 ms |
| `predict_grid`, 1 thread | 1038 ms |
| `predict_grid`, 4 threads | 266 ms (3.9x) |
| `predict_grid`, 8 threads | 142 ms (7.3x) |

The scaling is near linear to eight threads — the engine parallelizes;
the application's strip sharding and JSON rendering were the loss. The
per-point physics is unchanged (`portcheck`: 23,040 cells, zero drift
after the seam).

## What replaces the skeleton

The inner physics will be re-formed for batches next (the
structure-of-arrays ionosphere pass — the measured 47% — then the
packed HFB1 answer format, then GPU kernels; see the roadmap). The
API, the conditioning, and the two rulers stay: the ionosonde harness
for accuracy, and the verification mode plus the grid tests as the
equivalence envelope between the research columns and whatever
computes the answers next.
