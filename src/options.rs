use clap;
use clap::builder::TypedValueParser;
use clap::command;
use clap::Parser;
use std::fs;
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum AppMode {
    Inject,
    Extract,
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
    
    #[arg(short, long)]
    pub size: Option<u8>,

    #[arg(short='g', long)]
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
                let file_path = args.input_file_path.unwrap_or_else(|| panic!("Missing input file"));
                println!("Input file: {}", file_path);
                let data = fs::read(file_path).expect("Unable to read file");
                VideoOptions::InjectInVideo({
                    InjectOptions {
                        file_buffer: data,
                        output_video_file: args.output_video_path.unwrap_or_else( || "video.mp4v".to_string()),
                        size: args.size.unwrap_or_else(|| 1), 
                        fps: args.fps.unwrap_or_else(|| 30),
                        height: args.height.unwrap_or_else(|| 2160),
                        width: args.width.unwrap_or_else(|| 3840),
                    }
                })
            }
            AppMode::Extract => VideoOptions::ExtractFromVideo({ ExtractOptions {} }),
        },
        None => panic!("Encrypt mode is required"),
    })
}

/// Required options for the injection of the file into a video
#[derive(Clone)]
pub struct InjectOptions {
    pub file_buffer: Vec<u8>,
    pub output_video_file: String,
    pub fps: u8,
    pub width: u16,
    pub height: u16,
    pub size: u8,
}

#[derive(Clone)]
pub struct ExtractOptions {}

#[derive(Clone)]
pub enum VideoOptions {
    InjectInVideo(InjectOptions),
    ExtractFromVideo(ExtractOptions),
}
