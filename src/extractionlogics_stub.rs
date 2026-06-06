use std::fs;

use crate::options::ExtractOptions;
use crate::videoframe_stub::VideoFrame;

/// Video decoding requires OpenCV.
pub fn video_to_frames(_extract_options: &ExtractOptions) -> Vec<VideoFrame> {
    Vec::new()
}

/// Frame decoding requires OpenCV-backed frame pixels.
pub fn frames_to_data(_extract_options: &ExtractOptions, _frames: Vec<VideoFrame>) -> Vec<u8> {
    panic!("frames_to_data requires the opencv-backend feature");
}

pub fn data_to_files(extract_options: &ExtractOptions, whole_movie_data: Vec<u8>) {
    fs::write(
        extract_options.extracted_file_path.clone(),
        whole_movie_data,
    )
    .expect("Writing file fail");
}
