//! Step timers, for finding where an area run spends itself.
//!
//! **Temporary and not part of the engine's behaviour.** Nothing in the
//! model reads these; they exist so an optimisation can be chosen from a
//! measurement rather than from reading the code, which has already
//! produced one change worth a single percent.
//!
//! Off unless `HFCAST_PERF` is set, so the cost is one relaxed load per
//! timed region when it is not wanted.
//!
//! Counters nest: `gethp` runs inside `genion`, so their shares overlap
//! and do not sum to the whole. `report` says so rather than pretending
//! otherwise.
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Instant;

pub const GENION: usize = 0;
pub const GETHP: usize = 1;
pub const LUFFY: usize = 2;
pub const IONO_HOUR: usize = 3;
pub const FINDF: usize = 4;
const STEPS: usize = 5;

const NAMES: [&str; STEPS] =
    ["genion", "gethp", "luffy_freq_loop", "iono.hour", "findf"];

static ON: AtomicBool = AtomicBool::new(false);
static NS: [AtomicU64; STEPS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];
static CALLS: [AtomicU64; STEPS] = [
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
    AtomicU64::new(0),
];

thread_local! {
    /// Depth per step, so a recursive or re-entrant region is counted
    /// once rather than once per level.
    static DEPTH: Cell<[u32; STEPS]> = const { Cell::new([0; STEPS]) };
}

/// Turns the timers on. Called once, from whatever wants a report.
pub fn enable() {
    ON.store(true, Ordering::Relaxed);
}

pub fn enabled() -> bool {
    ON.load(Ordering::Relaxed)
}

/// Times the region it lives in, and stops at the end of the scope.
pub struct Step {
    which: usize,
    started: Option<Instant>,
}

impl Step {
    pub fn new(which: usize) -> Self {
        if !enabled() {
            return Step { which, started: None };
        }
        let outer = DEPTH.with(|d| {
            let mut levels = d.get();
            let was = levels[which];
            levels[which] = was + 1;
            d.set(levels);
            was
        });
        // Already inside this region, so the outer timer is already
        // counting the same nanoseconds.
        if outer > 0 {
            return Step { which, started: None };
        }
        CALLS[which].fetch_add(1, Ordering::Relaxed);
        Step { which, started: Some(Instant::now()) }
    }
}

impl Drop for Step {
    fn drop(&mut self) {
        if !enabled() {
            return;
        }
        DEPTH.with(|d| {
            let mut levels = d.get();
            levels[self.which] = levels[self.which].saturating_sub(1);
            d.set(levels);
        });
        if let Some(at) = self.started {
            let ns = at.elapsed().as_nanos() as u64;
            NS[self.which].fetch_add(ns, Ordering::Relaxed);
        }
    }
}

/// What each step cost, against the whole run.
pub fn report(whole: std::time::Duration) -> String {
    let total = whole.as_nanos() as f64;
    let mut out = String::from(
        "step               calls        ms   share\n\
         ------------------------------------------\n",
    );
    for i in 0..STEPS {
        let ns = NS[i].load(Ordering::Relaxed) as f64;
        let calls = CALLS[i].load(Ordering::Relaxed);
        out.push_str(&format!(
            "{:<16} {:>8} {:>9.1} {:>6.1}%\n",
            NAMES[i],
            calls,
            ns / 1e6,
            if total > 0.0 { 100.0 * ns / total } else { 0.0 },
        ));
    }
    out.push_str(&format!("{:<16} {:>8} {:>9.1} {:>6.1}%\n", "whole run", 1, total / 1e6, 100.0));
    out.push_str("\ngethp nests inside genion, so their shares overlap.\n");
    out
}
