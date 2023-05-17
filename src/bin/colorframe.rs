use hdmifiletransporter::{frames_to_video, options::AlgoFrame, InjectOptions, VideoFrame};

fn main() {
    let size = 1 as u8;
    let width = 1920 as u16;
    let height = 1080 as u16;
    let mut frames: Vec<VideoFrame> = Vec::new();
    let colors: [(u8, u8, u8); 9] = [
        (255, 0, 0),     // Red
        (255, 225, 0),   // Yellow
        (75, 255, 0),    // Green
        (0, 255, 255),   // Cyan
        (0, 125, 255),   // Dark blue
        (0, 125, 255),   // Dark blue
        (110, 0, 255),   // Purple
        (255, 0, 190),   // Pink
        (255, 255, 255), // White
    ];

    let color_size = colors.len() as u16;
    let total_pixel_per_frame = (width as u64 * height as u64) / size as u64;
    let total_pixel_per_frame2 = total_pixel_per_frame as usize;
    println!(
        "Creating a colorful frame of size {}x{} with a size of {} and using {} colors",
        width, height, size, color_size
    );
    let mut x: u16;
    let mut y: u16 = 0;

    // Find the color index to use
    let mut colors_index = Vec::with_capacity(total_pixel_per_frame2);
    while y < height {
        x = 0;
        while x < width {
            let mut color_index1 = (color_size - 1) as usize;

            if x < color_size * size as u16 {
                color_index1 = x as usize / size as usize;
            } else if width - x < color_size * size as u16 {
                color_index1 = (width as usize - x as usize) / size as usize - 1;
            }
            let mut color_index2 = (color_size - 1) as usize;

            if y < color_size * size as u16 {
                color_index2 = y as usize / size as usize;
            } else if height - y < color_size * size as u16 {
                color_index2 = (height as usize - y as usize) / size as usize - 1;
            }
            let color_index = std::cmp::min(color_index1, color_index2) as u8; // because black (default) will always be higher

            colors_index.push(color_index);
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
                let array_color_index = colors_index[vec_color_index];
                let color = colors[array_color_index as usize];
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
            output_video_file: "outputs/color_video.mp4".to_string(),
            show_progress: true,
            size: 1
        },
        frames,
    );
    println!("Done");
}
