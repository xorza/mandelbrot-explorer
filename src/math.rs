use std::ops::{Add, AddAssign, Div, Mul, Sub};

use bytemuck::{Pod, Zeroable};

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct Vec2u32 {
    pub x: u32,
    pub y: u32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
pub struct Vec2i32 {
    pub x: i32,
    pub y: i32,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Pod, Zeroable, Default)]
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


impl Vec2f32 {
    pub fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
    pub fn all(v: f32) -> Self {
        Self { x: v, y: v }
    }
}

impl Vec2f64 {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }
    pub fn all(v: f64) -> Self {
        Self { x: v, y: v }
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


impl Sub for Vec2i32 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self {
            x: self.x - rhs.x,
            y: self.y - rhs.y,
        }
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


impl AddAssign for Vec2f64 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
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
