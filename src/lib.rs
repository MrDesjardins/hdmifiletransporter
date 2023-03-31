mod options;

// Re-export for external access (main.rs)
pub use crate::options::{VideoOptions,CliData,extract_options};

pub fn execute_with_video_options(options: VideoOptions) {
    match options {
        VideoOptions::InjectInVideo(n) => {
            todo!("To do inject");
        }
        VideoOptions::ExtractFromVideo(n) => {
            todo!("To do extract");
        }
    }
}
