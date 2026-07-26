//! Compares parsed listings field by field.
//!
//! One caveat worth stating plainly: this measures the *printed* listing, which
//! VOACAP has already rounded — `REL` to two decimals, `SNR` to whole dB. So
//! the spread measured here is the spread a reader of the listing can observe,
//! not the spread in the engine's internal floats. That is the right quantity
//! for a port's acceptance criterion, because the listing is the contract
//! everything downstream reads, but a difference of zero here is not evidence
//! of bit-identical arithmetic.
//!
//! Raw differences are kept rather than summarised per case, so percentiles
//! over a whole sweep are exact instead of being averages of percentiles.

use std::collections::{BTreeMap, HashMap, HashSet};

use crate::listing::ParsedListing;

/// Values smaller than this use absolute difference only, since a relative
/// difference against a near-zero reference is not informative.
const REL_FLOOR: f64 = 1e-6;

#[derive(Debug, Clone, Default)]
struct FieldAccum {
    diffs: Vec<f64>,
    max_rel: f64,
    only_in_a: usize,
    only_in_b: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModeStats {
    pub compared: usize,
    pub mismatched: usize,
    pub only_in_a: usize,
    pub only_in_b: usize,
}

#[derive(Debug, Clone, Default)]
pub struct Comparison {
    fields: BTreeMap<String, FieldAccum>,
    pub modes: ModeStats,
}

/// Summary of one output row over everything compared.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldStats {
    pub row: String,
    /// Cells present in both listings.
    pub samples: usize,
    /// Cells whose printed value differs at all.
    pub differing: usize,
    pub max_abs: f64,
    pub p50_abs: f64,
    pub p95_abs: f64,
    pub p99_abs: f64,
    /// Largest absolute difference relative to the reference magnitude.
    pub max_rel: f64,
    /// Cells printed by one listing and not the other. A non-zero count means
    /// the two runs disagreed about whether a path existed at all, which no
    /// numeric tolerance can paper over.
    pub only_in_one: usize,
}

impl Comparison {
    pub fn is_empty(&self) -> bool {
        self.fields.is_empty()
    }

    /// Folds another comparison into this one, keeping raw differences.
    pub fn merge(&mut self, other: Comparison) {
        for (row, acc) in other.fields {
            let slot = self.fields.entry(row).or_default();
            slot.diffs.extend(acc.diffs);
            slot.max_rel = slot.max_rel.max(acc.max_rel);
            slot.only_in_a += acc.only_in_a;
            slot.only_in_b += acc.only_in_b;
        }
        self.modes.compared += other.modes.compared;
        self.modes.mismatched += other.modes.mismatched;
        self.modes.only_in_a += other.modes.only_in_a;
        self.modes.only_in_b += other.modes.only_in_b;
    }

    pub fn stats(&self) -> Vec<FieldStats> {
        self.fields
            .iter()
            .map(|(row, acc)| {
                let mut sorted = acc.diffs.clone();
                sorted.sort_by(|a, b| a.partial_cmp(b).expect("differences are never NaN"));
                FieldStats {
                    row: row.clone(),
                    samples: sorted.len(),
                    differing: sorted.iter().filter(|d| **d != 0.0).count(),
                    max_abs: sorted.last().copied().unwrap_or(0.0),
                    p50_abs: percentile(&sorted, 0.50),
                    p95_abs: percentile(&sorted, 0.95),
                    p99_abs: percentile(&sorted, 0.99),
                    max_rel: acc.max_rel,
                    only_in_one: acc.only_in_a + acc.only_in_b,
                }
            })
            .collect()
    }
}

fn percentile(sorted: &[f64], q: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (q * sorted.len() as f64).ceil() as usize;
    let index = rank.saturating_sub(1).min(sorted.len() - 1);
    sorted[index]
}

pub fn compare_listings(a: &ParsedListing, b: &ParsedListing) -> Comparison {
    let mut fields: BTreeMap<String, FieldAccum> = BTreeMap::new();

    let b_by_key: HashMap<(u8, &str, i8), f64> =
        b.numeric.iter().map(|s| (s.key(), s.value)).collect();
    let mut seen: HashSet<(u8, &str, i8)> = HashSet::with_capacity(a.numeric.len());

    for sa in &a.numeric {
        let key = sa.key();
        seen.insert(key);
        let acc = fields.entry(sa.row.clone()).or_default();
        match b_by_key.get(&key) {
            None => acc.only_in_a += 1,
            Some(&other) => {
                let diff = (sa.value - other).abs();
                acc.diffs.push(diff);
                let magnitude = sa.value.abs();
                if magnitude > REL_FLOOR {
                    acc.max_rel = acc.max_rel.max(diff / magnitude);
                }
            }
        }
    }

    for sb in &b.numeric {
        if !seen.contains(&sb.key()) {
            fields.entry(sb.row.clone()).or_default().only_in_b += 1;
        }
    }

    Comparison {
        fields,
        modes: compare_modes(a, b),
    }
}

fn compare_modes(a: &ParsedListing, b: &ParsedListing) -> ModeStats {
    let b_by_key: HashMap<(u8, i8), &str> =
        b.modes.iter().map(|m| (m.key(), m.mode.as_str())).collect();
    let mut seen: HashSet<(u8, i8)> = HashSet::with_capacity(a.modes.len());
    let mut stats = ModeStats::default();

    for ma in &a.modes {
        seen.insert(ma.key());
        match b_by_key.get(&ma.key()) {
            None => stats.only_in_a += 1,
            Some(&other) => {
                stats.compared += 1;
                if ma.mode != other {
                    stats.mismatched += 1;
                }
            }
        }
    }

    stats.only_in_b = b.modes.iter().filter(|m| !seen.contains(&m.key())).count();
    stats
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::listing::{ModeSample, Sample};

    fn sample(row: &str, slot: i8, value: f64) -> Sample {
        Sample {
            hour: 1,
            row: row.to_string(),
            slot,
            value,
        }
    }

    fn listing(values: &[(&str, i8, f64)]) -> ParsedListing {
        ParsedListing {
            numeric: values.iter().map(|(r, s, v)| sample(r, *s, *v)).collect(),
            modes: Vec::new(),
        }
    }

    fn stats_for(c: &Comparison, row: &str) -> FieldStats {
        c.stats()
            .into_iter()
            .find(|f| f.row == row)
            .unwrap_or_else(|| panic!("no stats for {row}"))
    }

    #[test]
    fn identical_listings_show_no_difference() {
        let a = listing(&[("SNR", 0, 10.0), ("REL", 0, 0.5)]);
        let c = compare_listings(&a, &a);
        assert_eq!(stats_for(&c, "SNR").differing, 0);
        assert_eq!(stats_for(&c, "SNR").max_abs, 0.0);
    }

    #[test]
    fn measures_absolute_and_relative_difference() {
        let a = listing(&[("SNR", 0, 100.0)]);
        let b = listing(&[("SNR", 0, 101.0)]);
        let s = stats_for(&compare_listings(&a, &b), "SNR");
        assert_eq!(s.max_abs, 1.0);
        assert!((s.max_rel - 0.01).abs() < 1e-12);
    }

    #[test]
    fn a_cell_missing_from_one_side_is_not_counted_as_a_difference() {
        // The two runs disagreed about whether the band was open at all. That
        // is a structural difference, and averaging it into a tolerance would
        // hide it.
        let a = listing(&[("SNR", 0, 10.0), ("SNR", 1, 20.0)]);
        let b = listing(&[("SNR", 0, 10.0)]);
        let s = stats_for(&compare_listings(&a, &b), "SNR");
        assert_eq!(s.samples, 1);
        assert_eq!(s.differing, 0);
        assert_eq!(s.only_in_one, 1);
    }

    #[test]
    fn near_zero_references_do_not_inflate_the_relative_figure() {
        let a = listing(&[("REL", 0, 0.0)]);
        let b = listing(&[("REL", 0, 0.01)]);
        let s = stats_for(&compare_listings(&a, &b), "REL");
        assert_eq!(s.max_abs, 0.01);
        assert_eq!(s.max_rel, 0.0);
    }

    #[test]
    fn merging_keeps_percentiles_exact() {
        // Ten cells differing by 1 and ninety by 0: the 95th percentile is 1
        // only if the raw values survive the merge.
        let mut merged = Comparison::default();
        for i in 0..100 {
            let value = if i < 10 { 1.0 } else { 0.0 };
            let a = listing(&[("SNR", 0, 0.0)]);
            let b = listing(&[("SNR", 0, value)]);
            merged.merge(compare_listings(&a, &b));
        }
        let s = stats_for(&merged, "SNR");
        assert_eq!(s.samples, 100);
        assert_eq!(s.differing, 10);
        assert_eq!(s.p50_abs, 0.0);
        assert_eq!(s.p95_abs, 1.0);
    }

    #[test]
    fn mode_mismatches_are_counted_separately() {
        let a = ParsedListing {
            numeric: Vec::new(),
            modes: vec![ModeSample {
                hour: 1,
                slot: 0,
                mode: "1F2".into(),
            }],
        };
        let b = ParsedListing {
            numeric: Vec::new(),
            modes: vec![ModeSample {
                hour: 1,
                slot: 0,
                mode: "2F2".into(),
            }],
        };
        let c = compare_listings(&a, &b);
        assert_eq!(c.modes.compared, 1);
        assert_eq!(c.modes.mismatched, 1);
    }
}
