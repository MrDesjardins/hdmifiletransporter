//! Resilience + speed benchmark for the HDMI file-transport pipeline.
//!
//! The transport has several knobs that trade robustness against speed:
//!   * `algo`  - BW (1 bit / cell, robust) vs RGB (3 bytes / cell, dense but fragile)
//!   * `size`  - the pixel width/height of a cell (bigger = survives blur/offset, but
//!               fewer cells per frame, so more frames and a longer video)
//!   * resolution - more pixels = more cells per frame
//!
//! Goal: move a file (e.g. a `.zip`) and reconstruct it byte-for-byte after it has
//! crossed a real HDMI display -> capture-card path. That path introduces a
//! positional offset/overscan, anisotropic rescaling, lossy JPEG-style
//! compression (most USB capture cards deliver MJPEG), a luminance/level remap
//! (limited vs full RGB range, brightness/contrast drift), per-frame sub-pixel
//! jitter, and sensor noise. We model all of those in `simulate_capture`.
//!
//! Two studies are produced:
//!
//! 1. Resilience + speed matrix: every (resolution, algo, size) config is run
//!    through encode -> perturb -> `register_frame` -> `frames_to_data` at several
//!    capture severities, recording bytes/frame, playback throughput, CPU time,
//!    and PASS/FAIL (exact reconstruction).
//!
//! 2. Color-variance study (RGB): BW is robust because it uses only two colours
//!    255 apart. RGB packs far more data (3x256 values) but neighbouring values
//!    (e.g. 254 vs 255) are easily confused after compression. This study sweeps
//!    the number of levels per channel (i.e. the spacing/"variance" between
//!    symbols) and measures the per-channel decode accuracy through the same
//!    value-domain distortion, to find how much spacing is required to be
//!    reliable - and how many bits/cell that buys versus BW. It writes an SVG
//!    accuracy-vs-levels chart.
//!
//! Results: `benchmark_results.md` (report + recommendations), plus
//! `benchmark_results.csv`, `color_variance.csv`, and `color_variance.svg`.
//!
//! Run with: `cargo run --release --bin benchmark`

use std::fs;
use std::panic::{self, AssertUnwindSafe};
use std::time::Instant;

use hdmifiletransporter::options::AlgoFrame;
use hdmifiletransporter::{
    content_cell_xy, create_starting_frame, data_to_frames, frame_capacity, frames_to_data,
    register_frame, ExtractOptions, InjectOptions, VideoFrame, HEADER_BITS,
};
use opencv::core::{copy_make_border, Mat, Scalar, Size, Vec3b, Vector, BORDER_CONSTANT};
use opencv::imgcodecs::{imdecode, imencode, IMREAD_COLOR, IMWRITE_JPEG_QUALITY};
use opencv::imgproc::{resize, INTER_LINEAR};
use opencv::prelude::*;

// --- Benchmark configuration (tune these to trade coverage for runtime) -------

/// Size of the synthetic payload, emulating a small `.zip`. Kept modest so the
/// matrix runs in a reasonable time; bump it for a heavier test.
const PAYLOAD_BYTES: usize = 128 * 1024;

const FPS: u32 = 30;

const SIZES: [u8; 6] = [2, 3, 4, 6, 8, 10];
const RESOLUTIONS: [(u16, u16); 2] = [(1280, 720), (1920, 1080)];
const ALGOS: [AlgoFrame; 2] = [AlgoFrame::RGB, AlgoFrame::BW];

/// The capture severity a recommended config must still survive.
const TARGET_PROFILE: &str = "Harsh";

// Color-variance study geometry (kept independent of the main matrix so it runs
// quickly). Cells are laid out across the whole frame; no calibration/markers
// are needed because this study isolates value-domain (colour) robustness.
const VAR_WIDTH: u16 = 1280;
const VAR_HEIGHT: u16 = 720;
const VAR_CELL: u8 = 8;
/// Levels per channel to sweep. 2 levels == BW-like spacing (gap 255); 256 ==
/// raw 8-bit (gap 1). Spacing = 255 / (levels - 1).
const VAR_LEVELS: [u32; 10] = [2, 3, 4, 6, 8, 16, 32, 64, 128, 256];
/// Accuracy (fraction) a level count must reach to be considered "reliable".
const VAR_RELIABLE: f64 = 0.9999;

// --- Tiny reproducible PRNG (xorshift64*) -------------------------------------

struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the all-zero state.
        Rng(seed | 1)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }
    /// Uniform in [0, 1).
    fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// Uniform integer in [lo, hi] inclusive.
    fn range_i32(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            lo
        } else {
            lo + (self.next_u64() % ((hi - lo) as u64 + 1)) as i32
        }
    }
    /// Gaussian sample via Box-Muller.
    fn gaussian(&mut self, stddev: f64) -> f64 {
        let u1 = self.next_f64().max(1e-12);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos() * stddev
    }
}

// --- Capture-simulation profiles ----------------------------------------------

#[derive(Clone, Copy)]
struct Profile {
    name: &'static str,
    /// `None` => identity (feed the canonical frame straight to registration).
    perturb: Option<Perturb>,
}

#[derive(Clone, Copy)]
struct Perturb {
    pad_top: i32,
    pad_bottom: i32,
    pad_left: i32,
    pad_right: i32,
    scale_x: f64,
    scale_y: f64,
    /// Random +/- pixels added to each pad per frame (sub-pixel grid jitter).
    jitter: i32,
    /// Photometric remap out = in * contrast + brightness, applied before JPEG.
    /// Limited-range capture (16..235) is contrast ~0.86, brightness 16.
    contrast: f64,
    brightness: f64,
    /// Std-dev of additive Gaussian sensor noise (0..255 scale).
    noise_stddev: f64,
    jpeg_quality: i32,
}

const PROFILES: [Profile; 4] = [
    Profile {
        name: "Clean",
        perturb: None,
    },
    Profile {
        name: "Mild",
        perturb: Some(Perturb {
            pad_top: 9,
            pad_bottom: 5,
            pad_left: 13,
            pad_right: 7,
            scale_x: 1.2,
            scale_y: 0.85,
            jitter: 1,
            contrast: 1.0,
            brightness: 0.0,
            noise_stddev: 1.5,
            jpeg_quality: 90,
        }),
    },
    Profile {
        name: "Harsh",
        perturb: Some(Perturb {
            pad_top: 20,
            pad_bottom: 14,
            pad_left: 28,
            pad_right: 18,
            scale_x: 1.3,
            scale_y: 0.80,
            jitter: 2,
            contrast: 0.92,
            brightness: 6.0,
            noise_stddev: 3.0,
            jpeg_quality: 70,
        }),
    },
    Profile {
        name: "Brutal",
        perturb: Some(Perturb {
            pad_top: 30,
            pad_bottom: 22,
            pad_left: 40,
            pad_right: 30,
            scale_x: 1.4,
            scale_y: 0.70,
            jitter: 3,
            contrast: 0.86, // limited-range squeeze toward the middle
            brightness: 16.0,
            noise_stddev: 5.0,
            jpeg_quality: 50,
        }),
    },
];

/// Add zero-mean Gaussian noise to every pixel/channel (in place).
fn add_noise(mat: &mut Mat, stddev: f64, rng: &mut Rng) {
    if stddev <= 0.0 {
        return;
    }
    let rows = mat.rows();
    let cols = mat.cols();
    for y in 0..rows {
        for x in 0..cols {
            let px = mat.at_2d_mut::<Vec3b>(y, x).expect("noise pixel");
            for c in 0..3 {
                let v = px[c] as f64 + rng.gaussian(stddev);
                px[c] = v.round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

/// Mimic a capture pipeline: jittered pad (offset/overscan), anisotropic resize
/// (scaling), photometric remap (limited range / brightness / contrast),
/// additive sensor noise, then a JPEG round-trip (DCT + chroma loss, as MJPEG
/// capture cards do).
fn simulate_capture(canonical: &Mat, p: &Perturb, rng: &mut Rng) -> Mat {
    let jt = p.jitter;
    let pad_top = (p.pad_top + rng.range_i32(-jt, jt)).max(0);
    let pad_bottom = (p.pad_bottom + rng.range_i32(-jt, jt)).max(0);
    let pad_left = (p.pad_left + rng.range_i32(-jt, jt)).max(0);
    let pad_right = (p.pad_right + rng.range_i32(-jt, jt)).max(0);

    let mut padded = Mat::default();
    copy_make_border(
        canonical,
        &mut padded,
        pad_top,
        pad_bottom,
        pad_left,
        pad_right,
        BORDER_CONSTANT,
        Scalar::new(255.0, 255.0, 255.0, 0.0), // white quiet surroundings
    )
    .expect("pad");

    let new_size = Size::new(
        ((padded.cols() as f64 * p.scale_x) as i32).max(1),
        ((padded.rows() as f64 * p.scale_y) as i32).max(1),
    );
    let mut resized = Mat::default();
    resize(&padded, &mut resized, new_size, 0.0, 0.0, INTER_LINEAR).expect("resize");

    // Photometric remap (out = in * contrast + brightness). -1 keeps the type.
    let mut adjusted = Mat::default();
    resized
        .convert_to(&mut adjusted, -1, p.contrast, p.brightness)
        .expect("photometric convert");

    add_noise(&mut adjusted, p.noise_stddev, rng);

    let mut params: Vector<i32> = Vector::new();
    params.push(IMWRITE_JPEG_QUALITY);
    params.push(p.jpeg_quality);
    let mut buf: Vector<u8> = Vector::new();
    imencode(".jpg", &adjusted, &mut buf, &params).expect("jpeg encode");
    imdecode(&buf, IMREAD_COLOR).expect("jpeg decode")
}

// --- Synthetic payload --------------------------------------------------------

/// Deterministic incompressible-looking bytes, a fair stand-in for an archive.
fn synthetic_payload(len: usize) -> Vec<u8> {
    let mut rng = Rng::new(0x9E3779B97F4A7C15);
    let mut out = Vec::with_capacity(len);
    while out.len() < len {
        out.extend_from_slice(&rng.next_u64().to_le_bytes());
    }
    out.truncate(len);
    out
}

// --- Result records (resilience + speed matrix) -------------------------------

struct ProfileResult {
    name: &'static str,
    registered_rate: f64,
    decode_ms: f64,
    pass: bool,
}

struct ConfigResult {
    width: u16,
    height: u16,
    size: u8,
    algo: AlgoFrame,
    bytes_per_frame: usize,
    frame_count: usize,
    throughput_kbps: f64,
    encode_ms: f64,
    profiles: Vec<ProfileResult>,
}

impl ConfigResult {
    fn algo_str(&self) -> &'static str {
        algo_str(self.algo)
    }

    fn passed(&self, profile_name: &str) -> bool {
        self.profiles
            .iter()
            .any(|p| p.name == profile_name && p.pass)
    }

    /// Harshest profile (in declaration order) that still reconstructs exactly.
    fn max_survived(&self) -> &'static str {
        let mut best = "none";
        for prof in PROFILES.iter() {
            if self.passed(prof.name) {
                best = prof.name;
            }
        }
        best
    }
}

fn algo_str(algo: AlgoFrame) -> &'static str {
    match algo {
        AlgoFrame::RGB => "rgb",
        AlgoFrame::BW => "bw",
        AlgoFrame::Quantized(_) => "quantized",
        AlgoFrame::Brightness(_) => "brightness",
    }
}

// --- Option builders ----------------------------------------------------------

fn inject_options(width: u16, height: u16, size: u8, algo: AlgoFrame) -> InjectOptions {
    InjectOptions {
        file_path: String::new(),
        output_video_file: String::new(),
        fps: FPS as u8,
        width,
        height,
        size,
        algo,
        show_progress: false,
    }
}

fn extract_options(width: u16, height: u16, size: u8, algo: AlgoFrame) -> ExtractOptions {
    ExtractOptions {
        video_file_path: String::new(),
        extracted_file_path: String::new(),
        fps: FPS as u8,
        width,
        height,
        size,
        algo,
        show_progress: false,
    }
}

// --- One config (resilience + speed) ------------------------------------------

fn bytes_per_frame(width: u16, height: u16, size: u8, algo: AlgoFrame) -> usize {
    let capacity = frame_capacity(width, height, size);
    match algo {
        AlgoFrame::RGB => capacity * 3,
        AlgoFrame::BW => capacity / 8,
        AlgoFrame::Quantized(levels) => capacity * 3 * (levels.trailing_zeros() as usize) / 8,
        AlgoFrame::Brightness(levels) => capacity * (levels.trailing_zeros() as usize) / 8,
    }
}

fn run_config(
    width: u16,
    height: u16,
    size: u8,
    algo: AlgoFrame,
    payload: &[u8],
) -> Option<ConfigResult> {
    let bpf = bytes_per_frame(width, height, size, algo);
    if bpf == 0 {
        return None; // frame too small for header + payload in this algo
    }

    let io = inject_options(width, height, size, algo);

    let t_enc = Instant::now();
    let mut frames: Vec<VideoFrame> = vec![create_starting_frame(payload.len() as u64, &io)];
    frames.extend(data_to_frames(&io, payload.to_vec()));
    let encode_ms = t_enc.elapsed().as_secs_f64() * 1000.0;

    let frame_count = frames.len();
    let video_seconds = frame_count as f64 / FPS as f64;
    let throughput_kbps = (payload.len() as f64 / video_seconds) / 1024.0;

    let eo = extract_options(width, height, size, algo);

    // Deterministic per-config seed so runs are reproducible.
    let mut rng = Rng::new(
        0xC0FFEE
            ^ ((width as u64) << 20)
            ^ ((height as u64) << 8)
            ^ (size as u64)
            ^ ((matches!(algo, AlgoFrame::BW) as u64) << 40),
    );

    let mut profiles = Vec::with_capacity(PROFILES.len());
    for profile in PROFILES.iter() {
        let t_dec = Instant::now();

        let mut registered: Vec<VideoFrame> = Vec::with_capacity(frame_count);
        for f in &frames {
            let maybe = match &profile.perturb {
                None => register_frame(&f.image, width, height, size),
                Some(p) => {
                    let perturbed = simulate_capture(&f.image, p, &mut rng);
                    register_frame(&perturbed, width, height, size)
                }
            };
            if let Some(vf) = maybe {
                registered.push(vf);
            }
        }
        let registered_rate = registered.len() as f64 / frame_count as f64;

        let decoded = panic::catch_unwind(AssertUnwindSafe(|| frames_to_data(&eo, registered)));
        let decode_ms = t_dec.elapsed().as_secs_f64() * 1000.0;
        let pass = matches!(&decoded, Ok(bytes) if bytes.as_slice() == payload);

        profiles.push(ProfileResult {
            name: profile.name,
            registered_rate,
            decode_ms,
            pass,
        });
    }

    Some(ConfigResult {
        width,
        height,
        size,
        algo,
        bytes_per_frame: bpf,
        frame_count,
        throughput_kbps,
        encode_ms,
        profiles,
    })
}

// --- Color-variance study -----------------------------------------------------

struct VarPoint {
    profile: &'static str,
    levels: u32,
    spacing: f64,
    bits_per_cell: f64,
    accuracy: f64,
}

/// Quantize a 0..255 value to the nearest of `levels` evenly-spaced symbols and
/// return the symbol index.
fn nearest_symbol(value: f64, spacing: f64, levels: u32) -> u32 {
    let idx = (value / spacing).round() as i64;
    idx.clamp(0, levels as i64 - 1) as u32
}

/// Average the centre of the cell at pixel (px, py) to mirror how the real
/// decoder samples (`get_pixel` centre-inset), returning (r, g, b).
fn sample_cell_center(frame: &VideoFrame, px: u16, py: u16, size: u8) -> (f64, f64, f64) {
    let inset = size / 4;
    let start = inset;
    let end = size - inset; // full cell for size <= 3
    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let mut n = 0.0;
    for j in start..end {
        for i in start..end {
            let c = frame.read_coordinate_color(px + u16::from(i), py + u16::from(j));
            r += c.r as f64;
            g += c.g as f64;
            b += c.b as f64;
            n += 1.0;
        }
    }
    (r / n, g / n, b / n)
}

/// For one severity profile and one `levels` value, fill a frame with random
/// per-channel symbols, push it through the value-domain capture distortion
/// (no geometric registration - this isolates colour confusability), read each
/// cell back, and return the fraction of channel-symbols decoded correctly.
fn measure_variance(p: &Perturb, levels: u32, rng: &mut Rng) -> f64 {
    let spacing = 255.0 / (levels as f64 - 1.0);
    let size = VAR_CELL;
    let cols = (VAR_WIDTH / u16::from(size)) as usize;
    let rows = (VAR_HEIGHT / u16::from(size)) as usize;

    let mut frame = VideoFrame::new(VAR_WIDTH, VAR_HEIGHT);
    // Record the symbol written to each cell/channel so we can score the readback.
    let mut expected: Vec<[u32; 3]> = Vec::with_capacity(cols * rows);
    for cy in 0..rows {
        for cx in 0..cols {
            let s = [
                rng.range_i32(0, levels as i32 - 1) as u32,
                rng.range_i32(0, levels as i32 - 1) as u32,
                rng.range_i32(0, levels as i32 - 1) as u32,
            ];
            let r = (s[0] as f64 * spacing).round() as u8;
            let g = (s[1] as f64 * spacing).round() as u8;
            let b = (s[2] as f64 * spacing).round() as u8;
            let px = (cx * size as usize) as u16;
            let py = (cy * size as usize) as u16;
            frame.write(r, g, b, px, py, size);
            expected.push(s);
        }
    }

    // Value-domain distortion only: no pad, no scaling, so the frame keeps its
    // dimensions and cells stay at their original coordinates.
    let value_only = Perturb {
        pad_top: 0,
        pad_bottom: 0,
        pad_left: 0,
        pad_right: 0,
        scale_x: 1.0,
        scale_y: 1.0,
        jitter: 0,
        contrast: p.contrast,
        brightness: p.brightness,
        noise_stddev: p.noise_stddev,
        jpeg_quality: p.jpeg_quality,
    };
    let distorted_mat = simulate_capture(&frame.image, &value_only, rng);
    let distorted = VideoFrame::from(distorted_mat, size).expect("same-size frame");

    let mut correct = 0u64;
    let mut total = 0u64;
    let mut idx = 0usize;
    for cy in 0..rows {
        for cx in 0..cols {
            let px = (cx * size as usize) as u16;
            let py = (cy * size as usize) as u16;
            let (r, g, b) = sample_cell_center(&distorted, px, py, size);
            let got = [
                nearest_symbol(r, spacing, levels),
                nearest_symbol(g, spacing, levels),
                nearest_symbol(b, spacing, levels),
            ];
            let exp = expected[idx];
            for c in 0..3 {
                if got[c] == exp[c] {
                    correct += 1;
                }
                total += 1;
            }
            idx += 1;
        }
    }

    correct as f64 / total as f64
}

fn run_color_variance() -> Vec<VarPoint> {
    let mut out = Vec::new();
    let mut rng = Rng::new(0x5EED_C0DE);
    for profile in PROFILES.iter() {
        let Some(p) = profile.perturb else {
            continue; // Clean is trivially perfect in the value domain
        };
        for &levels in VAR_LEVELS.iter() {
            let accuracy = measure_variance(&p, levels, &mut rng);
            let spacing = 255.0 / (levels as f64 - 1.0);
            let bits_per_cell = 3.0 * (levels as f64).log2();
            println!(
                "  color-variance {} levels={} spacing={:.1} accuracy={:.4}",
                profile.name, levels, spacing, accuracy
            );
            out.push(VarPoint {
                profile: profile.name,
                levels,
                spacing,
                bits_per_cell,
                accuracy,
            });
        }
    }
    out
}

// --- Reporting ----------------------------------------------------------------

fn profile_decode_ms(c: &ConfigResult, name: &str) -> f64 {
    c.profiles
        .iter()
        .find(|p| p.name == name)
        .map(|p| p.decode_ms)
        .unwrap_or(0.0)
}

fn cell(b: bool) -> &'static str {
    if b {
        "PASS"
    } else {
        "FAIL"
    }
}

fn write_markdown(results: &[ConfigResult], variance: &[VarPoint]) -> String {
    let mut out = String::new();
    out.push_str("# Benchmark results\n\n");
    out.push_str(&format!(
        "Payload: {} bytes of pseudo-random data (incompressible, like a .zip). fps: {}.\n\n",
        PAYLOAD_BYTES, FPS
    ));
    out.push_str(
        "Capture simulation models offset/overscan, anisotropic rescale, per-frame \
         jitter, a limited-range/brightness/contrast remap, sensor noise, and a \
         JPEG (MJPEG-style) round-trip. Throughput is payload bytes per second \
         while the video plays at `fps`. Decode ms is the `Mild` profile. \
         PASS = exact byte-for-byte recovery.\n\n",
    );

    for (w, h) in RESOLUTIONS.iter() {
        out.push_str(&format!("## {}x{}\n\n", w, h));
        out.push_str(
            "| algo | size | bytes/frame | frames | throughput KB/s | encode ms | decode ms | Clean | Mild | Harsh | Brutal | max survived |\n",
        );
        out.push_str(
            "|------|------|-------------|--------|-----------------|-----------|-----------|-------|------|-------|--------|--------------|\n",
        );
        for c in results.iter().filter(|c| c.width == *w && c.height == *h) {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {:.1} | {:.1} | {:.1} | {} | {} | {} | {} | {} |\n",
                c.algo_str(),
                c.size,
                c.bytes_per_frame,
                c.frame_count,
                c.throughput_kbps,
                c.encode_ms,
                profile_decode_ms(c, "Mild"),
                cell(c.passed("Clean")),
                cell(c.passed("Mild")),
                cell(c.passed("Harsh")),
                cell(c.passed("Brutal")),
                c.max_survived(),
            ));
        }
        out.push('\n');
    }

    // Resilience + speed recommendation.
    out.push_str("## Recommendation (resilient + fast)\n\n");
    let recommended = results
        .iter()
        .filter(|c| c.passed(TARGET_PROFILE))
        .max_by(|a, b| {
            a.throughput_kbps
                .partial_cmp(&b.throughput_kbps)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    match recommended {
        Some(c) => {
            out.push_str(&format!(
                "Survives `{}`, then highest throughput:\n\n- **algo `{}`, size `{}`, {}x{}** - {:.1} KB/s at {} fps, {} bytes/frame, {} frames, survives up to `{}`.\n\nCLI: `--algo {} --size {} --width {} --height {} --fps {}`\n\n",
                TARGET_PROFILE, c.algo_str(), c.size, c.width, c.height,
                c.throughput_kbps, FPS, c.bytes_per_frame, c.frame_count, c.max_survived(),
                c.algo_str(), c.size, c.width, c.height, FPS,
            ));
        }
        None => {
            out.push_str(&format!(
                "No configuration reconstructed the payload exactly at the `{}` profile. Try a larger `size`, a higher resolution, or BW mode.\n\n",
                TARGET_PROFILE
            ));
        }
    }

    // Color-variance section.
    out.push_str("## Color variance (RGB levels per channel)\n\n");
    out.push_str(
        "How far apart must colour values be to survive the channel? Each row is a \
         number of evenly-spaced levels per channel (2 = BW-like, gap 255; 256 = \
         raw 8-bit, gap 1). Accuracy is the fraction of channel-symbols decoded \
         correctly after value-domain distortion (no geometric loss). `bits/cell` \
         is the data density (BW packs 1 bit/cell for comparison).\n\n",
    );
    out.push_str("![Accuracy vs levels per channel](color_variance.svg)\n\n");
    out.push_str("| profile | levels | spacing | bits/cell | accuracy |\n");
    out.push_str("|---------|--------|---------|-----------|----------|\n");
    for v in variance {
        out.push_str(&format!(
            "| {} | {} | {:.1} | {:.2} | {:.4} |\n",
            v.profile, v.levels, v.spacing, v.bits_per_cell, v.accuracy
        ));
    }
    out.push('\n');

    // Variance recommendation at the target profile.
    out.push_str("### Color-variance recommendation\n\n");
    let best = variance
        .iter()
        .filter(|v| v.profile == TARGET_PROFILE && v.accuracy >= VAR_RELIABLE)
        .max_by_key(|v| v.levels);
    match best {
        Some(v) => out.push_str(&format!(
            "At the `{}` profile, the most levels per channel that stay >= {:.2}% accurate is **{} levels** (spacing {:.0}), i.e. **{:.1} bits/cell** - about {:.1}x the density of BW (1 bit/cell) while remaining reliable. Fewer/raw 8-bit levels (gap 1) are too close and get confused.\n",
            v.profile, VAR_RELIABLE * 100.0, v.levels, v.spacing, v.bits_per_cell, v.bits_per_cell / 1.0,
        )),
        None => out.push_str(&format!(
            "At the `{}` profile, no tested level count reached {:.2}% accuracy - only BW-like 2-level spacing is reliable here, so BW is the safe choice.\n",
            TARGET_PROFILE, VAR_RELIABLE * 100.0,
        )),
    }

    out
}

fn write_csv(results: &[ConfigResult]) -> String {
    let mut out = String::new();
    out.push_str(
        "width,height,algo,size,bytes_per_frame,frames,throughput_kbps,encode_ms,profile,registered_rate,decode_ms,pass\n",
    );
    for c in results {
        for p in &c.profiles {
            out.push_str(&format!(
                "{},{},{},{},{},{},{:.3},{:.3},{},{:.4},{:.3},{}\n",
                c.width,
                c.height,
                c.algo_str(),
                c.size,
                c.bytes_per_frame,
                c.frame_count,
                c.throughput_kbps,
                c.encode_ms,
                p.name,
                p.registered_rate,
                p.decode_ms,
                p.pass,
            ));
        }
    }
    out
}

fn write_variance_csv(variance: &[VarPoint]) -> String {
    let mut out = String::new();
    out.push_str("profile,levels,spacing,bits_per_cell,accuracy\n");
    for v in variance {
        out.push_str(&format!(
            "{},{},{:.4},{:.4},{:.6}\n",
            v.profile, v.levels, v.spacing, v.bits_per_cell, v.accuracy
        ));
    }
    out
}

/// Hand-rolled SVG line chart of accuracy (%) vs levels-per-channel, one line
/// per severity profile. No external crates required.
fn write_variance_svg(variance: &[VarPoint]) -> String {
    let width = 760.0;
    let height = 460.0;
    let left = 64.0;
    let right = 600.0;
    let top = 40.0;
    let bottom = 400.0;
    let plot_w = right - left;
    let plot_h = bottom - top;

    let levels = &VAR_LEVELS;
    let n = levels.len();
    let x_at = |i: usize| -> f64 {
        if n <= 1 {
            left
        } else {
            left + (i as f64 / (n as f64 - 1.0)) * plot_w
        }
    };
    let y_at = |acc_pct: f64| -> f64 { bottom - (acc_pct / 100.0) * plot_h };

    let colors = ["#1f77b4", "#ff7f0e", "#2ca02c", "#d62728", "#9467bd"];

    let mut svg = String::new();
    svg.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{}\" height=\"{}\" viewBox=\"0 0 {} {}\" font-family=\"sans-serif\" font-size=\"12\">\n",
        width, height, width, height
    ));
    svg.push_str("<rect width=\"100%\" height=\"100%\" fill=\"white\"/>\n");
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"24\" font-size=\"16\" font-weight=\"bold\">RGB decode accuracy vs levels per channel</text>\n",
        left
    ));

    // Y gridlines + labels (0..100%).
    for t in [0, 25, 50, 75, 100] {
        let y = y_at(t as f64);
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#ddd\"/>\n",
            left, y, right, y
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"end\">{}%</text>\n",
            left - 8.0,
            y + 4.0,
            t
        ));
    }
    // Axes.
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#333\"/>\n",
        left, top, left, bottom
    ));
    svg.push_str(&format!(
        "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"#333\"/>\n",
        left, bottom, right, bottom
    ));
    // X ticks (level values).
    for (i, lv) in levels.iter().enumerate() {
        let x = x_at(i);
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">{}</text>\n",
            x,
            bottom + 18.0,
            lv
        ));
    }
    svg.push_str(&format!(
        "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\">levels per channel</text>\n",
        (left + right) / 2.0,
        bottom + 40.0
    ));

    // One polyline per profile.
    let profile_names: Vec<&str> = PROFILES
        .iter()
        .filter(|p| p.perturb.is_some())
        .map(|p| p.name)
        .collect();
    for (pi, pname) in profile_names.iter().enumerate() {
        let color = colors[pi % colors.len()];
        let mut points = String::new();
        for (i, lv) in levels.iter().enumerate() {
            if let Some(v) = variance
                .iter()
                .find(|v| v.profile == *pname && v.levels == *lv)
            {
                let x = x_at(i);
                let y = y_at(v.accuracy * 100.0);
                points.push_str(&format!("{:.1},{:.1} ", x, y));
                svg.push_str(&format!(
                    "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"2.5\" fill=\"{}\"/>\n",
                    x, y, color
                ));
            }
        }
        svg.push_str(&format!(
            "<polyline fill=\"none\" stroke=\"{}\" stroke-width=\"2\" points=\"{}\"/>\n",
            color,
            points.trim()
        ));
        // Legend entry.
        let ly = top + 8.0 + pi as f64 * 18.0;
        svg.push_str(&format!(
            "<line x1=\"{:.1}\" y1=\"{:.1}\" x2=\"{:.1}\" y2=\"{:.1}\" stroke=\"{}\" stroke-width=\"2\"/>\n",
            right + 16.0,
            ly,
            right + 40.0,
            ly,
            color
        ));
        svg.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\">{}</text>\n",
            right + 46.0,
            ly + 4.0,
            pname
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

// --- Large-file planner (N-levels-per-channel) --------------------------------
//
// The colour-variance study above measures only value-domain confusability. The
// planner closes the loop: it encodes payload with a real N-levels-per-channel
// codec, pushes each frame through the *full* capture simulation + geometric
// `register_frame`, decodes by sampling cell centres, and checks byte-exact
// recovery. From the measured per-frame survival it then models how long it
// takes to move a 1/10/50 MB file losslessly.
//
// Why "losslessly" is always achievable, and why time is the real metric: the
// source loops the video and every frame carries a CRC, so a receiver keeps any
// frame whose CRC checks and waits for the next loop to re-acquire the rest.
// Corruption is therefore impossible; a flakier (denser) config just needs more
// loops/passes. The planner picks the levels/bits/size that minimise transfer
// time while staying reliable.

/// Levels per channel to sweep. Restricted to powers of two so each channel
/// carries a whole number of bits (clean bit-packing). 2 levels = 1 bit/channel
/// (3 bits/cell, the densest maximally-separated option, strictly better than
/// BW's 1 bit/cell); 256 = raw 8-bit RGB.
const PLAN_LEVELS: [u32; 8] = [2, 4, 8, 16, 32, 64, 128, 256];
const PLAN_SIZES: [u8; 3] = [4, 6, 8];
const PLAN_RES: (u16, u16) = (1920, 1080);
/// Frames sampled per (size, levels, profile) to estimate survival/error rate.
const PLAN_SAMPLES: usize = 24;
/// Profiles measured by the planner (must match names in `PROFILES`).
const PLAN_PROFILES: [&str; 2] = ["Harsh", "Brutal"];
/// File sizes to plan for (bytes).
const PLAN_FILE_SIZES: [(&str, u64); 3] =
    [("1 MB", 1 << 20), ("10 MB", 10 << 20), ("50 MB", 50 << 20)];
/// Frame rates to report transfer time for. Fidelity is fps-independent in this
/// model, so higher fps is always faster - capped only by what the real
/// display/capture path can carry without dropping or tearing frames.
const PLAN_FPS: [u32; 3] = [30, 60, 120];

/// Bits carried by one channel symbol for a power-of-two `levels`.
fn bits_per_channel(levels: u32) -> u32 {
    levels.trailing_zeros()
}

/// Look up a profile's perturbation by name.
fn perturb_by_name(name: &str) -> Option<Perturb> {
    PROFILES
        .iter()
        .find(|p| p.name == name)
        .and_then(|p| p.perturb)
}

/// Encode `bytes` into one frame using `levels` symbols. Layout is identical to
/// the real pipeline (calibration ring + reserved header cells) so registration
/// behaves the same; only the payload-cell colours differ. When `luma` is set,
/// each cell carries a single grey symbol (R = G = B); otherwise each of the
/// three channels carries its own symbol.
fn encode_level_frame(
    bytes: &[u8],
    levels: u32,
    size: u8,
    width: u16,
    height: u16,
    luma: bool,
) -> VideoFrame {
    let b = bits_per_channel(levels);
    let spacing = 255.0 / (levels as f64 - 1.0);
    let capacity = frame_capacity(width, height, size);

    let mut frame = VideoFrame::new(width, height);
    frame.write_calibration(size);
    // Deterministically blacken the reserved header cells (we score the payload
    // by ground truth, so no real header is needed, but the cells must not be
    // uninitialised memory).
    for i in 0..HEADER_BITS {
        let (x, y) = content_cell_xy(i, width, size);
        frame.write(0, 0, 0, x, y, size);
    }

    let total_bits = bytes.len() * 8;
    let mut bit = 0usize;
    let mut next_symbol = |b: u32| -> u32 {
        let mut sym = 0u32;
        for _ in 0..b {
            let v = if bit < total_bits {
                (bytes[bit / 8] >> (7 - (bit % 8))) & 1
            } else {
                0
            };
            sym = (sym << 1) | v as u32;
            bit += 1;
        }
        sym
    };

    for cell in 0..capacity {
        let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
        if luma {
            let v = (next_symbol(b) as f64 * spacing).round() as u8;
            frame.write(v, v, v, x, y, size);
        } else {
            let r = (next_symbol(b) as f64 * spacing).round() as u8;
            let g = (next_symbol(b) as f64 * spacing).round() as u8;
            let bl = (next_symbol(b) as f64 * spacing).round() as u8;
            frame.write(r, g, bl, x, y, size);
        }
    }
    frame
}

/// Decode `n_bytes` from a registered frame, quantising back to the nearest of
/// `levels` symbols (centre sampling, like the real decoder). When `luma` is
/// set, the three channels are averaged into one grey value per cell.
fn decode_level_frame(
    frame: &VideoFrame,
    levels: u32,
    size: u8,
    width: u16,
    height: u16,
    n_bytes: usize,
    luma: bool,
) -> Vec<u8> {
    let b = bits_per_channel(levels);
    let spacing = 255.0 / (levels as f64 - 1.0);
    let capacity = frame_capacity(width, height, size);
    let needed_bits = n_bytes * 8;

    let mut bits: Vec<u8> = Vec::with_capacity(needed_bits + 24);
    'cells: for cell in 0..capacity {
        let (x, y) = content_cell_xy(HEADER_BITS + cell, width, size);
        let (r, g, bl) = sample_cell_center(frame, x, y, size);
        let (samples, count) = if luma {
            ([(r + g + bl) / 3.0, 0.0, 0.0], 1usize)
        } else {
            ([r, g, bl], 3usize)
        };
        for &val in &samples[..count] {
            let sym = (val / spacing).round().clamp(0.0, (levels - 1) as f64) as u32;
            for k in (0..b).rev() {
                bits.push(((sym >> k) & 1) as u8);
                if bits.len() >= needed_bits {
                    break 'cells;
                }
            }
        }
    }

    let mut out = Vec::with_capacity(n_bytes);
    for byte_i in 0..n_bytes {
        let mut byte = 0u8;
        for k in 0..8 {
            byte = (byte << 1) | bits.get(byte_i * 8 + k).copied().unwrap_or(0);
        }
        out.push(byte);
    }
    out
}

/// Expected number of full video loops (passes) for a receiver to acquire all
/// `n` data frames, when each frame independently survives a pass with
/// probability `p`. `E[passes] = sum_{k>=0} (1 - (1 - (1-p)^k)^n)`.
fn expected_passes(p: f64, n: u64) -> f64 {
    if p >= 1.0 {
        return 1.0;
    }
    if p <= 0.0 {
        return f64::INFINITY;
    }
    let q = 1.0 - p;
    let n = n as f64;
    let mut sum = 0.0;
    let mut k = 0i32;
    loop {
        let miss = q.powi(k); // P(a given frame still missing after k passes)
        let term = 1.0 - (1.0 - miss).powf(n);
        sum += term;
        k += 1;
        if (term < 1e-9 && k > 1) || k > 1_000_000 {
            break;
        }
    }
    sum
}

struct PlanPoint {
    mode: &'static str,
    luma: bool,
    size: u8,
    levels: u32,
    bits_per_cell: f64,
    spacing: f64,
    bytes_per_frame: usize,
    profile: &'static str,
    frame_survival: f64,
    byte_error_rate: f64,
    encode_ms: f64,
    decode_ms: f64,
}

/// Payload-encoding modes the planner compares: colour (one symbol per channel,
/// 3x bits/cell, but rides on the subsampled chroma) vs brightness/luma (one
/// grey symbol per cell, full-resolution luminance, far more robust).
const PLAN_MODES: [(&str, bool); 2] = [("color", false), ("brightness", true)];

fn run_large_file_planner() -> Vec<PlanPoint> {
    let (width, height) = PLAN_RES;
    let mut rng = Rng::new(0x1234_5678_9ABC_DEF0);
    let mut out = Vec::new();

    for &size in PLAN_SIZES.iter() {
        let capacity = frame_capacity(width, height, size);
        if capacity == 0 {
            continue;
        }
        for &(mode_name, luma) in PLAN_MODES.iter() {
            for &levels in PLAN_LEVELS.iter() {
                let b = bits_per_channel(levels);
                let symbols_per_cell = if luma { 1 } else { 3 };
                let bytes_per_frame = capacity * symbols_per_cell * b as usize / 8;
                if bytes_per_frame == 0 {
                    continue;
                }
                let spacing = 255.0 / (levels as f64 - 1.0);
                let bits_per_cell = symbols_per_cell as f64 * b as f64;

                for &pname in PLAN_PROFILES.iter() {
                    let perturb = match perturb_by_name(pname) {
                        Some(p) => p,
                        None => continue,
                    };

                    let mut frame_ok = 0usize;
                    let mut wrong_bytes = 0u64;
                    let mut total_bytes = 0u64;
                    let mut encode_total = 0.0;
                    let mut decode_total = 0.0;

                    for _ in 0..PLAN_SAMPLES {
                        let payload: Vec<u8> =
                            (0..bytes_per_frame).map(|_| rng.next_u64() as u8).collect();

                        let t0 = Instant::now();
                        let frame = encode_level_frame(&payload, levels, size, width, height, luma);
                        encode_total += t0.elapsed().as_secs_f64() * 1000.0;

                        let captured = simulate_capture(&frame.image, &perturb, &mut rng);

                        let t1 = Instant::now();
                        let decoded = register_frame(&captured, width, height, size).map(|reg| {
                            decode_level_frame(
                                &reg,
                                levels,
                                size,
                                width,
                                height,
                                bytes_per_frame,
                                luma,
                            )
                        });
                        decode_total += t1.elapsed().as_secs_f64() * 1000.0;

                        total_bytes += bytes_per_frame as u64;
                        match decoded {
                            Some(bytes) => {
                                let mismatched = bytes
                                    .iter()
                                    .zip(payload.iter())
                                    .filter(|(a, b)| a != b)
                                    .count()
                                    as u64;
                                wrong_bytes += mismatched;
                                if mismatched == 0 {
                                    frame_ok += 1;
                                }
                            }
                            None => {
                                // Frame could not be registered -> whole frame lost.
                                wrong_bytes += bytes_per_frame as u64;
                            }
                        }
                    }

                    out.push(PlanPoint {
                        mode: mode_name,
                        luma,
                        size,
                        levels,
                        bits_per_cell,
                        spacing,
                        bytes_per_frame,
                        profile: pname,
                        frame_survival: frame_ok as f64 / PLAN_SAMPLES as f64,
                        byte_error_rate: wrong_bytes as f64 / total_bytes as f64,
                        encode_ms: encode_total / PLAN_SAMPLES as f64,
                        decode_ms: decode_total / PLAN_SAMPLES as f64,
                    });
                    println!(
                        "  planner mode={} size={} levels={} bits/cell={:.0} {} survival={:.2} ber={:.2e}",
                        mode_name,
                        size,
                        levels,
                        bits_per_cell,
                        pname,
                        out.last().unwrap().frame_survival,
                        out.last().unwrap().byte_error_rate,
                    );
                }
            }
        }
    }
    out
}

/// Human-readable seconds (s / min / h).
fn fmt_duration(secs: f64) -> String {
    if !secs.is_finite() {
        return "inf".to_string();
    }
    if secs < 90.0 {
        format!("{:.1}s", secs)
    } else if secs < 5400.0 {
        format!("{:.1}min", secs / 60.0)
    } else {
        format!("{:.1}h", secs / 3600.0)
    }
}

fn write_planner_markdown(points: &[PlanPoint]) -> String {
    let (width, height) = PLAN_RES;
    let mut out = String::new();
    out.push_str("# Large-file transfer planner\n\n");
    out.push_str(&format!(
        "Resolution {}x{}. Each row is a real encode pushed through the full capture simulation (offset, anisotropic rescale, jitter, level remap, sensor noise, MJPEG) and geometric registration, then decoded by centre sampling and checked byte-for-byte. `mode` = `color` (one symbol per R/G/B channel, `3*log2(levels)` bits/cell, carried on the **subsampled chroma**) or `brightness` (one grey symbol per cell, `log2(levels)` bits/cell, carried on **full-resolution luma**). `spacing` = 255/(levels-1).\n\n",
        width, height
    ));
    out.push_str(
        "`survival` = fraction of sampled frames recovered byte-exact in a single pass. \
         `byte err` = fraction of payload bytes wrong. Because the source loops the video and \
         every frame is CRC-checked, the file is **always** reconstructed without corruption; a \
         lower survival just means more passes (loops), hence more time.\n\n",
    );

    for &pname in PLAN_PROFILES.iter() {
        out.push_str(&format!("## Reliability at `{}`\n\n", pname));
        out.push_str("| mode | size | levels | spacing | bits/cell | bytes/frame | survival | byte err | enc ms/f | dec ms/f |\n");
        out.push_str("|------|------|--------|---------|-----------|-------------|----------|----------|----------|----------|\n");
        for p in points.iter().filter(|p| p.profile == pname) {
            out.push_str(&format!(
                "| {} | {} | {} | {:.1} | {:.0} | {} | {:.2} | {:.2e} | {:.1} | {:.1} |\n",
                p.mode,
                p.size,
                p.levels,
                p.spacing,
                p.bits_per_cell,
                p.bytes_per_frame,
                p.frame_survival,
                p.byte_error_rate,
                p.encode_ms,
                p.decode_ms,
            ));
        }
        out.push('\n');
    }

    // Transfer-time model, driven by the target ("Harsh") profile.
    let target = TARGET_PROFILE;
    out.push_str(&format!(
        "## Transfer time for a lossless file (target profile `{}`)\n\n",
        target
    ));
    out.push_str(
        "Time = expected video loops (passes) x frames / fps. `passes` accounts for re-acquiring \
         any frame that did not survive a loop (CRC + retransmit). Encode/decode CPU is a one-off \
         (encode) / per-pass (decode) cost shown separately from the on-wire time.\n\n",
    );

    // Pick the recommended config: highest density (bytes/frame) that still
    // survives the target profile in a single pass (survival == 1.0), so the
    // file moves in one loop.
    let reliable: Vec<&PlanPoint> = points
        .iter()
        .filter(|p| p.profile == target && p.frame_survival >= 1.0)
        .collect();
    let recommended = reliable
        .iter()
        .max_by(|a, b| a.bytes_per_frame.cmp(&b.bytes_per_frame))
        .copied();

    match recommended {
        Some(p) => {
            out.push_str(&format!(
                "**Recommended: `{}` mode, {} levels ({:.0} bits/cell, spacing {:.0}), size {}** - the densest config that recovers every frame in one pass at `{}` ({} bytes/frame).\n\n",
                p.mode, p.levels, p.bits_per_cell, p.spacing, p.size, target, p.bytes_per_frame
            ));
            out.push_str("| file | frames | passes | ");
            for fps in PLAN_FPS.iter() {
                out.push_str(&format!("on-wire @ {}fps | ", fps));
            }
            out.push_str("enc CPU | dec CPU/pass |\n");
            out.push_str("|------|--------|--------|");
            for _ in PLAN_FPS.iter() {
                out.push_str("----------------|");
            }
            out.push_str("--------|--------------|\n");
            for (label, bytes) in PLAN_FILE_SIZES.iter() {
                let n = (bytes + p.bytes_per_frame as u64 - 1) / p.bytes_per_frame as u64;
                let passes = expected_passes(p.frame_survival, n);
                out.push_str(&format!("| {} | {} | {:.1} | ", label, n, passes));
                for fps in PLAN_FPS.iter() {
                    let on_wire = passes * n as f64 / *fps as f64;
                    out.push_str(&format!("{} | ", fmt_duration(on_wire)));
                }
                let enc_cpu = p.encode_ms * n as f64 / 1000.0;
                let dec_cpu = p.decode_ms * n as f64 / 1000.0;
                out.push_str(&format!(
                    "{} | {} |\n",
                    fmt_duration(enc_cpu),
                    fmt_duration(dec_cpu)
                ));
            }
            out.push('\n');
            let cli_algo = if p.luma { "brightness" } else { "quantized" };
            out.push_str(&format!(
                "CLI: `--algo {} --levels {} --size {} --width {} --height {} --fps 60`.\n\n",
                cli_algo, p.levels, p.size, width, height
            ));
        }
        None => {
            out.push_str(&format!(
                "No swept config recovered every frame in one pass at `{}`. Use fewer levels (wider spacing) or a larger cell `size`.\n\n",
                target
            ));
        }
    }

    // Density-vs-time trade-off across ALL target-profile configs for 50 MB at
    // 60 fps, so the optimum (and the cost of over-packing) is visible.
    out.push_str("## Density vs. time trade-off (50 MB @ 60 fps, `Harsh`)\n\n");
    out.push_str(
        "| mode | size | levels | bits/cell | survival | frames | passes | total time |\n",
    );
    out.push_str(
        "|------|------|--------|-----------|----------|--------|--------|------------|\n",
    );
    let fifty: u64 = 50 << 20;
    for p in points.iter().filter(|p| p.profile == target) {
        let n = (fifty + p.bytes_per_frame as u64 - 1) / p.bytes_per_frame as u64;
        let passes = expected_passes(p.frame_survival, n);
        let total = passes * n as f64 / 60.0;
        out.push_str(&format!(
            "| {} | {} | {} | {:.0} | {:.2} | {} | {} | {} |\n",
            p.mode,
            p.size,
            p.levels,
            p.bits_per_cell,
            p.frame_survival,
            n,
            if passes.is_finite() {
                format!("{:.1}", passes)
            } else {
                "inf".to_string()
            },
            fmt_duration(total),
        ));
    }
    out.push('\n');

    out
}

fn write_planner_csv(points: &[PlanPoint]) -> String {
    let mut out = String::new();
    out.push_str(
        "width,height,mode,size,levels,spacing,bits_per_cell,bytes_per_frame,profile,frame_survival,byte_error_rate,encode_ms,decode_ms\n",
    );
    let (width, height) = PLAN_RES;
    for p in points {
        out.push_str(&format!(
            "{},{},{},{},{},{:.4},{:.2},{},{},{:.4},{:.6e},{:.4},{:.4}\n",
            width,
            height,
            p.mode,
            p.size,
            p.levels,
            p.spacing,
            p.bits_per_cell,
            p.bytes_per_frame,
            p.profile,
            p.frame_survival,
            p.byte_error_rate,
            p.encode_ms,
            p.decode_ms,
        ));
    }
    out
}

// --- Entry point --------------------------------------------------------------

fn run_matrix(payload: &[u8]) {
    println!("== Resilience + speed matrix ==");
    let mut results: Vec<ConfigResult> = Vec::new();
    for (width, height) in RESOLUTIONS.iter().copied() {
        for algo in ALGOS.iter().copied() {
            for size in SIZES.iter().copied() {
                print!(
                    "Benchmarking {}x{} algo={} size={} ... ",
                    width,
                    height,
                    algo_str(algo),
                    size
                );
                use std::io::Write;
                let _ = std::io::stdout().flush();

                match run_config(width, height, size, algo, payload) {
                    Some(result) => {
                        println!(
                            "{} frames, {:.1} KB/s, max survived: {}",
                            result.frame_count,
                            result.throughput_kbps,
                            result.max_survived()
                        );
                        results.push(result);
                    }
                    None => println!("skipped (frame too small for header + payload)"),
                }
            }
        }
    }

    println!("\n== Color-variance study (RGB levels per channel) ==");
    let variance = run_color_variance();

    let md = write_markdown(&results, &variance);
    fs::write("benchmark_results.md", &md).expect("write benchmark_results.md");
    fs::write("benchmark_results.csv", write_csv(&results)).expect("write benchmark_results.csv");
    fs::write("color_variance.csv", write_variance_csv(&variance))
        .expect("write color_variance.csv");
    fs::write("color_variance.svg", write_variance_svg(&variance))
        .expect("write color_variance.svg");
    println!("\nWrote benchmark_results.md, benchmark_results.csv, color_variance.csv, color_variance.svg");
}

fn run_planner() {
    println!("== Large-file transfer planner (N levels per channel) ==");
    let points = run_large_file_planner();
    let md = write_planner_markdown(&points);
    fs::write("planner_results.md", &md).expect("write planner_results.md");
    fs::write("planner_results.csv", write_planner_csv(&points))
        .expect("write planner_results.csv");
    println!("\nWrote planner_results.md, planner_results.csv");
    println!("\n{}", md);
}

fn main() {
    // Mode selection: `BENCH_MODE=matrix|planner|all` (default all). The matrix +
    // colour-variance study and the large-file planner are independent, so the
    // planner can be iterated on without re-running the multi-minute matrix.
    let mode = std::env::var("BENCH_MODE").unwrap_or_else(|_| "all".to_string());
    let run_matrix_study = mode == "all" || mode == "matrix";
    let run_planner_study = mode == "all" || mode == "planner";

    // Silence panic backtraces from the intentional `frames_to_data` failures
    // caught inside `run_config`; restore the default hook afterwards.
    let previous_hook = panic::take_hook();
    if std::env::var("BENCH_DEBUG_PANICS").is_ok() {
        // Debug aid: surface the real panic location/message instead of hiding it.
        panic::set_hook(Box::new(|info| {
            eprintln!("[panic] {}", info);
        }));
    } else {
        panic::set_hook(Box::new(|_| {}));
    }

    if run_matrix_study {
        let payload = match std::env::args().nth(1) {
            Some(path) if path != "matrix" && path != "planner" && path != "all" => {
                println!("Loading payload from {}", path);
                fs::read(&path).unwrap_or_else(|e| panic!("Unable to read {}: {}", path, e))
            }
            _ => {
                println!("Using {}-byte synthetic payload", PAYLOAD_BYTES);
                synthetic_payload(PAYLOAD_BYTES)
            }
        };
        run_matrix(&payload);
    }

    if run_planner_study {
        run_planner();
    }

    panic::set_hook(previous_hook);
}
