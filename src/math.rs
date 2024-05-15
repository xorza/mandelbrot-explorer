use bytemuck::{Pod, Zeroable};
use glam::{DVec2, IVec2, UVec2};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct URect {
    pub pos: UVec2,
    pub size: UVec2,
}

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct IRect {
    pub pos: IVec2,
    pub size: IVec2,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct DRect {
    pub pos: DVec2,
    pub size: DVec2,
}

impl URect {
    pub fn from_pos_size(pos: UVec2, size: UVec2) -> Self {
        Self { pos, size }
    }
    pub fn intersects(&self, other: &Self) -> bool {
        self.pos.x < other.pos.x + other.size.x
            && self.pos.x + self.size.x > other.pos.x
            && self.pos.y < other.pos.y + other.size.y
            && self.pos.y + self.size.y > other.pos.y
    }
    pub fn center(&self) -> UVec2 {
        self.pos + self.size / 2
    }
    pub fn upper_right(&self) -> UVec2 {
        self.pos + self.size
    }
}

impl IRect {
    pub fn from_pos_size(pos: IVec2, size: IVec2) -> Self {
        Self { pos, size }
    }
    pub fn intersects(&self, other: &Self) -> bool {
        self.pos.x < other.pos.x + other.size.x
            && self.pos.x + self.size.x > other.pos.x
            && self.pos.y < other.pos.y + other.size.y
            && self.pos.y + self.size.y > other.pos.y
    }
    pub fn center(&self) -> IVec2 {
        self.pos + self.size / 2
    }
}

impl From<URect> for IRect {
    fn from(value: URect) -> Self {
        Self {
            pos: value.pos.try_into().unwrap(),
            size: value.size.try_into().unwrap(),
        }
    }
}

impl DRect {
    pub fn from_pos_size(pos: DVec2, size: DVec2) -> Self {
        Self { pos, size }
    }
    pub fn from_center_size(center: DVec2, size: DVec2) -> Self {
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
    pub fn center(&self) -> DVec2 {
        self.pos + self.size / 2.0
    }
    pub fn upper_right(&self) -> DVec2 {
        self.pos + self.size
    }
}

impl std::fmt::Debug for DRect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "DRect {{ pos: ({:.3}, {:.3}), size: ({:.3}, {:.3}) }}",
            self.pos.x, self.pos.y, self.size.x, self.size.y
        )
    }
}

impl std::fmt::Display for DRect {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "pos: ({:.3}, {:.3}), size: ({:.3}, {:.3})",
            self.pos.x, self.pos.y, self.size.x, self.size.y
        )
    }
}
