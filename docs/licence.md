# Licence and provenance

What the port is derived from, and what would have to ship with it.
Researched 2026-07-27 from the vendored `voacapl` tree. This records
findings, not legal advice.

## The code

Three licences apply to the vendored distribution
(`vendor/voacapl/LICENSE`):

| Part                             | Status                                                                          |
| -------------------------------- | ------------------------------------------------------------------------------- |
| Original VOACAP (NTIA/ITS)       | US Government work, stated as "not subject to copyright protection in the U.S." |
| J.A. Watson's Linux port changes | CC0                                                                             |
| `dst2csv.f90`, `dst2ascii.f90`   | GPL-3.0                                                                         |

**The GPL files do not reach the port.** They are two standalone
programs that convert `.DST` binary files to text, built as their own
`bin_PROGRAMS` under `voacapl/itshfbc/bin/dst/`. The port is a
translation of `src/voacapw/` and `src/hfmufesw/`, and nothing in
`propcore` reads, calls or derives from either file or from the
`f90getopt` module they use.

So the ported engine derives only from the public-domain original and
Watson's CC0 changes.

One caveat worth stating plainly: "not subject to copyright protection
in the U.S." is a statement about US law. US Government works are not
automatically in the public domain elsewhere. Every redistributor of
VOACAP for the last two decades has treated it as freely
redistributable, which is evidence but not a guarantee.

## The data files

Measured by what the engine opens, not by directory. A prediction
needs about 760 KB:

| File                                  | Count |   Size | Why                                |
| ------------------------------------- | ----: | -----: | ---------------------------------- |
| `coeffs/coeffNNw.bin`                 |    12 | 452 KB | the month's ionospheric maps       |
| `coeffs/fof2CCIR.daw`, `fof2URSI.daw` |     2 | 185 KB | foF2 maps, one per `COEFFS` card   |
| `antennas/default/`                   |    12 | 120 KB | the isotrope and the CCIR defaults |
| `database/version.w32`                |     1 |   17 B | the listing header's version       |
| `antennas/samples/`                   |    61 | 1.0 MB | only if a caller names one         |

What is **not** needed, which is most of the 8.1 MB source tree:

- The 2.7 MB of `.asc` coefficient sources. `makeitshfbc` converts
  them into the binary files at install time and the engine only ever
  opens the binaries.
- `coeffNN.bin` without the `w`. The port reads only the `w` variant,
  as the reference does.
- `database/north_pole.txt`. It is never read — the reference builds
  its path without a separator, so the built-in pole always wins (see
  the defects list). Only a user-supplied `run/north_pole.txt`
  overrides it.
- `geocity`, `geonatio`, `geostate`, `news` — 3.1 MB belonging to the
  interactive front end.

**The open question is the coefficients.** `coeff01.asc` to
`coeff12.asc` and `fof2URSI.asc` are the CCIR and URSI ionospheric
maps, which originate in CCIR Report 340 and URSI publications rather
than with NTIA/ITS. The ITU asserts copyright over its publications.
These same files ship inside VOACAP, ICEPAC, REC533, ITURHFProp and
several Python packages, and have for years — but wide redistribution
is not the same as a cleared licence, and this is the one item that
needs a decision rather than more research.

This is what couples the licence question to how the crate ships. The
antenna files and `version.w32` are pure NTIA/ITS with no question
over them, so they can be embedded whatever is decided. The
coefficients are the 637 KB that the question is about.

- If redistribution is accepted, embed all of it: about 760 KB, and
  the crate works with no external tree.
- If not, embed the antennas and the version file (120 KB) and have
  the caller supply a coefficients directory. Still far less of an
  imposition than today, where the caller must build the Fortran and
  run `makeitshfbc` to get a tree at all.

## Attribution

Whatever is decided, the crate should credit NTIA/ITS for VOACAP,
Greg Hand as its maintainer, and J.A. Watson for the Linux port,
and carry the NTIA/ITS disclaimer — it asks for no warranty claims
and no implication of US Government endorsement.
