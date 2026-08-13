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

## What replaces the skeleton

The inner physics will be re-formed for batches (structure-of-arrays,
threads inside the engine, then GPU kernels — see the roadmap). The
API, the conditioning, and the two rulers stay: the ionosonde harness
for accuracy, and this verification mode as the equivalence envelope
between the research columns and whatever computes the answers next.
