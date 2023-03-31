use clap::Parser;
use hdmifiletransporter::execute_with_video_options;
use hdmifiletransporter::{CliData,extract_options};
fn main() {
    let args = CliData::parse();
    let options = extract_options(args);
    match options {
        Ok(oop) => {
            execute_with_video_options(oop);
        },
        Err(error) => panic!("{:?}", error),
    };
    std::process::exit(0);
}
