//! The ionospheric coefficient database for one month at one sunspot number.
//!
//! Port of `redmap.for`. Two files under `coeffs/` supply the data:
//!
//! * `fof2CCIR.daw` (or `fof2URSI.daw`) — direct-access, one 7904-byte
//!   record per month, holding `XF2COF(13,76,2)` as little-endian f32: the
//!   foF2 spherical-harmonic map at sunspot numbers 0 and 100.
//! * `coeffNNw.bin` — gfortran sequential unformatted (each record framed
//!   by 4-byte length markers), nine records matching the nine `READ`
//!   statements in the Fortran.
//!
//! Arrays that exist at two sunspot levels are interpolated linearly to the
//! run's SSN; the rest pass through. The interpolation is done in f32 with
//! the source's exact expressions, so a run at SSN 70 produces bit-identical
//! coefficients to the Fortran.
//!
//! Array indexing here is the Fortran's reversed and 0-based: Fortran
//! `A(i,j,k)` is `a[k-1][j-1][i-1]`. That makes the innermost Rust index the
//! fastest-varying one in the file (Fortran column-major order), so arrays
//! fill in plain read order.

use std::io::{Error, ErrorKind, Result};
use std::path::Path;

use super::con::R;

/// One month's coefficient set after sunspot interpolation — the contents
/// of the Fortran commons `/ONE/`, `/TWO/` and `/FONE/` as `REDMAP` leaves
/// them (except `IA`/`IB`, which `REDMAP` does not touch).
#[derive(Debug, Clone)]
pub struct CoefficientSet {
    // COMMON /ONE/ — critical frequency maps.
    /// Solar-activity index table, `IKIM(10,6)`.
    pub ikim: [[i32; 10]; 6],
    /// foF2 map coefficients at the run SSN, `F2COF(13,76)`.
    pub f2cof: [[R; 13]; 76],
    /// M(3000)F2 map coefficients, `FM3COF(9,49)`.
    pub fm3cof: [[R; 9]; 49],
    /// Sporadic-E median map, `ESMCOF(7,61)`.
    pub esmcof: [[R; 7]; 61],
    /// Sporadic-E lower-decile map, `ESLCOF(5,55)`.
    pub eslcof: [[R; 5]; 55],
    /// Sporadic-E upper-decile map, `ESUCOF(5,55)`.
    pub esucof: [[R; 5]; 55],
    /// E-region map, `ERCOF(9,22)`.
    pub ercof: [[R; 9]; 22],
    // COMMON /TWO/ — noise coefficients and distribution tables.
    /// `F2D(16,6,6)`.
    pub f2d: [[[R; 16]; 6]; 6],
    /// `FAKP(29,16,6)`.
    pub fakp: [[[R; 29]; 16]; 6],
    /// `FAKMAP(29,16)`.
    pub fakmap: [[R; 29]; 16],
    /// F2 height ratio map at the run SSN, `HMYM(29,16)`.
    pub hmym: [[R; 29]; 16],
    /// `FAKABP(2,6)`.
    pub fakabp: [[R; 2]; 6],
    /// `ABMAP(2,3)`.
    pub abmap: [[R; 2]; 3],
    /// `DUD(5,12,5)`.
    pub dud: [[[R; 5]; 12]; 5],
    /// `FAM(14,12)`.
    pub fam: [[R; 14]; 12],
    /// `SYS(9,16,6)`.
    pub sys: [[[R; 9]; 16]; 6],
    /// `PERR(9,4,6)`.
    pub perr: [[[R; 9]; 4]; 6],
    // COMMON /FONE/ — F1 layer monthly coefficients.
    pub anew: [R; 3],
    pub bnew: [R; 3],
    pub achi: [R; 2],
    pub bchi: [R; 2],
}

/// A cursor over little-endian 4-byte values.
struct Cursor<'a> {
    bytes: &'a [u8],
    pos: usize,
    /// Which file the cursor reads, for error messages.
    name: &'a str,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8], name: &'a str) -> Self {
        Self {
            bytes,
            pos: 0,
            name,
        }
    }

    fn take4(&mut self) -> Result<[u8; 4]> {
        let end = self.pos + 4;
        let slice = self.bytes.get(self.pos..end).ok_or_else(|| {
            Error::new(
                ErrorKind::UnexpectedEof,
                format!("{}: truncated at byte {}", self.name, self.pos),
            )
        })?;
        self.pos = end;
        Ok([slice[0], slice[1], slice[2], slice[3]])
    }

    fn f32(&mut self) -> Result<R> {
        Ok(R::from_le_bytes(self.take4()?))
    }

    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_le_bytes(self.take4()?))
    }

    /// Checks a gfortran record marker (the byte count framing a record).
    fn marker(&mut self, expected: u32) -> Result<()> {
        let got = u32::from_le_bytes(self.take4()?);
        if got != expected {
            return Err(Error::new(
                ErrorKind::InvalidData,
                format!(
                    "{}: record marker {} where {} was expected at byte {}",
                    self.name,
                    got,
                    expected,
                    self.pos - 4
                ),
            ));
        }
        Ok(())
    }
}

/// Fills a 2-D array in Fortran column-major read order.
fn read2<const I: usize, const J: usize>(c: &mut Cursor) -> Result<[[R; I]; J]> {
    let mut out = [[0.0 as R; I]; J];
    for row in out.iter_mut() {
        for v in row.iter_mut() {
            *v = c.f32()?;
        }
    }
    Ok(out)
}

/// Fills a 3-D array in Fortran column-major read order.
fn read3<const I: usize, const J: usize, const K: usize>(c: &mut Cursor) -> Result<[[[R; I]; J]; K]> {
    let mut out = [[[0.0 as R; I]; J]; K];
    for plane in out.iter_mut() {
        for row in plane.iter_mut() {
            for v in row.iter_mut() {
                *v = c.f32()?;
            }
        }
    }
    Ok(out)
}

fn read1<const I: usize>(c: &mut Cursor) -> Result<[R; I]> {
    let mut out = [0.0 as R; I];
    for v in out.iter_mut() {
        *v = c.f32()?;
    }
    Ok(out)
}

/// `XF2COF(13,76,2)` — one direct-access record of the foF2 file.
const F2_RECORD_BYTES: usize = 13 * 76 * 2 * 4;

/// Reads the month's `XF2COF(13,76,2)` from a `.daw` file's bytes.
fn f2_planes(bytes: &[u8], month: u32, name: &str) -> Result<[[[R; 13]; 76]; 2]> {
    let start = (month as usize - 1) * F2_RECORD_BYTES;
    let record = bytes.get(start..start + F2_RECORD_BYTES).ok_or_else(|| {
        Error::new(
            ErrorKind::UnexpectedEof,
            format!("{name}: no record for month {month}"),
        )
    })?;
    let mut c = Cursor::new(record, name);
    read3::<13, 76, 2>(&mut c)
}

/// The nine sequential records of `coeffNNw.bin`, before interpolation.
struct RawMonthFile {
    ikim: [[i32; 10]; 6],
    fakp: [[[R; 29]; 16]; 6],
    fakabp: [[R; 2]; 6],
    dud: [[[R; 5]; 12]; 5],
    fam: [[R; 14]; 12],
    sys: [[[R; 9]; 16]; 6],
    xfm3cf: [[[R; 9]; 49]; 2],
    f2d: [[[R; 16]; 6]; 6],
    perr: [[[R; 9]; 4]; 6],
    anew: [R; 3],
    bnew: [R; 3],
    achi: [R; 2],
    bchi: [R; 2],
    fakmap: [[R; 29]; 16],
    abmap: [[R; 2]; 3],
    xesmcf: [[[R; 7]; 61]; 2],
    xpmap: [[[R; 29]; 16]; 2],
    xeslcf: [[[R; 5]; 55]; 2],
    xesucf: [[[R; 5]; 55]; 2],
    xercof: [[[R; 9]; 22]; 2],
}

fn parse_month_file(bytes: &[u8], name: &str) -> Result<RawMonthFile> {
    let c = &mut Cursor::new(bytes, name);
    // Each Fortran READ consumes one record; the marker values below are
    // the record payload sizes, summed element counts times 4 bytes.
    let record = |len: u32, c: &mut Cursor| c.marker(len);

    record(240, c)?;
    let mut ikim = [[0i32; 10]; 6];
    for row in ikim.iter_mut() {
        for v in row.iter_mut() {
            *v = c.i32()?;
        }
    }
    record(240, c)?;

    record(11184, c)?;
    let fakp = read3::<29, 16, 6>(c)?;
    let fakabp = read2::<2, 6>(c)?;
    record(11184, c)?;

    record(5328, c)?;
    let dud = read3::<5, 12, 5>(c)?;
    let fam = read2::<14, 12>(c)?;
    let sys = read3::<9, 16, 6>(c)?;
    record(5328, c)?;

    record(3528, c)?;
    let xfm3cf = read3::<9, 49, 2>(c)?;
    record(3528, c)?;

    record(3168, c)?;
    let f2d = read3::<16, 6, 6>(c)?;
    let perr = read3::<9, 4, 6>(c)?;
    record(3168, c)?;

    record(1920, c)?;
    let anew = read1::<3>(c)?;
    let bnew = read1::<3>(c)?;
    let achi = read1::<2>(c)?;
    let bchi = read1::<2>(c)?;
    let fakmap = read2::<29, 16>(c)?;
    let abmap = read2::<2, 3>(c)?;
    record(1920, c)?;

    record(7128, c)?;
    let xesmcf = read3::<7, 61, 2>(c)?;
    let xpmap = read3::<29, 16, 2>(c)?;
    record(7128, c)?;

    record(4400, c)?;
    let xeslcf = read3::<5, 55, 2>(c)?;
    let xesucf = read3::<5, 55, 2>(c)?;
    record(4400, c)?;

    record(1584, c)?;
    let xercof = read3::<9, 22, 2>(c)?;
    record(1584, c)?;

    Ok(RawMonthFile {
        ikim,
        fakp,
        fakabp,
        dud,
        fam,
        sys,
        xfm3cf,
        f2d,
        perr,
        anew,
        bnew,
        achi,
        bchi,
        fakmap,
        abmap,
        xesmcf,
        xpmap,
        xeslcf,
        xesucf,
        xercof,
    })
}

/// Interpolates a pair of sunspot planes with `(a*wa + b*wb) / d`, the
/// Fortran's expression shape, element by element in f32.
fn blend<const I: usize, const J: usize>(
    x: &[[[R; I]; J]; 2],
    wa: R,
    wb: R,
    d: R,
) -> [[R; I]; J] {
    let mut out = [[0.0 as R; I]; J];
    for j in 0..J {
        for i in 0..I {
            out[j][i] = (x[0][j][i] * wa + x[1][j][i] * wb) / d;
        }
    }
    out
}

/// Which foF2 coefficient family the run uses (the `COEFFS` card).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FoF2Model {
    Ccir,
    Ursi,
}

impl FoF2Model {
    fn daw_name(self) -> &'static str {
        match self {
            FoF2Model::Ccir => "fof2CCIR.daw",
            FoF2Model::Ursi => "fof2URSI.daw",
        }
    }
}

/// Port of `REDMAP`: loads and interpolates the month's coefficients from
/// an `itshfbc` tree.
pub fn redmap(itshfbc: &Path, model: FoF2Model, month: u32, ssn: R) -> Result<CoefficientSet> {
    if !(1..=12).contains(&month) {
        return Err(Error::new(
            ErrorKind::InvalidInput,
            format!("month {month} out of range"),
        ));
    }
    let coeffs = itshfbc.join("coeffs");
    let daw_path = coeffs.join(model.daw_name());
    let daw = std::fs::read(&daw_path)?;
    let xf2cof = f2_planes(&daw, month, model.daw_name())?;

    let bin_name = format!("coeff{month:02}w.bin");
    let bin = std::fs::read(coeffs.join(&bin_name))?;
    let raw = parse_month_file(&bin, &bin_name)?;

    // The sunspot interpolations, factors exactly as redmap.for computes
    // them: SSN100 = 100-SSN and so on.
    let ssn100 = 100.0 - ssn;
    let ssn150 = 150.0 - ssn;
    let ssn10 = ssn - 10.0;
    let ssn125 = 125.0 - ssn;
    let ssn25 = ssn - 25.0;

    Ok(CoefficientSet {
        ikim: raw.ikim,
        f2cof: blend(&xf2cof, ssn100, ssn, 100.0),
        fm3cof: blend(&raw.xfm3cf, ssn100, ssn, 100.0),
        esmcof: blend(&raw.xesmcf, ssn150, ssn10, 140.0),
        eslcof: blend(&raw.xeslcf, ssn150, ssn10, 140.0),
        esucof: blend(&raw.xesucf, ssn150, ssn10, 140.0),
        ercof: blend(&raw.xercof, ssn150, ssn10, 140.0),
        hmym: blend(&raw.xpmap, ssn125, ssn25, 100.0),
        f2d: raw.f2d,
        fakp: raw.fakp,
        fakmap: raw.fakmap,
        fakabp: raw.fakabp,
        abmap: raw.abmap,
        dud: raw.dud,
        fam: raw.fam,
        sys: raw.sys,
        perr: raw.perr,
        anew: raw.anew,
        bnew: raw.bnew,
        achi: raw.achi,
        bchi: raw.bchi,
    })
}

impl CoefficientSet {
    /// Every array flattened in Fortran storage order, labelled with the
    /// names the trace dump uses — the stage-comparison surface.
    pub fn flattened(&self) -> Vec<(&'static str, Vec<f64>)> {
        fn f1(a: &[R]) -> Vec<f64> {
            a.iter().map(|&v| f64::from(v)).collect()
        }
        fn f2<const I: usize, const J: usize>(a: &[[R; I]; J]) -> Vec<f64> {
            a.iter().flatten().map(|&v| f64::from(v)).collect()
        }
        fn f3<const I: usize, const J: usize, const K: usize>(a: &[[[R; I]; J]; K]) -> Vec<f64> {
            a.iter().flatten().flatten().map(|&v| f64::from(v)).collect()
        }
        vec![
            (
                "IKIM",
                self.ikim.iter().flatten().map(|&v| f64::from(v)).collect(),
            ),
            ("F2COF", f2(&self.f2cof)),
            ("FM3COF", f2(&self.fm3cof)),
            ("ESMCOF", f2(&self.esmcof)),
            ("ESLCOF", f2(&self.eslcof)),
            ("ESUCOF", f2(&self.esucof)),
            ("ERCOF", f2(&self.ercof)),
            ("HMYM", f2(&self.hmym)),
            ("F2D", f3(&self.f2d)),
            ("FAKP", f3(&self.fakp)),
            ("FAKMAP", f2(&self.fakmap)),
            ("FAKABP", f2(&self.fakabp)),
            ("ABMAP", f2(&self.abmap)),
            ("DUD", f3(&self.dud)),
            ("FAM", f2(&self.fam)),
            ("SYS", f3(&self.sys)),
            ("PERR", f3(&self.perr)),
            ("ANEW", f1(&self.anew)),
            ("BNEW", f1(&self.bnew)),
            ("ACHI", f1(&self.achi)),
            ("BCHI", f1(&self.bchi)),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    /// Builds a synthetic gfortran sequential file whose every element is
    /// its running index, so any transposition or offset error shows up as
    /// a wrong value somewhere.
    fn synthetic_month_file() -> Vec<u8> {
        let sizes: &[u32] = &[240, 11184, 5328, 3528, 3168, 1920, 7128, 4400, 1584];
        let mut counter = 0u32;
        let mut out = Vec::new();
        for &len in sizes {
            out.write_all(&len.to_le_bytes()).unwrap();
            for _ in 0..len / 4 {
                if counter < 60 {
                    // The first record is the integer array IKIM.
                    out.write_all(&(counter as i32).to_le_bytes()).unwrap();
                } else {
                    out.write_all(&(counter as f32).to_le_bytes()).unwrap();
                }
                counter += 1;
            }
            out.write_all(&len.to_le_bytes()).unwrap();
        }
        out
    }

    #[test]
    fn the_month_file_layout_matches_the_fortran_reads() {
        let bytes = synthetic_month_file();
        assert_eq!(bytes.len(), 38552, "the real files are this exact size");
        let raw = parse_month_file(&bytes, "synthetic").expect("parse");

        // Spot checks against hand-computed running indices. IKIM(10,6)
        // is first: IKIM(3,2) is element 12 (0-based) of the file.
        assert_eq!(raw.ikim[0][0], 0);
        assert_eq!(raw.ikim[1][2], 12);
        // FAKP(29,16,6) starts at element 60; FAKP(2,3,4) is
        // 60 + (1) + (2)*29 + (3)*29*16 = 60+1+58+1392 = 1511.
        assert_eq!(raw.fakp[3][2][1], 1511.0);
        // FAKABP(2,6) follows FAKP: starts at 60 + 2784 = 2844.
        assert_eq!(raw.fakabp[0][0], 2844.0);
        // The last element of the file is XERCOF(9,22,2)'s final value:
        // 38480 payload bytes = 9620 elements, so index 9619.
        assert_eq!(raw.xercof[1][21][8], 9619.0);
    }

    #[test]
    fn a_wrong_record_marker_is_an_error() {
        let mut bytes = synthetic_month_file();
        bytes[0] = 39; // corrupt the first length marker
        assert!(parse_month_file(&bytes, "corrupt").is_err());
    }

    #[test]
    fn interpolation_matches_the_fortran_expressions() {
        // One element checked end to end: at SSN 70,
        // f = (x1*(100-70) + x2*70) / 100.
        let x = [[[2.0f32; 1]; 1], [[10.0f32; 1]; 1]];
        let out = blend(&x, 100.0 - 70.0, 70.0, 100.0);
        assert_eq!(out[0][0], (2.0 * 30.0 + 10.0 * 70.0) / 100.0);
        // The Es form: (x1*(150-SSN) + x2*(SSN-10)) / 140 — at SSN 10 the
        // second plane has zero weight, at SSN 150 the first does.
        let low = blend(&x, 150.0 - 10.0, 0.0, 140.0);
        assert_eq!(low[0][0], 2.0);
        let high = blend(&x, 0.0, 150.0 - 10.0, 140.0);
        assert_eq!(high[0][0], 10.0);
    }

    #[test]
    fn daw_month_selection_reads_the_right_record() {
        // Two records: month 1 all 1.0, month 2 all 2.0.
        let mut bytes = Vec::new();
        for month_value in [1.0f32, 2.0] {
            for _ in 0..13 * 76 * 2 {
                bytes.extend_from_slice(&month_value.to_le_bytes());
            }
        }
        let m2 = f2_planes(&bytes, 2, "synthetic").expect("read");
        assert_eq!(m2[0][0][0], 2.0);
        assert_eq!(m2[1][75][12], 2.0);
        assert!(f2_planes(&bytes, 3, "synthetic").is_err());
    }
}
