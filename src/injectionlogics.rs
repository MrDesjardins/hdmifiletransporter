use opencv::{
    core::Size,
    videoio::{VideoWriter, VideoWriterTrait},
};
use std::fs;

use crate::{
    bitlogics::{get_bit_at, get_rgb_for_bit},
    injectionextraction::EOF_CHAR,
    options::{AlgoFrame, InjectOptions},
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

pub fn data_to_frames(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    if inject_options.algo == AlgoFrame::RGB {
        data_to_frames_method_rgb(inject_options, data)
    } else {
        data_to_frames_method_bw(inject_options, data)
    }
}

/// Move data into many frames of the video using RGB
/// Each data (character) is going in to a R or G or B.
/// It means that a pixel can hold 3 characters of a file.
fn data_to_frames_method_rgb(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut data_index = 0;

    while data_index < data.len() {
        // Step 1: Create a single frame
        let mut frame = VideoFrame::new(inject_options.width, inject_options.height);
        for y in (0..inject_options.height).step_by(usize::from(inject_options.size)) {
            for x in (0..inject_options.width).step_by(usize::from(inject_options.size)) {
                // Step 2: For each pixel of the frame, extract a byte of the vector
                // If there is not pixel, we keep filling with the EOF_CHAR to complete`
                // the frame
                let r = if data_index < data.len() {
                    data[data_index]
                } else {
                    EOF_CHAR
                };
                let g = if data_index + 1 < data.len() {
                    data[data_index + 1]
                } else {
                    EOF_CHAR
                };
                let b = if data_index + 2 < data.len() {
                    data[data_index + 2]
                } else {
                    EOF_CHAR
                };
                // Step 3: Apply the pixel to the frame
                frame.write(r, g, b, x, y, inject_options.size);
                data_index += 3; // 3 because R, G, B
            }
        }
        frames.push(frame);
    } // Step 4: Loop until the frame is full or that there is no mode byte
    frames
}

/// Move data into many frames of the video using bit and black and white
/// Each data (character) is going to 8 pixels. Each pixel is black (0) or white (1)
/// It means that a pixel alone represent 1/8 of a byte (a character).
fn data_to_frames_method_bw(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut data_index: usize = 0;
    let mut bit_index: u8 = 7;

    let total_size = u32::from(inject_options.width) * u32::from(inject_options.height)
        / u32::from(inject_options.size);
    if total_size < 8 {
        panic!("The frame size must be at least big enough to accept a single character");
    }

    while data_index < data.len() {
        // Create a single frame
        let mut frame = VideoFrame::new(inject_options.width, inject_options.height);
        for y in (0..inject_options.height).step_by(usize::from(inject_options.size)) {
            for x in (0..inject_options.width).step_by(usize::from(inject_options.size)) {
                // For each pixel of the frame, extract a bit of the active byte of the vector
                if data_index < data.len() {
                    // Still have a char, we get the bit we are at of that char
                    let bit = get_bit_at(data[data_index], bit_index);
                    let (r, g, b) = get_rgb_for_bit(bit);
                    frame.write(r, g, b, x, y, inject_options.size);
                } else {
                    // If there is no char, we keep filling with the EOF_CHAR char to complete frame
                    let bit = get_bit_at(EOF_CHAR, bit_index);
                    let (r, g, b) = get_rgb_for_bit(bit);
                    frame.write(r, g, b, x, y, inject_options.size);
                }

                // Rotate from 0 to 7 inclusively
                // Change character only when all bit of the current one is done
                if bit_index > 1 {
                    bit_index -= 1; // Decrease only the bit because we have not yet written all the bit of the char (8 bits in 1 byte = 1 char)
                } else {
                    data_index += 1; // 1 because increase 1 character at a time
                    bit_index = 7; // Reset
                }
            }
        }
        frames.push(frame);
    } // Step 4: Loop until the frame is full or that there is no mode byte
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

    let fourcc = VideoWriter::fourcc('p', 'n', 'g', ' ');
    //let fourcc =  VideoWriter::fourcc('j', 'p', 'e', 'g');
    //let fourcc = VideoWriter::fourcc('H','2','6','4');
    //let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v');

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
                    }
                    let result_release = video_unwrapped.release();
                    match result_release {
                        Ok(_s) => {
                            println!("Video saved:{}", options.output_video_file.as_str());
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
            println!("{:?}", error)
        }
    }
}

#[cfg(test)]
mod injectionlogics_tests {
    use opencv::prelude::MatTraitConst;

    use crate::injectionextraction::EOF_CHAR;

    use super::*;
    #[test]
    fn test_data_to_frames_short_message_bigger_frame_expect_1_frame() {
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 10,
            width: 10,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
        };
        // Text: This is a test
        let frames = data_to_frames_method_rgb(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
        );
        assert_eq!(frames.len(), 1)
    }

    #[test]
    fn test_data_to_frames_short_message_shorter_frame_expect_2_frame() {
        // 2x2 = 4 with 3 colors = 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 2,
            width: 2,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
        };
        // Text: This is a test, 14 chars
        let frames = data_to_frames_method_rgb(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
        );
        assert_eq!(frames.len(), 2)
    }

    #[test]
    fn test_data_to_frames_short_message_remaining_color_eof() {
        // 2x2 = 4 with 3 colors = 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 2,
            width: 2,
            size: 1,
            algo: crate::options::AlgoFrame::RGB,
        };
        // Text: This
        let frames = data_to_frames_method_rgb(&options, vec![54, 68, 69, 73]);
        let first_frame = &frames[0];
        let color1 = first_frame.read_coordinate_color(0, 0);
        assert_eq!(color1.r, 54);
        assert_eq!(color1.g, 68);
        assert_eq!(color1.b, 69);
        let color2 = first_frame.read_coordinate_color(1, 0);
        assert_eq!(color2.r, 73);
        assert_eq!(color2.g, EOF_CHAR);
        assert_eq!(color2.b, EOF_CHAR);
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
    fn test_data_to_frames_method_blackwhite() {
        // 2x2 = 4 bits per frame. With 14 chars we have 14x8bits = 112bits. 112/16 = 7 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 4,
            width: 4,
            size: 1,
            algo: crate::options::AlgoFrame::BW,
        };
        // Text: This is a test, 14 chars
        let frames = data_to_frames_method_bw(
            &options,
            vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74],
        );
        assert_eq!(frames.len(), 7)
    }

    #[test]
    fn test_data_to_frames_method_blackwhite_remaining_color_eof() {
        // 2x2 = 4 bits per frame. With 4 chars we have 4 = 32bits. 32/4 = 8 frames
        let options = InjectOptions {
            file_path: "".to_string(),
            output_video_file: "".to_string(),
            fps: 30,
            height: 4,
            width: 4,
            size: 1,
            algo: crate::options::AlgoFrame::BW,
        };
        // Text: This
        // T 54 = 0011 0110
        let frames = data_to_frames_method_bw(&options, vec![54, 68, 69, 73]);
        let first_frame = &frames[0];
        let color1 = first_frame.read_coordinate_color(0, 0);
        assert_eq!(color1.r, 0);
        assert_eq!(color1.g, 0);
        assert_eq!(color1.b, 0);
        let color2 = first_frame.read_coordinate_color(1, 0);
        assert_eq!(color2.r, 0);
        assert_eq!(color2.g, 0);
        assert_eq!(color2.b, 0);
        let color3 = first_frame.read_coordinate_color(2, 0);
        assert_eq!(color3.r, 255);
        assert_eq!(color3.g, 255);
        assert_eq!(color3.b, 255);
        let color4 = first_frame.read_coordinate_color(3, 0);
        assert_eq!(color4.r, 255);
        assert_eq!(color4.g, 255);
        assert_eq!(color4.b, 255);
    }
}
