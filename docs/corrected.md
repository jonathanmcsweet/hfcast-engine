# The corrected tier — what each fix changes, and what it measured

`Model::Corrected` fixes VOACAP's documented defects. This file records,
per fix, what it moves and whether it makes predictions better. Both
questions are needed: "it is a defect" does not imply "fixing it
improves the forecast", because VOACAP's empirical constants were
fitted with the defects present, so a defect can be load-bearing.

Two measurements per fix:

- **What moved.** `correctcheck --fix NAME --corpus NAME` runs a corpus
  twice, once compatible and once with only that fix on, and reports
  which printed cells differ. The corpus has to reach the fix's site:
  a corpus that cannot reports no movement, which reads exactly like a
  fix that changes nothing.
- **Whether it helped.** `validate --fix NAME` scores the ported engine
  against measured WSPR reception across the eight validation months,
  beside `validate --ported` as the control. Same engine both sides, so
  the comparison is one fix rather than a fix plus a change of engine.
  This is only available where WSPR can see the quantity at all.

## Status

| fix                 | implemented | what moved                        | accuracy effect          |
| ------------------- | ----------- | --------------------------------- | ------------------------ |
| `pole_file`         | yes         | 2.6% of cells, all 96 sweep cases | none measurable; kept on |
| `luf_scan_best`     | yes         | 51% of LUFs, 41 of 48 method-26   | unmeasurable; kept on    |
| `luf_pass_area`     | yes         | 13% of LUFs, 11 of 48 method-26   | unmeasurable; kept on    |
| `curtain_elevation` | yes         | 6.9% of cells, 56 of 96 curtain   | unmeasurable; kept on    |
| `area_centre_nudge` | yes         | the centre point of 3 of 4 grids  | unmeasurable; kept on    |
| `area_antenna_end`  | yes         | nothing any input can reach       | unmeasurable; kept on    |

All six are implemented, so `Model::Corrected` is now the complete set
of documented defects with their fixes on.

"Unmeasurable" is not "none": it means no corpus of measured radio
exists for the quantity the fix changes. See the last section.
`area_antenna_end` is weaker still — no corpus of any kind reaches it,
for the reason its own section gives.

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

Every row `correctcheck` reports, so the counts sum to the 12,807 above:

| row    | cells moved | worst change |
| ------ | ----------: | -----------: |
| V HITE |        3353 |       331.00 |
| SIG LW |        1569 |        24.00 |
| SIG UP |        1044 |        23.20 |
| SNR LW |         944 |        17.10 |
| SNR UP |         835 |        19.40 |
| LOSS   |         642 |        45.00 |
| S DBW  |         642 |        45.00 |
| RPWRG  |         634 |        46.00 |
| SNR    |         634 |        46.00 |
| SNRxx  |         634 |        46.00 |
| TANGLE |         633 |        29.00 |
| DBU    |         611 |        46.00 |
| DELAY  |         320 |         2.10 |
| REL    |         112 |         0.14 |
| S PRB  |          86 |         0.08 |
| MUFday |          55 |         0.36 |
| MPROB  |          25 |         0.58 |
| MUF    |          20 |         0.10 |
| MODE   |          13 |            — |
| N DBW  |           1 |         1.00 |

`MODE` is a text field, so it has no worst change.

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

## The LUF corpus

Both LUF fixes live in `luffy_luf`, which only card methods 26 to 29
run. The corpus is therefore the fuzz corpus with the method changed
to 26: those decks contribute path, season, sunspot number and
antennas, and their frequency cards are ignored because the LUF
methods sweep a complement of their own. `correctcheck --corpus luf`
runs it.

Its compatible half has the same oracle as everything else: `lufcheck`
runs all 48 cases through the reference and compares every column of
the method-26 table, and after both fixes landed it still reports 1152
hour-rows with every cell matching. So the movement below is the fix,
not a port that drifted.

What the table prints is GMT, LMT, FOT, HPF, the sporadic-E MUF, the
circuit MUF and the LUF. Only the LUF can move: the rest are computed
before the LUF search runs.

## `luf_scan_best` — the no-LUF-found scan keeps its running best

**The defect.** When no frequency in the complement meets the required
reliability, the search returns the most reliable one it saw. The loop
never reassigns its running best, so every slot is compared against
slot 1's reliability and the answer is the last slot beating slot 1
rather than the highest. The source carries a comment questioning the
test. The value is printed as a negative LUF, which is how the listing
says "nothing qualified, here is the best there was".

**What moved.** 41 of 48 method-26 cases; 587 of 1152 printed LUFs,
51%, by up to 20.00 MHz. Nothing else in the table moved, and nothing
appeared or vanished.

Half the hours changing is expected rather than alarming: the scan only
runs on hours where no frequency qualified, and on those hours any slot
after the first that beats slot 1 can be overwritten by a worse one
later in the sweep.

**Whether it helped: no measurement exists.** See the last section.

**Decision: kept on.** The routine's stated purpose is to return the
most reliable frequency, and with the defect it returns a different one
by up to 20 MHz.

## `luf_pass_area` — the short LUF pass uses one area throughout

**The defect.** The short LUF pass builds its reflectrix and raysets
with `FINDF` and `FDIST`, which take the sample area as an argument,
and then reads its modes through routines that pick the area up
internally from `JMODE`. Those two are the same area in every systems
pass. They are not here: the electron-density chain ends with
`IF((IPFG.EQ.100).OR.(K.GT.1))GO TO 87`, which names only `IPFG` 100,
so the LUF pass falls through and runs the receiver-end area as well,
leaving `K = KFX`. The pass therefore builds raysets for one area and
reads modes for another.

**The fix** uses the controlling area throughout, which is what the
systems pass does. That direction rather than the other one because
the LUF is defined as the frequency at which the systems model's
reliability meets the requirement: computing it from a different area
than the systems model uses makes the threshold and the thing being
thresholded disagree.

**What moved.** 11 of 48 method-26 cases; 146 of 1152 printed LUFs,
13%, by up to 35.52 MHz. Nothing else moved.

Fewer cases than `luf_scan_best` because the two areas only differ when
the electron-density chain ran a second time — when the controlling
area is the first one and the path has more than one sample area. Where
they do differ, the change is larger.

**Whether it helped: no measurement exists.** See the last section.

**Decision: kept on**, on the argument above. It is worth being plain
that this is a reading of the program's intent, not a measurement:
nothing here shows a corrected LUF is closer to the lowest usable
frequency on a real path.

## `curtain_elevation` — the threshold that lost its decimal point

**The defect.** The IONCAP curtain, antenna type 26, decides whether to
compute its pattern by testing the elevation against the integer
literal `0001` where `.0001` was meant. One radian, not a ten-thousandth
of one. So every elevation within a radian of vertical — above about 33
degrees — skips the calculation and takes the floor gain. And on that
path `SOK` still holds `EX(1)`, the elements per bay, so what the
antenna reports is the floor plus its element count rather than the
floor.

A curtain is a high-gain array aimed at a specific elevation, and 33
degrees is well inside the range a short or medium path uses, so this
is not a corner.

**The fix** is the threshold alone, `.0001` for `0001`. What the
comparison still does at that value is guard the division by zero at
exactly vertical, which is what the decimal point was there for: at
90 degrees both tiers take the floor.

**The corpus.** The sweep paths with the tree's one type-26 file,
`samples/sample.26`, at both ends. `correctcheck --corpus curtain` runs
it. Two things underwrite its compatible half: `antcheck` compares the
whole gain table for that file against the reference's own
`gain01.dat`, 2766 cells at the 0.001 dB the file carries, and an
integration test compares a whole curtain listing against the reference
byte for byte. The second was added with this fix, because nothing
covered it — the fuzz corpus draws IONCAP types 21, 24 and 27 but not
26.

**What moved.** 56 of 96 curtain cases; 33,553 of 486,144 printed
cells, 6.9%, no structural change. An SNR by up to 85 dB, a transmit
gain by up to 45.8 dB, a virtual height by 403 km, and 496 cells
changing which mode dominates.

| row    | cells moved | worst change |
| ------ | ----------: | -----------: |
| DBU    |        3149 |        64.00 |
| RPWRG  |        3057 |        85.00 |
| SNRxx  |        3057 |        85.00 |
| LOSS   |        3019 |        85.00 |
| S DBW  |        3019 |        85.00 |
| SNR    |        3008 |        85.00 |
| TGAIN  |        2491 |        45.80 |
| RGAIN  |        2472 |        43.90 |
| S PRB  |        1856 |         0.86 |
| SIG UP |        1127 |        20.90 |
| SNR UP |        1080 |        18.60 |
| REL    |        1007 |         1.00 |
| SIG LW |         961 |        19.50 |
| V HITE |         931 |       403.00 |
| DELAY  |         884 |         2.90 |
| SNR LW |         876 |        16.50 |
| TANGLE |         801 |        42.10 |
| MODE   |         496 |            — |
| MUFday |         187 |         0.92 |
| MPROB  |          75 |         0.99 |

The 40 cases that did not move are the long ones — the north-south,
near-antipodal and South American paths, and most polar hours. A long
path takes low takeoff angles, which never reach the threshold, so the
defect never bites. Every short, medium and equatorial case moved.

**Whether it helped: no measurement exists.** See the last section.

**Decision: kept on.** A threshold of one radian on a quantity that
runs from zero to π/2 cannot be what was meant, and the behaviour it
causes is a high-gain array reporting less than an isotrope over the
whole upper half of its pattern. Against that, note what is being
assumed: that the intent was a small number and the decimal point was
lost. Nothing in the source states the intended value.

## `area_centre_nudge` — a zero-length path at the grid's own centre

**The defect.** An area run predicts to every point of a grid, and one
of those points can land on the station itself, which would be a
path of no length. The driver moves such a point a twentieth of a
degree east. The test that decides this compares the grid point's
longitude, which `GRIDXY` has already folded into 0 to 360, against
the station's, which is the value the input file gave and may be
negative. A station at 5.9 degrees west therefore differs from its own
grid point by a full 360 degrees, the offset never happens, and the run
computes a zero-length path at its own centre.

**The fix** folds the station's longitude the same way before comparing.

**The corpus.** Four grids, each five points on a side so that a point
lands exactly on the centre, three of them west of Greenwich and one
east as the control. `correctcheck --corpus area` runs them. Its
compatible half is `areacheck`, whose `odd` case is a grid centred on a
station at 0.13 degrees west with a point at the origin — the defect's
own conditions, matching the reference in every printed cell.

**What moved.** 3 of 4 grids; 59 of 2600 printed cells. The east-of-
Greenwich control did not move, and in each of the other three exactly
one point did: the centre. Under it the takeoff angle moves by 72.85
degrees, the virtual height by 347 km, the signal power by 30 dB.

Those are the numbers a zero-length path produces against those of a
path 0.05 degrees long, so this fix does not adjust a prediction so
much as replace a meaningless one.

**Whether it helped: no measurement exists.** See the last section.

**Decision: kept on.** The offset exists to avoid the degenerate path,
and with the defect it never runs for half the world's longitudes.

## `area_antenna_end` — aiming an area antenna by the end it serves

**The defect.** An area antenna's pattern is one frequency over 360
bearings, so the lookup cuts it at a bearing rather than interpolating
in frequency. `GAIN` picks which of the two path bearings to use from
the antenna's **position in the list** — it tests the loop index
against 1 and 2 — where every other test in the same routine goes
through `iats`, the end the card serves. A list holding the receive
card first would cut the receive pattern along the transmitter's
bearing.

**The fix** asks which end the card serves.

**There is no corpus, and this is the one fix with no differential.**
Every area run this crate can build installs the transmit card first,
because `build_area_antennas` iterates the two ends in that order, so
"first in the list" and "serves the transmitter" name the same card and
the fix cannot change a printed cell. Running `correctcheck` over the
area corpus would report zero movement, which here means the corpus
cannot reach the site rather than that the fix is inert.

What stands in its place is a unit test,
`an_area_antenna_is_aimed_by_its_end_only_on_the_corrected_tier`, which
builds the antenna set by hand with the receive card first and shows
the two tiers reading different bearings — 20 dB against the floor. It
is a smaller claim than a corpus makes: it shows the branch is wired
and does what it says, not that any run's numbers change.

**Decision: kept on**, with that limit stated. The alternative was to
leave a documented defect unfixed on the corrected tier because no
input reaches it, which would make `Model::Corrected` mean "the defects
we could measure" rather than "the defects we found".

## Why five of the fixes have no accuracy measurement

The WSPR pipeline cannot score the LUF fixes, the curtain fix or the
area fixes at all. It measures point-to-point systems predictions
against beacon reports: what it holds is reception reports at fixed
frequencies on real paths, so it can say whether a predicted SNR or
reliability was right. It holds nothing about the lowest usable
frequency and nothing about a coverage area.

The curtain fails for a second reason worth stating separately. The
validation fits one signal-level offset per path, precisely because the
antennas at both ends are unknown, so an antenna-pattern change is
partly absorbed by that fit even where one exists. No WSPR station in
the corpus is known to use a curtain in any case.

The gap is in the ground truth, not in the harness, so it cannot be
closed by running more of what already exists. For these fixes, "what
moved" plus a reading of the source is the whole of the available
evidence. Each decision above therefore rests on what the program was
trying to do, which is a weaker argument than a measurement, and is
labelled as such rather than papered over with a number that came from
somewhere else.
