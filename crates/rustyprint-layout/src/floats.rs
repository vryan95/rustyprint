//! Floats (CSS 2.1 § 9.5): the floats of a block formatting context, where new ones go, and
//! how much room they leave for lines and boxes beside them.
//!
//! Positions are in the coordinates of the formatting context's root: from the top-left
//! corner of its border box.

use crate::display::Rect;
use crate::shapes::Shape;

/// A placed float's margin box, which side it floats to, and the shape lines wrap around.
#[derive(Debug, Clone)]
struct Placed {
    rect: Rect,
    left: bool,
    shape: Option<Shape>,
}

/// The floats placed in a block formatting context so far.
#[derive(Debug, Default)]
pub(crate) struct Floats {
    placed: Vec<Placed>,
}

/// How far floats reach into a band of a containing block, from its left and right edges.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub(crate) struct Band {
    pub left: f32,
    pub right: f32,
}

impl Floats {
    pub fn is_empty(&self) -> bool {
        self.placed.is_empty()
    }

    /// How far the floats beside the band from `top` down `height` reach into the space
    /// between `left` and `right`. An empty band still meets the floats it starts in.
    pub fn band(&self, top: f32, height: f32, left: f32, right: f32) -> Band {
        let mut band = Band::default();
        for float in &self.placed {
            let r = float.rect;
            let overlaps = r.y < top + height.max(0.01) && r.bottom() > top;
            if !overlaps || r.height <= 0.0 {
                continue;
            }
            if float.left {
                band.left = band.left.max(r.right() - left);
            } else {
                band.right = band.right.max(right - r.x);
            }
        }
        band
    }

    /// Places a float with a margin box of `width` by `height` on the left or right side of
    /// a containing block between `left` and `right`, as high as it can go from `top` (CSS
    /// 2.1 § 9.5.1): below earlier floats' tops, and down past floats until it fits or
    /// nothing is beside it. Returns its margin box.
    pub fn place(
        &mut self,
        width: f32,
        height: f32,
        float_left: bool,
        top: f32,
        left: f32,
        right: f32,
    ) -> Rect {
        // A float is no higher than any earlier float.
        let mut y = self
            .placed
            .iter()
            .map(|float| float.rect.y)
            .fold(top, f32::max);
        loop {
            let band = self.band(y, height, left, right);
            let available = right - left - band.left - band.right;
            let blocked = band.left > 0.0 || band.right > 0.0;
            if width <= available + 0.01 || !blocked {
                let x = if float_left {
                    left + band.left
                } else {
                    right - band.right - width
                };
                let rect = Rect::new(x, y, width, height);
                self.placed.push(Placed {
                    rect,
                    left: float_left,
                    shape: None,
                });
                return rect;
            }
            // Move down to where the next float beside it ends.
            y = self.next_bottom(y, height);
        }
    }

    /// Gives the last placed float a shape, which lines wrap around instead of its margin
    /// box.
    pub fn shape_last(&mut self, shape: Shape) {
        if let Some(float) = self.placed.last_mut() {
            float.shape = Some(shape);
        }
    }

    /// Like `band`, for lines: floats with a shape reach as far as their shape does in the
    /// band, within their margin box (CSS Shapes 1 § 4).
    pub fn line_band(&self, top: f32, height: f32, left: f32, right: f32) -> Band {
        let mut band = Band::default();
        for float in &self.placed {
            let r = float.rect;
            let overlaps = r.y < top + height.max(0.01) && r.bottom() > top;
            if !overlaps || r.height <= 0.0 {
                continue;
            }
            let (from, to) = match &float.shape {
                None => (r.x, r.right()),
                Some(shape) => match shape.extent(top, height) {
                    Some((from, to)) => (from.max(r.x), to.min(r.right())),
                    None => continue,
                },
            };
            if to <= from {
                continue;
            }
            if float.left {
                band.left = band.left.max(to - left);
            } else {
                band.right = band.right.max(right - from);
            }
        }
        band
    }

    /// The highest bottom of the floats beside the band from `top` down `height`, below
    /// `top`.
    fn next_bottom(&self, top: f32, height: f32) -> f32 {
        self.placed
            .iter()
            .map(|float| float.rect)
            .filter(|r| r.y < top + height.max(0.01) && r.bottom() > top)
            .map(|r| r.bottom())
            .fold(f32::INFINITY, f32::min)
    }

    /// Where a box that clears floats on the left, the right or both starts at the latest:
    /// below the bottom margin edges of those floats. `None` if there are none.
    pub fn clearance(&self, left: bool, right: bool) -> Option<f32> {
        self.placed
            .iter()
            .filter(|float| (float.left && left) || (!float.left && right))
            .map(|float| float.rect.bottom())
            .reduce(f32::max)
    }

    /// The lowest bottom margin edge of any float, which a formatting context root grows to
    /// contain.
    pub fn bottom(&self) -> Option<f32> {
        self.clearance(true, true)
    }

    /// Where a box of `height` that cannot overlap floats fits between `left` and `right`
    /// from `top` down, with at least `min_width` of room: its top and the band it gets.
    pub fn fit(&self, top: f32, height: f32, min_width: f32, left: f32, right: f32) -> (f32, Band) {
        let mut y = top;
        loop {
            let band = self.band(y, height, left, right);
            let blocked = band.left > 0.0 || band.right > 0.0;
            if !blocked || right - left - band.left - band.right >= min_width - 0.01 {
                return (y, band);
            }
            let next = self.next_bottom(y, height);
            if !next.is_finite() {
                return (y, band);
            }
            y = next;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn floats_stack_and_wrap() {
        let mut floats = Floats::default();
        let a = floats.place(100.0, 50.0, true, 0.0, 0.0, 300.0);
        assert_eq!((a.x, a.y), (0.0, 0.0));
        let b = floats.place(100.0, 30.0, true, 0.0, 0.0, 300.0);
        assert_eq!((b.x, b.y), (100.0, 0.0));
        let c = floats.place(150.0, 30.0, false, 0.0, 0.0, 300.0);
        // Does not fit beside both left floats: it goes below the shorter one.
        assert_eq!((c.x, c.y), (150.0, 30.0));
        assert_eq!(
            floats.band(10.0, 10.0, 0.0, 300.0),
            Band {
                left: 200.0,
                right: 0.0
            }
        );
        assert_eq!(floats.clearance(false, true), Some(60.0));
        assert_eq!(floats.bottom(), Some(60.0));
    }
}
