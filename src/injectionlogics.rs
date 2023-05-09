use opencv::{
    core::Size,
    videoio::{VideoWriter, VideoWriterTrait},
};
use std::fs;

use crate::{
    bitlogics::{get_bit_at, get_rgb_for_bit},
    injectionextraction::NULL_CHAR,
    instructionlogics::Instruction,
    options::{self, AlgoFrame, InjectOptions},
    videoframe::VideoFrame,
};

pub fn file_to_data(options: &InjectOptions) -> Vec<u8> {
    fs::read(&options.file_path).unwrap_or_else(|err| {
        panic!(
            "Unable to read file: {} with error: {}",
            &options.file_path, err
        )
    })
}
use indicatif::ProgressBar;
use pretty_bytes_rust::pretty_bytes;

/// Create a starting frame to indicate that we are starting the transmission of the data
///
/// Needed because the source will play the video with all the data in loop. The consumer
/// needs to read the stream of video and catch the first frame of data (the one after this
/// starting frame) until it sees the same starting frame again.
pub fn create_starting_frame(inject_options: &InjectOptions) -> VideoFrame {
    let mut frame = VideoFrame::new(inject_options.width, inject_options.height);
    for y in (0..inject_options.height).step_by(usize::from(inject_options.size)) {
        for x in (0..inject_options.width).step_by(usize::from(inject_options.size)) {
            // Set a full red frame to indicate that the next one is the start of the data
            let r = 255;
            let g = 0;
            let b = 0;
            frame.write(r, g, b, x, y, inject_options.size);
        }
    }
    frame
}

pub fn data_to_frames(
    inject_options: &InjectOptions,
    data: Vec<u8>,
    instruction: Instruction,
) -> Vec<VideoFrame> {
    if inject_options.algo == AlgoFrame::RGB {
        data_to_frames_method_rgb(inject_options, data, instruction)
    } else {
        data_to_frames_method_bw(inject_options, data, instruction)
    }
}

/// Move data into many frames of the video using RGB
/// Each data (character) is going in to a R or G or B.
/// It means that a pixel can hold 3 characters of a file.
fn data_to_frames_method_rgb(
    inject_options: &InjectOptions,
    data: Vec<u8>,
    instruction: Instruction,
) -> Vec<VideoFrame> {
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut data_index = 0;

    if u32::from(inject_options.width) * u32::from(inject_options.height) < 64 {
        panic!(
            "Instruction must fit in the first frame. Frame size: {}",
            u32::from(inject_options.width) * u32::from(inject_options.height)
        );
    }

    let total_data = data.len();
    let total_bytes: f64 = total_data as f64 + 64_f64; // Instruction
    let total_expected_frame = (total_bytes
        / (f64::from(inject_options.width) * f64::from(inject_options.height)
            / f64::from(inject_options.size * inject_options.size)
            / 8.0))
        .ceil();
    let pb = ProgressBar::new(total_expected_frame as u64);
    if inject_options.show_progress {
        println!(
            "Inserting {} into {} frames",
            pretty_bytes(total_bytes as u64, None),
            total_expected_frame
        );
    }

    let mut need_to_write_instruction = true;
    while data_index < total_data {
        let mut x: u16 = 0;
        let mut y: u16 = 0;

        // Create a single frame
        let mut frame = VideoFrame::new(inject_options.width, inject_options.height);

        // Write instruction only once, on the first frame
        if need_to_write_instruction {
            let current_pos = frame.write_instruction(&instruction, inject_options.size);
            x = current_pos.0;
            y = current_pos.1;
            need_to_write_instruction = false;
        }
        while y < inject_options.height {
            while x < inject_options.width {
                // Step 2: For each pixel of the frame, extract a byte of the vector
                // If there is not pixel, we keep filling with the NULL_CHAR to complete`
                // the frame
                let r = if data_index < total_data {
                    data[data_index]
                } else {
                    NULL_CHAR
                };
                let g = if data_index + 1 < total_data {
                    data[data_index + 1]
                } else {
                    NULL_CHAR
                };
                let b = if data_index + 2 < total_data {
                    data[data_index + 2]
                } else {
                    NULL_CHAR
                };
                // Step 3: Apply the pixel to the frame
                frame.write(r, g, b, x, y, inject_options.size);
                data_index += 3; // 3 because R, G, B

                x += inject_options.size as u16;
            }
            y += inject_options.size as u16;
            x = 0;
        }
        if inject_options.show_progress {
            pb.inc(1);
        }
        frames.push(frame);
    } // Loop until the frame is full or that there is no mode byte
    if inject_options.show_progress {
        pb.finish_with_message("done");
    }
    frames
}

/// Move data into many frames of the video using bit and black and white
/// Each data (character) is going to 8 pixels. Each pixel is black (0) or white (1)
/// It means that a pixel alone represent 1/8 of a byte (a character).
fn data_to_frames_method_bw(
    inject_options: &InjectOptions,
    data: Vec<u8>,
    instruction: Instruction,
) -> Vec<VideoFrame> {
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut data_index: usize = 0;
    let mut bit_index: u8 = 7;

    if u32::from(inject_options.width) * u32::from(inject_options.height) < 64 {
        panic!("Instruction must fit in the first frame");
    }

    let total_size = u32::from(inject_options.width) * u32::from(inject_options.height)
        / u32::from(inject_options.size);
    if total_size < 8 {
        panic!("The frame size must be at least big enough to accept a single character");
    }

    let total_data = data.len();
    let total_bytes = total_data as f64 + 64_f64; // Instruction
    let total_expected_frame = (total_bytes * 8.0
        / (f64::from(inject_options.width) * f64::from(inject_options.height)
            / f64::from(inject_options.size)
            / f64::from(inject_options.size)))
    .ceil();
    let pb = ProgressBar::new(total_expected_frame as u64);
    if inject_options.show_progress {
        println!(
            "Inserting {} into {} frames",
            pretty_bytes(total_bytes as u64, None),
            total_expected_frame
        );
    }

    let mut need_to_write_instruction = true;
    let vertical = inject_options.height - inject_options.size as u16;
    let horizontal = inject_options.width - inject_options.size as u16;
    while data_index < total_data {
        let mut x: u16 = 0;
        let mut y: u16 = 0;

        // Create a single frame
        let mut frame = VideoFrame::new(inject_options.width, inject_options.height);

        // Write instruction only once, on the first frame
        if need_to_write_instruction {
            let current_pos = frame.write_instruction(&instruction, inject_options.size);
            x = current_pos.0;
            y = current_pos.1;
            need_to_write_instruction = false;
        }
        while y <= vertical {
            while x <= horizontal {
                // For each pixel of the frame, extract a bit of the active byte of the vector
                if data_index < total_data {
                    // Still have a char, we get the bit we are at of that char
                    let bit = get_bit_at(data[data_index], bit_index);
                    let (r, g, b) = get_rgb_for_bit(bit);
                    frame.write(r, g, b, x, y, inject_options.size);
                } else {
                    // If there is no char, we keep filling with the NULL_CHAR char to complete frame
                    let bit = get_bit_at(NULL_CHAR, bit_index);
                    let (r, g, b) = get_rgb_for_bit(bit);
                    frame.write(r, g, b, x, y, inject_options.size);
                }

                // Rotate from 0 to 7 inclusively
                // Change character only when all bit of the current one is done
                if bit_index > 0 {
                    bit_index -= 1; // Decrease only the bit because we have not yet written all the bit of the char (8 bits in 1 byte = 1 char)
                } else {
                    data_index += 1; // 1 because increase 1 character at a time
                    bit_index = 7; // Reset
                }
                x += inject_options.size as u16;
            }
            y += inject_options.size as u16;
            x = 0;
        }
        if inject_options.show_progress {
            pb.inc(1);
        }
        frames.push(frame);
    } // Loop until the frame is full or that there is no mode byte
    if inject_options.show_progress {
        pb.finish_with_message("done");
    }
    frames
}

pub fn frames_to_video(options: InjectOptions, frames: Vec<VideoFrame>) {
    let frame_size = Size {
        height: options.height as i32,
        width: options.width as i32,
    };
    //Fourcc is a code for video codecs, trying to use a lossless one
    // See list of codec here: https://learn.fotoware.com/On-Premises/Getting_started/Metadata_in_the_FotoWare_system/04_Operators_to_search_in_specific_fields/FourCC_codes
    // Careful, codec and file extension must match

    //let fourcc = VideoWriter::fourcc('p', 'n', 'g', ' '); // Required when using RGB because lossless compression
    //let fourcc =  VideoWriter::fourcc('j', 'p', 'e', 'g');
    //let fourcc = VideoWriter::fourcc('H','2','6','4');
    //let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v');
    //let fourcc = VideoWriter::fourcc('a', 'v', 'c', '1');
    let fourcc = if options.algo == options::AlgoFrame::RGB {
        //VideoWriter::fourcc('p', 'n', 'g', ' ')
        VideoWriter::fourcc('m', 'p', '4', 'v')
    } else {
        VideoWriter::fourcc('m', 'p', '4', 'v')
    };
    let total_frames = frames.len() as u64;
    if options.show_progress {
        println!("Frames to video");
    }
    let pb = ProgressBar::new(total_frames);

    match fourcc {
        Ok(fourcc_unwrapped) => {
            let video = VideoWriter::new(
                options.output_video_file.as_str(),
                fourcc_unwrapped,
                options.fps.into(),
                frame_size,
                true,
            );
            match video {
                Ok(mut video_unwrapped) => {
                    for frame in frames {
                        let image = frame.image;
                        video_unwrapped
                            .write(&image)
                            .expect("All frame must be written");
                        if options.show_progress {
                            pb.inc(1);
                        }
                    }
                    let result_release = video_unwrapped.release();
                    match result_release {
                        Ok(_s) => {
                            if options.show_progress {
                                pb.finish_with_message("done");
                                println!("Video saved:{}", options.output_video_file.as_str());
                            }
                        }
                        Err(error_release) => {
                            println!("Error saving the video");
                            println!("{:?}", error_release);
                        }
                    }
                }
                Err(error_video) => {
                    println!("Error with video writer: {:?}", error_video)
                }
            }
        }
        Err(error) => {
            println!("Error: {:?}", error)
        }
    }
}

#[cfg(test)]
mod injectionlogics_tests {
    use opencv::prelude::MatTraitConst;

    use super::*;

    #[test]
    fn test_data_to_frames_short_message_bigger_frame_expect_1_frame() {
        let instruction = Instruction::new(100);
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 10,
            width: 10,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
            show_progress: false,
        };
        // Text: This is a test
        let frames = data_to_frames_method_rgb(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );
        assert_eq!(frames.len(), 1)
    }

    #[test]
    fn test_data_to_frames_method_rgb_short_message_shorter_frame_expect_2_frame() {
        let instruction = Instruction::new(100);
        // 8x8 = 64 = instruction = 1 frame. Data is 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 8,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
            show_progress: false,
        };
        // Text: This is a test, 14 chars
        let frames = data_to_frames_method_rgb(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );
        assert_eq!(frames.len(), 2)
    }

    #[test]
    fn test_data_to_frames_method_rgb_short_message_remaining_color_instruction() {
        let instruction = Instruction::new(3465345363523452834); // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
                                                                 // 8x8 = 64 with 3 colors = 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 8,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
            show_progress: false,
        };
        // Text: This
        let frames = data_to_frames_method_rgb(&options, vec![54, 68, 69, 73], instruction);
        let first_frame = &frames[0];
        let color = first_frame.read_coordinate_color(0, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        let color = first_frame.read_coordinate_color(1, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        let color = first_frame.read_coordinate_color(2, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
                                  // and so on for 64 pixels
        let second_frame = &frames[1];
        let color = second_frame.read_coordinate_color(0, 0);
        assert_eq!(color.r, 54); // 1st content
        assert_eq!(color.g, 68); // 2nd content
        assert_eq!(color.b, 69); // 3rd
        let color = second_frame.read_coordinate_color(1, 0);
        assert_eq!(color.r, 73); // 4th
    }

    #[test]
    #[should_panic]
    fn test_data_to_frames_method_rgb_frame_too_small() {
        let instruction = Instruction::new(3465345363523452834); // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
                                                                 // 8x8 = 64 with 3 colors = 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 7, // 8x7 = 56... smaller than 64
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
            show_progress: false,
        };
        data_to_frames_method_rgb(&options, vec![54, 68, 69, 73], instruction);
    }

    #[test]
    fn test_create_starting_frame() {
        let w: i32 = 10;
        let h: i32 = 10;
        let inject_options = &InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            width: w as u16,
            height: h as u16,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
            show_progress: false,
        };
        let result = create_starting_frame(inject_options);
        for x in 0..w {
            for y in 0..h {
                let bgr = result.image.at_2d::<opencv::core::Vec3b>(y, x).unwrap();
                assert_eq!(bgr[2], 255);
                assert_eq!(bgr[1], 0);
                assert_eq!(bgr[0], 0);
            }
        }
    }

    #[test]
    fn test_data_to_frames_method_bw_frame_size() {
        let instruction = Instruction::new(100); // 00000000 ...  01100100
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 8,
            size: 1,
            algo: crate::options::AlgoFrame::BW,
            show_progress: false,
        };
        // Text: This is a test, 14 chars = 14 bytes = 14*8bit =112 pixel
        // Instruction is 64 bits = 64 pixel
        // Total pixel: 176
        // 1 frame is 8x48 pixel = 64
        // 176/64 = 3 frames
        let frames = data_to_frames_method_bw(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );
        assert_eq!(frames.len(), 3);

        let frame = &frames[0];
        let color = frame.read_coordinate_color(0, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
    }

    #[test]
    fn test_data_to_frames_method_bw_instruction() {
        let instruction = Instruction::new(100); // 00000000 ...  0110 0100
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 100,
            size: 1,
            algo: crate::options::AlgoFrame::BW,
            show_progress: false,
        };
        // Instruction is 64 bits = 64 pixel
        let frames = data_to_frames_method_bw(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );

        let frame = &frames[0];
        let mut color = frame.read_coordinate_color(56, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(57, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(58, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(59, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(60, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(61, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(62, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(63, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction

        color = frame.read_coordinate_color(64, 0);
        assert_eq!(color.r, 0); // 54
        assert_eq!(color.g, 0); // 54
        assert_eq!(color.b, 0); // 54
        color = frame.read_coordinate_color(65, 0);
        assert_eq!(color.r, 0); // 54
        assert_eq!(color.g, 0); // 54
        assert_eq!(color.b, 0); // 54
        color = frame.read_coordinate_color(66, 0);
        assert_eq!(color.r, 255); // 54
        assert_eq!(color.g, 255); // 54
        assert_eq!(color.b, 255); // 54
        color = frame.read_coordinate_color(67, 0);
        assert_eq!(color.r, 255); // 54
        assert_eq!(color.g, 255); // 54
        assert_eq!(color.b, 255); // 54
        color = frame.read_coordinate_color(68, 0);
        assert_eq!(color.r, 0); // 54
        assert_eq!(color.g, 0); // 54
        assert_eq!(color.b, 0); // 54
        color = frame.read_coordinate_color(69, 0);
        assert_eq!(color.r, 255); // 54
        assert_eq!(color.g, 255); // 54
        assert_eq!(color.b, 255); // 54
        color = frame.read_coordinate_color(70, 0);
        assert_eq!(color.r, 255); // 54
        assert_eq!(color.g, 255); // 54
        assert_eq!(color.b, 255); // 54
        color = frame.read_coordinate_color(71, 0);
        assert_eq!(color.r, 0); // 54
        assert_eq!(color.g, 0); // 54
        assert_eq!(color.b, 0); // 54
    }

    #[test]
    fn test_data_to_frames_method_bw_size10() {
        let instruction = Instruction::new(100);
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 150,
            width: 150,
            size: 10,
            algo: crate::options::AlgoFrame::BW,
            show_progress: false,
        };
        // Text: This is a test, 14 chars = 14 bytes = 14*8bit =112 pixel
        // Instruction is 64 bits = 64 pixel
        // Size is 10
        // Total pixel: 176 * (10x10) 17 600 total pixel
        // 1 frame is 150x150 pixel = 22 500 pixels
        // 22 500/17 600 < 1 = 1 frame
        let frames = data_to_frames_method_bw(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );
        assert_eq!(frames.len(), 1);
    }

    #[test]
    fn test_data_to_frames_method_bw_instruction_size2() {
        let instruction = Instruction::new(100); // 00000000 ...  0110 0100
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 200,
            size: 2,
            algo: crate::options::AlgoFrame::BW,
            show_progress: false,
        };
        // Instruction is 64 bits = 64 pixel
        let frames = data_to_frames_method_bw(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );

        let frame = &frames[0];
        let mut color = frame.read_coordinate_color(112, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(113, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(112, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(113, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction

        color = frame.read_coordinate_color(114, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(114, 1);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(115, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(115, 1);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction

        color = frame.read_coordinate_color(116, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(116, 1);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(117, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(117, 1);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction

        color = frame.read_coordinate_color(118, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(118, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(119, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(119, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction

        color = frame.read_coordinate_color(120, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(120, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(121, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(121, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction

        color = frame.read_coordinate_color(122, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(122, 1);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(123, 0);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction
        color = frame.read_coordinate_color(123, 1);
        assert_eq!(color.r, 255); // Instruction
        assert_eq!(color.g, 255); // Instruction
        assert_eq!(color.b, 255); // Instruction

        color = frame.read_coordinate_color(124, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(124, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(125, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(125, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction

        color = frame.read_coordinate_color(126, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(126, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(127, 0);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction
        color = frame.read_coordinate_color(127, 1);
        assert_eq!(color.r, 0); // Instruction
        assert_eq!(color.g, 0); // Instruction
        assert_eq!(color.b, 0); // Instruction

        // color = frame.read_coordinate_color(64, 0);
        // assert_eq!(color.r, 0); // 54
        // assert_eq!(color.g, 0); // 54
        // assert_eq!(color.b, 0); // 54
        // color = frame.read_coordinate_color(65, 0);
        // assert_eq!(color.r, 0); // 54
        // assert_eq!(color.g, 0); // 54
        // assert_eq!(color.b, 0); // 54
        // color = frame.read_coordinate_color(66, 0);
        // assert_eq!(color.r, 255); // 54
        // assert_eq!(color.g, 255); // 54
        // assert_eq!(color.b, 255); // 54
        // color = frame.read_coordinate_color(67, 0);
        // assert_eq!(color.r, 255); // 54
        // assert_eq!(color.g, 255); // 54
        // assert_eq!(color.b, 255); // 54
        // color = frame.read_coordinate_color(68, 0);
        // assert_eq!(color.r, 0); // 54
        // assert_eq!(color.g, 0); // 54
        // assert_eq!(color.b, 0); // 54
        // color = frame.read_coordinate_color(69, 0);
        // assert_eq!(color.r, 255); // 54
        // assert_eq!(color.g, 255); // 54
        // assert_eq!(color.b, 255); // 54
        // color = frame.read_coordinate_color(70, 0);
        // assert_eq!(color.r, 255); // 54
        // assert_eq!(color.g, 255); // 54
        // assert_eq!(color.b, 255); // 54
        // color = frame.read_coordinate_color(71, 0);
        // assert_eq!(color.r, 0); // 54
        // assert_eq!(color.g, 0); // 54
        // assert_eq!(color.b, 0); // 54
    }

    #[test]
    fn test_data_to_frames_method_bw_with_restriction_size() {
        let instruction = Instruction::new(3465345363523452834); // 0011000000010111011000010011111101111000110111001011111110100010
                                                                 // 2x2 = 4 bits per frame. With 4 chars we have 4 = 32bits. 32/4 = 8 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 64,
            width: 64, // First line will be full instruction (64 bits)
            size: 1,
            algo: crate::options::AlgoFrame::BW,
            show_progress: false,
        };
        // Text: This
        // 84 104 105 115
        // 01010100 01101000 01101001 01110011
        let frames = data_to_frames_method_bw(&options, vec![84, 104, 105, 115], instruction);

        // Assert what we wrote
        let first_frame = &frames[0];
        // First random instruction checks
        let mut color = first_frame.read_coordinate_color(0, 0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(1, 0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(2, 0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);

        color = first_frame.read_coordinate_color(62, 0);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(63, 0);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);

        // First Char (after instruction)
        let mut color = first_frame.read_coordinate_color(0, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(1, 1);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(2, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(3, 1);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(4, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(5, 1);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(6, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(7, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        // Second Char
        let mut color = first_frame.read_coordinate_color(8, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(9, 1);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(10, 1);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(11, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(12, 1);
        assert_eq!(color.r, 255);
        assert_eq!(color.g, 255);
        assert_eq!(color.b, 255);
        color = first_frame.read_coordinate_color(13, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(14, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
        color = first_frame.read_coordinate_color(15, 1);
        assert_eq!(color.r, 0);
        assert_eq!(color.g, 0);
        assert_eq!(color.b, 0);
    }

    #[test]
    #[should_panic]
    fn test_data_to_frames_method_bw_frame_too_small() {
        let instruction = Instruction::new(100);
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 8,
            width: 7,
            size: 1,
            algo: crate::options::AlgoFrame::BW,
            show_progress: false,
        };
        // Text: This is a test, 14 chars = 14 bytes = 14*8bit =112 pixel
        // Instruction is 64 bits = 64 pixel
        // Total pixel: 176
        // 1 frame is 8x48 pixel = 64
        // 176/64 = 3 frames
        let frames = data_to_frames_method_bw(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
            instruction,
        );
        assert_eq!(frames.len(), 3);
    }
}
