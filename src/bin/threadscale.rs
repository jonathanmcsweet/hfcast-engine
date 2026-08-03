//! Does a batch of area strips get faster on more threads?
//!
//! The application cuts a whole-world grid into latitude strips and hands
//! them to a fixed thread pool inside one process — see
//! `HfcastEngineModule.kt`. A device then reported 3.9 seconds of engine
//! time for 34,560 points on "16 strips on 8 threads", which is about
//! what one core alone would give. Either the strips are not running in
//! parallel, or a phone core is far slower than assumed.
//!
//! This measures the first half of that, in the arrangement the
//! application actually uses: several threads in one address space,
//! sharing one allocator, each calling `service::run`. Separate
//! processes are not the same test — they share nothing, so they cannot
//! show contention that only threads have.
//!
//! Usage: `cargo run --release --bin threadscale [-- strips]`

use std::process::ExitCode;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;

/// The whole-world fine lattice the application runs.
const LAT_STEP: f64 = 1.25;
const LON_STEP: f64 = 1.5;

/// One strip of that lattice, as the sharding cuts it.
fn strip(index: usize, strips: usize) -> String {
    let span = 180.0 / strips as f64;
    let lat_min = -90.0 + span * index as f64;
    // The engine is asked for cell centres, so the bounds sit half a step
    // inside the edges — the same arithmetic `shard.ts` uses.
    let lo = lat_min + LAT_STEP / 2.0;
    let hi = lat_min + span - LAT_STEP / 2.0;
    format!(
        r#"{{"itshfbc":"<embedded>","mode":"area",
           "fromLat":33.75,"fromLon":-84.39,
           "month":8,"year":2026,"ssn":60,"watts":100,
           "requiredSnrDb":-24,"noiseDbw":-145,"hour":18,
           "freqMhz":7.1,
           "latStep":{LAT_STEP},"lonStep":{LON_STEP},
           "latMin":{lo},"latMax":{hi},
           "lonMin":-180,"lonMax":178.5}}"#
    )
}

/// Runs every strip across `threads` workers, and returns the wall time.
///
/// A channel hands work out rather than one strip per thread, so a pool
/// of four running sixteen strips is four workers taking the next strip
/// each time — which is what a fixed thread pool does.
fn run_all(requests: &[String], threads: usize) -> (u128, usize) {
    let started = Instant::now();
    let (send, recv) = mpsc::channel::<usize>();
    for i in 0..requests.len() {
        send.send(i).expect("the queue closed early");
    }
    drop(send);
    let recv = std::sync::Mutex::new(recv);

    let points = thread::scope(|scope| {
        let handles: Vec<_> = (0..threads)
            .map(|_| {
                scope.spawn(|| {
                    let mut seen = 0usize;
                    loop {
                        let next = {
                            let guard = recv.lock().expect("the queue lock was poisoned");
                            guard.try_recv()
                        };
                        let Ok(index) = next else { break };
                        let answer = hfcast::service::run(&requests[index])
                            .unwrap_or_else(|e| panic!("strip {index} failed: {e}"));
                        seen += answer.matches("\"lat\":").count();
                    }
                    seen
                })
            })
            .collect();
        handles
            .into_iter()
            .map(|h| h.join().expect("a worker panicked"))
            .sum()
    });

    (started.elapsed().as_millis(), points)
}

fn main() -> ExitCode {
    let strips: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(16);

    let requests: Vec<String> = (0..strips).map(|i| strip(i, strips)).collect();

    // One pass first, thrown away. The first run of any of this pays for
    // whatever the allocator and the caches have not seen yet, and that
    // cost would land entirely on the one-thread reading and flatter
    // every other one.
    let (_, points) = run_all(&requests[..1], 1);
    println!("{strips} strips, {points} points a strip\n");
    println!("{:>7}  {:>9}  {:>8}", "threads", "wall (ms)", "speedup");

    let mut base = 0u128;
    for threads in [1usize, 2, 4, 8, 16] {
        if threads > strips {
            break;
        }
        let (ms, total) = run_all(&requests, threads);
        if threads == 1 {
            base = ms;
            println!("{threads:>7}  {ms:>9}  {:>8}  ({total} points)", "1.00x");
        } else {
            let speedup = base as f64 / ms as f64;
            println!("{threads:>7}  {ms:>9}  {speedup:>7.2}x");
        }
    }

    println!(
        "\n{} cores available to this process",
        thread::available_parallelism().map_or(0, |n| n.get())
    );
    ExitCode::SUCCESS
}
