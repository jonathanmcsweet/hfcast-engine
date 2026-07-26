//! Reads IRTAM foF2 coefficient files and writes them the way VOACAP expects.
//!
//! IRTAM (the IRI-based Real-Time Assimilative Model) refits the global foF2
//! map every 15 minutes from GIRO ionosonde soundings and publishes the
//! result as ASCII coefficient files (<https://ulcar.uml.edu/GAMBIT/>). The
//! basis is the same Jones-Gallet expansion VOACAP's climatology uses: 76
//! spatial functions, 13 temporal (diurnal) terms each, laid out temporal
//! fastest — verified by matching the block-start value pattern against
//! VOACAP's own `fof2CCIR.daw`. IRTAM appends one extra map of 76 linear
//! trend coefficients, which this reader drops: the harmonic terms alone
//! describe the diurnal cycle over the file's trailing 24-hour window.
//!
//! VOACAP reads foF2 from `coeffs/fof2CCIR.daw`: 12 direct-access records
//! (one per month) of 13 x 76 x 2 little-endian float32 — two maps, one for
//! sunspot number 0 and one for 100, interpolated linearly (`redmap.for`).
//! Writing the same IRTAM map into both planes makes the foF2 input
//! independent of the sunspot number, which is the point: the measured
//! ionosphere replaces the proxy.

use std::io;
use std::path::Path;

/// Temporal terms VOACAP uses per spatial function.
pub const TEMPORAL: usize = 13;
/// Spatial functions in the Jones-Gallet foF2 basis.
pub const SPATIAL: usize = 76;
/// One `.daw` record: 13 x 76 coefficients at two sunspot levels, float32.
pub const RECORD_BYTES: usize = TEMPORAL * SPATIAL * 2 * 4;

/// One IRTAM foF2 map: the 13 x 76 harmonic coefficients, temporal fastest.
#[derive(Debug, Clone)]
pub struct IrtamMap {
    pub coeffs: Vec<f64>,
}

/// Parses an `IRTAM_foF2_COEFFS_*.ASC` file.
///
/// The header is `#` lines; the body is 14 x 76 = 1064 numbers of which the
/// last 76 are the linear-trend map. A mean foF2 outside 1-20 MHz in the
/// leading coefficient means the file is not what this expects.
pub fn parse_asc(text: &str) -> Result<IrtamMap, String> {
    let values: Vec<f64> = text
        .lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .flat_map(str::split_whitespace)
        .map(|t| t.parse::<f64>().map_err(|e| format!("bad number {t}: {e}")))
        .collect::<Result<_, _>>()?;
    let expected = (TEMPORAL + 1) * SPATIAL;
    if values.len() != expected {
        return Err(format!(
            "expected {expected} coefficients, found {}",
            values.len()
        ));
    }
    let mean = values[0];
    if !(1.0..=20.0).contains(&mean) {
        return Err(format!(
            "leading coefficient {mean} is not a mean foF2 in MHz"
        ));
    }
    Ok(IrtamMap {
        coeffs: values[..TEMPORAL * SPATIAL].to_vec(),
    })
}

pub fn load_asc(path: &Path) -> io::Result<Result<IrtamMap, String>> {
    Ok(parse_asc(&std::fs::read_to_string(path)?))
}

/// Builds a complete `fof2CCIR.daw` replacement holding this map.
///
/// Every month record gets the same map in both sunspot planes, so whichever
/// record and sunspot number the run asks for, foF2 comes out as the IRTAM
/// values.
pub fn daw_file(map: &IrtamMap) -> Vec<u8> {
    let mut record = Vec::with_capacity(RECORD_BYTES);
    for plane in 0..2 {
        let _ = plane;
        for value in &map.coeffs {
            record.extend_from_slice(&(*value as f32).to_le_bytes());
        }
    }
    let mut file = Vec::with_capacity(RECORD_BYTES * 12);
    for month in 0..12 {
        let _ = month;
        file.extend_from_slice(&record);
    }
    file
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_text() -> String {
        let mut body = String::from(
            "# START_HEADER\n# Ionospheric Characteristic: foF2 [MHz]\n# END_HEADER\n",
        );
        for i in 0..(TEMPORAL + 1) * SPATIAL {
            let value = if i == 0 { 8.8 } else { 0.001 * i as f64 };
            body.push_str(&format!(" {value:.6e}"));
            if i % 4 == 3 {
                body.push('\n');
            }
        }
        body
    }

    #[test]
    fn parses_and_drops_the_trend_block() {
        let map = parse_asc(&sample_text()).expect("parses");
        assert_eq!(map.coeffs.len(), TEMPORAL * SPATIAL);
        assert!((map.coeffs[0] - 8.8).abs() < 1e-9);
        // The last kept coefficient is index 987, not one from the trend map.
        let last = map.coeffs[TEMPORAL * SPATIAL - 1];
        assert!((last - 0.001 * 987.0).abs() < 1e-9);
    }

    #[test]
    fn rejects_a_wrong_sized_file() {
        assert!(parse_asc("# header\n1.0 2.0 3.0\n").is_err());
    }

    #[test]
    fn rejects_an_implausible_mean() {
        let text = sample_text().replacen("8.8", "88.0", 1);
        assert!(parse_asc(&text).is_err());
    }

    #[test]
    fn daw_file_has_twelve_identical_records_with_doubled_planes() {
        let map = parse_asc(&sample_text()).expect("parses");
        let file = daw_file(&map);
        assert_eq!(file.len(), RECORD_BYTES * 12);
        // Record 5 equals record 0.
        assert_eq!(
            file[..RECORD_BYTES],
            file[RECORD_BYTES * 5..RECORD_BYTES * 6]
        );
        // Within a record, plane 2 equals plane 1.
        let half = RECORD_BYTES / 2;
        assert_eq!(file[..half], file[half..RECORD_BYTES]);
        // The first float is the mean foF2.
        let first = f32::from_le_bytes([file[0], file[1], file[2], file[3]]);
        assert!((first - 8.8).abs() < 1e-6);
    }
}
