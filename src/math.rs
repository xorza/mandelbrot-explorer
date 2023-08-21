use std::mem::size_of;
use std::ops::{Add, AddAssign, Div, Mul, MulAssign, Neg, Sub, SubAssign};

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct Vec2u32 {
    pub x: u32,
    pub y: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct Vec2i32 {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Pod, Zeroable, Default)]
pub struct Vec4u32 {
    pub x: u32,
    pub y: u32,
    pub z: u32,
    pub w: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct Vec4f32 {
    pub x: f32,
    pub y: f32,
    pub z: f32,
    pub w: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct Vec2f32 {
    pub x: f32,
    pub y: f32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct Vec2f64 {
    pub x: f64,
    pub y: f64,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct Mat4x4f32([f32; 16]);

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct RectU32 {
    pub pos: Vec2u32,
    pub size: Vec2u32,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct RectI32 {
    pub pos: Vec2i32,
    pub size: Vec2i32,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct RectF64 {
    pub pos: Vec2f64,
    pub size: Vec2f64,
}


impl Vec2f32 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn all(v: f32) -> Self {
        Self { x: v, y: v }
    }
    pub fn length_squared(self) -> f32 {
        self.x * self.x + self.y * self.y
    }
}

impl Vec2f64 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn all(v: f64) -> Self {
        Self { x: v, y: v }
    }

    pub fn length_squared(self) -> f64 {
        self.x * self.x + self.y * self.y
    }
}

impl Vec4f32 {
    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self { x, y, z, w }
    }
    pub fn all(v: f32) -> Self {
        Self { x: v, y: v, z: v, w: v }
    }
}

impl Vec4u32 {
    pub fn new(x: u32, y: u32, z: u32, w: u32) -> Self {
        Self { x, y, z, w }
    }
    pub fn all(v: u32) -> Self {
        Self { x: v, y: v, z: v, w: v }
    }
}

impl Vec2u32 {
    pub fn new(x: u32, y: u32) -> Self {
        Self { x, y }
    }
    pub fn all(v: u32) -> Self {
        Self { x: v, y: v }
    }
    pub fn length_squared(self) -> u32 {
        self.x * self.x + self.y * self.y
    }
}

impl Vec2i32 {
    pub fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
    pub fn all(v: i32) -> Self {
        Self { x: v, y: v }
    }
    pub fn length_squared(self) -> i32 {
        self.x * self.x + self.y * self.y
    }
}


impl Add for Vec2u32 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
impl Sub for Vec2u32 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}
impl Mul for Vec2u32 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}
impl Div for Vec2u32 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}
impl Div<u32> for Vec2u32 {
    type Output = Vec2u32;

    fn div(self, scalar: u32) -> Self::Output {
        Vec2u32 {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}
impl AddAssign for Vec2u32 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}
impl SubAssign for Vec2u32 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}
impl From<Vec2f32> for Vec2u32 {
    fn from(value: Vec2f32) -> Self {
        Self {
            x: value.x as u32,
            y: value.y as u32,
        }
    }
}
impl From<Vec2f64> for Vec2u32 {
    fn from(value: Vec2f64) -> Self {
        Self {
            x: value.x as u32,
            y: value.y as u32,
        }
    }
}
impl From<Vec2i32> for Vec2u32 {
    fn from(value: Vec2i32) -> Self {
        Self {
            x: value.x as u32,
            y: value.y as u32,
        }
    }
}

impl Neg for Vec2i32 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl Add for Vec2i32 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
impl Sub for Vec2i32 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}
impl Div<i32> for Vec2i32 {
    type Output = Vec2i32;

    fn div(self, scalar: i32) -> Self::Output {
        Vec2i32 {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}
impl AddAssign for Vec2i32 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}
impl SubAssign for Vec2i32 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}
impl From<Vec2u32> for Vec2i32 {
    fn from(value: Vec2u32) -> Self {
        Self {
            x: value.x as i32,
            y: value.y as i32,
        }
    }
}
impl From<Vec2f32> for Vec2i32 {
    fn from(value: Vec2f32) -> Self {
        Self {
            x: value.x as i32,
            y: value.y as i32,
        }
    }
}
impl From<Vec2f64> for Vec2i32 {
    fn from(value: Vec2f64) -> Self {
        Self {
            x: value.x as i32,
            y: value.y as i32,
        }
    }
}


impl AddAssign for Vec2f64 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}
impl SubAssign for Vec2f64 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}
impl MulAssign for Vec2f64 {
    fn mul_assign(&mut self, rhs: Self) {
        self.x *= rhs.x;
        self.y *= rhs.y;
    }
}
impl MulAssign<f64> for Vec2f64 {
    fn mul_assign(&mut self, rhs: f64) {
        self.x *= rhs;
        self.y *= rhs;
    }
}
impl Neg for Vec2f64 {
    type Output = Self;

    fn neg(self) -> Self::Output {
        Self {
            x: -self.x,
            y: -self.y,
        }
    }
}
impl Add for Vec2f64 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
impl Div for Vec2f64 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}
impl Sub for Vec2f64 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}
impl Mul for Vec2f64 {
    type Output = Self;

    fn mul(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x * rhs.x,
            y: self.y * rhs.y,
        }
    }
}
impl Mul<f64> for Vec2f64 {
    type Output = Vec2f64;

    fn mul(self, scalar: f64) -> Self::Output {
        Vec2f64 {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}
impl Div<f64> for Vec2f64 {
    type Output = Vec2f64;

    fn div(self, scalar: f64) -> Self::Output {
        Vec2f64 {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}
impl Sub<f64> for Vec2f64 {
    type Output = Vec2f64;

    fn sub(self, scalar: f64) -> Self::Output {
        Vec2f64 {
            x: self.x - scalar,
            y: self.y - scalar,
        }
    }
}
impl Mul<Vec2f64> for f64 {
    type Output = Vec2f64;

    fn mul(self, vec: Vec2f64) -> Self::Output {
        Vec2f64 {
            x: vec.x * self,
            y: vec.y * self,
        }
    }
}
impl From<Vec2u32> for Vec2f64 {
    fn from(value: Vec2u32) -> Self {
        Self {
            x: value.x as f64,
            y: value.y as f64,
        }
    }
}
impl From<Vec2i32> for Vec2f64 {
    fn from(value: Vec2i32) -> Self {
        Self {
            x: value.x as f64,
            y: value.y as f64,
        }
    }
}
impl From<Vec2f32> for Vec2f64 {
    fn from(value: Vec2f32) -> Self {
        Self {
            x: value.x as f64,
            y: value.y as f64,
        }
    }
}


impl Mul<Vec2f32> for f32 {
    type Output = Vec2f32;

    fn mul(self, vec: Vec2f32) -> Self::Output {
        Vec2f32 {
            x: vec.x * self,
            y: vec.y * self,
        }
    }
}
impl Mul<f32> for Vec2f32 {
    type Output = Vec2f32;

    fn mul(self, scalar: f32) -> Self::Output {
        Vec2f32 {
            x: self.x * scalar,
            y: self.y * scalar,
        }
    }
}
impl Sub<f32> for Vec2f32 {
    type Output = Vec2f32;

    fn sub(self, scalar: f32) -> Self::Output {
        Vec2f32 {
            x: self.x - scalar,
            y: self.y - scalar,
        }
    }
}
impl Div<f32> for Vec2f32 {
    type Output = Vec2f32;

    fn div(self, scalar: f32) -> Self::Output {
        Vec2f32 {
            x: self.x / scalar,
            y: self.y / scalar,
        }
    }
}
impl Div for Vec2f32 {
    type Output = Self;

    fn div(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x / rhs.x,
            y: self.y / rhs.y,
        }
    }
}
impl Sub for Vec2f32 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
    }
}
impl Add for Vec2f32 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x + rhs.x,
            y: self.y + rhs.y,
        }
    }
}
impl From<Vec2i32> for Vec2f32 {
    fn from(value: Vec2i32) -> Self {
        Self {
            x: value.x as f32,
            y: value.y as f32,
        }
    }
}
impl From<Vec2u32> for Vec2f32 {
    fn from(value: Vec2u32) -> Self {
        Self {
            x: value.x as f32,
            y: value.y as f32,
        }
    }
}
impl From<Vec2f64> for Vec2f32 {
    fn from(value: Vec2f64) -> Self {
        Self {
            x: value.x as f32,
            y: value.y as f32,
        }
    }
}


impl Mat4x4f32 {
    pub fn as_bytes(&self) -> &[u8] {
        bytemuck::bytes_of(self)
    }
    pub fn size_in_bytes() -> u32 {
        size_of::<Mat4x4f32>() as u32
    }

    pub fn translate2d(&mut self, vec: Vec2f32) -> &mut Self {
        self.0[12] += vec.x * self.0[0];
        self.0[13] += vec.y * self.0[5];

        self
    }
    pub fn scale(&mut self, factor: Vec2f32) -> &mut Self {
        self.0[0] *= factor.x;
        self.0[5] *= factor.y;

        self
    }
}
impl Default for Mat4x4f32 {
    fn default() -> Self {
        Self([
            // @formatter:off
            1.0, 0.0, 0.0, 0.0, // 1st column
            0.0, 1.0, 0.0, 0.0, // 2nd column
            0.0, 0.0, 1.0, 0.0, // 3rd column
            0.0, 0.0, 0.0, 1.0, // 4th column
            // @formatter:on
        ])
    }
}


impl RectU32 {
    pub fn new(pos: Vec2u32, size: Vec2u32) -> Self {
        Self {
            pos,
            size,
        }
    }
    pub fn intersects(&self, other: &Self) -> bool {
        self.pos.x < other.pos.x + other.size.x
            && self.pos.x + self.size.x > other.pos.x
            && self.pos.y < other.pos.y + other.size.y
            && self.pos.y + self.size.y > other.pos.y
    }
    pub fn center(&self) -> Vec2u32 {
        self.pos + self.size / 2
    }
}


impl RectI32 {
    pub fn new(pos: Vec2i32, size: Vec2i32) -> Self {
        Self {
            pos,
            size,
        }
    }
    pub fn intersects(&self, other: &Self) -> bool {
        self.pos.x < other.pos.x + other.size.x
            && self.pos.x + self.size.x > other.pos.x
            && self.pos.y < other.pos.y + other.size.y
            && self.pos.y + self.size.y > other.pos.y
    }
    pub fn center(&self) -> Vec2i32 {
        self.pos + self.size / 2
    }
}
impl From<RectU32> for RectI32 {
    fn from(value: RectU32) -> Self {
        Self {
            pos: value.pos.into(),
            size: value.size.into(),
        }
    }
}


impl RectF64 {
    pub fn pos_size(pos: Vec2f64, size: Vec2f64) -> Self {
        Self {
            pos,
            size,
        }
    }
    pub fn center_size(center: Vec2f64, size: Vec2f64) -> Self {
        Self {
            pos: center - size / 2.0,
            size,
        }
    }
    pub fn intersects(&self, other: &Self) -> bool {
        self.pos.x < other.pos.x + other.size.x
            && self.pos.x + self.size.x > other.pos.x
            && self.pos.y < other.pos.y + other.size.y
            && self.pos.y + self.size.y > other.pos.y
    }
    pub fn contains(&self, other: &Self) -> bool {
        self.pos.x <= other.pos.x
            && self.pos.x + self.size.x >= other.pos.x + other.size.x
            && self.pos.y <= other.pos.y
            && self.pos.y + self.size.y >= other.pos.y + other.size.y
    }
    pub fn center(&self) -> Vec2f64 {
        self.pos + self.size / 2.0
    }
}


impl std::fmt::Debug for RectF64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "RectF64 {{ pos: ({:.3}, {:.3}), size: ({:.3}, {:.3}) }}",
            self.pos.x, self.pos.y, self.size.x, self.size.y
        )
    }
}
impl std::fmt::Display for RectF64 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pos: ({:.3}, {:.3}), size: ({:.3}, {:.3})",
            self.pos.x, self.pos.y, self.size.x, self.size.y
        )
    }
}