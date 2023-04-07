mod extractionlogics;
mod injectionextraction;
mod injectionlogics;
mod options;
mod videoframe;

use extractionlogics::{data_to_files, extract_relevant_frames};
use injectionlogics::{create_starting_frame, file_to_data};

// Re-export for external access (main.rs)
pub use crate::extractionlogics::{frames_to_data, video_to_frames};
pub use crate::injectionlogics::{data_to_frames, frames_to_video};
pub use crate::options::{extract_options, CliData, VideoOptions};

/// Execute video logics
/// Two executions possible: inject a file into a video or extract it.
pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {
            let data = file_to_data(&n);
            let starting_frame = create_starting_frame(&n);
            let frames = data_to_frames(&n, data);
            let mut merged_frames = vec![starting_frame];
            merged_frames.extend(frames);
            frames_to_video(n, merged_frames);
        }
        VideoOptions::ExtractFromVideo(n) => {
            let frames = video_to_frames(&n);
            let ordered_frames = extract_relevant_frames(&n, frames);
            let data = frames_to_data(&n, ordered_frames);
            data_to_files(&n, data);
        }
    }
}
