mod options;
mod videoframe;
mod injectionextraction;
mod injectionlogics;
mod extractionlogics;

use extractionlogics::data_to_files;
use injectionlogics::file_to_data;

// Re-export for external access (main.rs)
pub use crate::options::{extract_options, CliData, VideoOptions};
pub use crate::injectionlogics::{data_to_frames, frames_to_video};
pub use crate::extractionlogics::{video_to_frames, frames_to_data};

/// Execute video logics
/// Two executions possible: inject a file into a video or extract it.
pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {

            let data = file_to_data(&n);
            let frames = data_to_frames(&n, data);
            frames_to_video(n, frames);
        }
        VideoOptions::ExtractFromVideo(n) => {
            let frames = video_to_frames(&n);
            let data = frames_to_data(&n, frames);
            data_to_files(&n, data);
        }
    }
}