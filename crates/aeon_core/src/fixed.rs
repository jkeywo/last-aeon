//! Fixed-point arithmetic for authoritative quantities.
//!
//! All gameplay-significant numbers — resources, rates, progress, opinion —
//! are integers or [`Milli`] fixed-point values. Integer arithmetic is
//! bit-identical across native and WebAssembly builds, which floating point
//! is not once transcendental functions or aggressive optimisation enter the
//! picture. Floats belong to presentation only.

use core::fmt;
use core::iter::Sum;
use core::ops::{Add, AddAssign, Mul, Neg, Sub, SubAssign};

use serde::{Deserialize, Serialize};

/// A fixed-point quantity in thousandths.
///
/// `Milli::from_milli(1_250)` is 1.25. Arithmetic saturates rather than
/// wrapping: campaign quantities pegging at a bound is recoverable, silent
/// wraparound is not.
#[derive(
    Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default, Serialize, Deserialize,
)]
pub struct Milli(i64);

impl Milli {
    /// Zero.
    pub const ZERO: Milli = Milli(0);
    /// One whole unit.
    pub const ONE: Milli = Milli(1000);

    /// A value from raw thousandths.
    pub fn from_milli(thousandths: i64) -> Self {
        Self(thousandths)
    }

    /// A value from whole units.
    pub fn from_whole(whole: i64) -> Self {
        Self(whole.saturating_mul(1000))
    }

    /// Raw thousandths.
    pub fn milli(self) -> i64 {
        self.0
    }

    /// Whole units, rounded towards negative infinity.
    pub fn whole_floor(self) -> i64 {
        self.0.div_euclid(1000)
    }

    /// Multiplies by `num / den` exactly, rounding towards negative infinity.
    ///
    /// # Panics
    /// Panics if `den` is zero.
    pub fn mul_div(self, num: i64, den: i64) -> Self {
        assert!(den != 0, "mul_div denominator must be non-zero");
        let scaled = i128::from(self.0) * i128::from(num);
        let result = scaled.div_euclid(i128::from(den));
        Self(clamp_i128(result))
    }

    /// Scales by a permille factor: `scale_permille(250)` is 25%.
    pub fn scale_permille(self, permille: i64) -> Self {
        self.mul_div(permille, 1000)
    }

    /// This value clamped to be at least zero.
    pub fn max_zero(self) -> Self {
        Self(self.0.max(0))
    }
}

fn clamp_i128(value: i128) -> i64 {
    if value > i128::from(i64::MAX) {
        i64::MAX
    } else if value < i128::from(i64::MIN) {
        i64::MIN
    } else {
        value as i64
    }
}

impl Add for Milli {
    type Output = Milli;
    fn add(self, rhs: Milli) -> Milli {
        Milli(self.0.saturating_add(rhs.0))
    }
}

impl AddAssign for Milli {
    fn add_assign(&mut self, rhs: Milli) {
        *self = *self + rhs;
    }
}

impl Sub for Milli {
    type Output = Milli;
    fn sub(self, rhs: Milli) -> Milli {
        Milli(self.0.saturating_sub(rhs.0))
    }
}

impl SubAssign for Milli {
    fn sub_assign(&mut self, rhs: Milli) {
        *self = *self - rhs;
    }
}

impl Neg for Milli {
    type Output = Milli;
    fn neg(self) -> Milli {
        Milli(self.0.saturating_neg())
    }
}

impl Mul<i64> for Milli {
    type Output = Milli;
    fn mul(self, rhs: i64) -> Milli {
        Milli(self.0.saturating_mul(rhs))
    }
}

impl Sum for Milli {
    fn sum<I: Iterator<Item = Milli>>(iter: I) -> Milli {
        iter.fold(Milli::ZERO, Add::add)
    }
}

impl fmt::Display for Milli {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { "-" } else { "" };
        let magnitude = self.0.unsigned_abs();
        write!(f, "{sign}{}.{:03}", magnitude / 1000, magnitude % 1000)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn construction_and_accessors() {
        assert_eq!(Milli::from_whole(3).milli(), 3000);
        assert_eq!(Milli::from_milli(1250).whole_floor(), 1);
        assert_eq!(Milli::from_milli(-1250).whole_floor(), -2);
    }

    #[test]
    fn arithmetic_saturates() {
        let max = Milli::from_milli(i64::MAX);
        assert_eq!(max + Milli::ONE, max);
        let min = Milli::from_milli(i64::MIN);
        assert_eq!(min - Milli::ONE, min);
    }

    #[test]
    fn mul_div_is_exact_and_floors() {
        let value = Milli::from_whole(10);
        assert_eq!(value.mul_div(1, 3).milli(), 3333);
        assert_eq!(Milli::from_milli(-10_000).mul_div(1, 3).milli(), -3334);
        assert_eq!(value.scale_permille(250), Milli::from_milli(2500));
    }

    #[test]
    fn display_formats_thousandths() {
        assert_eq!(Milli::from_milli(1250).to_string(), "1.250");
        assert_eq!(Milli::from_milli(-50).to_string(), "-0.050");
        assert_eq!(Milli::ZERO.to_string(), "0.000");
    }

    #[test]
    fn sum_and_max_zero() {
        let total: Milli = [Milli::ONE, Milli::from_milli(500)].into_iter().sum();
        assert_eq!(total.milli(), 1500);
        assert_eq!(Milli::from_milli(-5).max_zero(), Milli::ZERO);
    }
}
