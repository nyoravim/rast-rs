use super::image::CoordinateIterator;

#[derive(Debug, Clone)]
pub struct Scissor {
    pub x: usize,
    pub y: usize,

    pub width: usize,
    pub height: usize,
}

impl Scissor {
    pub fn coordinates(&self) -> CoordinateIterator {
        CoordinateIterator::new(self.width, self.height)
    }

    pub fn contains(&self, x: usize, y: usize) -> bool {
        let x1 = self.x + self.width;
        let y1 = self.y + self.height;

        x >= self.x && x < x1 && y >= self.y && y < y1
    }

    pub fn intersect_with(&self, other: &Scissor) -> Option<Scissor> {
        let x0 = self.x.max(other.x);
        let y0 = self.y.max(other.y);

        let x1 = (self.x + self.width).min(other.x + other.width);
        let y1 = (self.y + self.height).min(other.y + other.height);

        if x1 <= x0 || y1 <= y0 {
            None
        } else {
            Some(Scissor {
                x: x0,
                y: y0,

                width: x1 - x0,
                height: y1 - y0,
            })
        }
    }
}
