/*!
# HDMI File Transporter
The HDMI File Transporter is a Rust library that inject the content of a single file into a single video file. The library
allows the reverse by reading the video content and export the content of the original file.

# Goals

The goals are:
1. Injecting content into a video format to transfer the content using HDMI (or other display port)
2. Extracting the content (text, zip file, etc) from the video and consuming it from another computer.

# Examples

## Injecting a file into a video

```no_run
use hdmifiletransporter::execute_with_video_options;
use hdmifiletransporter::options::{VideoOptions, InjectOptions, AlgoFrame};

let options = VideoOptions::InjectInVideo({
    InjectOptions {
        file_path: "/your/file/here.zip".to_string(),
        output_video_file: "/your/video.mp4".to_string(),
        fps: 30,
        width: 1080,
        height: 1920,
        size: 1,
        algo: AlgoFrame::RGB,
        show_progress: false
    }
});
execute_with_video_options(options);
```

## Extract the file from the video

```no_run
use hdmifiletransporter::execute_with_video_options;
use hdmifiletransporter::options::{VideoOptions, ExtractOptions, AlgoFrame};

let options = VideoOptions::ExtractFromVideo({
    ExtractOptions {
        video_file_path:"/your/video.mp4".to_string(),
        extracted_file_path: "/your/file/here.zip".to_string(),
        fps: 30,
        width: 1080,
        height:1920,
        size: 1,
        algo: AlgoFrame::RGB,
        show_progress: false
    }
});
execute_with_video_options(options);
*/

mod bitlogics;
mod extractionlogics;
mod injectionextraction;
mod injectionlogics;
mod instructionlogics;
pub mod options;
mod videoframe;

use extractionlogics::data_to_files;
use injectionlogics::file_to_data;

// Re-export for external access (main.rs)
pub use crate::extractionlogics::{frames_to_data, video_to_frames};
pub use crate::injectionlogics::{create_starting_frame, data_to_frames, frames_to_video};
pub use crate::options::{extract_options, CliData, ExtractOptions, InjectOptions, VideoOptions};
pub use crate::videoframe::VideoFrame;
pub use crate::instructionlogics::Instruction;

/// Execute video logics
/// Two executions possible: inject a file into a video or extract it.
pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {
            let data = file_to_data(&n);
            let instruction_data = Instruction::new(data.len() as u64);
            let starting_frame = create_starting_frame(&instruction_data, &n);
            let frames = data_to_frames(&n, data);
            let mut merged_frames = vec![starting_frame];
            merged_frames.extend(frames);
            frames_to_video(n, merged_frames);
        }
        VideoOptions::ExtractFromVideo(n) => {
            let frames = video_to_frames(&n);
            let data = frames_to_data(&n, frames);
            data_to_files(&n, data);
        }
    }
}
