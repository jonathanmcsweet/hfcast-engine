//! The prediction the server consumes, as JSON on stdout.
//!
//! Reads one request object from stdin and writes one prediction object to
//! stdout. A process boundary rather than a binding, because it is the least
//! machinery that removes the Fortran toolchain from the deployment.
//!
//! ```text
//! echo '{"fromLat":47.6,...}' | predict
//! ```
//!
//! Everything about the request and the answer is in [`hfcast::service`],
//! which an application with the engine compiled in calls directly.

use std::io::{self, Read, Write};

use hfcast::json::{obj, str_of};
use std::process::ExitCode;

fn main() -> ExitCode {
    let mut input = String::new();
    if let Err(e) = io::stdin().read_to_string(&mut input) {
        return fail(&format!("could not read stdin: {e}"));
    }
    // Temporary: step timers, off unless asked for. See `src/perf.rs`.
    let timing = std::env::var_os("HFCAST_PERF").is_some();
    if timing {
        hfcast::perf::enable();
    }
    let started = std::time::Instant::now();
    let answer = hfcast::service::run(&input);
    if timing {
        eprint!("{}", hfcast::perf::report(started.elapsed()));
    }
    match answer {
        Ok(text) => {
            let mut out = io::stdout().lock();
            if writeln!(out, "{text}").is_err() {
                return ExitCode::FAILURE;
            }
            ExitCode::SUCCESS
        }
        Err(e) => fail(&e),
    }
}

fn fail(message: &str) -> ExitCode {
    let body = obj([("error", str_of(message))]).write();
    println!("{body}");
    eprintln!("predict: {message}");
    ExitCode::FAILURE
}
