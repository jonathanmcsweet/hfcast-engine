//! Reads every field of a VOACAP method 30 listing.
//!
//! The server's TypeScript parser reads only `REL` and `SNR`, because that is
//! all the app shows. Setting a port tolerance needs the opposite: every number
//! the engine prints, because each output field has its own scale and its own
//! sensitivity to how the arithmetic was evaluated.
//!
//! Column geometry of a table row, 0-indexed and half-open:
//!
//! ```text
//!   [0, 6)    UTC hour, present only on the FREQ line
//!   [6, 11)   the value at the MUF
//!   [11, 66)  eleven 5-wide frequency slots
//!   [66, ..)  the row label
//! ```
//!
//! Two traps. Splitting on whitespace is wrong, because a value that fills its
//! field runs into its neighbour: `9.7011.85` is two numbers. Taking the last
//! whitespace token as the label is also wrong, because labels contain spaces
//! and `S DBW` and `N DBW` would both reduce to `DBW`.

use std::collections::BTreeSet;

const HOUR_END: usize = 6;
const MUF_END: usize = 11;
const FIRST_SLOT: usize = 11;
const SLOT_WIDTH: usize = 5;
const LABEL_START: usize = 66;

/// Frequency slots on one card.
pub const SLOT_COUNT: usize = 11;

/// Slot index standing for the at-the-MUF column, which is an output too.
pub const MUF_SLOT: i8 = -1;

/// The row name used for the MUF itself, printed in the FREQ line's MUF column.
pub const MUF_ROW: &str = "MUF";

/// Every numeric row a method 30 block prints, in listing order.
///
/// Restricting to a known set keeps page headers and the echoed input deck out
/// of the table without needing to model the listing's pagination.
pub const NUMERIC_ROWS: &[&str] = &[
    "TANGLE", "DELAY", "V HITE", "MUFday", "LOSS", "DBU", "S DBW", "N DBW", "SNR", "RPWRG", "REL",
    "MPROB", "S PRB", "SIG LW", "SIG UP", "SNR LW", "SNR UP", "TGAIN", "RGAIN", "SNRxx",
];

/// One printed numeric cell.
#[derive(Debug, Clone, PartialEq)]
pub struct Sample {
    /// UTC hour, 0-23.
    pub hour: u8,
    /// Row label, or [`MUF_ROW`] for the MUF itself.
    pub row: String,
    /// [`MUF_SLOT`] for the at-the-MUF column, otherwise the 0-based slot.
    pub slot: i8,
    pub value: f64,
}

impl Sample {
    /// Stable identity used to align the same cell across two listings.
    pub fn key(&self) -> (u8, &str, i8) {
        (self.hour, self.row.as_str(), self.slot)
    }
}

/// One printed propagation mode, such as `1F2`. Discrete: it matches or it does
/// not, so no tolerance can apply to it.
#[derive(Debug, Clone, PartialEq)]
pub struct ModeSample {
    pub hour: u8,
    pub slot: i8,
    pub mode: String,
}

impl ModeSample {
    pub fn key(&self) -> (u8, i8) {
        (self.hour, self.slot)
    }
}

#[derive(Debug, Clone, Default)]
pub struct ParsedListing {
    pub numeric: Vec<Sample>,
    pub modes: Vec<ModeSample>,
}

impl ParsedListing {
    /// Distinct row labels present, for checking a listing is fully covered.
    pub fn rows(&self) -> BTreeSet<&str> {
        self.numeric.iter().map(|s| s.row.as_str()).collect()
    }
}

/// Byte range of an ASCII line, clamped to its length.
///
/// The caller has already established the line is ASCII, so byte indices are
/// also character boundaries.
fn cols(line: &str, start: usize, end: usize) -> &str {
    let len = line.len();
    if start >= len {
        return "";
    }
    &line[start..end.min(len)]
}

fn label_of(line: &str) -> &str {
    cols(line, LABEL_START, usize::MAX).trim()
}

/// A 5-wide slot as text, or `None` where the listing prints a dash for a slot
/// no frequency was assigned to.
fn slot_text(line: &str, index: usize) -> Option<&str> {
    let start = FIRST_SLOT + index * SLOT_WIDTH;
    let raw = cols(line, start, start + SLOT_WIDTH).trim();
    if raw.is_empty() || raw == "-" {
        None
    } else {
        Some(raw)
    }
}

fn slot_number(line: &str, index: usize) -> Option<f64> {
    slot_text(line, index).and_then(|raw| raw.parse::<f64>().ok())
}

/// The at-the-MUF column as text. Every row has one, including `MODE`.
fn muf_text(line: &str) -> Option<&str> {
    let raw = cols(line, HOUR_END, MUF_END).trim();
    if raw.is_empty() || raw == "-" {
        None
    } else {
        Some(raw)
    }
}

fn muf_number(line: &str) -> Option<f64> {
    muf_text(line).and_then(|raw| raw.parse::<f64>().ok())
}

/// VOACAP numbers hours 1..=24, where 24 is the midnight ending the day. Fold
/// to 0..=23 so hours sort and index normally.
fn normalise_hour(raw: f64) -> u8 {
    ((raw.round() as i64).rem_euclid(24)) as u8
}

pub fn parse_listing(text: &str) -> ParsedListing {
    let numeric_rows: BTreeSet<&str> = NUMERIC_ROWS.iter().copied().collect();
    let mut out = ParsedListing::default();
    let mut hour: Option<u8> = None;

    for line in text.lines() {
        // The listing is plain ASCII. Anything else is not a table row, and
        // treating it as one would slice inside a multi-byte character.
        if !line.is_ascii() {
            continue;
        }

        let label = label_of(line);
        if label.is_empty() {
            continue;
        }

        if label == "FREQ" {
            let Ok(raw) = cols(line, 0, HOUR_END).trim().parse::<f64>() else {
                continue;
            };
            let h = normalise_hour(raw);
            hour = Some(h);
            if let Some(muf) = muf_number(line) {
                out.numeric.push(Sample {
                    hour: h,
                    row: MUF_ROW.to_string(),
                    slot: MUF_SLOT,
                    value: muf,
                });
            }
            continue;
        }

        // A labelled row outside any block means the listing is not shaped the
        // way this parser expects. Skipping is safer than guessing an hour.
        let Some(h) = hour else { continue };

        if label == "MODE" {
            // The mode at the MUF is printed in the same column as every other
            // row's at-the-MUF value, so it is an output like any other.
            if let Some(mode) = muf_text(line) {
                out.modes.push(ModeSample {
                    hour: h,
                    slot: MUF_SLOT,
                    mode: mode.to_string(),
                });
            }
            for s in 0..SLOT_COUNT {
                if let Some(mode) = slot_text(line, s) {
                    out.modes.push(ModeSample {
                        hour: h,
                        slot: s as i8,
                        mode: mode.to_string(),
                    });
                }
            }
            continue;
        }

        if !numeric_rows.contains(label) {
            continue;
        }

        if let Some(at_muf) = muf_number(line) {
            out.numeric.push(Sample {
                hour: h,
                row: label.to_string(),
                slot: MUF_SLOT,
                value: at_muf,
            });
        }
        for s in 0..SLOT_COUNT {
            if let Some(value) = slot_number(line, s) {
                out.numeric.push(Sample {
                    hour: h,
                    row: label.to_string(),
                    slot: s as i8,
                    value,
                });
            }
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One complete hour block, copied byte for byte from a real listing.
    const BLOCK: &str = concat!(
        "   1.0 16.3  6.1  7.2  9.7 11.9 13.7 15.4 17.7 21.6 25.9  0.0  0.0 FREQ\n",
        "        1F2  1F2  1F2  1F2  1F2  1F2  1F2  1F2  1F2  1F2   -    -  MODE  \n",
        "       13.7  7.8  7.8  8.1  8.7  9.5 10.9 14.7 14.7 14.7   -    -  TANGLE\n",
        "        109  100  101  102  104  105  107  122  163  230   -    -  LOSS  \n",
        "       -165 -149 -152 -158 -161 -163 -164 -166 -169 -171   -    -  N DBW \n",
        "        -52  -43  -44  -45  -47  -48  -50  -65 -106 -173   -    -  S DBW \n",
        "        113  106  109  113  114  115  114  102   62   -3   -    -  SNR   \n",
        "       0.97 1.00 1.00 1.00 1.00 1.00 0.98 0.91 0.30 0.00   -    -  REL   \n",
    );

    fn value_of(p: &ParsedListing, row: &str, slot: i8) -> Option<f64> {
        p.numeric
            .iter()
            .find(|s| s.row == row && s.slot == slot)
            .map(|s| s.value)
    }

    #[test]
    fn reads_the_muf_from_the_freq_line() {
        let p = parse_listing(BLOCK);
        assert_eq!(value_of(&p, MUF_ROW, MUF_SLOT), Some(16.3));
    }

    #[test]
    fn distinguishes_labels_that_share_a_last_word() {
        let p = parse_listing(BLOCK);
        // Splitting on whitespace would collapse `S DBW` and `N DBW` to `DBW`.
        assert_eq!(value_of(&p, "N DBW", MUF_SLOT), Some(-165.0));
        assert_eq!(value_of(&p, "S DBW", MUF_SLOT), Some(-52.0));
    }

    #[test]
    fn reads_run_together_fields_by_column() {
        let p = parse_listing(BLOCK);
        // Printed as `9.7011.85` with no separator on a full FREQUENCY card;
        // here the FREQ echo puts 21.6 and 25.9 in adjacent slots.
        assert_eq!(value_of(&p, "TANGLE", 7), Some(14.7));
        assert_eq!(value_of(&p, "TANGLE", 8), Some(14.7));
    }

    #[test]
    fn treats_dashes_as_absent_rather_than_zero() {
        let p = parse_listing(BLOCK);
        assert_eq!(value_of(&p, "SNR", 9), None);
        assert_eq!(value_of(&p, "SNR", 10), None);
        // Nine assigned frequencies plus the at-the-MUF column.
        assert_eq!(p.numeric.iter().filter(|s| s.row == "SNR").count(), 10);
    }

    #[test]
    fn folds_hour_24_to_zero() {
        let block = BLOCK.replacen("   1.0", "  24.0", 1);
        let p = parse_listing(&block);
        assert!(p.numeric.iter().all(|s| s.hour == 0));
    }

    #[test]
    fn ignores_the_echoed_input_deck() {
        // The FREQUENCY input card ends at column 66, so it has no label field
        // and must not be mistaken for a FREQ table row.
        let card = " FREQUENCY  6.07 7.20 9.7011.8513.7015.3517.7321.6525.89 0.00 0.00\n";
        assert!(parse_listing(card).numeric.is_empty());
    }

    #[test]
    fn records_modes_separately_from_numbers() {
        let p = parse_listing(BLOCK);
        assert_eq!(p.modes.len(), 10);
        assert!(p.modes.iter().all(|m| m.mode == "1F2"));
        assert!(!p.rows().contains("MODE"));
    }
}
