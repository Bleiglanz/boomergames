//! Deterministic floating-point primitives backed exclusively by `libm`.
//!
//! All transcendental operations route through `libm` so native (`x86_64`) and
//! browser (`wasm32-unknown-unknown`) targets produce bit-identical results.

use core::ops::{Add, Sub};

/// Normalizes `-0.0` to `+0.0` so bit-level hashes and JSON outputs are canonical.
#[must_use]
pub fn canonical_zero(value: f64) -> f64 {
    if value == 0.0 { 0.0 } else { value }
}

/// Deterministic absolute value.
#[must_use]
pub fn fabs(value: f64) -> f64 {
    canonical_zero(libm::fabs(value))
}

/// Deterministic square root via `libm`.
#[must_use]
pub fn sqrt(value: f64) -> f64 {
    canonical_zero(libm::sqrt(value))
}

/// Deterministic sine via `libm`.
#[must_use]
pub fn sin(radians: f64) -> f64 {
    canonical_zero(libm::sin(radians))
}

/// Deterministic cosine via `libm`.
#[must_use]
pub fn cos(radians: f64) -> f64 {
    canonical_zero(libm::cos(radians))
}

/// Deterministic tangent via `libm`.
#[must_use]
pub fn tan(radians: f64) -> f64 {
    canonical_zero(libm::tan(radians))
}

/// Deterministic two-argument arctangent via `libm`.
#[must_use]
pub fn atan2(y: f64, x: f64) -> f64 {
    canonical_zero(libm::atan2(y, x))
}

/// Deterministic Euclidean hypotenuse via `libm`.
#[must_use]
pub fn hypot(x: f64, y: f64) -> f64 {
    canonical_zero(libm::hypot(x, y))
}

/// Deterministic exponentiation via `libm`.
#[must_use]
pub fn pow(base: f64, exp: f64) -> f64 {
    canonical_zero(libm::pow(base, exp))
}

/// Clamps `value` to the inclusive range `[min, max]`.
#[must_use]
pub fn clamp(value: f64, min: f64, max: f64) -> f64 {
    if value < min {
        min
    } else if value > max {
        max
    } else {
        canonical_zero(value)
    }
}

/// Deterministic 2D vector in metres or normalized coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vec2 {
    /// Horizontal coordinate.
    pub x: f64,
    /// Vertical coordinate.
    pub y: f64,
}

impl Vec2 {
    /// Zero vector `(0.0, 0.0)`.
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    /// Creates a new vector with canonicalized zero components.
    #[must_use]
    pub fn new(x: f64, y: f64) -> Self {
        Self {
            x: canonical_zero(x),
            y: canonical_zero(y),
        }
    }

    /// Scalar multiplication.
    #[must_use]
    pub fn scale(self, factor: f64) -> Self {
        Self::new(self.x * factor, self.y * factor)
    }

    /// Dot product of two vectors.
    #[must_use]
    pub fn dot(self, rhs: Self) -> f64 {
        canonical_zero((self.x * rhs.x) + (self.y * rhs.y))
    }

    /// Squared Euclidean length.
    #[must_use]
    pub fn len_sq(self) -> f64 {
        self.dot(self)
    }

    /// Euclidean length using `libm::sqrt`.
    #[must_use]
    pub fn len(self) -> f64 {
        sqrt(self.len_sq())
    }

    /// Euclidean distance between two points.
    #[must_use]
    pub fn dist(self, other: Self) -> f64 {
        (self - other).len()
    }

    /// Returns a unit vector in the same direction, or `None` if length is below `epsilon`.
    #[must_use]
    pub fn normalized(self, epsilon: f64) -> Option<Self> {
        let length = self.len();
        if length <= epsilon {
            None
        } else {
            Some(self.scale(1.0 / length))
        }
    }
}

impl Add for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canonical_zero_removes_negative_zero() {
        let neg_zero = -0.0_f64;
        assert_eq!(neg_zero.to_bits(), 0x8000_0000_0000_0000);
        assert_eq!(canonical_zero(neg_zero).to_bits(), 0.0_f64.to_bits());
    }

    #[test]
    fn vec2_basic_operations_are_exact() {
        let a = Vec2::new(3.0, 4.0);
        assert!((a.len() - 5.0).abs() < 1e-12);
        let unit = a.normalized(1e-12).unwrap();
        assert!((unit.x - 0.6).abs() < 1e-12);
        assert!((unit.y - 0.8).abs() < 1e-12);
        assert!(Vec2::ZERO.normalized(1e-12).is_none());
    }
}
