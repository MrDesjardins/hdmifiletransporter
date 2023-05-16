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

struct FrameBytesInfo {
    pub bytes: Vec<u8>,
    pub is_red_frame: bool,
    pub instruction: Option<Instruction>,
    pub pagination: Option<u64>,
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
    let mut previous_frame_checksum = 0;
    let total_expected_frame = frames.len() as u64;
    let pb = ProgressBar::new(total_expected_frame);
    let mut instruction: Option<Instruction> = None;
    let mut bytes_processes_count = 0;
    let mut frame_counter = 0;
    if extract_options.show_progress {
        println!("Initial Frames count: {}", total_expected_frame);
    }
    for frame in frames.iter() {
        let frame_data = if extract_options.algo == AlgoFrame::RGB {
            frame_to_data_method_rgb(
                frame,
                actual_size,
                extract_options,
                &mut instruction,
                bytes_processes_count,
                is_red_frame_found,
            )
        } else {
            frame_to_data_method_bw(
                frame,
                actual_size,
                extract_options,
                &mut instruction,
                bytes_processes_count,
                is_red_frame_found,
            )
        };
        bytes_processes_count += frame_data.bytes.len() as u64;
        // Set the instruction for all subsequent frame
        if instruction.is_none() && frame_data.instruction.is_some() {
            instruction = frame_data.instruction;
            if extract_options.show_progress {
                println!(
                    "Instruction found with data size of {}",
                    // pretty_bytes(instruction.unwrap().get_data_size() as u64, None)
                    instruction.unwrap().get_data_size()
                );
            }
        }

        if is_red_frame_found && !frame_data.is_red_frame {
            let current_frame_checksum = crc32fast::hash(frame_data.bytes.as_slice());
            if current_frame_checksum != previous_frame_checksum {
                if extract_options.show_progress {
                    pb.inc(1);
                }
                byte_data.extend(frame_data.bytes); // Between two red frames, we accumulate
                relevant_frame_count += 1;
                previous_frame_checksum = current_frame_checksum;
            }
        } else if is_red_frame_found && frame_data.is_red_frame && byte_data.len() > 0 {
            // Check length in case there is two or more red frame next to each other
            if extract_options.show_progress {
                println!("Relevant Frames count: {}", relevant_frame_count);
            }
            return byte_data; // We have all our frames
        } else if !is_red_frame_found && frame_data.is_red_frame {
            is_red_frame_found = true; // From that point, we start accumulating byte
            if extract_options.show_progress {
                println!("Red frame found at frame # {frame_counter}");
            }
        }
        frame_counter += 1;
    }
    let p = relevant_frame_count as f32 / total_expected_frame as f32;
    if extract_options.show_progress {
        pb.finish_with_message("done");
        println!(
            "Relevant Frames count: {}/{} ({:.3})",
            relevant_frame_count, total_expected_frame, p
        );
    }
    byte_data
}

/// Extract from a frame all the data. Once the end of file character is found, the loop is done.
///
/// Pass the instruction that might be coming from a previous frame (or none if first frame)
///
/// # Source
/// https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L280
fn frame_to_data_method_rgb(
    source: &VideoFrame,
    actual_size: Size,
    options: &ExtractOptions,
    instruction: &mut Option<Instruction>,
    bytes_processes_count: u64,
    red_frame_found: bool,
) -> FrameBytesInfo {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = options.size as usize;
    let mut result = FrameBytesInfo {
        bytes: Vec::new(),
        is_red_frame: false,
        instruction: None,
        pagination: None,
    };

    let mut instruction_data = instruction.unwrap_or_else(|| Instruction {
        relevant_byte_count_in_64bits: [false; 64],
    });
    let mut instruction_bits_index = 0;
    let has_already_instruction_from_past = instruction.is_some();

    let mut rgbs = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, options.size);
            rgbs.push(vec![rgb[0], rgb[1], rgb[2]]); // Always, with or without instruction
            if red_frame_found {
                let bit_value = get_bit_from_rgb(&rgb);
                if !has_already_instruction_from_past && instruction_bits_index < 64 {
                    // Will get here only on the first frame and until we have the whole instruction message (64 bits)
                    instruction_data.relevant_byte_count_in_64bits[instruction_bits_index] =
                        bit_value;
                    instruction_bits_index += 1;
                } else {
                    let max = instruction_data.get_data_size();
                    result.bytes.push(rgb[0]);
                    if bytes_processes_count + result.bytes.len() as u64 >= max {
                        return result; // The frame has reached a point that it has no more relevant information
                    }

                    result.bytes.push(rgb[1]);
                    if bytes_processes_count + result.bytes.len() as u64 >= max {
                        return result; // The frame has reached a point that it has no more relevant information
                    }

                    result.bytes.push(rgb[2]);
                    if bytes_processes_count + result.bytes.len() as u64 >= max {
                        return result; // The frame has reached a point that it has no more relevant information
                    }
                }
            }
        }
    }
    let is_red_frame = check_red_frame(&rgbs);
    result.is_red_frame = is_red_frame;
    result
}

/// Extract from a frame all the data
/// Pass the instruction that might be coming from a previous frame (or none if first frame)
fn frame_to_data_method_bw(
    source: &VideoFrame,
    actual_size: Size,
    options: &ExtractOptions,
    instruction: &mut Option<Instruction>,
    bytes_processes_count: u64,
    red_frame_found: bool,
) -> FrameBytesInfo {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = options.size as usize;
    let mut pagination_data = instruction.unwrap_or_else(|| Instruction {
        relevant_byte_count_in_64bits: [false; 64],
    });
    let mut pagination_bits_index = 0;

    let mut result = FrameBytesInfo {
        bytes: Vec::new(),
        is_red_frame: false,
        instruction: None,
        pagination: None,
    };
    let mut bit_index: u8 = 7;
    let mut data: u8 = 0;
    let mut instruction_data = instruction.unwrap_or_else(|| Instruction {
        relevant_byte_count_in_64bits: [false; 64],
    });
    let mut instruction_bits_index = 0;
    let has_already_instruction_from_past = instruction.is_some();
    let mut rgbs = Vec::new();

    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, options.size);
            rgbs.push(vec![rgb[0], rgb[1], rgb[2]]); // Always, with or without instruction
            if red_frame_found {
                let bit_value = get_bit_from_rgb(&rgb);
                if !has_already_instruction_from_past && instruction_bits_index < 64 {
                    // Will get here only on the first frame and until we have the whole instruction message (64 bits)
                    instruction_data.relevant_byte_count_in_64bits[instruction_bits_index] =
                        bit_value;
                    instruction_bits_index += 1;
                } else {
                    // We have not yet found the pagination data (if the option requires it)
                    if options.pagination && pagination_bits_index < 64 {
                        pagination_data.relevant_byte_count_in_64bits[pagination_bits_index] =
                            bit_value;
                        pagination_bits_index += 1;
                    } else {
                        mutate_byte(&mut data, bit_value, bit_index);
                        bit_index = if bit_index == 0 { 7 } else { bit_index - 1 };
                        let max = instruction_data.get_data_size();
                        if (bytes_processes_count + result.bytes.len() as u64) <= max
                            && bit_index == 7
                        {
                            result.bytes.push(data);
                            data = 0; // Reset, next character needs to accumulate 8 bits
                        }
                        if bytes_processes_count + result.bytes.len() as u64 == max {
                            mutate_frame(&mut result, &rgbs, &pagination_data);
                            return result; // The frame has reached a point that it has no more relevant information
                        }
                    }
                }
                if !has_already_instruction_from_past && instruction_bits_index == 63 {
                    *instruction = Some(instruction_data); // Send it back for subsequence frames to use
                    if options.show_progress {
                        println!(
                            "Instruction found with data size of: {}",
                            pretty_bytes(instruction_data.get_data_size() as u64, None),
                        )
                    }
                }
            }
        }
    }
    mutate_frame(&mut result, &rgbs, &pagination_data);
    result
}

fn mutate_frame(frame: &mut FrameBytesInfo, rgbs: &Vec<Vec<u8>>, pagination_data: &Instruction) {
    let is_red_frame = check_red_frame(rgbs);
    frame.is_red_frame = is_red_frame;
    frame.pagination = Some(pagination_data.get_data_size());
}

/// Check if the list of rgbs are all redish
/// The list should be the content of a single frame
pub fn check_red_frame(list_rgbs: &Vec<Vec<u8>>) -> bool {
    let mut counter = 0;
    for rgb in list_rgbs.iter() {
        // Red is 255, 0 ,0 but we give some room
        if rgb[0] >= 220 && rgb[1] <= 30 && rgb[2] <= 30 {
            counter += 1; // Found one
        }
    }

    let size_list = list_rgbs.len() as f64;
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
            pagination: false,
        };
    }
    #[test]
    fn test_frame_to_data_method_rgb_different_samerow() {
        let size = map_to_size(100, 64); // 64 pixels for instruction (64 bits) and 3 pixels of data (9 values) =  2 irrelevantS pixel on the first row
        let mut frame = VideoFrame::new(100, 64);
        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        // 0000...00000110 = Tell that we want 6 relevant bytes
        instr.relevant_byte_count_in_64bits[61] = true;
        instr.relevant_byte_count_in_64bits[62] = true;
        instr.relevant_byte_count_in_64bits[63] = false;
        frame.write_instruction(&instr, 1);

        // Write 9 bytes
        frame.write(10, 20, 30, 64, 0, 1);
        frame.write(40, 50, 60, 65, 0, 1);
        frame.write(70, 80, 90, 66, 0, 1); // Irrelevant, because instruction specify 6 not 9

        // Act
        let mut instruction_from_frame: Option<Instruction> = None;
        let result = frame_to_data_method_rgb(
            &frame,
            size,
            &get_unit_test_option(1),
            &mut instruction_from_frame,
            0,
            true,
        );
        assert_eq!(result.bytes.len(), 6);
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        // -> Below does not exist since in the instruction we marked to have only 6 relevants!
        // assert_eq!(result.bytes[6], 70);
        // assert_eq!(result.bytes[7], 80);
        // assert_eq!(result.bytes[8], 90);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_rgb_different_row() {
        let size = map_to_size(64, 64); // 64 pixels for instruction (64 bits) and 3 pixels of data (9 values) =  2 irrelevantS pixel on the first row
        let mut frame = VideoFrame::new(64, 64);
        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        // 0000...00000110 = Tell that we want 6 relevant bytes
        instr.relevant_byte_count_in_64bits[61] = true;
        instr.relevant_byte_count_in_64bits[62] = true;
        instr.relevant_byte_count_in_64bits[63] = false;
        frame.write_instruction(&instr, 1);

        // Write 9 bytes
        frame.write(10, 20, 30, 0, 1, 1);
        frame.write(40, 50, 60, 1, 1, 1);
        frame.write(70, 80, 90, 2, 1, 1); // Irrelevant, because instruction specify 6 not 9

        // Act
        let mut instruction_from_frame: Option<Instruction> = None;
        let result = frame_to_data_method_rgb(
            &frame,
            size,
            &get_unit_test_option(1),
            &mut instruction_from_frame,
            0,
            true,
        );
        assert_eq!(result.bytes.len(), 6);
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        // -> Below does not exist since in the instruction we marked to have only 6 relevants!
        // assert_eq!(result.bytes[6], 70);
        // assert_eq!(result.bytes[7], 80);
        // assert_eq!(result.bytes[8], 90);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_rgb_size2() {
        let size = map_to_size(128, 64); // 64 pixels for instruction (64 bits) and 3 pixels of data (9 values) =  2 irrelevantS pixel on the first row
        let mut frame = VideoFrame::new(128, 64);
        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        // 0000...00000110 = Tell that we want 6 relevant bytes
        instr.relevant_byte_count_in_64bits[61] = true;
        instr.relevant_byte_count_in_64bits[62] = true;
        instr.relevant_byte_count_in_64bits[63] = false;
        frame.write_instruction(&instr, 2);

        // Write 9 bytes
        frame.write(10, 20, 30, 0, 2, 2);
        frame.write(40, 50, 60, 2, 2, 2);
        frame.write(70, 80, 90, 4, 2, 2); // Irrelevant, because instruction specify 6 not 9

        // Act
        let mut instruction_from_frame: Option<Instruction> = None;
        let result = frame_to_data_method_rgb(
            &frame,
            size,
            &get_unit_test_option(2),
            &mut instruction_from_frame,
            0,
            true,
        );
        assert_eq!(result.bytes.len(), 6);
        assert_eq!(result.bytes[0], 10);
        assert_eq!(result.bytes[1], 20);
        assert_eq!(result.bytes[2], 30);
        assert_eq!(result.bytes[3], 40);
        assert_eq!(result.bytes[4], 50);
        assert_eq!(result.bytes[5], 60);
        // -> Below does not exist since in the instruction we marked to have only 6 relevants!
        // assert_eq!(result.bytes[6], 70);
        // assert_eq!(result.bytes[7], 80);
        // assert_eq!(result.bytes[8], 90);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw() {
        let size = map_to_size(64, 64);
        let mut frame = VideoFrame::new(64, 64);
        let write_data = 0b0011_1011; // The byte to write into a frame

        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instr.relevant_byte_count_in_64bits[63] = true;
        frame.write_instruction(&instr, 1);

        // Write on the second row (first was instruction)
        frame.write(0, 0, 0, 0, 1, 1); // White 0 bit
        frame.write(0, 0, 0, 1, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 2, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 3, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 4, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 5, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 6, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 7, 1, 1); // Black 1 bit

        // Act
        let mut instruction_from_frame: Option<Instruction> = None;
        let result = frame_to_data_method_bw(
            &frame,
            size,
            &get_unit_test_option(1),
            &mut instruction_from_frame,
            0,
            true,
        );
        assert_eq!(result.bytes.len(), 1); // Only 1 byte found, even if the frame can have 8 bytes
        assert_eq!(result.bytes[0], write_data);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw_size2() {
        let size = map_to_size(128, 64);
        let mut frame = VideoFrame::new(128, 64);

        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instr.relevant_byte_count_in_64bits[63] = true; // 1 byte
        frame.write_instruction(&instr, 2);

        // Write on the second row (first was instruction)
        frame.write(0, 0, 0, 0, 2, 2); // White 0 bit
        frame.write(0, 0, 0, 2, 2, 2); // White 0 bit
        frame.write(255, 255, 255, 4, 2, 2); // Black 1 bit
        frame.write(255, 255, 255, 6, 2, 2); // Black 1 bit
        frame.write(255, 255, 255, 8, 2, 2); // Black 1 bit
        frame.write(0, 0, 0, 10, 2, 2); // White 0 bit
        frame.write(255, 255, 255, 12, 2, 2); // Black 1 bit
        frame.write(255, 255, 255, 14, 2, 2); // Black 1 bit

        // Act
        let mut instruction_from_frame: Option<Instruction> = None;
        let result = frame_to_data_method_bw(
            &frame,
            size,
            &get_unit_test_option(2),
            &mut instruction_from_frame,
            0,
            true,
        );
        assert_eq!(result.bytes.len(), 1); // Only 1 byte found, even if the frame can have 8 bytes
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn test_frame_to_data_method_bw_with_processesbytes() {
        let size = map_to_size(64, 64);
        let mut frame = VideoFrame::new(64, 64);

        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        // Change to 0000011 = 3 to have 3 bytes
        instr.relevant_byte_count_in_64bits[62] = true;
        instr.relevant_byte_count_in_64bits[63] = true;
        frame.write_instruction(&instr, 1);

        // Write on the second row (first was instruction since 64 bits)
        // First relevant byte (value 59)
        frame.write(0, 0, 0, 0, 1, 1); // White 0 bit
        frame.write(0, 0, 0, 1, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 2, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 3, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 4, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 5, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 6, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 7, 1, 1); // Black 1 bit

        // Second relevant byte (value 251)
        frame.write(255, 255, 255, 8, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 9, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 10, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 11, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 12, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 5, 13, 1); // White 0 bit
        frame.write(255, 255, 255, 14, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 15, 1, 1); // Black 1 bit

        // Third irrelevant byte (value 153)
        frame.write(255, 255, 255, 16, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 17, 1, 1); // White 0 bit
        frame.write(0, 0, 0, 18, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 19, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 20, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 21, 1, 1); // White 0 bit
        frame.write(0, 0, 0, 22, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 23, 1, 1); // Black 1 bit

        // Forth irrelevant byte (value 153)
        frame.write(255, 255, 255, 24, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 25, 1, 1); // White 0 bit
        frame.write(0, 0, 0, 26, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 27, 1, 1); // Black 1 bit
        frame.write(255, 255, 255, 28, 1, 1); // Black 1 bit
        frame.write(0, 0, 0, 29, 1, 1); // White 0 bit
        frame.write(0, 0, 0, 30, 1, 1); // White 0 bit
        frame.write(255, 255, 255, 31, 1, 1); // Black 1 bit

        // Act
        let mut instruction_from_frame: Option<Instruction> = None;
        let result = frame_to_data_method_bw(
            &frame,
            size,
            &get_unit_test_option(1),
            &mut instruction_from_frame,
            1,
            true,
        );
        assert_eq!(result.bytes.len(), 2); // The frame should have only the relevant byte #1 and #2
        assert_eq!(result.bytes[0], 59);
        assert_eq!(result.bytes[1], 251);
        assert_eq!(result.is_red_frame, false);
    }

    #[test]
    fn text_check_red_frame_white() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![0, 0, 0]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn text_check_red_frame_black() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![255, 255, 255]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn text_check_red_frame_red() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![255, 0, 0]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, true)
    }

    #[test]
    fn text_check_red_frame_almost_red() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![245, 5, 10]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, true)
    }

    #[test]
    fn text_check_red_frame_too_far_from_red() {
        let mut rgbs = Vec::new();
        rgbs.push(vec![245, 45, 10]);
        let result = check_red_frame(&rgbs);
        assert_eq!(result, false)
    }

    #[test]
    fn test_frame_to_data_method_bw_pagination() {
        let size = 1;
        let page_number = 5;
        let size_frame = map_to_size(64, 64);
        let mut frame = VideoFrame::new(64, 64);
        let write_data = 0b0011_1011; // The byte to write into a frame

        let mut instr = Instruction {
            relevant_byte_count_in_64bits: [false; 64],
        };
        instr.relevant_byte_count_in_64bits[63] = true;
        frame.write_instruction(&instr, 1);

        // Write on the second row (first was instruction)
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
        let mut instruction_from_frame: Option<Instruction> = Some(instr);
        let mut options = get_unit_test_option(1);
        options.pagination = true;
        let result = frame_to_data_method_bw(
            &frame,
            size_frame,
            &options,
            &mut instruction_from_frame,
            0,
            true,
        );
        assert_eq!(result.bytes.len(), 1); // Only 1 byte found, even if the frame can have 8 bytes
        assert_eq!(result.pagination.unwrap(), page_number); // Check if we can read back the page number
        assert_eq!(result.bytes[0], write_data); // Check if we can read the byte we wrote after the pagination
        assert_eq!(result.is_red_frame, false);
    }
}
