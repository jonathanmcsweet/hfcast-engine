//! The whole point-to-point prediction, end to end: what `HFMUFS`
//! drives per hour — geometry, magnetic field, coefficients, layer
//! parameters, MUF, the LUFFY passes with the smoothing blend, `SETLUF`
//! and `OUTBOD`'s sentinels — with the same deck assumptions as the
//! server (method 30, all 24 UTC hours, isotropic antennas, one power).
//!
//! [`listing_text`] renders the result as the method-30 listing body:
//! the same rows, formats and rounding as `OUTBOD`/`FLOLIN`/`FIXLIN`,
//! so `listing::parse_listing` reads it exactly as it reads the
//! Fortran engine's output. That is the whole-engine comparison
//! surface — the tolerance envelope in `docs/sensitivity.md` is defined
//! on these printed cells.

use std::path::Path;

use super::coefficients::{redmap, CoefficientSet, FoF2Model};
use super::con::{MagneticPole, D2R, R};
use super::geometry::path_geometry;
use super::ionogram::{alosfv, fobby, genion, sang, selmod};
use super::ionosphere::{
    alatd, cofion, esind, geotim, ground_constants, layer_parameters, virtim,
};
use super::magnetic::magvar;
use super::modes::{
    es_slots, luffy_freq_loop, luffy_smooth, outbod_sentinels, setlng, setluf, DeckParams, Geog,
    HourSaves, ModeLoopState, PassCtx, Son,
};
use super::muf::{curmuf, ionset, lecden, IonoState};
use super::noise::{anois1, genois};
use super::sigdis::{sigdis, SignalDistribution};

/// Everything a prediction needs; the deck-card equivalents.
#[derive(Debug, Clone)]
pub struct RunInputs {
    pub from_lat_deg: f64,
    pub from_lon_deg: f64,
    pub to_lat_deg: f64,
    pub to_lon_deg: f64,
    /// 1-12.
    pub month: u32,
    pub ssn: R,
    /// Frequencies in MHz, at most 11 (the card slots).
    pub freqs_mhz: Vec<R>,
    pub required_snr_db: R,
    /// Man-made noise at 3 MHz, dB below 1 W (positive).
    pub noise_dbw: i32,
    pub watts: R,
    pub sporadic_e: bool,
}

/// One hour's outputs: the MUF block and the thirteen `/SON/` slots
/// (slot 12 is the at-the-MUF column).
#[derive(Debug, Clone)]
pub struct HourPrediction {
    /// UT hour 1-24.
    pub gmt: R,
    pub allmuf: R,
    pub fot: R,
    pub hpf: R,
    pub angmuf: R,
    pub xluf: R,
    pub frel: [R; 12],
    pub son: [Son; 13],
    /// The `JLONG` flag after the hour: the listing's MODE row uses the
    /// long-path format when the last pass was the long model.
    pub long_model: bool,
}

/// Runs the full prediction for all 24 hours.
pub fn run(itshfbc: &Path, inp: &RunInputs) -> Result<Vec<HourPrediction>, String> {
    let pole = MagneticPole::for_tree(itshfbc);
    let geo = path_geometry(
        inp.from_lat_deg as R,
        inp.from_lon_deg as R,
        inp.to_lat_deg as R,
        inp.to_lon_deg as R,
        false,
        pole,
    );
    let mags: Vec<_> = geo.points.iter().map(|p| magvar(p.lat, p.lon)).collect();
    let set: CoefficientSet =
        redmap(itshfbc, FoF2Model::Ccir, inp.month, inp.ssn).map_err(|e| e.to_string())?;
    let cof = cofion(&set);
    let grounds = ground_constants(&set, &geo.points, &mags);
    let _ = alatd(&geo.points);
    let clats: Vec<R> = geo.points.iter().map(|p| p.lat).collect();
    let glats: Vec<R> = geo.points.iter().map(|p| p.gmlat).collect();
    let psc = [1.0, 1.0, 1.0, if inp.sporadic_e { 1.0 } else { 0.0 }];
    let nang = sang(geo.gcd_km, 0.1);
    let pwrkw = inp.watts / 1000.0;
    let deck = DeckParams {
        amind: 0.1,
        rsn: inp.required_snr_db,
        lufp: 90,
        pmp: 3.0,
        dmp: 0.1,
        pwrdb: 30.0 + 10.0 * pwrkw.log10(),
    };
    let mut base_frel = [0.0 as R; 12];
    for (slot, f) in base_frel.iter_mut().zip(&inp.freqs_mhz) {
        *slot = *f;
    }
    let from_lon_rad = inp.from_lon_deg as R * D2R;
    let to_lon_rad = inp.to_lon_deg as R * D2R;
    let to_lat_rad = inp.to_lat_deg as R * D2R;

    let mut lp = ModeLoopState::default();
    let mut fsecv_carry = [0.0 as R; 3];
    let mut out = Vec::with_capacity(24);
    for jt in 1..=24i32 {
        let gmt = jt as R;
        let ab = virtim(&cof, &set.ikim, gmt);
        let params = layer_parameters(
            &set,
            &ab,
            &geo.points,
            &mags,
            inp.month,
            inp.ssn,
            gmt,
            &psc,
        );
        let es = esind(&set, &ab, &geo.points, &mags, &psc);
        let mut state = IonoState::from_layers(&params);
        state.fsecv = fsecv_carry;
        ionset(&mut state);
        let mut es_state = es.clone();
        let clcks: Vec<R> = params.iter().map(|p| p.clck).collect();
        let hour = curmuf(
            &mut state,
            &mut es_state,
            &set.f2d,
            &clats,
            &clcks,
            geo.gcd,
            geo.gcd_km,
            deck.amind,
            inp.ssn,
        );
        let times = geotim(jt, 1, from_lon_rad, to_lon_rad);
        let an = anois1(&set, times.gmtr, to_lat_rad, to_lon_rad, inp.to_lon_deg as R);
        let fof2_end = state.fi[state.kfx - 1][2];
        let noise_for =
            |f: R| genois(&set, &an, f, to_lat_rad, fof2_end, inp.noise_dbw);

        // The LUFFY passes for MSPEC = 121.
        let jmode = selmod(&state);
        struct PassPlan {
            long: bool,
            areas: Vec<usize>,
        }
        let plans: Vec<PassPlan> = if geo.gcd_km > 10000.0 {
            vec![PassPlan {
                long: true,
                areas: if state.kfx > 1 {
                    vec![0, state.kfx - 1]
                } else {
                    vec![0]
                },
            }]
        } else if geo.gcd_km >= 7000.0 {
            let mut p2 = Vec::new();
            if jmode != 0 {
                p2.push(0);
            }
            if state.kfx > 1 && jmode != state.kfx - 1 {
                p2.push(state.kfx - 1);
            }
            vec![
                PassPlan {
                    long: false,
                    areas: vec![jmode],
                },
                PassPlan {
                    long: true,
                    areas: p2,
                },
            ]
        } else {
            vec![PassPlan {
                long: false,
                areas: vec![jmode],
            }]
        };
        let (mut fs, mut hs) = es_slots(&es_state);
        let mut geog = Geog::from_points(&params, &mags, &grounds);
        let mut hour_m = hour.clone();
        let mut saves = HourSaves::default();
        let mut frel = base_frel;
        frel[11] = hour.allmuf;
        let mut sd_last: Option<SignalDistribution> = None;
        for plan in &plans {
            for &k in &plan.areas {
                lecden(&mut state, k);
                let mut ion = genion(&state, k);
                let table = fobby(&ion, nang);
                alosfv(&state, k, &mut ion, &hour.layers);
                lp.areas[k].update(ion, &table);
            }
            setlng(&mut state, &mut fs, &mut hs, &mut geog, &mut lp.areas);
            sd_last = Some(sigdis(
                &set,
                &state,
                &hour,
                &lp.areas[jmode].ion,
                &glats,
                &clcks,
                jmode,
                geo.gcd_km,
            ));
            let sd = sd_last.as_ref().expect("just set");
            geog.apply_sigdis(sd);
            let ctx = PassCtx {
                state: &state,
                fs: &fs,
                hs: &hs,
                geog: &geog,
                sig: sd,
                deck,
                gcd: geo.gcd,
                gcdkm: geo.gcd_km,
                jmode,
                nang,
                long: plan.long,
            };
            luffy_freq_loop(&mut lp, &ctx, &mut hour_m, &noise_for, &frel, &mut saves);
        }
        if plans.len() == 2 {
            let sd = sd_last.as_ref().expect("two passes ran");
            let ctx = PassCtx {
                state: &state,
                fs: &fs,
                hs: &hs,
                geog: &geog,
                sig: sd,
                deck,
                gcd: geo.gcd,
                gcdkm: geo.gcd_km,
                jmode,
                nang,
                long: true,
            };
            luffy_smooth(&mut lp, &ctx, &noise_for, &frel, &saves);
        }
        let xluf = setluf(&lp.son, &frel, deck.lufp);
        outbod_sentinels(&mut lp.son, hour.allmuf);
        out.push(HourPrediction {
            gmt,
            allmuf: hour.allmuf,
            fot: hour.fot,
            hpf: hour.hpf,
            angmuf: hour.angmuf,
            xluf,
            frel,
            son: lp.son,
            long_model: geo.gcd_km >= 7000.0,
        });
        fsecv_carry = state.fsecv;
    }
    Ok(out)
}

// ---------------------------------------------------------------------
// The listing body, formatted like OUTBOD

/// Fortran `F5.1`: width 5, one decimal, asterisks on overflow.
fn f5_1(v: R) -> String {
    let s = format!("{:5.1}", f64::from(v));
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// Fortran `F5.2`.
fn f5_2(v: R) -> String {
    let s = format!("{:5.2}", f64::from(v));
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// Fortran `ANINT` then `I5`: round half away from zero.
fn i5(v: R) -> String {
    let s = format!("{:5}", v.round() as i64);
    if s.len() > 5 {
        "*****".to_string()
    } else {
        s
    }
}

/// The two-character `LAYTYP` label for a layer index (0 keeps the
/// program-start NUL bytes, 6 is `OUTBOD`'s "NA" sentinel).
fn laytyp(layer: i32) -> &'static str {
    match layer {
        1 => " E",
        2 => "F1",
        3 => "F2",
        4 => "ES",
        5 => " N",
        6 => "NA",
        _ => "\u{0}\u{0}",
    }
}

/// One data row: six spaces, the at-the-MUF field, `jfreq` slot fields,
/// dashes to eleven slots, then the six-character label.
fn row(label: &str, muf_field: String, fields: Vec<String>, jfreq: i32) -> String {
    let mut line = String::from("      ");
    line.push_str(&muf_field);
    for f in &fields {
        line.push_str(f);
    }
    for _ in fields.len()..11 {
        line.push_str("   - "); // 1X + NDASH ('  - ')
    }
    line.push(' ');
    line.push_str(label);
    let _ = jfreq;
    line
}

/// Renders the hours as the method-30 listing body `OUTBOD` prints
/// (the FREQ line and the 21 bottom rows per hour), for comparison via
/// `listing::parse_listing`.
pub fn listing_text(hours: &[HourPrediction]) -> String {
    let mut out = String::new();
    for h in hours {
        // The FREQ line: hour, the MUF, the eleven card frequencies.
        let mut line = format!("  {:4.1}", f64::from(h.gmt));
        line.push_str(&f5_1(h.allmuf));
        for f in &h.frel[..11] {
            line.push_str(&f5_1(*f));
        }
        line.push_str(" FREQ");
        out.push_str(&line);
        out.push('\n');

        // JFREQ: the last slot with a mode, bounded by the last slot
        // with a frequency; nothing prints when no slot has a mode.
        let mut ifreq = 1usize;
        for ifq in 2..=11 {
            if h.frel[ifq - 1] > 0.0 {
                ifreq = ifq;
            }
        }
        let mut jfreq: i32 = -1;
        for ifq in 1..=ifreq {
            if h.son[ifq - 1].nhp > 0 {
                jfreq = ifq as i32;
            }
        }
        let jfreq = jfreq.min(12);
        if jfreq <= 0 {
            continue;
        }
        let slots: Vec<usize> = (0..jfreq as usize).collect();
        let muf = &h.son[11];

        // MODE.
        let mode_field = |s: &Son| {
            if h.long_model {
                format!(" {}{}", laytyp(s.mode_layer), laytyp(s.moder_layer))
            } else {
                format!(" {:2}{}", s.nhp, laytyp(s.mode_layer))
            }
        };
        out.push_str(&row(
            "MODE  ",
            mode_field(muf),
            slots.iter().map(|&i| mode_field(&h.son[i])).collect(),
            jfreq,
        ));
        out.push('\n');

        // The numeric rows, in OUTBOD's order and formats.
        type Field = (
            &'static str,
            fn(&Son) -> String,
        );
        let rows: [Field; 20] = [
            ("TANGLE", |s| f5_1(s.angle)),
            ("DELAY ", |s| f5_1(s.delay)),
            ("V HITE", |s| i5(s.vhigh)),
            ("MUFday", |s| f5_2(s.cprob)),
            ("LOSS  ", |s| i5(s.dblos)),
            ("DBU   ", |s| i5(s.dbu)),
            ("S DBW ", |s| i5(s.dbw)),
            ("N DBW ", |s| i5(s.xnynois + s.rneff)),
            ("SNR   ", |s| i5(s.sndb)),
            ("RPWRG ", |s| i5(s.snpr)),
            ("REL   ", |s| f5_2(s.reliab)),
            ("MPROB ", |s| f5_2(s.probmp)),
            ("S PRB ", |s| f5_2(s.sprob)),
            ("SIG LW", |s| f5_1(s.dblosl)),
            ("SIG UP", |s| f5_1(s.dblosu)),
            ("SNR LW", |s| f5_1(s.snrlw)),
            ("SNR UP", |s| f5_1(s.snrup)),
            ("TGAIN ", |s| f5_1(s.gaint)),
            ("RGAIN ", |s| f5_1(s.gainr)),
            ("SNRxx ", |s| i5(s.snxx)),
        ];
        for (label, field) in rows {
            out.push_str(&row(
                label,
                field(muf),
                slots.iter().map(|&i| field(&h.son[i])).collect(),
                jfreq,
            ));
            out.push('\n');
            // The long path prints its reception angle after TANGLE as
            // RANGLE; the comparison surface ignores it, so it is not
            // rendered here.
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_match_the_fortran_edit_descriptors() {
        assert_eq!(f5_1(12.34), " 12.3");
        assert_eq!(f5_1(-999.0), "*****");
        assert_eq!(f5_2(0.999), " 1.00");
        assert_eq!(i5(-998.6), " -999");
        assert_eq!(i5(2.5), "    3"); // ANINT rounds half away from zero
    }

    #[test]
    fn listing_lines_have_the_parser_geometry() {
        let mut son = [Son::default(); 13];
        son[0].nhp = 1;
        son[0].mode_layer = 3;
        son[0].reliab = 0.87;
        let h = HourPrediction {
            gmt: 1.0,
            allmuf: 12.3,
            fot: 10.0,
            hpf: 14.0,
            angmuf: 5.0,
            xluf: 7.0,
            frel: [7.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0, 12.3],
            son,
            long_model: false,
        };
        let text = listing_text(&[h]);
        let parsed = crate::listing::parse_listing(&text);
        let rel: Vec<_> = parsed
            .numeric
            .iter()
            .filter(|s| s.row == "REL" && s.slot == 0)
            .collect();
        assert_eq!(rel.len(), 1);
        assert!((rel[0].value - 0.87).abs() < 1e-9);
        let muf: Vec<_> = parsed.numeric.iter().filter(|s| s.row == "MUF").collect();
        assert_eq!(muf.len(), 1);
        assert!((muf[0].value - 12.3).abs() < 1e-9);
        let modes: Vec<_> = parsed.modes.iter().filter(|m| m.slot == 0).collect();
        assert_eq!(modes[0].mode, "1F2");
    }
}
