//! `cos` without the library's general implementation.
//!
//! An area run makes 43.9 million cosine calls, and on a 32-bit ARM
//! tablet the library's costs 35.9 ns against 2.4 ns on a desktop build
//! host. That is 8.4 percent of a world grid on the device and about one
//! percent on the machine the engine is usually measured on, which is
//! why it went unnoticed.
//!
//! The arguments run from -3.3 to 16 pi, measured over a world grid, so
//! the reduction never has to be careful about huge inputs: splitting pi
//! into three pieces keeps `n * pi` exact for every `n` reachable here.
//!
//! Everything is `f32` and there is no table. Both are deliberate. The
//! same arithmetic on a build host and on the device gives the same
//! bits, so `portcheck` still measures what ships, and nothing here
//! competes for the data cache the hot loops are using.
//!
//! Against an exact reference over the engine's range: 1.53 units of the
//! last place at worst, 0.21 typically, where the library call is 0.56
//! and 0.11.
use crate::voacap::con::R;

/// `cos(r)` for `|r| <= pi/2`, in rising powers of `r * r`.
const COS: [R; 6] = [
    1.0,
    -0.5,
    0.041666634,
    -0.0013888361,
    2.4760135e-05,
    -2.6051077e-07,
];

/// `1 / pi`.
const INV_PI: R = 0.31830987;

/// Pi in three pieces, each with enough trailing zeros that `n * piece`
/// is exact for the `|n| <= 16` this engine reaches. Subtracted in this
/// order, largest first, so the cancellation happens once.
const PI: [R; 3] = [3.140625, 0.00096702576, 6.277114e-07];

/// `1.5 * 2^23`: adding it forces a float to its nearest integer in the
/// low mantissa bits, a rounding the hardware does without a library
/// call. `f32::round` would be a call, and rounds halves the other way.
const MAGIC: R = 12_582_912.0;

/// `x.cos()`, for the arguments this engine produces.
///
/// Accurate to under two units of the last place for `|x| <= 1000`,
/// which is twenty times the largest a world grid produces. Past that
/// the three pieces of pi stop covering the cancellation and it decays:
/// 4 units by 10,000 and useless by a million. Nothing here reaches it,
/// and the assertion says so where a test would catch it.
pub fn cos(x: R) -> R {
    debug_assert!(x.abs() <= 1000.0, "cos_fast is not reduced for {x}");
    let t = x * INV_PI + MAGIC;
    // The integer landed in the low mantissa bits. Its parity is the
    // half-turn count, which is the sign of the answer.
    let odd = t.to_bits() & 1 == 1;
    let n = t - MAGIC;
    // Pi in pieces rather than whole, so each cancellation happens
    // against a piece that `n` multiplies exactly.
    let r = PI.iter().fold(x, |r, &piece| r - n * piece);
    let u = r * r;
    let p = COS.iter().rev().fold(0.0, |acc, &c| acc * u + c);
    if odd {
        -p
    } else {
        p
    }
}

/// Which engine's numerics a run uses.
///
/// The parity engine has to reproduce the reference down to the last
/// digit. `portcheck` measured what happens otherwise: eleven fields
/// leave the envelope. Truecast is not bound to it, so where the
/// reference made a compromise for 1970s hardware, Truecast may not.
///
/// Carried in the call rather than kept beside it. A caller runs several
/// requests at once and two of them can want different arithmetic, so a
/// shared switch would hand one of them the other's answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Numerics {
    /// The library's, which is what the parity gate is measured against.
    #[default]
    Reference,
    /// Truecast's, which is free to differ from the reference where
    /// differing is better. Some of it trades a little accuracy for
    /// speed, and some of it is both faster and more accurate.
    Truecast,
}

impl Numerics {
    /// `x.cos()`, by whichever route this run asked for.
    #[inline(always)]
    pub fn cos(self, x: R) -> R {
        match self {
            Self::Reference => x.cos(),
            Self::Truecast => cos(x),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The spread a world grid produces, held to two units of the last
    /// place of the exact answer.
    #[test]
    fn tracks_cos_over_the_engine_range() {
        // A sweep in value rather than in bit pattern: stepping the bits
        // of a negative float walks away from zero, which leaves the
        // range this is built for.
        const N: usize = 4_000_000;
        let (lo, hi) = (-3.5f64, 16.0 * std::f64::consts::PI);
        let (worst, at) = (0..=N)
            .map(|i| (lo + (hi - lo) * i as f64 / N as f64) as f32)
            .map(|x| {
                let off = ((cos(x) as f64) - (x as f64).cos()).abs() / f32::EPSILON as f64;
                (off, x)
            })
            .fold((0.0f64, 0.0f32), |a, b| if b.0 > a.0 { b } else { a });
        assert!(worst < 2.0, "{worst} ulp at {at}");
    }

    #[test]
    fn holds_at_the_landmarks() {
        let off = [
            (0.0f32, 1.0f32),
            (std::f32::consts::FRAC_PI_2, 0.0),
            (std::f32::consts::PI, -1.0),
            (-std::f32::consts::PI, -1.0),
        ]
        .into_iter()
        .find(|&(x, want)| (cos(x) - want).abs() >= 1e-6);
        assert_eq!(off, None, "landmark off by more than 1e-6");
    }

    /// Even, the way the function it replaces is.
    #[test]
    fn is_even() {
        let odd_one = std::iter::successors(Some(0.01f32), |x| Some(x * 1.0009))
            .take_while(|&x| x < 50.0)
            .find(|&x| cos(x) != cos(-x));
        assert_eq!(odd_one, None, "cos disagreed with its mirror");
    }
}
