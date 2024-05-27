#[derive(Clone, Copy, Debug)]
pub struct Pixel {
    pub red: u8,
    pub green: u8,
    pub blue: u8,
    pub alpha: u8,
}

impl Pixel {
    #[inline]
    pub const fn new(red: u8, green: u8, blue: u8, alpha: u8) -> Self {
        Self {red, green, blue, alpha}
    }
    #[inline]
    pub const fn calculate_hash_index(self) -> usize { // guaranteed to output 0..=63
        (self.red as usize * 3 + self.green as usize * 5 + self.blue as usize * 7 + self.alpha as usize * 11) % 64
    }
    // puts pixel data in output buffer and increments output index. used only in decoder.
    #[inline]
    pub const fn to_output<const N: usize>(self, mut output: [u8; N], mut index: usize) -> ([u8; N], usize) {
        output[index] = self.red; index += 1;
        output[index] = self.green; index += 1;
        output[index] = self.blue; index += 1;
        output[index] = self.alpha; index += 1;
        (output, index)
    }
    // puts RGB chunk tag and data in output buffer and increments output index. used only in encoder.
    #[inline]
    pub const fn rgb_to_output<const N: usize>(self, mut output: [u8; N], mut index: usize) -> ([u8; N], usize) {
        output[index] = 254; index += 1;        // QOI_OP_RGB: 8bit tag  (11111110)
        output[index] = self.red; index += 1;   // RED:        8bit data (0..=255)
        output[index] = self.green; index += 1; // GREEN:      8bit data (0..=255)
        output[index] = self.blue; index += 1;  // BLUE:       8bit data (0..=255)
        (output, index)
    }
    // puts RGBA chunk tag and data in output buffer and increments output index. used only in encoder.
    #[inline]
    pub const fn rgba_to_output<const N: usize>(self, mut output: [u8; N], mut index: usize) -> ([u8; N], usize) {
        output[index] = 255; index += 1;        // QOI_OP_RGBA: 8bit tag  (11111111)
        output[index] = self.red; index += 1;   // RED:         8bit data (0..=255)
        output[index] = self.green; index += 1; // GREEN:       8bit data (0..=255)
        output[index] = self.blue; index += 1;  // BLUE:        8bit data (0..=255)
        output[index] = self.alpha; index += 1; // ALPHA:       8bit data (0..=255)
        (output, index)
    }
    #[inline]
    pub const fn is_same(self, other: Self) -> bool {
        self.red == other.red && self.green == other.green && self.blue == other.blue && self.alpha == other.alpha
    }
    // returns diff chunk to put into output buffer. must call on new pixel and feed in old. used only in encoder.
    #[inline]
    pub const fn diff(self, old: Self) -> Option<u8> { // QOI_OP_DIFF: 2bit tag (01), 3x2bit vals (00) rgb diffs
        let red = self.red.wrapping_sub(old.red).wrapping_add(2);       // Subtracting old from new gives you
        let green = self.green.wrapping_sub(old.green).wrapping_add(2); // the difference. Add bias of 2 for storage.
        let blue = self.blue.wrapping_sub(old.blue).wrapping_add(2);    // (0 means -2 difference, 3 means +1 difference)
        if red > 3 || green > 3 || blue > 3 {return None;}
        Some(64 | red << 4 | green << 2 | blue) // Use bitwise OR and bitshifts on tag to apply rgb diff vals.
    }
    // returns luma chunk to put into output buffer. must call on new pixel and feed in old. used only in encoder.
    #[inline] // QOI_OP_LUMA: 2bit tag (10), 6bit val (000000) green diff, bias 32 (0 means -32)
    pub const fn luma(self, old: Self) -> Option<(u8, u8)> {
        let red_diff = self.red.wrapping_sub(old.red);                 // Subtract old from new to get the difference
        let green_diff = self.green.wrapping_sub(old.green);           // and then add the bias. Green is 6bit val with
        let blue_diff = self.blue.wrapping_sub(old.blue);              // a bias of 32 (0 means -32, 63 means +31). Red
        let red = red_diff.wrapping_add(8).wrapping_sub(green_diff);   // and blue are 4bit vals with a bias of 8 (0 means
        let green = green_diff.wrapping_add(32);                       // -8, 15 means +7). Red and blue base their diffs
        let blue = blue_diff.wrapping_add(8).wrapping_sub(green_diff); // off of the green difference.
        if red > 15 || green > 63 || blue > 15 {return None;}
        Some((128 | green, red << 4 | blue)) // 1st byte: tag bitwise OR green. 2nd byte red bitshift left and bitwise OR blue
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::is_identical;
    use super::Pixel;
    #[test]
    const fn infallible_calculate_hash_index() {
        assert!(Pixel::new(0, 0, 0, 0).calculate_hash_index() == 0);
        assert!(Pixel::new(255, 255, 255, 255).calculate_hash_index() == 38);
    }
    #[test]
    const fn infallible_to_output() {
        let (output, index) = Pixel::new(0, 0, 0, 0).to_output([1, 1, 1, 1], 0);
        assert!(is_identical(&output, &[0, 0, 0, 0]));
        assert!(index == 4);
    }
    #[test]
    const fn infallible_rgb_to_output() {
        let (output, index) = Pixel::new(0, 0, 0, 0).rgb_to_output([1, 1, 1, 1, 1], 0);
        assert!(is_identical(&output, &[254, 0, 0, 0, 1]));
        assert!(index == 4);
    }
    #[test]
    const fn infallible_rgba_to_output() {
        let (output, index) = Pixel::new(0, 0, 0, 5).rgba_to_output([1, 1, 1, 1, 1, 1], 0);
        assert!(is_identical(&output, &[255, 0, 0, 0, 5, 1]));
        assert!(index == 5);
    }
    #[test]
    const fn infallible_is_same() {
        let a = Pixel::new(5, 5, 5, 5);
        let b = Pixel::new(5, 5, 5, 5);
        let c = Pixel::new(6, 6, 6, 6);
        assert!(a.is_same(b));
        assert!(!a.is_same(c));
    }
    #[test]
    const fn infallible_diff() {
        let mut new = Pixel::new(0, 0, 0, 255);         // red diff:   -1 stored as 1 (b01)
        let mut old = Pixel::new(1, 1, 1, 255);         // green diff: -1 stored as 1 (b01)
        let new_from_old = new.diff(old);               // blue diff:  -1 stored as 1 (b01)
        assert!(new_from_old.is_some());                // diff tag: 64 (b01000000)
        if let Some(byte) = new_from_old {              //  ttrrggbb (t=tag; r=red; g=green; b=blue)
            assert!((64 | 1 << 4 | 1 << 2 | 1) == 85);  // b01010101 (2bit tag 01, 3x2bit rgb vals)
            assert!(byte == 85);
        }
        new = Pixel::new(255, 255, 255, 255);           // red diff:   -2 stored as 0 (b00)
        old = Pixel::new(1, 1, 1, 255);                 // green diff: -2 stored as 0 (b00)
        let new_from_old = new.diff(old);               // blue diff:  -2 stored as 0 (b00)
        assert!(new_from_old.is_some());                // diff tag: 64 (b01000000)
        if let Some(byte) = new_from_old {              //  ttrrggbb (t=tag; r=red; g=green; b=blue)
            assert!((64 | 0 << 4 | 0 << 2 | 0) == 64);  // b01000000 (2bit tag 01, 3x2bit rgb vals)
            assert!(byte == 64);
        }
        new = Pixel::new(0, 1, 2, 255);                 // red diff:   -1 stored as 1 (b01)
        old = Pixel::new(1, 1, 1, 255);                 // green diff:  0 stored as 2 (b10)
        let new_from_old = new.diff(old);               // blue diff:  +1 stored as 3 (b11)
        assert!(new_from_old.is_some());                // diff tag: 64 (b01000000)
        if let Some(byte) = new_from_old {              //  ttrrggbb (t=tag; r=red; g=green; b=blue)
            assert!((64 | 1 << 4 | 2 << 2 | 3) == 91);  // b01011011 (2bit tag 01, 3x2bit rgb vals)
            assert!(byte == 91);
        }
        new = Pixel::new(1, 1, 1, 255);                 // red diff:   +1 stored as 3 (b11)
        old = Pixel::new(0, 0, 0, 255);                 // green diff: +1 stored as 3 (b11)
        let new_from_old = new.diff(old);               // blue diff:  +1 stored as 3 (b11)
        assert!(new_from_old.is_some());                // diff tag: 64 (b01000000)
        if let Some(byte) = new_from_old {              //  ttrrggbb (t=tag; r=red; g=green; b=blue)
            assert!((64 | 3 << 4 | 3 << 2 | 3) == 127); // b01111111 (2bit tag 01, 3x2bit rgb vals)
            assert!(byte == 127);
        }
        new = Pixel::new(1, 1, 10, 255);
        old = Pixel::new(0, 0, 0, 255);
        let new_from_old = new.diff(old);
        assert!(new_from_old.is_none());
    }
    #[test]
    const fn infallible_luma() {
        let mut new = Pixel::new(5, 5, 5, 255);             // red diff:   -5; computed to 8  (b1000)    +key------------+
        let mut old = Pixel::new(10, 10, 10, 255);          // green diff: -5; computed to 27 (b011011)  |ws=wrapping sub|
        let new_from_old = new.luma(old);                   // blue diff:  -5; computed to 8  (b1000)    |wa=wrapping add|
        assert!(new_from_old.is_some());                    // r/b: -5 wa 8 ws (ng(5) ws og(10) = 251)   |ng/og=new/old g|
        if let Some((tag_green, red_blue)) = new_from_old { // g: -5(251) wa 32                          | t=tag; r=red; |
            assert!(tag_green == 155);                      // tag (128)  ttgggggg  rrrrbbbb             |g=green; b=blue|
            assert!(red_blue == 136);                       // b10000000 b10011011 b10001000             +---------------+
        }
        new = Pixel::new(10, 10, 10, 255);                  // red diff:   +5; computed to 8  (b1000)    +key------------+
        old = Pixel::new(5, 5, 5, 255);                     // green diff: +5; computed to 37 (b100101)  |ws=wrapping sub|
        let new_from_old = new.luma(old);                   // blue diff:  +5; computed to 8  (b1000)    |wa=wrapping add|
        assert!(new_from_old.is_some());                    // r/b: +5 wa 8 ws (ng(10) ws og(5) = 5)     |ng/og=new/old g|
        if let Some((tag_green, red_blue)) = new_from_old { // g: 5 wa 32                                | t=tag; r=red; |
            assert!(tag_green == 165);                      // tag (128)  ttgggggg  rrrrbbbb             |g=green; b=blue|
            assert!(red_blue == 136);                       // b10000000 b10100101 b10001000             +---------------+
        }
        new = Pixel::new(80, 80, 44, 255);                  // red diff:   +26; computed to 4  (b0100)   +key------------+
        old = Pixel::new(54, 50, 15, 255);                  // green diff: +30; computed to 62 (b111110) |ws=wrapping sub|
        let new_from_old = new.luma(old);                   // blue diff:  +29; computed to 7  (b0111)   |wa=wrapping add|
        assert!(new_from_old.is_some());                    // r/b: diff wa 8 ws (ng(80) ws og(50) = 30) |ng/og=new/old g|
        if let Some((tag_green, red_blue)) = new_from_old { // g: 30 wa 32                               | t=tag; r=red; |
            assert!(tag_green == 190);                      // tag (128)  ttgggggg  rrrrbbbb             |g=green; b=blue|
            assert!(red_blue == 71);                        // b10000000 b10111110 b01000111             +---------------+
        }
        new = Pixel::new(128, 128, 128, 255);
        old = Pixel::new(1, 1, 1, 255);
        let new_from_old = new.luma(old);
        assert!(new_from_old.is_none());
    }
}
