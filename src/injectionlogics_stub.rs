use std::fs;

use crate::options::InjectOptions;
use crate::videoframe_stub::VideoFrame;

pub fn file_to_data(options: &InjectOptions) -> Vec<u8> {
    fs::read(&options.file_path).unwrap_or_else(|err| {
        panic!(
            "Unable to read file: {} with error: {}",
            &options.file_path, err
        )
    })
}

/// Create a placeholder starting frame when OpenCV support is disabled.
///
/// Enable the default `opencv-backend` feature to create a real encoded frame.
pub fn create_starting_frame(_total_data_size: u64, inject_options: &InjectOptions) -> VideoFrame {
    VideoFrame::new(inject_options.width, inject_options.height)
}

/// Return placeholder frames when OpenCV support is disabled.
///
/// Enable the default `opencv-backend` feature to encode bytes into video
/// frames.
pub fn data_to_frames(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    if data.is_empty() {
        Vec::new()
    } else {
        vec![VideoFrame::new(inject_options.width, inject_options.height)]
    }
}

/// Video writing requires OpenCV.
pub fn frames_to_video(_options: InjectOptions, _frames: Vec<VideoFrame>) -> Result<(), String> {
    Err("frames_to_video requires the opencv-backend feature".to_string())
}
