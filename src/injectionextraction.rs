pub const NULL_CHAR: u8 = 0u8;

use opencv::core::Size;

///
/// Represent a single pixel of color (R, G, B)
///
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Create a size to hold the height and width of a frame for the opencv framework
pub fn map_to_size(width: u16, height: u16) -> Size {
    Size::new(i32::from(width), i32::from(height))
}


#[cfg(test)]
mod injectionextraction_tests {
  use super::*;
  #[test]
  fn test_calculate_actual_size_1() {
    let result = map_to_size(100, 50);
    assert_eq!(result.width, 100);
    assert_eq!(result.height, 50);
  }
  #[test]
  fn test_calculate_actual_size_2() {
    let result = map_to_size(1000, 500);
    assert_eq!(result.width, 1000);
    assert_eq!(result.height, 500);
  }
}