mod options;
mod videoframe;

use options::{ExtractOptions, InjectOptions};

// Re-export for external access (main.rs)
pub use crate::options::{extract_options, CliData, VideoOptions};
use crate::videoframe::VideoFrame;
use opencv::videoio::VideoCapture;

use opencv::core::{Mat, Size};
use opencv::prelude::MatTraitConst;
use opencv::prelude::VideoCaptureTrait;
use opencv::prelude::VideoWriterTrait;
use opencv::videoio::VideoWriter;
use opencv::videoio::CAP_ANY;

use std::fs;

// const NUMBER_BIT_PER_BYTE: u8 = 8;
const EOF_CHAR: u8 = 4u8;

///
/// Represent a single pixel of color (R, G, B)
///
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

/// Execute video logics
/// Two executions possible: inject a file into a video or extract it.
pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {
            let data = fs::read(&n.file_path)
                .expect(format!("Unable to read file: {}", &n.file_path).as_str());
            let frames = data_to_frames(n.clone(), data);
            frames_to_video(n.clone(), frames);
        }
        VideoOptions::ExtractFromVideo(n) => {
            let frames = video_to_frames(n.clone());
            let data = frames_to_data(n.clone(), frames);
            data_to_files(n.clone(), data);
        }
    }
}

pub fn video_to_frames(extract_options: ExtractOptions) -> Vec<VideoFrame> {
    let mut video = VideoCapture::from_file(&extract_options.video_file_path, CAP_ANY)
        .expect("Could not open video path");

    let mut all_frames = Vec::new();
    loop {
        let mut frame = Mat::default();
        video.read(&mut frame).expect("Reading frame shouldn't crash");
        
        if frame.cols() == 0 {
            break;
        }

        let source = VideoFrame::from(frame, extract_options.size);
        match source {
            Ok(frame) => {
                all_frames.push(frame);
            }
            Err(err) => {
                panic!("{:?}", err);
            }
        }

    }
    return all_frames;
}

/// Take the pixels from a collection of frames into a collection of byte
/// The byte values are from the RGB of the pixels
pub fn frames_to_data(extract_options: ExtractOptions, frames: Vec<VideoFrame>) -> Vec<u8> {
    let mut byte_data = Vec::new();
    for frame in frames.iter() {
        let frame_data = frame_to_data(&frame);
        byte_data.extend(frame_data);
    }
    byte_data
}


/// Extract from a frame all the data. Once the end of file character is found, the loop is done.
/// # Source
/// https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L280
fn frame_to_data(source: &VideoFrame) -> Vec<u8> {
    let width = source.actual_size.width;
    let height = source.actual_size.height;
    let size = source.size as usize;

    let mut byte_data: Vec<u8> = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(&source, x, y);
            if rgb[0] == EOF_CHAR {
                return byte_data;
            }
            byte_data.push(rgb[0]);

            if rgb[1] == EOF_CHAR {
                return byte_data;
            }
            byte_data.push(rgb[1]);

            if rgb[2] == EOF_CHAR {
                return byte_data;
            }
            byte_data.push(rgb[2]);
        }
    }


    return byte_data;
}


/// Extract a pixel value that might be spread on many sibling pixel to reduce innacuracy
/// # Source
/// Code is a copy of https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L121
fn get_pixel(frame: &VideoFrame, x: i32, y: i32) -> Vec<u8> {
    let mut r_list: Vec<u8> = Vec::new();
    let mut g_list: Vec<u8> = Vec::new();
    let mut b_list: Vec<u8> = Vec::new();

    for i in 0..frame.size {
        for j in 0..frame.size {
            let bgr = frame
                .image
                .at_2d::<opencv::core::Vec3b>(i32::from(y) + i32::from(i), i32::from(x) + i32::from(j))
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

    return rgb_average;
}

/// Move all the data from gathered from the movie file into
/// a file that should be the original file.
///
/// # Example
/// if we injected a .zip file, we expect the file to be written to be also a .zip
///
pub fn data_to_files(extract_options: ExtractOptions, whole_movie_data: Vec<u8>) -> () {
    fs::write(extract_options.extracted_file_path, whole_movie_data).expect("Writing file fail");
}

/// Move data into many frame of the video
///
pub fn data_to_frames(inject_options: InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut data_index = 0;

    while data_index < data.len() {
        // Step 1: Create a single frame
        let mut frame = VideoFrame::new(
            inject_options.size,
            inject_options.width,
            inject_options.height,
        );
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
                frame.write(r, g, b, x, y);
                data_index += 3; // 3 because R, G, B
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

    //let fourcc = VideoWriter::fourcc('p', 'n', 'g', ' ');
    //let fourcc =  VideoWriter::fourcc('j', 'p', 'e', 'g');
    //let fourcc = VideoWriter::fourcc('H','2','6','4');
    let fourcc = VideoWriter::fourcc('m', 'p', '4', 'v');

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
                        video_unwrapped.write(&image);
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
mod lib_tests {
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
        };
        // Text: This is a test
        let frames = data_to_frames(
            options,
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
        };
        // Text: This is a test, 14 chars
        let frames = data_to_frames(
            options,
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
        };
        // Text: This
        let frames = data_to_frames(options, vec![54, 68, 69, 73]);
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
}
