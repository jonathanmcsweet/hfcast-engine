//! Writes a VOACAP input deck for sweep cases.
//!
//! This is deliberately not a translation of the server's deck builder. The
//! server pins everything a consumer app never varies — isotropic antennas, the
//! amateur band plan, 100 W. A characterisation sweep has to vary them, so this
//! takes frequencies and system parameters directly.
//!
//! The format is punched-card fixed width: a 10-column keyword field, then
//! 5-column numeric fields with no separators. A value that fills its field
//! runs straight into the next one, which is legal and expected. Column
//! positions are the contract, so these fields are never joined with spaces.

use std::fmt;

/// Frequency slots on one FREQUENCY card.
pub const FREQ_SLOTS: usize = 11;

/// Isotropic at both ends.
///
/// Antenna gain enters the result as a per-slot offset, so varying it would add
/// a dimension to the sweep without exercising any more of the model's
/// arithmetic.
const ANTENNA_FILE: &str = "default/isotrope";

#[derive(Debug, Clone, PartialEq)]
pub struct DeckCase {
    /// Short stable name, used for run filenames and in the report.
    pub id: String,
    pub from_lat: f64,
    pub from_lon: f64,
    pub to_lat: f64,
    pub to_lon: f64,
    /// 1-12.
    pub month: u32,
    pub year: u32,
    pub ssn: f64,
    /// Transmit power in watts.
    pub watts: f64,
    /// Signal-to-noise ratio the mode needs, in dB.
    pub required_snr_db: f64,
    /// Man-made noise at 3 MHz, as a positive number of dBW below zero.
    pub noise_dbw: f64,
    /// Frequencies in MHz, ascending, at most [`FREQ_SLOTS`] of them.
    pub freqs_mhz: Vec<f64>,
    /// Enables VOACAP's sporadic-E layer (the FPROB card's fourth value).
    ///
    /// Standard practice runs with it off, because VOACAP's sporadic-E model
    /// is considered unreliable. That also removes a real summer mechanism,
    /// so the validation uses this switch to measure what turning it on
    /// changes. `decred.for` documents the card: each critical frequency is
    /// multiplied by the value, and the fourth is Es.
    pub sporadic_e: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum DeckError {
    /// A value was wider than the column it has to sit in. Silently truncating
    /// would shift every following field, so this is always fatal.
    FieldOverflow {
        value: String,
        width: usize,
    },
    MonthOutOfRange(u32),
    TooManyFrequencies(usize),
}

impl fmt::Display for DeckError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DeckError::FieldOverflow { value, width } => {
                write!(f, "value {value:?} overflows a {width}-column field")
            }
            DeckError::MonthOutOfRange(m) => write!(f, "month out of range: {m}"),
            DeckError::TooManyFrequencies(n) => {
                write!(f, "{n} frequencies given, at most {FREQ_SLOTS} fit")
            }
        }
    }
}

impl std::error::Error for DeckError {}

/// Right-justify in a fixed-width field, Fortran style.
fn field(value: &str, width: usize) -> Result<String, DeckError> {
    if value.len() > width {
        return Err(DeckError::FieldOverflow {
            value: value.to_string(),
            width,
        });
    }
    Ok(format!("{value:>width$}"))
}

/// Left-justify, truncating text that does not fit. Unlike a numeric field, a
/// label running long is cosmetic.
fn text(value: &str, width: usize) -> String {
    if value.len() > width {
        value[..width].to_string()
    } else {
        format!("{value:<width$}")
    }
}

/// `35.80N` — 5 columns of number then the hemisphere.
fn lat_compact(lat: f64) -> Result<String, DeckError> {
    let hemi = if lat >= 0.0 { 'N' } else { 'S' };
    Ok(format!("{}{hemi}", field(&format!("{:.2}", lat.abs()), 5)?))
}

/// 9 columns of number then the hemisphere.
fn lon_wide(lon: f64) -> Result<String, DeckError> {
    let hemi = if lon >= 0.0 { 'E' } else { 'W' };
    Ok(format!("{}{hemi}", field(&format!("{:.2}", lon.abs()), 9)?))
}

fn lat_wide(lat: f64) -> Result<String, DeckError> {
    let hemi = if lat >= 0.0 { 'N' } else { 'S' };
    Ok(format!("{}{hemi}", field(&format!("{:.2}", lat.abs()), 9)?))
}

/// An antenna file path, padded to the 21 columns inside the brackets.
fn antenna_ref(path: &str) -> String {
    format!("[{}]", text(path, 21))
}

pub fn build_deck(c: &DeckCase) -> Result<String, DeckError> {
    if c.freqs_mhz.len() > FREQ_SLOTS {
        return Err(DeckError::TooManyFrequencies(c.freqs_mhz.len()));
    }
    if c.month < 1 || c.month > 12 {
        return Err(DeckError::MonthOutOfRange(c.month));
    }

    let mut freq_card = String::new();
    for slot in 0..FREQ_SLOTS {
        let mhz = c.freqs_mhz.get(slot).copied().unwrap_or(0.0);
        freq_card.push_str(&field(&format!("{mhz:.2}"), 5)?);
    }

    // VOACAP takes transmit power in kilowatts.
    let kw = c.watts / 1000.0;

    let lines = vec![
        "LINEMAX      55       number of lines-per-page".to_string(),
        "COEFFS    CCIR".to_string(),
        // All 24 hours, stepping by one, in UTC.
        format!(
            "TIME      {}{}{}{}",
            field("1", 5)?,
            field("24", 5)?,
            field("1", 5)?,
            field("1", 5)?
        ),
        format!(
            "MONTH     {}{}",
            field(&c.year.to_string(), 5)?,
            field(&format!("{:.2}", c.month as f64), 5)?
        ),
        format!("SUNSPOT   {}", field(&format!("{}.", c.ssn.round()), 5)?),
        format!("LABEL     {}{}", text(&c.id, 20), text("sweep", 20)),
        format!(
            "CIRCUIT   {}{}{}{}  S     0",
            lat_compact(c.from_lat)?,
            lon_wide(c.from_lon)?,
            lat_wide(c.to_lat)?,
            lon_wide(c.to_lon)?
        ),
        format!(
            "SYSTEM    {}{}{}{}{}{}{}",
            field("1.", 5)?,
            field(&format!("{}.", c.noise_dbw), 5)?,
            field("0.10", 5)?,
            field("90.", 5)?,
            field(&format!("{:.1}", c.required_snr_db), 5)?,
            field("3.00", 5)?,
            field("0.10", 5)?
        ),
        format!(
            "FPROB      1.00 1.00 1.00 {}",
            if c.sporadic_e { "1.00" } else { "0.00" }
        ),
        format!(
            "ANTENNA   {}{}{}{}{}{}{}{}",
            field("1", 5)?,
            field("1", 5)?,
            field("2", 5)?,
            field("30", 5)?,
            field("0.000", 10)?,
            antenna_ref(ANTENNA_FILE),
            field("0.0", 5)?,
            field(&format!("{kw:.4}"), 10)?
        ),
        format!(
            "ANTENNA   {}{}{}{}{}{}{}{}",
            field("2", 5)?,
            field("2", 5)?,
            field("2", 5)?,
            field("30", 5)?,
            field("0.000", 10)?,
            antenna_ref(ANTENNA_FILE),
            field("0.0", 5)?,
            field("0.0000", 10)?
        ),
        format!("FREQUENCY {freq_card}"),
        format!("METHOD    {}{}", field("30", 5)?, field("0", 5)?),
        "EXECUTE".to_string(),
        "QUIT".to_string(),
    ];

    let mut deck = lines.join("\n");
    deck.push('\n');
    Ok(deck)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_case() -> DeckCase {
        DeckCase {
            id: "med-eu".to_string(),
            from_lat: 35.8,
            from_lon: -5.9,
            to_lat: 44.9,
            to_lon: 20.5,
            month: 6,
            year: 1994,
            ssn: 100.0,
            watts: 100.0,
            required_snr_db: 24.0,
            noise_dbw: 145.0,
            freqs_mhz: vec![6.07, 7.2, 9.7, 11.85],
            sporadic_e: false,
        }
    }

    fn line_starting(deck: &str, prefix: &str) -> String {
        deck.lines()
            .find(|l| l.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {prefix} card in deck"))
            .to_string()
    }

    #[test]
    fn circuit_card_matches_the_vendor_column_layout() {
        let deck = build_deck(&a_case()).expect("deck");
        // Byte-for-byte against the vendored test01.dat circuit card.
        assert_eq!(
            line_starting(&deck, "CIRCUIT"),
            "CIRCUIT   35.80N     5.90W    44.90N    20.50E  S     0"
        );
    }

    #[test]
    fn frequency_card_packs_five_column_slots_without_separators() {
        let deck = build_deck(&a_case()).expect("deck");
        let card = line_starting(&deck, "FREQUENCY");
        // 9.70 and 11.85 run together; that is the format, not a bug.
        assert!(card.contains("9.7011.85"), "got {card:?}");
        // Ten columns of keyword plus eleven 5-wide slots.
        assert_eq!(card.len(), 10 + FREQ_SLOTS * 5);
    }

    #[test]
    fn unused_frequency_slots_are_zero_filled() {
        let deck = build_deck(&a_case()).expect("deck");
        assert!(line_starting(&deck, "FREQUENCY").ends_with(" 0.00 0.00 0.00 0.00 0.00 0.00 0.00"));
    }

    #[test]
    fn southern_and_western_hemispheres_get_their_letters() {
        let mut c = a_case();
        c.from_lat = -33.87;
        c.from_lon = 151.21;
        let deck = build_deck(&c).expect("deck");
        let card = line_starting(&deck, "CIRCUIT");
        assert!(
            card.starts_with("CIRCUIT   33.87S   151.21E"),
            "got {card:?}"
        );
    }

    #[test]
    fn power_is_written_in_kilowatts() {
        let mut c = a_case();
        c.watts = 1500.0;
        let deck = build_deck(&c).expect("deck");
        assert!(line_starting(&deck, "ANTENNA   ").ends_with("    1.5000"));
    }

    #[test]
    fn rejects_a_month_outside_the_year() {
        let mut c = a_case();
        c.month = 13;
        assert_eq!(build_deck(&c), Err(DeckError::MonthOutOfRange(13)));
    }

    #[test]
    fn rejects_more_frequencies_than_the_card_holds() {
        let mut c = a_case();
        c.freqs_mhz = vec![1.0; FREQ_SLOTS + 1];
        assert_eq!(
            build_deck(&c),
            Err(DeckError::TooManyFrequencies(FREQ_SLOTS + 1))
        );
    }

    #[test]
    fn sporadic_e_switch_sets_the_fprob_card() {
        let mut c = a_case();
        let off = build_deck(&c).expect("deck");
        assert!(
            off.contains("FPROB      1.00 1.00 1.00 0.00"),
            "got {off:?}"
        );
        c.sporadic_e = true;
        let on = build_deck(&c).expect("deck");
        assert!(on.contains("FPROB      1.00 1.00 1.00 1.00"), "got {on:?}");
    }

    #[test]
    fn rejects_a_value_too_wide_for_its_column() {
        // A five-column field cannot hold a six-figure frequency, and quietly
        // truncating it would shift every field after it.
        let mut c = a_case();
        c.freqs_mhz = vec![123456.0];
        assert!(matches!(
            build_deck(&c),
            Err(DeckError::FieldOverflow { .. })
        ));
    }
}
