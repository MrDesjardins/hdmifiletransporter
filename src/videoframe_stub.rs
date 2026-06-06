use crate::injectionextraction::Color;

/// Placeholder frame type used when the `opencv-backend` feature is disabled.
///
/// Enable the default `opencv-backend` feature to create, read, write, encode,
/// decode, and register real video frames.
#[derive(Clone)]
pub struct VideoFrame {
    pub width: u16,
    pub height: u16,
}

impl VideoFrame {
    pub fn new(width: u16, height: u16) -> VideoFrame {
        VideoFrame { width, height }
    }

    pub fn write(&mut self, _r: u8, _g: u8, _b: u8, _x: u16, _y: u16, _size: u8) {}

    pub fn read_coordinate_color(&self, _x: u16, _y: u16) -> Color {
        Color { r: 0, g: 0, b: 0 }
    }
}
