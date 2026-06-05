//! Capture-simulation harness.
//!
//! These tests prove that the registration + per-frame CRC pipeline survives the
//! kind of perturbations a real HDMI display -> capture-card path introduces:
//! a positional offset/overscan, anisotropic rescaling, and lossy JPEG-style
//! compression. We build frames with the real encoder, distort each frame, then
//! feed them back through `register_frame` + `frames_to_data` and assert the
//! original bytes are recovered exactly in black/white mode (the HDMI-grade mode).

use hdmifiletransporter::{
    create_starting_frame, data_to_frames, frames_to_data, options::AlgoFrame, register_frame,
    ExtractOptions, InjectOptions, VideoFrame,
};
use opencv::core::{copy_make_border, Mat, Scalar, Size, Vector, BORDER_CONSTANT};
use opencv::imgcodecs::{imdecode, imencode, IMREAD_COLOR, IMWRITE_JPEG_QUALITY};
use opencv::imgproc::{resize, INTER_LINEAR};
use opencv::prelude::*;

// Cells are 6x6 pixels so the 7-cell finder patterns (42 px) and their 6 px
// rings survive JPEG compression and rescaling.
const WIDTH: u16 = 384;
const HEIGHT: u16 = 384;
const SIZE: u8 = 6;

fn inject_options(algo: AlgoFrame) -> InjectOptions {
    InjectOptions {
        file_path: String::new(),
        output_video_file: String::new(),
        fps: 30,
        width: WIDTH,
        height: HEIGHT,
        size: SIZE,
        algo,
        show_progress: false,
    }
}

fn extract_options(algo: AlgoFrame) -> ExtractOptions {
    ExtractOptions {
        video_file_path: String::new(),
        extracted_file_path: String::new(),
        fps: 30,
        width: WIDTH,
        height: HEIGHT,
        size: SIZE,
        algo,
        show_progress: false,
    }
}

fn build_frames(data: &[u8], algo: AlgoFrame) -> Vec<VideoFrame> {
    let io = inject_options(algo);
    let mut frames = vec![create_starting_frame(data.len() as u64, &io)];
    frames.extend(data_to_frames(&io, data.to_vec()));
    frames
}

/// Mimic a capture pipeline: pad (offset/overscan), rescale anisotropically
/// (scaling), then JPEG round-trip (chroma/DCT loss).
fn simulate_capture(canonical: &Mat) -> Mat {
    let mut padded = Mat::default();
    copy_make_border(
        canonical,
        &mut padded,
        9,  // top
        5,  // bottom
        13, // left
        7,  // right
        BORDER_CONSTANT,
        Scalar::new(255.0, 255.0, 255.0, 0.0), // white quiet surroundings
    )
    .expect("pad");

    let new_size = Size::new(
        (padded.cols() as f64 * 1.2) as i32,
        (padded.rows() as f64 * 0.85) as i32,
    );
    let mut resized = Mat::default();
    resize(&padded, &mut resized, new_size, 0.0, 0.0, INTER_LINEAR).expect("resize");

    let mut params: Vector<i32> = Vector::new();
    params.push(IMWRITE_JPEG_QUALITY);
    params.push(90);
    let mut buf: Vector<u8> = Vector::new();
    imencode(".jpg", &resized, &mut buf, &params).expect("jpeg encode");
    imdecode(&buf, IMREAD_COLOR).expect("jpeg decode")
}

#[test]
fn test_clean_registration_round_trip_bw() {
    let data: Vec<u8> = (0..200u32).map(|i| (i * 7 % 251) as u8).collect();
    let frames = build_frames(&data, AlgoFrame::BW);

    // Feed the canonical frames straight through registration (identity warp).
    let mut registered = Vec::new();
    for f in &frames {
        let vf = register_frame(&f.image, WIDTH, HEIGHT, SIZE)
            .expect("markers must be found in a clean frame");
        registered.push(vf);
    }

    let result = frames_to_data(&extract_options(AlgoFrame::BW), registered);
    assert_eq!(result, data);
}

#[test]
fn test_capture_simulation_bw_recovers_exact_bytes() {
    let data: Vec<u8> = (0..200u32).map(|i| (i * 13 % 251) as u8).collect();
    let frames = build_frames(&data, AlgoFrame::BW);

    let mut registered = Vec::new();
    for f in &frames {
        let perturbed = simulate_capture(&f.image);
        if let Some(vf) = register_frame(&perturbed, WIDTH, HEIGHT, SIZE) {
            registered.push(vf);
        }
    }

    assert!(
        registered.len() >= 2,
        "registration should recover the Start frame and at least one data frame"
    );

    let result = frames_to_data(&extract_options(AlgoFrame::BW), registered);
    assert_eq!(
        result, data,
        "BW mode must recover the exact bytes through a simulated capture"
    );
}

#[test]
fn test_capture_simulation_quantized_2_levels_recovers_exact_bytes() {
    // 2 levels/channel keeps the colours maximally separated (0 and 255) while
    // packing 3 bits/cell (3x BW density), so it should survive the same
    // simulated capture path that BW does.
    let algo = AlgoFrame::Quantized(2);
    let data: Vec<u8> = (0..240u32).map(|i| (i * 11 % 251) as u8).collect();
    let frames = build_frames(&data, algo);

    let mut registered = Vec::new();
    for f in &frames {
        let perturbed = simulate_capture(&f.image);
        if let Some(vf) = register_frame(&perturbed, WIDTH, HEIGHT, SIZE) {
            registered.push(vf);
        }
    }

    assert!(
        registered.len() >= 2,
        "registration should recover the Start frame and at least one data frame"
    );

    let result = frames_to_data(&extract_options(algo), registered);
    assert_eq!(
        result, data,
        "Quantized(2) must recover the exact bytes through a simulated capture"
    );
}

fn assert_brightness_capture_round_trip(levels: u32) {
    let algo = AlgoFrame::Brightness(levels);
    let data: Vec<u8> = (0..240u32).map(|i| (i * 17 % 251) as u8).collect();
    let frames = build_frames(&data, algo);

    let mut registered = Vec::new();
    for f in &frames {
        let perturbed = simulate_capture(&f.image);
        if let Some(vf) = register_frame(&perturbed, WIDTH, HEIGHT, SIZE) {
            registered.push(vf);
        }
    }

    assert!(
        registered.len() >= 2,
        "registration should recover the Start frame and at least one data frame"
    );

    let result = frames_to_data(&extract_options(algo), registered);
    assert_eq!(
        result, data,
        "Brightness({levels}) must recover the exact bytes through a simulated capture"
    );
}

#[test]
fn test_capture_simulation_brightness_2_levels_recovers_exact_bytes() {
    assert_brightness_capture_round_trip(2);
}

#[test]
fn test_capture_simulation_brightness_4_levels_recovers_exact_bytes() {
    // Luma carries data on full-resolution luminance, so even 4 grey levels
    // (spacing 85) survive the capture path - this is the whole point of the
    // brightness mode versus packing the same levels into the subsampled chroma.
    assert_brightness_capture_round_trip(4);
}
