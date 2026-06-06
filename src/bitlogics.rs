/// Get a bit value on a unsigned number
pub fn get_bit_at64(input: u64, n: u8) -> bool {
    if n < 64 {
        input & (1 << n) != 0 // 1 == true, 0 == false
    } else {
        panic!("The bit position must be between 0 and 63 inclusively on a 64 bits number");
    }
}

/// Get a bit value on a unsigned number
pub fn get_bit_at(input: u8, n: u8) -> bool {
    if n < 8 {
        input & (1 << n) != 0 // 1 == true, 0 == false
    } else {
        panic!("The bit position must be between 0 and 7 inclusively on a 8 bits number");
    }
}

/// Get color of white or back from a bit
/// 1 -> true -> white
/// 0 -> false -> black
pub fn get_rgb_for_bit(bit: bool) -> (u8, u8, u8) {
    if bit {
        // If true (1) = white = 255,255,255
        (255, 255, 255)
    } else {
        (0, 0, 0) // black = 0,0,0
    }
}

/// Get the bit value from black and white value. Does not need to be perfect white and black.
/// Choose the value depending if closer of 0 or 255 in average
pub fn get_bit_from_rgb(rgb: &Vec<u8>) -> bool {
    let sum: u32 = rgb.iter().map(|x| *x as u32).sum();
    sum >= (255_u32 * (rgb.len() as u32) / 2)
}

/// Change a bit from an existing byte
pub fn mutate_byte(byte_val: &mut u8, bit_val: bool, position: u8) {
    let bi = if bit_val { 1 } else { 0 };
    // *byte_val = *byte_val & !(bi << position);
    *byte_val = *byte_val & !(1 << position) | (bi << position);
}

/// Number of bits carried by a single channel for a quantized frame with
/// `levels` evenly spaced symbols. `levels` must be a power of two, so this is
/// simply `log2(levels)`.
pub fn bits_per_channel(levels: u32) -> u32 {
    levels.trailing_zeros()
}

/// Map a quantized symbol (`0..levels`) to the 8-bit channel value that sits at
/// its evenly spaced position. With `levels = 2` the symbols land on 0 and 255
/// (maximum separation); with `levels = 256` symbol and value are identical.
pub fn symbol_to_value(symbol: u32, levels: u32) -> u8 {
    if levels <= 1 {
        return 0;
    }
    let spacing = 255.0 / (levels as f64 - 1.0);
    (symbol as f64 * spacing).round() as u8
}

/// Map a measured 8-bit channel value back to the nearest quantized symbol in
/// `0..levels`. This is the decode counterpart of [`symbol_to_value`]; rounding
/// to the closest level is what gives the scheme its noise tolerance.
pub fn value_to_symbol(value: u8, levels: u32) -> u32 {
    if levels <= 1 {
        return 0;
    }
    let spacing = 255.0 / (levels as f64 - 1.0);
    let symbol = (value as f64 / spacing).round();
    symbol.clamp(0.0, (levels - 1) as f64) as u32
}

/// Get a byte from a list of bit
pub fn get_byte_from_bits(bits: [bool; 8]) -> u8 {
    let mut result: u8 = 0;
    for i in 0..8 {
        let position = 8 - i - 1 as u8;
        result += u8::pow(2, i as u32) * bits[position as usize] as u8
    }
    result
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
    #[should_panic]
    fn test_get_bit_at_outside_range() {
        let value = 54; // 0011 0110
        assert_eq!(get_bit_at(value, 8), false);
    }

    #[test]
    fn test_get_bit_at64() {
        let value = 54; // 0011 0110
        assert_eq!(get_bit_at64(value, 7), false);
        assert_eq!(get_bit_at64(value, 6), false);
        assert_eq!(get_bit_at64(value, 5), true);
        assert_eq!(get_bit_at64(value, 4), true);
        assert_eq!(get_bit_at64(value, 3), false);
        assert_eq!(get_bit_at64(value, 2), true);
        assert_eq!(get_bit_at64(value, 1), true);
        assert_eq!(get_bit_at64(value, 0), false);
    }

    #[test]
    #[should_panic]
    fn test_get_bit_at64_outside_range() {
        let value = 54; // 0011 0110
        assert_eq!(get_bit_at64(value, 64), false);
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
        let bit = get_bit_from_rgb(&vec![0, 0, 0]);
        assert_eq!(bit, false);
    }
    #[test]
    fn test_get_bit_from_rgb_all_255() {
        let bit = get_bit_from_rgb(&vec![255, 255, 255]);
        assert_eq!(bit, true);
    }
    #[test]
    fn test_get_bit_from_rgb_more_than_three() {
        let bit = get_bit_from_rgb(&vec![255, 255, 255, 0, 0, 0, 0, 0]);
        assert_eq!(bit, false);
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
    #[test]
    fn test_mutate_byte_many_mutate() {
        let mut input: u8 = 0b0000_0000;
        let expected: u8 = 0b0011_1011;
        mutate_byte(&mut input, false, 7);
        mutate_byte(&mut input, false, 6);
        mutate_byte(&mut input, true, 5);
        mutate_byte(&mut input, true, 4);
        mutate_byte(&mut input, true, 3);
        mutate_byte(&mut input, false, 2);
        mutate_byte(&mut input, true, 1);
        mutate_byte(&mut input, true, 0);
        assert_eq!(input, expected)
    }

    #[test]
    fn test_bits_per_channel() {
        assert_eq!(bits_per_channel(2), 1);
        assert_eq!(bits_per_channel(4), 2);
        assert_eq!(bits_per_channel(8), 3);
        assert_eq!(bits_per_channel(256), 8);
    }

    #[test]
    fn test_symbol_value_round_trip_two_levels() {
        // 2 levels -> black/white with maximum separation.
        assert_eq!(symbol_to_value(0, 2), 0);
        assert_eq!(symbol_to_value(1, 2), 255);
        assert_eq!(value_to_symbol(0, 2), 0);
        assert_eq!(value_to_symbol(255, 2), 1);
        // Noisy reads still snap to the nearest level.
        assert_eq!(value_to_symbol(20, 2), 0);
        assert_eq!(value_to_symbol(200, 2), 1);
    }

    #[test]
    fn test_symbol_value_degenerate_levels() {
        assert_eq!(symbol_to_value(10, 1), 0);
        assert_eq!(value_to_symbol(200, 1), 0);
        assert_eq!(symbol_to_value(10, 0), 0);
        assert_eq!(value_to_symbol(200, 0), 0);
    }

    #[test]
    fn test_symbol_value_round_trip_all_symbols() {
        for &levels in &[2u32, 4, 8, 16, 256] {
            for symbol in 0..levels {
                let value = symbol_to_value(symbol, levels);
                assert_eq!(
                    value_to_symbol(value, levels),
                    symbol,
                    "levels={levels} symbol={symbol} value={value}"
                );
            }
        }
    }

    #[test]
    fn test_get_bytes_from_bits_1() {
        // 155 = 10011011
        let input: [bool; 8] = [true, false, false, true, true, false, true, true];
        let output = get_byte_from_bits(input);
        assert_eq!(output, 155)
    }
}
