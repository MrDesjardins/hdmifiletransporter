use hdmifiletransporter::{
    create_starting_frame, data_to_frames, execute_with_video_options, frames_to_data,
    options::AlgoFrame, ExtractOptions, InjectOptions, VideoOptions,
};
use std::fs;
use std::path::PathBuf;

// Frames must be large enough to hold the calibration ring plus the header and
// some payload, so we use 64x64 (content is 48x48 cells with size 1).
const TEST_WIDTH: u16 = 64;
const TEST_HEIGHT: u16 = 64;

fn get_unit_test_injection_option(size: u8, width: u16, height: u16) -> InjectOptions {
    InjectOptions {
        fps: 30,
        width,
        height,
        size,
        algo: AlgoFrame::BW,
        show_progress: false,
        file_path: "".to_string(),
        output_video_file: "".to_string(),
    }
}

fn get_unit_test_extraction_option(size: u8, width: u16, height: u16) -> ExtractOptions {
    ExtractOptions {
        video_file_path: "".to_string(),
        extracted_file_path: "".to_string(),
        fps: 30,
        width,
        height,
        size,
        algo: AlgoFrame::BW,
        show_progress: false,
    }
}

fn get_unit_test_data(number_of_byte: u64) -> Vec<u8> {
    let mut result = Vec::new();
    for i in 0..number_of_byte {
        result.push(((i % 65) + 65) as u8);
    }
    result
}

fn swap_elements<T: Clone>(vec: &mut [T], index1: usize, index2: usize) {
    vec.swap(index1, index2);
}

#[test]
fn test_frames_to_data_all_frames_good_order() {
    let size = 1;
    let inject_options = get_unit_test_injection_option(size, TEST_WIDTH, TEST_HEIGHT);
    let extract_options = get_unit_test_extraction_option(size, TEST_WIDTH, TEST_HEIGHT);

    // Enough bytes to span several data frames.
    let number_bytes = 1000u64;
    let starting_frame = create_starting_frame(number_bytes, &inject_options);

    let frame_data = get_unit_test_data(number_bytes);
    let frames = data_to_frames(&inject_options, frame_data);
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);

    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    assert_eq!(data_from_frames.len(), number_bytes as usize);
    // The extracted bytes must be identical to what was injected, not only the same length.
    assert_eq!(data_from_frames, get_unit_test_data(number_bytes));
}

#[test]
fn test_frames_to_data_all_frames_mixed_order() {
    let size = 1;
    let inject_options = get_unit_test_injection_option(size, TEST_WIDTH, TEST_HEIGHT);
    let extract_options = get_unit_test_extraction_option(size, TEST_WIDTH, TEST_HEIGHT);

    let number_bytes = 1000u64;
    let starting_frame = create_starting_frame(number_bytes, &inject_options);

    let frame_data = get_unit_test_data(number_bytes);
    let frames = data_to_frames(&inject_options, frame_data);

    // Mix the order of the frames.
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);
    swap_elements(&mut merged_frames, 0, 1);
    swap_elements(&mut merged_frames, 2, 3);

    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    assert_eq!(data_from_frames.len(), number_bytes as usize);
    // Page numbers must reassemble the data in the right order despite shuffling.
    assert_eq!(data_from_frames, get_unit_test_data(number_bytes));
}

#[test]
fn test_frames_to_data_all_frames_repeting_frame() {
    let size = 1;
    let inject_options = get_unit_test_injection_option(size, TEST_WIDTH, TEST_HEIGHT);
    let extract_options = get_unit_test_extraction_option(size, TEST_WIDTH, TEST_HEIGHT);

    let number_bytes = 1000u64;
    let starting_frame = create_starting_frame(number_bytes, &inject_options);

    let frame_data = get_unit_test_data(number_bytes);
    let frames = data_to_frames(&inject_options, frame_data);
    let clone1 = frames[0].clone();
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);
    merged_frames.push(clone1); // Add the first frame twice

    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    assert_eq!(data_from_frames.len(), number_bytes as usize);
    // The duplicated frame must be de-duplicated and not corrupt the content.
    assert_eq!(data_from_frames, get_unit_test_data(number_bytes));
}

#[test]
#[should_panic]
fn test_frames_to_data_missing_one_frame() {
    let size = 1;
    let inject_options = get_unit_test_injection_option(size, TEST_WIDTH, TEST_HEIGHT);
    let extract_options = get_unit_test_extraction_option(size, TEST_WIDTH, TEST_HEIGHT);

    let number_bytes = 1000u64;
    let starting_frame = create_starting_frame(number_bytes, &inject_options);

    let frame_data = get_unit_test_data(number_bytes);
    let frames = data_to_frames(&inject_options, frame_data);
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);
    merged_frames.remove(2); // Drop a data frame -> a page is missing.

    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    assert_eq!(data_from_frames.len(), number_bytes as usize)
}

/// Inject `data` into a real video file and extract it back, asserting the round
/// trip reproduces the bytes exactly. This proves the codec used by
/// `frames_to_video` is lossless and that registration works on clean frames.
fn assert_video_round_trip(
    algo: AlgoFrame,
    width: u16,
    height: u16,
    size: u8,
    data: Vec<u8>,
    label: &str,
) {
    let dir: PathBuf = std::env::temp_dir().join(format!(
        "hdmift_{}_{}_{}",
        label,
        std::process::id(),
        data.len()
    ));
    fs::create_dir_all(&dir).expect("create temp dir");
    let input_path = dir.join("input.bin");
    let video_path = dir.join("video.mkv");
    let output_path = dir.join("output.bin");

    fs::write(&input_path, &data).expect("write input file");

    execute_with_video_options(VideoOptions::InjectInVideo(InjectOptions {
        file_path: input_path.to_string_lossy().to_string(),
        output_video_file: video_path.to_string_lossy().to_string(),
        fps: 30,
        width,
        height,
        size,
        algo,
        show_progress: false,
    }))
    .expect("injection should succeed");

    execute_with_video_options(VideoOptions::ExtractFromVideo(ExtractOptions {
        video_file_path: video_path.to_string_lossy().to_string(),
        extracted_file_path: output_path.to_string_lossy().to_string(),
        fps: 30,
        width,
        height,
        size,
        algo,
        show_progress: false,
    }))
    .expect("extraction should succeed");

    let extracted = fs::read(&output_path).expect("read extracted file");
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(extracted, data, "round trip for {} must be lossless", label);
}

// Use 6x6 cells on a 384x384 frame: large enough that the 7-cell finder
// patterns (42 px) survive the affine registration warp's sub-pixel resampling.
// 1x1 cells (the old value) are corrupted by any fractional grid offset the warp
// introduces, which is unrepresentative of real usage.
const RT_WIDTH: u16 = 384;
const RT_HEIGHT: u16 = 384;
const RT_SIZE: u8 = 6;

#[test]
fn test_video_round_trip_bw() {
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    assert_video_round_trip(AlgoFrame::BW, RT_WIDTH, RT_HEIGHT, RT_SIZE, data, "bw");
}

#[test]
fn test_video_round_trip_rgb() {
    let data: Vec<u8> = (0..1000u32).map(|i| (i % 251) as u8).collect();
    assert_video_round_trip(AlgoFrame::RGB, RT_WIDTH, RT_HEIGHT, RT_SIZE, data, "rgb");
}

#[test]
#[should_panic(expected = "Instruction not found while extracting data from video")]
fn test_frames_to_data_missing_instruction_frame() {
    let size = 1;
    let inject_options = get_unit_test_injection_option(size, TEST_WIDTH, TEST_HEIGHT);
    let extract_options = get_unit_test_extraction_option(size, TEST_WIDTH, TEST_HEIGHT);

    let number_bytes = 1000u64;
    let frame_data = get_unit_test_data(number_bytes);
    let frames = data_to_frames(&inject_options, frame_data);

    // No Start frame -> the total byte count is unknown.
    let data_from_frames = frames_to_data(&extract_options, frames);

    assert_eq!(data_from_frames.len(), number_bytes as usize)
}
