use crate::{
    consts::{DEFAULT_PIXEL, END_MARKER, ZERO_PIXEL},
    error::QoiError,
    header::{QoiHeader, QoiHeaderInternal},
    pixel::Pixel,
    utils::{array_from_input, is_identical}
};

/// Indicates whether the [`QoiDecoder`] is finished.
#[allow(clippy::large_enum_variant)]
pub enum QoiDecoderProgress<const N: usize> {
    /// Returns [`QoiDecoder`] for further processing and the filled output buffer.
    /// The output buffer must be divisible by `4` which means it will always be full with new `4` byte RGBA pixel data.
    Unfinished((QoiDecoder, [u8; N])),
    /// Returns the output buffer and the amount of bytes that should be considered as free space.
    Finished(([u8; N], usize)),
}

/// A streaming decoder for the QOI image format.
///
/// To generate a [`QoiDecoder`] and retrieve a [`QoiHeader`] you must input the QOI image data as a slice of bytes.\
/// To decode the image you must process the chunks by inputting an array to be used as a buffer.\
/// You can then match on [`QoiDecoderProgress`] to retrieve your buffer and either the decoder (to continue
/// processing more chunks) or the amount of bytes that are considered free space in your buffer.
#[allow(clippy::module_name_repetitions)]
pub struct QoiDecoder {
    state: QoiDecoderInternal,
    expected_pixels: u64, // total size of image in pixels, does not change
}

impl QoiDecoder {
    /// Generates a [`QoiDecoder`] and a [`QoiHeader`] from the input bytes of a QOI image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if input is less than `23` bytes, the end marker contains invalid values or the header
    /// is malformed in the following ways:
    ///
    /// 1: The magic bytes are incorrect (they should be "qoif" ([`113`, `111`, `105`, `102`])).\
    /// 2: The width or height values are `0`.\
    /// 3: The channels value is not `3` (RGB) or `4` (RGBA).\
    /// 4: The colorspace value is not `0` (sRGB with linear alpha) or `1` (all channels linear).
    pub const fn new(input: &[u8]) -> Result<(Self, QoiHeader), QoiError> {
        if input.len() <= 22 {return Err(QoiError::InputTooSmall(input.len()));}
        match QoiHeaderInternal::extract(input) {
            Ok(header) => {
                let end: [u8; 8] = array_from_input(input, input.len() - 8);
                if !is_identical(&end, &END_MARKER) {
                    return Err(QoiError::InvalidEndMarker(end[0], end[1], end[2], end[3], end[4], end[5], end[6], end[7]));
                }
                let image_size = (header.width as u64) * (header.height as u64);
                Ok((Self {state: QoiDecoderInternal::new(14, image_size), expected_pixels: image_size}, header.public()))
            },
            Err(e) => Err(e),
        }
    }
    /// Processes the input bytes as QOI chunks and fills the output buffer with bytes representing RGBA pixel values.
    /// The output buffer is guaranteed to be full except on the final call.
    ///
    /// The minimum size buffer required is `4` bytes.
    /// This would be the least efficient buffer size as it would be the equivalent of processing `1` pixel at a time
    /// resulting in calling this method the same amount of times as there are total pixels.
    ///
    /// # Errors
    ///
    /// Will return `Err` if output buffer is not divisible by `4` or if input data is malformed in the following ways:
    ///
    /// 1: The header specifies more pixels than the data contains.\
    /// 2: The header specifies less pixels than the data contains.\
    /// 3: The final chunk is missing required bytes.
    #[inline]
    pub const fn process_chunks<const N: usize>(mut self,
                                                input: &[u8],
                                                output: [u8; N]) -> Result<QoiDecoderProgress<N>, QoiError> {
        if output.len() % 4 != 0 {return Err(QoiError::IncorrectBufferSize(output.len()));}
        let (decoder, output) = self.state.process_chunks(input, output);
        self.state = decoder;
        if self.all_pixels_processed() {
            if !self.is_byte_index_correct_for_end(input) {
                if self.is_byte_index_too_high(input) {
                    let last_five: [u8; 5] = array_from_input(input, input.len() - 13);
                    let amount = 8 - (input.len() - self.state.byte_index);
                    return Err(QoiError::EndAsChunksFinished(last_five, amount));
                }
                let difference = (input.len() - 8) - self.state.byte_index;
                return Err(QoiError::MoreDataBeforeEnd(self.expected_pixels, difference));
            }
            Ok(QoiDecoderProgress::Finished((output, self.state.output_buffer_space)))
        } else {
            if self.is_byte_index_too_high(input) {
                let last_five: [u8; 5] = array_from_input(input, input.len() - 13);
                let amount = 8 - (input.len() - self.state.byte_index);
                return Err(QoiError::EndAsChunksUnfinished(self.state.pixel_amount, last_five, amount));
            } else if self.is_byte_index_correct_for_end(input) {
                let processed_pixels = self.expected_pixels - self.state.pixel_amount;
                return Err(QoiError::IncorrectPixelAmount(self.expected_pixels, processed_pixels));
            }
            Ok(QoiDecoderProgress::Unfinished((self, output)))
        }
    }
    #[inline]
    const fn all_pixels_processed(&self) -> bool {
        self.state.pixel_amount == 0
    }
    #[inline]
    const fn is_byte_index_correct_for_end(&self, input: &[u8]) -> bool {
        self.state.byte_index == input.len() - 8
    }
    #[inline]
    const fn is_byte_index_too_high(&self, input: &[u8]) -> bool {
        self.state.byte_index > input.len() - 8
    }
}

struct QoiDecoderInternal {
    byte_index: usize,          // keeps track of input index, always increments
    seen_pixels: [Pixel; 64],
    previous_pixel: Pixel,
    pixel_amount: u64,          // keeps track of pixels to process, always decrements
    output_buffer_space: usize, // last process_chunks may end in space in the output
    run_amount: u8,             // keeps track of processing run chunk when output buffer full
}

impl QoiDecoderInternal {
    const fn new(byte_index: usize, pixel_amount: u64) -> Self {
        Self {
            byte_index,
            seen_pixels: [ZERO_PIXEL; 64],
            previous_pixel: DEFAULT_PIXEL,
            pixel_amount,
            output_buffer_space: 0,
            run_amount: 0,
        }
    }
    #[inline]
    const fn process_chunks<const N: usize>(mut self, input: &[u8], mut output: [u8; N]) -> (Self, [u8; N]) {
        let mut output_index = 0;
        while self.pixel_amount != 0 && self.is_byte_index_safe(input) {
            let tag = input[self.byte_index];
            let mut current_pixel = self.previous_pixel;
            let mut run = false;
            match tag {
                254 => { // QOI_OP_RGB: 8bit tag (11111110)
                    self.byte_index += 1;
                    current_pixel.red = input[self.byte_index]; self.byte_index += 1;
                    current_pixel.green = input[self.byte_index]; self.byte_index += 1;
                    current_pixel.blue = input[self.byte_index]; self.byte_index += 1;
                },
                255 => { // QOI_OP_RGBA: 8bit tag (11111111)
                    self.byte_index += 1;
                    current_pixel.red = input[self.byte_index]; self.byte_index += 1;
                    current_pixel.green = input[self.byte_index]; self.byte_index += 1;
                    current_pixel.blue = input[self.byte_index]; self.byte_index += 1;
                    current_pixel.alpha = input[self.byte_index]; self.byte_index += 1;
                },
                0..=63 => { // QOI_OP_INDEX:  2bit tag (00), 6bit val (000000)
                    self.byte_index += 1;
                    current_pixel = self.seen_pixels[tag as usize];
                },
                64..=127 => { // QOI_OP_DIFF: 2bit tag (01), 3x2bit vals (00) rgb diffs, bias 2 (0 means -2)
                    self.byte_index += 1;
                    current_pixel.red = current_pixel.red.wrapping_add((tag >> 4) & 0x03).wrapping_sub(2);
                    current_pixel.green = current_pixel.green.wrapping_add((tag >> 2) & 0x03).wrapping_sub(2);
                    current_pixel.blue = current_pixel.blue.wrapping_add(tag & 0x03).wrapping_sub(2);
                },
                128..=191 => { // QOI_OP_LUMA: 2bit tag (10), 6bit val (000000) green diff, bias 32 (0 means -32)
                    self.byte_index += 1;
                    let green_diff = (tag & 0x3f).wrapping_sub(32); // clear tag with bitwise AND, include bias
                    let from_green = green_diff.wrapping_sub(8); // include bias, used for red and blue diff calcs
                    let red_and_blue = input[self.byte_index]; self.byte_index += 1; // 2x4bit values (0000)
                    current_pixel.red = current_pixel.red.wrapping_add(from_green.wrapping_add((red_and_blue >> 4) & 0x0f));
                    current_pixel.green = current_pixel.green.wrapping_add(green_diff);
                    current_pixel.blue = current_pixel.blue.wrapping_add(from_green.wrapping_add(red_and_blue & 0x0f));
                },
                192..=253 => { // QOI_OP_RUN: 2bit tag (11), 6bit val (000000), bias -1 (0 means 1)
                    if self.run_amount == 0 {self.run_amount = (tag & 0x3f) + 1;} // clear tag with bitwise AND, include bias
                    while self.run_amount != 0 {
                        if output_index == output.len() {break;}
                        (output, output_index) = current_pixel.to_output(output, output_index);
                        self.pixel_amount -= 1;
                        self.run_amount -= 1;
                    }
                    if self.run_amount == 0 {self.byte_index += 1;}
                    run = true;
                },
            }
            if !run {
                (output, output_index) = current_pixel.to_output(output, output_index);
                self.pixel_amount -= 1;
            }
            let index = current_pixel.calculate_hash_index();
            self.seen_pixels[index] = current_pixel;
            self.previous_pixel = current_pixel;
            if output_index == output.len() {break;}
        }
        self.output_buffer_space = output.len() - output_index;
        (self, output)
    }
    #[inline]
    const fn is_byte_index_safe(&self, input: &[u8]) -> bool {
        self.byte_index < (input.len() - 8)
    }
}

#[cfg(test)]
mod tests {
    use crate::{error::QoiError, utils::is_identical};
    use super::{QoiDecoder, QoiDecoderProgress};
    #[test]
    const fn good_new() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     198,                     // Run chunk (amount 7)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, header)) = both {
            assert!(decoder.state.byte_index == 14);
            let mut index = 0;
            while index < 64 {
                assert!(decoder.state.seen_pixels[index].red == 0);
                assert!(decoder.state.seen_pixels[index].green == 0);
                assert!(decoder.state.seen_pixels[index].blue == 0);
                assert!(decoder.state.seen_pixels[index].alpha == 0);
                index += 1;
            }
            assert!(decoder.state.previous_pixel.red == 0);
            assert!(decoder.state.previous_pixel.green == 0);
            assert!(decoder.state.previous_pixel.blue == 0);
            assert!(decoder.state.previous_pixel.alpha == 255);
            assert!(decoder.state.pixel_amount == 8);
            assert!(decoder.state.output_buffer_space == 0);
            assert!(decoder.state.run_amount == 0);
            assert!(decoder.expected_pixels == 8);
            assert!(is_identical(&header.magic_bytes(), &[113, 111, 105, 102]));
            assert!(header.width() == 2);
            assert!(header.height() == 4);
            assert!(header.channels() == 4);
            assert!(header.colorspace() == 0);
        }
    }
    #[test]
    const fn bad_header() {
        let input = [113, 110, 105, 102,      // incorrect magic bytes
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     198,                     // Run chunk (amount 7)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::InvalidMagicBytes(a, b, c, d) => {
                    assert!(a == 113);
                    assert!(b == 110);
                    assert!(c == 105);
                    assert!(d == 102);
                },
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn bad_end_marker() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     198,                     // Run chunk (amount 7)
                     0, 0, 0, 0, 5, 0, 0, 1]; // incorrect end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::InvalidEndMarker(a, b, c, d, f, g, h, i) => {
                    assert!(a == 0);
                    assert!(b == 0);
                    assert!(c == 0);
                    assert!(d == 0);
                    assert!(f == 5);
                    assert!(g == 0);
                    assert!(h == 0);
                    assert!(i == 1);
                },
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn good_process_chunks_unfinished() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     3,                       // channels (3 = RGB)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     254, 255, 255, 255,      // RGB chunk
                     127,                     // Diff chunk (r+1, g+1, b+1)
                     128, 55,                 // Luma chunk (r-37, g-32, b-33)
                     38,                      // Index chunk
                     195,                     // Run chunk (amount 4)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 16];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_ok());
            if let Ok(progress) = progress {
                match progress {
                    QoiDecoderProgress::Unfinished((decoder, buffer)) => {
                        assert!(decoder.state.byte_index == 22);
                        let mut index = 0;
                        while index < 64 {
                            if index == 38 { // rgb chunk
                                assert!(decoder.state.seen_pixels[index].red == 255);
                                assert!(decoder.state.seen_pixels[index].green == 255);
                                assert!(decoder.state.seen_pixels[index].blue == 255);
                                assert!(decoder.state.seen_pixels[index].alpha == 255);
                            } else if index == 63 { // luma chunk
                                assert!(decoder.state.seen_pixels[index].red == 219);
                                assert!(decoder.state.seen_pixels[index].green == 224);
                                assert!(decoder.state.seen_pixels[index].blue == 223);
                                assert!(decoder.state.seen_pixels[index].alpha == 255);
                            } else if index == 53 { // diff chunk
                                assert!(decoder.state.seen_pixels[index].red == 0);
                                assert!(decoder.state.seen_pixels[index].green == 0);
                                assert!(decoder.state.seen_pixels[index].blue == 0);
                                assert!(decoder.state.seen_pixels[index].alpha == 255);
                            } else {
                                assert!(decoder.state.seen_pixels[index].red == 0);
                                assert!(decoder.state.seen_pixels[index].green == 0);
                                assert!(decoder.state.seen_pixels[index].blue == 0);
                                assert!(decoder.state.seen_pixels[index].alpha == 0);
                            }
                            index += 1;
                        }
                        assert!(decoder.state.previous_pixel.red == 255);
                        assert!(decoder.state.previous_pixel.green == 255);
                        assert!(decoder.state.previous_pixel.blue == 255);
                        assert!(decoder.state.previous_pixel.alpha == 255);
                        assert!(decoder.state.pixel_amount == 4);
                        assert!(decoder.state.output_buffer_space == 0);
                        assert!(decoder.state.run_amount == 0);
                        assert!(decoder.expected_pixels == 8);
                        let expected = [255, 255, 255, 255,  // pixel from RGB chunk
                                        0, 0, 0, 255,        // pixel from Diff chunk
                                        219, 224, 223, 255,  // pixel from Luma chunk
                                        255, 255, 255, 255]; // pixel from Index chunk
                        assert!(is_identical(&buffer, &expected));
                    },
                    QoiDecoderProgress::Finished(_) => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn good_process_chunks_finished() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     198,                     // Run chunk (amount 7)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 32];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_ok());
            if let Ok(progress) = progress {
                match progress {
                    QoiDecoderProgress::Finished((buffer, empty_space)) => {
                        assert!(is_identical(&buffer, &[255; 32]));
                        assert!(empty_space == 0);
                    },
                    QoiDecoderProgress::Unfinished(_) => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn bad_buffer_size() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     198,                     // Run chunk (amount 7)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 5];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_err());
            if let Err(e) = progress {
                match e {
                    QoiError::IncorrectBufferSize(size) => assert!(size == 5),
                    _ => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn bad_process_chunks_header_overstates_pixels_index_ready_for_end() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 4,              // height (4xu8 into 1xu32 big endian: 4)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                                              // missing chunks: header stated 8 pixels only found 1
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 8];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_err());
            if let Err(e) = progress {
                match e {
                    QoiError::IncorrectPixelAmount(header, actual) => {
                        assert!(header == 8);
                        assert!(actual == 1);
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn bad_process_chunks_header_understates_pixels_index_before_end() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 2,              // height (4xu8 into 1xu32 big endian: 2)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 222, // RGBA chunk
                     255, 255, 255, 255, 252, // RGBA chunk
                     255, 255, 255, 254, 155, // RGBA chunk
                     255, 255, 254, 255, 255, // RGBA chunk (last pixel specified by header)
                     255, 255, 255, 255, 211, // RGBA chunk
                     255, 251, 255, 255, 151, // RGBA chunk
                     255, 253, 255, 255, 251, // RGBA chunk
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 32];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_err());
            if let Err(e) = progress {
                match e {
                    QoiError::MoreDataBeforeEnd(header, chunk_bytes) => {
                        assert!(header == 4);
                        assert!(chunk_bytes == 15);
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn bad_process_chunks_header_overstates_pixels_index_into_end() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 2,              // height (4xu8 into 1xu32 big endian: 2)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     255, 255, 255, 255, 252, // RGBA chunk
                     255,                     // RGBA chunk (incomplete, should have 4 more bytes)
                                              // missing 4th pixel (header states 2x2 = 4 pixels)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 32];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_err());
            if let Err(e) = progress {
                match e {
                    QoiError::EndAsChunksUnfinished(chunks_left, last_five, end_bytes) => {
                        assert!(chunks_left == 1);
                        assert!(is_identical(&last_five, &[255, 255, 255, 252, 255]));
                        assert!(end_bytes == 4);
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn bad_process_chunks_header_states_correct_pixels_index_into_end() {
        let input = [113, 111, 105, 102,      // magic bytes (qoif)
                     0, 0, 0, 2,              // width (4xu8 into 1xu32 big endian: 2)
                     0, 0, 0, 2,              // height (4xu8 into 1xu32 big endian: 2)
                     4,                       // channels (4 = RGBA)
                     0,                       // colorspace (0 = sRGB with linear alpha)
                     255, 255, 255, 255, 255, // RGBA chunk
                     255, 255, 255, 255, 252, // RGBA chunk
                     255, 255, 253, 255, 252, // RGBA chunk
                     255,                     // RGBA chunk (incomplete, should have 4 more bytes)
                     0, 0, 0, 0, 0, 0, 0, 1]; // end marker
        let both = QoiDecoder::new(&input);
        assert!(both.is_ok());
        if let Ok((decoder, _)) = both {
            let output = [0; 32];
            let progress = decoder.process_chunks(&input, output);
            assert!(progress.is_err());
            if let Err(e) = progress {
                match e {
                    QoiError::EndAsChunksFinished(last_five, end_bytes) => {
                        assert!(is_identical(&last_five, &[255, 253, 255, 252, 255]));
                        assert!(end_bytes == 4);
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
}
