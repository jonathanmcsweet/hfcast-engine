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

/// Isotropic at both ends unless the case names an antenna.
pub const ANTENNA_FILE: &str = "default/isotrope";

/// The `LINEMAX` card: lines per page before the header repeats.
pub const LINES_PER_PAGE: i32 = 55;

/// `SYSTEM` card fields no case varies. They are named here because the
/// listing header prints them, and printing one number while the card
/// carries another would make the two engines answer different
/// questions.
pub const MIN_ANGLE_DEG: f64 = 0.10;
pub const REQUIRED_RELIABILITY_PCT: i32 = 90;
pub const MULTIPATH_POWER_DB: f64 = 3.00;
pub const MULTIPATH_DELAY_MS: f64 = 0.10;

/// One `ANTENNA` card.
///
/// An end may have several. `GAIN` walks the cards in order and takes the
/// first one serving that end whose frequency range holds the frequency,
/// so several cards split the bands between them, and a frequency in no
/// card's range gets no antenna at all.
#[derive(Debug, Clone, PartialEq)]
pub struct AntennaChoice {
    /// Path under `<itshfbc>/antennas`, at most the card's 21 columns.
    pub file: String,
    pub design_freq: f64,
    /// Main beam bearing, degrees.
    pub beam_deg: f64,
    /// The card's frequency range in whole MHz (`minfreq`, `maxfreq`).
    pub min_freq: i32,
    pub max_freq: i32,
    /// The card's last field, ten columns wide. A transmit card carries
    /// kilowatts there and `None` writes [`DeckCase::watts`]; a receive
    /// card carries a gain that replaces the design frequency when it is
    /// not zero.
    pub last_field: Option<f64>,
}

impl AntennaChoice {
    /// One card over the whole 2 to 30 MHz range, taking the deck's power.
    pub fn whole_band(file: &str, beam_deg: f64) -> Self {
        Self {
            file: file.to_string(),
            design_freq: 0.0,
            beam_deg,
            min_freq: 2,
            max_freq: 30,
            last_field: None,
        }
    }

    /// The isotrope every end gets when the case names no antenna.
    pub fn isotrope() -> Self {
        Self::whole_band(ANTENNA_FILE, 0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DeckCase {
    /// Short stable name, used for run filenames and in the report. It
    /// is also the `LABEL` card's first twenty columns, which name the
    /// transmitter in the listing header.
    pub id: String,
    /// The `LABEL` card's second twenty columns, which name the
    /// receiver. The harness cases all use `sweep`.
    pub rx_label: String,
    pub from_lat: f64,
    pub from_lon: f64,
    pub to_lat: f64,
    pub to_lon: f64,
    /// The METHOD card's number; 30 is the smoothed systems model.
    pub method: u32,
    /// The COEFFS card: URSI88 foF2 coefficients instead of CCIR.
    pub ursi: bool,
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
    /// The transmitter's `ANTENNA` cards, in the order the deck writes
    /// them. Empty is one isotrope over 2 to 30 MHz.
    pub tx_antennas: Vec<AntennaChoice>,
    pub rx_antennas: Vec<AntennaChoice>,
    /// Enables VOACAP's sporadic-E layer (the FPROB card's fourth value).
    ///
    /// Standard practice runs with it off, because VOACAP's sporadic-E model
    /// is considered unreliable. That also removes a real summer mechanism,
    /// so the validation uses this switch to measure what turning it on
    /// changes. `decred.for` documents the card: each critical frequency is
    /// multiplied by the value, and the fourth is Es.
    pub sporadic_e: bool,
    /// The whole `FPROB` card when the case needs multipliers the
    /// [`DeckCase::sporadic_e`] switch cannot express: E, F1, F2 and
    /// sporadic E, each multiplying that layer's critical frequency.
    /// `None` is the switch's own card.
    pub fprob: Option<[f64; 4]>,
    /// A `BOTLINES` card: the body lines to print, in the order they
    /// are listed. It overrides whatever the method would select, and
    /// is how card method 23 says what to print.
    pub botlines: Option<Vec<u32>>,
    /// A `TOPLINES` card: which of the header's lines print. Like
    /// `BOTLINES` it overrides the method's own selection.
    pub toplines: Option<Vec<u32>>,
    /// The `EXECUTE` card's `KRUN` field, which says how much of the
    /// per-hour ionosphere the driver recomputes: 0 all of it, 1 all
    /// but sporadic E, 2 only the map evaluation and sporadic E, 3 or
    /// more none of it. Anything above zero is what lets an `EFVAR` or
    /// `ESVAR` card's values stand.
    pub krun: i32,
    /// `EFVAR` cards: per control point, the E, F1 and F2 critical
    /// frequency, semithickness and height of maximum.
    pub efvar: Vec<EfVar>,
    /// `ESVAR` cards: per control point, the sporadic-E lower decile,
    /// median and upper decile, and the height of reflection.
    pub esvar: Vec<EsVar>,
    /// An `EDP` card and the electron density profile that follows it:
    /// 50 true heights then 50 plasma frequencies squared. `LECDEN`
    /// then leaves the profile alone.
    pub edp: Option<Edp>,
    /// Cards written verbatim before the `METHOD` card. This is how the
    /// deck carries a card that reaches no computation — `FREEFORM` and
    /// `ANTOUT` both set a variable nothing reads — so that a run with
    /// one can be compared against the reference and shown to be
    /// unchanged.
    pub extra_cards: Vec<String>,
    /// A `COMMENT` card's text, which follows the ten-character name
    /// field. The card reaches no computation; it is here because a
    /// comment beginning `GROUP` is echoed in a different place from
    /// every other card.
    pub comment: Option<String>,
    /// An `INTEGRATE` card, carrying the value it puts in `IEDP`. The
    /// card decides how layer heights are obtained: without it `IEDP`
    /// stays at its program-start -1 and every height comes from the
    /// density profile; at zero or above the E layer takes fixed heights
    /// and a profile with no F1 layer takes parabolic segments.
    pub integrate: Option<i32>,
    /// An `OUTGRAPH` card: up to twelve further card methods whose MUF
    /// table or diurnal graph is printed after the run's own output. A
    /// negative number sends that method's pages to a second output
    /// unit, which the driver never opens.
    pub outgraph: Option<Vec<i32>>,
}

impl DeckCase {
    /// The four `FPROB` multipliers the deck carries, whether they come
    /// from the card or from the sporadic-E switch.
    pub fn fprob(&self) -> [f64; 4] {
        self.fprob
            .unwrap_or([1.0, 1.0, 1.0, if self.sporadic_e { 1.0 } else { 0.0 }])
    }

    /// The `LABEL` card's forty characters, which the listing header
    /// prints as four ten-character fields.
    pub fn label(&self) -> String {
        format!("{}{}", text(&self.id, 20), text(&self.rx_label, 20))
    }

    /// The `ANTENNA` cards this case writes, paired with the end each
    /// serves: every transmit card, then every receive card, which is the
    /// order they are numbered and the order `GAIN` searches. Each card's
    /// last field is resolved to the number that goes in the column, so
    /// the deck text and the engine's inputs cannot disagree about it.
    pub fn antenna_cards(&self) -> Vec<(i32, AntennaChoice)> {
        let kw = self.watts / 1000.0;
        let one_end = |iat: i32, listed: &[AntennaChoice]| -> Vec<(i32, AntennaChoice)> {
            let cards = if listed.is_empty() {
                vec![AntennaChoice::isotrope()]
            } else {
                listed.to_vec()
            };
            cards
                .into_iter()
                .map(|mut card| {
                    let default = if iat == 1 { kw } else { 0.0 };
                    card.last_field = Some(card.last_field.unwrap_or(default));
                    (iat, card)
                })
                .collect()
        };
        let mut out = one_end(1, &self.tx_antennas);
        out.extend(one_end(2, &self.rx_antennas));
        out
    }
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

/// A `TOPLINES` or `BOTLINES` card, or nothing when the case has none.
fn line_card(name: &str, lines: Option<&[u32]>) -> Result<String, DeckError> {
    let Some(lines) = lines else {
        return Ok(String::new());
    };
    let mut card = format!("{name}  ");
    for l in lines.iter().take(14) {
        card.push_str(&field(&l.to_string(), 5)?);
    }
    Ok(card)
}

/// The `OUTGRAPH` card: twelve `I5` fields of card method numbers.
fn outgraph_card(methods: Option<&[i32]>) -> Result<String, DeckError> {
    let Some(methods) = methods else {
        return Ok(String::new());
    };
    let mut card = String::from("OUTGRAPH  ");
    for m in methods.iter().take(12) {
        card.push_str(&field(&m.to_string(), 5)?);
    }
    Ok(card)
}

/// One `EFVAR` card.
#[derive(Debug, Clone, PartialEq)]
pub struct EfVar {
    /// Control point, 1 to 5.
    pub area: usize,
    /// Critical frequency, semithickness and height per layer.
    pub fi: [f64; 3],
    pub yi: [f64; 3],
    pub hi: [f64; 3],
}

/// One `ESVAR` card.
#[derive(Debug, Clone, PartialEq)]
pub struct EsVar {
    pub area: usize,
    pub fs: [f64; 3],
    pub hs: f64,
}

/// An `EDP` card with its profile.
#[derive(Debug, Clone, PartialEq)]
pub struct Edp {
    /// The card's sample area field. Outside 1 to 3 the driver reads it
    /// as 1.
    pub area: i32,
    pub htr: [f64; 50],
    pub fnsq: [f64; 50],
}

/// The `EFVAR`, `ESVAR` and `EDP` cards of a case, in that order.
fn override_cards(c: &DeckCase) -> Result<Vec<String>, DeckError> {
    let mut cards = Vec::new();
    for e in &c.efvar {
        let mut card = format!("EFVAR     {}", field(&e.area.to_string(), 5)?);
        for layer in 0..3 {
            card.push_str(&field(&format!("{:.2}", e.fi[layer]), 5)?);
            card.push_str(&field(&format!("{:.1}", e.yi[layer]), 5)?);
            card.push_str(&field(&format!("{:.1}", e.hi[layer]), 5)?);
        }
        cards.push(card);
    }
    for e in &c.esvar {
        let mut card = format!("ESVAR     {}", field(&e.area.to_string(), 5)?);
        for v in e.fs {
            card.push_str(&field(&format!("{v:.2}"), 5)?);
        }
        card.push_str(&field(&format!("{:.1}", e.hs), 5)?);
        cards.push(card);
    }
    if let Some(edp) = &c.edp {
        cards.push(format!("EDP       {}", field(&edp.area.to_string(), 5)?));
        // Two profiles of 50 values, each written 16 to a record. The
        // card's format is `F5.2`, but a height needs more than five
        // characters at two decimals, and an explicit decimal point
        // overrides the implied one on input, so one decimal is what
        // fits.
        for values in [&edp.htr, &edp.fnsq] {
            for chunk in values.chunks(16) {
                let mut record = String::new();
                for v in chunk {
                    record.push_str(&field(&format!("{v:.1}"), 5)?);
                }
                cards.push(record);
            }
        }
    }
    Ok(cards)
}

/// One value as its card column carries it: formatted the way
/// [`build_deck`] formats it, then read back.
///
/// Going through the formatter rather than arithmetic is what makes
/// the result exactly what the column holds, whatever rounding rule
/// the formatter applies.
fn as_card(v: f64, decimals: usize) -> f64 {
    format!("{v:.decimals$}").parse().unwrap_or(v)
}

/// A coordinate as its column carries it. The card writes the
/// magnitude and puts the sign in a hemisphere letter, so the
/// rounding applies to the magnitude.
///
/// A negative value that rounds to zero must come back as positive
/// zero: the card prints it `0.00N`, which the reference reads as
/// +0.0, and the sign of zero is observable through `atan2` — an
/// equatorial path could otherwise print a bearing of -180 against
/// the reference's 180.
fn as_card_coord(v: f64) -> f64 {
    let magnitude = as_card(v.abs(), 2);
    if v >= 0.0 || magnitude == 0.0 {
        magnitude
    } else {
        -magnitude
    }
}

impl DeckCase {
    /// The same case holding exactly what its cards would carry.
    ///
    /// A card column has a fixed number of decimals, so a deck cannot
    /// express a finer value. Running the engine from the unrounded
    /// case while echoing the rounded deck would print a listing whose
    /// header and numbers disagree — the header shows `35.88 N` beside
    /// a bearing computed from 35.8765. Passing a case through here
    /// first makes the text and the computation one description.
    ///
    /// **Edit this with [`build_deck`].** Every field below mirrors
    /// that function's format for the same field, and a field missed
    /// here is a field where the two disagree again. What proves they
    /// agree is `tests/api_reference.rs`, which runs the reference on
    /// the written deck and compares the whole listing.
    pub fn as_written(&self) -> DeckCase {
        let antenna = |cards: &[AntennaChoice]| -> Vec<AntennaChoice> {
            cards
                .iter()
                .map(|a| AntennaChoice {
                    file: a.file.clone(),
                    design_freq: as_card(a.design_freq, 3),
                    beam_deg: as_card(a.beam_deg, 1),
                    min_freq: a.min_freq,
                    max_freq: a.max_freq,
                    last_field: a.last_field.map(|v| as_card(v, 4)),
                })
                .collect()
        };
        DeckCase {
            id: self.id.clone(),
            rx_label: self.rx_label.clone(),
            from_lat: as_card_coord(self.from_lat),
            from_lon: as_card_coord(self.from_lon),
            to_lat: as_card_coord(self.to_lat),
            to_lon: as_card_coord(self.to_lon),
            method: self.method,
            ursi: self.ursi,
            month: self.month,
            year: self.year,
            // The card carries a whole number and a trailing point.
            ssn: self.ssn.round(),
            // Power reaches the deck as kilowatts in four decimals, so
            // the grid is a tenth of a watt.
            watts: as_card(self.watts / 1000.0, 4) * 1000.0,
            required_snr_db: as_card(self.required_snr_db, 1),
            // Written as the value then a point, so a fractional noise
            // figure would overflow the five-column field rather than
            // round. Whole numbers are the only values the card takes.
            noise_dbw: self.noise_dbw.round(),
            freqs_mhz: self.freqs_mhz.iter().map(|f| as_card(*f, 2)).collect(),
            tx_antennas: antenna(&self.tx_antennas),
            rx_antennas: antenna(&self.rx_antennas),
            sporadic_e: self.sporadic_e,
            fprob: self.fprob.map(|p| p.map(|v| as_card(v, 2))),
            botlines: self.botlines.clone(),
            toplines: self.toplines.clone(),
            krun: self.krun,
            efvar: self
                .efvar
                .iter()
                .map(|e| EfVar {
                    area: e.area,
                    fi: e.fi.map(|v| as_card(v, 2)),
                    yi: e.yi.map(|v| as_card(v, 1)),
                    hi: e.hi.map(|v| as_card(v, 1)),
                })
                .collect(),
            esvar: self
                .esvar
                .iter()
                .map(|e| EsVar {
                    area: e.area,
                    fs: e.fs.map(|v| as_card(v, 2)),
                    hs: as_card(e.hs, 1),
                })
                .collect(),
            edp: self.edp.as_ref().map(|e| Edp {
                area: e.area,
                htr: e.htr.map(|v| as_card(v, 1)),
                fnsq: e.fnsq.map(|v| as_card(v, 1)),
            }),
            extra_cards: self.extra_cards.clone(),
            comment: self.comment.clone(),
            integrate: self.integrate,
            outgraph: self.outgraph.clone(),
        }
    }
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

    // The cards are numbered from one across both ends, because the
    // second field is the slot in the engine's own antenna table.
    let mut antenna_lines = Vec::new();
    for (slot, (iat, card)) in c.antenna_cards().into_iter().enumerate() {
        if card.file.len() > 21 {
            return Err(DeckError::FieldOverflow {
                value: card.file,
                width: 21,
            });
        }
        antenna_lines.push(format!(
            "ANTENNA   {}{}{}{}{}{}{}{}",
            field(&iat.to_string(), 5)?,
            field(&(slot + 1).to_string(), 5)?,
            field(&card.min_freq.to_string(), 5)?,
            field(&card.max_freq.to_string(), 5)?,
            field(&format!("{:.3}", card.design_freq), 10)?,
            antenna_ref(&card.file),
            field(&format!("{:.1}", card.beam_deg), 5)?,
            field(&format!("{:.4}", card.last_field.unwrap_or(0.0)), 10)?
        ));
    }

    let mut lines = vec![
        format!(
            "LINEMAX   {}       number of lines-per-page",
            field(&LINES_PER_PAGE.to_string(), 5)?
        ),
        if c.ursi {
            "COEFFS    URSI88".to_string()
        } else {
            "COEFFS    CCIR".to_string()
        },
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
        format!("LABEL     {}", c.label()),
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
            field(&format!("{MIN_ANGLE_DEG:.2}"), 5)?,
            field(&format!("{REQUIRED_RELIABILITY_PCT}."), 5)?,
            field(&format!("{:.1}", c.required_snr_db), 5)?,
            field(&format!("{MULTIPATH_POWER_DB:.2}"), 5)?,
            field(&format!("{MULTIPATH_DELAY_MS:.2}"), 5)?
        ),
        {
            let p = c.fprob();
            format!(
                "FPROB     {}{}{}{}",
                field(&format!("{:.2}", p[0]), 5)?,
                field(&format!("{:.2}", p[1]), 5)?,
                field(&format!("{:.2}", p[2]), 5)?,
                field(&format!("{:.2}", p[3]), 5)?
            )
        },
    ];
    lines.append(&mut antenna_lines);
    lines.extend([
        format!("FREQUENCY {freq_card}"),
        // The TOPLINES and BOTLINES cards each take fourteen I5 fields;
        // the unused ones stay blank, which reads as zero and selects no
        // line.
        override_cards(c)?.join("\n"),
        c.extra_cards.join("\n"),
        match &c.comment {
            Some(text) => format!("COMMENT   {text}"),
            None => String::new(),
        },
        match c.integrate {
            Some(v) => format!("INTEGRATE {}", field(&v.to_string(), 5)?),
            None => String::new(),
        },
        outgraph_card(c.outgraph.as_deref())?,
        line_card("TOPLINES", c.toplines.as_deref())?,
        line_card("BOTLINES", c.botlines.as_deref())?,
        format!(
            "METHOD    {}{}",
            field(&c.method.to_string(), 5)?,
            field("0", 5)?
        ),
        format!("EXECUTE   {}", field(&c.krun.to_string(), 5)?),
        "QUIT".to_string(),
    ]);
    let lines: Vec<String> = lines.into_iter().filter(|l| !l.is_empty()).collect();

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
            rx_label: "sweep".to_string(),
            method: 30,
            ursi: false,
            fprob: None,
            botlines: None,
            toplines: None,
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
            tx_antennas: Vec::new(),
            rx_antennas: Vec::new(),
            sporadic_e: false,
            outgraph: None,
            integrate: None,
            comment: None,
            extra_cards: Vec::new(),
            krun: 0,
            efvar: Vec::new(),
            esvar: Vec::new(),
            edp: None,
        }
    }

    #[test]
    fn a_coordinate_rounding_to_zero_comes_back_as_positive_zero() {
        let mut c = a_case();
        c.from_lat = -0.004;
        c.from_lon = -0.0049;
        let w = c.as_written();
        // The card prints `0.00N`, which the reference reads as +0.0;
        // -0.0 here would let the port and the reference disagree
        // about `atan2` at the sign of zero.
        assert!(w.from_lat == 0.0 && w.from_lat.is_sign_positive());
        assert!(w.from_lon == 0.0 && w.from_lon.is_sign_positive());
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

    /// Two bands at the transmitter and one at the receiver: the slot
    /// numbers run 1, 2, 3 across both ends, each card carries its own
    /// frequency range, and the receive card's last field stays zero.
    #[test]
    fn several_cards_per_end_are_numbered_across_both_ends() {
        let mut c = a_case();
        c.watts = 1000.0;
        c.tx_antennas = vec![
            AntennaChoice {
                max_freq: 13,
                ..AntennaChoice::whole_band("samples/sample.21", 45.0)
            },
            AntennaChoice {
                min_freq: 14,
                last_field: Some(0.25),
                ..AntennaChoice::whole_band("samples/sample.31", 90.0)
            },
        ];
        c.rx_antennas = vec![AntennaChoice::whole_band("samples/sample.48", 0.0)];
        let deck = build_deck(&c).expect("deck");
        let cards: Vec<&str> = deck
            .lines()
            .filter(|l| l.starts_with("ANTENNA   "))
            .collect();
        assert_eq!(cards.len(), 3);
        assert_eq!(
            cards[0],
            "ANTENNA       1    1    2   13     0.000[samples/sample.21    ] 45.0    1.0000"
        );
        assert_eq!(
            cards[1],
            "ANTENNA       1    2   14   30     0.000[samples/sample.31    ] 90.0    0.2500"
        );
        assert_eq!(
            cards[2],
            "ANTENNA       2    3    2   30     0.000[samples/sample.48    ]  0.0    0.0000"
        );
    }

    /// A receive card's last field is a gain, not a power, so the deck's
    /// kilowatts must not leak into it.
    #[test]
    fn a_receive_card_carries_a_gain_in_its_last_field() {
        let mut c = a_case();
        c.rx_antennas = vec![AntennaChoice {
            last_field: Some(3.5),
            ..AntennaChoice::isotrope()
        }];
        let deck = build_deck(&c).expect("deck");
        let card = deck
            .lines()
            .filter(|l| l.starts_with("ANTENNA   "))
            .nth(1)
            .expect("a receive card");
        assert!(card.ends_with("    3.5000"), "got {card:?}");
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
