use opencv::videoio::VideoCapture;

use std::fs;

use crate::injectionextraction::map_to_size;
use crate::videoframe::VideoFrame;
use opencv::core::{Mat, Size};
use opencv::prelude::MatTraitConst;
use opencv::prelude::VideoCaptureTrait;
use opencv::videoio::CAP_ANY;

use crate::{injectionextraction::EOF_CHAR, options::ExtractOptions};

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
                panic!("{:?}", err);
            }
        }
    }
    all_frames
}

/// Take the pixels from a collection of frames into a collection of byte
/// The byte values are from the RGB of the pixels
pub fn frames_to_data(extract_options: &ExtractOptions, frames: Vec<VideoFrame>) -> Vec<u8> {
    let mut byte_data = Vec::new();
    let actual_size = map_to_size(
        extract_options.width,
        extract_options.height
    );
    for frame in frames.iter() {
        let frame_data = frame_to_data(frame, actual_size, extract_options.size);
        byte_data.extend(frame_data);
    }
    byte_data
}

/// Loop each frame to find the one that is fully red
/// Accumulate each frame after the red one until the end or until reach another red frame
pub fn extract_relevant_frames(
    extract_options: &ExtractOptions,
    frames: Vec<VideoFrame>,
) -> Vec<VideoFrame> {
    let mut relevant_frames = Vec::new();
    let actual_size = map_to_size(
        extract_options.width,
        extract_options.height
    );

    let mut starting_frame_found = false;
    for frame in frames.iter() {
        let current_frame_is_starting =
            is_starting_frame(frame, actual_size, extract_options.size);

        if starting_frame_found && current_frame_is_starting {
            // Code went back to the starting frame
            break;
        }

        if starting_frame_found && !current_frame_is_starting {
            // We found in the past the starting frame and the current is not the current frame (first or subsequent from loop)
            relevant_frames.push(frame.clone());
        }

        if !starting_frame_found && current_frame_is_starting {
            // Starting frame found, from there we start accumulating
            starting_frame_found = true;
        }
    }

    relevant_frames
}


/// Indicate if the frame is the starting frame
fn is_starting_frame(source: &VideoFrame, actual_size: Size, info_size: u8) -> bool {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = info_size as usize;

    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, info_size);
            if rgb[0] != 255 {
                return false;
            }
            if rgb[1] != 0 {
                return false;
            }
            if rgb[2] != 0 {
                return false;
            }
        }
    }

    return true;
}

/// Extract from a frame all the data. Once the end of file character is found, the loop is done.
/// # Source
/// https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L280
fn frame_to_data(source: &VideoFrame, actual_size: Size, info_size: u8) -> Vec<u8> {
    let width = actual_size.width;
    let height = actual_size.height;
    let size = info_size as usize;

    let mut byte_data: Vec<u8> = Vec::new();
    for y in (0..height).step_by(size) {
        for x in (0..width).step_by(size) {
            let rgb = get_pixel(source, x, y, info_size);
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

    byte_data
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
