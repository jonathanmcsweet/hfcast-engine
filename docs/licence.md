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

The engine reads three of the eleven directories in the `itshfbc`
tree. The rest belong to the interactive front end.

| Directory                                 | Size   | Read by the engine                    |
| ----------------------------------------- | ------ | ------------------------------------- |
| `coeffs`                                  | 3.0 MB | yes — ionospheric maps                |
| `antennas`                                | 1.2 MB | yes — 73 pattern definitions          |
| `database`                                | 44 KB  | yes — `north_pole.txt`, `version.w32` |
| `areadata`, `area_inv`, `run`             | 236 KB | area runs and scratch                 |
| `geocity`, `geonatio`, `geostate`, `news` | 3.1 MB | no                                    |

So a library needs about 4.2 MB of the 8.1 MB tree, and less if the
ASCII coefficient sources are dropped in favour of the binary files
the engine actually opens (`coeff01w.bin`, `fof2CCIR.daw`).

**The open question is the coefficients.** `coeff01.asc` to
`coeff12.asc` and `fof2URSI.asc` are the CCIR and URSI ionospheric
maps, which originate in CCIR Report 340 and URSI publications rather
than with NTIA/ITS. The ITU asserts copyright over its publications.
These same files ship inside VOACAP, ICEPAC, REC533, ITURHFProp and
several Python packages, and have for years — but wide redistribution
is not the same as a cleared licence, and this is the one item that
needs a decision rather than more research.

Options if it cannot be settled: ship without the coefficient files
and have the crate read a user-installed `itshfbc` tree, which is what
it does today; or point at the ITU's own distribution.

## Attribution

Whatever is decided, the crate should credit NTIA/ITS for VOACAP,
Greg Hand as its maintainer, and J.A. Watson for the Linux port,
and carry the NTIA/ITS disclaimer — it asks for no warranty claims
and no implication of US Government endorsement.
