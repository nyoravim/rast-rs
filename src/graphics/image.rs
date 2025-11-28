use std::iter::{self, Iterator};
use std::mem;

pub struct Image<T: Sized> {
    data: Vec<T>,
    width: usize,
    height: usize,
}

impl<T: Sized + Default> Image<T> {
    pub fn new(width: usize, height: usize) -> Image<T> {
        let total_pixels = width * height;

        Image {
            data: Vec::from_iter(iter::repeat_with(|| T::default()).take(total_pixels)),
            width: width,
            height: height,
        }
    }
}

impl<T: Sized> Image<T> {
    fn index_of(&self, x: usize, y: usize) -> Option<usize> {
        if x >= self.width || y >= self.height {
            None
        } else {
            Some(y * self.width + x)
        }
    }

    pub fn at<'a>(&'a self, x: usize, y: usize) -> Option<&'a T> {
        self.index_of(x, y).map(|index| &self.data[index])
    }

    pub fn exchange(&mut self, x: usize, y: usize, value: T) -> Option<T> {
        self.index_of(x, y).map(|index| {
            let mut other = value;
            mem::swap(&mut other, &mut self.data[index]);

            other
        })
    }

    pub fn size(&self) -> (usize, usize) {
        (self.width, self.height)
    }

    pub fn coordinates(&self) -> CoordinateIterator {
        CoordinateIterator {
            pixel_index: 0,
            width: self.width,
            height: self.height,
        }
    }
}

pub struct CoordinateIterator {
    pixel_index: usize,
    width: usize,
    height: usize,
}

impl CoordinateIterator {
    pub(crate) fn new(width: usize, height: usize) -> CoordinateIterator {
        CoordinateIterator {
            pixel_index: 0,
            width,
            height,
        }
    }
}

impl Iterator for CoordinateIterator {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        let total_pixels = self.width * self.height;
        if self.pixel_index >= total_pixels {
            return None;
        }

        let x = self.pixel_index % self.width;
        let y = self.pixel_index / self.width;
        self.pixel_index += 1;

        return Some((x, y));
    }
}
