use bytemuck::{Pod, Zeroable};
use glam::{DVec2, UVec2};

#[repr(C)]
#[derive(Clone, Copy, Debug, PartialEq, Pod, Zeroable)]
pub struct URect {
    pub pos: UVec2,
    pub size: UVec2,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Pod, Zeroable)]
pub struct DRect {
    pub pos: DVec2,
    pub size: DVec2,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn r(px: f64, py: f64, sx: f64, sy: f64) -> DRect {
        DRect::from_pos_size(DVec2::new(px, py), DVec2::new(sx, sy))
    }

    #[test]
    fn from_center_size_and_center_roundtrip() {
        let rect = DRect::from_center_size(DVec2::new(3.0, -1.0), DVec2::new(4.0, 2.0));
        assert_eq!(rect.pos, DVec2::new(1.0, -2.0)); // center - size/2
        assert_eq!(rect.size, DVec2::new(4.0, 2.0));
        assert_eq!(rect.center(), DVec2::new(3.0, -1.0));
    }

    #[test]
    fn intersects_is_strict_overlap() {
        let a = r(0.0, 0.0, 2.0, 2.0);
        // Overlapping.
        assert!(a.intersects(&r(1.0, 1.0, 2.0, 2.0)));
        // Fully disjoint to the right.
        assert!(!a.intersects(&r(3.0, 0.0, 1.0, 1.0)));
        // Edge-touching at x=2 is NOT an intersection (strict `<`/`>`).
        assert!(!a.intersects(&r(2.0, 0.0, 1.0, 1.0)));
        // Overlapping by an epsilon is.
        assert!(a.intersects(&r(1.999, 0.0, 1.0, 1.0)));
    }

    #[test]
    fn contains_allows_equal_edges_but_not_overhang() {
        let outer = r(0.0, 0.0, 4.0, 4.0);
        assert!(outer.contains(&r(1.0, 1.0, 2.0, 2.0))); // strictly inside
        assert!(outer.contains(&outer)); // equal rects: inclusive bounds
        assert!(!outer.contains(&r(3.0, 3.0, 2.0, 2.0))); // overhangs far corner
        assert!(!outer.contains(&r(-0.5, 0.0, 1.0, 1.0))); // overhangs near corner
    }
}
