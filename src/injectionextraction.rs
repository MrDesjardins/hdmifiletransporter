// const NUMBER_BIT_PER_BYTE: u8 = 8;
pub const EOF_CHAR: u8 = 4u8;

use opencv::core::Size;

///
/// Represent a single pixel of color (R, G, B)
///
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// The actual size is calculated using the size of the information.
/// It represents the number of data that will actually fits into the
/// frame. The size of the info affect the actual size of the frame but
/// will always render the frame_size. Thus, actual_size is equal or
/// smaller than the frame_size.
///
/// # Examples
/// E.g. A width of 100, height of 100 and a size of 1 gives an actual
/// size of 10 000.
/// E.g. A width of 100, height of 100 and a size of 2 gives an actual
/// size of 2 500.
pub fn calculate_actual_size(width: u16, height: u16, size: u8) -> Size {
    let actual_width = width - (width % u16::from(size));
    let actual_height = height - (height % u16::from(size));
    let actual_size = Size::new(i32::from(actual_width), i32::from(actual_height));
    actual_size
}
