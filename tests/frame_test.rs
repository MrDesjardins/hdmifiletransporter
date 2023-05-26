use hdmifiletransporter::{
    create_starting_frame, data_to_frames, frames_to_data, options::AlgoFrame, ExtractOptions,
    InjectOptions, Instruction, VideoFrame,
};

fn get_unit_test_injection_option(size: u8, width: u16, height: u16) -> InjectOptions {
    return InjectOptions {
        fps: 30,
        width: width,
        height: height,
        size: size,
        algo: AlgoFrame::BW,
        show_progress: false,
        file_path: "".to_string(),
        output_video_file: "".to_string(),
    };
}

fn get_unit_test_extraction_option(size: u8, width: u16, height: u16) -> ExtractOptions {
    return ExtractOptions {
        video_file_path: "".to_string(),
        extracted_file_path: "".to_string(),
        fps: 30,
        width: width,
        height: height,
        size: size,
        algo: AlgoFrame::BW,
        show_progress: false,
    };
}

fn get_unit_test_frame(page_number: u64) -> VideoFrame {
    let size = 1;
    let mut frame = VideoFrame::new(64, 2);
    let (x, y) = frame.write_pagination(0, 0, &page_number, size);
    for i in 0..64 {
        frame.write(0, 0, 0, i, y, size); // Black pixel
    }
    return frame;
}

fn get_unit_test_data(number_of_byte: u64) -> Vec<u8> {
    let mut result = Vec::new();
    for i in 0..number_of_byte {
        result.push(((i % 65) + 65) as u8);
    }
    return result;
}
fn swap_elements<T: Clone>(vec: &mut Vec<T>, index1: usize, index2: usize) {
    let temp1 = vec[index1].clone();
    let temp2 = vec[index2].clone();

    vec[index1] = temp2;
    vec[index2] = temp1;
}
#[test]
fn test_frames_to_data_all_frames_good_order() {
    // Arrange
    let size = 1;
    let width = 64;
    let height = 3;
    let inject_options = get_unit_test_injection_option(size, width, height);
    let extract_options = get_unit_test_extraction_option(size, width, height);

    // Let's have 1 starting frame + 5 frames of data
    let number_bytes = width as u64 * (height as u64 - 1) * 5;
    let instruction_data = Instruction::new(number_bytes);
    let starting_frame = create_starting_frame(&instruction_data, &inject_options);

    let frame_data = get_unit_test_data(number_bytes); // -1 for the pagination information that take 64 spaces
    let frames = data_to_frames(&inject_options, frame_data);
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);

    // Act
    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    // Assert
    assert_eq!(data_from_frames.len(), number_bytes as usize)
}
#[test]
fn test_frames_to_data_all_frames_mixed_order() {
    // Arrange
    let size = 1;
    let width = 64;
    let height = 3;
    let inject_options = get_unit_test_injection_option(size, width, height);
    let extract_options = get_unit_test_extraction_option(size, width, height);

    // Let's have 1 starting frame + 5 frames of data
    let number_bytes = width as u64 * (height as u64 - 1) * 5;
    let instruction_data = Instruction::new(number_bytes);
    let starting_frame = create_starting_frame(&instruction_data, &inject_options);

    let frame_data = get_unit_test_data(number_bytes); // -1 for the pagination information that take 64 spaces
    let frames = data_to_frames(&inject_options, frame_data);

    // Mix the order of the frames
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);
    swap_elements(&mut merged_frames, 0, 1);
    swap_elements(&mut merged_frames, 2, 3);

    // Act
    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    // Assert
    assert_eq!(data_from_frames.len(), number_bytes as usize)
}

#[test]
fn test_frames_to_data_all_frames_repeting_frame() {
    // Arrange
    let size = 1;
    let width = 64;
    let height = 3;
    let inject_options = get_unit_test_injection_option(size, width, height);
    let extract_options = get_unit_test_extraction_option(size, width, height);

    // Let's have 1 starting frame + 5 frames of data
    let number_bytes = width as u64 * (height as u64 - 1) * 5;
    let instruction_data = Instruction::new(number_bytes);
    let starting_frame = create_starting_frame(&instruction_data, &inject_options);

    let frame_data = get_unit_test_data(number_bytes); // -1 for the pagination information that take 64 spaces

    let frames = data_to_frames(&inject_options, frame_data);
    let clone1 = frames[0].clone();
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);
    merged_frames.push(clone1); // Add the first frame twice

    // Act
    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    // Assert
    assert_eq!(data_from_frames.len(), number_bytes as usize)
}

#[test]
#[should_panic]
fn test_frames_to_data_missing_one_frame() {
    // Arrange
    let size = 1;
    let width = 64;
    let height = 3;
    let inject_options = get_unit_test_injection_option(size, width, height);
    let extract_options = get_unit_test_extraction_option(size, width, height);

    // Let's have 1 starting frame + 5 frames of data
    let number_bytes = width as u64 * (height as u64 - 1) * 5;
    let instruction_data = Instruction::new(number_bytes);
    let starting_frame = create_starting_frame(&instruction_data, &inject_options);

    let frame_data = get_unit_test_data(number_bytes); // -1 for the pagination information that take 64 spaces

    let frames = data_to_frames(&inject_options, frame_data);
    let mut merged_frames = vec![starting_frame];
    merged_frames.extend(frames);
    merged_frames.remove(2);

    // Act
    let data_from_frames = frames_to_data(&extract_options, merged_frames);

    // Assert
    assert_eq!(data_from_frames.len(), number_bytes as usize)
}

#[test]
#[should_panic(expected = "Instruction not found while extracting data from video")]
fn test_frames_to_data_missing_instruction_frame() {
    // Arrange
    let size = 1;
    let width = 64;
    let height = 3;
    let inject_options = get_unit_test_injection_option(size, width, height);
    let extract_options = get_unit_test_extraction_option(size, width, height);

    // Let's have 1 starting frame + 5 frames of data
    let number_bytes = width as u64 * (height as u64 - 1) * 5;

    let frame_data = get_unit_test_data(number_bytes); // -1 for the pagination information that take 64 spaces
    let frames = data_to_frames(&inject_options, frame_data);

    // Act
    let data_from_frames = frames_to_data(&extract_options, frames);

    // Assert
    assert_eq!(data_from_frames.len(), number_bytes as usize)
}
