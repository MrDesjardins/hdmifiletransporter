use crate::bitlogics::{get_bit_at, get_bit_at64, get_byte_from_bits};
use crate::injectionextraction::{FORMAT_MAGIC, HEADER_BITS};

/// Information to pass from the injection to the extraction.
/// The way to move information from one to the other is to
/// reserve this structure of data.
///
/// The data transmitted is the size of the relevant byte in the payload
/// allowing to know how many byte of data is part of the data injected.
///
/// The produced data might contain more byte to fill up a frame.
///
#[derive(Debug, Clone, Copy)]
pub struct Instruction {
    /// 0 => most left visually
    /// 63 => most right visually
    pub relevant_byte_count_in_64bits: [bool; 64],
}
impl Instruction {
    // Create a new instruction. Passing the information we want to tell.
    // byte_len is the number of byte of the content
    pub fn new(byte_len: u64) -> Instruction {
        let mut relevant_byte: [bool; 64] = [false; 64];
        for i in 0..64 {
            relevant_byte[i] = get_bit_at64(byte_len, (64 - i - 1) as u8);
        }
        Instruction {
            relevant_byte_count_in_64bits: relevant_byte,
        }
    }

    /// From the bits saved, gets the bytes from position
    /// Position 0 = Most left side visually, Position 7 = Most right side visually
    pub fn get_byte(&self, visual_position_from_left: u8) -> u8 {
        if visual_position_from_left >= 8 {
            panic!("Only position of 0 to 7 inclusively exist in a 64 bits");
        }
        // Array
        // Position: [0, 1, 2, 3, 4, 5, 6, 7]
        // Bits array: [0....63]
        let mut slice: [bool; 8] = [false; 8];
        let start_position = visual_position_from_left * 8;
        let slice_values = &self.relevant_byte_count_in_64bits
            [start_position as usize..start_position as usize + 8];
        slice.copy_from_slice(slice_values);
        get_byte_from_bits(slice)
    }

    /// Array of bytes
    /// Position 0 = Most left side visually, Position 7 = Most right side visually
    pub fn get_bytes(&self) -> [u8; 8] {
        let mut result: [u8; 8] = [0; 8];
        for i in 0..8 {
            result[i] = self.get_byte(i as u8);
        }

        result
    }

    /// Get the number of relevant bit in a number format. This match the number
    /// passing in the new function
    pub fn get_data_size(&self) -> u64 {
        let mut result: u64 = 0;
        for i in 0..64 {
            let position = 64 - i - 1;
            let bi = if self.relevant_byte_count_in_64bits[position] {
                1
            } else {
                0
            };
            result = result & !(1 << i) | (bi << i);
        }
        result
    }
}

/// The role of a frame in the stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameType {
    /// The red marker frame. Its value carries the total number of data bytes.
    Start,
    /// A data frame. Its value carries the page (frame) number.
    Data,
}

impl FrameType {
    fn to_byte(self) -> u8 {
        match self {
            FrameType::Start => 0,
            FrameType::Data => 1,
        }
    }
    fn from_byte(b: u8) -> Option<FrameType> {
        match b {
            0 => Some(FrameType::Start),
            1 => Some(FrameType::Data),
            _ => None,
        }
    }
}

/// Per-frame header written in black/white in the first `HEADER_BITS` content
/// cells. Every field is stored most-significant-bit first.
///
/// ```text
///   bits   0..8   format magic (FORMAT_MAGIC)
///   bits   8..16  frame type (0 = Start, 1 = Data)
///   bits  16..80  value (Start = total byte count, Data = page number)
///   bits  80..112 CRC32 over [type byte, value big-endian, payload bytes]
///   bits 112..128 reserved (zero)
/// ```
///
/// The CRC lets the extractor reject torn or garbled frames before they are
/// trusted, and the explicit type removes the need to guess the start frame
/// from its colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameHeader {
    pub frame_type: FrameType,
    pub value: u64,
    pub crc: u32,
}

impl FrameHeader {
    /// CRC32 over the type byte, the value (big-endian) and the payload bytes.
    pub fn compute_crc(frame_type: FrameType, value: u64, payload: &[u8]) -> u32 {
        let mut hasher = crc32fast::Hasher::new();
        hasher.update(&[frame_type.to_byte()]);
        hasher.update(&value.to_be_bytes());
        hasher.update(payload);
        hasher.finalize()
    }

    /// Build a header, computing the CRC over the given payload.
    pub fn new(frame_type: FrameType, value: u64, payload: &[u8]) -> FrameHeader {
        FrameHeader {
            frame_type,
            value,
            crc: FrameHeader::compute_crc(frame_type, value, payload),
        }
    }

    /// Serialize to exactly `HEADER_BITS` bits (true = white pixel).
    pub fn to_bits(&self) -> Vec<bool> {
        let mut bits = vec![false; HEADER_BITS];
        let mut idx = 0;
        push_byte_bits(&mut bits, &mut idx, FORMAT_MAGIC);
        push_byte_bits(&mut bits, &mut idx, self.frame_type.to_byte());
        for i in 0..64 {
            bits[idx] = get_bit_at64(self.value, (63 - i) as u8);
            idx += 1;
        }
        for i in 0..32 {
            bits[idx] = get_bit_at64(self.crc as u64, (31 - i) as u8);
            idx += 1;
        }
        bits
    }

    /// Parse a header from at least `HEADER_BITS` bits. Returns `None` when the
    /// format magic does not match or the frame type is unknown (which also
    /// happens for noise read off a non-aligned frame).
    pub fn from_bits(bits: &[bool]) -> Option<FrameHeader> {
        if bits.len() < HEADER_BITS {
            return None;
        }
        let mut idx = 0;
        let magic = read_byte_bits(bits, &mut idx);
        if magic != FORMAT_MAGIC {
            return None;
        }
        let type_byte = read_byte_bits(bits, &mut idx);
        let frame_type = FrameType::from_byte(type_byte)?;
        let mut value: u64 = 0;
        for _ in 0..64 {
            value = (value << 1) | (bits[idx] as u64);
            idx += 1;
        }
        let mut crc: u32 = 0;
        for _ in 0..32 {
            crc = (crc << 1) | (bits[idx] as u32);
            idx += 1;
        }
        Some(FrameHeader {
            frame_type,
            value,
            crc,
        })
    }

    /// True when the stored CRC matches the CRC recomputed over this header's
    /// type and value plus the supplied payload bytes.
    pub fn verify(&self, payload: &[u8]) -> bool {
        self.crc == FrameHeader::compute_crc(self.frame_type, self.value, payload)
    }
}

fn push_byte_bits(bits: &mut [bool], idx: &mut usize, byte: u8) {
    for i in 0..8 {
        bits[*idx] = get_bit_at(byte, (7 - i) as u8);
        *idx += 1;
    }
}

fn read_byte_bits(bits: &[bool], idx: &mut usize) -> u8 {
    let mut slice = [false; 8];
    for s in slice.iter_mut() {
        *s = bits[*idx];
        *idx += 1;
    }
    get_byte_from_bits(slice)
}

#[cfg(test)]
mod injectionlogics_tests {
    use super::*;

    #[test]
    fn test_instruction_new_ver_small() {
        let instruction = Instruction::new(1); // 00000000000000000000000000000000000000000000000000000000...1100100

        assert_eq!(instruction.relevant_byte_count_in_64bits[63], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[62], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[61], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[60], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[59], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[58], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[57], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[0], false);
    }
    #[test]
    fn test_instruction_new_small() {
        let instruction = Instruction::new(100); // 00000000000000000000000000000000000000000000000000000000...1100100

        assert_eq!(instruction.relevant_byte_count_in_64bits[63], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[62], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[61], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[60], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[59], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[58], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[57], true);
    }

    #[test]
    fn test_instruction_new_large() {
        let instruction = Instruction::new(389657); // 00000000000000000000000000000000000000000000000000000000...1011111001000011001

        assert_eq!(instruction.relevant_byte_count_in_64bits[63], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[62], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[61], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[60], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[59], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[58], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[57], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[56], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[55], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[54], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[53], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[52], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[51], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[50], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[49], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[48], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[47], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[46], false);
        assert_eq!(instruction.relevant_byte_count_in_64bits[45], true);
        assert_eq!(instruction.relevant_byte_count_in_64bits[44], false);
    }

    #[test]
    fn test_instruction_get_byte() {
        // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        // 48 23 97 63 120 220 191 162
        let instruction = Instruction::new(3465345363523452834); // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        let byte_0 = instruction.get_byte(0); // 00110000 -> 48
        let byte_1 = instruction.get_byte(1); // 00010111 -> 23
        let byte_2 = instruction.get_byte(2); // 01100001 -> 97
        let byte_3 = instruction.get_byte(3); // 00111111 -> 63
        let byte_4 = instruction.get_byte(4); // 01111000 -> 120
        let byte_5 = instruction.get_byte(5); // 01111000 -> 220
        let byte_6 = instruction.get_byte(6); // 10111111 -> 191
        let byte_7 = instruction.get_byte(7); // 10100010 -> 162
        assert_eq!(byte_0, 48);
        assert_eq!(byte_1, 23);
        assert_eq!(byte_2, 97);
        assert_eq!(byte_3, 63);
        assert_eq!(byte_4, 120);
        assert_eq!(byte_5, 220);
        assert_eq!(byte_6, 191);
        assert_eq!(byte_7, 162);
    }

    #[test]
    #[should_panic]
    fn test_instruction_get_byte_outside_range() {
        // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        // 48 23 97 63 120 220 191 162
        let instruction = Instruction::new(3465345363523452834); // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        instruction.get_byte(64); // Outside range
    }

    #[test]
    fn test_instruction_get_bytes() {
        // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        // 48 23 97 63 120 220 191 162
        let instruction = Instruction::new(3465345363523452834); // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        let result = instruction.get_bytes();
        assert_eq!(result[0], 48);
        assert_eq!(result[1], 23);
        assert_eq!(result[2], 97);
        assert_eq!(result[3], 63);
        assert_eq!(result[4], 120);
        assert_eq!(result[5], 220);
        assert_eq!(result[6], 191);
        assert_eq!(result[7], 162);
    }

    #[test]
    fn test_instruction_get_data_size_small() {
        let instruction = Instruction::new(75); // 000000 ... 1001011
        let result = instruction.get_data_size();
        assert_eq!(result, 75)
    }

    #[test]
    fn test_instruction_get_data_size() {
        let instruction = Instruction::new(3465345363523452834); // 00110000 00010111 01100001 00111111 01111000 11011100 10111111 10100010
        let result = instruction.get_data_size();
        assert_eq!(result, 3465345363523452834)
    }

    #[test]
    fn test_instruction_zero() {
        let instruction = Instruction::new(0); // 000000000000000000000000000000000000000000000000000000... 000

        assert_eq!(instruction.get_data_size(), 0);
    }

    #[test]
    fn test_frame_header_round_trip_data() {
        let payload = vec![1u8, 2, 3, 250, 0, 7];
        let header = FrameHeader::new(FrameType::Data, 42, &payload);
        let bits = header.to_bits();
        assert_eq!(bits.len(), HEADER_BITS);
        let parsed = FrameHeader::from_bits(&bits).expect("magic should match");
        assert_eq!(parsed, header);
        assert_eq!(parsed.frame_type, FrameType::Data);
        assert_eq!(parsed.value, 42);
        assert!(parsed.verify(&payload));
    }

    #[test]
    fn test_frame_header_round_trip_start() {
        let header = FrameHeader::new(FrameType::Start, 123456, &[]);
        let parsed = FrameHeader::from_bits(&header.to_bits()).unwrap();
        assert_eq!(parsed.frame_type, FrameType::Start);
        assert_eq!(parsed.value, 123456);
        assert!(parsed.verify(&[]));
    }

    #[test]
    fn test_frame_header_crc_detects_payload_corruption() {
        let payload = vec![10u8, 20, 30];
        let header = FrameHeader::new(FrameType::Data, 1, &payload);
        let parsed = FrameHeader::from_bits(&header.to_bits()).unwrap();
        // A single flipped payload byte must fail verification.
        let corrupted = vec![10u8, 20, 31];
        assert!(!parsed.verify(&corrupted));
    }

    #[test]
    fn test_frame_header_bad_magic_is_rejected() {
        // All-false bits => magic 0x00 != FORMAT_MAGIC
        let bits = vec![false; HEADER_BITS];
        assert!(FrameHeader::from_bits(&bits).is_none());
    }
}
