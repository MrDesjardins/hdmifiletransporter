use clap::Parser;
use hdmifiletransporter::execute_with_video_options;
use hdmifiletransporter::{extract_options, CliData};

fn main() {
    let args = CliData::parse();
    // Transform the user's input into something the application can understand.
    // Allows to set default value and open the file from the user input
    let options = extract_options(args);
    match options {
        Ok(oop) => {
            // Start the transformation of the vector of byte into a video format
            // This is only executed if the file provided was valid and was decoded
            // into a vector of byte.
            execute_with_video_options(oop);
        }
        Err(error) => panic!("{:?}", error),
    };
    std::process::exit(0);
}
