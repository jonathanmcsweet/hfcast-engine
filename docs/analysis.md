# Static analysis

`tools/analyze.sh` runs the checks a tool like SonarQube would run, and
prints which ones could not run. `--gate` makes a broken gate exit
non-zero.

SonarQube itself does not analyse Rust. `rust-code-analysis`, which
reports the same per-function metrics, builds its parsers from C and
C++ sources and so needs a host C++ toolchain — not every machine that
can build this crate has one. The suite is therefore several tools plus
one written here.

## What runs

| Step | Tool | Gate |
| ---- | ---- | ---- |
| Lints | `cargo clippy`, default set | yes |
| Complexity | `src/bin/complexity.rs` | yes |
| Unreferenced public items | reference count in the script | no |
| Deeper lints | `cargo clippy`, pedantic and nursery | no |
| Duplication | `jscpd` | no |
| Coverage | `cargo llvm-cov` | no |
| Size | `tokei` | no |

The last three are optional. If one is missing the script says so in
its summary instead of passing quietly.

```bash
cargo install cargo-llvm-cov tokei
rustup component add llvm-tools
npm install -g jscpd
```

## Why only two gates

Most of `src/voacap/` is one Rust function per Fortran subroutine, on
purpose, so a reader can hold the two side by side. That makes the port
score badly on nearly every measurement of new code: the functions are
long, they branch often, they take many arguments, and they cast
between number types on almost every line. All of that is faithful.

A gate that fires on the port would be switched off within a week. So
the gates fire on **change** instead:

- Clippy's default lints are held at zero. They are about Rust, not
  about arithmetic, and the crate has always passed them.
- Complexity is held against a recorded baseline. A function that gets
  worse fails. A function that is already complicated does not.

## The lints that must never be applied

The pedantic and nursery run turns off a fixed list, kept in
`analyze.sh`. Two groups are there for a reason worth stating.

**Casts and float comparisons** are the Fortran's own. `REAL` to
`INTEGER`, and `IF (X .EQ. Y)`, are what the reference does; writing
them differently would describe different arithmetic.

**`suboptimal_flops` and `imprecise_flops`** suggest `mul_add` and
`ln_1p`. A fused multiply-add keeps the intermediate product at full
width and rounds once instead of twice. It is more accurate and it
gives a different number, which is exactly the class of change the
parity harnesses exist to catch. Applying these would break
byte-identical parity with `voacapl`, which is the crate's whole claim.

The remainder of the list is documentation and naming preferences.

## Complexity

`complexity` reports, per function: cyclomatic complexity, body length,
deepest block, and argument count. Cyclomatic complexity is McCabe's
count of decision points — one, plus one for each `if`, `while`, `for`,
`loop`, match arm, `&&`, `||` and `?`.

`#[cfg(test)]` modules are not measured.

```bash
cargo run --release --bin complexity             # the report
cargo run --release --bin complexity -- --check  # the gate
cargo run --release --bin complexity -- --update # rewrite the baseline
```

Thresholds for code not in the baseline: cyclomatic 15, 100 lines,
5 levels of nesting, 7 arguments.

`tools/complexity-baseline.tsv` holds the accepted figures. Run
`--update` when a function is legitimately restructured, and say in the
commit message why the new figure is right.

A dispatch over byte or token classes scores high and is not hard to
read; `complexity`'s own `walk` and `measure` are in the baseline for
that reason. Cyclomatic complexity counts branches, not difficulty.

## Unreferenced public items

Neither `dead_code` nor `unreachable_pub` can find a `pub` item that no
caller names, because every `pub` item in a library is reachable by
definition. The script counts references instead: one occurrence in the
whole tree means the definition and nothing else.

Read each result before deleting it. A name reached only through a
macro looks the same to a reference count.

## Coverage

`cargo llvm-cov` runs the unit and integration tests only. The parity
harnesses — `portcheck`, `paritycheck`, `archcheck`, `areacheck` — are
separate binaries, and they reach a great deal that the figure does not
show. Read it as a floor, not as the amount of the engine that is
tested.
