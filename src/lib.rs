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
        output_video_file: "/your/video.mkv".to_string(),
        fps: 30,
        width: 1920,
        height: 1080,
        size: 1,
        algo: AlgoFrame::RGB,
        show_progress: false
    }
});
let _ = execute_with_video_options(options);
```

## Extract the file from the video

```no_run
use hdmifiletransporter::execute_with_video_options;
use hdmifiletransporter::options::{VideoOptions, ExtractOptions, AlgoFrame};

let options = VideoOptions::ExtractFromVideo({
    ExtractOptions {
        video_file_path:"/your/video.mkv".to_string(),
        extracted_file_path: "/your/file/here.zip".to_string(),
        fps: 30,
        width: 1920,
        height: 1080,
        size: 1,
        algo: AlgoFrame::RGB,
        show_progress: false
    }
});
let _ = execute_with_video_options(options);
```
*/

mod bitlogics;
#[cfg(feature = "opencv-backend")]
mod extractionlogics;
#[cfg(not(feature = "opencv-backend"))]
mod extractionlogics_stub;
mod injectionextraction;
#[cfg(feature = "opencv-backend")]
mod injectionlogics;
#[cfg(not(feature = "opencv-backend"))]
mod injectionlogics_stub;
mod instructionlogics;
pub mod options;
#[cfg(feature = "opencv-backend")]
mod videoframe;
#[cfg(not(feature = "opencv-backend"))]
mod videoframe_stub;

#[cfg(feature = "opencv-backend")]
use extractionlogics::data_to_files;
#[cfg(not(feature = "opencv-backend"))]
use extractionlogics_stub::data_to_files;
#[cfg(feature = "opencv-backend")]
use injectionlogics::file_to_data;
#[cfg(not(feature = "opencv-backend"))]
use injectionlogics_stub::file_to_data;

// Re-export for external access (main.rs)
#[cfg(feature = "opencv-backend")]
pub use crate::extractionlogics::{frames_to_data, register_frame, video_to_frames};
#[cfg(not(feature = "opencv-backend"))]
pub use crate::extractionlogics_stub::{frames_to_data, video_to_frames};
pub use crate::injectionextraction::{content_cell_xy, frame_capacity, HEADER_BITS};
#[cfg(feature = "opencv-backend")]
pub use crate::injectionlogics::{create_starting_frame, data_to_frames, frames_to_video};
#[cfg(not(feature = "opencv-backend"))]
pub use crate::injectionlogics_stub::{create_starting_frame, data_to_frames, frames_to_video};
pub use crate::instructionlogics::{FrameHeader, FrameType, Instruction};
pub use crate::options::{extract_options, CliData, ExtractOptions, InjectOptions, VideoOptions};
#[cfg(feature = "opencv-backend")]
pub use crate::videoframe::VideoFrame;
#[cfg(not(feature = "opencv-backend"))]
pub use crate::videoframe_stub::VideoFrame;

/// Execute video logics
/// Two executions possible: inject a file into a video or extract it.
///
/// Returns an error describing the failure (for example if the video could not
/// be written) so the caller can react instead of silently continuing.
pub fn execute_with_video_options(options: VideoOptions) -> Result<(), String> {
    match options {
        VideoOptions::InjectInVideo(n) => {
            let data = file_to_data(&n);
            let starting_frame = create_starting_frame(data.len() as u64, &n);
            let frames = data_to_frames(&n, data);
            let mut merged_frames = vec![starting_frame];
            merged_frames.extend(frames);
            frames_to_video(n, merged_frames)?;
        }
        VideoOptions::ExtractFromVideo(n) => {
            let frames = video_to_frames(&n);
            let data = frames_to_data(&n, frames);
            data_to_files(&n, data);
        }
    }
    Ok(())
}
