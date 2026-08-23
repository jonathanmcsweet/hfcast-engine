//! Times the daily-middles lattice at each worker count, and checks the
//! answer does not move between them.
//!
//! The middles are the lattice the map correction reads: every point run
//! for all 24 hours, then the middle of each day kept. Points carry
//! nothing to each other, so rows split across workers, and the check
//! here is that splitting them changes no value.
use hfcast::voacap::area::{Grid, Projection};
use hfcast::voacap::coefficients::FoF2Model;
use hfcast::voacap::model::Model;
use hfcast::voacap::run::run_area_daily_median;
use hfcast::voacap::run::AreaInputs;
use std::time::Instant;

fn main() -> Result<(), String> {
    let root = std::path::PathBuf::from(
        std::env::var("HFCAST_ITSHFBC").unwrap_or_else(|_| "<embedded>".into()),
    );
    // The app's fine correction lattice: 5 by 7.5 degrees, whole world.
    let (nx, ny) = (48, 36);
    let area = AreaInputs {
        arith: Default::default(),
        grid: Grid {
            projection: Projection::LatLon,
            plat: 47.0,
            plon: 8.0,
            xmin: -180.0,
            xmax: 180.0,
            ymin: -90.0,
            ymax: 90.0,
            nx,
            ny,
        },
        tx_lat_deg: 47.0,
        tx_lon_deg: 8.0,
        month: 6,
        ssn: 80.0,
        hour: 13,
        freqs_mhz: vec![7.1],
        required_snr_db: 24.0,
        noise_dbw: 145,
        watts: 100.0,
        psc: [1.0, 1.0, 1.0, 0.0],
        method: 30,
        fof2: FoF2Model::Ccir,
        inverse: false,
        tx_antenna: None,
        rx_antenna: None,
        model: Model::Compatible,
    };
    println!("daily middles, {nx} x {ny} = {} points, 24 hours", nx * ny);
    let mut first: Option<Vec<f64>> = None;
    for threads in [1usize, 2, 4, 8, 0] {
        let t = Instant::now();
        let out = run_area_daily_median(&root, &area, threads)?;
        let ms = t.elapsed().as_millis();
        let flat: Vec<f64> = out.iter().flat_map(|m| m.median_snr_db.clone()).collect();
        if threads == 1 {
            let dump: Vec<String> = flat.iter().map(|v| format!("{v:.9}")).collect();
            std::fs::write(
                std::env::var("DUMP").unwrap_or_else(|_| "/dev/null".into()),
                dump.join("\n"),
            )
            .ok();
        }
        let label = if threads == 0 {
            "all".into()
        } else {
            threads.to_string()
        };
        let same = match &first {
            None => {
                first = Some(flat);
                "baseline".to_string()
            }
            Some(f) => {
                if *f == flat {
                    "identical".into()
                } else {
                    format!(
                        "{} of {} DIFFER",
                        f.iter().zip(&flat).filter(|(a, b)| a != b).count(),
                        f.len()
                    )
                }
            }
        };
        println!("  {label:>3} thread(s): {ms:>6} ms   {same}");
    }
    Ok(())
}
