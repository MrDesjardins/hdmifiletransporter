use opencv::videoio::VideoCapture;

use std::fs;

use crate::bitlogics::{get_bit_from_rgb, mutate_byte};
use crate::injectionextraction::map_to_size;
use crate::instructionlogics::Instruction;
use crate::options::AlgoFrame;
use crate::videoframe::VideoFrame;
use opencv::core::{Mat, Size};
use opencv::prelude::MatTraitConst;
use opencv::prelude::VideoCaptureTrait;
use opencv::videoio::CAP_ANY;

use crate::options::ExtractOptions;
use indicatif::ProgressBar;
use pretty_bytes_rust::pretty_bytes;
use std::collections::HashMap;
use std::iter::Iterator;

struct FrameBytesInfo {
    pub bytes: Vec<u8>,
    pub is_red_frame: bool,
    pub pagination_or_instruction: Option<Instruction>,
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
    let mut all_frames_bytes = HashMap::new();

    let mut byte_data = Vec::new();
    let actual_size = map_to_size(extract_options.width, extract_options.height);
    let mut relevant_frame_count = 0;
    let total_video_frame = frames.len() as u64;
    let pb = ProgressBar::new(total_video_frame);

    if extract_options.show_progress {
        println!("Initial Frames count: {}", total_video_frame);
    }
    let mut instruction: Option<Instruction> = None;
    for frame in frames.iter() {
        let frame_data = if extract_options.algo == AlgoFrame::RGB {
            frame_to_data_method_rgb(frame, actual_size, extract_options)
        } else {
            frame_to_data_method_bw(frame, actual_size, extract_options)
        };

        // Set the instruction for all subsequent frame
        if frame_data.is_red_frame && frame_data.pagination_or_instruction.is_some() {
            instruction = Some(frame_data.pagination_or_instruction.unwrap());
            if extract_options.show_progress {
                println!(
                    "Instruction found with data size of {}",
                    // pretty_bytes(instruction.unwrap().get_data_size() as u64, None)
                    instruction.unwrap().get_data_size()
                );
            }
        } else if !frame_data.is_red_frame {
            let page_number = frame_data
                .pagination_or_instruction
                .unwrap()
                .get_data_size();
            if extract_options.show_progress {
                pb.inc(1); // Increment means we have analyzed a frame, not that we have found a unique one. We do not know the total until we find the instruction
            }
            if !all_frames_bytes.contains_key(&page_number) {
                all_frames_bytes.insert(page_number, frame_data.bytes);
                relevant_frame_count += 1;
            }
        }
    }
    let p = relevant_frame_count as f32 / total_video_frame as f32;
    if extract_options.show_progress {
        pb.finish_with_message("done");
        println!(
            "Relevant Frames count: {}/{} ({:.3})",
            relevant_frame_count, total_video_frame, p
        );
    }

    match instruction {
        Some(inst) => {
            // Merge all frames in orders
            let mut frame_index = 0;
            while all_frames_bytes.contains_key(&frame_index) {
                byte_data.extend(all_frames_bytes.get(&frame_index).unwrap());
                frame_index += 1;
            }

            // We might have less frame if a frame never reached the target computer. In that case, we panic
            let instr_data_bytes = inst.get_data_size();
            if (byte_data.len() as u64) < instr_data_bytes {
                panic!("We have not receive all frames. We received {} frames for a total of {} bytes and we expected {} bytes", frame_index, byte_data.len(),instr_data_bytes)
            }

            // Return only the number of byte expected from the instruction. We propably have more if the last frame had some null character to fill the frame
            byte_data
                .into_iter()
                .take(instr_data_bytes as usize)
                .collect()
        }
        None => {
            panic!("Instruction not found while extracting data from video");
        }
    }
}

/// Extract from a frame all the data.
///
/// # Source
/// https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L280
fn frame_to_data_method_rgb(
    source: &VideoFrame,
    actual_size: Size,
    options: &ExtractOptions,
) -> FrameBytesInfo {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = options.size as usize;
    let mut result = FrameBytesInfo {
        bytes: Vec::new(),
        is_red_frame: false,
        pagination_or_instruction: None,
    };

    let mut pagination_data = Instruction::new(0);
    let mut pagination_bits_index = 0;

    let mut rgbs = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, options.size);
            rgbs.push(vec![rgb[0], rgb[1], rgb[2]]); // Always, with or without instruction

            let bit_value = get_bit_from_rgb(&rgb);
            if pagination_bits_index < 64 {
                // Will get here only on the first frame and until we have the whole instruction message (64 bits)
                pagination_data.relevant_byte_count_in_64bits[pagination_bits_index] = bit_value;
                if pagination_bits_index == 63 {
                    result.pagination_or_instruction = Some(pagination_data); // Send it back for subsequence frames to use
                }
                pagination_bits_index += 1;
            } else {
                result.bytes.push(rgb[0]);
                result.bytes.push(rgb[1]);
                result.bytes.push(rgb[2]);
            }
        }
    }
    mutate_frame(&mut result, &rgbs, &pagination_data);
    result
}

/// Extract from a frame all the data
fn frame_to_data_method_bw(
    source: &VideoFrame,
    actual_size: Size,
    options: &ExtractOptions,
) -> FrameBytesInfo {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = options.size as usize;
    let mut pagination_data = Instruction::new(0);
    let mut pagination_bits_index = 0;
    let mut result = FrameBytesInfo {
        bytes: Vec::new(),
        is_red_frame: false,
        pagination_or_instruction: None,
    };
    let mut bit_index: u8 = 7;
    let mut data: u8 = 0;
    let mut rgbs = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, options.size);
            rgbs.push(vec![rgb[0], rgb[1], rgb[2]]); // Always, with or without pagination
            let bit_value = get_bit_from_rgb(&rgb);

            if pagination_bits_index < 64 {
                // Will get here only on the first frame and until we have the whole pagination message (64 bits)
                pagination_data.relevant_byte_count_in_64bits[pagination_bits_index] = bit_value;
                if pagination_bits_index == 63 {
                    result.pagination_or_instruction = Some(pagination_data); // Send it back for subsequence frames to use
                }
                pagination_bits_index += 1;
            } else {
                mutate_byte(&mut data, bit_value, bit_index);
                if bit_index == 0 {
                    result.bytes.push(data);
                    data = 0; // Reset, next character needs to accumulate 8 bits
                }
                bit_index = if bit_index == 0 { 7 } else { bit_index - 1 };
            }
        }
    }
    mutate_frame(&mut result, &rgbs, &pagination_data);
    result
}

fn mutate_frame(frame: &mut FrameBytesInfo, rgbs: &Vec<Vec<u8>>, pagination_data: &Instruction) {
    let is_red_frame = check_red_frame(rgbs);
    frame.is_red_frame = is_red_frame;
    frame.pagination_or_instruction = Some(Instruction::new(pagination_data.get_data_size()));
}

/// Check if the list of rgbs are all redish
/// The list should be the content of a single frame
pub fn check_red_frame(list_rgbs: &Vec<Vec<u8>>) -> bool {
    let mut counter = 0;
    for (pos, rgb) in list_rgbs.iter().enumerate() {
        if pos >= 64 {
            // First 64 are for instruction or pagination
            // Red is 255, 0 ,0 but we give some room
            if rgb[0] >= 220 && rgb[1] <= 30 && rgb[2] <= 30 {
                counter += 1; // Found one
            }
        }
    }

    let size_list = list_rgbs.len() as f64 - 64_f64;
    let percentage = counter as f64 / size_list as f64;
    let is_red = percentage > 0.9;
    return is_red;
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
    fn get_unit_test_option(size: u8) -> ExtractOptions {
        return ExtractOptions {
            video_file_path: "".to_string(),
            extracted_file_path: "".to_string(),
            fps: 30,
            width: 100,
            height: 200,
            size: size,
            algo: AlgoFrame::RGB,
            show_progress: false,
        };
    }

    #[test]
    fn test_frame_to_data_method_rgb_different_samerow() {
        let size = 1;
        let size_frame = map_to_size(100, 64); // 64 pixels for instruction (64 bits) and 3 pixels of data (9 values) =  2 irrelevantS pixel on the first row
        let mut frame = VideoFrame::new(100, 64);
        let (x, y) = frame.write_pagination(0, 0, &1, size);

        // Write 9 bytes
        frame.write(10, 20, 30, x, y, size);
        frame.write(40, 50, 60, x + size as u16, y, size);
        frame.write(70, 80, 90, x + size as u16 * 2, y, size); // Irrelevant, because instruction specify 6 not 9

        // Act
        let result = frame_to_data_method_rgb(&frame, size_frame, &get_unit_test_option(size));
        assert_eq!(
            result.bytes.len(),
            (100 * 64 - (64 * (size as usize * size as usize))) / (size as usize * size as usize)
                * 3
        );
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        assert_eq!(result.bytes[6], 70);
        assert_eq!(result.bytes[7], 80);
        assert_eq!(result.bytes[8], 90);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_rgb_different_row() {
        let size = 1;
        let size_frame = map_to_size(64, 64); // 64 pixels for instruction (64 bits) and 3 pixels of data (9 values) =  2 irrelevantS pixel on the first row
        let mut frame = VideoFrame::new(64, 64);

        let (x, y) = frame.write_pagination(0, 0, &1, size);
        // Write 9 bytes
        frame.write(10, 20, 30, x, y, size);
        frame.write(40, 50, 60, x + size as u16, y, size);
        frame.write(70, 80, 90, x + size as u16 * 2, y, size); // Irrelevant, because instruction specify 6 not 9

        // Act
        let result = frame_to_data_method_rgb(&frame, size_frame, &get_unit_test_option(1));
        assert_eq!(result.bytes.len(), ((64 * 64 - 64) * 3)); // -64 for the pagination
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        assert_eq!(result.bytes[6], 70);
        assert_eq!(result.bytes[7], 80);
        assert_eq!(result.bytes[8], 90);
        assert_eq!(result.is_red_frame, false);
        assert_eq!(result.pagination_or_instruction.unwrap().get_data_size(), 1);
    }

    #[test]
    fn test_frame_to_data_method_rgb_size2() {
        let size = 2;
        let size_frame = map_to_size(64, 64); // 64 pixels for instruction (64 bits) and 3 pixels of data (9 values) =  2 irrelevantS pixel on the first row
        let mut frame = VideoFrame::new(64, 64);
        let (x, y) = frame.write_pagination(0, 0, &1, size);

        // Write 9 bytes
        frame.write(10, 20, 30, x, y, size);
        frame.write(40, 50, 60, x + size as u16, y, size);
        frame.write(70, 80, 90, x + size as u16 * 2, y, size);

        // Act
        let result = frame_to_data_method_rgb(&frame, size_frame, &get_unit_test_option(size));
        assert_eq!(
            result.bytes.len(),
            (64 * 64 - (64 * (size as usize * size as usize))) / (size as usize * size as usize)
                * 3
        );
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        assert_eq!(result.bytes[6], 70);
        assert_eq!(result.bytes[7], 80);
        assert_eq!(result.bytes[8], 90);
        assert_eq!(result.is_red_frame, false);
        assert_eq!(result.pagination_or_instruction.unwrap().get_data_size(), 1);
    }

    #[test]
    fn test_frame_to_data_method_bw() {
        let size_frame = map_to_size(64, 64);
        let mut frame = VideoFrame::new(64, 64);
        let write_data = 0b0011_1011; // The byte to write into a frame

        let (x, y) = frame.write_pagination(0, 0, &1, 1);
        frame.write(0, 0, 0, x, y, 1); // White 0 bit
        frame.write(0, 0, 0, x + 1, y, 1); // White 0 bit
        frame.write(255, 255, 255, x + 2, y, 1); // Black 1 bit
        frame.write(255, 255, 255, x + 3, y, 1); // Black 1 bit
        frame.write(255, 255, 255, x + 4, y, 1); // Black 1 bit
        frame.write(0, 0, 0, 5, y, 1); // White 0 bit
        frame.write(255, 255, 255, x + 6, y, 1); // Black 1 bit
        frame.write(255, 255, 255, x + 7, y, 1); // Black 1 bit

        // Act
        let result = frame_to_data_method_bw(&frame, size_frame, &get_unit_test_option(1));
        assert_eq!(result.bytes.len(), (64 * 64 - 64) / 8); // -64 for the pagination
        assert_eq!(result.bytes[0], write_data);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw_size2() {
        let size = 2;
        let size_frame = map_to_size(128, 64);
        let frame = VideoFrame::new(128, 64);

        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instr.relevant_byte_count_in_64bits[63] = true; // 1 byte

        // Act
        let result = frame_to_data_method_bw(&frame, size_frame, &get_unit_test_option(size));
        assert_eq!(
            result.bytes.len(),
            ((128_u64 / size as u64 * (64_u64 - 2) / size as u64) / 8) as usize
        );
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw_with_processesbytes() {
        let size = 1;
        let size_frame = map_to_size(64, 64);
        let mut frame = VideoFrame::new(64, 64);
        let (_x, y) = frame.write_pagination(0, 0, &1, size);

        // Write on the second row (first was instruction since 64 bits)
        // First relevant byte (value 59)
        frame.write(0, 0, 0, 0, y, size); // White 0 bit
        frame.write(0, 0, 0, 1, y, size); // White 0 bit
        frame.write(255, 255, 255, 2, y, size); // Black 1 bit
        frame.write(255, 255, 255, 3, y, size); // Black 1 bit
        frame.write(255, 255, 255, 4, y, size); // Black 1 bit
        frame.write(0, 0, 0, 5, y, size); // White 0 bit
        frame.write(255, 255, 255, 6, y, size); // Black 1 bit
        frame.write(255, 255, 255, 7, y, size); // Black 1 bit

        // Second relevant byte (value 251)
        frame.write(255, 255, 255, 8, y, size); // Black 1 bit
        frame.write(255, 255, 255, 9, y, size); // Black 1 bit
        frame.write(255, 255, 255, 10, y, size); // Black 1 bit
        frame.write(255, 255, 255, 11, y, size); // Black 1 bit
        frame.write(255, 255, 255, 12, y, size); // Black 1 bit
        frame.write(0, 0, 0, 5, y, size); // White 0 bit
        frame.write(255, 255, 255, 14, y, size); // Black 1 bit
        frame.write(255, 255, 255, 15, y, size); // Black 1 bit

        // Third irrelevant byte (value 153)
        frame.write(255, 255, 255, 16, y, size); // Black 1 bit
        frame.write(0, 0, 0, 17, y, size); // White 0 bit
        frame.write(0, 0, 0, 18, y, size); // White 0 bit
        frame.write(255, 255, 255, 19, y, size); // Black 1 bit
        frame.write(255, 255, 255, 20, y, size); // Black 1 bit
        frame.write(0, 0, 0, 21, y, size); // White 0 bit
        frame.write(0, 0, 0, 22, y, size); // White 0 bit
        frame.write(255, 255, 255, 23, y, size); // Black 1 bit

        // Forth irrelevant byte (value 153)
        frame.write(255, 255, 255, 24, y, size); // Black 1 bit
        frame.write(0, 0, 0, 25, y, size); // White 0 bit
        frame.write(0, 0, 0, 26, y, size); // White 0 bit
        frame.write(255, 255, 255, 27, y, size); // Black 1 bit
        frame.write(255, 255, 255, 28, y, size); // Black 1 bit
        frame.write(0, 0, 0, 29, y, size); // White 0 bit
        frame.write(0, 0, 0, 30, y, size); // White 0 bit
        frame.write(255, 255, 255, 31, y, size); // Black 1 bit

        // Act
        let result = frame_to_data_method_bw(&frame, size_frame, &get_unit_test_option(1));
        assert_eq!(result.bytes.len(), (64 * 64 - 64) / 8); // The frame should have only the relevant byte #1 and #2, -64 for the pagination
        assert_eq!(result.bytes[0], 59);
        assert_eq!(result.bytes[1], 251);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn text_check_red_frame_white() {
        let mut rgbs = Vec::new();
        for i in 0..64 {
            rgbs.push(vec![0, 0, i]); // Pagination random
        }
        rgbs.push(vec![0, 0, 0]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn text_check_red_frame_black() {
        let mut rgbs = Vec::new();
        for i in 0..64 {
            rgbs.push(vec![0, 0, i]); // Pagination random
        }
        rgbs.push(vec![255, 255, 255]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn text_check_red_frame_red() {
        let mut rgbs = Vec::new();
        for i in 0..64 {
            rgbs.push(vec![0, 0, i]); // Pagination random
        }
        rgbs.push(vec![255, 0, 0]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, true)
    }

    #[test]
    fn text_check_red_frame_almost_red() {
        let mut rgbs = Vec::new();
        for i in 0..64 {
            rgbs.push(vec![0, 0, i]); // Pagination random
        }
        rgbs.push(vec![245, 5, 10]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, true)
    }

    #[test]
    fn text_check_red_frame_too_far_from_red() {
        let mut rgbs = Vec::new();
        for i in 0..64 {
            rgbs.push(vec![0, 0, i]); // Pagination random
        }
        rgbs.push(vec![245, 45, 10]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn test_frame_to_data_method_bw_pagination() {
        let size = 1;
        let page_number = 5;
        let size_frame = map_to_size(64, 10);
        let mut frame = VideoFrame::new(64, 10);
        let write_data = 0b0011_1011; // The byte to write into a frame

        // Write on the first row (first was instruction)
        let (x, _y) = frame.write_pagination(0, 0, &page_number, size);
        frame.write(0, 0, 0, x, 1, 1); // White 0 bit
        frame.write(0, 0, 0, x + 1, 1, 1); // White 0 bit
        frame.write(255, 255, 255, x + 2, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, x + 3, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, x + 4, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 5, x + 1, 1); // White 0 bit
        frame.write(255, 255, 255, x + 6, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, x + 7, 1, 1); // Black 1 bit

        // Act
        let options = get_unit_test_option(1);
        let result = frame_to_data_method_bw(&frame, size_frame, &options);
        assert_eq!(result.bytes.len(), (64 * 10 - 64) / 8);
        assert_eq!(
            result.pagination_or_instruction.unwrap().get_data_size(),
            page_number
        ); // Check if we can read back the page number
        assert_eq!(result.bytes[0], write_data); // Check if we can read the byte we wrote after the pagination
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_rgb_pagination() {
        let size = 1;
        let page_number = 5;
        let size_frame = map_to_size(64, 3);
        let mut frame = VideoFrame::new(64, 3);

        // Write on the second row (first was instruction)
        let (_x, _y) = frame.write_pagination(0, 0, &page_number, size); // Y=1
        for i in 0..64 {
            frame.write(10, 20, 30, i, 1, size); // Y=1
            frame.write(10, 20, 30, i, 2, size); // Y=2
        }

        // Act
        let options = get_unit_test_option(1);
        let result = frame_to_data_method_rgb(&frame, size_frame, &options);
        assert_eq!(result.bytes.len(), 384); // Loop 64 times 3 bytes = 192 x 2 rows =
        assert_eq!(
            result.pagination_or_instruction.unwrap().get_data_size(),
            page_number
        ); // Check if we can read back the page number
        assert_eq!(result.bytes[0], 10); // Check if we can read the byte we wrote after the pagination
        assert_eq!(result.bytes[1], 20); // Check if we can read the byte we wrote after the pagination
        assert_eq!(result.bytes[2], 30); // Check if we can read the byte we wrote after the pagination
        assert_eq!(result.is_red_frame, false);
    }

}
