pub const NULL_CHAR: u8 = 0u8;

#[cfg(feature = "opencv-backend")]
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
#[cfg(feature = "opencv-backend")]
pub fn map_to_size(width: u16, height: u16) -> Size {
    Size::new(i32::from(width), i32::from(height))
}

// ---------------------------------------------------------------------------
// Frame geometry shared by injection and extraction.
//
// Every frame is laid out as:
//   * an outer calibration ring `BORDER_CELLS` thick that holds three QR-style
//     finder patterns (top-left, top-right, bottom-left) used to re-align a
//     captured frame back to canonical pixels (see extractionlogics).
//   * an inner content rectangle that holds, in row-major cell order, a fixed
//     `HEADER_BITS` header followed by the payload.
//
// A "cell" is a `size` x `size` block of identical pixels (spatial redundancy).
// ---------------------------------------------------------------------------

/// Thickness, in cells, of the calibration ring drawn around every frame.
///
/// Must be at least `QUIET_CELLS + MARKER_CELLS + QUIET_CELLS`: an outer quiet
/// cell, the finder pattern, and an *inner* quiet cell. The inner quiet cell is
/// the critical part - it is a white moat between each finder and the payload.
/// Without it the finder's black ring touches dark payload content (even
/// diagonally, under 8-connected contour following), merging the marker into the
/// content blob so it can no longer be located. That misregistration corrupted
/// every decoded frame. `1 + 7 + 1 = 9`.
pub const BORDER_CELLS: usize = 9;

/// A finder pattern is `MARKER_CELLS` x `MARKER_CELLS` cells (QR-style 7x7).
pub const MARKER_CELLS: usize = 7;

/// Quiet zone, in cells, between the absolute frame edge and a finder pattern.
pub const QUIET_CELLS: usize = 1;

/// Number of black/white cells reserved for the per-frame header.
pub const HEADER_BITS: usize = 128;

/// Identifies our frame format. A mismatch means the frame is not ours (or is a
/// different/older format) and must be rejected.
pub const FORMAT_MAGIC: u8 = 0xA5;

/// Number of cells across the whole frame.
pub fn cells_wide(width: u16, size: u8) -> usize {
    width as usize / size as usize
}

/// Number of cells down the whole frame.
pub fn cells_high(height: u16, size: u8) -> usize {
    height as usize / size as usize
}

/// Number of usable content cells across (excluding the calibration ring).
pub fn content_cols(width: u16, size: u8) -> usize {
    cells_wide(width, size).saturating_sub(2 * BORDER_CELLS)
}

/// Number of usable content cells down (excluding the calibration ring).
pub fn content_rows(height: u16, size: u8) -> usize {
    cells_high(height, size).saturating_sub(2 * BORDER_CELLS)
}

/// Number of payload cells available in a single frame, after reserving the
/// header. Returns 0 if the frame is too small to hold even the header.
pub fn frame_capacity(width: u16, height: u16, size: u8) -> usize {
    let content = content_cols(width, size) * content_rows(height, size);
    content.saturating_sub(HEADER_BITS)
}

/// Pixel coordinate (top-left) of the content cell at linear index `index`.
/// Index 0 is the first header cell; index `HEADER_BITS` is the first payload
/// cell. Cells are laid out row-major inside the content rectangle.
pub fn content_cell_xy(index: usize, width: u16, size: u8) -> (u16, u16) {
    let cols = content_cols(width, size);
    let cy = index / cols;
    let cx = index % cols;
    let x = (BORDER_CELLS + cx) * size as usize;
    let y = (BORDER_CELLS + cy) * size as usize;
    (x as u16, y as u16)
}

/// Cell offset (column or row) of the centre of a finder pattern measured from
/// the corresponding frame edge.
const MARKER_CENTER_CELLS: f32 = QUIET_CELLS as f32 + MARKER_CELLS as f32 / 2.0;

/// Canonical pixel centres of the three finder patterns, in the order
/// `[top-left, top-right, bottom-left]`. These are the destination points used
/// to compute the affine transform that re-aligns a captured frame.
pub fn marker_centers_px(width: u16, height: u16, size: u8) -> [(f32, f32); 3] {
    let s = size as f32;
    let off = MARKER_CENTER_CELLS * s;
    let tl = (off, off);
    let tr = (width as f32 - off, off);
    let bl = (off, height as f32 - off);
    [tl, tr, bl]
}

/// Cell (column, row) top-left positions of the three finder patterns, in the
/// order `[top-left, top-right, bottom-left]`. Used by the encoder to draw them.
pub fn marker_cell_origins(width: u16, height: u16, size: u8) -> [(usize, usize); 3] {
    let cols = cells_wide(width, size);
    let rows = cells_high(height, size);
    let tl = (QUIET_CELLS, QUIET_CELLS);
    let tr = (cols - QUIET_CELLS - MARKER_CELLS, QUIET_CELLS);
    let bl = (QUIET_CELLS, rows - QUIET_CELLS - MARKER_CELLS);
    [tl, tr, bl]
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

    #[test]
    fn test_frame_capacity_excludes_border_and_header() {
        // 64 cells wide/high, border removes 2*BORDER_CELLS each side.
        let content = 64 - 2 * BORDER_CELLS;
        let cap = frame_capacity(64, 64, 1);
        assert_eq!(cap, content * content - HEADER_BITS);
    }

    #[test]
    fn test_content_cell_xy_is_inside_content_region() {
        let (x, y) = content_cell_xy(0, 64, 1);
        assert_eq!(x as usize, BORDER_CELLS);
        assert_eq!(y as usize, BORDER_CELLS);
    }

    #[test]
    fn test_marker_centers_are_symmetric() {
        let [tl, tr, bl] = marker_centers_px(100, 80, 1);
        assert_eq!(tl, (4.5, 4.5));
        assert_eq!(tr, (95.5, 4.5));
        assert_eq!(bl, (4.5, 75.5));
    }
}
