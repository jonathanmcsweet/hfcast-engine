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
`hfcast` reads, calls or derives from either file or from the
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
needs 653 KB, measured by copying exactly what is embedded:

| File                                  | Count |   Size | Why                                |
| ------------------------------------- | ----: | -----: | ---------------------------------- |
| `coeffs/coeffNNw.bin`                 |    12 | 452 KB | the month's ionospheric maps       |
| `coeffs/fof2CCIR.daw`, `fof2URSI.daw` |     2 | 185 KB | foF2 maps, one per `COEFFS` card   |
| `antennas/default/`                   |    30 |  16 KB | the isotrope and the CCIR defaults |
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

### Researched, 2026-07-30: the ITU publishes the coefficients itself

The paragraph above says wide redistribution is not a cleared licence. That was
written without checking what the rights holder does with the files, and the
answer changes the picture.

**ITU-R Study Group 3 distributes these coefficients publicly**, in the official
P.533 reference implementation at
`github.com/ITU-R-Study-Group-3/ITU-R-HF`. `P533/Data/` holds `COEFF01W.BIN`
through `COEFF12W.BIN` and the same files as text, under this statement:

> The ITURHFProp, P533 and P372 software has been developed collaboratively by
> participants in ITU-R Study Group 3. It may be used by implementers in their
> implementation of the Recommendation as well as in revisions of the specific
> original Recommendation and in other ITU Recommendations, free from any
> copyright assertions.

Checked against what is embedded here: not byte-identical, because `makeitshfbc`
packs its own binary format and the ITU's is different — ours is little-endian
`f32` after an integer header, theirs is not. Comparing the ITU's ASCII form
against our binary, 57.6% of its values appear in ours at five significant
figures, and the remainder is layout rather than different numbers. Same
coefficient set, different container.

Two things this does not settle, stated plainly:

- The grant is worded around implementations of **ITU Recommendations**. This
  engine implements VOACAP, which is an NTIA/ITS model, not P.533. So the
  position is "the rights holder publishes this data freely for implementers"
  rather than "there is a licence written to cover this use".
- It is still not legal advice, and neither is this file.

What it does settle is the factual question the earlier paragraph left open:
these are not files whose distribution the ITU restricts.

### Decided, 2026-07-30: embed all of it

All 45 files, 653 KB, are in `embedded/` and compiled in by
`src/voacap/data.rs`. The engine now runs with no external tree, which is
what the application on a phone needs — a phone user cannot be asked to
build the Fortran and run `makeitshfbc`.

The reasoning for going ahead while the licence question is open is that
**building for one's own devices is not redistribution.** Embedding is a
technical arrangement; distributing the result to other people is the act
the question is about. So `Cargo.toml` keeps `publish = false`, and whether
application builds may be handed out — F-Droid, an app store, a download —
is a separate decision.

That decision is now being taken: the user intends to publish to Google Play,
F-Droid, Accrescent, Obtainium and direct download (2026-07-30). The section
above is why that is a smaller step than this paragraph assumed when it was
written. `publish = false` on the crate is unchanged and is a separate question
from distributing an application binary.

If the answer ever turns out to be no, the fallback is unchanged and cheap:
`embedded/coeffs/` comes out of the repository, `data.rs` keeps the antennas
and the version file, and the caller supplies a coefficients directory
through the overlay root the module already supports.

## Attribution

Whatever is decided, the crate should credit NTIA/ITS for VOACAP,
Greg Hand as its maintainer, and J.A. Watson for the Linux port,
and carry the NTIA/ITS disclaimer — it asks for no warranty claims
and no implication of US Government endorsement.
