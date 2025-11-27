pub mod color;
pub mod graphics;

use color::RGBA8;
use graphics::*;

fn dump_image(data: &Image<u32>) {
    let (width, height) = data.size();
    let mut image = bmp::Image::new(width as u32, height as u32);

    for (x, y) in image.coordinates() {
        let color = match data.at(x as usize, y as usize) {
            Some(value) => RGBA8::from(value.clone()),
            None => RGBA8::default(),
        };

        image.set_pixel(
            x,
            y,
            bmp::Pixel {
                r: color.r,
                g: color.g,
                b: color.b,
            },
        );
    }

    image.save("dump.bmp").unwrap();
}

fn main() {
    let _ = Rasterizer {
        // uh
    };

    let mut fb = Framebuffer::new(1600, 900, 1, false);

    fb.clear(&ClearValue {
        color: 0x00FF00FF,
        depth: 1.0,
    });

    dump_image(&fb.color_attachments()[0]);
}
