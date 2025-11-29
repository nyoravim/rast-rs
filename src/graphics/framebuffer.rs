use std::iter::{self, Iterator};

use super::image::Image;

fn fill_image<T: Sized + Copy>(attachment: &mut Image<T>, value: T) {
    for (x, y) in attachment.coordinates() {
        attachment.exchange(x, y, value);
    }
}

pub struct Framebuffer {
    width: usize,
    height: usize,

    color: Vec<Image<u32>>,
    depth: Option<Image<f32>>,
}

#[derive(Debug)]
pub struct ClearValue {
    pub color: u32,
    pub depth: f32,
}

pub struct MutableScanline<'a> {
    pub y: usize,
    pub color: Vec<&'a mut [u32]>,
    pub depth: Option<&'a mut [f32]>,
}

impl Framebuffer {
    pub fn new(width: usize, height: usize, num_color: usize, has_depth: bool) -> Framebuffer {
        Framebuffer {
            width: width,
            height: height,

            color: Vec::from_iter(iter::repeat_with(|| Image::new(width, height)).take(num_color)),
            depth: match has_depth {
                true => Some(Image::new(width, height)),
                false => None,
            },
        }
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn color_attachments(&self) -> &Vec<Image<u32>> {
        &self.color
    }

    pub fn depth_attachment(&self) -> &Option<Image<f32>> {
        &self.depth
    }

    pub fn clear(&mut self, value: &ClearValue) {
        for attachment in &mut self.color {
            fill_image(attachment, value.color);
        }

        if let Some(depth) = &mut self.depth {
            fill_image(depth, value.depth);
        }
    }

    pub fn scanlines<'a>(&'a mut self, offset: usize, count: usize) -> Vec<MutableScanline<'a>> {
        if offset >= self.height || offset + count > self.height {
            panic!("Invalid scanline range!");
        }

        let start = offset * self.width;
        let end = (offset + count) * self.width;

        // vector of advancing cursors through the color attachments through the range selected by
        // the user
        let mut color_cursors: Vec<_> = self
            .color
            .iter_mut()
            .map(|attachment| &mut attachment.data_mut()[start..end])
            .collect();

        // advancing cursor through depth attachment if one exists
        let mut depth_cursor = self
            .depth
            .as_mut()
            .map(|attachment| &mut attachment.data_mut()[start..end]);

        let mut scanlines = Vec::new();
        for delta_y in 0..count {
            let mut color = Vec::new();
            let mut advanced_cursors = Vec::new();

            // advance color attachment cursors by a row and append a shorter slice at the previous
            // location
            for cursor in color_cursors {
                let (first, second) = cursor.split_at_mut(self.width);

                color.push(first);
                advanced_cursors.push(second);
            }

            color_cursors = advanced_cursors;
            scanlines.push(MutableScanline {
                y: offset + delta_y,
                color,

                // i would rather use Option::map but it moves the value regardless of if its
                // Some(_) or None so we have to match
                depth: match depth_cursor {
                    // if we have a depth attachment, advance it by a row and return a shorter
                    // slice at the previous location
                    Some(cursor) => {
                        let (first, second) = cursor.split_at_mut(self.width);
                        depth_cursor = Some(second);

                        Some(first)
                    }

                    // otherwise we dont care
                    None => None,
                },
            });
        }

        scanlines
    }
}
