# HDMI File Transporter

[<img alt="github" src="https://img.shields.io/badge/github-mrdesjardins/hdmifiletransporter-8dagcb?labelColor=555555&logo=github" height="20">](https://github.com/MrDesjardins/hdmifiletransporter)
[<img alt="crates.io" src="https://img.shields.io/crates/v/hdmifiletransporter.svg?color=fc8d62&logo=rust" height="20">](https://crates.io/crates/hdmifiletransporter)
[<img alt="docs.rs" src="https://img.shields.io/badge/docs.hdmifiletransporter-66c2a5?labelColor=555555&logo=docs.rs" height="20">](https://docs.rs/hdmifiletransporter/latest/hdmifiletransporter)
[![CI Build](https://github.com/MrDesjardins/hdmifiletransporter/actions/workflows/rust.yml/badge.svg?branch=master)](https://github.com/MrDesjardins/hdmifiletransporter/actions/workflows/rust.yml)
[![codecov](https://codecov.io/gh/MrDesjardins/hdmifiletransporter/branch/master/graph/badge.svg?token=58EGU3M0A1)](https://codecov.io/gh/MrDesjardins/hdmifiletransporter)

This repository is a Rust implementation for a proof-of-concept to transfer files using video by leveraging HDMI from one computer and USB on a second computer. The computer sending the information might be in a secured environment with very restricted access to Internet or USB devices that can be connected. However, monitors are rarely a source targeted by security. Thus, the concept is to send files using HDMI and captured using a video card with USB on a second computer with no security restriction.

![](./readmeAssets/BlogHdmiFileTransporterConcept.drawio.png)

For details about the concept and code visit these articles:

1. [How to Transfer Files Between Computer Using HDMI (Part 1: Plan)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-1-Plan)
1. [How to Transfer Files Between Computers Using HDMI (Part 2: Prototype Code Video Creation)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-2-prototype-code-video)
1. [How to Transfer Files Between Computers Using HDMI (Part 3: Reading Video)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-3-reading-video)
1. [How to Transfer Files Between Computers Using HDMI (Part 4: HDMI Failure)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-4-hdmi-failure)
1. [How to Transfer Files Between Computers Using HDMI (Part 5: Instruction Header)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-5-instruction-header)
1. [How to Transfer Files Between Computers Using HDMI (Part 6: Instruction and Black and White)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-6-instruction-black-white)
1. [How to Transfer Files Between Computers Using HDMI (Part 7: Pagination)](https://patrickdesjardins.com/blog/how-to-transfer-files-between-computers-using-HDMI-Part-7-pagination)

# Scope of this Code Base

The code base contains a Rust script that inject a file into a video file. Also, it does the other side: from video file to file.

What is out of scope is the HDMI part. The details can be found in the several articles written.

# Information for the Consumers of the Library


## Install

```sh
cargo add hdmifiletransporter
```

# Information for the Consumers of the CLI

todo: Different options we can use with the CLI

# Information for the Developers of the Repository
This section is intended for developers who are contributing to this repository. They are few pointers to how to perform development tasks.

## What to Install?

You need to install the right toolchain:

```sh
rustup toolchain install stable
rustup default stable
```

To perform test coverage you need to install

```sh
cargo install grcov
rustup component add llvm-tools-preview
```

To generate benchmark plots you need to install GnuPlot

```sh
sudo apt update
sudo apt install gnuplot

# To confirm that it is properly installed:
which gnuplot
```

To use opencv on WSL:

```sh
sudo su 
apt install libopencv-dev clang libclang-dev
sudo apt install cmake
```

## Execute

To get all options using `cargo run`:

```sh
cargo run -- -help
```

## Tests

All tests:

```sh
cargo test
```

Only integration tests:

```sh
cargo test --test "*"
```

## Tests Coverage

You must install few components before running coverage:

```sh
cargo install grcov
rustup component add llvm-tools-preview
```

Then, you can run:

```sh
./coverage.sh
```

Further explanation in the [Mozilla grcov website](https://github.com/mozilla/grcov)

## Documentation
The documentation is generated from the source code using:

```sh
cargo doc --open  -document-private-items
```

## Testing CLI

All commands for the user works but instead of using 

```sh
hdmifiletransporter -m inject -i testAssets/test1.zip -o out1.mp4
```

### Inject Text to Video

```sh
cargo run -- -m inject -i testAssets/text1.txt -o outputs/out1.mp4 --fps 30 --height 1080 --width 1920 --size 1 -p true

cargo run -- -m inject -i testAssets/text1.txt -o outputs/out1.mp4 --fps 30 --height 1080 --width 1920 --size 1 -p true -a bw

```
### Extract Text from Video

```sh
cargo run -- -m extract -i outputs/out1.mp4 -o outputs/text1.txt --fps 30 --height 1080 --width 1920 --size 1 -p true

cargo run -- -m extract -i outputs/out1.mp4 -o outputs/text1.txt --fps 30 --height 1080 --width 1920 --size 1 -p true -a bw
```
# Benchmark

```sh
cargo bench
```

# Publishing

## Test the Cargo Content

```sh
cargo package --allow-dirty
```

Then go to `hdmifiletransporter/target/package/` to see the content

## Push a new Cargo Package

```sh
cargo login
cargo publish --dry-run
cargo publish
```

# Debugging

You must install [CodeLLDB](https://marketplace.visualstudio.com/items?itemName=vadimcn.vscode-lldb) if you want to set break point with VsCode.

# ffmpeg Command to Read From Other Computer 

The generated video file must be ran at the same resolution as the one available for your video card. Then, on the other side of the card run this command until at least you see two times a red frames. If you are on a Windows machine, the video card might not be accessible (easily) using WSL. Thus, you might want to install and run ffmeg on a Windows terminal.

```sh
ffmpeg -r 30 -f dshow -s 1920x1080 -vcodec mjpeg -i video="USB Video" -r 30 out.mp4
```

# Other Bins
There is another bin called `colorframe`. It creates a small video with colors that change around the edge for testing purposed of the capture card.

```sh
cargo run --bin=colorframe
cargo run --bin=diagonal
```