use crate::bitlogics::{get_bit_at64, get_byte_from_bits};

///
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

    /// Get the size back in a number format
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
}
