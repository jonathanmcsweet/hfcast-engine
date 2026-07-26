//! Small statistics helpers shared by the measurement binaries.
//!
//! Medians are used ahead of means throughout, because every dataset here has
//! heavy tails: one hour of a magnetic disturbance or one misreporting station
//! should not move the summary.

/// Median of a slice. Sorts in place; an empty slice reads as zero.
pub fn median(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let mid = values.len() / 2;
    if values.len().is_multiple_of(2) {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    }
}

/// The value at fraction `q` of the data, so `q = 0.1` is the lower decile.
///
/// Sorts in place. Uses the same nearest-rank convention as the rest of the
/// crate; an empty slice reads as zero.
pub fn quantile(values: &mut [f64], q: f64) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).expect("no NaN"));
    let rank = (q * values.len() as f64).ceil() as usize;
    values[rank.saturating_sub(1).min(values.len() - 1)]
}

/// Root mean square.
pub fn rms(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    (values.iter().map(|v| v * v).sum::<f64>() / values.len() as f64).sqrt()
}

/// Pearson correlation, or `None` if either side does not vary.
pub fn correlation(a: &[f64], b: &[f64]) -> Option<f64> {
    if a.len() != b.len() || a.len() < 3 {
        return None;
    }
    let n = a.len() as f64;
    let mean_a = a.iter().sum::<f64>() / n;
    let mean_b = b.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut da = 0.0;
    let mut db = 0.0;
    for (x, y) in a.iter().zip(b) {
        num += (x - mean_a) * (y - mean_b);
        da += (x - mean_a).powi(2);
        db += (y - mean_b).powi(2);
    }
    if da <= 0.0 || db <= 0.0 {
        return None;
    }
    Some(num / (da * db).sqrt())
}

/// Standard normal cumulative distribution, via Abramowitz-Stegun 7.1.26.
///
/// Accurate to about 1e-7, far below anything these measurements resolve.
pub fn phi(z: f64) -> f64 {
    let x = z.abs() / std::f64::consts::SQRT_2;
    let t = 1.0 / (1.0 + 0.3275911 * x);
    let poly = t
        * (0.254829592
            + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))));
    let erf = 1.0 - poly * (-x * x).exp();
    if z >= 0.0 {
        0.5 * (1.0 + erf)
    } else {
        0.5 * (1.0 - erf)
    }
}

/// Least-squares fit of `observed = intercept + slope * predicted`.
///
/// The slope matters as much as the fit. A model can put the peaks and troughs
/// in the right places and still swing too hard between them; correlation
/// cannot see that, because it ignores scale, but the slope shows it directly.
pub fn fit_line(observed: &[f64], predicted: &[f64]) -> Option<(f64, f64)> {
    if observed.len() != predicted.len() || observed.len() < 3 {
        return None;
    }
    let n = observed.len() as f64;
    let mean_p = predicted.iter().sum::<f64>() / n;
    let mean_o = observed.iter().sum::<f64>() / n;
    let mut num = 0.0;
    let mut den = 0.0;
    for (p, o) in predicted.iter().zip(observed) {
        num += (p - mean_p) * (o - mean_o);
        den += (p - mean_p).powi(2);
    }
    if den <= 0.0 {
        return None;
    }
    let slope = num / den;
    Some((mean_o - slope * mean_p, slope))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn median_of_odd_and_even_lengths() {
        assert_eq!(median(&mut [3.0, 1.0, 2.0]), 2.0);
        assert_eq!(median(&mut [4.0, 1.0, 2.0, 3.0]), 2.5);
        assert_eq!(median(&mut []), 0.0);
    }

    #[test]
    fn median_resists_one_wild_value() {
        assert_eq!(median(&mut [1.0, 2.0, 3.0, 4.0, 1000.0]), 3.0);
    }

    #[test]
    fn quantile_finds_the_deciles() {
        let mut values: Vec<f64> = (1..=10).map(|n| n as f64).collect();
        assert_eq!(quantile(&mut values, 0.1), 1.0);
        assert_eq!(quantile(&mut values, 0.9), 9.0);
        assert_eq!(quantile(&mut values, 0.5), 5.0);
    }

    #[test]
    fn rms_of_a_constant_is_that_constant() {
        assert!((rms(&[2.0, 2.0, 2.0]) - 2.0).abs() < 1e-12);
    }

    #[test]
    fn correlation_sees_a_perfect_line_regardless_of_scale() {
        let a = [1.0, 2.0, 3.0, 4.0];
        let b = [10.0, 20.0, 30.0, 40.0];
        assert!((correlation(&a, &b).expect("varies") - 1.0).abs() < 1e-12);
    }

    #[test]
    fn correlation_refuses_a_constant_side() {
        assert_eq!(correlation(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]), None);
    }

    #[test]
    fn phi_hits_the_anchors_that_matter() {
        // The median day, and the definition of a decile.
        assert!((phi(0.0) - 0.5).abs() < 1e-9);
        assert!((phi(-1.2816) - 0.1).abs() < 1e-3);
        assert!((phi(1.2816) - 0.9).abs() < 1e-3);
    }

    #[test]
    fn fit_line_recovers_a_known_slope() {
        let predicted = [0.0, 10.0, 20.0, 30.0];
        let observed: Vec<f64> = predicted.iter().map(|p| 5.0 + 0.5 * p).collect();
        let (a, b) = fit_line(&observed, &predicted).expect("fits");
        assert!((a - 5.0).abs() < 1e-12);
        assert!((b - 0.5).abs() < 1e-12);
    }
}
