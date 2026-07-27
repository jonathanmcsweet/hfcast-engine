# The corrected tier — what each fix changes, and what it measured

`Model::Corrected` fixes VOACAP's documented defects. This file records,
per fix, what it moves and whether it makes predictions better. Both
questions are needed: "it is a defect" does not imply "fixing it
improves the forecast", because VOACAP's empirical constants were
fitted with the defects present, so a defect can be load-bearing.

Two measurements per fix:

- **What moved.** `correctcheck --fix NAME` runs a corpus twice, once
  compatible and once with only that fix on, and reports which printed
  cells differ.
- **Whether it helped.** `validate --fix NAME` scores the ported engine
  against measured WSPR reception across the eight validation months,
  beside `validate --ported` as the control. Same engine both sides, so
  the comparison is one fix rather than a fix plus a change of engine.

## Status

| fix                 | implemented | what moved                        | accuracy effect          |
| ------------------- | ----------- | --------------------------------- | ------------------------ |
| `pole_file`         | yes         | 2.6% of cells, all 96 sweep cases | none measurable; kept on |
| `curtain_elevation` | no          | —                                 | —                        |
| `luf_scan_best`     | no          | —                                 | —                        |
| `luf_pass_area`     | no          | —                                 | —                        |
| `area_centre_nudge` | no          | —                                 | —                        |
| `area_antenna_end`  | no          | —                                 | —                        |

## `pole_file` — the magnetic pole database file is read

**The defect.** `MagneticPole::for_tree` builds the database path
without a separator, so the reference looks for
`<tree>database/north_pole.txt` and never finds the installed
`<tree>/database/north_pole.txt`. Every run therefore uses the built-in
pole at 78.5 N, and the file the distribution ships — which exists
precisely so the pole can be moved, and carries a page of
correspondence about doing so — has no effect. The installed file says
79.5 N, so the fix moves the geomagnetic pole one degree.

A `run/north_pole.txt` is read either way, so the fix only changes runs
whose tree has a database file and no run file. That is the stock
installation.

**What moved.** All 96 sweep cases touched; 12,807 of 486,144 printed
cells, 2.6%, with no structural change — no cell appeared or vanished.

| row    | cells moved | worst change |
| ------ | ----------: | -----------: |
| V HITE |        3353 |       331.00 |
| SIG LW |        1569 |        24.00 |
| SIG UP |        1044 |        23.20 |
| SNR LW |         944 |        17.10 |
| SNR UP |         835 |        19.40 |
| LOSS   |         642 |        45.00 |
| S DBW  |         642 |        45.00 |
| SNR    |         634 |        46.00 |
| TANGLE |         633 |        29.00 |
| REL    |         112 |         0.14 |
| MODE   |          13 |            — |

So it is not cosmetic: a virtual height moves by up to 331 km, an SNR
by up to 46 dB, and thirteen cells change which propagation mode
dominates. Those extremes are high-latitude paths, where a one-degree
pole shift moves the geomagnetic latitude most.

**Whether it helped: no measurable difference.** Eight months, about
26,000 path-hours, ported engine both sides.

| month   | median error  | RMS           | correlation   | slope       | after gain fit |
| ------- | ------------- | ------------- | ------------- | ----------- | -------------- |
| 2015-03 | 3.8 → **3.5** | 11.3 → 11.3   | +0.44 → +0.43 | 0.20 → 0.20 | 2.1 → 2.1      |
| 2019-06 | 2.0 → 2.0     | 4.1 → 4.1     | +0.43 → +0.43 | 0.33 → 0.33 | 1.6 → 1.6      |
| 2019-12 | 3.0 → 3.0     | 7.0 → 7.0     | +0.52 → +0.52 | 0.27 → 0.27 | 1.7 → 1.7      |
| 2022-09 | 3.5 → **3.2** | 8.7 → 8.7     | +0.66 → +0.65 | 0.27 → 0.27 | 1.5 → 1.5      |
| 2024-12 | 3.0 → 3.0     | 12.0 → 12.0   | +0.67 → +0.67 | 0.33 → 0.34 | 2.2 → 2.2      |
| 2025-03 | 3.5 → 3.5     | 11.3 → 11.3   | +0.50 → +0.50 | 0.20 → 0.20 | 1.7 → 1.7      |
| 2025-06 | 3.0 → 3.0     | 8.6 → 8.6     | +0.71 → +0.71 | 0.31 → 0.30 | 1.5 → 1.5      |
| 2025-07 | 3.0 → 3.0     | 7.9 → **7.8** | +0.72 → +0.72 | 0.36 → 0.36 | 1.7 → **1.6**  |

Median error improves 0.3 dB in two months and is unchanged in six.
Correlation falls 0.01 in two months. Everything else is flat. Nothing
here supports a claim in either direction.

The reason is the path population, not the fix: WSPR paths are
overwhelmingly mid-latitude, where a one-degree pole shift barely moves
the geomagnetic latitude. The cells that moved most are polar, and the
validation corpus has almost none.

**Decision: kept on.** It does not measure worse, and it is what the
program intended — the file exists to be read, and with the defect in
place a user editing it gets silence. Anyone wanting the old pole can
still have it, either through `Model::Compatible` or by putting the
value in `run/north_pole.txt`, which is read on both tiers.

## The other five fixes need their own corpora

Not yet implemented, and worth recording why the obvious measurement
will not serve them. Each lives on a code path the sweep corpus and the
WSPR validation cannot reach, because both are method 30,
point-to-point, isotropic:

- `curtain_elevation` is inside `ioncap.rs` at `KOP = 6`, reached only
  by a curtain antenna. Every sweep case and every WSPR run uses
  `default/isotrope`.
- `luf_scan_best` and `luf_pass_area` are inside `luffy_luf`, reached
  only through `run_luf` — card methods 26 to 29. The app and the
  corpus use method 30.
- `area_centre_nudge` and `area_antenna_end` are area-coverage only.

So `correctcheck` over the sweep corpus reports zero movement for all
five whether or not they are implemented, which is a measurement that
proves nothing. Each needs a corpus that reaches its site: curtain
antenna cases, method-26 decks, and area grids respectively.

The WSPR pipeline cannot score any of them at all. It measures
point-to-point systems predictions against beacon reports; there is no
measured ground truth here for area coverage, for the LUF, or for a
curtain antenna's pattern. For those, "what moved" plus a reading of
the source is the whole of the available evidence, and that limit
should be stated rather than papered over with a number that came from
somewhere else.
