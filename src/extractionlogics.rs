use opencv::videoio::VideoCapture;

use std::fs;

use crate::bitlogics::{get_bit_from_rgb, mutate_byte};
use crate::injectionextraction::map_to_size;
use crate::options::AlgoFrame;
use crate::videoframe::VideoFrame;
use opencv::core::{Mat, Size};
use opencv::prelude::MatTraitConst;
use opencv::prelude::VideoCaptureTrait;
use opencv::videoio::CAP_ANY;

use crate::{injectionextraction::EOF_CHAR, options::ExtractOptions};

struct FrameBytesInfo {
    pub bytes: Vec<u8>,
    pub is_red_frame: bool,
}

pub fn video_to_frames(extract_options: &ExtractOptions) -> Vec<VideoFrame> {
    let mut video = VideoCapture::from_file(&extract_options.video_file_path, CAP_ANY)
        .expect("Could not open video path");
    let mut all_frames = Vec::new();
    loop {
        let mut frame = Mat::default();
        video
            .read(&mut frame)
            .expect("Reading frame shouldn't crash");

        if frame.cols() == 0 {
            break;
        }

        let source = VideoFrame::from(frame, extract_options.size);
        match source {
            Ok(frame) => {
                all_frames.push(frame);
            }
            Err(err) => {
                panic!("Reading from VideoFrame: {:?}", err);
            }
        }
    }

    all_frames
}

/// Take the pixels from a collection of frames into a collection of byte
/// The byte values are from the RGB of the pixels
pub fn frames_to_data(extract_options: &ExtractOptions, frames: Vec<VideoFrame>) -> Vec<u8> {
    let mut byte_data = Vec::new();
    let actual_size = map_to_size(extract_options.width, extract_options.height);
    let mut is_red_frame_found = false;
    let mut relevant_frame_count = 0;
    println!("Initial Frames count: {}", frames.len());
    for frame in frames.iter() {
        let frame_data = if extract_options.algo == AlgoFrame::RGB {
            frame_to_data_method_rgb(frame, actual_size, extract_options.size)
        } else {
            frame_to_data_method_bw(frame, actual_size, extract_options.size)
        };
        if is_red_frame_found && !frame_data.is_red_frame {
            byte_data.extend(frame_data.bytes); // Between two red frames, we accumulate
            relevant_frame_count += 1;
        } else if is_red_frame_found && frame_data.is_red_frame {
            return byte_data; // We have all our frames
        } else if !is_red_frame_found && frame_data.is_red_frame {
            is_red_frame_found = true; // From that point, we start accumulating byte
        }
    }
    println!("Relevant Frames count: {}", relevant_frame_count);
    byte_data
}

/// Extract from a frame all the data. Once the end of file character is found, the loop is done.
///
/// Handle the case that we found a EOF character
///
/// # Source
/// https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L280
fn frame_to_data_method_rgb(
    source: &VideoFrame,
    actual_size: Size,
    info_size: u8,
) -> FrameBytesInfo {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = info_size as usize;
    let mut result = FrameBytesInfo {
        bytes: Vec::new(),
        is_red_frame: false,
    };
    let mut rgbs = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, info_size);
            rgbs.push(vec![rgb[0], rgb[1], rgb[2]]);
            if rgb[0] == EOF_CHAR {
                return result;
            }
            result.bytes.push(rgb[0]);

            if rgb[1] == EOF_CHAR {
                return result;
            }
            result.bytes.push(rgb[1]);

            if rgb[2] == EOF_CHAR {
                return result;
            }
            result.bytes.push(rgb[2]);
        }
    }
    let is_red_frame = check_red_frame(rgbs);
    result.is_red_frame = is_red_frame;
    result
}

/// Extract from a frame all the data
///
/// Handle the case that we found a EOF character
fn frame_to_data_method_bw(
    source: &VideoFrame,
    actual_size: Size,
    info_size: u8,
) -> FrameBytesInfo {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = info_size as usize;
    let mut result = FrameBytesInfo {
        bytes: Vec::new(),
        is_red_frame: false,
    };
    let mut bit_index: u8 = 7;
    let mut data: u8 = 0;
    let mut rgbs = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, info_size);
            rgbs.push(vec![rgb[0], rgb[1], rgb[2]]);
            let bit_value = get_bit_from_rgb(rgb);
            mutate_byte(&mut data, bit_value, bit_index);
            bit_index = if bit_index == 0 { 7 } else { bit_index - 1 };
            if bit_index == 7 {
                if data != EOF_CHAR {
                    result.bytes.push(data);
                    data = 0;
                } else {
                    return result; // The frame has reach a point that it has no more relevant information
                }
            }
        }
    }
    let is_red_frame = check_red_frame(rgbs);
    result.is_red_frame = is_red_frame;
    result
}

/// Check if the list of rgbs are all redish
/// The list should be the content of a single frame
pub fn check_red_frame(list_rgbs: Vec<Vec<u8>>) -> bool {
    for rgb in list_rgbs.iter() {
        // Red is 255, 0 ,0 but we give some room
        if !(rgb[0] >= 220 && rgb[1] <= 30 && rgb[2] <= 30) {
            return false;
        }
    }

    return true;
}
/// Extract a pixel value that might be spread on many sibling pixel to reduce innacuracy
/// # Source
/// Code is a copy of https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L121
fn get_pixel(frame: &VideoFrame, x: i32, y: i32, size: u8) -> Vec<u8> {
    let mut r_list: Vec<u8> = Vec::new();
    let mut g_list: Vec<u8> = Vec::new();
    let mut b_list: Vec<u8> = Vec::new();

    for i in 0..size {
        for j in 0..size {
            let bgr = frame
                .image
                .at_2d::<opencv::core::Vec3b>(y + i32::from(i), x + i32::from(j))
                .unwrap();
            r_list.push(bgr[2]);
            g_list.push(bgr[1]);
            b_list.push(bgr[0]);
        }
    }

    let r_sum: usize = r_list.iter().map(|&x| x as usize).sum();
    let r_average = r_sum / r_list.len();
    let g_sum: usize = g_list.iter().map(|&x| x as usize).sum();
    let g_average = g_sum / g_list.len();
    let b_sum: usize = b_list.iter().map(|&x| x as usize).sum();
    let b_average = b_sum / b_list.len();
    let rgb_average = vec![r_average as u8, g_average as u8, b_average as u8];

    rgb_average
}

/// Move all the data from gathered from the movie file into
/// a file that should be the original file.
///
/// # Example
/// if we injected a .zip file, we expect the file to be written to be also a .zip
///
pub fn data_to_files(extract_options: &ExtractOptions, whole_movie_data: Vec<u8>) {
    fs::write(
        extract_options.extracted_file_path.clone(),
        whole_movie_data,
    )
    .expect("Writing file fail");
}

#[cfg(test)]
mod extractionlogics_tests {
    use super::*;

    #[test]
    fn test_frame_to_data_method_rgb() {
        let size = map_to_size(8, 8);
        let mut frame = VideoFrame::new(8, 8);
        frame.write(10, 20, 30, 0, 0, 1);
        frame.write(40, 50, 60, 1, 0, 1);

        let result = frame_to_data_method_rgb(&frame, size, 1);
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw() {
        let size = map_to_size(8, 8);
        let mut frame = VideoFrame::new(8, 8);
        let write_data = 0b0011_1011;
        
        frame.write(0, 0, 0, 0, 0, 1); // White 0 bit
        frame.write(0, 0, 0, 1, 0, 1); // White 0 bit
        frame.write(255, 255, 255, 2, 0, 1); // Black 1 bit
        frame.write(255, 255, 255, 3, 0, 1); // Black 1 bit
        frame.write(255, 255, 255, 4, 0, 1); // Black 1 bit
        frame.write(0, 0, 0, 2, 5, 1); // White 0 bit
        frame.write(255, 255, 255, 6, 0, 1); // Black 1 bit
        frame.write(255, 255, 255, 7, 0, 1); // Black 1 bit
        
        let result = frame_to_data_method_bw(&frame, size, 1);
        assert_eq!(result.bytes[0], write_data);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw_with_eof() {
        let size = map_to_size(8, 8);
        let mut frame = VideoFrame::new(8, 8);
        let write_data = 0b0011_1011;
        frame.write(0, 0, 0, 0, 0, 1); // White 0 bit
        frame.write(0, 0, 0, 1, 0, 1); // White 0 bit
        frame.write(255, 255, 255, 2, 0, 1); // Black 1 bit
        frame.write(255, 255, 255, 3, 0, 1); // Black 1 bit
        frame.write(255, 255, 255, 4, 0, 1); // Black 1 bit
        frame.write(0, 0, 0, 5, 0, 1); // White 0 bit
        frame.write(255, 255, 255, 6, 0, 1); // Black 1 bit
        frame.write(255, 255, 255, 7, 0, 1); // Black 1 bit

        // EOF = 4 = 0000_1000
        frame.write(0, 0, 0, 0, 1, 1); // White 0 bit EOF
        frame.write(0, 0, 0, 1, 1, 1); // White 0 bit EOF
        frame.write(0, 0, 0, 2, 1, 1); // White 0 bit EOF
        frame.write(0, 0, 0, 3, 1, 1); // White 0 bit EOF
        frame.write(0, 0, 0, 4, 1, 1); // White 0 bit EOF
        frame.write(255, 255, 255, 5, 1, 1); // Black 1 bit EOF
        frame.write(0, 0, 0, 6, 1, 1); // White 0 bit EOF
        frame.write(0, 0, 0, 7, 1, 1); // White 0 bit EOF
        let result = frame_to_data_method_bw(&frame, size, 1);
        assert_eq!(result.bytes[0], write_data); // First
        assert_eq!(result.bytes.len(), 1); // Not the EOF, only the write_data
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn text_check_red_frame_white() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![0, 0, 0]);
        let result = check_red_frame(rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn text_check_red_frame_black() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![255, 255, 255]);
        let result = check_red_frame(rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn text_check_red_frame_red() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![255, 0, 0]);
        let result = check_red_frame(rgbs);
        assert_eq!(result, true)
    }

    #[test]
    fn text_check_red_frame_almost_red() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![245, 5, 10]);
        let result = check_red_frame(rgbs);
        assert_eq!(result, true)
    }

    #[test]
    fn text_check_red_frame_too_far_from_red() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![245, 45, 10]);
        let result = check_red_frame(rgbs);
        assert_eq!(result, false)
    }
}
