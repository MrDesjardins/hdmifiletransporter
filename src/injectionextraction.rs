// const NUMBER_BIT_PER_BYTE: u8 = 8;
pub const EOF_CHAR: u8 = 4u8;

///
/// Represent a single pixel of color (R, G, B)
///
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}