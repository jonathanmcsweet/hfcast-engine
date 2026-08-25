# Licence and provenance

What the port derives from, and what may ship with it. Researched
2026-07-27 to 2026-08-03 from the vendored `voacapl` tree. This records
findings, not legal advice.

## The code

Three licences apply to the vendored distribution
(`vendor/voacapl/LICENSE`):

| Part                             | Status                                                                          |
| -------------------------------- | ------------------------------------------------------------------------------- |
| Original VOACAP                  | US Government work, stated as "not subject to copyright protection in the U.S." |
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

Measured by what the engine opens, not by directory. A prediction needs
653 KB, measured by copying exactly what is embedded:

| File                                  | Count |   Size | Why                                |
| ------------------------------------- | ----: | -----: | ---------------------------------- |
| `coeffs/coeffNNw.bin`                 |    12 | 452 KB | the month's maps and noise tables  |
| `coeffs/fof2CCIR.daw`                 |     1 |  93 KB | the foF2 map, the default `COEFFS` |
| `coeffs/fof2URSI.daw`                 |     1 |  93 KB | only for a `COEFFS URSI88` card    |
| `antennas/default/`                   |    30 |  16 KB | the isotrope and the CCIR defaults |
| `database/version.w32`                |     1 |   17 B | the listing header's version       |
| `antennas/samples/`                   |    61 | 1.0 MB | only if a caller names one         |

What is **not** needed, which is most of the 8.1 MB source tree:

- The 2.7 MB of `.asc` coefficient sources. `makeitshfbc` converts them
  into the binary files at install time and the engine only ever opens
  the binaries.
- `coeffNN.bin` without the `w`. The port reads only the `w` variant, as
  the reference does.
- `database/north_pole.txt`. It is never read, because the reference
  builds its path without a separator, so the built-in pole always wins
  (see the defects list). Only a user-supplied `run/north_pole.txt`
  overrides it.
- `geocity`, `geonatio`, `geostate`, `news`: 3.1 MB belonging to the
  interactive front end.

## What the coefficients are

`coeffNNw.bin` is nine Fortran records and `redmap.for` names what each
one holds. Counted array by array, per month and then times twelve, with
`fof2CCIR.daw` and `fof2URSI.daw` added whole:

| Data                                                             |   Size | Written by     |
| ---------------------------------------------------------------- | -----: | -------------- |
| foF2 map (`fof2CCIR.daw`) and M(3000)F2 map (`XFM3CF`)           | 134 KB | CCIR Report 340 |
| Atmospheric noise: `FAKP`, `FAKABP`, `DUD`, `FAM`, `SYS`, `FAKMAP`, `ABMAP` | 216 KB | CCIR Report 322 |
| Sporadic E, E region, F1, F2 height ratio, `IKIM`, `PERR`, `F2D` | 195 KB | NTIA/ITS       |
| URSI-88 foF2 map (`fof2URSI.daw`)                                |  93 KB | URSI           |

**About 195 KB is not CCIR data at all.** It is NTIA/ITS work, in the
same class as the antenna files, and carries the same status.

**Most of the CCIR data is atmospheric noise, not ionospheric maps.**
216 KB of the 350 KB is CCIR Report 322, the source for ITU-R
Recommendation P.372. That matters because the ITU's free-use statement
below names the **P372** software as well as P533, so the noise data and
the foF2 maps are covered by one sentence rather than being two
questions.

How this was measured, stated plainly: from the Fortran array names and
what VOACAP's own documentation says each model is, not by opening CCIR
Reports 322 and 340 and comparing numbers. `FAM`, `DUD` and `SYS` are
the Report 322 noise parameters; `XFM3CF` and the `.daw` files are the
Report 340 maps. Anyone who wants certainty rather than a strong reading
should compare against the P372 data in the ITU repository.

## The ITU publishes these coefficients itself

**ITU-R Study Group 3 distributes these coefficients publicly**, in the
official P.533 reference implementation at
`github.com/ITU-R-Study-Group-3/ITU-R-HF`. `P533/Data/` holds
`COEFF01W.BIN` through `COEFF12W.BIN` and the same files as text, under
this statement:

> The ITURHFProp, P533 and P372 software has been developed collaboratively by
> participants in ITU-R Study Group 3. It may be used by implementers in their
> implementation of the Recommendation as well as in revisions of the specific
> original Recommendation and in other ITU Recommendations, free from any
> copyright assertions.

Checked against what is embedded here: not byte-identical, because
`makeitshfbc` packs its own binary format and the ITU's is different:
ours is little-endian `f32` after an integer header, theirs is not.
Comparing the ITU's ASCII form against our binary, 57.6% of its values
appear in ours at five significant figures, and the remainder is layout
rather than different numbers. Same coefficient set, different
container.

Two things this does not settle, stated plainly:

- The grant is worded around implementations of **ITU Recommendations**.
  This engine implements VOACAP, which is a US Government model, not
  P.533. So the position is "the rights holder publishes this data
  freely for implementers" rather than "there is a licence written to
  cover this use".
- It is still not legal advice, and neither is this file.

## What ships, and what does not

**The crate is published without the coefficients.** What goes to
crates.io is the engine, the 30 antenna files and the version file, all
NTIA/ITS work, all US Government work, none of it in question.
`embedded/coeffs/` is behind the `embedded-coefficients` feature, which
is off by default and whose files `Cargo.toml` excludes from the
package. A dependent from crates.io reads the coefficients from an
`itshfbc` tree, which is how the reference engine has always found them,
and gets a message naming the feature and the reason if it asks for
`<embedded>` without them. CI asserts this: the `Package` step fails if
a coefficient file ever appears in the tarball.

**The repository is public, and carries 44 of the 45 files.**

- The 195 KB of NTIA/ITS data. Same status as the antenna files.
- The 350 KB from CCIR Reports 322 and 340. The ITU publishes this data
  itself, for implementers, "free from any copyright assertions", in its
  P.372 and P.533 reference software.

What goes:

- `fof2URSI.daw`, 93 KB. It is the one part with no ITU publication
  behind it, and the only reader is a `COEFFS URSI88` card, which
  nothing in this project writes: the flag defaults to false and only
  `fuzz --coeffs URSI88` sets it. `fuzz` drives a real `itshfbc` tree
  through `runner.rs`, so it is unaffected. A caller that asks
  `<embedded>` for the URSI maps gets a message saying they are in no
  build and that a real root is needed, deliberately not a message
  naming the feature, which would send the reader in a circle.

The removal costs nothing this project uses and takes the weakest item
off the list.

`NOTICE` carries the same breakdown, because `NOTICE` is where
Apache-2.0 expects the status of other people's work to be recorded, and
it is the file that travels with the code.

**An application binary still carries the rest.** An Android build turns
the feature on, because a phone has no `itshfbc` tree, so every APK
holds the 544 KB. Handing that APK to other people is redistribution,
and the position for it is the one above: the ITU publishes the CCIR
data itself, for implementers, free from any copyright assertions.

If that answer ever turns out to be no, the fallback is already built:
`embedded/coeffs/` comes out of the repository, `data.rs` keeps the
antennas and the version file, and the caller supplies a coefficients
directory through the overlay root the module already supports.

## Attribution

Whatever is decided, the crate should credit the Voice of America for
VOACAP, NTIA/ITS for the IONCAP it was built from and for the data
files, Greg Hand as VOACAP's maintainer, and J.A. Watson for the Linux
port, and carry the NTIA/ITS disclaimer, which asks for no warranty
claims and no implication of US Government endorsement.
