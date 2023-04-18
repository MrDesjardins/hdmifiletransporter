use opencv::core::prelude::*;
use opencv::core::{Mat, Size, CV_8UC3};

use crate::injectionextraction::Color;

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
    pub fn new(width: u16, height: u16) -> VideoFrame {
        let frame_size = Size::new(width.into(), height.into());
        unsafe {
            let image = Mat::new_rows_cols(frame_size.height, frame_size.width, CV_8UC3)
                .expect("Failed to create new Mat");

            VideoFrame { image, frame_size }
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

        Ok(VideoFrame { image, frame_size })
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

#[cfg(test)]
mod videoframe_tests {
    use super::VideoFrame;
    use opencv::core::prelude::*;
    use opencv::prelude::MatTraitConstManual;
    use opencv::core::{Mat, Size, CV_8UC3};
    #[test]
    fn test_new_create_image_size() {
        let result = VideoFrame::new(100, 50);
        assert_eq!(result.frame_size.width, 100);
        assert_eq!(result.frame_size.height, 50);
    }

    #[test]
    fn test_new_create_image_mat_size() {
        let result = VideoFrame::new(100, 50);
        let s = result.image.size().unwrap();
        assert_eq!(s.width, 100);
        assert_eq!(s.height, 50);
    }

    #[test]
    fn test_write_image_color() {
        let mut videoframe = VideoFrame::new(100, 50);
        videoframe.write(10, 20, 30, 0, 0, 1);
        let pixel = videoframe.image.at_2d::<opencv::core::Vec3b>(0, 0).unwrap();
        assert_eq!(pixel[0], 30);
        assert_eq!(pixel[1], 20);
        assert_eq!(pixel[2], 10);
    }

    #[test]
    fn test_read_coordinate_color() {
        let mut videoframe = VideoFrame::new(100, 50);
        videoframe.write(10, 20, 30, 0, 0, 1);
        let color = videoframe.read_coordinate_color(0, 0);
        assert_eq!(color.b, 30);
        assert_eq!(color.g, 20);
        assert_eq!(color.r, 10);
    }

    // #[test]
    // fn test_from_save_mat() {
    //     let mat = Mat::default();
    //     let ref1 =  &mat as *const Mat;
    //     let videoframe = VideoFrame::from(mat, 1);
    //     let unwrapped = videoframe.unwrap();
    //     assert!(&unwrapped.image as *const Mat == ref1);
        
    // }

    #[test]
    fn test_from_define_size() {

        unsafe {
            let mat = Mat::new_rows_cols(100, 200, CV_8UC3).unwrap();
            let videoframe = VideoFrame::from(mat, 1);
            let unwrapped = videoframe.unwrap();
            assert_eq!(unwrapped.frame_size.width, 200);
            assert_eq!(unwrapped.frame_size.height, 100);
        }
 
        
    }
}
