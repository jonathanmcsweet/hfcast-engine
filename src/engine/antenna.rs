//! Antenna gain tables: the `ANTCALC` and `GAIN` path.
//!
//! The engine does not read an antenna definition file while it
//! predicts. `ANTCALC` runs first, turns each `ANTENNA` card into a
//! table of 30 frequencies by 91 elevation angles, and writes it to
//! `run/gainNN.dat`; `DECRED` reads that back into `/cantenna/`, and
//! `GAIN` interpolates it per mode. The file is therefore the interface
//! between the two halves, which also makes it the verification
//! surface: `antcheck` compares this module's table against the one the
//! reference wrote, at the 0.001 dB the file's `f7.3` fields carry.
//!
//! Antenna types, from `parm(2)` of the definition file:
//!
//! | type | family | ported |
//! | --- | --- | --- |
//! | 0 | isotrope | yes |
//! | 1-9 | CCIR (`ccirgain`, `gainrel`) | yes |
//! | 10 | vertical monopole, table | yes |
//! | 11 | gain table over 91 elevations | yes |
//! | 12 | NTIA curtain arrays (`curtain`) | yes |
//! | 13 | gain table over 360 azimuths | yes |
//! | 14 | gain table over 30 frequencies | yes |
//! | 21-30 | IONCAP (`ioninit`, `iongain`) | yes |
//! | 31-47 | HFMUFES (`mufesint`, `mufesgan`) | no |
//! | 48 | NOSC (`invcon`) | yes |
//! | 90+ | Harris (`harris`) | no |
//!
//! The unported families return [`Unsupported`] rather than a wrong
//! number, so `antcheck` reports them as pending instead of passing.

use std::fs;
use std::path::Path;

use super::con::R;

/// Frequencies in a table: 1 to 30 MHz, one row each.
pub const FREQS: usize = 30;
/// Elevation angles in a table: 0 to 90 degrees, one column each.
pub const ELEVS: usize = 91;

/// Which end of the circuit an antenna serves (`iat` on the card).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AntennaEnd {
    Transmit,
    Receive,
}

/// A family this module does not compute yet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsupported {
    pub jant: i32,
    pub family: &'static str,
}

impl std::fmt::Display for Unsupported {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "antenna type {} ({}) is not ported", self.jant, self.family)
    }
}

/// A parsed antenna definition file (`readant`).
#[derive(Debug, Clone)]
pub struct AntennaFile {
    pub description: String,
    pub parm: [R; 20],
    /// How `setmaxgain` derives the maximum gain: 0 takes `parm(1)`,
    /// 1 interpolates on operating and design frequency, 2 on the
    /// operating frequency alone, 3 is the curtain normalising factor.
    pub modegain: i32,
    /// `gainmax(3,2)`, for types 1-4, 8 and 9.
    pub gainmax: [[R; 3]; 2],
    /// `gainmaxb(30)`, for types 5-7, 10 and 12.
    pub gainmaxb: [R; FREQS],
    /// `gain10(90,29)`: elevation 1-90 by frequency 2-30.
    pub gain10: Vec<[R; 90]>,
    /// Type 11's gain at each of 91 elevation angles.
    pub gain_type11: [R; ELEVS],
    /// Type 13's 360 azimuths by 91 elevations.
    pub type13: Vec<[R; ELEVS]>,
    /// Type 14's 30 frequencies: an efficiency and 91 elevations each.
    pub type14: Vec<(R, [R; ELEVS])>,
}

impl Default for AntennaFile {
    fn default() -> Self {
        Self {
            description: String::new(),
            parm: [0.0; 20],
            modegain: 0,
            gainmax: [[0.0; 3]; 2],
            gainmaxb: [0.0; FREQS],
            gain10: Vec::new(),
            gain_type11: [0.0; ELEVS],
            type13: Vec::new(),
            type14: Vec::new(),
        }
    }
}

impl AntennaFile {
    /// The antenna type, `parm(2)` rounded.
    pub fn jant(&self) -> i32 {
        nint(self.parm[1])
    }
}

/// A gain table, as `ANTCALC` computes it and `GAIN` consumes it.
#[derive(Debug, Clone)]
pub struct GainTable {
    /// `array(30,91)`: gain in dB at frequency 1-30 MHz by elevation
    /// 0-90 degrees. Rows outside the card's frequency range stay zero,
    /// because `ANTCALC` clears the whole table and only fills the
    /// range the card asked for.
    pub gains: Vec<[R; ELEVS]>,
    /// `aeff(30)`: the efficiency per frequency row.
    pub eff: [R; FREQS],
    /// The second header line of `gainNN.dat`: first and last frequency,
    /// the main beam bearing, the off-azimuth, then `parm(4)` and
    /// `parm(3)` — conductivity and dielectric constant.
    pub fs: R,
    pub fe: R,
    pub beam_main: R,
    pub offazim: R,
    pub cond: R,
    pub diel: R,
}

impl Default for GainTable {
    fn default() -> Self {
        Self {
            gains: vec![[0.0; ELEVS]; FREQS],
            eff: [0.0; FREQS],
            fs: 0.0,
            fe: 0.0,
            beam_main: 0.0,
            offazim: 0.0,
            cond: 0.0,
            diel: 0.0,
        }
    }
}

/// One `ANTENNA` card, plus the path azimuth the pattern is cut along.
#[derive(Debug, Clone)]
pub struct AntennaSetup<'a> {
    pub file: &'a AntennaFile,
    pub end: AntennaEnd,
    /// The card's frequency range, in whole MHz.
    pub min_freq: i32,
    pub max_freq: i32,
    pub design_freq: R,
    pub beam_deg: R,
    /// The card's last field. For a transmitter this is power in kW;
    /// for a receiver a non-zero value is reused as the isotrope's
    /// gain, which is what the card's own comment calls fixing the
    /// isotrope gain.
    pub power_field: R,
    /// Azimuth from this end to the other, from [`dazel0`].
    pub azimuth_deg: R,
}

/// Fortran's `NINT`: round half away from zero.
fn nint(v: R) -> i32 {
    if v >= 0.0 {
        (v + 0.5) as i32
    } else {
        (v - 0.5) as i32
    }
}

/// Reads a fixed-width numeric field, Fortran style: blank is zero.
fn fixed(line: &str, start: usize, width: usize) -> R {
    let bytes: Vec<char> = line.chars().collect();
    if start >= bytes.len() {
        return 0.0;
    }
    let end = (start + width).min(bytes.len());
    let text: String = bytes[start..end].iter().collect();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return 0.0;
    }
    trimmed.parse::<R>().unwrap_or(0.0)
}

/// Takes the first list-directed value from a record.
///
/// The definition files carry a value then a label — `  0.00  [ 1] Max
/// Gain dBi..:` — and a list-directed read stops as soon as its list is
/// satisfied, so everything after the number is ignored.
fn first_value(line: &str) -> Option<R> {
    line.split_whitespace().next().and_then(|t| t.parse().ok())
}

/// Removes every blank, as `rblankc` does.
///
/// This is why an antenna path cannot contain a space: the blanks are
/// stripped rather than the name being quoted, so `my ant` and `myant`
/// name the same file.
fn strip_blanks(s: &str) -> String {
    s.chars().filter(|c| !c.is_whitespace()).collect()
}

/// Reads an antenna definition file (`readant`).
///
/// `relative` is the card's bracketed field, relative to
/// `<itshfbc>/antennas`. An empty name is the engine's "no file" case
/// and gives a 0 dB isotrope.
pub fn read_antenna(itshfbc: &Path, relative: &str) -> Result<AntennaFile, String> {
    let name = strip_blanks(relative);
    // `nch.le.2` in the source: a name of two characters or fewer, or
    // one ending in the path separator, means no antenna was named.
    if name.len() <= 2 || name.ends_with('/') {
        return Ok(AntennaFile {
            description: "0 dB gain".to_string(),
            ..Default::default()
        });
    }

    let path = itshfbc.join("antennas").join(&name);
    let text = fs::read_to_string(&path)
        .map_err(|e| format!("opening antenna file {}: {e}", path.display()))?;
    let lines: Vec<&str> = text.lines().collect();
    let mut next = 0usize;
    let mut take = || -> Result<&str, String> {
        let line = lines
            .get(next)
            .copied()
            .ok_or_else(|| format!("antenna file {name} ended early"))?;
        next += 1;
        Ok(line)
    };

    let description = take()?.chars().take(70).collect::<String>();
    let n = first_value(take()?).ok_or_else(|| format!("{name}: no parameter count"))? as usize;
    let mut out = AntennaFile {
        description,
        ..Default::default()
    };
    for i in 0..n.min(20) {
        let line = take()?;
        out.parm[i] =
            first_value(line).ok_or_else(|| format!("{name}: parameter {} unreadable", i + 1))?;
    }

    let jant = out.jant();
    out.modegain = 0;
    if (1..=4).contains(&jant) || jant == 8 || jant == 9 {
        out.modegain = 1;
        for row in 0..2 {
            let line = take()?;
            let mut values = line.split_whitespace();
            for col in 0..3 {
                out.gainmax[row][col] = values
                    .next()
                    .and_then(|t| t.parse().ok())
                    .ok_or_else(|| format!("{name}: gainmax row {row} short"))?;
            }
        }
    } else if (5..=7).contains(&jant) {
        out.modegain = 2;
        // (10x,10f6.2): three records of ten.
        for chunk in 0..3 {
            let line = take()?;
            for i in 0..10 {
                out.gainmaxb[chunk * 10 + i] = fixed(line, 10 + i * 6, 6);
            }
        }
    } else if jant == 10 {
        out.modegain = 2;
        // (10x,f6.2,(t19,10f6.2)) per frequency 2 to 30: the maximum
        // gain, then 90 elevation values ten to a line from column 19.
        for _ in 2..=30 {
            let line = take()?;
            let index = out.gain10.len();
            out.gainmaxb[index + 1] = fixed(line, 10, 6);
            let mut row = [0.0; 90];
            for (i, slot) in row.iter_mut().enumerate().take(10) {
                *slot = fixed(line, 18 + i * 6, 6);
            }
            for block in 1..9 {
                let cont = take()?;
                for i in 0..10 {
                    row[block * 10 + i] = fixed(cont, 18 + i * 6, 6);
                }
            }
            out.gain10.push(row);
        }
    } else if jant == 11 {
        // List-directed over as many records as it takes to fill 91.
        let mut values: Vec<R> = Vec::with_capacity(ELEVS);
        while values.len() < ELEVS {
            let line = take()?;
            for token in line.split_whitespace() {
                if values.len() == ELEVS {
                    break;
                }
                values.push(
                    token
                        .parse()
                        .map_err(|_| format!("{name}: bad gain value {token:?}"))?,
                );
            }
        }
        out.gain_type11.copy_from_slice(&values);
    } else if jant == 12 {
        out.modegain = 3;
        // (10x,5f10.3): six records of five. A file that stops early
        // is not an error — the source fills the whole set with
        // -99999 and carries on.
        let mut normalising = [0.0; FREQS];
        let mut complete = true;
        for chunk in 0..6 {
            match take() {
                Ok(line) => {
                    for i in 0..5 {
                        normalising[chunk * 5 + i] = fixed(line, 10 + i * 10, 10);
                    }
                }
                Err(_) => {
                    complete = false;
                    break;
                }
            }
        }
        out.gainmaxb = if complete {
            normalising
        } else {
            [-99999.0; FREQS]
        };
    } else if jant == 13 {
        // (9x,10f7.3) per azimuth: 91 elevations, ten to a line.
        for _ in 0..360 {
            let mut row = [0.0; ELEVS];
            let line = take()?;
            for (i, slot) in row.iter_mut().enumerate().take(10) {
                *slot = fixed(line, 9 + i * 7, 7);
            }
            for block in 1..10 {
                let cont = take()?;
                for i in 0..10 {
                    let slot = block * 10 + i;
                    if slot < ELEVS {
                        row[slot] = fixed(cont, 9 + i * 7, 7);
                    }
                }
            }
            out.type13.push(row);
        }
    } else if jant == 14 {
        // (2x,f6.1,(t10,10f7.3)) per frequency: an efficiency, then 91
        // elevations ten to a line from column 10.
        for _ in 0..FREQS {
            let line = take()?;
            let eff = fixed(line, 2, 6);
            let mut row = [0.0; ELEVS];
            for (i, slot) in row.iter_mut().enumerate().take(10) {
                *slot = fixed(line, 9 + i * 7, 7);
            }
            for block in 1..10 {
                let cont = take()?;
                for i in 0..10 {
                    let slot = block * 10 + i;
                    if slot < ELEVS {
                        row[slot] = fixed(cont, 9 + i * 7, 7);
                    }
                }
            }
            out.type14.push((eff, row));
        }
    }

    Ok(out)
}

/// `gainterb`: interpolates a per-MHz maximum gain.
fn gainterb(gainab: &[R; FREQS], freq: R) -> R {
    let mut idx = freq as usize;
    if idx == 30 {
        idx = 29;
    }
    let fact = freq - idx as R;
    // The table is one-based in the source, so index `idx` is slot
    // `idx - 1` here.
    let lo = gainab[idx.saturating_sub(1)];
    let hi = gainab[idx.min(FREQS - 1)];
    lo + (hi - lo) * fact
}

/// `setmaxgain`, for the modes this module can already reach.
fn max_gain(file: &AntennaFile, parm: &[R; 20], freq_oper: R) -> Result<R, Unsupported> {
    match file.modegain {
        0 => Ok(parm[0]),
        2 => Ok(gainterb(&file.gainmaxb, freq_oper)),
        // Mode 1 needs `gainterp` over the design frequency, and mode 3
        // the curtain normalising factor; both belong with the CCIR
        // families that use them.
        _ => Err(Unsupported {
            jant: file.jant(),
            family: "CCIR maximum-gain interpolation",
        }),
    }
}

/// Builds the point-to-point table for one card (`ANTCALC`).
pub fn point_to_point_table(s: &AntennaSetup) -> Result<GainTable, Unsupported> {
    let file = s.file;
    let mut parm = file.parm;
    let jant = file.jant();

    // The card's last field doubles as the receive isotrope's gain.
    let mut design_freq = s.design_freq;
    if s.end == AntennaEnd::Receive && s.power_field != 0.0 {
        design_freq = s.power_field;
    }
    if jant == 0 {
        parm[0] = design_freq;
    }

    let mut table = GainTable {
        fs: s.min_freq as R,
        fe: s.max_freq as R,
        beam_main: s.beam_deg,
        cond: parm[3],
        diel: parm[2],
        ..Default::default()
    };
    let mut offazim = s.azimuth_deg - s.beam_deg;
    if offazim < 0.0 {
        offazim += 360.0;
    }
    table.offazim = offazim;

    let lo = s.min_freq.max(1);
    let hi = s.max_freq.min(FREQS as i32);

    match jant {
        0 | 10 | 11 => {
            for ifreq in lo..=hi {
                let freq = ifreq as R;
                parm[4] = freq;
                let giso = max_gain(file, &parm, freq)?;
                if jant == 11 {
                    table.eff[(ifreq - 1) as usize] = parm[2];
                }
                let row = &mut table.gains[(ifreq - 1) as usize];
                for (ielev, slot) in row.iter_mut().enumerate() {
                    *slot = antcal(file, &parm, giso, freq, ielev as R)?;
                }
            }
        }
        13 => {
            // One azimuth cut through the 360-azimuth table, the same
            // for every frequency in the card's range.
            let mut az = offazim;
            if az < 0.0 {
                az += 360.0;
            }
            let iazim = az as usize;
            let mut iazim2 = iazim + 1;
            if iazim2 == 360 {
                iazim2 = 0;
            }
            let fract = az - iazim as R;
            let mut cut = [0.0; ELEVS];
            for (ielev, slot) in cut.iter_mut().enumerate() {
                let g1 = file.type13[iazim % 360][ielev];
                let g2 = file.type13[iazim2 % 360][ielev];
                *slot = g1 + (g2 - g1) * fract + parm[0];
            }
            for ifreq in lo..=hi {
                table.eff[(ifreq - 1) as usize] = parm[2];
                table.gains[(ifreq - 1) as usize] = cut;
            }
        }
        14 => {
            for ifreq in lo..=hi {
                let (eff, row) = file.type14[(ifreq - 1) as usize];
                table.eff[(ifreq - 1) as usize] = eff;
                let out = &mut table.gains[(ifreq - 1) as usize];
                for (slot, value) in out.iter_mut().zip(row.iter()) {
                    *slot = value + parm[0];
                }
            }
        }
        1..=9 => {
            // The REC705 patterns. ANTINIT2's first act is SETMAXGAIN,
            // which overwrites parm(5) and parm(8) (and parm(6) for
            // the quadrant), so the mutation must happen before the
            // parameter extraction reads them.
            for ifreq in lo..=hi {
                let freq = ifreq as R;
                parm[4] = freq;
                let giso = super::ccir::setmaxgain(file, &mut parm, freq, design_freq)
                    .expect("types 1-9 always have a max-gain mode");
                let ant = super::ccir::antinit2(file, &parm);
                let row = &mut table.gains[(ifreq - 1) as usize];
                for (ielev, slot) in row.iter_mut().enumerate() {
                    *slot = ant.ccirgain(ielev as R, offazim, giso);
                }
            }
        }
        12 => {
            // The NTIA curtain normalises from the file's GainNorm
            // table; ANTINIT2 returns before touching anything for
            // type 12, so no parm mutation happens here.
            for ifreq in lo..=hi {
                let freq = ifreq as R;
                parm[4] = freq;
                // ANTCAL's gnorm interpolation; at whole frequencies it
                // reads the slot exactly.
                let ifq = ifreq as usize;
                let gn = if ifq < 30 {
                    file.gainmaxb[ifq - 1]
                        + (freq - ifq as R) * (file.gainmaxb[ifq] - file.gainmaxb[ifq - 1])
                } else {
                    file.gainmaxb[29]
                };
                let row = &mut table.gains[(ifreq - 1) as usize];
                for (ielev, slot) in row.iter_mut().enumerate() {
                    let mut g = super::ccir::curtain::gain(&parm, offazim, ielev as R, gn);
                    if g < -30.0 {
                        g = -30.0;
                    }
                    *slot = g;
                }
            }
        }
        21..=30 => {
            let indx = jant - 20;
            let ip = super::ioncap::ioninit(indx, &parm);
            // The reference's SAVE state persists for the whole
            // program, so with several IONCAP cards in one deck the
            // stale-X quirk crosses antennas. One antenna per table
            // build starts clean, which matches a single-card deck.
            let mut ion_state = super::ioncap::IoncapState::default();
            for ifreq in lo..=hi {
                let freq = ifreq as R;
                let row = (ifreq - 1) as usize;
                for ielev in 0..ELEVS {
                    // ANTCALC's own degree-to-radian constant, shorter
                    // than the engine's D2R.
                    let delev = ielev as R * 0.017_453_29;
                    let (rain, eff) =
                        super::ioncap::iongain(&mut ion_state, indx, offazim, &ip, delev, freq);
                    table.gains[row][ielev] = rain;
                    table.eff[row] = eff;
                }
            }
        }
        31..=47 => {
            return Err(Unsupported {
                jant,
                family: "HFMUFES",
            })
        }
        48 => {
            // The inverted cone is a measured table and takes neither
            // the maximum gain nor an efficiency.
            for ifreq in lo..=hi {
                let freq = ifreq as R;
                let row = &mut table.gains[(ifreq - 1) as usize];
                for (ielev, slot) in row.iter_mut().enumerate() {
                    *slot = invcon(freq, ielev as R);
                }
            }
        }
        _ => {
            return Err(Unsupported {
                jant,
                family: "Harris",
            })
        }
    }

    // The file's f7.3 fields cannot hold a number below -99.99, so the
    // source clamps before writing rather than printing asterisks. The
    // clamp is part of what the engine later reads back, so it belongs
    // in the table and not in the writer.
    for row in table.gains.iter_mut() {
        for slot in row.iter_mut() {
            if *slot < -99.99 {
                *slot = -99.99;
            }
        }
    }
    Ok(table)
}

/// `antcal`: one gain, for the types whose pattern is a table.
fn antcal(
    file: &AntennaFile,
    _parm: &[R; 20],
    giso: R,
    freq: R,
    elev_deg: R,
) -> Result<R, Unsupported> {
    match file.jant() {
        0 => Ok(giso),
        10 => {
            let ielev = nint(elev_deg).clamp(0, 90);
            if ielev == 0 {
                // The monopole's table starts at one degree, and the
                // source answers a flat -30 dB at the horizon.
                return Ok(-30.0);
            }
            let ifreq = freq as usize;
            let row = ielev as usize - 1;
            if ifreq < 30 {
                // gain10(ielev, ifreq-1) and (ielev, ifreq) in the
                // source: the table starts at 2 MHz, so column
                // `ifreq-1` is the frequency `ifreq`.
                let lo = file.gain10[ifreq.saturating_sub(2)][row];
                let hi = file.gain10[(ifreq - 1).min(file.gain10.len() - 1)][row];
                Ok(lo + (freq - ifreq as R) * (hi - lo))
            } else {
                Ok(file.gain10[28][row])
            }
        }
        11 => {
            let ielev = nint(elev_deg).clamp(0, 90) as usize;
            Ok(giso + file.gain_type11[ielev])
        }
        jant => Err(Unsupported {
            jant,
            family: "pattern model",
        }),
    }
}

/// `invcon`'s frequency axis, in MHz.
const CONE_FREQS: [R; 22] = [
    2.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0, 16.0, 17.0, 18.0, 20.0,
    22.0, 24.0, 26.0, 28.0, 30.0,
];

/// `invcon`'s elevation axis, in degrees.
const CONE_ELEVS: [R; 13] = [
    0.0, 5.0, 10.0, 15.0, 20.0, 25.0, 30.0, 35.0, 40.0, 45.0, 50.0, 55.0, 90.0,
];

/// The NOSC inverted cone's measured gain, one row per elevation angle
/// of [`CONE_ELEVS`], each across the frequencies of [`CONE_FREQS`].
const CONE: [[R; 22]; 13] = [
    [-20.0; 22],
    [
        -9.0, -7.0, -8.2, -8.6, -8.8, -8.3, -7.7, -7.7, -8.3, -8.3, -8.0, -8.0, -8.8, -10.0, -10.0,
        -10.0, -10.0, -10.0, -10.0, -10.0, -10.0, -10.0,
    ],
    [
        -5.0, -3.0, -3.5, -3.8, -3.9, -4.0, -4.1, -3.9, -4.7, -5.1, -4.9, -4.7, -6.4, -7.0, -6.5,
        -6.0, -6.4, -7.6, -8.9, -10.0, -10.0, -10.0,
    ],
    [
        -2.1, -0.1, -0.7, -1.0, -1.3, -1.4, -1.5, -1.7, -2.0, -2.6, -2.6, -2.9, -4.7, -4.7, -4.0,
        -3.9, -4.7, -5.6, -6.7, -8.0, -10.0, -10.0,
    ],
    [
        -0.7, 1.3, 0.8, 0.4, 0.1, -0.7, -2.2, -2.3, -1.7, -2.4, -5.1, -6.7, -5.0, -3.8, -3.5, -3.7,
        -4.3, -5.1, -5.7, -6.7, -8.3, -10.0,
    ],
    [
        -0.5, 1.5, 1.4, 1.2, 0.3, -1.0, -4.2, -4.7, -3.1, -2.9, -6.6, -6.9, -4.0, -3.2, -3.7, -4.2,
        -4.8, -5.3, -5.6, -6.1, -7.6, -9.0,
    ],
    [
        -0.3, 1.7, 1.6, 1.5, 1.2, -1.6, -5.5, -7.2, -6.0, -5.0, -8.0, -8.5, -3.5, -2.7, -4.0, -4.7,
        -5.3, -5.7, -6.0, -6.9, -8.1, -10.0,
    ],
    [
        -0.4, 1.5, 1.5, 1.5, 1.5, -2.2, -7.0, -11.5, -7.5, -5.8, -10.4, -12.0, -2.5, -2.0, -4.3,
        -5.3, -6.3, -6.9, -7.4, -8.0, -9.0, -10.0,
    ],
    [
        -0.5, 1.5, 1.5, 1.5, 1.3, -3.0, -8.4, -10.6, -6.7, -5.2, -9.8, -14.0, -2.4, -2.4, -5.0,
        -6.2, -7.0, -7.8, -8.3, -10.0, -10.0, -10.0,
    ],
    [
        -0.8, 1.2, 1.3, 1.2, 0.3, -4.1, -10.5, -11.3, -5.9, -5.0, -8.5, -17.0, -7.5, -6.0, -6.4,
        -6.9, -7.6, -8.3, -10.0, -10.0, -10.0, -10.0,
    ],
    [
        -1.5, 0.5, 0.4, 0.1, -1.0, -5.4, -12.1, -10.3, -6.1, -5.2, -8.5, -17.0, -15.0, -10.3, -8.5,
        -8.0, -8.1, -8.8, -10.0, -10.0, -10.0, -10.0,
    ],
    [
        -2.2, -0.2, -0.5, -1.0, -2.2, -7.3, -12.0, -8.5, -5.6, -5.2, -7.5, -15.5, -20.0, -15.0,
        -10.5, -8.9, -8.9, -10.0, -10.0, -10.0, -10.0, -10.0,
    ],
    [-40.0; 22],
];

/// `invcon`: the NOSC inverted-cone antenna, type 48.
///
/// A measured table interpolated in frequency and elevation. The source
/// searches each axis from its second entry for the first bound at or
/// above the wanted value, and leaves the index unset if the value is
/// past the end of the axis — a frequency above 30 MHz or an elevation
/// above 90 degrees would read an uninitialised local. Neither is
/// reachable from an `ANTENNA` card, and the fall-back here is the last
/// interval.
pub fn invcon(freq: R, elev_deg: R) -> R {
    let interp = |a: R, b: R, c: R| a * (1.0 - c) + b * c;
    let weight = |a: R, b: R, c: R| (a - b) / (c - b);

    let ian = CONE_ELEVS
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, bound)| elev_deg <= **bound)
        .map(|(i, _)| i)
        .unwrap_or(CONE_ELEVS.len() - 1);
    let ifr = CONE_FREQS
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, bound)| freq <= **bound)
        .map(|(i, _)| i)
        .unwrap_or(CONE_FREQS.len() - 1);

    let fwt = weight(freq, CONE_FREQS[ifr], CONE_FREQS[ifr - 1]);
    let awt = weight(elev_deg, CONE_ELEVS[ian], CONE_ELEVS[ian - 1]);
    let g1 = interp(CONE[ian][ifr], CONE[ian - 1][ifr], awt);
    let g2 = interp(CONE[ian][ifr - 1], CONE[ian - 1][ifr - 1], awt);
    interp(g1, g2, fwt)
}

/// `GAIN`'s interpolation: gain and efficiency at a frequency and
/// elevation angle.
///
/// Two out-of-range reads in the source are worth naming, because this
/// port cannot reproduce them. `I = FMC` truncates, so a frequency
/// below 1 MHz indexes `array(0,...)`, and one at or above 31 MHz
/// indexes past the end; the elevation index goes the same way for a
/// negative angle. Fortran does not check bounds, so those read
/// whatever sits next to the table. The clamps here keep the port
/// inside its arrays, and every frequency an `ANTENNA` card admits
/// (2 to 30 MHz) is inside the table anyway.
pub fn gain_lookup(table: &GainTable, fmc: R, delta_rad: R) -> (R, R) {
    let i = (fmc as i32).clamp(1, FREQS as i32);
    let ip1 = (i + 1).min(FREQS as i32);
    let xfmc = fmc - (fmc as i32) as R;
    let deltd = delta_rad.to_degrees();
    let j = ((deltd + 1.0) as i32).clamp(1, ELEVS as i32);
    let jp1 = (j + 1).min(ELEVS as i32);
    let xdelta = deltd - (j - 1) as R;

    let (i0, i1) = ((i - 1) as usize, (ip1 - 1) as usize);
    let (j0, j1) = ((j - 1) as usize, (jp1 - 1) as usize);
    let xx = table.gains[i0][j0];
    let yx = table.gains[i1][j0];
    let xy = table.gains[i0][j1];
    let yy = table.gains[i1][j1];
    let rx = xx + xfmc * (yx - xx);
    let ry = xy + xfmc * (yy - xy);
    let rain = rx + xdelta * (ry - rx);
    let eff = table.eff[i0] + xfmc * (table.eff[i1] - table.eff[i0]);
    (rain, eff)
}

/// `DAZEL0` mode 0: the azimuth and great-circle distance from one
/// point to another.
///
/// This is the antenna half's own geometry, separate from
/// `engine::geometry`, and it differs on purpose: it computes in double
/// precision and uses a 6370 km Earth, so the azimuth a pattern is cut
/// along is not bit-identical to the path azimuth the propagation model
/// uses. Both are reproduced as they are.
///
/// The arguments are `R` because the source's are `REAL*4`: a
/// coordinate arrives already rounded to single precision and is then
/// widened. Accepting `f64` here once flipped the last printed digit of
/// borderline gain cells, because 35.80 as an `f32` is 35.799999
/// widened, not 35.8.
pub fn dazel0(tlat: R, tlon: R, rlat: R, rlon: R) -> (R, R) {
    const RERTH: f64 = 6370.0;
    const DTOR: f64 = 0.01745329252;
    const RTOD: f64 = 57.29577951;

    let mut tlats = f64::from(tlat);
    let mut rlats = f64::from(rlat);
    let rlons = f64::from(rlon);
    let tlons = f64::from(tlon);
    if tlats <= -90.0 {
        tlats = -89.999;
    }
    if tlats >= 90.0 {
        tlats = 89.999;
    }
    if rlats <= -90.0 {
        rlats = -89.999;
    }
    if rlats >= 90.0 {
        rlats = 89.999;
    }
    // Directly opposite points leave the azimuth undefined, so the
    // source moves the receiver a tenth of a degree.
    if ((tlons - rlons).abs() - 180.0).abs() <= 0.001 && (tlats + rlats).abs() <= 0.002 {
        rlats += 0.1;
        if rlats >= 90.0 {
            rlats = 89.9;
        }
    }

    let delat = rlats - tlats;
    let adlat = delat.abs();
    let mut delon = rlons - tlons;
    while delon < -180.0 {
        delon += 360.0;
    }
    while delon > 180.0 {
        delon -= 360.0;
    }
    let adlon = delon.abs();

    if adlon <= 1.0e-5 {
        if adlat <= 1.0e-5 {
            return (0.0, 0.0);
        }
        // Same longitude: due north or due south.
        let ztaz = if delat <= 0.0 { 180.0 } else { 0.0 };
        let gc = adlat * DTOR;
        return (ztaz as R, (gc * RERTH) as R);
    }

    // The source names the western point W and the eastern point E and
    // computes both azimuths, then picks by the sign of the longitude
    // difference.
    let (wlat, elat) = if delon <= 0.0 {
        (rlats * DTOR, tlats * DTOR)
    } else {
        (tlats * DTOR, rlats * DTOR)
    };
    let sdlat_half = (0.5 * adlat * DTOR).sin();
    let sdlon_half = (0.5 * adlon * DTOR).sin();
    let sadln = (adlon * DTOR).sin();
    let cwlat = wlat.cos();
    let celat = elat.cos();
    let p = 2.0 * (sdlat_half * sdlat_half + sdlon_half * sdlon_half * cwlat * celat);
    let sgc = (p * (2.0 - p)).sqrt();
    let sdlat = (elat - wlat).sin();
    let cwaz = (2.0 * celat * wlat.sin() * sdlon_half * sdlon_half + sdlat) / sgc;
    let swaz = sadln * celat / sgc;
    let waz = swaz.atan2(cwaz) * RTOD;
    let ceaz = (2.0 * cwlat * elat.sin() * sdlon_half * sdlon_half - sdlat) / sgc;
    let seaz = sadln * cwlat / sgc;
    let eaz = 360.0 - seaz.atan2(ceaz) * RTOD;

    let ztaz = if delon <= 0.0 { eaz } else { waz };
    let gc = sgc.atan2(1.0 - p);
    (ztaz as R, (gc * RERTH) as R)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn tree() -> PathBuf {
        std::env::var_os("HFCAST_ITSHFBC")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").expect("HOME")).join("itshfbc")
            })
    }

    #[test]
    fn an_empty_name_is_a_zero_db_isotrope() {
        let file = read_antenna(&tree(), "   ").expect("no-file case");
        assert_eq!(file.jant(), 0);
        assert_eq!(file.parm[0], 0.0);
        assert_eq!(file.description, "0 dB gain");
    }

    #[test]
    fn the_isotrope_file_parses() {
        let file = read_antenna(&tree(), "default/isotrope     ").expect("isotrope");
        assert_eq!(file.jant(), 0);
        assert_eq!(file.parm[0], 0.0);
        assert_eq!(file.modegain, 0);
    }

    #[test]
    fn an_isotropes_table_is_its_max_gain_everywhere_in_band() {
        let file = read_antenna(&tree(), "default/isotrope").expect("isotrope");
        let table = point_to_point_table(&AntennaSetup {
            file: &file,
            end: AntennaEnd::Transmit,
            min_freq: 2,
            max_freq: 30,
            design_freq: 0.0,
            beam_deg: 0.0,
            power_field: 0.1,
            azimuth_deg: 57.0,
        })
        .expect("isotrope table");
        // Row 1 is outside the card's range and stays zero.
        assert!(table.gains[0].iter().all(|g| *g == 0.0));
        assert!(table.gains[9].iter().all(|g| *g == 0.0));
        assert_eq!(table.fs, 2.0);
        assert_eq!(table.fe, 30.0);
    }

    #[test]
    fn a_receive_isotrope_takes_its_gain_from_the_power_field() {
        // The card's last field is power for a transmitter and the
        // isotrope's gain for a receiver.
        let file = read_antenna(&tree(), "default/isotrope").expect("isotrope");
        let table = point_to_point_table(&AntennaSetup {
            file: &file,
            end: AntennaEnd::Receive,
            min_freq: 2,
            max_freq: 30,
            design_freq: 0.0,
            beam_deg: 0.0,
            power_field: 6.0,
            azimuth_deg: 0.0,
        })
        .expect("isotrope table");
        assert_eq!(table.gains[9][0], 6.0);
    }

    #[test]
    fn a_type_11_table_adds_its_max_gain() {
        let file = read_antenna(&tree(), "default/swwhip.voa").expect("swwhip");
        assert_eq!(file.jant(), 11);
        // The file's first elevation value.
        assert_eq!(file.gain_type11[0], -20.0);
        assert_eq!(file.gain_type11[ELEVS - 1], -21.9);
        let table = point_to_point_table(&AntennaSetup {
            file: &file,
            end: AntennaEnd::Receive,
            min_freq: 2,
            max_freq: 30,
            design_freq: 0.0,
            beam_deg: 0.0,
            power_field: 0.0,
            azimuth_deg: 0.0,
        })
        .expect("table");
        assert_eq!(table.gains[9][0], -20.0);
        // parm(3) is the efficiency for this type.
        assert_eq!(table.eff[9], -4.8);
    }

    #[test]
    fn unported_families_say_so_instead_of_answering() {
        let file = read_antenna(&tree(), "samples/sample.31").expect("sample.31");
        let err = point_to_point_table(&AntennaSetup {
            file: &file,
            end: AntennaEnd::Transmit,
            min_freq: 2,
            max_freq: 30,
            design_freq: 0.0,
            beam_deg: 0.0,
            power_field: 0.1,
            azimuth_deg: 57.0,
        })
        .expect_err("HFMUFES is not ported yet");
        assert_eq!(err.jant, 31);
    }

    #[test]
    fn the_inverted_cone_reads_its_table_at_the_grid_points() {
        // The table's own corners, where no interpolation happens.
        assert_eq!(invcon(2.0, 0.0), -20.0);
        assert_eq!(invcon(4.0, 5.0), -7.0);
        assert_eq!(invcon(30.0, 90.0), -40.0);
        // Halfway between 4 and 5 MHz at 10 degrees: -3.0 and -3.5.
        assert!((invcon(4.5, 10.0) - -3.25).abs() < 1e-4);
    }

    #[test]
    fn gain_lookup_interpolates_between_rows_and_columns() {
        let mut table = GainTable::default();
        // 10 dB at 10 MHz and 20 dB at 11 MHz, flat in elevation.
        table.gains[9] = [10.0; ELEVS];
        table.gains[10] = [20.0; ELEVS];
        table.eff[9] = 1.0;
        table.eff[10] = 3.0;
        let (gain, eff) = gain_lookup(&table, 10.5, 0.0);
        assert!((gain - 15.0).abs() < 1e-4, "got {gain}");
        assert!((eff - 2.0).abs() < 1e-4, "got {eff}");
    }

    #[test]
    fn dazel0_matches_the_reference_azimuth_for_the_test_circuit() {
        // The vendor test circuit, whose gain file records az = 57.41
        // at the transmitter and 254.7 at the receiver.
        let (az, dist) = dazel0(35.8, -5.9, 44.9, 20.5);
        assert!((az - 57.41).abs() < 0.01, "azimuth {az}");
        assert!(dist > 2400.0 && dist < 2500.0, "distance {dist}");
        let (back, _) = dazel0(44.9, 20.5, 35.8, -5.9);
        assert!((back - 254.7).abs() < 0.05, "reverse azimuth {back}");
    }

    #[test]
    fn dazel0_handles_the_degenerate_cases() {
        assert_eq!(dazel0(10.0, 20.0, 10.0, 20.0), (0.0, 0.0));
        let (az, _) = dazel0(10.0, 20.0, 40.0, 20.0);
        assert_eq!(az, 0.0);
        let (az, _) = dazel0(40.0, 20.0, 10.0, 20.0);
        assert_eq!(az, 180.0);
    }
}
