pub fn get_bit_at(input: u8, n: u8) -> bool {
    if n < 32 {
        input & (1 << n) != 0 // 1 == true, 0 == false
    } else {
        false
    }
}

pub fn get_rgb_for_bit(bit: bool) -> (u8, u8, u8) {
    if bit {
        // If true (1) = white = 255,255,255
        (255, 255, 255)
    } else {
        (0, 0, 0) // black = 0,0,0
    }
}

/**
 * Get the bit value from black and white value. Does not need to be perfect white and black.
 * Choose the value depending if closer of 0 or 255 in average
 **/
pub fn get_bit_from_rgb(rgb: Vec<u8>) -> bool {
    let sum: u32 = rgb.iter().map(|x| *x as u32).sum();
    if sum < (255_u32 * 3 / 2) {
        false
    } else {
        true
    }
}

pub fn mutate_byte(byte_val: &mut u8, bit_val: bool, position: u8) {
    let bi = if bit_val { 1 } else { 0 };
    // *byte_val = *byte_val & !(bi << position);
    *byte_val = *byte_val & !(1 << position) | (bi << position);
}

#[cfg(test)]
mod injectionlogics_tests {
    use super::*;

    #[test]
    fn test_get_bit_at() {
        let value = 54; // 0011 0110
        assert_eq!(get_bit_at(value, 7), false);
        assert_eq!(get_bit_at(value, 6), false);
        assert_eq!(get_bit_at(value, 5), true);
        assert_eq!(get_bit_at(value, 4), true);
        assert_eq!(get_bit_at(value, 3), false);
        assert_eq!(get_bit_at(value, 2), true);
        assert_eq!(get_bit_at(value, 1), true);
        assert_eq!(get_bit_at(value, 0), false);
    }

    #[test]
    fn test_get_rgb_for_bit_true() {
        let value = get_rgb_for_bit(true);
        assert_eq!(value.0, 255);
        assert_eq!(value.1, 255);
        assert_eq!(value.2, 255);
    }

    #[test]
    fn test_get_rgb_for_bit_false() {
        let value = get_rgb_for_bit(false);
        assert_eq!(value.0, 0);
        assert_eq!(value.1, 0);
        assert_eq!(value.2, 0);
    }

    #[test]
    fn test_get_bit_from_rgb_all_0() {
        let bit = get_bit_from_rgb(vec![0, 0, 0]);
        assert_eq!(bit, false);
    }
    #[test]
    fn test_get_bit_from_rgb_all_255() {
        let bit = get_bit_from_rgb(vec![255, 255, 255]);
        assert_eq!(bit, true);
    }

    #[test]
    fn test_mutate_byte_0_bit_true() {
        let mut input: u8 = 0b0000_0000;
        mutate_byte(&mut input, true, 0);
        assert_eq!(input, 0b0000_0001)
    }
    #[test]
    fn test_mutate_byte_1_bit_true() {
        let mut input: u8 = 0b0000_0000;
        mutate_byte(&mut input, true, 1);
        assert_eq!(input, 0b0000_0010)
    }
    #[test]
    fn test_mutate_byte_2_bit_false() {
        let mut input: u8 = 0b0000_0010;
        mutate_byte(&mut input, false, 1);
        assert_eq!(input, 0b0000_0000)
    }
}
