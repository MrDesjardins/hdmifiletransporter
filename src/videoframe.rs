use opencv::core::{Mat, Size, CV_8UC3};
use opencv::prelude::*;

use crate::bitlogics::get_rgb_for_bit;
use crate::injectionextraction::{
    cells_high, cells_wide, content_cell_xy, marker_cell_origins, Color, BORDER_CELLS, MARKER_CELLS,
};
use crate::instructionlogics::FrameHeader;

/// Define a single frame that the video will play
/// E.g. on a 30fps video, there will be 30 VideoFrame every second
///
/// Original source: <https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/embedsource.rs>
#[derive(Clone)]
pub struct VideoFrame {
    /// A Mat is a dense array to store color
    ///
    /// Reference: <https://docs.opencv.org/3.4/d3/d63/classcv_1_1Mat.html>
    pub image: Mat,

    /// Each frame has as width and height. This is the multiplication of both.
    /// The frame_size is the resolution of the video. We expect each frame of
    /// the video to have the same frame size
    pub frame_size: Size,
}

impl VideoFrame {
    pub fn new(width: u16, height: u16) -> VideoFrame {
        let frame_size = Size::new(width.into(), height.into());
        unsafe {
            let image = Mat::new_rows_cols(frame_size.height, frame_size.width, CV_8UC3)
                .expect("Failed to create new Mat");

            VideoFrame { image, frame_size }
        }
    }

    pub fn write(&mut self, r: u8, g: u8, b: u8, x: u16, y: u16, size: u8) {
        for i in 0..size {
            for j in 0..size {
                let result = self.image.at_2d_mut::<opencv::core::Vec3b>(
                    i32::from(y) + i32::from(i),
                    i32::from(x) + i32::from(j),
                );
                match result {
                    Ok(bgr) => {
                        // Opencv works with bgr format instead of rgb
                        bgr[2] = r;
                        bgr[1] = g;
                        bgr[0] = b;
                    }
                    Err(e) => panic!("x:{}, y:{}, i:{}, j:{}, Error Message:{:?}", x, y, i, j, e),
                }
            }
        }
    }

    pub fn from(image: Mat, size: u8) -> Result<VideoFrame, String> {
        let width = image.cols();
        let height = image.rows();
        let frame_size = Size::new(width, height);

        if height % i32::from(size) != 0 {
            return Err("Image size is not a multiple of the size".to_string());
        }

        Ok(VideoFrame { image, frame_size })
    }

    pub fn read_coordinate_color(&self, x: u16, y: u16) -> Color {
        let bgr = self
            .image
            .at_2d::<opencv::core::Vec3b>(y.into(), x.into())
            .unwrap();

        Color {
            r: bgr[2],
            g: bgr[1],
            b: bgr[0],
        }
    }

    /// Draw the calibration ring used by the extractor to re-align a captured
    /// frame: a white quiet-zone border with three QR-style finder patterns in
    /// the top-left, top-right and bottom-left corners. The asymmetry (only
    /// three corners) lets the decoder recover orientation.
    pub fn write_calibration(&mut self, size: u8) {
        let width = self.frame_size.width as u16;
        let height = self.frame_size.height as u16;
        let cols = cells_wide(width, size);
        let rows = cells_high(height, size);

        // White quiet-zone border ring.
        for cy in 0..rows {
            for cx in 0..cols {
                let in_ring = cx < BORDER_CELLS
                    || cx >= cols - BORDER_CELLS
                    || cy < BORDER_CELLS
                    || cy >= rows - BORDER_CELLS;
                if in_ring {
                    let x = (cx * size as usize) as u16;
                    let y = (cy * size as usize) as u16;
                    self.write(255, 255, 255, x, y, size);
                }
            }
        }

        // Finder patterns at the three corners.
        for (ox, oy) in marker_cell_origins(width, height, size) {
            self.draw_finder_pattern(ox, oy, size);
        }
    }

    /// Draw a single `MARKER_CELLS` x `MARKER_CELLS` concentric-square finder
    /// pattern with its top-left at the given cell coordinate.
    fn draw_finder_pattern(&mut self, origin_cx: usize, origin_cy: usize, size: u8) {
        let last = MARKER_CELLS - 1;
        for r in 0..MARKER_CELLS {
            for c in 0..MARKER_CELLS {
                let outer_ring = r == 0 || r == last || c == 0 || c == last;
                let center = r >= 2 && r <= MARKER_CELLS - 3 && c >= 2 && c <= MARKER_CELLS - 3;
                let (rr, gg, bb) = if outer_ring || center {
                    (0, 0, 0) // black
                } else {
                    (255, 255, 255) // white middle ring
                };
                let x = ((origin_cx + c) * size as usize) as u16;
                let y = ((origin_cy + r) * size as usize) as u16;
                self.write(rr, gg, bb, x, y, size);
            }
        }
    }

    /// Write the per-frame header (black/white) into the first `HEADER_BITS`
    /// content cells (just inside the calibration ring).
    pub fn write_header(&mut self, header: &FrameHeader, size: u8) {
        let width = self.frame_size.width as u16;
        let bits = header.to_bits();
        for (index, bit) in bits.iter().enumerate() {
            let (x, y) = content_cell_xy(index, width, size);
            let (r, g, b) = get_rgb_for_bit(*bit);
            self.write(r, g, b, x, y, size);
        }
    }
}

#[cfg(test)]
mod videoframe_tests {
    use crate::injectionextraction::{content_cell_xy, BORDER_CELLS};
    use crate::instructionlogics::{FrameHeader, FrameType};

    use super::VideoFrame;
    use opencv::core::prelude::*;
    use opencv::core::{Mat, CV_8UC3};

    #[test]
    fn test_new_create_image_size() {
        let result = VideoFrame::new(100, 50);
        assert_eq!(result.frame_size.width, 100);
        assert_eq!(result.frame_size.height, 50);
    }

    #[test]
    fn test_new_create_image_mat_size() {
        let result = VideoFrame::new(100, 50);
        let s = result.image.size().unwrap();
        assert_eq!(s.width, 100);
        assert_eq!(s.height, 50);
    }

    #[test]
    fn test_write_image_color() {
        let mut videoframe = VideoFrame::new(100, 50);
        videoframe.write(10, 20, 30, 0, 0, 1);
        let pixel = videoframe.image.at_2d::<opencv::core::Vec3b>(0, 0).unwrap();
        assert_eq!(pixel[0], 30);
        assert_eq!(pixel[1], 20);
        assert_eq!(pixel[2], 10);
    }

    #[test]
    fn test_read_coordinate_color() {
        let mut videoframe = VideoFrame::new(100, 50);
        videoframe.write(10, 20, 30, 0, 0, 1);
        let color = videoframe.read_coordinate_color(0, 0);
        assert_eq!(color.b, 30);
        assert_eq!(color.g, 20);
        assert_eq!(color.r, 10);
    }

    #[test]
    fn test_from_define_size() {
        unsafe {
            let mat = Mat::new_rows_cols(100, 200, CV_8UC3).unwrap();
            let videoframe = VideoFrame::from(mat, 1);
            let unwrapped = videoframe.unwrap();
            assert_eq!(unwrapped.frame_size.width, 200);
            assert_eq!(unwrapped.frame_size.height, 100);
        }
    }

    #[test]
    fn test_from_rejects_height_not_multiple_of_cell_size() {
        unsafe {
            let mat = Mat::new_rows_cols(101, 200, CV_8UC3).unwrap();
            match VideoFrame::from(mat, 2) {
                Ok(_) => panic!("Expected frame size validation to fail"),
                Err(err) => assert_eq!(err, "Image size is not a multiple of the size"),
            }
        }
    }

    #[test]
    fn test_write_calibration_draws_white_corner_and_finder() {
        // Large enough to hold the border ring and finder patterns.
        let mut videoframe = VideoFrame::new(128, 128);
        videoframe.write_calibration(1);

        // The very top-left pixel is the quiet zone (white).
        let color = videoframe.read_coordinate_color(0, 0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);

        // The finder pattern starts one quiet cell in: its outer ring is black.
        let color = videoframe.read_coordinate_color(1, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);

        // The centre of the 7x7 finder pattern (cell 1+3, 1+3) is black.
        let color = videoframe.read_coordinate_color(4, 4);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);

        // The middle ring (cell 1+1, 1+3) is white.
        let color = videoframe.read_coordinate_color(2, 4);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
    }

    #[test]
    fn test_write_header_round_trips_into_content_cells() {
        let mut videoframe = VideoFrame::new(128, 128);
        let header = FrameHeader::new(FrameType::Data, 7, &[1, 2, 3]);
        videoframe.write_header(&header, 1);

        // Read the HEADER_BITS cells back and parse them.
        let bits: Vec<bool> = (0..crate::injectionextraction::HEADER_BITS)
            .map(|i| {
                let (x, y) = content_cell_xy(i, 128, 1);
                let c = videoframe.read_coordinate_color(x, y);
                // White (>=128 average) means bit set.
                (c.r as u32 + c.g as u32 + c.b as u32) >= 382
            })
            .collect();
        let parsed = FrameHeader::from_bits(&bits).expect("header should parse");
        assert_eq!(parsed, header);
        // First content cell is just inside the border ring.
        let (x, y) = content_cell_xy(0, 128, 1);
        assert_eq!(x as usize, BORDER_CELLS);
        assert_eq!(y as usize, BORDER_CELLS);
    }
}
