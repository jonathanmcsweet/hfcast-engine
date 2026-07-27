//! The printed tables no systems method uses: `OUTMUF`, `OUTLAY`,
//! `OUTPAR` and `OUTTAB`/`TABBOD`.
//!
//! Nothing here computes anything. Every number is already checked
//! against the reference cell by cell — `mufcheck` compares `OUTMUF`,
//! `OUTLAY` and `OUTPAR` and `lufcheck` compares the LUF column — so
//! what this module adds is the text around the numbers, which is what
//! lets a whole run of these methods be compared as a file.

use super::con::R;
use super::modes::AllModesOut;
use super::output::{f, i, nint, Pager};
use super::run::{laytyp, HourPrediction, MufHourOut, ParRow};

/// `/OUTLAB/`'s `LABEL`, the five-character column names the MUF tables
/// and the graphs share. Slot 11 is blank.
pub const LABEL: [&str; 11] = [
    "  ANG", " EMUF", "ESMUF", "  FOT", "F1MUF", "F2MUF", "  HPF", "  LUF", " MODE", "  MUF",
    "     ",
];

/// `OUTMUF`: the FOT, HPF, sporadic-E MUF and circuit MUF of every hour,
/// plus the LUF for method 26.
///
/// The routine builds its format at run time from a vector of six-
/// character pieces, and its comment says it prints a dash where a MUF
/// is one or less. It cannot: the four columns are set to print a dash
/// and then immediately reset to print a value, every hour, so the dash
/// pieces are dead. The only piece the method chooses is the LUF column,
/// and when the method is not 26 that column is removed rather than
/// dashed.
///
/// `OUTMUF` runs after the hour loop, so `JTX` is the last hour's index
/// and the header carries no tilde unless the run asked for one hour.
pub fn outmuf(pager: &mut Pager, hours: &[MufHourOut], method: u32) -> String {
    let luf = method == 26;
    pager.lines = 0;
    let mut out = pager.outtop(hours.len() == 1);
    // Two blank records, the heading, then one more blank record.
    out.push_str("\n\n   ");
    out.push_str("   GMT");
    out.push_str("   LMT");
    for name in [LABEL[3], LABEL[6], LABEL[2], LABEL[9]] {
        out.push_str("  ");
        out.push_str(name);
    }
    if luf {
        out.push_str("  ");
        out.push_str(LABEL[7]);
    }
    out.push_str("\n\n");
    for h in hours {
        out.push_str("   ");
        out.push_str(&f(h.gmt, 6, 1));
        out.push_str(&f(h.lmt, 6, 1));
        out.push_str(&f(h.fot, 7, 2));
        out.push_str(&f(h.hpf, 7, 2));
        out.push_str(&f(h.esmuf, 7, 2));
        out.push_str(&f(h.allmuf, 7, 2));
        if luf {
            out.push_str(&f(h.xluf, 7, 2));
        }
        out.push('\n');
    }
    out
}

/// `OUTLAY`: method 7's per-layer table, two lines an hour.
///
/// The heading names the layer pairs `ELAYER/F2LAYER` and
/// `F1LAYER(E)/ESLAYER`, but the columns hold the layers in the order
/// `CURMUF` fills them — E, F1 on the first line and F2, Es on the
/// second — so the left heading covers E and F2 and the right one covers
/// F1 and Es. The port prints the reference's heading over the
/// reference's columns.
///
/// The header block is charged four lines where it prints five, so the
/// page arithmetic runs a line behind. It cannot show: 24 hours at two
/// lines each never reach the page limit from a first page of nine.
pub fn outlay(pager: &mut Pager, hours: &[MufHourOut]) -> String {
    let mut out = String::new();
    for (index, h) in hours.iter().enumerate() {
        if pager.lines >= pager.linmax {
            out.push_str(&pager.outtop(index == 0));
            out.push('\n');
            out.push_str(&format!(
                "{:26}ELAYER/F2LAYER{:28}F1LAYER(E)/ESLAYER\n\n",
                "", ""
            ));
            // The two spaces that separate the layer groups come from a
            // `2X`, which moves the write position rather than writing
            // anything, so the pair at the end of a record leaves no
            // trailing blanks.
            out.push_str("  GMT   LMT");
            out.push_str("   FOT   MUF   HPF ANGLE VIRTL  TRUE FVERT");
            out.push_str("     FOT   MUF   HPF ANGLE VIRTL  TRUE FVERT");
            out.push_str("\n\n");
            pager.lines += 4;
        }
        let group = |l: &super::muf::LayerMuf| {
            format!(
                "{}{}{}{}{}{}{}",
                f(l.yfot, 6, 1),
                f(l.ymuf, 6, 1),
                f(l.yhpf, 6, 1),
                f(l.delmuf, 6, 1),
                f(l.hpmuf, 6, 0),
                f(l.htmuf, 6, 0),
                f(l.fvmuf, 6, 1)
            )
        };
        out.push_str(&format!(
            " {}{}{}  {}\n",
            f(h.gmt, 4, 1),
            f(h.lmt, 6, 1),
            group(&h.layers[0]),
            group(&h.layers[1])
        ));
        out.push_str(&format!(
            "{:11}{}  {}\n",
            "",
            group(&h.layers[2]),
            group(&h.layers[3])
        ));
        pager.lines += 2;
    }
    out
}

/// `OUTPAR`: method 1's ionospheric parameters, one line per control
/// point per hour.
///
/// The page test runs after the line count is incremented and compares
/// `.GT.`, so the first row of the run breaks a page. The heading over
/// the columns prints the E semithickness and height and the
/// sporadic-E height of control point 1 for whichever hour broke the
/// page.
///
/// `OUTTOP` here sets `LINES` to its own count alone, and the four
/// records the heading adds are not counted, so the second page arrives
/// four rows later than the limit would suggest.
///
/// `first_hour_rows` is how many of the leading rows belong to the
/// run's first hour, which is what decides the header's tilde: `JTX` is
/// the hour index, so every row of hour 1 carries it and no later row
/// does. Card method 1 passes all its hours at once and so passes its
/// control point count; card method 25 passes one hour at a time and so
/// passes either the whole row count or none.
pub fn outpar(pager: &mut Pager, rows: &[ParRow], first_hour_rows: usize) -> String {
    let mut out = String::new();
    for (index, p) in rows.iter().enumerate() {
        pager.lines += 1;
        if pager.lines > pager.linmax {
            out.push_str(&pager.outtop(index < first_hour_rows));
            out.push_str(" \n");
            out.push_str(&format!(
                "{:33}YE = {}     HE = {}     HS = {}\n\n",
                "",
                f(p.ye, 5, 1),
                f(p.he, 5, 1),
                f(p.hs, 5, 1)
            ));
            out.push_str(
                "   LAT   LONG    LMT    UT    E     F1    Y1     H1  FH/2   F2Z    Y2     H2   ES  MED   HI M3000   HPF2   RAT    ZEN  ZMAX  MAGL\n",
            );
            pager.lines = pager.header.top.count;
        }
        let hemi = |v: R, positive: char, negative: char| {
            if v < 0.0 {
                negative
            } else {
                positive
            }
        };
        out.push_str(&format!(
            " {}{} {}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}{}\n",
            f(p.lat.abs(), 5, 1),
            hemi(p.lat, 'N', 'S'),
            f(p.lon.abs(), 5, 1),
            hemi(p.lon, 'E', 'W'),
            f(p.lmt, 6, 1),
            f(p.gmt, 6, 1),
            f(p.fe, 6, 2),
            f(p.f1, 6, 1),
            f(p.y1, 6, 1),
            f(p.h1, 7, 1),
            f(p.fh2, 6, 1),
            f(p.f2z, 6, 1),
            f(p.y2, 6, 1),
            f(p.h2, 7, 1),
            f(p.es, 5, 1),
            f(p.med, 5, 1),
            f(p.esu, 5, 1),
            f(p.m3000, 6, 2),
            f(p.hpf2, 7, 1),
            f(p.rat, 6, 1),
            f(p.zen, 7, 1),
            f(p.zmax, 6, 1),
            f(p.magl.abs(), 6, 1),
            hemi(p.magl, 'N', 'S')
        ));
    }
    out
}

/// `OUTTAB` and `TABBOD`: method 24's reliability against frequency.
///
/// `OUTTAB` copies the hour's reliabilities into the plot slots, breaks a
/// page when the count has reached the limit, and then sets the count to
/// a literal 9 — the source calls it a fake for `TABBOD`, which reads
/// that 9 to decide whether it has just been given a fresh page and owes
/// the frequency heading.
///
/// `TABBOD` skips an hour whose circuit MUF is one or less, and it skips
/// it before printing anything, so such an hour leaves no line at all.
/// Its format is built the same way `OUTMUF`'s is, and here the dash
/// pieces are live: a frequency slot whose reliability is not positive
/// prints three spaces and a dash.
pub fn outtab(pager: &mut Pager, hours: &[HourPrediction], freqs: &[R]) -> String {
    let mut out = String::new();
    for (index, h) in hours.iter().enumerate() {
        // `OUTTAB` copies the reliabilities before it tests the page, so
        // the page can break on an hour `TABBOD` then declines to print.
        if pager.lines >= pager.linmax {
            out.push_str(&pager.outtop(index == 0));
            pager.lines = 9;
        }
        if h.allmuf <= 1.0 {
            continue;
        }
        if pager.lines <= TABBOD_FRESH_PAGE {
            out.push_str("\n                           FREQUENCY / RELIABILITY\n\n");
            // The heading: GMT, LMT and MUF as five-character labels,
            // then one field per frequency the card gave, then MUF again
            // for the at-the-MUF column. A slot the card left empty
            // prints the same dash a reliability of zero would.
            out.push_str("  GMT  LMT  MUF  ");
            for slot in 0..11 {
                match freqs.get(slot) {
                    Some(v) if *v > 0.0 => out.push_str(&f(*v, 5, 1)),
                    _ => out.push_str("   - "),
                }
            }
            out.push_str("  MUF\n\n");
            pager.lines += 1;
        }
        out.push_str(&format!(
            "{}{}{}  ",
            f(h.gmt, 5, 1),
            f(h.lmt, 5, 1),
            f(h.allmuf, 5, 1)
        ));
        for slot in 0..11 {
            let v = h.son[slot].reliab;
            if v > 0.0 {
                out.push_str(&f(v, 5, 2));
            } else {
                out.push_str("   - ");
            }
        }
        out.push_str(&f(h.son[11].reliab, 6, 2));
        out.push('\n');
        pager.lines += 1;
    }
    out
}

/// `LINES` after `OUTTAB` has faked it: the value `TABBOD` reads as
/// "this page is fresh".
pub const TABBOD_FRESH_PAGE: i32 = 9;

/// `OUTALL`: method 25's all-modes tables, one block per frequency.
///
/// `LUFFY` calls it from inside the frequency loop, so a block prints
/// while the hour is still running and the header carries the tilde for
/// every hour, not only the first. The first frequency of an hour also
/// prints the ionospheric parameters, because `OUTALL` calls `OUTPAR`
/// when its argument is 1 — which means a deck whose first frequency
/// slot is empty gets no parameter table at all, the loop having
/// skipped that slot before reaching the call.
///
/// The page count is charged 30 lines a block, then reset after a break
/// to `LINTOP(10) + 30`. `LINTOP(10)` is a line flag, not the line
/// count — the count is `LINTOP(15)` — and for method 25 it is -1, so
/// the reset lands on 29 and every block after the first breaks a page.
///
/// A frequency with no modes at all cannot be printed: the block's
/// formats are built at run time with the mode count as a repeat count,
/// and a repeat count of zero is not a legal format, so the reference
/// stops with a Fortran runtime error part way through the file. The
/// port refuses the same run rather than invent an output for it.
pub fn outall(pager: &mut Pager, hours: &[HourPrediction]) -> Result<String, String> {
    let mut out = String::new();
    for (jtx, h) in hours.iter().enumerate() {
        let first_hour = jtx == 0;
        for (ifx, rec) in h.allmodes.iter().enumerate() {
            let Some(rec) = rec else { continue };
            if ifx == 0 {
                pager.lines = pager.linmax;
                let leading = if first_hour { h.par.len() } else { 0 };
                out.push_str(&outpar(pager, &h.par, leading));
            }
            pager.lines += OUTALL_LINES;
            if pager.lines > pager.linmax {
                out.push_str(&pager.outtop(first_hour));
                pager.lines = OUTALL_TOP_LINE + OUTALL_LINES;
            }
            out.push_str(&block(rec, h.gmt)?);
        }
    }
    Ok(out)
}

/// `NADD`: the lines `OUTALL` charges a block against the page.
const OUTALL_LINES: i32 = 30;

/// `LINTOP(10)` for method 25 — a line flag `OUTALL` reads where it
/// means the line count `LINTOP(15)`.
const OUTALL_TOP_LINE: i32 = -1;

/// The `/allMODE/` rows, in the order `OUTALL` writes them: the label,
/// the array it prints across the modes, and whether a most-reliable
/// column follows.
fn block(rec: &AllModesOut, gmt: R) -> Result<String, String> {
    let a = &rec.all;
    let ist = a.nmmod;
    if ist == 0 {
        return Err(format!(
            "method 25 cannot print {:.1} MHz at {:.0} UT: no modes, \
             and the reference's run-time format needs at least one",
            rec.freq, gmt
        ));
    }
    let nrel = a.nrel.saturating_sub(1);
    let rows: [(&str, &[R; 20], Option<R>); 17] = [
        (" TIME DEL.", &a.timed, Some(rec.son.delay)),
        (" ANGLE    ", &a.b, Some(rec.son.angle)),
        (" VIR. HITE", &a.hp, Some(rec.son.vhigh)),
        (" TRAN.LOSS", &a.tloss, Some(rec.son.dblos)),
        (" T. GAIN  ", &a.tgain, Some(a.tgain[nrel])),
        (" R. GAIN  ", &a.rgain, Some(a.rgain[nrel])),
        (" ABSORB   ", &a.abps, None),
        (" FS. LOSS ", &a.fslos, None),
        (" FIELD ST.", &a.fldst, Some(rec.son.dbu)),
        (" SIG. POW.", &a.sigpow, Some(rec.son.dbw)),
        (" SNR      ", &a.sn, Some(rec.son.sndb)),
        (" MODE PROB", &a.prob, Some(rec.son.cprob)),
        (" R. PWRG  ", &a.crel, Some(rec.son.snpr)),
        (" RELIABIL ", &a.rely, Some(rec.son.reliab)),
        (" SERV PROB", &a.spro, Some(rec.son.sprob)),
        (" SIG LOW  ", &a.tllow, Some(rec.son.dblosl)),
        (" SIG  UP  ", &a.tlhgh, Some(rec.son.dblosu)),
    ];
    // Two blank records, then the summary line.
    let mut out = format!(
        " \n \n SUMMARY  {} MODES   FREQ = {} MHZ  UT = {}\n",
        i(ist as i64, 3),
        f(rec.freq, 5, 1),
        f(gmt, 5, 1)
    );
    // The heading over the most-reliable column is placed by an `X`
    // count worked out from the mode count, so it sits at the end of
    // the mode row whatever that row's width.
    out.push_str(&format!("{:width$} Most REL\n", "", width = 10 * ist + 13));
    out.push_str(&format!("{:12}", ""));
    for im in 0..ist {
        out.push_str(&format!("   {}{} ", f(a.hn[im], 4, 0), laytyp(a.nmode[im])));
    }
    out.push_str(&format!(
        "    {}{}\n",
        f(rec.son.nhp as R, 4, 0),
        laytyp(rec.son.mode_layer)
    ));
    for (name, values, last) in rows {
        out.push_str(&format!(" {name}"));
        for value in values.iter().take(ist) {
            out.push_str(&format!(" {}", f(*value, 9, 2)));
        }
        // A row with no most-reliable value ends where its last mode
        // does: the two spaces before that column come from an `X`,
        // which positions rather than writes.
        if let Some(v) = last {
            out.push_str(&format!("  {}", f(v, 9, 2)));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "  NOISE = {}      S. POWER = {}\n",
        i(nint(rec.son.xnynois + rec.son.rneff) as i64, 6),
        f(rec.son.dbw, 6, 1)
    ));
    out.push_str(&format!(
        "  SIGNAL ={}{}{}  /{}{}{}\n",
        f(rec.dsl, 7, 1),
        f(rec.asm, 7, 1),
        f(rec.dsu, 7, 1),
        f(rec.sls, 8, 1),
        f(rec.ads, 8, 1),
        f(rec.sus, 8, 1)
    ));
    out.push_str(&format!(
        "  NOISE = {}{}{}  /{}{}{}\n",
        f(rec.du, 7, 1),
        f(rec.rcnse + rec.son.rneff, 7, 1),
        f(rec.dl, 7, 1),
        f(rec.sygu, 8, 1),
        f(rec.sigm, 8, 1),
        f(rec.sygl, 8, 1)
    ));
    out.push_str(&format!(
        "  RELIAB ={}{}{}\n",
        f(rec.d90r, 7, 1),
        f(rec.d50r, 7, 1),
        f(rec.d10r, 7, 1)
    ));
    out.push_str(&format!(
        "  SPROB = {}{}{}\n",
        f(rec.d90s, 7, 1),
        f(rec.d50s, 7, 1),
        f(rec.d10s, 7, 1)
    ));
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn outmuf_drops_the_luf_column_for_every_method_but_26() {
        let header = crate::engine::output::Header {
            coeff: "CCIR".into(),
            method: 3,
            model: crate::engine::output::MODEL.into(),
            version: "16.1207W".into(),
            month: 1,
            year: "  1990".into(),
            ssn: 100.0,
            amind: 0.1,
            znoise: 145.0,
            lufp: 90,
            rsn: 24.0,
            pmp: 3.0,
            dmp: 0.1,
            label: " ".repeat(40),
            long_path: false,
            tx_lat: 0.0,
            tx_lon: 0.0,
            rx_lat: 0.0,
            rx_lon: 0.0,
            btrd: 0.0,
            brtd: 0.0,
            gcd_km: 0.0,
            antennas: Vec::new(),
            top: crate::engine::output::top_lines(3, None),
        };
        let hour = MufHourOut {
            gmt: 1.0,
            lmt: 2.0,
            fot: 9.0,
            hpf: 12.0,
            esmuf: 3.0,
            allmuf: 11.0,
            angmuf: -1.0,
            xluf: -1.0,
            layers: [super::super::muf::LayerMuf::default(); 4],
        };
        let mut pager = Pager::new(&header, 55);
        let text = outmuf(&mut pager, std::slice::from_ref(&hour), 3);
        let row = text.lines().last().unwrap();
        assert_eq!(row, "      1.0   2.0   9.00  12.00   3.00  11.00");
        let mut pager = Pager::new(&header, 55);
        let text = outmuf(&mut pager, std::slice::from_ref(&hour), 26);
        let row = text.lines().last().unwrap();
        assert_eq!(row, "      1.0   2.0   9.00  12.00   3.00  11.00  -1.00");
        // The header of a 24-hour run carries no tilde: `OUTMUF` runs
        // after the hour loop, where `JTX` is 24.
        let hours = vec![hour; 24];
        let mut pager = Pager::new(&header, 55);
        let text = outmuf(&mut pager, &hours, 3);
        assert!(text.lines().next().unwrap().contains(" METHOD"));
    }
}
