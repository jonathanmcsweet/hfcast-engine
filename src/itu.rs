//! Drives the ITU-R reference implementation of Recommendation P.533.
//!
//! `ITURHFProp` is the ITU-R Study Group 3 reference program: about 13,300
//! lines of C for the model itself, against VOACAP's 22,800 lines of FORTRAN
//! 77. It computes the same kind of answer from the same kind of inputs, but it
//! is a different model, not a different implementation of the same one.
//!
//! Its input is a key-value file rather than punched cards, and `Path.hour` and
//! `Path.frequency` both accept comma lists, so one run covers a whole sweep
//! case. Its output is comma-separated with a documented column list.
//!
//! Not every input maps cleanly from a VOACAP deck. Man-made noise is a
//! category here and a number there, and the required signal-to-noise ratio is
//! defined over a stated bandwidth rather than implicitly. Those mismatches are
//! why the engine comparison reports propagation quantities and leaves the
//! noise-dependent ones out; see `src/bin/engines.rs`.

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::deck::DeckCase;
use crate::runner::RunError;

/// Where the built reference program and its data live.
#[derive(Debug, Clone)]
pub struct ItuPaths {
    pub bin: PathBuf,
    pub data: PathBuf,
    /// Directories holding `libp533.so` and `libp372.so`.
    pub lib: Vec<PathBuf>,
}

impl ItuPaths {
    /// Locations produced by building the checkout in `vendor/itu-r-hf`.
    pub fn from_checkout(root: &Path) -> Self {
        Self {
            bin: root.join("ITURHFProp/Linux/ITURHFProp"),
            data: root.join("P372/Data/"),
            lib: vec![root.join("P533/Linux"), root.join("P372/Linux")],
        }
    }

    pub fn is_built(&self) -> bool {
        self.bin.is_file()
    }
}

/// P.372 man-made noise categories, as named in `Noise.h`.
///
/// A VOACAP deck states man-made noise as a number of dB below 1 W at 3 MHz.
/// P.533 takes a category instead, so the sweep's two settings are mapped to
/// the nearest named environment. This is an approximation, and it is the main
/// reason noise-dependent outputs are not compared between the engines.
fn noise_category(noise_dbw: f64) -> &'static str {
    // The categories bracket roughly -125 (city) to -155 (quiet rural).
    if noise_dbw >= 150.0 {
        "QUIETRURAL"
    } else if noise_dbw >= 140.0 {
        "RURAL"
    } else if noise_dbw >= 130.0 {
        "RESIDENTIAL"
    } else {
        "CITY"
    }
}

/// Transmit power in dB relative to 1 kW, which is what `Path.txpower` wants.
fn dbkw(watts: f64) -> f64 {
    10.0 * (watts / 1000.0).log10()
}

/// Writes the input file for one sweep case.
///
/// The area-coverage corners are all set to the receive point because this is a
/// point-to-point run; the program still requires them.
pub fn write_input(
    case: &DeckCase,
    to: &Path,
    data_dir: &Path,
    bandwidth_hz: f64,
) -> io::Result<()> {
    let hours: Vec<String> = (1..=24).map(|h| h.to_string()).collect();
    let freqs: Vec<String> = case.freqs_mhz.iter().map(|f| format!("{f:.3}")).collect();

    let text = format!(
        concat!(
            "PathName \"{id}\"\n",
            "PathTXName \"TX\"\n",
            "Path.L_tx.lat {tx_lat}\n",
            "Path.L_tx.lng {tx_lon}\n",
            "TXAntFilePath \"ISOTROPIC\"\n",
            "TXGOS 0.0\n",
            "PathRXName \"RX\"\n",
            "Path.L_rx.lat {rx_lat}\n",
            "Path.L_rx.lng {rx_lon}\n",
            "RXAntFilePath \"ISOTROPIC\"\n",
            "RXGOS 0.0\n",
            "AntennaOrientation \"ARBITRARY\"\n",
            "Path.year {year}\n",
            "Path.month {month}\n",
            "Path.hour {hours}\n",
            "Path.SSN {ssn}\n",
            "Path.frequency {freqs}\n",
            "Path.txpower {txpower:.4}\n",
            "Path.BW {bandwidth:.1}\n",
            "Path.SNRr {snr:.1}\n",
            "Path.SNRXXp 90\n",
            "Path.ManMadeNoise \"{noise}\"\n",
            "Path.Modulation \"ANALOG\"\n",
            "Path.SIRr 23.76\n",
            "Path.A 0.0\n",
            "Path.TW 0.0\n",
            "Path.FW 0.0\n",
            "Path.T0 0.0\n",
            "Path.F0 0.0\n",
            "Path.SorL \"SHORTPATH\"\n",
            "RptFileFormat \"RPT_D | RPT_BMUF | RPT_OPMUF | RPT_E | RPT_PR | RPT_SNR | RPT_DOMMODE\"\n",
            "LL.lat {rx_lat}\nLL.lng {rx_lon}\n",
            "LR.lat {rx_lat}\nLR.lng {rx_lon}\n",
            "UL.lat {rx_lat}\nUL.lng {rx_lon}\n",
            "UR.lat {rx_lat}\nUR.lng {rx_lon}\n",
            "latinc 1.0\nlnginc 1.0\n",
            "DataFilePath \"{data}\"\n",
        ),
        id = case.id,
        tx_lat = case.from_lat,
        tx_lon = case.from_lon,
        rx_lat = case.to_lat,
        rx_lon = case.to_lon,
        year = case.year,
        month = case.month,
        hours = hours.join(","),
        ssn = case.ssn.round() as i64,
        freqs = freqs.join(","),
        txpower = dbkw(case.watts),
        bandwidth = bandwidth_hz,
        snr = case.required_snr_db,
        noise = noise_category(case.noise_dbw),
        data = data_dir.display(),
    );

    fs::write(to, text)
}

/// Runs one case and returns the report text.
///
/// The program is built as a wrapper around two shared libraries that are not
/// installed, so their directories are put on the loader path for the child.
pub fn run_case(
    paths: &ItuPaths,
    case: &DeckCase,
    work: &Path,
    bandwidth_hz: f64,
) -> Result<String, ItuError> {
    let input = work.join("case.in");
    let output = work.join("case.out");
    write_input(case, &input, &paths.data, bandwidth_hz)?;
    // A stale report from an earlier case would otherwise be read back if the
    // program failed to write a new one.
    let _ = fs::remove_file(&output);

    let joined = std::env::join_paths(&paths.lib)
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();

    let out = std::process::Command::new(&paths.bin)
        .arg(&input)
        .arg(&output)
        .env("LD_LIBRARY_PATH", joined)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .output()?;

    if !out.status.success() {
        return Err(ItuError::Run(RunError::Failed {
            code: out.status.code(),
            output: String::from_utf8_lossy(&out.stderr).into_owned(),
        }));
    }

    let text = fs::read_to_string(&output)?;
    if parse_report(&text).is_empty() {
        return Err(ItuError::NoData);
    }
    Ok(text)
}

/// One output row: a single hour and frequency.
#[derive(Debug, Clone, PartialEq)]
pub struct ItuRow {
    /// UTC hour folded to 0-23, matching the listing parser's convention.
    pub hour: u8,
    pub freq_mhz: f64,
    pub distance_km: f64,
    /// Path basic MUF.
    pub bmuf: f64,
    /// Operational MUF.
    pub opmuf: f64,
    /// Field strength, dB relative to 1 uV/m.
    pub field_strength: f64,
    /// Median receiver power, dB.
    pub receiver_power: f64,
    pub snr: f64,
    /// Dominant propagation mode, such as `1F2`.
    pub mode: String,
}

#[derive(Debug)]
pub enum ItuError {
    Run(RunError),
    Io(io::Error),
    /// The program completed but printed no data rows.
    NoData,
}

impl fmt::Display for ItuError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ItuError::Run(e) => write!(f, "{e}"),
            ItuError::Io(e) => write!(f, "io error: {e}"),
            ItuError::NoData => write!(f, "no data rows in the report"),
        }
    }
}

impl From<io::Error> for ItuError {
    fn from(e: io::Error) -> Self {
        ItuError::Io(e)
    }
}

impl From<RunError> for ItuError {
    fn from(e: RunError) -> Self {
        ItuError::Run(e)
    }
}

/// Reads the comma-separated block between the calculated-parameters markers.
///
/// The header repeats the column list, so rows are recognised by shape rather
/// than by position in the file: a data row starts with two integers and has at
/// least ten fields.
pub fn parse_report(text: &str) -> Vec<ItuRow> {
    let mut rows = Vec::new();

    for line in text.lines() {
        let fields: Vec<&str> = line.split(',').map(|f| f.trim()).collect();
        if fields.len() < 10 {
            continue;
        }
        // Column 1 is the month and column 2 the hour; both are plain integers
        // only on data rows.
        let (Ok(_month), Ok(hour_raw)) = (fields[0].parse::<u32>(), fields[1].parse::<u32>())
        else {
            continue;
        };

        let number = |index: usize| -> Option<f64> { fields.get(index)?.parse::<f64>().ok() };

        let (
            Some(freq_mhz),
            Some(distance_km),
            Some(bmuf),
            Some(opmuf),
            Some(field_strength),
            Some(receiver_power),
            Some(snr),
        ) = (
            number(2),
            number(3),
            number(4),
            number(5),
            number(6),
            number(7),
            number(8),
        )
        else {
            continue;
        };

        rows.push(ItuRow {
            // The program numbers hours 1..=24; fold 24 to 0 as the VOACAP
            // listing parser does, so the two can be aligned.
            hour: (hour_raw % 24) as u8,
            freq_mhz,
            distance_km,
            bmuf,
            opmuf,
            field_strength,
            receiver_power,
            snr,
            mode: fields.get(9).unwrap_or(&"").to_string(),
        });
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a_case() -> DeckCase {
        DeckCase {
            id: "med-eu".into(),
            method: 30,
            from_lat: 35.8,
            from_lon: -5.9,
            to_lat: 44.9,
            to_lon: 20.5,
            month: 7,
            year: 2026,
            ssn: 70.0,
            watts: 100.0,
            required_snr_db: 24.0,
            noise_dbw: 145.0,
            freqs_mhz: vec![7.1, 14.2],
            tx_antenna: None,
            rx_antenna: None,
            sporadic_e: false,
        }
    }

    #[test]
    fn hundred_watts_is_minus_ten_dbkw() {
        assert!((dbkw(100.0) - -10.0).abs() < 1e-12);
        assert!((dbkw(1000.0) - 0.0).abs() < 1e-12);
    }

    #[test]
    fn sweep_noise_settings_map_to_named_environments() {
        assert_eq!(noise_category(145.0), "RURAL");
        assert_eq!(noise_category(125.0), "CITY");
    }

    #[test]
    fn input_lists_every_hour_and_frequency() {
        let dir = std::env::temp_dir().join("propcore-itu-input-test");
        fs::create_dir_all(&dir).expect("temp dir");
        let file = dir.join("case.in");
        write_input(&a_case(), &file, Path::new("/data/"), 2500.0).expect("write");
        let text = fs::read_to_string(&file).expect("read back");

        assert!(text
            .contains("Path.hour 1,2,3,4,5,6,7,8,9,10,11,12,13,14,15,16,17,18,19,20,21,22,23,24"));
        assert!(text.contains("Path.frequency 7.100,14.200"));
        assert!(text.contains("Path.txpower -10.0000"));
        assert!(text.contains("Path.ManMadeNoise \"RURAL\""));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Two real rows, copied from a report.
    const REPORT: &str = concat!(
        "Column 01: Month\n",
        "07, 23,   21.200,  2440.92,  16.34,  19.61,  -9.16,-142.89,  -4.72,  1F2 ,   9.75\n",
        "07, 24,    1.840,  2440.92,  15.46,  18.55,   8.56,-103.93,  -2.15,  2F2 ,  26.19\n",
    );

    #[test]
    fn parses_data_rows_and_ignores_the_header() {
        let rows = parse_report(REPORT);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].hour, 23);
        assert!((rows[0].bmuf - 16.34).abs() < 1e-9);
        assert_eq!(rows[0].mode, "1F2");
        assert!((rows[1].freq_mhz - 1.84).abs() < 1e-9);
    }

    #[test]
    fn folds_hour_24_to_zero_like_the_listing_parser() {
        let rows = parse_report(REPORT);
        assert_eq!(rows[1].hour, 0);
    }

    #[test]
    fn reads_negative_columns_that_run_against_their_neighbour() {
        // `-9.16,-142.89` has no space after the comma.
        let rows = parse_report(REPORT);
        assert!((rows[0].field_strength - -9.16).abs() < 1e-9);
        assert!((rows[0].receiver_power - -142.89).abs() < 1e-9);
    }
}
