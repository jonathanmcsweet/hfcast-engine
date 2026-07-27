//! Turns NOAA SWPC's feeds into the inputs one prediction needs.
//!
//! The soak runs the same paths every day against that day's real
//! space weather, so the input dimension a fixed corpus holds still is
//! sampled over the soak's length. This program does the arithmetic;
//! fetching is left to whatever runs it, so a run is reproducible from
//! the two files it was given.
//!
//! Usage:
//!
//! ```text
//! spacewx --flux f107.json --kp kp.json --month YYYY-MM
//! ```
//!
//! Prints `key=value` lines for a shell to read. Any missing or
//! unparseable input is an error and exits non-zero: a soak day that
//! quietly fell back to a default would look live and would not be.

use std::path::PathBuf;
use std::process::ExitCode;

use hfcast::json::{self, Json};

/// SWPC uses -1 rather than null for "not computed yet".
const MISSING: f64 = -1.0;

/// Invert the standard F10.7 to sunspot number relation
///
/// ```text
/// F = 63.7 + 0.728 R + 0.00089 R^2
/// ```
///
/// which is the fit VOACAP's own documentation uses in the other
/// direction.
fn ssn_from_f107(f107: f64) -> f64 {
    let a = 0.00089;
    let b = 0.728;
    let c = 63.7 - f107;
    let discriminant = b * b - 4.0 * a * c;
    if discriminant <= 0.0 {
        return 0.0;
    }
    ((-b + discriminant.sqrt()) / (2.0 * a)).max(0.0)
}

fn read_json(path: &PathBuf) -> Result<Json, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    json::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn run() -> Result<String, String> {
    let argv: Vec<String> = std::env::args().collect();
    let flag = |name: &str| -> Option<String> {
        argv.iter()
            .position(|a| a == name)
            .and_then(|i| argv.get(i + 1))
            .cloned()
    };
    let flux_path = PathBuf::from(flag("--flux").ok_or("--flux FILE is required")?);
    let kp_path = PathBuf::from(flag("--kp").ok_or("--kp FILE is required")?);
    let month_arg = flag("--month").ok_or("--month YYYY-MM is required")?;

    let (year, month) = month_arg
        .split_once('-')
        .ok_or_else(|| format!("--month {month_arg}: expected YYYY-MM"))?;
    let year: u32 = year
        .parse()
        .map_err(|_| format!("--month {month_arg}: bad year"))?;
    let month: u32 = month
        .parse()
        .map_err(|_| format!("--month {month_arg}: bad month"))?;
    if !(1..=12).contains(&month) {
        return Err(format!("--month {month_arg}: month out of range"));
    }

    // The flux feed is newest first.
    let flux = read_json(&flux_path)?;
    let records = flux.as_array().ok_or("flux feed is not an array")?;
    let latest = records.first().ok_or("flux feed is empty")?;
    let f107 = latest
        .get("flux")
        .and_then(Json::as_f64)
        .filter(|v| *v > MISSING)
        .ok_or("flux feed has no current observation")?;
    let observed_at = latest
        .get("time_tag")
        .and_then(Json::as_str)
        .unwrap_or("unknown")
        .to_string();

    // The K index feed is oldest first, one record per three hours.
    // The current block plus the eight before it cover 24 hours, which
    // is the window the storm-spread measurement used.
    let kp_json = read_json(&kp_path)?;
    let kp_records = kp_json.as_array().ok_or("K index feed is not an array")?;
    let kp = kp_records
        .last()
        .and_then(|r| r.get("Kp"))
        .and_then(Json::as_f64)
        .ok_or("K index feed has no current observation")?;
    let start = kp_records.len().saturating_sub(9);
    let kp_max_24h = kp_records[start..]
        .iter()
        .filter_map(|r| r.get("Kp").and_then(Json::as_f64))
        .fold(0.0_f64, f64::max);

    // No Kp derate here. That heuristic belongs to an application
    // deciding what to show a user; a parity run wants the plain
    // relation, so that a day's input is a function of the feed and
    // nothing else.
    let ssn = ssn_from_f107(f107);

    Ok(format!(
        "month={month}\nyear={year}\nssn={ssn:.1}\nf107={f107:.1}\nkp={kp:.2}\nkpmax24h={kp_max_24h:.2}\nobserved_at={observed_at}\n"
    ))
}

fn main() -> ExitCode {
    match run() {
        Ok(text) => {
            print!("{text}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("spacewx: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_ssn_relation_inverts_its_own_forward_form() {
        for r in [0.0, 25.0, 80.0, 150.0, 220.0] {
            let f = 63.7 + 0.728 * r + 0.00089 * r * r;
            assert!((ssn_from_f107(f) - r).abs() < 0.01, "R={r}");
        }
    }

    #[test]
    fn a_flux_below_the_quiet_sun_floor_gives_zero_rather_than_a_negative() {
        assert_eq!(ssn_from_f107(60.0), 0.0);
    }
}
