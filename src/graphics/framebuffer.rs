use std::iter;

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

pub struct ClearValue {
    pub color: u32,
    pub depth: f32,
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
}
