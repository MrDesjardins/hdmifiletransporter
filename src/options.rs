use clap::builder::TypedValueParser;
use clap::command;
use clap::Parser;

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AppMode {
    Inject,
    Extract,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AlgoFrame {
    RGB,
    BW,
}

impl std::fmt::Display for AppMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Inject => "inject",
            Self::Extract => "extract",
        };
        s.fmt(f)
    }
}

impl std::str::FromStr for AppMode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "inject" => Ok(Self::Inject),
            "extract" => Ok(Self::Extract),
            _ => Err(format!("Unknown mode: {s}")),
        }
    }
}

impl std::fmt::Display for AlgoFrame {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::RGB => "rgb",
            Self::BW => "bw",
        };
        s.fmt(f)
    }
}

impl std::str::FromStr for AlgoFrame {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rgb" => Ok(Self::RGB),
            "bw" => Ok(Self::BW),
            _ => Err(format!("Unknown algo: {s}")),
        }
    }
}

#[derive(Clone)]
pub struct InjectOption {
    pub message: String,
    pub password: Option<String>,
    pub input_image_path: String,
    pub output_image_path: String,
}

/// CLI arguments
///
/// The command line provided in this Cargo accepts many options to encrypt a message
/// and decrypt a message. The full list of options are available in the `CliData` struct.
///
#[derive(Parser)]
#[clap(name = "from_str")]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
pub struct CliData {
    /// The source of the file to inject the video
    #[arg(short, long)]
    input_file_path: Option<String>,

    #[arg(short, long)]
    pub fps: Option<u8>,

    /// Depending of how we will translate the information into the video frame,
    /// we may color the information in different size. The size represent the number
    /// of pixel (width and height) for each value.
    ///
    /// # Expected Values
    /// 1, 2 or 4. Should never be 0 and would be innefficient to have bigger than 4.
    ///
    /// # Examples
    /// E.g. A size of 1 means each info is colored into 1 pixel
    /// E.g. A size of 2 means each info is colored into a 2x2 pixel (4 pixels)
    #[arg(short, long)]
    pub size: Option<u8>,

    #[arg(short = 'g', long)]
    pub height: Option<u16>,

    #[arg(short, long)]
    pub width: Option<u16>,

    /// When extracting, where to save the file
    #[arg(short, long)]
    pub output_video_path: Option<String>,

    /// Possible values:
    /// "inject"= inject the file into an image.
    /// "extract" = extract from an video the file.
    #[arg(short='m', long, value_parser = clap::builder::PossibleValuesParser::new(["inject", "extract"])
    .map(|s| s.parse::<AppMode>().unwrap()),)]
    pub mode: Option<AppMode>,

    /// Determine how the data is injected and extract into a frame
    #[arg(short='a', long, value_parser = clap::builder::PossibleValuesParser::new(["rgb", "bw"])
    .map(|s| s.parse::<AlgoFrame>().unwrap()),)]
    pub algo: Option<AlgoFrame>,
}

/// Extract from the command line (CLI) argument the option.
/// Depending of the mode, the function returns
/// the proper formed structure or panic telling what argument
/// is missing
///
/// # Arguments
/// args - The command line argument that may contain encrypt or decrypt information
///
/// # Returns
/// Return a well formed structure for the task asked or return a failure with the missing
/// fields
pub fn extract_options(args: CliData) -> Result<VideoOptions, String> {
    Ok(match args.mode {
        Some(i) => match i {
            AppMode::Inject => {
                let file_path = args
                    .input_file_path
                    .unwrap_or_else(|| panic!("Missing input file"));
                println!("Input file: {}", file_path);

                VideoOptions::InjectInVideo({
                    InjectOptions {
                        file_path,
                        output_video_file: args
                            .output_video_path
                            .unwrap_or_else(|| "video.mp4".to_string()),
                        size: args.size.unwrap_or(1),
                        fps: args.fps.unwrap_or(30),
                        height: args.height.unwrap_or(2160),
                        width: args.width.unwrap_or(3840),
                        algo: args.algo.unwrap_or(AlgoFrame::RGB)
                    }
                })
            }
            AppMode::Extract => VideoOptions::ExtractFromVideo({
                ExtractOptions {
                    video_file_path: args
                        .input_file_path
                        .unwrap_or_else(|| "video.mp4".to_string()),
                    extracted_file_path: args
                        .output_video_path
                        .unwrap_or_else(|| "mydata.txt".to_string()),
                    size: args.size.unwrap_or(1),
                    fps: args.fps.unwrap_or(30),
                    height: args.height.unwrap_or(2160),
                    width: args.width.unwrap_or(3840),
                    algo: args.algo.unwrap_or(AlgoFrame::RGB)
                }
            }),
        },
        None => panic!("Encrypt mode is required"),
    })
}

/// Required options for the injection of the file into a video
#[derive(Clone)]
pub struct InjectOptions {
    pub file_path: String,
    pub output_video_file: String,
    pub fps: u8,
    pub width: u16,
    pub height: u16,
    pub size: u8,
    pub algo: AlgoFrame,
}

#[derive(Clone)]
pub struct ExtractOptions {
    pub video_file_path: String,
    pub extracted_file_path: String,
    pub fps: u8,
    pub width: u16,
    pub height: u16,
    pub size: u8,
    pub algo: AlgoFrame,
}

#[derive(Clone)]
pub enum VideoOptions {
    InjectInVideo(InjectOptions),
    ExtractFromVideo(ExtractOptions),
}

#[cfg(test)]
mod options_tests {

    use super::*;
    use crate::options::AppMode;
    use crate::VideoOptions::{ExtractFromVideo, InjectInVideo};
    #[test]
    #[should_panic]
    fn test_extract_options_no_mode() {
        let _ = extract_options(CliData {
            fps: None,
            height: None,
            input_file_path: Some("inputfile.txt".to_string()),
            mode: None,
            output_video_path: None,
            size: None,
            width: None,
            algo: None,
        });
    }
    #[test]
    #[should_panic]
    fn test_extract_options_inject_no_input_file_path() {
        let _ = extract_options(CliData {
            fps: None,
            height: None,
            input_file_path: None,
            mode: Some(AppMode::Inject),
            output_video_path: None,
            size: None,
            width: None,
            algo: None,
        });
    }
    #[test]
    fn test_extract_options_inject_default() {
        let options = extract_options(CliData {
            fps: None,
            height: None,
            input_file_path: Some("inputfile.txt".to_string()),
            mode: Some(AppMode::Inject),
            output_video_path: None,
            size: None,
            width: None,
            algo: None,
        });
        let unwrapped_options = options.unwrap();
        if let InjectInVideo(op) = unwrapped_options {
            assert_eq!(op.fps, 30);
            assert_eq!(op.height, 2160);
            assert_eq!(op.width, 3840);
            assert_eq!(op.size, 1);
            assert_eq!(op.output_video_file, "video.mp4");
            assert_eq!(op.algo, AlgoFrame::RGB);
        } else {
            assert!(true, "Failed to unwrapped inject options");
        }
    }
    #[test]
    fn test_extract_options_extract_default() {
        let options = extract_options(CliData {
            fps: None,
            height: None,
            input_file_path: None,
            mode: Some(AppMode::Extract),
            output_video_path: None,
            size: None,
            width: None,
            algo: None
        });
        let unwrapped_options = options.unwrap();
        if let ExtractFromVideo(op) = unwrapped_options {
            assert_eq!(op.fps, 30);
            assert_eq!(op.height, 2160);
            assert_eq!(op.width, 3840);
            assert_eq!(op.size, 1);
            assert_eq!(op.extracted_file_path, "mydata.txt");
            assert_eq!(op.video_file_path, "video.mp4");
            assert_eq!(op.algo, AlgoFrame::RGB);
        } else {
            assert!(true, "Failed to unwrapped extract options");
        }
    }
}
