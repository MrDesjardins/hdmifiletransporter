use opencv::videoio::VideoCapture;

use std::fs;

use crate::bitlogics::{bits_per_channel, get_bit_from_rgb, mutate_byte, value_to_symbol};
use crate::injectionextraction::{content_cell_xy, frame_capacity, map_to_size, marker_centers_px, HEADER_BITS};
use crate::instructionlogics::{FrameHeader, FrameType};
use crate::options::AlgoFrame;
use crate::videoframe::VideoFrame;
use opencv::core::{Mat, Point, Point2f, Scalar, Vec4i, Vector, BORDER_CONSTANT};
use opencv::imgproc::{
    contour_area, cvt_color_def, find_contours_with_hierarchy, get_affine_transform_slice, moments,
    threshold, warp_affine, CHAIN_APPROX_SIMPLE, COLOR_BGR2GRAY, INTER_LINEAR, RETR_TREE,
    THRESH_BINARY_INV,
};
use opencv::prelude::*;
use opencv::videoio::CAP_ANY;

use crate::options::ExtractOptions;
use indicatif::ProgressBar;
use std::collections::HashMap;
use std::iter::Iterator;

/// Result of decoding a single (already aligned) frame.
struct FrameBytesInfo {
    /// The parsed header, or `None` when the header could not be read (bad
    /// magic, not one of our frames, or noise from a misaligned frame).
    pub header: Option<FrameHeader>,
    /// Decoded payload bytes (empty for a Start frame).
    pub payload: Vec<u8>,
    /// True when the recomputed CRC matches the header CRC. Only frames with a
    /// valid CRC are trusted.
    pub crc_valid: bool,
}

pub fn video_to_frames(extract_options: &ExtractOptions) -> Vec<VideoFrame> {
    let mut video = VideoCapture::from_file(&extract_options.video_file_path, CAP_ANY)
        .expect("Could not open video path");
    let mut all_frames = Vec::new();
    loop {
        let mut frame = Mat::default();
        video
            .read(&mut frame)
            .expect("Reading frame shouldn't crash");

        if frame.cols() == 0 {
            break;
        }

        // Re-align the captured frame to canonical pixels using the calibration
        // markers. Frames where the markers cannot be located are skipped; in a
        // looped HDMI stream they will be captured cleanly on another pass.
        if let Some(registered) = register_frame(
            &frame,
            extract_options.width,
            extract_options.height,
            extract_options.size,
        ) {
            all_frames.push(registered);
        }
    }

    all_frames
}

/// Locate the three finder patterns in a (possibly offset/scaled/compressed)
/// captured frame and affine-warp it back to canonical `width` x `height`
/// pixels so the cell grid lines up with what the encoder wrote. Returns `None`
/// when the markers cannot be found.
pub fn register_frame(
    image: &Mat,
    width: u16,
    height: u16,
    size: u8,
) -> Option<VideoFrame> {
    let w = image.cols();
    let h = image.rows();
    if w == 0 || h == 0 {
        return None;
    }

    let mut gray = Mat::default();
    // OpenCV 4.11+ added a trailing `hint` arg to cvtColor; `cvt_color_def`
    // keeps the pre-4.11 behavior (dst_cn = 0, default algorithm hint).
    cvt_color_def(image, &mut gray, COLOR_BGR2GRAY).ok()?;
    let mut thresh = Mat::default();
    // Dark finder rings become foreground; the white quiet-zone border drops out.
    threshold(&gray, &mut thresh, 128.0, 255.0, THRESH_BINARY_INV).ok()?;

    let markers = detect_markers(&thresh, w, h)?;
    let src = [
        Point2f::new(markers[0].0, markers[0].1),
        Point2f::new(markers[1].0, markers[1].1),
        Point2f::new(markers[2].0, markers[2].1),
    ];
    let dst_px = marker_centers_px(width, height, size);
    let dst = [
        Point2f::new(dst_px[0].0, dst_px[0].1),
        Point2f::new(dst_px[1].0, dst_px[1].1),
        Point2f::new(dst_px[2].0, dst_px[2].1),
    ];

    let transform = get_affine_transform_slice(&src, &dst).ok()?;
    let mut warped = Mat::default();
    warp_affine(
        image,
        &mut warped,
        &transform,
        map_to_size(width, height),
        INTER_LINEAR,
        BORDER_CONSTANT,
        Scalar::default(),
    )
    .ok()?;

    VideoFrame::from(warped, size).ok()
}

/// Centroid and (absolute) enclosed area of contour `idx`, or `None` when it is
/// degenerate.
fn contour_centroid_area(contours: &Vector<Vector<Point>>, idx: usize) -> Option<(f32, f32, f64)> {
    let contour = contours.get(idx).ok()?;
    let area = contour_area(&contour, false).unwrap_or(0.0).abs();
    if area < f64::EPSILON {
        return None;
    }
    let m = moments(&contour, false).ok()?;
    if m.m00.abs() < f64::EPSILON {
        return None;
    }
    Some(((m.m10 / m.m00) as f32, (m.m01 / m.m00) as f32, area))
}

/// Find the centre of the best finder pattern in each of the three expected
/// corners (top-left, top-right, bottom-left). Returns the three centres in
/// TL/TR/BL order.
///
/// A finder pattern is a QR-style triple of concentric squares: a black outer
/// ring, a white middle ring, and a black centre. After `THRESH_BINARY_INV`
/// (black -> foreground) that yields a very specific contour-tree signature:
/// an outer foreground contour whose child is a hole (the white ring) whose
/// child is an inner foreground contour (the centre), with all three sharing a
/// centre and an outer/inner area ratio near (7/3)^2 ~= 5.4.
///
/// The previous heuristic ("largest contour with >= 2 ancestors in the quadrant")
/// was not specific: dense payload content (black BW cells / dark RGB cells)
/// forms larger nested blobs in the corner quadrants and was selected instead of
/// the real markers, producing a misaligned warp that corrupted every frame -
/// even unperturbed ones. Validating the concentric triple makes detection
/// scale-invariant and robust to arbitrary payload content.
fn detect_markers(thresh: &Mat, w: i32, h: i32) -> Option<[(f32, f32); 3]> {
    let mut contours: Vector<Vector<Point>> = Vector::new();
    let mut hierarchy: Vector<Vec4i> = Vector::new();
    find_contours_with_hierarchy(
        thresh,
        &mut contours,
        &mut hierarchy,
        RETR_TREE,
        CHAIN_APPROX_SIMPLE,
        Point::new(0, 0),
    )
    .ok()?;

    let mut candidates: Vec<((f32, f32), f64)> = Vec::new();
    for i in 0..contours.len() {
        // hierarchy entry is [next, prev, first_child, parent].
        let node = hierarchy.get(i).ok()?;
        let child = node[2];
        if child < 0 {
            continue; // no middle-ring hole -> not a finder
        }
        let hole = hierarchy.get(child as usize).ok()?;
        let grand = hole[2];
        if grand < 0 {
            continue; // no inner centre -> not a finder
        }

        let (ox, oy, outer_area) = match contour_centroid_area(&contours, i) {
            Some(v) => v,
            None => continue,
        };
        let (gx, gy, inner_area) = match contour_centroid_area(&contours, grand as usize) {
            Some(v) => v,
            None => continue,
        };

        // Reject sub-pixel noise; finders are several cells across.
        if outer_area < 16.0 || inner_area < f64::EPSILON {
            continue;
        }

        // Concentric: the inner centre must sit on top of the outer centre,
        // tolerant to within ~30% of the outer square's side (sqrt of its area).
        let outer_side = (outer_area as f32).sqrt();
        let dx = ox - gx;
        let dy = oy - gy;
        if (dx * dx + dy * dy).sqrt() > 0.3 * outer_side {
            continue;
        }

        // Outer/inner area ratio: ideal (7/3)^2 ~= 5.44. A generous window keeps
        // it tolerant to warp/blur while still rejecting unrelated nestings.
        let ratio = outer_area / inner_area;
        if !(2.5..=12.0).contains(&ratio) {
            continue;
        }

        candidates.push(((ox, oy), outer_area));
    }

    let wf = w as f32;
    let hf = h as f32;
    let tl = best_in_region(&candidates, |cx, cy| cx < wf * 0.4 && cy < hf * 0.4)?;
    let tr = best_in_region(&candidates, |cx, cy| cx > wf * 0.6 && cy < hf * 0.4)?;
    let bl = best_in_region(&candidates, |cx, cy| cx < wf * 0.4 && cy > hf * 0.6)?;
    Some([tl, tr, bl])
}

/// Pick the largest-area candidate whose centre satisfies `pred`.
fn best_in_region(
    candidates: &[((f32, f32), f64)],
    pred: impl Fn(f32, f32) -> bool,
) -> Option<(f32, f32)> {
    candidates
        .iter()
        .filter(|((cx, cy), _)| pred(*cx, *cy))
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(c, _)| *c)
}

/// Decode a collection of (already aligned) frames back into the original bytes.
///
/// Each frame is decoded and its CRC checked; frames that fail the CRC (torn,
/// garbled or transition frames) are dropped. The Start frame supplies the total
/// byte count; Data frames are de-duplicated and ordered by their page number.
pub fn frames_to_data(extract_options: &ExtractOptions, frames: Vec<VideoFrame>) -> Vec<u8> {
    let mut pages: HashMap<u64, Vec<u8>> = HashMap::new();
    let mut total_bytes: Option<u64> = None;
    let mut relevant_frame_count = 0u64;

    let total_video_frame = frames.len() as u64;
    let pb = ProgressBar::new(total_video_frame);
    if extract_options.show_progress {
        println!("Initial Frames count: {}", total_video_frame);
    }

    for frame in frames.iter() {
        let frame_data = match extract_options.algo {
            AlgoFrame::RGB => frame_to_data_method_rgb(frame, extract_options),
            AlgoFrame::BW => frame_to_data_method_bw(frame, extract_options),
            AlgoFrame::Quantized(levels) => {
                frame_to_data_method_quantized(frame, extract_options, levels)
            }
            AlgoFrame::Brightness(levels) => {
                frame_to_data_method_brightness(frame, extract_options, levels)
            }
        };

        if extract_options.show_progress {
            pb.inc(1);
        }

        // Only trust frames whose CRC checks out.
        if !frame_data.crc_valid {
            continue;
        }
        let header = match frame_data.header {
            Some(h) => h,
            None => continue,
        };

        match header.frame_type {
            FrameType::Start => {
                total_bytes = Some(header.value);
                if extract_options.show_progress {
                    println!("Start frame found with data size of {}", header.value);
                }
            }
            FrameType::Data => {
                if let std::collections::hash_map::Entry::Vacant(e) = pages.entry(header.value) {
                    e.insert(frame_data.payload);
                    relevant_frame_count += 1;
                }
            }
        }
    }

    if extract_options.show_progress {
        pb.finish_with_message("done");
        println!("Relevant (unique, valid) data frames: {}", relevant_frame_count);
    }

    match total_bytes {
        Some(expected) => {
            // Merge the pages in order, starting at page 0.
            let mut byte_data = Vec::new();
            let mut page_index = 0u64;
            while let Some(payload) = pages.get(&page_index) {
                byte_data.extend(payload);
                page_index += 1;
            }

            if (byte_data.len() as u64) < expected {
                panic!(
                    "We have not received all frames. We assembled {} pages for a total of {} bytes and expected {} bytes",
                    page_index,
                    byte_data.len(),
                    expected
                );
            }

            // Drop the NULL padding from the last frame.
            byte_data.into_iter().take(expected as usize).collect()
        }
        None => {
            panic!("Instruction not found while extracting data from video");
        }
    }
}

/// Read the per-frame header from the first `HEADER_BITS` content cells. The
/// header is always written black/white regardless of the payload algorithm.
fn read_header(source: &VideoFrame, width: u16, size: u8) -> Option<FrameHeader> {
    let bits: Vec<bool> = (0..HEADER_BITS)
        .map(|i| {
            let (x, y) = content_cell_xy(i, width, size);
            let rgb = get_pixel(source, x as i32, y as i32, size);
            get_bit_from_rgb(&rgb)
        })
        .collect();
    FrameHeader::from_bits(&bits)
}

/// Decode a frame whose payload was encoded with RGB (3 bytes per content cell).
fn frame_to_data_method_rgb(source: &VideoFrame, options: &ExtractOptions) -> FrameBytesInfo {
    let width = options.width;
    let height = options.height;
    let size = options.size;

    let header = match read_header(source, width, size) {
        Some(h) => h,
        None => {
            return FrameBytesInfo {
                header: None,
                payload: Vec::new(),
                crc_valid: false,
            }
        }
    };

    match header.frame_type {
        FrameType::Start => {
            let crc_valid = header.verify(&[]);
            FrameBytesInfo {
                header: Some(header),
                payload: Vec::new(),
                crc_valid,
            }
        }
        FrameType::Data => {
            let capacity = frame_capacity(width, height, size);
            let mut payload = Vec::with_capacity(capacity * 3);
            for cell in 0..capacity {
                let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
                let rgb = get_pixel(source, x as i32, y as i32, size);
                payload.push(rgb[0]);
                payload.push(rgb[1]);
                payload.push(rgb[2]);
            }
            let crc_valid = header.verify(&payload);
            FrameBytesInfo {
                header: Some(header),
                payload,
                crc_valid,
            }
        }
    }
}

/// Decode a frame whose payload was encoded with black/white (1 bit per content
/// cell, 8 cells per byte, most-significant bit first).
fn frame_to_data_method_bw(source: &VideoFrame, options: &ExtractOptions) -> FrameBytesInfo {
    let width = options.width;
    let height = options.height;
    let size = options.size;

    let header = match read_header(source, width, size) {
        Some(h) => h,
        None => {
            return FrameBytesInfo {
                header: None,
                payload: Vec::new(),
                crc_valid: false,
            }
        }
    };

    match header.frame_type {
        FrameType::Start => {
            let crc_valid = header.verify(&[]);
            FrameBytesInfo {
                header: Some(header),
                payload: Vec::new(),
                crc_valid,
            }
        }
        FrameType::Data => {
            let capacity = frame_capacity(width, height, size);
            let bytes_per_frame = capacity / 8;
            let mut payload = Vec::with_capacity(bytes_per_frame);
            let mut data: u8 = 0;
            let mut bit_index: u8 = 7;
            for cell in 0..(bytes_per_frame * 8) {
                let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
                let rgb = get_pixel(source, x as i32, y as i32, size);
                let bit_value = get_bit_from_rgb(&rgb);
                mutate_byte(&mut data, bit_value, bit_index);
                if bit_index == 0 {
                    payload.push(data);
                    data = 0;
                    bit_index = 7;
                } else {
                    bit_index -= 1;
                }
            }
            let crc_valid = header.verify(&payload);
            FrameBytesInfo {
                header: Some(header),
                payload,
                crc_valid,
            }
        }
    }
}

/// Decode a frame whose payload was encoded with quantized colour: each channel
/// of each content cell is rounded to its nearest of `levels` symbols, yielding
/// `log2(levels)` bits per channel. The bit stream is read R, then G, then B per
/// cell, most-significant bit first, matching the injection order.
fn frame_to_data_method_quantized(
    source: &VideoFrame,
    options: &ExtractOptions,
    levels: u32,
) -> FrameBytesInfo {
    let width = options.width;
    let height = options.height;
    let size = options.size;

    let header = match read_header(source, width, size) {
        Some(h) => h,
        None => {
            return FrameBytesInfo {
                header: None,
                payload: Vec::new(),
                crc_valid: false,
            }
        }
    };

    match header.frame_type {
        FrameType::Start => {
            let crc_valid = header.verify(&[]);
            FrameBytesInfo {
                header: Some(header),
                payload: Vec::new(),
                crc_valid,
            }
        }
        FrameType::Data => {
            let bits_chan = bits_per_channel(levels) as usize;
            let capacity = frame_capacity(width, height, size);
            let bytes_per_frame = capacity * 3 * bits_chan / 8;
            let needed_bits = bytes_per_frame * 8;

            let mut bits: Vec<bool> = Vec::with_capacity(needed_bits + 24);
            'cells: for cell in 0..capacity {
                let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
                let rgb = get_pixel(source, x as i32, y as i32, size);
                for &value in &rgb {
                    let symbol = value_to_symbol(value, levels);
                    for k in (0..bits_chan).rev() {
                        bits.push((symbol >> k) & 1 == 1);
                    }
                    if bits.len() >= needed_bits {
                        break 'cells;
                    }
                }
            }

            let mut payload = Vec::with_capacity(bytes_per_frame);
            for byte_index in 0..bytes_per_frame {
                let mut data: u8 = 0;
                for k in 0..8 {
                    let bit = bits.get(byte_index * 8 + k).copied().unwrap_or(false);
                    data = (data << 1) | bit as u8;
                }
                payload.push(data);
            }

            let crc_valid = header.verify(&payload);
            FrameBytesInfo {
                header: Some(header),
                payload,
                crc_valid,
            }
        }
    }
}

/// Decode a frame whose payload was encoded with brightness (luma): each content
/// cell is a single grey shade, so its three (averaged) channels are collapsed to
/// one value and rounded to the nearest of `levels` symbols, yielding
/// `log2(levels)` bits per cell, most-significant bit first.
fn frame_to_data_method_brightness(
    source: &VideoFrame,
    options: &ExtractOptions,
    levels: u32,
) -> FrameBytesInfo {
    let width = options.width;
    let height = options.height;
    let size = options.size;

    let header = match read_header(source, width, size) {
        Some(h) => h,
        None => {
            return FrameBytesInfo {
                header: None,
                payload: Vec::new(),
                crc_valid: false,
            }
        }
    };

    match header.frame_type {
        FrameType::Start => {
            let crc_valid = header.verify(&[]);
            FrameBytesInfo {
                header: Some(header),
                payload: Vec::new(),
                crc_valid,
            }
        }
        FrameType::Data => {
            let bits_cell = bits_per_channel(levels) as usize;
            let capacity = frame_capacity(width, height, size);
            let bytes_per_frame = capacity * bits_cell / 8;
            let needed_bits = bytes_per_frame * 8;

            let mut bits: Vec<bool> = Vec::with_capacity(needed_bits + 8);
            'cells: for cell in 0..capacity {
                let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
                let rgb = get_pixel(source, x as i32, y as i32, size);
                // Collapse to luma: the channels are nominally equal, so an
                // average rejects per-channel chroma noise.
                let gray = ((rgb[0] as u32 + rgb[1] as u32 + rgb[2] as u32) / 3) as u8;
                let symbol = value_to_symbol(gray, levels);
                for k in (0..bits_cell).rev() {
                    bits.push((symbol >> k) & 1 == 1);
                    if bits.len() >= needed_bits {
                        break 'cells;
                    }
                }
            }

            let mut payload = Vec::with_capacity(bytes_per_frame);
            for byte_index in 0..bytes_per_frame {
                let mut data: u8 = 0;
                for k in 0..8 {
                    let bit = bits.get(byte_index * 8 + k).copied().unwrap_or(false);
                    data = (data << 1) | bit as u8;
                }
                payload.push(data);
            }

            let crc_valid = header.verify(&payload);
            FrameBytesInfo {
                header: Some(header),
                payload,
                crc_valid,
            }
        }
    }
}

/// Extract a pixel value that might be spread on many sibling pixel to reduce innacuracy
/// # Source
/// Code is a copy of https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/etcher.rs#L121
fn get_pixel(frame: &VideoFrame, x: i32, y: i32, size: u8) -> Vec<u8> {
    let mut r_list: Vec<u8> = Vec::new();
    let mut g_list: Vec<u8> = Vec::new();
    let mut b_list: Vec<u8> = Vec::new();

    let max_x = frame.image.cols() - 1;
    let max_y = frame.image.rows() - 1;

    // Sample the center of the cell rather than its top-left corner. The corner
    // sits on the seam between cells, exactly where a registration warp's
    // sub-pixel grid offset and JPEG edge bleed corrupt pixels. Insetting by
    // size/4 keeps the samples away from every seam, tolerating a grid offset of
    // roughly size/4 before reading a neighbor cell. For small cells (size <= 3)
    // the inset is 0, so the whole cell is sampled exactly as before.
    let inset = i32::from(size) / 4;
    let start = inset;
    let end = i32::from(size) - inset;

    for i in start..end {
        for j in start..end {
            // Clamp inside the image so a slightly oversized grid (e.g. after a
            // registration warp) never reads out of bounds.
            let sample_y = (y + i).clamp(0, max_y);
            let sample_x = (x + j).clamp(0, max_x);
            let bgr = frame
                .image
                .at_2d::<opencv::core::Vec3b>(sample_y, sample_x)
                .unwrap();
            r_list.push(bgr[2]);
            g_list.push(bgr[1]);
            b_list.push(bgr[0]);
        }
    }

    let r_sum: usize = r_list.iter().map(|&x| x as usize).sum();
    let r_average = r_sum / r_list.len();
    let g_sum: usize = g_list.iter().map(|&x| x as usize).sum();
    let g_average = g_sum / g_list.len();
    let b_sum: usize = b_list.iter().map(|&x| x as usize).sum();
    let b_average = b_sum / b_list.len();
    let rgb_average = vec![r_average as u8, g_average as u8, b_average as u8];

    rgb_average
}

/// Move all the data from gathered from the movie file into
/// a file that should be the original file.
///
/// # Example
/// if we injected a .zip file, we expect the file to be written to be also a .zip
///
pub fn data_to_files(extract_options: &ExtractOptions, whole_movie_data: Vec<u8>) {
    fs::write(
        extract_options.extracted_file_path.clone(),
        whole_movie_data,
    )
    .expect("Writing file fail");
}

#[cfg(test)]
mod extractionlogics_tests {
    use super::*;
    use crate::injectionlogics::{create_starting_frame, data_to_frames};
    use crate::options::InjectOptions;

    fn inject_opts(algo: AlgoFrame) -> InjectOptions {
        InjectOptions {
            file_path: String::new(),
            output_video_file: String::new(),
            fps: 30,
            width: 64,
            height: 64,
            size: 1,
            algo,
            show_progress: false,
        }
    }

    fn extract_opts(algo: AlgoFrame) -> ExtractOptions {
        ExtractOptions {
            video_file_path: String::new(),
            extracted_file_path: String::new(),
            fps: 30,
            width: 64,
            height: 64,
            size: 1,
            algo,
            show_progress: false,
        }
    }

    fn build_frames(data: &[u8], algo: AlgoFrame) -> Vec<VideoFrame> {
        let io = inject_opts(algo);
        let start = create_starting_frame(data.len() as u64, &io);
        let mut frames = vec![start];
        frames.extend(data_to_frames(&io, data.to_vec()));
        frames
    }

    #[test]
    fn test_round_trip_bw_in_memory() {
        let data: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
        let frames = build_frames(&data, AlgoFrame::BW);
        let result = frames_to_data(&extract_opts(AlgoFrame::BW), frames);
        assert_eq!(result, data);
    }

    #[test]
    fn test_round_trip_rgb_in_memory() {
        let data: Vec<u8> = (0..600u32).map(|i| (i % 253) as u8).collect();
        let frames = build_frames(&data, AlgoFrame::RGB);
        let result = frames_to_data(&extract_opts(AlgoFrame::RGB), frames);
        assert_eq!(result, data);
    }

    #[test]
    fn test_round_trip_quantized_in_memory() {
        // Every supported density must round-trip losslessly on a clean frame.
        for &levels in &[2u32, 4, 8, 16, 256] {
            let data: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
            let algo = AlgoFrame::Quantized(levels);
            let frames = build_frames(&data, algo);
            let result = frames_to_data(&extract_opts(algo), frames);
            assert_eq!(result, data, "levels={levels}");
        }
    }

    #[test]
    fn test_round_trip_brightness_in_memory() {
        for &levels in &[2u32, 4, 8, 16, 256] {
            let data: Vec<u8> = (0..600u32).map(|i| (i % 251) as u8).collect();
            let algo = AlgoFrame::Brightness(levels);
            let frames = build_frames(&data, algo);
            let result = frames_to_data(&extract_opts(algo), frames);
            assert_eq!(result, data, "brightness levels={levels}");
        }
    }

    #[test]
    fn test_quantized_crc_detects_corruption() {
        let data: Vec<u8> = (0..120u32).map(|i| (i % 200) as u8 + 1).collect();
        let levels = 4u32;
        let algo = AlgoFrame::Quantized(levels);
        let io = inject_opts(algo);
        let mut data_frames = data_to_frames(&io, data.clone());
        assert!(!data_frames.is_empty());
        let opts = extract_opts(algo);

        let info = frame_to_data_method_quantized(&data_frames[0], &opts, levels);
        assert!(info.crc_valid);
        assert_eq!(&info.payload[..data.len()], &data[..]);

        // Push the first content cell to the opposite end of the level range.
        let (x, y) = content_cell_xy(HEADER_BITS, io.width, io.size);
        let original = data_frames[0].read_coordinate_color(x, y);
        if original.r > 127 {
            data_frames[0].write(0, 0, 0, x, y, io.size);
        } else {
            data_frames[0].write(255, 255, 255, x, y, io.size);
        }
        let corrupted = frame_to_data_method_quantized(&data_frames[0], &opts, levels);
        assert!(!corrupted.crc_valid);
    }

    #[test]
    fn test_frame_crc_valid_then_corrupt_bw() {
        let data: Vec<u8> = (0..100u32).map(|i| (i % 200) as u8 + 1).collect();
        let io = inject_opts(AlgoFrame::BW);
        let mut data_frames = data_to_frames(&io, data.clone());
        assert!(!data_frames.is_empty());
        let opts = extract_opts(AlgoFrame::BW);

        // The clean data frame has a valid CRC and the first bytes match the input.
        let info = frame_to_data_method_bw(&data_frames[0], &opts);
        assert!(info.crc_valid);
        assert_eq!(&info.payload[..data.len()], &data[..]);

        // Flip the first payload cell -> CRC must now fail.
        let (x, y) = content_cell_xy(HEADER_BITS, io.width, io.size);
        let original = data_frames[0].read_coordinate_color(x, y);
        if original.r > 127 {
            data_frames[0].write(0, 0, 0, x, y, io.size);
        } else {
            data_frames[0].write(255, 255, 255, x, y, io.size);
        }
        let corrupted = frame_to_data_method_bw(&data_frames[0], &opts);
        assert!(!corrupted.crc_valid);
    }

    #[test]
    fn test_frames_to_data_ignores_duplicates_and_order() {
        let data: Vec<u8> = (0..600u32).map(|i| (i % 249) as u8).collect();
        let io = inject_opts(AlgoFrame::BW);
        let start = create_starting_frame(data.len() as u64, &io);
        let data_frames = data_to_frames(&io, data.clone());
        assert!(data_frames.len() >= 2);

        // Reorder and duplicate frames; CRC + page numbers must still reassemble.
        let mut frames = Vec::new();
        frames.push(data_frames[data_frames.len() - 1].clone());
        frames.push(start);
        for f in &data_frames {
            frames.push(f.clone());
        }
        frames.push(data_frames[0].clone());

        let result = frames_to_data(&extract_opts(AlgoFrame::BW), frames);
        assert_eq!(result, data);
    }

    #[test]
    #[should_panic(expected = "Instruction not found")]
    fn test_frames_to_data_panics_without_start() {
        let data: Vec<u8> = (0..50u32).map(|i| i as u8).collect();
        let io = inject_opts(AlgoFrame::BW);
        let frames = data_to_frames(&io, data);
        let _ = frames_to_data(&extract_opts(AlgoFrame::BW), frames);
    }
}
