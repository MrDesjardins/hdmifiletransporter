mod options;
mod videoframe;

// Re-export for external access (main.rs)
pub use crate::options::{extract_options, CliData, VideoOptions};
use crate::videoframe::VideoFrame;

const NUMBER_BIT_PER_BYTE: u8 = 8;
const EOF_CHAR :u8= 4u8 ;

pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {
            let mut frames: Vec<VideoFrame> = Vec::new();
            let mut data_index = 0;

            while (data_index < n.file_buffer.len()) {
                // Step 1: Create a single frame
                let mut frame = VideoFrame::new(n.size, n.width, n.height);
                for y in (0..n.height).step_by(usize::from(n.size)) {
                    for x in (0..n.width).step_by(usize::from(n.size)) {
                        // Step 2: For each pixel of the frame, extract a byte of the vector
                        let r = if (data_index < n.file_buffer.len()) {
                            n.file_buffer[data_index]
                        } else {
                            EOF_CHAR
                        };
                        let g = if (data_index + 1 < n.file_buffer.len()) {
                            n.file_buffer[data_index + 1]
                        } else {
                            EOF_CHAR
                        };
                        let b = if (data_index + 2 < n.file_buffer.len()) {
                            n.file_buffer[data_index + 2]
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
        }
        VideoOptions::ExtractFromVideo(n) => {
            todo!("To do extract");
        }
    }
}
