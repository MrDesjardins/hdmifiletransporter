use hdmifiletransporter::{frames_to_video, options::AlgoFrame, InjectOptions, VideoFrame};

fn main() {
    let size = 20 as u8;
    let diagonal_length = 10 * size as u16;
    let width = 1920 as u16;
    let height = 1080 as u16;
    let mut frames: Vec<VideoFrame> = Vec::new();
    let black_color: (u8, u8, u8) = (0, 0, 0); // Black
    let white_color: (u8, u8, u8) = (255, 255, 255); // White

    let total_pixel_per_frame = (width as u64 * height as u64) / size as u64;
    let total_pixel_per_frame2 = total_pixel_per_frame as usize;
    println!(
        "Creating a diagonal pattern frame of size {}x{} with a size of {}",
        width, height, size
    );
    let mut x: u16;
    let mut y: u16 = 0;

    // Find the color index to use
    let mut colors: Vec<(u8, u8, u8)> = Vec::with_capacity(total_pixel_per_frame2);
    while y < height {
        x = 0;
        while x < width {
            let mut col = white_color;

            if x < diagonal_length && y < diagonal_length
                || width - x <= diagonal_length && y <= diagonal_length
                || x <= diagonal_length && height - y <= diagonal_length
                || width - x <= diagonal_length && height - y <= diagonal_length
            {
                if (x as i16 - y as i16 == 0) // Top-Left
                    || (width as i16- size as i16- x as i16== y as i16) // Top-Right
                    || (x as i16== height as i16- size as i16- y as i16) // Bottom-Left
                    || (width  as i16- size as i16 - x as i16 == height as i16 - size as i16 - y as i16)
                // Bottom-Right
                {
                    col = black_color;
                }
            }

            colors.push(col);
            x += size as u16;
        }
        y += size as u16;
    }

    // Create frames using the colors computed
    for _frame_counter in 0..30 {
        y = 0;
        let mut frame = VideoFrame::new(width, height);
        while y < height {
            x = 0;
            while x < width {
                let vec_color_index = (y as usize / size as usize * width as usize / size as usize)
                    + (x as usize / size as usize);
                let color = colors[vec_color_index as usize];
                frame.write(color.0, color.1, color.2, x, y, size);
                x += size as u16;
            }
            y += size as u16;
        }
        frames.push(frame);
    }
    frames_to_video(
        InjectOptions {
            algo: AlgoFrame::RGB,
            file_path: "not_used".to_string(),
            fps: 30,
            height: height,
            width: width,
            output_video_file: "outputs/diagonal_video.mp4".to_string(),
            show_progress: true,
            size: 1,
        },
        frames,
    );
    println!("Done");
}
