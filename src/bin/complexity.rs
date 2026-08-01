//! How complicated is each function, and is any of it getting worse?
//!
//! This is the part of a static-analysis suite that nothing else here
//! provides. Clippy reports cognitive complexity and function length as
//! warnings; it does not report cyclomatic complexity, and it has no way to
//! say "this function was already this complicated and that was accepted".
//! Both are needed, because a port of Fortran cannot be judged by the same
//! numbers as new code.
//!
//! The usual tool for this is `rust-code-analysis`, which cannot be built on
//! every machine: its grammars are C++ and need a host C++ toolchain. This
//! reads the source directly and needs nothing, which matches the rest of the
//! crate.
//!
//! # What it measures
//!
//! Per function, ignoring `#[cfg(test)]` modules:
//!
//! - **cyclomatic** — 1, plus one for each `if`, `while`, `for`, `loop`,
//!   match arm, `&&`, `||` and `?`. This is the number of independent paths
//!   through the function, and it is the count of decision points that
//!   McCabe defined and that SonarQube still reports.
//! - **lines** — first line of the body to the last.
//! - **nesting** — deepest block inside the function. Any block counts, so a
//!   struct literal or a closure adds a level; it is a relative signal, not
//!   an exact count of control-flow depth.
//! - **params** — arguments, not counting `self`.
//!
//! # Why there is a baseline
//!
//! Most of `voacap/` is one Rust function per Fortran subroutine, on purpose,
//! so a reader can hold the two side by side. Those functions are long and
//! branchy because the subroutines are, and splitting them would make the
//! port harder to audit and would risk moving arithmetic across a boundary.
//! That was decided in the 0.61.0 analysis pass and should not be argued
//! again every time somebody runs a linter.
//!
//! So the accepted state is written down in `tools/complexity-baseline.tsv`,
//! and `--check` fails on two things only: a function that got worse than its
//! recorded figure, and a new function over the thresholds. Existing
//! complexity is not a failure. Growing complexity is.
//!
//! # Usage
//!
//! ```text
//! cargo run --release --bin complexity            # the report, worst first
//! cargo run --release --bin complexity -- --check # gate against the baseline
//! cargo run --release --bin complexity -- --update # rewrite the baseline
//! ```

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Ceilings for code that is not in the baseline.
///
/// Ten is McCabe's own recommendation and fifteen is where SonarQube's
/// default profile complains. Fifteen is used here because the arithmetic in
/// this crate reaches for a branch more often than ordinary code does, and a
/// gate that cries wolf is a gate that gets turned off.
const MAX_CYCLOMATIC: u32 = 15;
const MAX_LINES: usize = 100;
const MAX_NESTING: u32 = 5;
const MAX_PARAMS: usize = 7;

const BASELINE: &str = "tools/complexity-baseline.tsv";

#[derive(Clone, Debug, PartialEq, Eq)]
struct Fun {
    file: String,
    name: String,
    line: usize,
    lines: usize,
    cyclomatic: u32,
    nesting: u32,
    params: usize,
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mode = args.first().map(String::as_str).unwrap_or("--report");

    let root = Path::new(".");
    let mut files = Vec::new();
    collect(&root.join("src"), &mut files);
    files.sort();

    let funs: Vec<Fun> = files
        .iter()
        .filter_map(|path| {
            let text = fs::read_to_string(path).ok()?;
            let name = path.strip_prefix(root).unwrap_or(path);
            Some(scan(&name.to_string_lossy(), &text))
        })
        .flatten()
        .collect();

    match mode {
        "--report" => report(&funs),
        "--check" => return check(&funs),
        "--update" => update(&funs),
        _ => {
            eprintln!("usage: complexity [--report|--check|--update]");
            return ExitCode::from(2);
        }
    }
    ExitCode::SUCCESS
}

/// Every `.rs` file under a directory, deepest last.
fn collect(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    // A directory walk is recursion over a tree, and the recursion is the
    // point: `map` would still need somewhere to put the descent.
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// Comments and literal contents replaced by spaces, newlines kept.
///
/// Everything after this can look at the bytes without asking whether a
/// brace is inside a string or a `for` is inside a comment. Newlines survive
/// so line numbers still mean something.
fn blanked(src: &str) -> Vec<u8> {
    let b = src.as_bytes();
    let mut out = b.to_vec();
    let mut i = 0usize;

    // A hand-written scanner has to be a loop: each step decides how far to
    // advance from what it just read, which is what an iterator cannot do.
    // Each arm hands back the byte to carry on from.
    while i < b.len() {
        i = match b[i] {
            b'/' if b.get(i + 1) == Some(&b'/') => line_comment(b, &mut out, i),
            b'/' if b.get(i + 1) == Some(&b'*') => block_comment(b, &mut out, i),
            b'r' | b'b' if raw_open(b, i).is_some() => raw_string(b, &mut out, i),
            b'"' => string(b, &mut out, i),
            // A quote is a character literal or a lifetime, and the two are
            // told apart only by what follows. `'a'` closes; `'a` in
            // `&'a str` does not.
            b'\'' => match char_literal_end(b, i) {
                Some(end) => blank(&mut out, i, end + 1),
                None => i + 1,
            },
            _ => i + 1,
        };
    }
    out
}

/// Spaces from `from` up to `to`, leaving newlines alone. Returns `to`.
fn blank(out: &mut [u8], from: usize, to: usize) -> usize {
    let end = to.min(out.len());
    for byte in out.iter_mut().take(end).skip(from) {
        if *byte != b'\n' {
            *byte = b' ';
        }
    }
    end
}

fn line_comment(b: &[u8], out: &mut [u8], i: usize) -> usize {
    let end = (i..b.len()).find(|&j| b[j] == b'\n').unwrap_or(b.len());
    blank(out, i, end)
}

/// Block comments nest in Rust, so this counts rather than looking for the
/// first `*/`.
fn block_comment(b: &[u8], out: &mut [u8], i: usize) -> usize {
    let mut depth = 0usize;
    let mut j = i;
    while j < b.len() {
        if b[j] == b'/' && b.get(j + 1) == Some(&b'*') {
            depth += 1;
            j += 2;
        } else if b[j] == b'*' && b.get(j + 1) == Some(&b'/') {
            depth -= 1;
            j += 2;
            if depth == 0 {
                break;
            }
        } else {
            j += 1;
        }
    }
    blank(out, i, j)
}

fn raw_string(b: &[u8], out: &mut [u8], i: usize) -> usize {
    let Some((body, hashes)) = raw_open(b, i) else {
        return i + 1;
    };
    let mut j = body;
    while j < b.len() {
        if b[j] == b'"' && closes_raw(b, j + 1, hashes) {
            j += 1 + hashes;
            break;
        }
        j += 1;
    }
    blank(out, i, j)
}

fn string(b: &[u8], out: &mut [u8], i: usize) -> usize {
    let mut j = i + 1;
    while j < b.len() {
        match b[j] {
            b'\\' => j += 2,
            b'"' => {
                j += 1;
                break;
            }
            _ => j += 1,
        }
    }
    blank(out, i, j)
}

/// Where a raw string's body starts, and how many hashes close it.
fn raw_open(b: &[u8], i: usize) -> Option<(usize, usize)> {
    if i > 0 && (b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_') {
        return None;
    }
    let mut j = i;
    if b[j] == b'b' {
        j += 1;
    }
    if b.get(j) != Some(&b'r') {
        return None;
    }
    j += 1;
    let start_hashes = j;
    while b.get(j) == Some(&b'#') {
        j += 1;
    }
    if b.get(j) == Some(&b'"') {
        Some((j + 1, j - start_hashes))
    } else {
        None
    }
}

fn closes_raw(b: &[u8], at: usize, hashes: usize) -> bool {
    (0..hashes).all(|k| b.get(at + k) == Some(&b'#'))
}

/// The closing quote of a character literal, or `None` for a lifetime.
fn char_literal_end(b: &[u8], i: usize) -> Option<usize> {
    if b.get(i + 1) == Some(&b'\\') {
        // An escape runs to the next quote: `'\n'`, `'\u{1F600}'`.
        return (i + 2..b.len().min(i + 12)).find(|&j| b[j] == b'\'');
    }
    if b.get(i + 2) == Some(&b'\'') {
        return Some(i + 2);
    }
    None
}

/// Every function in one file.
fn scan(file: &str, src: &str) -> Vec<Fun> {
    let b = blanked(src);
    let skip = test_modules(&b);
    let mut out = Vec::new();
    let mut i = 0usize;

    // Positional scanning again: a match on `fn` has to jump past the whole
    // function it just measured, including any functions nested in it.
    while i + 2 <= b.len() {
        if !word_at(&b, i, b"fn") {
            i += 1;
            continue;
        }
        if skip.iter().any(|(s, e)| i >= *s && i < *e) {
            i += 2;
            continue;
        }
        let Some(fun) = measure(file, &b, i) else {
            i += 2;
            continue;
        };
        i = fun.1;
        out.push(fun.0);
    }
    out
}

/// Byte spans of `#[cfg(test)]` module bodies.
fn test_modules(b: &[u8]) -> Vec<(usize, usize)> {
    let needle = b"#[cfg(test)]";
    let mut out = Vec::new();
    let mut i = 0usize;
    while i + needle.len() <= b.len() {
        if &b[i..i + needle.len()] == needle {
            if let Some(open) = (i..b.len()).find(|&j| b[j] == b'{') {
                if let Some(close) = matching(b, open) {
                    out.push((i, close));
                    i = close;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// One function, measured, and the byte to carry on from.
fn measure(file: &str, b: &[u8], at: usize) -> Option<(Fun, usize)> {
    let mut i = at + 2;
    while i < b.len() && b[i].is_ascii_whitespace() {
        i += 1;
    }
    let name_start = i;
    while i < b.len() && (b[i].is_ascii_alphanumeric() || b[i] == b'_') {
        i += 1;
    }
    if i == name_start {
        // `fn(u8) -> u8` is a type, not a definition.
        return None;
    }
    let name = String::from_utf8_lossy(&b[name_start..i]).to_string();

    let open_paren = (i..b.len()).find(|&j| b[j] == b'(')?;
    let close_paren = matching_delim(b, open_paren, b'(', b')')?;
    let params = count_params(&b[open_paren + 1..close_paren]);

    // The body is the first brace after the signature. A trait method with
    // no body ends in a semicolon instead, and is not a function to measure.
    let mut j = close_paren + 1;
    let mut angle = 0i32;
    let mut square = 0i32;
    let body_open = loop {
        if j >= b.len() {
            return None;
        }
        match b[j] {
            b'<' => angle += 1,
            b'>' if angle > 0 => angle -= 1,
            b'[' => square += 1,
            b']' => square -= 1,
            b';' if angle == 0 && square == 0 => return None,
            b'{' if angle == 0 && square == 0 => break j,
            _ => {}
        }
        j += 1;
    };
    let body_close = matching(b, body_open)?;

    let body = &b[body_open..=body_close];
    let (cyclomatic, nesting) = walk(body);

    Some((
        Fun {
            file: file.to_string(),
            name,
            line: line_of(b, at),
            lines: line_of(b, body_close) - line_of(b, body_open) + 1,
            cyclomatic,
            nesting,
            params,
        },
        body_close + 1,
    ))
}

/// Arguments between the parentheses, not counting `self`.
fn count_params(inner: &[u8]) -> usize {
    let text = String::from_utf8_lossy(inner);
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0;
    }
    let mut depth = 0i32;
    let mut parts = 1usize;
    for &byte in inner {
        match byte {
            b'(' | b'[' | b'<' => depth += 1,
            b')' | b']' | b'>' => depth -= 1,
            b',' if depth == 0 => parts += 1,
            _ => {}
        }
    }
    if trimmed.ends_with(',') {
        parts -= 1;
    }
    let first = trimmed.split(',').next().unwrap_or("");
    if first.replace(['&', ' '], "").starts_with("mutself")
        || first.replace(['&', ' '], "").starts_with("self")
    {
        parts -= 1;
    }
    parts
}

/// Decision points and depth, over one function body.
fn walk(body: &[u8]) -> (u32, u32) {
    let mut cyclomatic = 1u32;
    let mut depth = 0i32;
    let mut deepest = 0i32;
    let mut i = 0usize;

    // One pass, deciding per byte how far to advance: the keyword tests need
    // to consume the whole word so `iffy` is not read as `if`.
    while i < body.len() {
        match body[i] {
            b'{' => {
                depth += 1;
                deepest = deepest.max(depth);
                i += 1;
            }
            b'}' => {
                depth -= 1;
                i += 1;
            }
            b'&' if body.get(i + 1) == Some(&b'&') => {
                cyclomatic += 1;
                i += 2;
            }
            b'|' if body.get(i + 1) == Some(&b'|') => {
                cyclomatic += 1;
                i += 2;
            }
            b'=' if body.get(i + 1) == Some(&b'>') => {
                cyclomatic += 1;
                i += 2;
            }
            b'?' => {
                cyclomatic += 1;
                i += 1;
            }
            c if c.is_ascii_alphabetic() || c == b'_' => {
                let start = i;
                while i < body.len() && (body[i].is_ascii_alphanumeric() || body[i] == b'_') {
                    i += 1;
                }
                let word = &body[start..i];
                if matches!(
                    word,
                    b"if" | b"while" | b"for" | b"loop" | b"match" | b"else"
                ) && word != b"else"
                {
                    cyclomatic += 1;
                }
                // `match` is counted through its arms, not its head: one arm
                // is one path, and a two-arm match is one decision.
                if word == b"match" {
                    cyclomatic -= 1;
                }
            }
            _ => i += 1,
        }
    }
    (cyclomatic, deepest.max(0) as u32)
}

fn word_at(b: &[u8], i: usize, word: &[u8]) -> bool {
    if i + word.len() > b.len() || &b[i..i + word.len()] != word {
        return false;
    }
    let before = i == 0 || !(b[i - 1].is_ascii_alphanumeric() || b[i - 1] == b'_');
    let after = b
        .get(i + word.len())
        .is_none_or(|c| !(c.is_ascii_alphanumeric() || *c == b'_'));
    before && after
}

fn matching(b: &[u8], open: usize) -> Option<usize> {
    matching_delim(b, open, b'{', b'}')
}

fn matching_delim(b: &[u8], open: usize, o: u8, c: u8) -> Option<usize> {
    let mut depth = 0i32;
    // Bracket matching is inherently sequential state.
    for (offset, &byte) in b[open..].iter().enumerate() {
        if byte == o {
            depth += 1;
        } else if byte == c {
            depth -= 1;
            if depth == 0 {
                return Some(open + offset);
            }
        }
    }
    None
}

fn line_of(b: &[u8], at: usize) -> usize {
    b[..at.min(b.len())].iter().filter(|&&c| c == b'\n').count() + 1
}

fn over(f: &Fun) -> bool {
    f.cyclomatic > MAX_CYCLOMATIC
        || f.lines > MAX_LINES
        || f.nesting > MAX_NESTING
        || f.params > MAX_PARAMS
}

fn report(funs: &[Fun]) {
    let mut sorted: Vec<&Fun> = funs.iter().collect();
    sorted.sort_by(|a, b| b.cyclomatic.cmp(&a.cyclomatic).then(a.file.cmp(&b.file)));

    println!("{} functions in src/", funs.len());
    let total: u32 = funs.iter().map(|f| f.cyclomatic).sum();
    println!(
        "mean cyclomatic {:.1}, over the thresholds: {}",
        f64::from(total) / funs.len().max(1) as f64,
        funs.iter().filter(|f| over(f)).count()
    );
    println!();
    println!("cyclo lines nest params  where");
    for f in sorted.iter().filter(|f| over(f)) {
        println!(
            "{:5} {:5} {:4} {:6}  {}:{} {}",
            f.cyclomatic, f.lines, f.nesting, f.params, f.file, f.line, f.name
        );
    }

    let mut worst: BTreeMap<&str, u32> = BTreeMap::new();
    for f in funs {
        *worst.entry(f.file.as_str()).or_default() += f.cyclomatic;
    }
    let mut by_file: Vec<(&&str, &u32)> = worst.iter().collect();
    by_file.sort_by(|a, b| b.1.cmp(a.1));
    println!();
    println!("heaviest files by total cyclomatic complexity");
    for (file, sum) in by_file.iter().take(10) {
        println!("{sum:6}  {file}");
    }
}

fn key(f: &Fun) -> String {
    format!("{}\t{}", f.file, f.name)
}

fn read_baseline() -> BTreeMap<String, (u32, usize)> {
    fs::read_to_string(BASELINE)
        .unwrap_or_default()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.trim().is_empty())
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let file = parts.next()?;
            let name = parts.next()?;
            let cyclo = parts.next()?.parse().ok()?;
            let lines = parts.next()?.parse().ok()?;
            Some((format!("{file}\t{name}"), (cyclo, lines)))
        })
        .collect()
}

fn check(funs: &[Fun]) -> ExitCode {
    let base = read_baseline();
    let mut failures = Vec::new();

    for f in funs {
        match base.get(&key(f)) {
            Some(&(cyclo, lines)) => {
                if f.cyclomatic > cyclo {
                    failures.push(format!(
                        "{}:{} {} cyclomatic {} was {}",
                        f.file, f.line, f.name, f.cyclomatic, cyclo
                    ));
                }
                if f.lines > lines {
                    failures.push(format!(
                        "{}:{} {} is {} lines, was {}",
                        f.file, f.line, f.name, f.lines, lines
                    ));
                }
            }
            None if over(f) => failures.push(format!(
                "{}:{} {} is new and over: cyclomatic {} lines {} nesting {} params {}",
                f.file, f.line, f.name, f.cyclomatic, f.lines, f.nesting, f.params
            )),
            None => {}
        }
    }

    let live: Vec<String> = funs.iter().map(key).collect();
    let stale: Vec<&String> = base.keys().filter(|k| !live.contains(k)).collect();
    if !stale.is_empty() {
        println!("{} baseline entries no longer exist:", stale.len());
        for k in &stale {
            println!("  {}", k.replace('\t', " "));
        }
        println!("run --update to drop them");
    }

    if failures.is_empty() {
        println!("complexity: no function got worse, nothing new is over the thresholds");
        return ExitCode::SUCCESS;
    }
    println!("complexity: {} regressions", failures.len());
    for f in &failures {
        println!("  {f}");
    }
    ExitCode::FAILURE
}

fn update(funs: &[Fun]) {
    let mut sorted: Vec<&Fun> = funs.iter().filter(|f| over(f)).collect();
    sorted.sort_by(|a, b| a.file.cmp(&b.file).then(a.name.cmp(&b.name)));

    let body: String = sorted
        .iter()
        .map(|f| format!("{}\t{}\t{}\t{}\n", f.file, f.name, f.cyclomatic, f.lines))
        .collect();

    let header = "\
# Accepted complexity, written by `cargo run --bin complexity -- --update`.
#
# Only functions over the thresholds are listed. `--check` fails when one of
# these gets worse, or when a function not listed here goes over. Most of
# these are one Rust function per Fortran subroutine, which is deliberate:
# see the module documentation in src/bin/complexity.rs.
#
# file\tfunction\tcyclomatic\tlines
";
    if let Err(e) = fs::write(BASELINE, format!("{header}{body}")) {
        eprintln!("could not write {BASELINE}: {e}");
        return;
    }
    println!("{BASELINE}: {} functions recorded", sorted.len());
}

#[cfg(test)]
mod tests {
    use super::*;

    fn one(src: &str) -> Fun {
        let funs = scan("t.rs", src);
        assert_eq!(funs.len(), 1, "expected one function in {src:?}");
        funs.into_iter().next().unwrap()
    }

    #[test]
    fn a_straight_line_function_is_one() {
        assert_eq!(one("fn f() { let a = 1; let b = 2; }").cyclomatic, 1);
    }

    #[test]
    fn each_branch_adds_one() {
        assert_eq!(one("fn f(x: i32) { if x > 0 { } }").cyclomatic, 2);
        assert_eq!(one("fn f(x: i32) { if x > 0 { } else { } }").cyclomatic, 2);
        assert_eq!(
            one("fn f(x: i32) { if x > 0 { } else if x < 0 { } }").cyclomatic,
            3
        );
    }

    #[test]
    fn a_match_counts_its_arms_and_not_its_head() {
        assert_eq!(
            one("fn f(x: i32) { match x { 1 => (), _ => () } }").cyclomatic,
            3
        );
    }

    #[test]
    fn logical_operators_are_decisions() {
        assert_eq!(one("fn f(a: bool, b: bool) { let _ = a && b; }").cyclomatic, 2);
        assert_eq!(one("fn f(a: bool, b: bool) { let _ = a || b; }").cyclomatic, 2);
    }

    #[test]
    fn the_question_mark_is_a_branch() {
        assert_eq!(
            one("fn f() -> Option<u8> { let x = g()?; Some(x) }").cyclomatic,
            2
        );
    }

    #[test]
    fn loops_count_and_a_word_that_starts_with_one_does_not() {
        assert_eq!(one("fn f() { for _ in 0..3 { } }").cyclomatic, 2);
        assert_eq!(one("fn f() { let iffy = 1; let former = 2; }").cyclomatic, 1);
    }

    #[test]
    fn a_keyword_inside_a_string_or_comment_is_not_a_branch() {
        assert_eq!(one(r#"fn f() { let s = "if a && b"; }"#).cyclomatic, 1);
        assert_eq!(one("fn f() { /* if a && b */ let x = 1; }").cyclomatic, 1);
        assert_eq!(one("fn f() { // if a && b\n let x = 1; }").cyclomatic, 1);
    }

    #[test]
    fn a_raw_string_holding_a_brace_does_not_end_the_function() {
        let f = one("fn f() { let s = r#\"} if x\"#; let y = 1; }");
        assert_eq!(f.cyclomatic, 1);
    }

    #[test]
    fn a_lifetime_is_not_a_character_literal() {
        let f = one("fn f<'a>(s: &'a str) -> &'a str { if s.is_empty() { s } else { s } }");
        assert_eq!(f.cyclomatic, 2);
        assert_eq!(f.params, 1);
    }

    #[test]
    fn self_is_not_a_parameter() {
        assert_eq!(one("impl S { fn f(&self, a: u8) { let _ = a; } }").params, 1);
        assert_eq!(one("impl S { fn f(&self) { } }").params, 0);
    }

    #[test]
    fn a_generic_argument_is_not_a_second_parameter() {
        assert_eq!(one("fn f(a: Map<K, V>) { let _ = a; }").params, 1);
    }

    #[test]
    fn nesting_is_the_deepest_block() {
        assert_eq!(one("fn f() { { { } } }").nesting, 3);
    }

    #[test]
    fn a_trait_method_without_a_body_is_not_measured() {
        assert!(scan("t.rs", "trait T { fn f(&self); }").is_empty());
    }

    #[test]
    fn a_function_pointer_type_is_not_a_definition() {
        assert!(scan("t.rs", "type F = fn(u8) -> u8;").is_empty());
    }

    #[test]
    fn test_modules_are_left_out() {
        let src = "fn f() { }\n#[cfg(test)]\nmod tests { fn g() { if true { } } }\n";
        let funs = scan("t.rs", src);
        assert_eq!(funs.len(), 1);
        assert_eq!(funs[0].name, "f");
    }

    #[test]
    fn a_nested_function_is_measured_once_with_its_parent() {
        // The inner function's decisions belong to the outer one's body as
        // well, which is what a reader of the outer function faces.
        let funs = scan("t.rs", "fn outer() { fn inner() { if true { } } }");
        assert_eq!(funs.len(), 1);
        assert_eq!(funs[0].name, "outer");
    }

    #[test]
    fn line_numbers_point_at_the_definition() {
        let funs = scan("t.rs", "\n\nfn f() {\n\n}\n");
        assert_eq!(funs[0].line, 3);
        assert_eq!(funs[0].lines, 3);
    }
}
