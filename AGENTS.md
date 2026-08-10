# Agents.md — guidance for AI agents working in this repo

## Your behavior

- Speak to me in ASD-STE100 Simplified Technical English
- Write documentation in ASD-STE100 Simplified Technical English
- Be concise, articulate with your language in interactions and avoid idioms that may confuse people who don't know what they mean. Use simple language

## Open work and progress

Open work is tracked by the maintainer outside this repository. Do not
create tracker or progress documents. If you defer work or find a gap,
describe it in the pull request and the maintainer will record it.

## Build and verify

Rust. `cargo` is all you need, except for the parity check.

```bash
cargo fmt --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test --all-features   # a source checkout has the coefficient maps
cargo test                  # the default build, which is what a dependent gets
tools/analyze.sh --gate     # complexity against tools/complexity-baseline.tsv
```

Run both test lines. `--all-features` turns on `embedded-coefficients`.
Without it the tests that read `<embedded>` do not run at all.

**The parity check is the claim this crate rests on.** `portcheck` runs
the Rust engine and the Fortran it was translated from over 23,040 cells
and compares them field by field against the envelope in
`docs/sensitivity.md`. It needs `gfortran` and a build of the reference,
so CI runs it in its own job:

```bash
VARIANTS=O2 tools/build-variants.sh
HFCAST_ITSHFBC="$HOME/itshfbc" cargo run --release --bin portcheck
```

Run it for any change that could move a number. A change that only moves
whitespace still needs it: "equivalent to the reference" is measured,
not assumed.

The complexity gate counts lines as well as branches, so a reformat can
fail it. If you refresh `tools/complexity-baseline.tsv`, show that the
cyclomatic column did not move.

Hooks run the first block for you. Turn them on once per clone:

```bash
git config core.hooksPath .githooks
```

`cargo publish` is the maintainer's to run. Do not run it.

## Chores

- Always bump the version number for any part of the product (ex: core and dashboard) based on semantic versioning when commiting your final work to a branch.
- Core and Dashboard do not need to have vesion parity.
- SemVer reference: https://semver.org

## Documentation

- Keep text descriptions short without excessive details unless necessary to prevent confusion
- Refrain from using jargon or idiomatic language such as "clobber," "belt and suspenders," etc. which may be read differently by different people

## Branches and Commit messages — use Conventional Commits

- follow the instructions in the ##Documentation section for writing commit messages

- Follow the spec: <https://www.conventionalcommits.org/en/v1.0.0/#specification>

```
<type>[optional scope][!]: <description>

[optional body]

[optional footer(s)]
```

- **Allowed types:** `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`,
  `build`, `ci`, `chore`, `revert`.
- **description:** imperative mood, lowercase, no trailing period.
- **Breaking changes:** add `!` after the type/scope (e.g. `feat(create)!:`) and/or
  a `BREAKING CHANGE:` footer.
- **Examples:**
  - `feat(security): add container fingerprint hardening`
  - `fix(create): bind sshd to loopback only`
  - `chore: adopt test/ and lib/ layout`
- End messages with the `Co-Authored-By:` trailer naming the AI model used.

## Before committing

- Run all unit tests
- Run all linting
- Never commit items in `.gitignore`.

## No inline foreign-language code — extract to its own file

- NEVER embed another language (Python, etc.) inline in any other file

## Coding style

- Always use a functional-first immutability-first coding style
  - Prefer `map`, `filter` and `reduce` over `for` loops;
  - Where a loop is kept, say in a comment what it does that the
    functional form cannot
    