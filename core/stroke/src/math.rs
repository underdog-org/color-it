//! 最小 2D 向量。
//!
//! 不引 glam 之類的數學 crate：整個 crate 只用到 add／sub／scale／lerp／length 五個運算，
//! 而 golden test 比的是浮點輸出——相依愈少，「換一版相依就整批 fixture 變紅」的機會愈小。

use std::ops::{Add, Mul, Sub};

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Vec2 {
    pub x: f32,
    pub y: f32,
}

impl Vec2 {
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };

    pub const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }

    pub fn length(self) -> f32 {
        self.x.hypot(self.y)
    }

    pub fn distance(self, other: Self) -> f32 {
        (other - self).length()
    }

    /// `t` 不做 clamp——弧長取樣只會餵 `[0, 1]`，而外插在這裡是呼叫端的 bug，
    /// 靜默夾住反而讓它更難被看見。
    pub fn lerp(self, other: Self, t: f32) -> Self {
        self + (other - self) * t
    }
}

impl Add for Vec2 {
    type Output = Self;
    fn add(self, rhs: Self) -> Self {
        Self::new(self.x + rhs.x, self.y + rhs.y)
    }
}

impl Sub for Vec2 {
    type Output = Self;
    fn sub(self, rhs: Self) -> Self {
        Self::new(self.x - rhs.x, self.y - rhs.y)
    }
}

impl Mul<f32> for Vec2 {
    type Output = Self;
    fn mul(self, rhs: f32) -> Self {
        Self::new(self.x * rhs, self.y * rhs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lerp_hits_both_ends() {
        let (a, b) = (Vec2::new(1.0, 2.0), Vec2::new(5.0, 10.0));
        assert_eq!(a.lerp(b, 0.0), a);
        assert_eq!(a.lerp(b, 1.0), b);
        assert_eq!(a.lerp(b, 0.5), Vec2::new(3.0, 6.0));
    }

    #[test]
    fn distance_is_euclidean() {
        assert_eq!(Vec2::ZERO.distance(Vec2::new(3.0, 4.0)), 5.0);
    }
}
