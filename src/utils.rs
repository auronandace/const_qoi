pub const fn is_identical(first: &[u8], second: &[u8]) -> bool {
    let mut index = 0;
    while index != first.len() {
        if first[index] != second[index] {return false;}
        index += 1;
    }
    true
}

// panics if input_index > input.len()
pub const fn array_from_input<const N: usize>(input: &[u8], mut input_index: usize) -> [u8; N] {
    let mut output = [0; N];
    let mut output_index = 0;
    while output_index != N {
        output[output_index] = input[input_index];
        output_index += 1;
        input_index += 1;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::{array_from_input, is_identical};
    #[test]
    const fn infallible_is_identical() {
        assert!(is_identical(&[0, 1, 2, 3], &[0, 1, 2, 3]));
        assert!(!is_identical(&[0, 1, 2, 3], &[0, 1, 2, 4]));
    }
    #[test]
    const fn infallible_array_from_input() {
        let input = [55, 88, 72, 89, 11, 60, 7, 2, 0, 5, 54];
        let output_one: [u8; 4] = array_from_input(&input, 0);
        assert!(is_identical(&output_one, &[55, 88, 72, 89]));
        let output_two: [u8; 4] = array_from_input(&input, 3);
        assert!(is_identical(&output_two, &[89, 11, 60, 7]));
        let output_three: [u8; 8] = array_from_input(&input, input.len() - 8);
        assert!(is_identical(&output_three, &[89, 11, 60, 7, 2, 0, 5, 54]));
    }
}
