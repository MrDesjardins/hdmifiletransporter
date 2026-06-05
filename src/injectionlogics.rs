use opencv::{
    core::Size,
    videoio::{VideoWriter, VideoWriterTrait, VideoWriterTraitConst},
};
use std::fs;

use crate::{
    bitlogics::{bits_per_channel, get_bit_at, get_rgb_for_bit, symbol_to_value},
    injectionextraction::{
        cells_high, cells_wide, content_cell_xy, frame_capacity, HEADER_BITS, NULL_CHAR,
    },
    instructionlogics::{FrameHeader, FrameType},
    options::{AlgoFrame, InjectOptions},
    videoframe::VideoFrame,
};

pub fn file_to_data(options: &InjectOptions) -> Vec<u8> {
    fs::read(&options.file_path).unwrap_or_else(|err| {
        panic!(
            "Unable to read file: {} with error: {}",
            &options.file_path, err
        )
    })
}
use indicatif::ProgressBar;

/// Create a starting frame to indicate that we are starting the transmission of the data.
///
/// Needed because the source will play the video with all the data in a loop. The consumer
/// reads the stream until it finds this Start frame, which carries the total number of data
/// bytes to expect (`total_data_size`).
///
/// The frame is filled red as a human visual cue, then the calibration ring (used to
/// re-align a captured frame) and the CRC-protected header are drawn on top.
pub fn create_starting_frame(total_data_size: u64, inject_options: &InjectOptions) -> VideoFrame {
    let size = inject_options.size;
    let mut frame = VideoFrame::new(inject_options.width, inject_options.height);
    // Fill only the full cells of the grid. Iterating raw width/height with
    // `step_by(size)` would start a cell at the last partial column/row when the
    // dimension is not a multiple of `size` (e.g. width 1280, size 3 -> x = 1278),
    // and `write` would then run past the frame edge. `cells_wide`/`cells_high`
    // floor to whole cells, matching the calibration/header/content writers.
    let cols = cells_wide(inject_options.width, size);
    let rows = cells_high(inject_options.height, size);
    for cy in 0..rows {
        for cx in 0..cols {
            let x = (cx * usize::from(size)) as u16;
            let y = (cy * usize::from(size)) as u16;
            frame.write(255, 0, 0, x, y, size); // full red visual cue
        }
    }
    frame.write_calibration(size);
    // The Start frame has no payload; its CRC covers only the type and value.
    let header = FrameHeader::new(FrameType::Start, total_data_size, &[]);
    frame.write_header(&header, size);
    frame
}

pub fn data_to_frames(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    match inject_options.algo {
        AlgoFrame::RGB => data_to_frames_method_rgb(inject_options, data),
        AlgoFrame::BW => data_to_frames_method_bw(inject_options, data),
        AlgoFrame::Quantized(levels) => {
            data_to_frames_method_quantized(inject_options, data, levels)
        }
        AlgoFrame::Brightness(levels) => {
            data_to_frames_method_brightness(inject_options, data, levels)
        }
    }
}

/// Slice `data` into the payload for page `page`, padded with NULL_CHAR up to
/// `bytes_per_frame` so every frame carries a fixed-size payload (the trailing
/// padding of the last frame is dropped at extraction time using the Start
/// frame's total byte count).
fn page_payload(data: &[u8], page: usize, bytes_per_frame: usize) -> Vec<u8> {
    let start = page * bytes_per_frame;
    let end = std::cmp::min(start + bytes_per_frame, data.len());
    let mut payload = if start < data.len() {
        data[start..end].to_vec()
    } else {
        Vec::new()
    };
    payload.resize(bytes_per_frame, NULL_CHAR);
    payload
}

/// Move data into many frames using RGB: each content cell holds 3 bytes (R, G, B).
fn data_to_frames_method_rgb(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    let size = inject_options.size;
    let width = inject_options.width;
    let height = inject_options.height;

    let capacity = frame_capacity(width, height, size);
    if capacity == 0 {
        panic!(
            "Frame is too small to hold the header and any payload. Increase width/height (content cells must exceed {} header cells).",
            HEADER_BITS
        );
    }
    let bytes_per_frame = capacity * 3; // 3 bytes per cell

    let total_data = data.len();
    let total_frames = total_data.div_ceil(bytes_per_frame);

    let pb = ProgressBar::new(total_frames as u64);
    if inject_options.show_progress {
        println!(
            "Inserting {} bytes into {} frames (RGB)",
            total_data, total_frames
        );
    }

    let mut frames: Vec<VideoFrame> = Vec::with_capacity(total_frames);
    for page in 0..total_frames {
        let payload = page_payload(&data, page, bytes_per_frame);
        let mut frame = VideoFrame::new(width, height);
        frame.write_calibration(size);
        let header = FrameHeader::new(FrameType::Data, page as u64, &payload);
        frame.write_header(&header, size);

        for cell in 0..capacity {
            let bi = cell * 3;
            let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
            frame.write(payload[bi], payload[bi + 1], payload[bi + 2], x, y, size);
        }

        frames.push(frame);
        if inject_options.show_progress {
            pb.inc(1);
        }
    }
    if inject_options.show_progress {
        pb.finish_with_message("done");
    }
    frames
}

/// Move data into many frames using black and white: each content cell holds one
/// bit (black = 0, white = 1), so 8 cells hold one byte. More robust over a lossy
/// HDMI capture than RGB.
fn data_to_frames_method_bw(inject_options: &InjectOptions, data: Vec<u8>) -> Vec<VideoFrame> {
    let size = inject_options.size;
    let width = inject_options.width;
    let height = inject_options.height;

    let capacity = frame_capacity(width, height, size);
    if capacity < 8 {
        panic!(
            "Frame is too small to hold the header and at least one byte of payload. Increase width/height (need more than {} header cells plus 8).",
            HEADER_BITS
        );
    }
    let bytes_per_frame = capacity / 8; // 8 cells per byte

    let total_data = data.len();
    let total_frames = total_data.div_ceil(bytes_per_frame);

    let pb = ProgressBar::new(total_frames as u64);
    if inject_options.show_progress {
        println!(
            "Inserting {} bytes into {} frames (BW)",
            total_data, total_frames
        );
    }

    let mut frames: Vec<VideoFrame> = Vec::with_capacity(total_frames);
    for page in 0..total_frames {
        let payload = page_payload(&data, page, bytes_per_frame);
        let mut frame = VideoFrame::new(width, height);
        frame.write_calibration(size);
        let header = FrameHeader::new(FrameType::Data, page as u64, &payload);
        frame.write_header(&header, size);

        let mut cell = 0;
        for byte in &payload {
            for bit_pos in (0u8..8).rev() {
                // Most-significant bit first.
                let bit = get_bit_at(*byte, bit_pos);
                let (r, g, b) = get_rgb_for_bit(bit);
                let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
                frame.write(r, g, b, x, y, size);
                cell += 1;
            }
        }
        // Fill any leftover content cells (capacity not a multiple of 8) with black.
        while cell < capacity {
            let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
            frame.write(0, 0, 0, x, y, size);
            cell += 1;
        }

        frames.push(frame);
        if inject_options.show_progress {
            pb.inc(1);
        }
    }
    if inject_options.show_progress {
        pb.finish_with_message("done");
    }
    frames
}

/// Move data into many frames using quantized colour: each channel of each cell
/// carries one of `levels` evenly spaced symbols, i.e. `log2(levels)` bits per
/// channel and `3*log2(levels)` bits per cell. This sits between BW (1 bit/cell,
/// most robust) and RGB (24 bits/cell, least robust): picking a small `levels`
/// keeps the colours far apart (resilient) while still packing several bits per
/// cell (denser than BW).
fn data_to_frames_method_quantized(
    inject_options: &InjectOptions,
    data: Vec<u8>,
    levels: u32,
) -> Vec<VideoFrame> {
    let size = inject_options.size;
    let width = inject_options.width;
    let height = inject_options.height;

    let bits_chan = bits_per_channel(levels) as usize;
    let capacity = frame_capacity(width, height, size);
    // Each cell holds 3 * bits_chan payload bits; we only fill whole bytes.
    let bytes_per_frame = capacity * 3 * bits_chan / 8;
    if bytes_per_frame == 0 {
        panic!(
            "Frame is too small to hold the header and at least one byte of payload at {levels} levels/channel. Increase width/height or levels."
        );
    }

    let total_data = data.len();
    let total_frames = total_data.div_ceil(bytes_per_frame);

    let pb = ProgressBar::new(total_frames as u64);
    if inject_options.show_progress {
        println!(
            "Inserting {} bytes into {} frames (Quantized, {} levels/channel)",
            total_data, total_frames, levels
        );
    }

    let mut frames: Vec<VideoFrame> = Vec::with_capacity(total_frames);
    for page in 0..total_frames {
        let payload = page_payload(&data, page, bytes_per_frame);
        let mut frame = VideoFrame::new(width, height);
        frame.write_calibration(size);
        let header = FrameHeader::new(FrameType::Data, page as u64, &payload);
        frame.write_header(&header, size);

        // Walk the payload as a most-significant-bit-first bit stream, pulling
        // `bits_chan` bits per channel (R, then G, then B) for each cell.
        let total_bits = payload.len() * 8;
        let mut bit_index = 0usize;
        for cell in 0..capacity {
            let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
            let mut channel = [0u8; 3];
            for slot in channel.iter_mut() {
                let mut symbol = 0u32;
                for _ in 0..bits_chan {
                    let bit = if bit_index < total_bits {
                        (payload[bit_index / 8] >> (7 - (bit_index % 8))) & 1
                    } else {
                        0
                    };
                    symbol = (symbol << 1) | bit as u32;
                    bit_index += 1;
                }
                *slot = symbol_to_value(symbol, levels);
            }
            frame.write(channel[0], channel[1], channel[2], x, y, size);
        }

        frames.push(frame);
        if inject_options.show_progress {
            pb.inc(1);
        }
    }
    if inject_options.show_progress {
        pb.finish_with_message("done");
    }
    frames
}

/// Move data into many frames using brightness (luma): each content cell is a
/// single grey shade (R = G = B) chosen from `levels` evenly spaced values, so a
/// cell carries `log2(levels)` bits. Capture cards keep luminance at full
/// resolution but subsample colour, so data hidden in brightness survives
/// compression much better than the same number of levels spread across the
/// colour channels.
fn data_to_frames_method_brightness(
    inject_options: &InjectOptions,
    data: Vec<u8>,
    levels: u32,
) -> Vec<VideoFrame> {
    let size = inject_options.size;
    let width = inject_options.width;
    let height = inject_options.height;

    let bits_cell = bits_per_channel(levels) as usize; // one symbol per cell
    let capacity = frame_capacity(width, height, size);
    let bytes_per_frame = capacity * bits_cell / 8;
    if bytes_per_frame == 0 {
        panic!(
            "Frame is too small to hold the header and at least one byte of payload at {levels} brightness levels. Increase width/height or levels."
        );
    }

    let total_data = data.len();
    let total_frames = total_data.div_ceil(bytes_per_frame);

    let pb = ProgressBar::new(total_frames as u64);
    if inject_options.show_progress {
        println!(
            "Inserting {} bytes into {} frames (Brightness, {} levels)",
            total_data, total_frames, levels
        );
    }

    let mut frames: Vec<VideoFrame> = Vec::with_capacity(total_frames);
    for page in 0..total_frames {
        let payload = page_payload(&data, page, bytes_per_frame);
        let mut frame = VideoFrame::new(width, height);
        frame.write_calibration(size);
        let header = FrameHeader::new(FrameType::Data, page as u64, &payload);
        frame.write_header(&header, size);

        let total_bits = payload.len() * 8;
        let mut bit_index = 0usize;
        for cell in 0..capacity {
            let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
            let mut symbol = 0u32;
            for _ in 0..bits_cell {
                let bit = if bit_index < total_bits {
                    (payload[bit_index / 8] >> (7 - (bit_index % 8))) & 1
                } else {
                    0
                };
                symbol = (symbol << 1) | bit as u32;
                bit_index += 1;
            }
            let value = symbol_to_value(symbol, levels);
            frame.write(value, value, value, x, y, size);
        }

        frames.push(frame);
        if inject_options.show_progress {
            pb.inc(1);
        }
    }
    if inject_options.show_progress {
        pb.finish_with_message("done");
    }
    frames
}

pub fn frames_to_video(options: InjectOptions, frames: Vec<VideoFrame>) -> Result<(), String> {
    let frame_size = Size {
        height: options.height as i32,
        width: options.width as i32,
    };

    // Make sure the destination folder exists, otherwise the video writer
    // silently fails to open the file.
    if let Some(parent) = std::path::Path::new(&options.output_video_file).parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "Unable to create output directory {:?}: {}",
                    parent, err
                )
            })?;
        }
    }

    // Fourcc is a code for video codecs. The data is encoded directly into pixel
    // values, so the codec MUST be lossless otherwise the recovered bytes differ
    // from the injected ones. FFV1 is a lossless codec; it requires a container
    // such as .mkv or .avi (a lossy .mp4 would corrupt the data).
    // See list of codec here: https://learn.fotoware.com/On-Premises/Getting_started/Metadata_in_the_FotoWare_system/04_Operators_to_search_in_specific_fields/FourCC_codes
    // Careful, codec and file extension must match.
    let fourcc = VideoWriter::fourcc('F', 'F', 'V', '1')
        .map_err(|error| format!("Unable to build the fourcc code: {:?}", error))?;

    let total_frames = frames.len() as u64;
    if options.show_progress {
        println!("Frames to video");
    }
    let pb = ProgressBar::new(total_frames);

    let mut video = VideoWriter::new(
        options.output_video_file.as_str(),
        fourcc,
        options.fps.into(),
        frame_size,
        true,
    )
    .map_err(|error_video| format!("Error with video writer: {:?}", error_video))?;

    if !video
        .is_opened()
        .map_err(|error| format!("Error checking the video writer state: {:?}", error))?
    {
        return Err(format!(
            "Unable to open the video file for writing: {}",
            options.output_video_file
        ));
    }

    for frame in frames {
        let image = frame.image;
        video
            .write(&image)
            .map_err(|error| format!("A frame could not be written: {:?}", error))?;
        if options.show_progress {
            pb.inc(1);
        }
    }

    video
        .release()
        .map_err(|error_release| format!("Error saving the video: {:?}", error_release))?;

    if options.show_progress {
        pb.finish_with_message("done");
        println!("Video saved:{}", options.output_video_file.as_str());
    }
    Ok(())
}

#[cfg(test)]
mod injectionlogics_tests {
    use super::*;
    use crate::injectionextraction::frame_capacity;
    use crate::options::AlgoFrame;

    fn opts(algo: AlgoFrame, width: u16, height: u16, size: u8) -> InjectOptions {
        InjectOptions {
            file_path: String::new(),
            output_video_file: String::new(),
            fps: 30,
            width,
            height,
            size,
            algo,
            show_progress: false,
        }
    }

    fn read_header_bits(frame: &VideoFrame, width: u16, size: u8) -> Vec<bool> {
        (0..HEADER_BITS)
            .map(|i| {
                let (x, y) = content_cell_xy(i, width, size);
                let c = frame.read_coordinate_color(x, y);
                (c.r as u32 + c.g as u32 + c.b as u32) >= 382 // white => bit set
            })
            .collect()
    }

    #[test]
    fn test_data_to_frames_rgb_frame_count() {
        let options = opts(AlgoFrame::RGB, 64, 64, 1);
        let bytes_per_frame = frame_capacity(64, 64, 1) * 3;
        let data = vec![7u8; bytes_per_frame * 2 + 5];
        let frames = data_to_frames_method_rgb(&options, data);
        assert_eq!(frames.len(), 3);
    }

    #[test]
    fn test_data_to_frames_bw_frame_count() {
        let options = opts(AlgoFrame::BW, 64, 64, 1);
        let bytes_per_frame = frame_capacity(64, 64, 1) / 8;
        let data = vec![9u8; bytes_per_frame + 1];
        let frames = data_to_frames_method_bw(&options, data);
        assert_eq!(frames.len(), 2);
    }

    #[test]
    #[should_panic]
    fn test_data_to_frames_rgb_frame_too_small() {
        // 24x24 content is only 8x8 = 64 cells, less than the header -> capacity 0.
        let options = opts(AlgoFrame::RGB, 24, 24, 1);
        data_to_frames_method_rgb(&options, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_create_starting_frame_is_mostly_red_with_white_border() {
        let io = opts(AlgoFrame::RGB, 64, 64, 1);
        let frame = create_starting_frame(123, &io);
        // A pixel deep inside the content rectangle is red (payload area).
        let c = frame.read_coordinate_color(40, 40);
        assert_eq!((c.r, c.g, c.b), (255, 0, 0));
        // The quiet-zone border corner is white.
        let c = frame.read_coordinate_color(0, 0);
        assert_eq!((c.r, c.g, c.b), (255, 255, 255));
    }

    #[test]
    fn test_create_starting_frame_header_is_start() {
        let io = opts(AlgoFrame::BW, 64, 64, 1);
        let total = 4242u64;
        let frame = create_starting_frame(total, &io);
        let header = FrameHeader::from_bits(&read_header_bits(&frame, 64, 1)).unwrap();
        assert_eq!(header.frame_type, FrameType::Start);
        assert_eq!(header.value, total);
        assert!(header.verify(&[]));
    }

    #[test]
    fn test_data_frame_headers_have_sequential_pages() {
        let io = opts(AlgoFrame::BW, 64, 64, 1);
        let bytes_per_frame = frame_capacity(64, 64, 1) / 8;
        let data = vec![3u8; bytes_per_frame * 3];
        let frames = data_to_frames_method_bw(&io, data);
        assert_eq!(frames.len(), 3);
        for (page, frame) in frames.iter().enumerate() {
            let header = FrameHeader::from_bits(&read_header_bits(frame, 64, 1)).unwrap();
            assert_eq!(header.frame_type, FrameType::Data);
            assert_eq!(header.value, page as u64);
        }
    }
}

