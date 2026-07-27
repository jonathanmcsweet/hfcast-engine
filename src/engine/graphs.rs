//! The line-printer graphs: `SETGPH` picks the curves and `GPHBOD` draws
//! them as characters on a 40-row grid.
//!
//! Nothing here computes anything. The values plotted are the same MUFs,
//! FOTs and LUFs `mufcheck` and `lufcheck` already compare column by
//! column; what this module adds is the plotting.

use super::con::R;
use super::output::{f, Pager};
use super::run::{IonPlot, MufHourOut};
use super::tables::LABEL;

/// Rows in the graph, top to bottom. Row 1 covers 39.5 to 40.5 MHz and
/// each row below it is one megahertz lower.
const ROWS: usize = 40;
/// Hours across it, plus the repeated midnight column on the left.
const COLUMNS: usize = 25;
/// `ONE`: a curve is drawn only where its value is above this. It is
/// -0.99, not zero, so a value of exactly zero is plotted — on the bottom
/// row, which is never printed with a scale label.
const FLOOR: R = -0.99;

/// What `SETGPH` chose: up to three curves with their column labels.
struct Plots {
    /// Values per hour, and the five-character label of each curve.
    curves: Vec<([R; 24], &'static str)>,
}

/// `SETGPH`: which curves a card method plots.
///
/// Every method starts with the circuit MUF and the FOT. A method with a
/// third curve replaces the blank third label; methods 27 and 29 replace
/// one of the first two curves rather than adding one, so they still plot
/// two.
fn setgph(hours: &[MufHourOut], method: u32) -> Plots {
    let column = |pick: fn(&MufHourOut) -> R| -> [R; 24] {
        let mut out = [-1.0 as R; 24];
        for (slot, h) in out.iter_mut().zip(hours) {
            *slot = pick(h);
        }
        out
    };
    let muf = (column(|h| h.allmuf), LABEL[9]);
    let fot = (column(|h| h.fot), LABEL[3]);
    let luf = (column(|h| h.xluf), LABEL[7]);
    match method {
        4 | 8 => Plots {
            curves: vec![muf, fot],
        },
        5 | 9 => Plots {
            curves: vec![muf, fot, (column(|h| h.hpf), LABEL[6])],
        },
        6 | 11 => Plots {
            curves: vec![muf, fot, (column(|h| h.esmuf), LABEL[2])],
        },
        10 => Plots {
            curves: vec![muf, fot, (column(|h| h.angmuf), LABEL[0])],
        },
        27 => Plots {
            curves: vec![luf, fot],
        },
        28 => Plots {
            curves: vec![muf, fot, luf],
        },
        29 => Plots {
            curves: vec![muf, luf],
        },
        // A method with no graph of its own draws nothing at all.
        _ => Plots { curves: Vec::new() },
    }
}

/// `OUTGPH`: the header, then `GPHBOD`'s graph.
///
/// `OUTGPH` runs after the hour loop, so `JTX` holds the last hour's index
/// and the header carries no tilde unless the run asked for one hour.
pub fn outgph(pager: &mut Pager, hours: &[MufHourOut], method: u32) -> String {
    pager.lines = 0;
    let mut out = pager.outtop(hours.len() == 1);
    out.push_str(&gphbod(&setgph(hours, method)));
    out
}

/// The symbol each curve is drawn with, in curve order.
const SYMBOLS: [char; 3] = ['.', 'X', '+'];
/// The key printed beside each curve's label.
const KEYS: [&str; 3] = ["(....)", "(XXXX)", "(++++)"];

/// `GPHBOD`: the graph itself.
///
/// A method with no curves prints nothing — not even the scales.
///
/// The 25 columns are the 24 hours plus a repeat of hour 24 at the left
/// edge, which is why both ends of the hour scale read `00`. Later curves
/// are drawn over earlier ones in the same cell, so where two curves meet
/// only the last one is visible.
fn gphbod(plots: &Plots) -> String {
    if plots.curves.is_empty() {
        return String::new();
    }
    let mut out = String::new();
    // The key line, with a blank label and key for each curve the method
    // does not plot. `A` fields write their blanks, so the line ends in
    // them.
    out.push_str(" \n\n");
    out.push_str(&format!("{:10}", ""));
    for (slot, key) in KEYS.iter().enumerate() {
        let (label, key) = match plots.curves.get(slot) {
            Some((_, label)) => (*label, *key),
            None => ("     ", "      "),
        };
        out.push_str(label);
        out.push_str(key);
        // Five spaces separate the groups; the set after the last group
        // comes from an `X`, which writes nothing at the end of a record.
        if slot < 2 {
            out.push_str("     ");
        }
    }
    out.push_str("\n\n");
    out.push_str(&hour_scale());
    out.push_str(&mhz_scale());

    // The value column starts at row 7 and takes one hour per row.
    let mut hour = 0usize;
    for row in 1..=ROWS {
        let top = 41.5 - row as R;
        let bottom = top - 1.0;
        let mut cells = [' '; COLUMNS];
        for (slot, (values, _)) in plots.curves.iter().enumerate() {
            for (index, v) in values.iter().enumerate() {
                if *v <= FLOOR || *v > top || *v <= bottom {
                    continue;
                }
                cells[index + 1] = SYMBOLS[slot];
                // Hour 24 is drawn twice: once at its own column and once
                // at the left edge, so the day closes on itself.
                if index + 1 == 24 {
                    cells[0] = SYMBOLS[slot];
                }
            }
        }
        // The scale label prints on odd rows only.
        let scale = if row % 2 == 1 {
            format!("{:02}", 41 - row)
        } else {
            "  ".to_string()
        };
        let mut line = format!("    {scale}-");
        for cell in cells {
            line.push(' ');
            line.push(cell);
        }
        line.push_str(&format!(" -{scale}"));
        // Row 5 carries the column headings, rows 7 to 30 the values, and
        // the rest nothing.
        if row == 5 {
            line.push_str("    GMT");
            for slot in 0..3 {
                match plots.curves.get(slot) {
                    Some((_, label)) => line.push_str(&format!(" {label}")),
                    None => line.push_str("      "),
                }
            }
        } else if (7..=30).contains(&row) && hour < 24 {
            line.push_str(&values_column(plots, hour));
            hour += 1;
        }
        out.push_str(&line);
        out.push('\n');
    }
    out.push_str(&mhz_scale());
    out.push_str(&hour_scale());
    out.push_str(&format!("{:21}UNIVERSAL TIME\n", ""));
    out
}

/// One hour's values beside the graph: the hour, then each curve's value
/// or a dash where the curve has none.
///
/// A dash is written as three spaces and the dash, with two more skipped;
/// the skip writes nothing when it ends the record, so a row ending in a
/// dash has no trailing blanks.
fn values_column(plots: &Plots, hour: usize) -> String {
    let mut out = format!("   {}", f(hour as R + 1.0, 4, 1));
    for (values, _) in &plots.curves {
        if values[hour] <= FLOOR {
            out.push_str("   -  ");
        } else {
            out.push_str(&f(values[hour], 6, 1));
        }
    }
    while out.ends_with(' ') {
        out.pop();
    }
    out
}

fn hour_scale() -> String {
    let mut out = format!("{:7}", "");
    for hour in (0..=22).step_by(2) {
        out.push_str(&format!("{hour:02}  "));
    }
    out.push_str("00\n");
    out
}

fn mhz_scale() -> String {
    let mut out = format!("{:5}MHZ", "");
    for _ in 0..24 {
        out.push_str("+-");
    }
    out.push_str("+MHZ\n");
    out
}

/// The vertical caption `OUTANT` writes one character per row, down the
/// left edge of the pattern. It is a `CHARACTER*46` holding ten leading
/// blanks, so the caption starts at row 11 of 46.
const ELEVATION_CAPTION: &str = "          ELEVATION ANGLE IN DEGREES";

/// `OUTANT`: the antenna patterns of methods 13, 14 and 15.
///
/// One page per `ANTENNA` card — transmit cards for method 13, receive
/// cards for 14, both for 15. The driver calls this before `SETOUT`, so
/// the run has no header block and no line count; the page banner here is
/// `OUTANT`'s own, and it starts with a form feed the way `OUTTOP`'s does.
///
/// A card spanning more than 20 MHz prints 21 columns: every whole
/// megahertz from 2 to 14, then every second one from 16 to 30. A
/// narrower card prints one column per megahertz of its range.
pub fn outant(
    page: &mut usize,
    antennas: &[super::output::AntennaLine],
    method: u32,
    version: &str,
) -> String {
    use super::output::{f, i, text_field};

    let ends: &[i32] = match method {
        13 => &[1],
        14 => &[2],
        _ => &[1, 2],
    };
    let mut out = String::new();
    for end in ends {
        for a in antennas.iter().filter(|a| a.iat == *end) {
            *page += 1;
            out.push_str(&format!(
                "\u{c}{:32}METHOD{} {} {}  PAGE{}\n\n",
                "",
                i(method as i64, 3),
                text_field(super::output::MODEL, 8),
                text_field(version, 8),
                i(*page as i64, 4)
            ));
            out.push_str(&format!(
                " {} ANTENNA PACKAGE{:26}ANTENNA PATTERN{:10}{}\n",
                text_field(&a.anttype, 10),
                "",
                "",
                if *end == 1 {
                    "TRANSMITTER"
                } else {
                    "RECEIVER   "
                }
            ));
            out.push_str(&format!(
                " [{}] {}\n",
                text_field(&a.file, 21),
                text_field(&a.description, 70)
            ));
            out.push_str(" Frequency Range  Design Freq  Bearing   Off Azim  Conduct.  Dielect.\n");
            out.push_str(&format!(
                " {} to {}  {}{}{}{}{}\n",
                f(a.xfqs, 5, 1),
                f(a.xfqe, 5, 1),
                f(a.design_freq, 10, 3),
                f(a.beam_main, 10, 1),
                f(a.offazim, 10, 1),
                f(a.cond, 10, 3),
                f(a.diel, 10, 3)
            ));
            let columns = pattern_columns(a.xfqs, a.xfqe);
            let ruler = {
                let mut r = format!("{:4}", "");
                for c in &columns {
                    r.push_str(&format!("{:4}{}", "", i(*c as i64, 2)));
                }
                r.push('\n');
                r
            };
            out.push_str(&ruler);
            for row in 0..46 {
                // Elevation 90 down to 0 in steps of two, and one
                // character of the caption per row.
                let elev = 90 - row * 2;
                let caption = ELEVATION_CAPTION.chars().nth(row).unwrap_or(' ');
                out.push_str(&format!(" {caption} {}", i(elev as i64, 2)));
                for c in &columns {
                    out.push_str(&f(a.gains[*c - 1][elev], 6, 1));
                }
                out.push('\n');
            }
            out.push_str(&ruler);
            out.push_str(&format!("\n\n{:48}FREQUENCY IN MEGAHERTZ\n", ""));
            out.push_str(&format!("\n\n{:48}ANTENNA EFFICIENCY\n", ""));
            out.push_str(&format!("{:5}", ""));
            for c in &columns {
                out.push_str(&f(a.eff[*c - 1], 6, 1));
            }
            out.push('\n');
            out.push_str(&ruler);
            out.push_str(&format!("\n\n{:48}FREQUENCY IN MEGAHERTZ\n", ""));
        }
    }
    out
}

/// Which frequency columns the pattern prints, in whole megahertz.
fn pattern_columns(xfqs: R, xfqe: R) -> Vec<usize> {
    let (first, last) = (xfqs as i32, xfqe as i32);
    if last - first <= 20 {
        (first..=last).map(|f| f as usize).collect()
    } else {
        let mut out: Vec<usize> = (2..=14).collect();
        out.extend((16..=30).step_by(2));
        out
    }
}

// ---------------------------------------------------------------------
// IONPLT: the ionograms

/// `IONPLT`'s row labels down the left edge, every tenth row.
const HEIGHT_LABELS: [&str; 6] = ["600", "500", "400", "300", "200", "100"];
/// `ILY`: the layer names of the first four rows.
const LAYER_NAMES: [&str; 4] = [" E=", "F1=", "F2=", "ES="];
/// `IS`: the characters the three sporadic-E segments are drawn with.
///
/// The array `FS` holds the lower decile, the median and the upper
/// decile in that order, so the segment nearest zero frequency is the
/// lower decile — but it is drawn with `U`, and the far segment with
/// `L`. The labels are the wrong way round; the port draws what the
/// reference draws.
const ES_MARKS: [u8; 3] = *b"UML";
/// `INTG`: how the absorption integral was taken.
const INTEGRATION: [&str; 2] = ["GAUSSIAN  ", "MODEL SEG "];
/// `IFONE`: what the F1 layer is doing.
const F1_STATE: [&str; 3] = ["GONE      ", "PARABOLIC ", "LINEAR    "];
/// `IEDP` with no `INTEGRATE` card, which is what `blkdat.for` sets.
/// Only a card can raise it, and raising it would pick `MODEL SEG` for
/// a point with no F1 layer.
const IEDP: i32 = -1;
/// Columns across the plot, one per `FINC` of sounding frequency.
const ION_COLUMNS: usize = 100;
/// Rows down it, 10 km of virtual height each from 605 km.
const ION_ROWS: usize = 51;

/// `OUTION`: card method 2's ionograms, one page per sample area per
/// hour.
///
/// The routine takes no notice of the line count — it calls `OUTTOP`
/// for every plot, so every plot starts a page whatever `LINEMAX` says.
pub fn oution(pager: &mut Pager, hours: &[Vec<IonPlot>]) -> String {
    let mut out = String::new();
    for (jtx, hour) in hours.iter().enumerate() {
        for plot in hour {
            out.push_str(&pager.outtop(jtx == 0));
            out.push_str(&ionplt(plot));
        }
    }
    out
}

/// One ionogram: virtual and true reflection height against sounding
/// frequency, drawn with `X` for the virtual height and `.` for the
/// true height.
///
/// The heading prints the point's latitude with a fixed `N` and its
/// longitude with a fixed `W`, whatever hemisphere the point is in.
/// The two distances beside them are read from `RD(2K-1)` and, below
/// three sample areas, `RD(KFX)` — not from the slot the latitude and
/// longitude come from.
fn ionplt(p: &IonPlot) -> String {
    // The frequency scale doubles once the F2 critical frequency passes
    // 10 MHz, which is what keeps the plot 100 columns wide.
    let (finc, scale): (R, [i32; 10]) = if p.fi[2] <= 10.0 {
        (0.1, [1, 2, 3, 4, 5, 6, 7, 8, 9, 10])
    } else {
        (0.2, [2, 4, 6, 8, 10, 12, 14, 16, 18, 20])
    };
    let mut index = 0usize;
    let ine;
    if p.fi[1] <= 0.0 {
        ine = 0;
        if IEDP >= 0 {
            index = 1;
        }
    } else if p.fsecv <= 0.0 {
        ine = 1;
    } else {
        ine = 2;
    }
    let mut out = format!(
        "   GMT = {}   LMT = {}   LAT = {} N  {} W  DIST = {} {}  KM  {} HP   F1 IS {}\n\n",
        f(p.gmt, 5, 1),
        f(p.lmt, 5, 1),
        f(p.lat, 6, 2),
        f(p.lon, 7, 2),
        f(p.rdx, 5, 0),
        f(p.rdy, 5, 0),
        INTEGRATION[index],
        F1_STATE[ine]
    );
    out.push_str(
        "                     VIRTUAL HEIGHT - REFLECTION HEIGHT VS. SOUNDING FREQUENCY -MHZ-\n",
    );
    out.push_str(&frequency_scale(&scale, true));
    out.push_str(&ruler());
    // `IHLS`: the row the sporadic-E layer sits on.
    let ihls = ((605.0 - p.hs) * 0.1 + 1.0) as i32;
    let mut label_index = 0usize;
    let mut next_label = 1i32;
    let mut zob2: R = 605.0;
    for idn in 1..=ION_ROWS as i32 {
        let mut ix = [b' '; ION_COLUMNS];
        let zob1 = zob2;
        zob2 = zob1 - 10.0;
        let (edge, left) = if idn < next_label {
            (b'-', "   ")
        } else {
            let label = HEIGHT_LABELS[label_index];
            label_index += 1;
            next_label += 10;
            (b'+', label)
        };
        if idn == ihls {
            let mut isx = 0i32;
            for (iz, mark) in ES_MARKS.iter().enumerate() {
                let isb = isx + 1;
                isx = ((p.fs[iz] / finc + 0.5) as i32).min(ION_COLUMNS as i32);
                if isx < isb {
                    continue;
                }
                for slot in isb..=isx {
                    ix[slot as usize - 1] = *mark;
                }
            }
        } else {
            for ih in 0..30 {
                let icr = ((p.ion.fvert[ih] / finc + 1.0) as i32)
                    .min(ION_COLUMNS as i32)
                    .max(1) as usize;
                if p.ion.hprim[ih] <= zob1 && p.ion.hprim[ih] > zob2 {
                    ix[icr - 1] = b'X';
                }
                // The true height is drawn after the virtual one, so it
                // wins where both land in the same cell.
                if p.ion.htrue[ih] <= zob1 && p.ion.htrue[ih] > zob2 {
                    ix[icr - 1] = b'.';
                }
            }
        }
        let edge = edge as char;
        let numbers = |i: usize| {
            format!(
                "{}{}{}{}",
                super::output::i(i as i64 + 1, 3),
                f(p.ion.fvert[i], 7, 2),
                f(p.ion.htrue[i], 7, 2),
                f(p.ion.hprim[i], 7, 2)
            )
        };
        let cells = |from: usize| String::from_utf8_lossy(&ix[from..]).into_owned();
        match idn {
            // The first four rows carry the layer parameters, written
            // over the left of the plot.
            1..=3 => {
                let k = idn as usize - 1;
                out.push_str(&format!(
                    "  {left}{edge} {}{}{}{} {}{edge}{}\n",
                    LAYER_NAMES[k],
                    f(p.fi[k], 5, 2),
                    f(p.yi[k], 6, 1),
                    f(p.hi[k], 6, 1),
                    cells(22),
                    numbers(k)
                ));
            }
            4 => {
                out.push_str(&format!(
                    "  {left}{edge} {}{} {} {} {} {}{edge}{}\n",
                    LAYER_NAMES[3],
                    f(p.fs[0], 5, 2),
                    f(p.fs[1], 5, 2),
                    f(p.fs[2], 5, 2),
                    f(p.hs, 5, 1),
                    cells(28),
                    numbers(3)
                ));
            }
            5..=30 => {
                out.push_str(&format!(
                    "  {left}{edge}{}{edge}{}\n",
                    cells(0),
                    numbers(idn as usize - 1)
                ));
            }
            _ => out.push_str(&format!("  {left}{edge}{}{edge}\n", cells(0))),
        }
    }
    out.push_str(&ruler());
    out.push_str(&frequency_scale(&scale, false));
    out
}

/// `502`: the tick rule under and over the plot.
fn ruler() -> String {
    format!("     {}\n", "+----".repeat(20))
}

/// `500` and `501`: the frequency scale. The upper one carries the
/// column names of the three numbers printed down the right edge; the
/// lower one stops at its last label, because the eight blanks after it
/// come from an `X`, which positions rather than writes.
fn frequency_scale(scale: &[i32; 10], with_names: bool) -> String {
    let mut out = String::from("              ");
    for (n, value) in scale.iter().enumerate() {
        if n > 0 {
            out.push_str("        ");
        }
        out.push_str(&super::output::i(*value as i64, 2));
    }
    if with_names {
        out.push_str("      FVERT  HTRUE  HPRIM");
    }
    out.push('\n');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_wide_card_prints_whole_megahertz_then_every_second_one() {
        assert_eq!(pattern_columns(2.0, 30.0).len(), 21);
        assert_eq!(pattern_columns(2.0, 30.0)[13], 16);
        assert_eq!(
            pattern_columns(7.0, 15.0),
            vec![7, 8, 9, 10, 11, 12, 13, 14, 15]
        );
    }

    #[test]
    fn the_scales_are_the_width_of_the_graph() {
        // 25 columns of two characters each, between the two labels.
        assert_eq!(mhz_scale().trim_end().len(), 60);
        assert!(hour_scale().starts_with("       00  02"));
        assert!(hour_scale().trim_end().ends_with("22  00"));
    }

    #[test]
    fn setgph_leaves_a_method_with_no_graph_empty() {
        let hours: Vec<MufHourOut> = Vec::new();
        assert!(setgph(&hours, 3).curves.is_empty());
        assert!(gphbod(&setgph(&hours, 3)).is_empty());
        assert_eq!(setgph(&hours, 27).curves.len(), 2);
        assert_eq!(setgph(&hours, 28).curves.len(), 3);
        // Method 29 plots the LUF in place of the FOT, so its second
        // label is the LUF and it still plots two curves.
        let plots = setgph(&hours, 29);
        assert_eq!(plots.curves.len(), 2);
        assert_eq!(plots.curves[1].1, "  LUF");
    }
}
