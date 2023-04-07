use opencv::core::prelude::*;
use opencv::core::{Mat, Size, CV_8UC3};

use crate::injectionextraction::{Color};



/// Define a single frame that the video will play
/// E.g. on a 30fps video, there will be 30 VideoFrame every second
///
/// Original source: https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/embedsource.rs
#[derive(Clone)]
pub struct VideoFrame {
    /// A Mat is a dense array to store color
    ///
    /// Reference: https://docs.opencv.org/3.4/d3/d63/classcv_1_1Mat.html
    pub image: Mat,

    /// Each frame has as width and height. This is the multiplication of both.
    /// The frame_size is the resolution of the video. We expect each frame of
    /// the video to have the same frame size
    pub frame_size: Size,

}

impl VideoFrame {
    pub fn new(size: u8, width: u16, height: u16) -> VideoFrame {
        let frame_size = Size::new(width.into(), height.into());
        unsafe {
            let image = Mat::new_rows_cols(frame_size.height, frame_size.width, CV_8UC3)
                .expect("Failed to create new Mat");

            VideoFrame {
                image,
                frame_size
            }
        }
    }

    pub fn write(&mut self, r: u8, g: u8, b: u8, x: u16, y: u16, size: u8) {
        for i in 0..size {
            for j in 0..size {
                let result = self.image.at_2d_mut::<opencv::core::Vec3b>(
                    i32::from(y) + i32::from(i),
                    i32::from(x) + i32::from(j),
                );
                match result {
                    Ok(bgr) => {
                        // Opencv works with bgr format instead of rgb
                        bgr[2] = r;
                        bgr[1] = g;
                        bgr[0] = b;
                    }
                    Err(e) => panic!("{:?}", e),
                }
            }
        }
    }

    pub fn from(image: Mat, size: u8) -> Result<VideoFrame, String> {
        let width = image.cols();
        let height = image.rows();
        let frame_size = Size::new(width, height);

        if height % i32::from(size) != 0 {
            return Err("Image size is not a multiple of the size".to_string());
        }

        let actual_size = Size::new(
            width - (width % i32::from(size)),
            height - (height % i32::from(size)),
        );

        Ok(VideoFrame {
            image,
            frame_size
        })
    }

    pub fn read_coordinate_color(&self, x: u16, y: u16) -> Color {
        let bgr = self
            .image
            .at_2d::<opencv::core::Vec3b>(y.into(), x.into())
            .unwrap();

        Color {
            r: bgr[2],
            g: bgr[1],
            b: bgr[0],
        }
    }
}
