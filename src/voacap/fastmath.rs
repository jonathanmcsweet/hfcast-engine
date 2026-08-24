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

/// Which deviations from the reference's arithmetic a run takes.
///
/// The parity engine has to reproduce the reference down to the last
/// digit. `portcheck` measured what happens otherwise: eleven fields
/// leave the envelope. Truecast is not bound to it, so where the
/// reference made a compromise for 1970s hardware, Truecast may not.
///
/// One flag per deviation rather than one switch for all of them, the
/// shape `Fixes` already uses for the defect fixes. The first two
/// deviations shipped together and were measured together, so their
/// combined cost against measured radio is known and neither one's is.
/// Separate flags are what lets a later deviation be withdrawn on its
/// own when a month of WSPR says it should be, without taking the ones
/// that earned their place with it.
///
/// Carried in the call rather than kept beside it. A caller runs several
/// requests at once and two of them can want different arithmetic, so a
/// shared switch would hand one of them the other's answers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Numerics {
    /// [`cos`] in place of the library's, at the sites the policy
    /// reaches. Accurate to under two units of the last place over the
    /// arguments this engine produces, against the library's half a
    /// unit.
    pub fast_cos: bool,
    /// [`cos`] at the mode loop's remaining cosine sites: `findf`,
    /// `regmod` and `settxr`, which together are the 10.7 percent of
    /// `modes.rs` that `fast_cos` does not cover. Separate from
    /// `fast_cos` because that flag's cost was measured over 68 months
    /// covering `gain_ground` alone, and widening it silently would
    /// leave the measurement describing something that no longer
    /// exists.
    pub fast_cos_modes: bool,
    /// The virtual height integral evaluated in closed form instead of
    /// by the reference's 40-point rule. Both cheaper and closer to the
    /// answer the reference was approximating: 0.0004 percent out
    /// typically against 0.885.
    pub exact_virtual_height: bool,
}

impl Numerics {
    /// The library's, which is what the parity gate is measured against.
    /// Every deviation off.
    pub const fn reference() -> Self {
        Self {
            fast_cos: false,
            fast_cos_modes: false,
            exact_virtual_height: false,
        }
    }

    /// Every deviation on, which is what a truecast run takes unless a
    /// caller asks for something narrower.
    pub const fn truecast() -> Self {
        Self {
            fast_cos: true,
            fast_cos_modes: true,
            exact_virtual_height: true,
        }
    }

    /// Whether this run deviates from the reference at all. A run with
    /// every flag off must print what the reference prints.
    pub const fn is_reference(self) -> bool {
        !self.fast_cos && !self.fast_cos_modes && !self.exact_virtual_height
    }

    /// The deviations Truecast ships with.
    ///
    /// A deviation is taken only where it has been measured to win by
    /// enough to pay for itself. The build host and the 32-bit Android
    /// tablet were both timed one flag at a time, over the world grid,
    /// on one thread.
    ///
    /// The virtual height won on both, 11.9 percent on the tablet and
    /// 11.3 on the host, and is a closer answer to the integral the
    /// reference approximates. It is taken.
    ///
    /// Neither fast cosine is. The one in `modes.rs` is level on both
    /// machines. The one at `gain_ground` is a real 1.4 percent on the
    /// tablet, replicated within the run, but a 7 percent loss on the
    /// host, so taking it means gating on the architecture, and that
    /// buys 1.4 percent on the oldest device class at the price of a
    /// Truecast whose answers depend on where it runs. Nothing gates
    /// that divergence, arm64 could not be measured to place it, and
    /// the virtual height keeps nine tenths of the win without it.
    ///
    /// Both flags stay, because they are how this was measured and how
    /// the next such question will be.
    pub const fn shipping() -> Self {
        Self {
            fast_cos: false,
            fast_cos_modes: false,
            exact_virtual_height: true,
        }
    }

    /// Every deviation's name, in the order the flags are declared. A
    /// caller names them on a command line to score one at a time.
    pub const NAMES: [&'static str; 3] = ["fast-cos", "fast-cos-modes", "exact-height"];

    /// The set a comma-separated list of names asks for. An empty list
    /// is the reference's arithmetic.
    ///
    /// An unrecognised name is the error rather than a name ignored,
    /// because ignoring one would silently score a run against itself
    /// and report no difference, which reads exactly like a deviation
    /// that changes nothing.
    pub fn from_names(list: &str) -> Result<Self, String> {
        list.split(',')
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .try_fold(Self::reference(), |taken, name| match name {
                "fast-cos" => Ok(Self {
                    fast_cos: true,
                    ..taken
                }),
                "fast-cos-modes" => Ok(Self {
                    fast_cos_modes: true,
                    ..taken
                }),
                "exact-height" => Ok(Self {
                    exact_virtual_height: true,
                    ..taken
                }),
                other => Err(other.to_string()),
            })
    }

    /// The names of the deviations this set takes.
    pub fn names(self) -> Vec<&'static str> {
        [
            (self.fast_cos, Self::NAMES[0]),
            (self.fast_cos_modes, Self::NAMES[1]),
            (self.exact_virtual_height, Self::NAMES[2]),
        ]
        .into_iter()
        .filter(|(taken, _)| *taken)
        .map(|(_, name)| name)
        .collect()
    }

    /// `x.cos()` at `gain_ground`, by whichever route this run asked
    /// for.
    #[inline(always)]
    pub fn cos(self, x: R) -> R {
        if self.fast_cos {
            cos(x)
        } else {
            x.cos()
        }
    }

    /// `x.cos()` at the mode loop's other cosine sites.
    #[inline(always)]
    pub fn cos_modes(self, x: R) -> R {
        if self.fast_cos_modes {
            cos(x)
        } else {
            x.cos()
        }
    }
}

impl Default for Numerics {
    /// The reference's, so a caller that says nothing gets parity.
    fn default() -> Self {
        Self::reference()
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

    /// Every flag has a name and every name sets a flag, so a
    /// deviation added without a name cannot reach a command line
    /// silently.
    #[test]
    fn every_deviation_is_nameable() {
        assert_eq!(Numerics::truecast().names(), Numerics::NAMES.to_vec());
        for name in Numerics::NAMES {
            let one = Numerics::from_names(name).expect("a declared name parses");
            assert_eq!(one.names(), vec![name], "{name} set some other flag");
        }
    }

    /// What Truecast ships is the same on every architecture. The
    /// virtual height is taken, neither cosine is, and no `cfg!`
    /// enters into it, so two machines running this engine cannot
    /// disagree about arithmetic.
    #[test]
    fn the_shipping_default_is_the_same_everywhere() {
        let taken = Numerics::shipping();
        assert!(
            taken.exact_virtual_height,
            "the virtual height won on both machines measured"
        );
        assert!(
            !taken.fast_cos && !taken.fast_cos_modes,
            "a cosine deviation would make Truecast depend on its host"
        );
        assert!(!taken.is_reference(), "a truecast run deviates somewhere");
    }

    #[test]
    fn an_empty_list_is_the_reference() {
        assert!(Numerics::from_names("")
            .expect("empty parses")
            .is_reference());
        assert!(Numerics::default().is_reference());
    }

    #[test]
    fn an_unknown_name_is_refused() {
        assert_eq!(
            Numerics::from_names("fast-cos,fast-sin"),
            Err("fast-sin".to_string())
        );
    }

    #[test]
    fn names_round_trip_through_a_list() {
        let want = Numerics {
            fast_cos: true,
            fast_cos_modes: false,
            exact_virtual_height: true,
        };
        assert_eq!(Numerics::from_names(&want.names().join(",")), Ok(want));
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
