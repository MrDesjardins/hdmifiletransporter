use opencv::core::prelude::*;
use opencv::core::{Mat, Size, CV_8UC3};

use crate::bitlogics::get_rgb_for_bit;
use crate::injectionextraction::Color;
use crate::instructionlogics::Instruction;

/// Define a single frame that the video will play
/// E.g. on a 30fps video, there will be 30 VideoFrame every second
///
/// Original source: https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/embedsource.rs
#[derive(Clone)]
pub struct VideoFrame {
    /// A Mat is a dense array to store color
    ///
    /// Reference: https://docs.opencv.org/3.4/d3/d63/classcv_1_1Mat.html
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

    /// Write at the beginning of the frame the instruction using the reserved space
    /// with the size pixel per bit regardless if BW or RGB mode. So, will be BW all
    /// the time and will take 64 "spaces" (E.g. 64 pixels if size is 1)
    pub fn write_instruction(&mut self, instruction: &Instruction, size: u8) -> (u16, u16) {
        let mut instruction_index = 0;
        let mut x: u16 = 0;
        let mut y: u16 = 0;
        'outer: for i in (0..self.frame_size.height as u16).step_by(size as usize) {
            for j in (0..self.frame_size.width as u16).step_by(size as usize) {
                if instruction_index < 64 {
                    let (r, g, b) = get_rgb_for_bit(
                        instruction.relevant_byte_count_in_64bits[instruction_index],
                    );
                    self.write(r, g, b, j, i, size);
                    x = j + size as u16;
                    instruction_index += 1;
                } else {
                    break 'outer;
                }
            }

            y = i + size as u16;
            x = 0; // Return to the beginning of the next line
        }
        if x == self.frame_size.width as u16 {
            x = 0; // y is already increased
        }
        return (x, y);
    }

    /// Write a number at a specific location
    pub fn write_pagination(
        &mut self,
        x_start: u16,
        y_start: u16,
        pagination: &u64,
        size: u8,
    ) -> (u16, u16) {
        let mut pagination_index = 0;
        let mut x: u16 = x_start;
        let mut y: u16 = y_start;
        // We reuse the pagination object since we will store the page number into a 64 bits also
        // This might is possible until more instructions are added then would need to move the logic
        // outside the Instruction
        let pagination_instruction = Instruction::new(*pagination);
        'outer: while y < self.frame_size.height as u16 {
            while x < self.frame_size.width as u16 {
                if pagination_index < 64 {
                    let (r, g, b) = get_rgb_for_bit(
                        pagination_instruction.relevant_byte_count_in_64bits[pagination_index],
                    );
                    self.write(r, g, b, x, y, size);
                    x += size as u16;
                    pagination_index += 1;
                } else {
                    break 'outer;
                }
            }
            y += size as u16;
            x = 0; // Return to the beginning of the next line
        }
        if x == self.frame_size.width as u16 {
            x = 0; // y is already increased
        }
        return (x, y);
    }
}

#[cfg(test)]
mod videoframe_tests {
    use crate::instructionlogics::Instruction;

    use super::VideoFrame;
    use opencv::core::prelude::*;
    use opencv::core::{Mat, CV_8UC3};
    use opencv::prelude::MatTraitConstManual;
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
    fn test_write_image_instruction() {
        let mut videoframe = VideoFrame::new(100, 100);
        let mut instruction = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instruction.relevant_byte_count_in_64bits[63] = true;

        videoframe.write_instruction(&instruction, 1);
        let mut color: crate::injectionextraction::Color;
        for i in 0..3 {
            color = videoframe.read_coordinate_color(i, 0);
            assert_eq!(color.b, 0);
            assert_eq!(color.g, 0);
            assert_eq!(color.r, 0);
        }
        color = videoframe.read_coordinate_color(63, 0); // The last bit of instruction is true
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
    }

    #[test]
    fn test_write_image_pagination_with_instruction() {
        let mut videoframe = VideoFrame::new(200, 100);
        let mut instruction = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instruction.relevant_byte_count_in_64bits[63] = true;

        let (x, y) = videoframe.write_instruction(&instruction, 1);
        videoframe.write_pagination(x, y, &1, 1);
        let mut color: crate::injectionextraction::Color;
        for i in 0..3 {
            color = videoframe.read_coordinate_color(i, 0);
            assert_eq!(color.b, 0);
            assert_eq!(color.g, 0);
            assert_eq!(color.r, 0);
        }
        color = videoframe.read_coordinate_color(63, 0); // The last bit of instruction is true
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
        color = videoframe.read_coordinate_color(64, 0); // The first bit of pagination
        assert_eq!(color.b, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.r, 0);
        color = videoframe.read_coordinate_color(126, 0); // The bit before last of pagination
        assert_eq!(color.b, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.r, 0);
        color = videoframe.read_coordinate_color(127, 0); // The last bit of pagination
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
    }

    #[test]
    fn test_write_image_pagination2_with_instruction() {
        let mut videoframe = VideoFrame::new(200, 100);
        let mut instruction = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instruction.relevant_byte_count_in_64bits[63] = true;

        let (x, y) = videoframe.write_instruction(&instruction, 1);
        videoframe.write_pagination(x, y, &3, 1);
        let mut color: crate::injectionextraction::Color;
        for i in 0..3 {
            color = videoframe.read_coordinate_color(i, 0);
            assert_eq!(color.b, 0);
            assert_eq!(color.g, 0);
            assert_eq!(color.r, 0);
        }
        color = videoframe.read_coordinate_color(63, 0); // The last bit of instruction is true
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
        color = videoframe.read_coordinate_color(64, 0); // The first bit of pagination
        assert_eq!(color.b, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.r, 0);
        color = videoframe.read_coordinate_color(126, 0); // The bit before last of pagination
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
        color = videoframe.read_coordinate_color(127, 0); // The last bit of pagination
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
    }
    #[test]
    fn test_write_image_pagination_without_instruction() {
        let mut videoframe = VideoFrame::new(200, 100);
        let (x, _y) = videoframe.write_pagination(0, 0, &3, 1);
        let mut color: crate::injectionextraction::Color;
        for i in 0..61 {
            color = videoframe.read_coordinate_color(i, 0);
            assert_eq!(color.b, 0);
            assert_eq!(color.g, 0);
            assert_eq!(color.r, 0);
        }
        color = videoframe.read_coordinate_color(62, 0); // The bit before last of instruction is true
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
        color = videoframe.read_coordinate_color(63, 0); // The last bit of pagination
        assert_eq!(color.b, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.r, 255);
        // Check if the x moved
        assert_eq!(x, 64) // (0..63) = Pagination, 64 is the next
    }
}
