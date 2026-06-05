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
    /// Raw 8-bit colour: 3 bytes per cell (256 levels/channel). Dense, fragile.
    RGB,
    /// Black/white: 1 bit per cell using all three channels. Sparse, robust.
    BW,
    /// Quantized colour: each channel carries one of `levels` evenly-spaced
    /// symbols (a power of two), i.e. `log2(levels)` bits per channel and
    /// `3*log2(levels)` bits per cell. `levels = 2` is the densest maximally
    /// separated option (3 bits/cell, 3x BW); `levels = 256` equals raw RGB.
    Quantized(u32),
    /// Brightness / luma: each cell is a single grey shade chosen from `levels`
    /// evenly-spaced values (R = G = B), i.e. `log2(levels)` bits per cell (one
    /// symbol, not three). Because a capture card keeps luminance at full
    /// resolution but subsamples colour, packing data into brightness instead of
    /// chroma survives compression far better - so more levels stay reliable
    /// than in `Quantized`, at 1/3 the bits/cell for the same level count.
    Brightness(u32),
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
        match self {
            Self::RGB => write!(f, "rgb"),
            Self::BW => write!(f, "bw"),
            Self::Quantized(levels) => write!(f, "quantized{levels}"),
            Self::Brightness(levels) => write!(f, "brightness{levels}"),
        }
    }
}

impl std::str::FromStr for AlgoFrame {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "rgb" => Ok(Self::RGB),
            "bw" => Ok(Self::BW),
            // The level count is supplied separately via `--levels`; these are
            // placeholders that `extract_options` rewrites with the real value.
            "quantized" => Ok(Self::Quantized(DEFAULT_QUANTIZED_LEVELS)),
            "brightness" => Ok(Self::Brightness(DEFAULT_QUANTIZED_LEVELS)),
            _ => Err(format!("Unknown algo: {s}")),
        }
    }
}

/// Default number of levels per channel when `--algo quantized` is selected
/// without an explicit `--levels`. Four levels = 2 bits/channel = 6 bits/cell,
/// a good density/robustness compromise for typical HDMI links.
pub const DEFAULT_QUANTIZED_LEVELS: u32 = 4;

/// Resolve the level count and validate the algo selection. For the level-based
/// algos (`quantized`, `brightness`) the levels come from `--levels` (falling
/// back to the default) and must be a power of two in `2..=256`.
fn resolve_algo(algo: AlgoFrame, levels: Option<u32>) -> AlgoFrame {
    let validate = |levels: u32| {
        if !levels.is_power_of_two() || !(2..=256).contains(&levels) {
            panic!("--levels must be a power of two between 2 and 256 (got {levels})");
        }
        levels
    };
    match algo {
        AlgoFrame::Quantized(_) => {
            AlgoFrame::Quantized(validate(levels.unwrap_or(DEFAULT_QUANTIZED_LEVELS)))
        }
        AlgoFrame::Brightness(_) => {
            AlgoFrame::Brightness(validate(levels.unwrap_or(DEFAULT_QUANTIZED_LEVELS)))
        }
        other => other,
    }
}

/// CLI arguments
///
/// The command line accepts options to inject a file into a video and to extract
/// a file back from a video. The full list of options is available in the
/// `CliData` struct.
///
#[derive(Parser)]
#[clap(name = "from_str")]
#[command(author, version, about, long_about = None)] // Read from `Cargo.toml`
pub struct CliData {
    /// The source of the file to inject the video
    #[arg(short = 'i', long)]
    input_file_path: Option<String>,

    #[arg(short = 'f', long)]
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
    #[arg(short = 's', long)]
    pub size: Option<u8>,

    #[arg(short = 'g', long)]
    pub height: Option<u16>,

    #[arg(short = 'w', long)]
    pub width: Option<u16>,

    /// When extracting, where to save the file
    #[arg(short = 'o', long)]
    pub output_video_path: Option<String>,

    /// Possible values:
    /// "inject"= inject the file into an image.
    /// "extract" = extract from an video the file.
    #[arg(short='m', long, value_parser = clap::builder::PossibleValuesParser::new(["inject", "extract"])
    .map(|s| s.parse::<AppMode>().unwrap()),)]
    pub mode: Option<AppMode>,

    /// Determine how the data is injected and extract into a frame
    #[arg(short='a', long, value_parser = clap::builder::PossibleValuesParser::new(["rgb", "bw", "quantized", "brightness"])
    .map(|s| s.parse::<AlgoFrame>().unwrap()),)]
    pub algo: Option<AlgoFrame>,

    /// Number of levels for the `quantized`/`brightness` algos. Must be a power
    /// of two in 2..=256. For `quantized` it is levels per channel (3*log2 bits
    /// per cell); for `brightness` it is grey shades per cell (log2 bits per
    /// cell). Ignored for the `rgb` and `bw` algos.
    #[arg(short = 'l', long)]
    pub levels: Option<u32>,

    #[arg(short = 'p', long)]
    pub show_progress: Option<bool>,

}

/// Extract from the command line (CLI) argument the option.
/// Depending of the mode, the function returns
/// the proper formed structure or panic telling what argument
/// is missing
///
/// # Arguments
/// args - The command line argument that may contain inject or extract information
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
                let size = args.size.unwrap_or(1);
                let height = args.height.unwrap_or(2160);
                let width = args.width.unwrap_or(3840);
                if i32::from(height) % i32::from(size) != 0 {
                    panic!("Height and size are not a divided round number");
                }
                if i32::from(width) % i32::from(size) != 0 {
                    panic!("Width and size are not a divided round number");
                }
                VideoOptions::InjectInVideo({
                    InjectOptions {
                        file_path,
                        output_video_file: args
                            .output_video_path
                            .unwrap_or_else(|| "video.mkv".to_string()),
                        size: args.size.unwrap_or(1),
                        fps: args.fps.unwrap_or(30),
                        height: args.height.unwrap_or(2160),
                        width: args.width.unwrap_or(3840),
                        algo: resolve_algo(args.algo.unwrap_or(AlgoFrame::RGB), args.levels),
                        show_progress: args.show_progress.unwrap_or(false)
                    }
                })
            }
            AppMode::Extract => VideoOptions::ExtractFromVideo({
                ExtractOptions {
                    video_file_path: args
                        .input_file_path
                        .unwrap_or_else(|| "video.mkv".to_string()),
                    extracted_file_path: args
                        .output_video_path
                        .unwrap_or_else(|| "mydata.txt".to_string()),
                    size: args.size.unwrap_or(1),
                    fps: args.fps.unwrap_or(30),
                    height: args.height.unwrap_or(2160),
                    width: args.width.unwrap_or(3840),
                    algo: resolve_algo(args.algo.unwrap_or(AlgoFrame::RGB), args.levels),
                    show_progress: args.show_progress.unwrap_or(false)
                }
            }),
        },
        None => panic!("Mode is required (use -m inject or -m extract)"),
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
    pub show_progress: bool,
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
    pub show_progress: bool,
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
            levels: None,
            show_progress: None
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
            levels: None,
            show_progress: None,
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
            levels: None,
            show_progress: None,
        });
        let unwrapped_options = options.unwrap();
        if let InjectInVideo(op) = unwrapped_options {
            assert_eq!(op.fps, 30);
            assert_eq!(op.height, 2160);
            assert_eq!(op.width, 3840);
            assert_eq!(op.size, 1);
            assert_eq!(op.output_video_file, "video.mkv");
            assert_eq!(op.algo, AlgoFrame::RGB);
            assert_eq!(op.show_progress, false);
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
            algo: None,
            levels: None,
            show_progress: None,
        });
        let unwrapped_options = options.unwrap();
        if let ExtractFromVideo(op) = unwrapped_options {
            assert_eq!(op.fps, 30);
            assert_eq!(op.height, 2160);
            assert_eq!(op.width, 3840);
            assert_eq!(op.size, 1);
            assert_eq!(op.extracted_file_path, "mydata.txt");
            assert_eq!(op.video_file_path, "video.mkv");
            assert_eq!(op.algo, AlgoFrame::RGB);
            assert_eq!(op.show_progress, false);
        } else {
            assert!(true, "Failed to unwrapped extract options");
        }
    }
}
