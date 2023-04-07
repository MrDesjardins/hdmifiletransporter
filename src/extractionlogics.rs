use opencv::videoio::VideoCapture;



use std::fs;

use crate::videoframe::VideoFrame;
use opencv::core::{Mat};
use opencv::prelude::MatTraitConst;
use opencv::prelude::VideoCaptureTrait;
use opencv::videoio::CAP_ANY;
use crate::{options::ExtractOptions, injectionextraction::EOF_CHAR};


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

