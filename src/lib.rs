mod options;
mod videoframe;

use options::InjectOptions;

// Re-export for external access (main.rs)
pub use crate::options::{extract_options, CliData, VideoOptions};
use crate::videoframe::VideoFrame;

// const NUMBER_BIT_PER_BYTE: u8 = 8;
const EOF_CHAR: u8 = 4u8;

pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {
            let frames = data_to_frames(n);
            frames_to_video(frames);
        }
        VideoOptions::ExtractFromVideo(n) => {
            todo!("To do extract");
        }
    }
}

pub fn data_to_frames(inject_options: InjectOptions) -> Vec<VideoFrame> {
    let mut frames: Vec<VideoFrame> = Vec::new();
    let mut data_index = 0;

    while data_index < inject_options.file_buffer.len() {
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
                let r = if data_index < inject_options.file_buffer.len() {
                    inject_options.file_buffer[data_index]
                } else {
                    EOF_CHAR
                };
                let g = if data_index + 1 < inject_options.file_buffer.len() {
                    inject_options.file_buffer[data_index + 1]
                } else {
                    EOF_CHAR
                };
                let b = if data_index + 2 < inject_options.file_buffer.len() {
                    inject_options.file_buffer[data_index + 2]
                } else {
                    EOF_CHAR
                };
                // Step 3: Apply the pixel to the frame
                frame.write(r, g, b, x, y);
                data_index += 3; // 3 because R, G, B

                // Step 4: Loop until the frame is full or that there is no mode byte
                // Step 5: If more more bytes are available, go back to step 1
                // Step 6: Otherwise, add an EnfOfFile character in the frame for the remaining of the frame
                // Step 7: Assemble all the frame into a video format
                // Step 8: Output the video into a file without compression
            }
        }
        frames.push(frame);
    }
    frames
}

pub fn frames_to_video(frames: Vec<VideoFrame>) {}

#[cfg(test)]
mod lib_tests {
    use super::*;
    #[test]
    fn test_data_to_frames_short_message_bigger_frame_expect_1_frame() {
        let options = InjectOptions {
            file_buffer: vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74], // Text: This is a test
            fps: 30,
            height: 10,
            width: 10,
            size: 1,
        };
        let frames = data_to_frames(options);
        assert_eq!(frames.len(), 1)
    }

    #[test]
    fn test_data_to_frames_short_message_shorter_frame_expect_2_frame() {
        // 2x2 = 4 with 3 colors = 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_buffer: vec![54, 68, 69, 73, 20, 69, 73, 20, 61, 20, 74, 65, 73, 74], // Text: This is a test, 14 chars
            fps: 30,
            height: 2, 
            width: 2, 
            size: 1,
        };
        let frames = data_to_frames(options);
        assert_eq!(frames.len(), 2)
    }

    #[test]
    fn test_data_to_frames_short_message_remaining_color_eof() {
        // 2x2 = 4 with 3 colors = 12 chars, thus < 14 => 2 frames
        let options = InjectOptions {
            file_buffer: vec![54, 68, 69, 73], // Text: This
            fps: 30,
            height: 2, 
            width: 2, 
            size: 1,
        };
        let frames = data_to_frames(options);
        let first_frame = &frames[0];
        let color1 = first_frame.read_coordinate_color(0,0);
        assert_eq!(color1.r, 54);
        assert_eq!(color1.g, 68);
        assert_eq!(color1.b, 69);
        let color2 = first_frame.read_coordinate_color(1,0);
        assert_eq!(color2.r, 73);
        assert_eq!(color2.g, EOF_CHAR);
        assert_eq!(color2.b, EOF_CHAR);
    }
}
