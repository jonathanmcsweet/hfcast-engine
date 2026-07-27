//! The printed listing, byte for byte: `LISTIN`'s preamble, `OUTTOP`'s
//! header block, `OUTBOD`'s body and the end-of-run line.
//!
//! Nothing here computes anything. Every number these routines print is
//! already checked through the cell comparison; what this module adds is
//! the surrounding text and the page breaks, so a whole run can be
//! compared against the reference as text rather than as parsed cells.
//!
//! Two things decide the shape of the output and both are modelled here:
//!
//! - `SETOUT` chooses which header lines and which body rows the method
//!   prints, and `TOPLINES` and `BOTLINES` cards override the choice.
//! - `OUTLIN` counts printed lines against the `LINEMAX` card and calls
//!   `OUTTOP` again whenever the next hour would not fit, so where the
//!   header repeats depends on how many rows the method prints.

use std::path::Path;

use crate::deck::{
    DeckCase, MIN_ANGLE_DEG, MULTIPATH_DELAY_MS, MULTIPATH_POWER_DB, REQUIRED_RELIABILITY_PCT,
};

use super::antenna::AntennaSet;
use super::con::R;
use super::run::{HourPrediction, PathReport};

/// The three-letter month names of `/ALPHA/`'s `IMON`.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// The model name `voacapw` sets: eight characters, right-aligned.
pub const MODEL: &str = "  VOACAP";

/// Reads the version string the header prints from the tree.
///
/// The reference opens `database/version.<compiler>` and reads it with
/// `(8x,a)` into a `CHARACTER*8`, so the label is the eight characters
/// following `Version `. It stops with an error when the file is
/// missing, and so does this.
pub fn read_version(itshfbc: &Path) -> Result<String, String> {
    let path = itshfbc.join("database").join("version.w32");
    let text = std::fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
    let line = text.lines().next().unwrap_or("");
    let rest: String = line.chars().skip(8).collect();
    Ok(text_field(&rest, 8))
}

/// A `CHARACTER*n` field: truncated to `width`, blank-padded to it.
pub(crate) fn text_field(s: &str, width: usize) -> String {
    let mut out: String = s.chars().take(width).collect();
    while out.chars().count() < width {
        out.push(' ');
    }
    out
}

/// Fortran `Fw.d`, asterisks when the number does not fit.
///
/// `F4.0` prints `117.` — Fortran always writes the decimal point, where
/// Rust's `{:.0}` leaves it off.
///
/// The reference is built with gfortran's `-fno-sign-zero`, so a negative
/// value that rounds to zero at this many decimals loses its sign: -0.03
/// under `F6.1` prints `   0.0`, not `  -0.0`. Rust keeps the sign, so
/// strip it here.
pub(crate) fn f(v: R, width: usize, decimals: usize) -> String {
    let mut digits = format!("{:.decimals$}", f64::from(v));
    if !digits.contains(|c: char| ('1'..='9').contains(&c)) {
        digits = digits.trim_start_matches('-').to_string();
    }
    let point = if decimals == 0 { "." } else { "" };
    let s = format!("{:>width$}", format!("{digits}{point}"));
    if s.len() > width {
        "*".repeat(width)
    } else {
        s
    }
}

/// Fortran `Iw`.
pub(crate) fn i(v: i64, width: usize) -> String {
    let s = format!("{v:>width$}");
    if s.len() > width {
        "*".repeat(width)
    } else {
        s
    }
}

/// Which of `OUTTOP`'s lines print.
///
/// `SETOUT` turns on lines 1 to `NTOP` and stores `NTOP` in
/// `LINTOP(15)`, which `OUTTOP` then uses as its line count. A
/// `TOPLINES` card replaces the selection with an arbitrary set — for
/// any method, not only method 23, because the jump that was meant to
/// skip that block for other methods is commented out in `SETOUT`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TopLines {
    /// Lines 1 to 14, indexed from zero. `OUTTOP` reads only the first
    /// seven; a card may name the rest, which then print nothing but
    /// still count.
    pub on: [bool; 14],
    /// `LINTOP(15)`, the count `OUTTOP` charges to the page. It is
    /// negative for method 23 without a `TOPLINES` card: `SETOUT` clears
    /// all fifteen slots to -1 and then jumps past the statement that
    /// would set the count, so the page arithmetic runs on -1.
    pub count: i32,
}

/// `SETOUT`'s `NTOP` for a method, before any `TOPLINES` card.
fn ntop(method: u32) -> i32 {
    // The order matters: the `ITRUN` tests come first in `SETOUT`, so a
    // MUF-only method never reaches the per-method values below.
    match rewritten(method) {
        3..=11 => 3,  // ITRUN 3 and 4
        26..=29 => 6, // ITRUN 8
        16 | 17 => 7,
        18 => 6,
        19 => 3,
        20..=22 => 7,
        24 | 25 => 6,
        // Method 23 never reaches the statement that sets the count.
        23 => -1,
        _ => 3,
    }
}

pub fn top_lines(method: u32, card: Option<&[u32]>) -> TopLines {
    let Some(card) = card else {
        let count = ntop(method);
        let mut on = [false; 14];
        for slot in on.iter_mut().take(count.max(0) as usize) {
            *slot = true;
        }
        return TopLines { on, count };
    };
    // The card's own count, which counts a repeated line twice: `SETOUT`
    // adds one per accepted field rather than counting the lines it
    // turned on.
    let mut on = [false; 14];
    let mut count = 0i32;
    for line in card.iter().take(14) {
        if *line == 0 || *line > 14 {
            continue;
        }
        on[(*line - 1) as usize] = true;
        count += 1;
    }
    TopLines { on, count }
}

/// `LINBOT(26)` as `SETOUT` leaves it: how many body rows the page break
/// charges for an hour before the first hour has printed.
///
/// A method 23 deck with no `BOTLINES` card never reaches the statement
/// that sets this either, so the count is the -1 `SETOUT` cleared the
/// whole array to. Each hour then charges one line instead of the two
/// it prints, which is why such a run gets a second page only after
/// dozens of hours.
pub fn nbod(method: u32, card: Option<&[u32]>) -> i32 {
    if let Some(card) = card {
        // `SETOUT` counts the fields it accepts, and it accepts line
        // numbers up to 25 — eleven more than `OUTBOD2` has labels for.
        // `OUTBOD` recounts as it prints and has no upper bound at all,
        // so from the second hour on the charge can be larger than this.
        return card
            .iter()
            .take(14)
            .filter(|l| **l > 0 && **l <= 25)
            .count() as i32;
    }
    match rewritten(method) {
        16 => 13,
        17 | 18 => 6,
        19 => 5,
        20..=22 => 21,
        23 => -1,
        25 => 22,
        _ => 1,
    }
}

/// The method number `SETOUT` sees: `DECRED` rewrites card method 30 to
/// 20 before it runs, so both select the same lines.
fn rewritten(method: u32) -> u32 {
    if method == 30 {
        20
    } else {
        method
    }
}

/// One `ANTENNA` card as the listing describes it: the fields `OUTTOP`
/// prints on its header line, plus the pattern `OUTANT` prints in full.
#[derive(Debug, Clone)]
pub struct AntennaLine {
    /// 1 transmit, 2 receive.
    pub iat: i32,
    /// The card's frequency range.
    pub xfqs: R,
    pub xfqe: R,
    /// `ANTMODEL`'s ten-character model label.
    pub anttype: String,
    /// The definition file's own description line.
    pub description: String,
    /// The card's antenna file field.
    pub file: String,
    /// The card's design-frequency field.
    pub design_freq: R,
    /// The card's main beam bearing and the azimuth it ends up cut
    /// along, both as the gain file's `f7.2` left them.
    pub beam_main: R,
    pub offazim: R,
    /// Ground conductivity and dielectric constant, from the gain file.
    pub cond: R,
    pub diel: R,
    /// Power in kilowatts, printed on a transmit card only.
    pub pwrkw: R,
    /// `array(30,91)` and `aeff(30)`: the pattern itself.
    pub gains: Vec<[R; 91]>,
    pub eff: [R; 30],
}

impl AntennaLine {
    /// The antennas of a run, in the order `OUTTOP` walks the slots.
    pub fn from_set(ants: &AntennaSet) -> Vec<Self> {
        ants.ants
            .iter()
            .map(|a| Self {
                iat: a.iat,
                xfqs: a.xfqs,
                xfqe: a.xfqe,
                anttype: a.table.anttype.clone(),
                description: a.table.description.clone(),
                file: a.file.clone(),
                design_freq: a.design_freq,
                beam_main: a.table.beam_main,
                offazim: a.table.offazim,
                cond: a.table.cond,
                diel: a.table.diel,
                pwrkw: a.pwrkw,
                gains: a.table.gains.clone(),
                eff: a.table.eff,
            })
            .collect()
    }
}

/// Everything `OUTTOP` prints.
#[derive(Debug, Clone)]
pub struct Header {
    /// The `COEFFS` card's four characters: `CCIR` or `URSI`.
    pub coeff: String,
    /// The method as printed, which is the card's number: `DECRED`
    /// rewrites card method 30 to 20 and `OUTTOP` turns it back.
    pub method: u32,
    pub model: String,
    pub version: String,
    /// 1-12, and the `MONTH` card's year field as the five characters it
    /// holds.
    pub month: u32,
    pub year: String,
    pub ssn: R,
    /// The `SYSTEM` card: minimum take-off angle, man-made noise at 3
    /// MHz as a positive number, required reliability and required
    /// signal-to-noise ratio, then the two multipath tolerances.
    pub amind: R,
    pub znoise: R,
    pub lufp: i32,
    pub rsn: R,
    pub pmp: R,
    pub dmp: R,
    /// The `LABEL` card's forty characters.
    pub label: String,
    /// The `CIRCUIT` card's long-path field.
    pub long_path: bool,
    pub tx_lat: R,
    pub tx_lon: R,
    pub rx_lat: R,
    pub rx_lon: R,
    /// Bearings each way and the path length in km.
    pub btrd: R,
    pub brtd: R,
    pub gcd_km: R,
    pub antennas: Vec<AntennaLine>,
    pub top: TopLines,
}

impl Header {
    /// The header for one deck case, given what the engine worked out
    /// about the path.
    ///
    /// Everything else comes from the case, so the header describes the
    /// same deck the reference was handed.
    pub fn for_case(c: &DeckCase, path: &PathReport, version: &str) -> Self {
        Self {
            coeff: if c.ursi { "URSI" } else { "CCIR" }.to_string(),
            method: c.method,
            model: MODEL.to_string(),
            version: version.to_string(),
            month: c.month,
            // The `MONTH` card's year field is five columns wide and
            // `DECRED` reads it as text, so the header prints the
            // columns rather than a number.
            year: format!("{:5}", c.year),
            ssn: c.ssn.round() as R,
            amind: MIN_ANGLE_DEG as R,
            znoise: c.noise_dbw as R,
            lufp: REQUIRED_RELIABILITY_PCT,
            rsn: c.required_snr_db as R,
            pmp: MULTIPATH_POWER_DB as R,
            dmp: MULTIPATH_DELAY_MS as R,
            label: c.label(),
            // The `CIRCUIT` card's path field, which the deck writer
            // always sets to the short way round.
            long_path: false,
            tx_lat: c.from_lat as R,
            tx_lon: c.from_lon as R,
            rx_lat: path.rlatd,
            rx_lon: c.to_lon as R,
            btrd: path.btrd,
            brtd: path.brtd,
            gcd_km: path.gcd_km,
            antennas: path.antennas.clone(),
            top: top_lines(c.method, c.toplines.as_deref()),
        }
    }
}

/// `LISTIN`'s banner and column ruler.
///
/// The first character is the form-feed flag, which a point-to-point run
/// leaves blank.
pub fn preamble(version: &str) -> String {
    let mut out = String::new();
    out.push_str("  IONOSPHERIC COMMUNICATIONS ANALYSIS AND PREDICTION PROGRAM\n");
    out.push_str(&format!("{:20}", ""));
    out.push_str(&format!(" VOACAP   VERSION {}\n", text_field(version, 8)));
    out.push_str("\n\n");
    let mut tens = String::from(" ");
    for n in 1..=7 {
        tens.push_str(&format!("{:9}{n}", ""));
    }
    out.push_str(&tens);
    out.push('\n');
    let mut units = String::from(" ");
    for _ in 0..7 {
        units.push_str("1234567890");
    }
    units.push_str("12345");
    out.push_str(&units);
    out.push_str("\n\n");
    out
}

/// The input deck as `LISTIN` echoes it: one leading space, trailing
/// blanks dropped.
pub fn echo_deck(deck: &str) -> String {
    let mut out = String::new();
    for line in deck.lines() {
        out.push(' ');
        out.push_str(line.trim_end());
        out.push('\n');
    }
    out
}

/// `HFMUFS`'s last line.
pub fn end_of_run(version: &str) -> String {
    format!(
        " *****END OF RUN*****     VOACAP {}\n",
        text_field(version, 8)
    )
}

/// One header block, and the antenna lines it printed.
///
/// `first_hour` carries `JTX = 1`: the tilde before `METHOD` marks a
/// header printed while the run was still on its first hour, so only the
/// first page of a 24-hour run has it.
fn header_block(h: &Header, page: usize, first_hour: bool, method: u32) -> (String, i32) {
    let mut out = String::new();
    // The page banner. The record opens with a literal form feed.
    out.push_str(&format!(
        "\u{c}     {} Coefficients        {}METHOD{} {} L {}  PAGE{}\n\n",
        text_field(&h.coeff, 4),
        if first_hour { '~' } else { ' ' },
        i(method as i64, 3),
        text_field(&h.model, 8),
        text_field(&h.version, 8),
        i(page as i64, 4)
    ));
    if h.top.on[0] && h.month >= 1 && h.month <= 12 {
        out.push_str(&format!(
            "  {}   {}{:10}SSN = {}{:16}Minimum Angle={} degrees\n",
            MONTHS[(h.month - 1) as usize],
            text_field(&h.year, 5),
            "",
            f(h.ssn, 4, 0),
            "",
            f(h.amind, 6, 3)
        ));
    }
    if h.top.on[1] {
        // `path` is a six-character variable the short-path case fills
        // with four blanks, so it is blank either way.
        let path = if h.long_path { "<Long>" } else { "      " };
        out.push_str(&format!(
            "  {}  AZIMUTHS  {path}  N. MI.      KM\n",
            text_field(&h.label, 40)
        ));
    }
    if h.top.on[2] {
        let hemi = |v: R, positive: char, negative: char| {
            if v < 0.0 {
                negative
            } else {
                positive
            }
        };
        out.push_str(&format!(
            "{} {}{} {} - {} {}{} {}{}{}{}{}\n",
            f(h.tx_lat.abs(), 7, 2),
            hemi(h.tx_lat, 'N', 'S'),
            f(h.tx_lon.abs(), 8, 2),
            hemi(h.tx_lon, 'E', 'W'),
            f(h.rx_lat.abs(), 5, 2),
            hemi(h.rx_lat, 'N', 'S'),
            f(h.rx_lon.abs(), 8, 2),
            hemi(h.rx_lon, 'E', 'W'),
            f(h.btrd, 10, 2),
            f(h.brtd, 8, 2),
            f(h.gcd_km * 0.54, 10, 1),
            f(h.gcd_km, 9, 1)
        ));
    }
    // Transmit cards carry their power; the receive line stops at the
    // off-azimuth, because `OUTTOP` passes one fewer value and the
    // record ends where the values run out.
    let mut knt = 0i32;
    for (iat, on, tag) in [(1, h.top.on[3], "XMTR"), (2, h.top.on[4], "RCVR")] {
        if !on {
            continue;
        }
        for a in h.antennas.iter().filter(|a| a.iat == iat) {
            out.push_str(&format!(
                "  {tag}{}-{} {}[{}] Az={} OFFaz={}",
                i(nint(a.xfqs) as i64, 3),
                i(nint(a.xfqe) as i64, 2),
                text_field(&a.anttype, 10),
                text_field(&a.file, 21),
                f(a.beam_main, 5, 1),
                f(a.offazim, 5, 1)
            ));
            if iat == 1 {
                out.push_str(&format!("{}kW", f(a.pwrkw, 8, 3)));
            }
            out.push('\n');
            knt += 1;
        }
    }
    if h.top.on[5] {
        out.push_str(&format!(
            "  3 MHz NOISE = {} dBW     REQ. REL = {}%    REQ. SNR ={} dB\n",
            f(-h.znoise, 6, 1),
            i(h.lufp as i64, 2),
            f(h.rsn, 5, 1)
        ));
    }
    if h.top.on[6] {
        out.push_str(&format!(
            "  MULTIPATH POWER TOLERANCE = {} dB   MULTIPATH DELAY TOLERANCE = {} ms\n",
            f(h.pmp, 4, 1),
            f(h.dmp, 6, 3)
        ));
    }
    (out, knt)
}

/// The page state the output routines share through `/OUTPRT/`:
/// `LPAGES`, the page number `OUTTOP` stamps, and `LINES`, the count each
/// routine charges its own output against.
///
/// `SETOUT` leaves `LINES` at the page limit, so whichever routine runs
/// first breaks a page before printing anything.
pub struct Pager<'a> {
    pub header: &'a Header,
    pub page: usize,
    pub lines: i32,
    pub linmax: i32,
    /// `METHOD`: the deck's method, except while an `OUTGRAPH` request
    /// is printing — the driver reassigns the global before it calls
    /// `OUTMUF` or `OUTGPH`. `OUTTOP` prints it, but a card method 30
    /// deck carries `MSPEC = 121` and `OUTTOP` prints a literal 30 for
    /// that, so an `OUTGRAPH` request there never shows in the header.
    pub method: u32,
}

impl<'a> Pager<'a> {
    pub fn new(header: &'a Header, linmax: i32) -> Self {
        Self {
            header,
            page: 0,
            lines: linmax,
            linmax,
            method: header.method,
        }
    }

    /// `CALL OUTTOP`: the header block, and the `LINES = LINTOP(15) + KNT`
    /// it leaves behind.
    ///
    /// `first_hour` is `JTX == 1`. `JTX` is the hour index, so a routine
    /// called after the hour loop — `OUTMUF` and `OUTGPH` — always sees
    /// the last hour's index and never gets the tilde, unless the run
    /// asked for a single hour.
    pub fn outtop(&mut self, first_hour: bool) -> String {
        self.page += 1;
        let meth = if self.header.method == 30 {
            30
        } else {
            self.method
        };
        let (block, knt) = header_block(self.header, self.page, first_hour, meth);
        self.lines = self.header.top.count + knt;
        block
    }
}

/// Fortran `NINT`.
pub(crate) fn nint(v: R) -> i32 {
    if v >= 0.0 {
        (f64::from(v) + 0.5).floor() as i32
    } else {
        -((-f64::from(v) + 0.5).floor() as i32)
    }
}

/// The complete listing for one run.
///
/// `lines` is the body row selection in print order, from
/// [`super::run::body_lines`]; `nbod_card` is [`nbod`] for the method,
/// which is the row count the first page break charges before `OUTBOD`
/// has recounted.
pub fn listing(
    deck: &str,
    h: &Header,
    hours: &[HourPrediction],
    lines: &[usize],
    nbod_card: i32,
    linmax: i32,
    botlines: bool,
) -> String {
    let mut out = preamble(&h.version);
    out.push_str(&echo_deck(deck));
    // `SETOUT` leaves the line count at the page limit, so the first
    // hour always breaks a page.
    let mut pager = Pager::new(h, linmax);
    out.push_str(&body(&mut pager, hours, lines, nbod_card, botlines));
    out.push_str(&end_of_run(&h.version));
    out
}

/// `OUTLIN` and `OUTBOD`: the systems methods' body, with the page breaks
/// `OUTLIN` decides.
pub fn body(
    pager: &mut Pager,
    hours: &[HourPrediction],
    lines: &[usize],
    nbod_card: i32,
    botlines: bool,
) -> String {
    let mut out = String::new();
    let linmax = pager.linmax;
    let mut linadd = nbod_card;
    for (index, hour) in hours.iter().enumerate() {
        if linadd + pager.lines >= linmax {
            // `OUTTOP` charges the page its own line count plus the
            // antenna lines, which is fewer lines than it printed.
            out.push_str(&pager.outtop(index == 0));
        }
        let (block, printed) = super::run::hour_block(hour, lines);
        out.push_str(&block);
        if !printed {
            // An hour with no mode in any slot prints its frequency line
            // and nothing else, and is charged three lines.
            pager.lines += 3;
            continue;
        }
        // A `BOTLINES` card makes `OUTBOD` recount as it prints, so from
        // here on the charge is the card's length rather than the
        // method's.
        if botlines {
            linadd = lines.len() as i32;
        }
        pager.lines += linadd + 2;
    }
    out
}

/// `JTOUT`: which output routine a card method prints through.
///
/// `DECRED` rewrites card method 30 to 20 before it reads the table, so
/// slot 30's value of 11 is never used and method 30 prints through
/// `OUTLIN` like method 20.
pub fn itout(method: u32) -> u32 {
    match rewritten(method) {
        1 => 1,
        2 => 2,
        3 => 3,
        4..=6 => 4,
        7 => 10,
        8..=11 => 4,
        12 => 5,
        13..=15 => 6,
        16..=23 => 7,
        24 => 8,
        25 => 9,
        26 => 3,
        27..=29 => 4,
        _ => 0,
    }
}

/// `JTRUN`: which computation a card method runs.
///
/// Slot 30 reads 8, but `DECRED` rewrites card method 30 to 20 first, so
/// the value that applies is slot 20's 7.
pub fn itrun(method: u32) -> u32 {
    match rewritten(method) {
        1 => 1,
        2 => 2,
        3..=6 => 3,
        7..=11 => 4,
        12 => 5,
        13..=15 => 6,
        16..=25 => 7,
        26..=30 => 8,
        _ => 0,
    }
}

/// The `OUTGRAPH` card: up to twelve further methods' MUF tables or
/// diurnal graphs, printed from the arrays the run has already filled.
///
/// The driver ignores the card unless the run computed MUFs or LUFs, and
/// then ignores every requested method whose own output is not a MUF
/// table or a diurnal graph. A request for the LUF table or a LUF graph
/// is ignored as well unless the run itself computed a LUF.
///
/// `SETOUT` does not run again, so a requested method prints the
/// original method's header lines under its own method number, and the
/// values are whatever the original method left in the arrays — a
/// nomogram run asked for method 10's graph plots the takeoff angle
/// `NOMMUF` never wrote.
///
/// A negative request writes to a second output unit that the driver
/// never opens, so those pages land in a stray `fort.16` rather than in
/// the listing. They still advance the page number, which is why the
/// port counts them without printing them.
fn outgraph(pager: &mut Pager, case: &DeckCase, hours: &[super::run::MufHourOut]) -> String {
    let mut out = String::new();
    let Some(requested) = case.outgraph.as_deref() else {
        return out;
    };
    let from = itrun(case.method);
    if from <= 2 || (5..=6).contains(&from) || from > 8 {
        return out;
    }
    for &want in requested.iter().take(12) {
        if want == case.method as i32 || want == 0 {
            continue;
        }
        let elsewhere = want < 0;
        let method = want.unsigned_abs();
        if method >= 30 {
            continue;
        }
        let (to_out, to_run) = (itout(method), itrun(method));
        // A LUF table or graph needs a run that computed one.
        if to_run == 8 && from != 7 && from != 8 {
            continue;
        }
        pager.method = method;
        let text = match to_out {
            3 => super::tables::outmuf(pager, hours, method),
            4 => super::graphs::outgph(pager, hours, method),
            _ => continue,
        };
        if !elsewhere {
            out.push_str(&text);
        }
    }
    pager.method = case.method;
    out
}

/// The whole printed output of one run, the way `HFMUFS` dispatches it.
///
/// `LISTIN` writes the preamble and echoes the deck, the routine `JTOUT`
/// names for the method writes the body, and `HFMUFS` writes the last
/// line. Methods that print through a routine still being ported return
/// the preamble and the last line, which is what the reference would
/// print if its own routine printed nothing.
pub fn render(itshfbc: &Path, case: &DeckCase, deck: &str) -> Result<String, String> {
    use super::run::{
        muf_view, path_report, run_listing, run_luf, run_muf, run_par, MufHourOut, RunInputs,
    };
    use super::tables::{outall, outlay, outmuf, outpar, outtab};

    let version = read_version(itshfbc)?;
    let inp = RunInputs::from(case);
    let mut out = preamble(&version);
    out.push_str(&echo_deck(deck));

    let path = path_report(itshfbc, &inp)?;
    let header = Header::for_case(case, &path, &version);
    let mut pager = Pager::new(&header, crate::deck::LINES_PER_PAGE);

    // What an `OUTGRAPH` card would print from, filled by whichever
    // branch computed MUFs or LUFs.
    let mut graphs: Vec<MufHourOut> = Vec::new();
    match itout(case.method) {
        1 => {
            let rows = run_par(itshfbc, &inp)?;
            // The rows come back hour by hour, so the count of control
            // points is the number of rows per hour.
            let points = rows.len() / 24;
            out.push_str(&outpar(&mut pager, &rows, points));
        }
        3 => {
            let hours = if case.method == 26 {
                run_luf(itshfbc, &inp)?
            } else {
                run_muf(itshfbc, &inp)?
            };
            out.push_str(&outmuf(&mut pager, &hours, case.method));
            graphs = hours;
        }
        4 => {
            let hours = if (27..=29).contains(&case.method) {
                run_luf(itshfbc, &inp)?
            } else {
                run_muf(itshfbc, &inp)?
            };
            out.push_str(&super::graphs::outgph(&mut pager, &hours, case.method));
            graphs = hours;
        }
        10 => {
            let hours = run_muf(itshfbc, &inp)?;
            out.push_str(&outlay(&mut pager, &hours));
            graphs = hours;
        }
        7 => {
            let prediction = run_listing(itshfbc, &inp)?;
            let rows = super::run::body_lines(case.method, case.botlines.as_deref());
            out.push_str(&body(
                &mut pager,
                &prediction.hours,
                &rows,
                nbod(case.method, case.botlines.as_deref()),
                case.botlines.is_some(),
            ));
            graphs = muf_view(&prediction.hours);
        }
        8 => {
            let prediction = run_listing(itshfbc, &inp)?;
            let freqs: Vec<R> = inp.freqs_mhz.clone();
            out.push_str(&outtab(&mut pager, &prediction.hours, &freqs));
            graphs = muf_view(&prediction.hours);
        }
        2 => {
            let hours = super::run::run_ion(itshfbc, &inp)?;
            out.push_str(&super::graphs::oution(&mut pager, &hours));
        }
        9 => {
            let prediction = run_listing(itshfbc, &inp)?;
            out.push_str(&outall(&mut pager, &prediction.hours)?);
            graphs = muf_view(&prediction.hours);
        }
        // `ITRUN = 6`, card methods 13 to 15: the driver prints the
        // antenna patterns and reads the next card, before `SETOUT` has
        // run, so no header block and no page arithmetic apply.
        6 => {
            let mut page = 0usize;
            out.push_str(&super::graphs::outant(
                &mut page,
                &path.antennas,
                case.method,
                &version,
            ));
        }
        // `ITRUN = 5`, card method 12: the driver leaves the hour loop
        // after the first hour and no output option matches, so the run
        // prints its preamble and nothing else.
        5 => {}
        _ => {}
    }
    out.push_str(&outgraph(&mut pager, case, &graphs));
    out.push_str(&end_of_run(&version));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setout_selects_the_header_lines_each_method_prints() {
        assert_eq!(top_lines(30, None).count, 7);
        // Method 23 without a card runs the page arithmetic on -1.
        assert_eq!(top_lines(23, None).count, -1);
        assert_eq!(nbod(23, None), -1);
        assert_eq!(nbod(23, Some(&[12, 2, 0])), 2);
        // A line past `OUTBOD2`'s labels still counts for `OUTBOD`, but
        // not for `SETOUT`.
        assert_eq!(nbod(30, Some(&[12, 26])), 1);
        assert_eq!(top_lines(20, None).count, 7);
        assert_eq!(top_lines(24, None).count, 6);
        assert_eq!(top_lines(5, None).count, 3);
        assert_eq!(top_lines(27, None).count, 6);
        // Lines 1 to NTOP, and nothing above it.
        let t = top_lines(20, None);
        assert!(t.on[..7].iter().all(|on| *on));
        assert!(!t.on[7]);
    }

    #[test]
    fn a_toplines_card_replaces_the_selection_for_any_method() {
        let t = top_lines(20, Some(&[3, 1, 0, 20, 7]));
        assert_eq!(t.count, 3);
        assert!(t.on[0] && t.on[2] && t.on[6]);
        assert!(!t.on[1]);
        // A repeated line counts twice, because `SETOUT` counts fields
        // rather than the lines it turned on.
        assert_eq!(top_lines(20, Some(&[3, 3])).count, 2);
    }

    #[test]
    fn the_ruler_is_the_width_of_a_card_image() {
        let text = preamble("16.1207W");
        let ruler: Vec<&str> = text.lines().collect();
        assert_eq!(ruler[4].len(), 71);
        assert!(ruler[4].ends_with("        7"));
        assert_eq!(ruler[5].len(), 76);
        assert!(ruler[5].ends_with("12345"));
    }

    #[test]
    fn the_echoed_deck_drops_trailing_blanks() {
        assert_eq!(
            echo_deck("LABEL     a    \nQUIT\n"),
            " LABEL     a\n QUIT\n"
        );
    }
}
