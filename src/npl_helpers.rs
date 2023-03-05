/// Decode the unary value of the given slice.
/// A 0 bit cannot be followed by a 1 bit.
///
/// # Arguments
/// * `bit_buffer` - buffer containing to calculate the value from
/// * `start` - start bit position
/// * `stop` - stop bit position
pub fn get_unary_value(bit_buffer: &[Option<bool>], start: usize, stop: usize) -> Option<i8> {
    let mut sum = 0;
    let mut old_bit = None;
    for bit in &bit_buffer[start..=stop] {
        (*bit)?;
        let s_bit = bit.unwrap();
        if s_bit && old_bit == Some(false) {
            return None;
        }
        sum += s_bit as i8;
        old_bit = *bit;
    }
    Some(sum)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_unary_value_all_0() {
        const UNARY_BUFFER: [Option<bool>; 4] =
            [Some(false), Some(false), Some(false), Some(false)];
        assert_eq!(get_unary_value(&UNARY_BUFFER, 0, 3), Some(0));
    }
    #[test]
    fn test_get_unary_value_all_1() {
        const UNARY_BUFFER: [Option<bool>; 4] = [Some(true), Some(true), Some(true), Some(true)];
        assert_eq!(get_unary_value(&UNARY_BUFFER, 0, 3), Some(4));
    }
    #[test]
    fn test_get_unary_value_middle() {
        const UNARY_BUFFER: [Option<bool>; 4] = [Some(true), Some(true), Some(false), Some(false)];
        assert_eq!(get_unary_value(&UNARY_BUFFER, 0, 3), Some(2));
    }
    #[test]
    fn test_get_unary_value_1_after_0() {
        const UNARY_BUFFER: [Option<bool>; 4] = [Some(false), Some(false), Some(true), Some(false)];
        assert_eq!(get_unary_value(&UNARY_BUFFER, 0, 3), None);
    }
    #[test]
    fn test_get_unary_value_invalid_none() {
        const UNARY_BUFFER: [Option<bool>; 4] = [Some(true), Some(true), None, Some(false)];
        assert_eq!(get_unary_value(&UNARY_BUFFER, 0, 3), None);
    }
}
