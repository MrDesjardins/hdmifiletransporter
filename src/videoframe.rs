use opencv::core::prelude::*;
use opencv::core::{Mat, Size, CV_8UC3};

/// Define a single frame that the video will play
/// E.g. on a 30fps video, there will be 30 VideoFrame every second
///
/// Original source: https://github.com/DvorakDwarf/Infinite-Storage-Glitch/blob/master/src/embedsource.rs
pub struct VideoFrame {
    /// A Mat is a dense array to store color
    ///
    /// Reference: https://docs.opencv.org/3.4/d3/d63/classcv_1_1Mat.html
    pub image: Mat,

    /// Depending of how we will translate the information into the video frame,
    /// we may color the information in different size. The size represent the number
    /// of pixel (width and height) for each value.
    ///
    /// # Expected Values
    /// 1, 2 or 4. Should never be 0 and would be innefficient to have bigger than 4.
    ///
    /// # Examples
    /// E.g. A size of 1 means each info is colored into 1 pixel
    /// E.g. A size of 2 means each info is colored into a 2x2 pixel (4 pixels)
    pub size: u8,

    /// Each frame has as width and height. This is the multiplication of both.
    /// The frame_size is the resolution of the video. We expect each frame of
    /// the video to have the same frame size
    pub frame_size: Size,

    /// The actual size is calculated using the size of the information.
    /// It represents the number of data that will actually fits into the
    /// frame. The size of the info affect the actual size of the frame but
    /// will always render the frame_size. Thus, actual_size is equal or
    /// smaller than the frame_size.
    ///
    /// # Examples
    /// E.g. A width of 100, height of 100 and a size of 1 gives an actual
    /// size of 10 000.
    /// E.g. A width of 100, height of 100 and a size of 2 gives an actual
    /// size of 2 500.
    pub actual_size: Size,
}

impl VideoFrame {
    pub fn new(size: u8, width: u16, height: u16) -> VideoFrame {
        let frame_size = Size::new(width.into(), height.into());
        let actual_width = width - (width % u16::from(size));
        let actual_height = height - (height % u16::from(size));
        let actual_size = Size::new(i32::from(actual_width), i32::from(actual_height));
        unsafe {
            let image = Mat::new_rows_cols(frame_size.height, frame_size.width, CV_8UC3)
                .expect("Failed to create new Mat");

            VideoFrame {
                image,
                size,
                frame_size,
                actual_size,
            }
        }
    }

    pub fn write(&mut self, r: u8, g: u8, b: u8, x: u16, y: u16) {
        for i in 0..self.size {
            for j in 0..self.size {
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

    pub fn from(image: Mat, size: u8, instruction: bool) -> Result<VideoFrame, String> {
        let width = image.cols();
        let height = image.rows();
        let frame_size = Size::new(width, height);

        if height % i32::from(size) != 0 && !(instruction) {
            return Err("Image size is not a multiple of the size".to_string());
        }

        let actual_size = Size::new(
            width - (width % i32::from(size)),
            height - (height % i32::from(size)),
        );

        Ok(VideoFrame {
            image,
            size,
            frame_size,
            actual_size,
        })
    }
}
