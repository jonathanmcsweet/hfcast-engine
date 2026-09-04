# The VOACAP port

VOACAP is the program most HF propagation prediction rests on. The Voice
of America had it built in Fortran from ITS's IONCAP, completed it in
1993, and it is still what other predictions are measured against to
this day. This engine is that program translated into Rust
(`vendor/voacapl` is the maintained source, `hfcast::engine` the
translation).

The two are meant to give the same answer to the last digit on the same
input, and where VOACAP has a defect this engine reproduces the defect.
"The same as the original" is only something you can test if it holds
everywhere, including where the original is wrong. Fixes live in one
named place instead, where each can be switched on alone and its effect
measured. See `corrected.md`.

## What it changes for an operator

Not the forecast. A faithful translation predicts what the original
predicts, so the numbers are neither better nor worse. If you want
better numbers, use our Truecast model, and read `comparison.md` for how
the two compare.

The original needs a Fortran compiler, its input in a fixed-width card
format, and a private copy of its working directory for every run,
because it writes scratch files under fixed names. Our Rust translation
runs in a modern environment and answers in JSON.

## Is it really the same

Measured, not asserted, by six harnesses that each compare this engine
against the original on the same input:

| what is compared | how much of it | result |
| --- | --- | --- |
| point-to-point listings | 463,104 printed cells and 23,040 mode labels, 96 paths | every cell identical |
| whole listing files | 600 generated decks, 434,116 lines | identical as text |
| coverage maps | 17,791 cells over 21 grids | identical but for the limit below |
| antenna gain tables | 204,684 cells, 74 of 76 definition files | identical |
| the fields an application reads | 7,104 fields, 8 shapes | identical |
| this engine on another processor | listings and coverage grids | identical to itself |

The two antenna files that are not compared are a Harris type the
original cannot compute either: it calls out to a program that is not in
the distribution and stops without it, so both refusing is agreement.

"Identical" here means the printed text, character for character, not a
tolerance. Where a difference is allowed at all it is stated in
`sensitivity.md`, which measures how far apart two builds of the
original Fortran land from each other.

One exception is known, and only card method 25 can show it. It prints
its values to two decimals where every other method prints one, and two
decimals is fine enough to show the last bit of an `f32`. Two cases have
been found where a value sitting within about a hundred-thousandth of a
rounding boundary rounds the other way. Both are inside the tolerance
envelope, and every IEEE-conformant build of the reference agrees with
itself on them, so this is the port's own last bit rather than build
noise. The first was traced to one antenna gain whose last bit turns
into a whole 0.001 when the reference writes it to `gainNN.dat` at three
decimals and reads it back.

## A limit the original states, and this engine lifts

Ask VOACAP for a coverage map on several frequencies at once and it
prints this before it starts:

    TRANSMIT antenna MUST BE non-directional for this purpose!

It means it. Before predicting anything, VOACAP works out how much gain
the antenna has in each direction. For a map on one frequency it does
that properly: every bearing, every take-off angle. For a map on several
frequencies at once it does not. It falls back to the table it builds
for an ordinary point-to-point path, which holds the gain along a single
bearing, the bearing from the station to the middle of the map, and then
uses that one bearing's gain for every point on the map.

For an operator that means a beam pointed north-east is treated as
pointing north-east at the map's western edge too. The take-off angle is
what the map's shading is drawn from, so a map built that way can show a
station as reachable in a direction the antenna was never turned to.
With an antenna that has the same gain in all directions there is no
bearing to get wrong, which is why the original can state the
restriction and stop there.

This engine builds the proper table for each frequency instead, so
asking for four bands together gives what asking for each band on its
own gives, directional antenna or not. It is the only input where the
two draw different coverage maps, and it is an input the original tells
you not to make.

How far apart they land was measured over ten configurations, a world
grid and a regional one, four antenna families, one and both ends
directional, two hours:

    worst single cell        0 to 37.5 degrees
    cells that moved at all  0 to 20.2 percent

The spread is the point. A non-directional antenna moves nothing. Where
the pattern is directional the worst cell sits near the station, where
take-off angles are high and a few degrees of bearing move them a long
way. How many cells move depends on the map: a fifth of a world grid,
under one percent of a regional grid centred on the station, where every
point lies close to the bearing of the centre.

This applies to both tiers, `Model::Compatible` included. Holding it
back to `Model::Corrected` would file a limit the original announces
alongside six defects it stays silent about, and it would cost the one
guarantee the behaviour exists to give: that a batch of bands answers as
the same bands run one at a time. `tests/area_bands.rs` holds the engine
to that guarantee. It cannot be reached point to point.

`areacheck` names the two cases that reach it, prints what they differ
by, and fails if either ever stops differing.

## What is out of scope

The interactive front end, the plotting, and the sibling engines ICEPAC
and REC533. This is the prediction engine of the `voacapl` program and
nothing around it.


## Checking it yourself

Every figure above is a measurement, and every measurement can be
re-run. The reference is not vendored, so fetch and build it first:

```
git clone --depth 1 https://github.com/jawatson/voacapl vendor/voacapl
tools/build-variants.sh   # O0 O1 O2 O3 fastmath. O2 is the reference
tools/build-trace.sh      # the instrumented build the stage traces read
```

A fresh clone needs its timestamps settled before that works. git gives
every file the same time, so make decides the generated autotools files
are out of date and tries to rebuild them with `aclocal-1.15`, which no
current distribution carries. The failure reads as a missing package and
is not one:

```
cd vendor/voacapl
touch configure.ac; sleep 1
touch aclocal.m4;   sleep 1
touch configure $(find . -name config.h.in); sleep 1
touch $(find . -name Makefile.in)
```

Each harness job runs a Fortran binary and copies a private `itshfbc`
tree, so the harnesses are heavier than their processor use suggests.
Sizing a pool from the core count gets a tool killed by the kernel,
which reports a bare `Killed` or exit 137. Pass a count instead: `--jobs
3` to the harnesses, `JOBS=4` to the build scripts. Above about a
hundred fuzz cases keep to `--jobs 2`, or the open-file limit runs out
and the report says the port and the reference disagree when they do
not.

Then, from the repository root:

| harness        | what it proves                                        | flags                                                                                                                                          |
| -------------- | ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `porttest`     | each stage's intermediates against the trace build    | `--cases N` `--only ID` `--seed N` `--fuzz N [--from N]`                                                                                       |
| `portcheck`    | whole listings over the 96 sweep cases                | `--cases N`                                                                                                                                    |
| `fuzz`         | whole listing files over generated decks              | `--cases N` `--from N` `--jobs J` `--seed N` `--show N` `--method M` `--coeffs URSI88` `--fprob a,b,c,d` `--botlines a,b,c` `--toplines a,b,c` |
| `antcheck`     | antenna gain tables against the reference's own files | `--only NAME` `--verbose`                                                                                                                      |
| `lufcheck`     | `OUTMUF`'s table from a method-26 deck                | `--cases N` `--from N` `--jobs J`                                                                                                              |
| `mufcheck`     | methods 1, 3 and 7 tables                             | `--method 1\|3\|7` `--cases N` `--from N` `--jobs J`                                                                                           |
| `areacheck`    | area coverage rows and antennas against the grid file | `--jobs J`                                                                                                                                     |
| `paritycheck`  | the fields an application reads, both production paths | `--jobs J` `--paths FILE --month M --year Y --ssn S` `--dump DIR`                                                                              |
| `archcheck`    | this engine's own listings and area grids elsewhere   | `--cases N` `--full`                                                                                                                           |
| `correctcheck` | what one corrected-tier fix changes                   | `--fix NAME` `--corpus sweep\|luf\|curtain\|area` `--cases N` `--jobs J`                                                                       |

They check three different things, because each catches what the others
miss. **Stage traces** (`porttest`) compare every intermediate value
against the instrumented Fortran, so a mistake surfaces in the stage
that holds it rather than in the answer. **Whole listings** (the rest)
compare the printed file character for character, which is the only
thing that catches the banner, the page breaks and the number
formatting. **The tolerance envelope** holds the port no further from
the reference than IEEE-conformant rebuilds of the reference are from
each other. It is in fact identical, but the envelope is what a verdict
is judged against, because identity is not something a floating-point
port can promise in advance.

`archcheck` is the only harness that does not involve the reference. It
renders what `portcheck` compares and prints a digest per case, so the
same binary can be run on two processors and the outputs diffed. That is
how the port is checked on a phone's processor, where running a Fortran
binary per case is impractical.

One harness is not currently usable: `porttest --fuzz` reports stage
mismatches on generated decks where `fuzz` finds the finished listings
identical, so the fault is in how it pairs its dumps. `porttest` over
the 96 sweep cases is the mode to trust.

## Rules the translation follows

Each of these cost a wrong answer before it was understood.

- **Compute in `f32`.** The Fortran uses 4-byte REAL, so the port uses
  `f32` (`con::R`) and writes each expression in the source's order.
  Double precision would be a different model, because a decision near a
  threshold can flip. Upgrading is a deliberate later step, not a free
  improvement.
- **Keep the source's association.** Fortran binds an exponent before a
  multiplication, so `aa * rl**2` is `aa` times the square. Flattening
  it to `(aa * rl) * rl` rounds differently, and moved three printed
  digits in the antenna code.
- **Round, then widen.** A routine computing in double precision whose
  arguments come from a single-precision store receives a value already
  rounded to `f32`. Passing the exact double instead moved an azimuth by
  one unit in the last place.
- **A value that travels through a file carries the file's decimals.**
  The engine computes with the antenna gain it read back from
  `gainNN.dat`, not the gain it computed.
- **Use the source's own constants.** Rust's `to_degrees()` is one `f32`
  step away from the `57.295779513` the Fortran holds, and one step is
  enough to move an interpolated gain across a rounding boundary. Every
  conversion goes through `con::R2D` and `con::D2R`.

Three more about the printed page:

- **Formatted output rounds half to even**, so 327.25 in an `F7.1` field
  prints as 327.2, not 327.3. `ANINT` inside the model rounds the other
  way, so both rules live in one program.
- **Read printed tables by column, not by splitting on spaces.** A MUF
  of 1000.00 fills its `F7.2` field completely and leaves no space
  before the next value.
- **A negative value that rounds to zero prints without its sign,**
  because every source file is compiled with `-fno-sign-zero`. A
  comparison that parses the numbers back can never see this, since
  `-0.0` equals `0.0`. It surfaced the first time an output was compared
  as text.

## Bugs are kept, not fixed

Where the original is wrong this engine is wrong the same way, and each
defect is documented at the code that reproduces it rather than listed
here. One for the flavour: `set_magnetic_pole` builds its database path
without a separator, so the installed `database/north_pole.txt` is
silently ignored and a built-in pole is what every run actually uses.
The port reproduces the malformed lookup. The geometry only matches this
way.

## Proving the checks reach the code

**A green verdict from a deck that cannot reach the branch proves
nothing.** The port applied a distance cutoff everywhere that the
original applies to one method only, and it survived several passing
sweeps: on the method the sweep ran, that cell reads zero whether the
port is right or wrong. Before trusting a result, ask what input would
make the branch visible in a printed cell, and whether the corpus holds
such an input. The cheap way to answer is to break the new code on
purpose and re-run, because **a case that stays green under a deliberate
error never reached the code.** Every stage since has been checked that
way, and how many cases each deliberate break moved is recorded at the
code.

**The listing does not print everything the engine computes.** A wrong
value that never reaches the page is invisible to the whole-file checks.
That is what the stage traces are for, and how a disagreement about the
sporadic E layer's hop count was found while both sides were printing
the same table.

**Concurrent harnesses shared their scratch trees.** Each copies the
`itshfbc` tree per case so runs cannot see each other's files, but the
copy was named after the case alone, so two harnesses working the same
corpus picked the same directory. It did not fail: it truncated one
run's listing and reported the missing cells as differences. The name
now carries the process id.

**A leftover file in the shared tree can neutralise the thing being
measured.** `validate --fix` renders against `~/itshfbc` directly, so
whatever is in that tree applies to both sides of the comparison. A
stray `run/north_pole.txt` makes the magnetic pole fix change nothing,
and the null result is an artefact of the tree rather than a finding.

## Two tiers in one engine

Reproducing the defects is what makes "identical to the reference"
checkable, but several of them are plainly defects and a readable engine
exists partly to be able to fix them. Both behaviours live in one
engine, chosen per request by `api::Request::model`:

- **HFcast Compatible**, VOACAP as it is. The default, and the only tier
  any harness here can judge, because the reference has no other
  behaviour to compare against.
- **HFcast Corrected**, the same engine with six documented defects
  fixed. `corrected.md` measures each one, including the fixes that
  measured worse and were left off, because a defect can be load-bearing
  when the model's constants were fitted with it present.

Three rules keep the split from spreading. Every divergence is named in
`src/voacap/model.rs`, so engine code asks about one defect rather than
about the tier, and counting those methods counts the divergence. Only
the two ends are public, because the combinations between them are a
measurement tool rather than something to build on. And only point
defects qualify: a fix that is one branch at one documented site. `f32`
to `f64`, evaluation order, and the state that persists between hours
are not defects a flag can honestly describe, and the result would not
be VOACAP with a fix but a different model.

## Where the rest of the detail is

This document is deliberately short. The engine is 20,000 lines of Rust
that follows a Fortran program stage for stage, and a prose retelling of
it would go stale against the code and lose every argument with it.

So the per-stage findings live at the code that reproduces them: what
each input card does, which of them reach no computation at all, how the
antenna families compute, where the original's own bugs are and what
each one does. `src/voacap/` is organised by stage and each module opens
with what it corresponds to in the Fortran. `docs/corrected.md` covers
the fixed tier, `docs/sensitivity.md` the tolerance, and
`docs/licence.md` where the code and the data come from.
