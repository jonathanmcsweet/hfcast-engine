//! HFB1: the packed grid answer.
//!
//! The application's fine globe crosses the JNI boundary today as about
//! 2.2 MB of JSON that becomes 34,560 JavaScript objects. The planes in
//! `GridPlanes` are already bare `f32`; this format writes them as they
//! are, so the crossing is one byte array and the JavaScript side can
//! view each plane as a `Float32Array` without parsing anything.
//!
//! Layout, all little-endian:
//!
//! | offset | field |
//! | --- | --- |
//! | 0 | magic `HFB1` |
//! | 4 | version `u16` (this layout is 1) |
//! | 6 | reserved `u16`, zero |
//! | 8 | `nx`, `ny`, `n_freqs`, `n_planes` as `u32` |
//! | 24 | lattice edges `xmin`, `xmax`, `ymin`, `ymax` as `f32` |
//! | 40 | reserved, zero |
//! | 48 | the frequencies, MHz, `n_freqs` x `f32` |
//! | 48 + 4 x `n_freqs` | the planes |
//!
//! Version 1 writes three planes in fixed order — reliability, median
//! SNR in dB, takeoff angle in degrees (NaN where the run produced no
//! angle) — each `n_freqs` x `ny` x `nx` values in `GridPlanes`' own
//! cell order. Point coordinates are not stored: the lattice edges and
//! counts in the header derive them, and two coordinate planes would
//! double a one-band globe. The header is 48 bytes and every plane
//! starts 4-byte aligned, so a zero-copy float view is always legal.

use crate::truecast::grid::{GridPlanes, GridRequest};

pub const MAGIC: [u8; 4] = *b"HFB1";
pub const VERSION: u16 = 1;
pub const HEADER_BYTES: usize = 48;
/// Reliability, SNR, takeoff.
pub const N_PLANES: usize = 3;

/// A decoded packed grid: the header fields and the planes, in the
/// same order and units the encoder took them.
#[derive(Debug, Clone, PartialEq)]
pub struct PackedGrid {
    pub nx: usize,
    pub ny: usize,
    pub n_freqs: usize,
    pub xmin: f32,
    pub xmax: f32,
    pub ymin: f32,
    pub ymax: f32,
    pub freqs_mhz: Vec<f32>,
    pub reliability: Vec<f32>,
    pub snr_db: Vec<f32>,
    pub takeoff_deg: Vec<f32>,
}

/// Encodes one grid answer against the request that produced it.
pub fn encode(req: &GridRequest, planes: &GridPlanes) -> Vec<u8> {
    let grid = &req.area.grid;
    let n_cells = planes.n_freqs * planes.ny * planes.nx;
    let mut out = Vec::with_capacity(HEADER_BYTES + 4 * planes.n_freqs + 4 * N_PLANES * n_cells);
    out.extend_from_slice(&MAGIC);
    out.extend_from_slice(&VERSION.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    // A loop over the four counts would hide which is which.
    out.extend_from_slice(&(planes.nx as u32).to_le_bytes());
    out.extend_from_slice(&(planes.ny as u32).to_le_bytes());
    out.extend_from_slice(&(planes.n_freqs as u32).to_le_bytes());
    out.extend_from_slice(&(N_PLANES as u32).to_le_bytes());
    for edge in [grid.xmin, grid.xmax, grid.ymin, grid.ymax] {
        out.extend_from_slice(&edge.to_le_bytes());
    }
    out.resize(HEADER_BYTES, 0);
    for f in req.area.freqs_mhz.iter().take(planes.n_freqs) {
        out.extend_from_slice(&f.to_le_bytes());
    }
    for plane in [&planes.reliability, &planes.snr_db, &planes.takeoff_deg] {
        for value in plane {
            out.extend_from_slice(&value.to_le_bytes());
        }
    }
    out
}

/// Decodes a packed grid, refusing anything that is not exactly a
/// version-1 HFB1 body of the advertised size.
pub fn decode(bytes: &[u8]) -> Result<PackedGrid, String> {
    let header = bytes
        .get(..HEADER_BYTES)
        .ok_or("shorter than an HFB1 header")?;
    if header[..4] != MAGIC {
        return Err("not an HFB1 body".to_string());
    }
    let u16_at = |at: usize| u16::from_le_bytes([header[at], header[at + 1]]);
    let u32_at = |at: usize| {
        u32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]]) as usize
    };
    let f32_at = |at: usize| {
        f32::from_le_bytes([header[at], header[at + 1], header[at + 2], header[at + 3]])
    };
    if u16_at(4) != VERSION {
        return Err(format!("HFB1 version {} is not {VERSION}", u16_at(4)));
    }
    let (nx, ny, n_freqs, n_planes) = (u32_at(8), u32_at(12), u32_at(16), u32_at(20));
    if n_planes != N_PLANES {
        return Err(format!("{n_planes} planes is not {N_PLANES}"));
    }
    let n_cells = n_freqs
        .checked_mul(ny)
        .and_then(|a| a.checked_mul(nx))
        .ok_or("cell count overflows")?;
    let expected = HEADER_BYTES + 4 * n_freqs + 4 * N_PLANES * n_cells;
    if bytes.len() != expected {
        return Err(format!(
            "{} bytes where {expected} were advertised",
            bytes.len()
        ));
    }
    let floats = |at: usize, n: usize| -> Vec<f32> {
        bytes[at..at + 4 * n]
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect()
    };
    let planes_at = HEADER_BYTES + 4 * n_freqs;
    Ok(PackedGrid {
        nx,
        ny,
        n_freqs,
        xmin: f32_at(24),
        xmax: f32_at(28),
        ymin: f32_at(32),
        ymax: f32_at(36),
        freqs_mhz: floats(HEADER_BYTES, n_freqs),
        reliability: floats(planes_at, n_cells),
        snr_db: floats(planes_at + 4 * n_cells, n_cells),
        takeoff_deg: floats(planes_at + 8 * n_cells, n_cells),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voacap::area::{Grid, Projection};
    use crate::voacap::coefficients::FoF2Model;
    use crate::voacap::model::Model;
    use crate::voacap::run::AreaInputs;

    fn request(nx: usize, ny: usize, freqs: Vec<f32>) -> GridRequest {
        GridRequest {
            area: AreaInputs {
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
                freqs_mhz: freqs,
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
            },
            threads: 1,
        }
    }

    /// Distinct values everywhere, one NaN, so a byte swapped anywhere
    /// changes the decode.
    fn planes(nx: usize, ny: usize, nf: usize) -> GridPlanes {
        let n = nf * ny * nx;
        let seq = |base: f32| (0..n).map(|i| base + i as f32).collect::<Vec<f32>>();
        let mut takeoff = seq(300.0);
        takeoff[0] = f32::NAN;
        GridPlanes {
            nx,
            ny,
            n_freqs: nf,
            lat_deg: vec![0.0; ny * nx],
            lon_deg: vec![0.0; ny * nx],
            reliability: seq(0.0),
            snr_db: seq(100.0),
            takeoff_deg: takeoff,
        }
    }

    #[test]
    fn the_round_trip_is_exact_including_nan() {
        let req = request(4, 3, vec![7.1, 14.1]);
        let grid = planes(4, 3, 2);
        let bytes = encode(&req, &grid);
        let back = decode(&bytes).expect("a fresh encode decodes");
        assert_eq!(back.nx, 4);
        assert_eq!(back.ny, 3);
        assert_eq!(back.freqs_mhz, vec![7.1f32, 14.1]);
        assert_eq!((back.xmin, back.ymax), (-180.0, 90.0));
        assert_eq!(back.reliability, grid.reliability);
        assert_eq!(back.snr_db, grid.snr_db);
        assert!(back.takeoff_deg[0].is_nan());
        assert_eq!(back.takeoff_deg[1..], grid.takeoff_deg[1..]);
    }

    #[test]
    fn the_size_is_the_header_plus_the_planes() {
        // A one-band fine globe: 48 + 4 + 3 x 34560 x 4 bytes, about
        // 405 KB against the 2.2 MB JSON crossing.
        let req = request(240, 144, vec![7.1]);
        let bytes = encode(&req, &planes(240, 144, 1));
        assert_eq!(bytes.len(), HEADER_BYTES + 4 + 3 * 240 * 144 * 4);
        // Every plane starts 4-byte aligned for zero-copy float views.
        assert!((HEADER_BYTES + 4).is_multiple_of(4));
    }

    #[test]
    fn damaged_bodies_are_refused() {
        let req = request(4, 3, vec![7.1]);
        let bytes = encode(&req, &planes(4, 3, 1));
        assert!(decode(&bytes[..20]).is_err());
        assert!(decode(&bytes[..bytes.len() - 4]).is_err());
        let mut wrong_magic = bytes.clone();
        wrong_magic[0] = b'X';
        assert!(decode(&wrong_magic).is_err());
        let mut wrong_version = bytes;
        wrong_version[4] = 9;
        assert!(decode(&wrong_version).is_err());
    }
}
