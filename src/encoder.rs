use crate::{
    consts::{DEFAULT_PIXEL, ZERO_PIXEL},
    error::QoiError,
    header::{QoiHeader, QoiHeaderInternal},
    pixel::Pixel,
};

/// Indicates whether the [`QoiEncoder`] is finished.
#[allow(clippy::large_enum_variant)]
pub enum QoiEncoderProgress<const N: usize> {
    /// Returns [`QoiEncoder`] for further processing, the output buffer and the empty space left in the output buffer.
    ///
    /// The encoded QOI chunks can vary in size.
    /// The largest chunk is `5` bytes which is the minimum allowed output buffer size.
    /// Due to the different size chunks the buffer may not always be returned full.
    Unfinished(QoiEncoder, [u8; N], usize),
    /// Returns the output buffer and the amount of bytes that should be considered as free space.
    Finished([u8; N], usize),
}

/// A streaming encoder for the QOI image format.
///
/// To generate a [`QoiEncoder`] and retrieve a [`QoiHeader`] you must input the pixel data as a slice of bytes.\
/// To encode the image you must process the pixels by inputting an array to be used as a buffer.\
/// You can then match on [`QoiEncoderProgress`] to retrieve your buffer and either the encoder (to continue
/// processing more pixels) or the amount of bytes that are considered free space in your buffer.
#[allow(clippy::module_name_repetitions)]
pub struct QoiEncoder {
    state: QoiEncoderInternal,
}

impl QoiEncoder {
    /// Generates a [`QoiEncoder`] and a [`QoiHeader`] from the input bytes of pixel data.
    ///
    /// The channels and colorspace values are purely informative and will be used to populate the returned header.
    ///
    /// The channels value is meant to specify whether the input data is `3` byte pixels (RGB) or `4` byte pixels (RGBA).
    /// The encoder will compare the specified width and height with the length of the input data to determine whether
    /// the input data represents `3` or `4` byte pixels.
    /// This means you can input a valid but incorrect value for channels and it will be used for the returned header but
    /// the encoder will process the input bytes correctly.
    ///
    /// # Errors
    ///
    /// Will return `Err` if the following is true:
    ///
    /// 1: The width or height values are `0`.\
    /// 2: The channels value is not `3` (RGB) or `4` (RGBA).\
    /// 3: The colorspace value is not `0` (sRGB with linear alpha) or `1` (all channels linear).\
    /// 4: The amount of bytes in input are not divisible by the specified channels value.\
    /// 5: The specified width and height calculate to a different amount of pixels compared to the input bytes.
    pub const fn new(input: &[u8],
                     width: u32,
                     height: u32,
                     channels: u8,
                     colorspace: u8) -> Result<(Self, QoiHeader), QoiError> {
        if width == 0 || height == 0 {return Err(QoiError::InvalidWidthHeight(width, height));}
        if channels != 3 && channels != 4 {return Err(QoiError::InvalidChannelsValue(channels));}
        if colorspace != 0 && colorspace != 1 {return Err(QoiError::InvalidColorspaceValue(colorspace));}
        let specified_pixel_amount = width as u64 * height as u64;
        if input.len() % (channels as usize) != 0 {return Err(QoiError::IncorrectInputData(input.len(), channels));}
        let three = (input.len() as u64) / 3;
        let four = (input.len() as u64) / 4;
        let (actual_pixel_amount, real_channels) = if three == specified_pixel_amount {(three, 3)} else {(four, 4)};
        if specified_pixel_amount != three && specified_pixel_amount != four {
            return Err(QoiError::InputHeaderMismatch(width, height, actual_pixel_amount));
        }
        let header = QoiHeaderInternal::new(width, height, channels, colorspace);
        let encoder = QoiEncoder {state: QoiEncoderInternal::new(specified_pixel_amount, real_channels != 3)};
        Ok((encoder, header.public()))
    }
    /// Processes the input bytes as pixel data and fills the output buffer with bytes representing QOI data chunks.
    ///
    /// The minimum size buffer required is `5` bytes.
    /// This is equivalent to the largest returnable QOI data chunk.
    ///
    /// # Errors
    ///
    /// Will return `Err` if output buffer is less than `5` bytes.
    #[inline]
    pub const fn process_pixels<const N: usize>(mut self,
                                                input: &[u8],
                                                output: [u8; N]) -> Result<QoiEncoderProgress<N>, QoiError> {
        if output.len() < 5 {return Err(QoiError::BufferTooSmall(output.len()));}
        let (encoder, output) = self.state.process_pixels(input, output);
        self.state = encoder;
        let empty = self.state.output_buffer_space;
        if self.all_pixels_processed() {
            Ok(QoiEncoderProgress::Finished(output, empty))
        } else {
            Ok(QoiEncoderProgress::Unfinished(self, output, empty))
        }
    }
    #[inline]
    const fn all_pixels_processed(&self) -> bool {
        self.state.pixel_amount == 0
    }
}

pub struct QoiEncoderInternal {
    byte_index: usize,             // keeps track of input index, always increments
    seen_pixels: [Pixel; 64],
    previous_pixel: Pixel,
    pixel_amount: u64,             // keeps track of pixels to process, always decrements
    alpha: bool,                   // determines whether input is 3 or 4 byte pixels
    output_buffer_space: usize,    // how much of the output buffer is free space
}

impl QoiEncoderInternal {
    const fn new(pixel_amount: u64, alpha: bool) -> Self {
        Self {
            byte_index: 0,
            seen_pixels: [ZERO_PIXEL; 64],
            previous_pixel: DEFAULT_PIXEL,
            pixel_amount,
            alpha,
            output_buffer_space: 0,
        }
    }
    #[allow(clippy::cast_possible_truncation)] // index guaranteed to be 0..=63 so cannot truncate when casting to u8
    #[inline]
    const fn process_pixels<const N: usize>(mut self, input: &[u8], mut output: [u8; N]) -> (Self, [u8; N]) {
        let mut output_index = 0;
        while self.pixel_amount != 0 {
            let new_pixel; (self, new_pixel) = self.advance_input_pixel(input);
            let index = new_pixel.calculate_hash_index();
            let index_pixel = self.seen_pixels[index];
            if index_pixel.is_same(new_pixel) {
                if self.previous_pixel.is_same(new_pixel) {
                    (self, output, output_index) = self.run_chunk(input, output, output_index);
                } else {
                    output[output_index] = index as u8; // QOI_OP_INDEX: 2bit tag (00), 6bit val (000000)
                    output_index += 1;
                }
            } else if new_pixel.alpha == self.previous_pixel.alpha {
                if self.byte_index == 3 || self.byte_index == 4 {
                    (self, output, output_index) = self.run_chunk(input, output, output_index);
                } else if let Some(diff) = new_pixel.diff(self.previous_pixel) {
                    output[output_index] = diff; // QOI_OP_DIFF: 2bit tag (01), 3x2bit rgb diff (00)
                    output_index += 1;
                } else if let Some((tag_green, red_blue)) = new_pixel.luma(self.previous_pixel) {
                    if output.len() - output_index < 2 {self = self.rewind_input_index(); break;}
                    output[output_index] = tag_green; // QOI_OP_LUMA: 2bit tag (10), 6bit green diff
                    output_index += 1;
                    output[output_index] = red_blue; // 4bit red diff, 4bit blue diff (both based on green diff)
                    output_index += 1;
                } else { // must be RGB chunk
                    if output.len() - output_index < 4 {self = self.rewind_input_index(); break;}
                    (output, output_index) = new_pixel.rgb_to_output(output, output_index);
                }
            } else { // must be RGBA chunk
                if output.len() - output_index < 5 {self = self.rewind_input_index(); break;}
                (output, output_index) = new_pixel.rgba_to_output(output, output_index);
            }
            self.previous_pixel = new_pixel;
            self.seen_pixels[index] = new_pixel;
            self.pixel_amount -= 1;
            if output_index == output.len() {break;}
        }
        self.output_buffer_space = output.len() - output_index;
        (self, output)
    }
    #[inline]
    const fn is_byte_index_safe(&self, input: &[u8]) -> bool {
        self.byte_index < input.len()
    }
    #[inline]
    const fn advance_input_pixel(mut self, input: &[u8]) -> (Self, Pixel) {
        let red = input[self.byte_index]; self.byte_index += 1;
        let green = input[self.byte_index]; self.byte_index += 1;
        let blue = input[self.byte_index]; self.byte_index += 1;
        let mut alpha = self.previous_pixel.alpha;
        if self.alpha {alpha = input[self.byte_index]; self.byte_index += 1;}
        (self, Pixel::new(red, green, blue, alpha))
    }
    #[inline]
    const fn rewind_input_index(mut self) -> Self {
        if self.alpha {self.byte_index -= 4;} else {self.byte_index -= 3;}
        self
    }
    #[inline]
    const fn run_chunk<const N: usize>(mut self,
                                       input: &[u8],
                                       mut output: [u8; N],
                                       mut output_index: usize) -> (Self, [u8; N], usize) {
        let mut new_pixel; let mut run = 0; // QOI_OP_RUN: 2bit tag (11), 6bit val (000000), bias -1 (0 means a run of 1)
        while self.is_byte_index_safe(input) {
            (self, new_pixel) = self.advance_input_pixel(input);
            if self.previous_pixel.is_same(new_pixel) && run < 61 { // bias -1 (61 means a run of 62)
                run += 1;
                self.pixel_amount -= 1;
            } else {
                self = self.rewind_input_index(); break;
            }
        }
        run |= 0xc0; // apply bitwise OR to add tag
        output[output_index] = run; output_index += 1;
        (self, output, output_index)
    }
}

#[cfg(test)]
mod tests {
    use crate::{error::QoiError, utils::is_identical};
    use super::{QoiEncoder, QoiEncoderProgress};
    #[test]
    const fn good_new_four_byte() {
        let input = [255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 2, 4, 0);
        assert!(both.is_ok());
        if let Ok((encoder, header)) = both {
            assert!(encoder.state.byte_index == 0);
            let mut index = 0;
            while index < 64 {
                assert!(encoder.state.seen_pixels[index].red == 0);
                assert!(encoder.state.seen_pixels[index].green == 0);
                assert!(encoder.state.seen_pixels[index].blue == 0);
                assert!(encoder.state.seen_pixels[index].alpha == 0);
                index += 1;
            }
            assert!(encoder.state.previous_pixel.red == 0);
            assert!(encoder.state.previous_pixel.green == 0);
            assert!(encoder.state.previous_pixel.blue == 0);
            assert!(encoder.state.previous_pixel.alpha == 255);
            assert!(encoder.state.pixel_amount == 4);
            assert!(encoder.state.alpha);
            assert!(encoder.state.output_buffer_space == 0);
            assert!(header.width() == 2);
            assert!(header.height() == 2);
            assert!(header.channels() == 4);
            assert!(header.colorspace() == 0);
        }
    }
    #[test]
    const fn good_new_three_byte() {
        let input = [255, 255, 255,
                     255, 255, 255,
                     255, 255, 255,
                     255, 255, 255,
                     255, 255, 255,
                     255, 255, 255,
                     255, 255, 255,
                     255, 255, 255,
                     255, 255, 255];
        let both = QoiEncoder::new(&input, 3, 3, 3, 0);
        assert!(both.is_ok());
        if let Ok((encoder, header)) = both {
            assert!(encoder.state.byte_index == 0);
            let mut index = 0;
            while index < 64 {
                assert!(encoder.state.seen_pixels[index].red == 0);
                assert!(encoder.state.seen_pixels[index].green == 0);
                assert!(encoder.state.seen_pixels[index].blue == 0);
                assert!(encoder.state.seen_pixels[index].alpha == 0);
                index += 1;
            }
            assert!(encoder.state.previous_pixel.red == 0);
            assert!(encoder.state.previous_pixel.green == 0);
            assert!(encoder.state.previous_pixel.blue == 0);
            assert!(encoder.state.previous_pixel.alpha == 255);
            assert!(encoder.state.pixel_amount == 9);
            assert!(!encoder.state.alpha);
            assert!(encoder.state.output_buffer_space == 0);
            assert!(header.width() == 3);
            assert!(header.height() == 3);
            assert!(header.channels() == 3);
            assert!(header.colorspace() == 0);
        }
    }
    #[test]
    const fn bad_width_height() {
        let input = [255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 0, 0, 4, 0);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::InvalidWidthHeight(width, height) => {
                    assert!(width == 0);
                    assert!(height == 0);
                },
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn bad_channels() {
        let input = [255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 2, 5, 0);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::InvalidChannelsValue(channels) => assert!(channels == 5),
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn bad_colorspace() {
        let input = [255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 2, 4, 2);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::InvalidColorspaceValue(colorspace) => assert!(colorspace == 2),
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn bad_input_data() {
        let input = [255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 2, 4, 0);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::IncorrectInputData(size, channels) => {
                    assert!(size == 15);
                    assert!(channels == 4);
                },
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn bad_input_mismatch() {
        let input = [255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 3, 4, 0);
        assert!(both.is_err());
        if let Err(e) = both {
            match e {
                QoiError::InputHeaderMismatch(width, height, pixel_amount) => {
                    assert!(width == 2);
                    assert!(height == 3);
                    assert!(pixel_amount == 4);
                },
                _ => unreachable!(),
            }
        }
    }
    #[test]
    const fn good_process_pixels_finished() {
                                          // starting previous pixel: [0, 0, 0, 255]
        let input = [0, 0, 0, 0,          // encoded as index chunk   [0] (special 1st index case)
                     1, 1, 1, 0,          // encoded as diff chunk    [127] new rgb(+1,+1,+1), same alpha
                     255, 255, 255, 255,  // encoded as rgba chunk    [255, 255, 255, 255, 255]
                     255, 255, 255, 255]; // encoded as run chunk     [192] run of 1
        let both = QoiEncoder::new(&input, 2, 2, 4, 0);
        assert!(both.is_ok());
        if let Ok((encoder, _)) = both {
            let ouput = [0; 10];
            let progress = encoder.process_pixels(&input, ouput);
            assert!(progress.is_ok());
            if let Ok(progress) = progress {
                match progress {
                    QoiEncoderProgress::Finished(buffer, empty) => {
                        assert!(is_identical(
                            &buffer, &[0,                       // [0, 0, 0, 0] encoded as index chunk
                                       127,                     // [1, 1, 1, 0] encoded as diff chunk
                                       255, 255, 255, 255, 255, // [255, 255, 255, 255] encoded as rgba chunk
                                       192,                     // [255, 255, 255, 255] encoded as run chunk (run of 1)
                                       0, 0                     // spare space in output buffer
                                       ])
                        );
                        assert!(empty == 2);
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn good_process_pixels_unfinished() {
                                          // starting previous pixel: [0, 0, 0, 255]
        let input = [0, 0, 0, 255,        // encoded as run chunk     [192] run of 1 (special 1st run case)
                     0, 0, 0, 222,        // encoded as rgba chunk    [255, 0, 0, 0, 222]
                     0, 0, 0, 222,        // encoded as run chunk     [192] run of 1
                     0, 0, 0, 222,        // replaces run chunk value [193] run of 2
                     0, 0, 0, 255,        // encoded as index chunk   [53]
                     0, 0, 0, 222,        // encoded as index chunk   [10]
                     0, 0, 0, 222,        // encoded as run chunk     [192] run of 1
                     0, 2, 0, 222,        // encoded as luma chunk    [162, 102] new rgb(+0,+2,+0), same alpha
                     128, 128, 128, 222,  // encoded as rgb chunk     [254, 128, 128, 128]
                     255, 255, 255, 255,  // not enough space left in output buffer (would be rgba chunk)
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 6, 4, 0);
        assert!(both.is_ok());
        if let Ok((encoder, _)) = both {
            let ouput = [0; 20];
            let progress = encoder.process_pixels(&input, ouput);
            assert!(progress.is_ok());
            if let Ok(progress) = progress {
                match progress {
                    QoiEncoderProgress::Unfinished(encoder, buffer, empty) => {
                        assert!(empty == 4);
                        assert!(encoder.state.byte_index == 36);
                        let mut index = 0;
                        while index < 64 {
                            match index {
                                10 => { // rgb chunk (overwrote first rgba chunk, same index from hash)
                                    assert!(encoder.state.seen_pixels[index].red == 128);
                                    assert!(encoder.state.seen_pixels[index].green == 128);
                                    assert!(encoder.state.seen_pixels[index].blue == 128);
                                    assert!(encoder.state.seen_pixels[index].alpha == 222);
                                },
                                20 => { // luma chunk
                                    assert!(encoder.state.seen_pixels[index].red == 0);
                                    assert!(encoder.state.seen_pixels[index].green == 2);
                                    assert!(encoder.state.seen_pixels[index].blue == 0);
                                    assert!(encoder.state.seen_pixels[index].alpha == 222);
                                },
                                53 => { // first index chunk (same as default previous pixel value)
                                    assert!(encoder.state.seen_pixels[index].red == 0);
                                    assert!(encoder.state.seen_pixels[index].green == 0);
                                    assert!(encoder.state.seen_pixels[index].blue == 0);
                                    assert!(encoder.state.seen_pixels[index].alpha == 255);
                                },
                                _ => { // default initialised pixel value in seen pixels
                                    assert!(encoder.state.seen_pixels[index].red == 0);
                                    assert!(encoder.state.seen_pixels[index].green == 0);
                                    assert!(encoder.state.seen_pixels[index].blue == 0);
                                    assert!(encoder.state.seen_pixels[index].alpha == 0);
                                },
                            }
                            index += 1;
                        }
                        assert!(encoder.state.previous_pixel.red == 128);
                        assert!(encoder.state.previous_pixel.green == 128);
                        assert!(encoder.state.previous_pixel.blue == 128);
                        assert!(encoder.state.previous_pixel.alpha == 222);
                        assert!(encoder.state.pixel_amount == 3);
                        assert!(encoder.state.alpha);
                        assert!(encoder.state.output_buffer_space == 4);
                        assert!(is_identical(              // [0, 0, 0, 255] starting previous pixel
                            &buffer, &[192,                // [0, 0, 0, 255] encoded as run chunk (run of 1)
                                       255, 0, 0, 0, 222,  // [0, 0, 0, 222] encoded as rgba chunk
                                       193,                // [0, 0, 0, 222] encoded as run chunk (run of 2)
                                       53,                 // [0, 0, 0, 255] encoded as index chunk
                                       10,                 // [0, 0, 0, 222] encoded as index chunk
                                       192,                // [0, 0, 0, 222] encoded as run chunk (run of 1)
                                       162, 102,           // [0, 2, 0, 222] encoded as luma chunk
                                       254, 128, 128, 128, // [128, 128, 128, 222] encoded as rgb chunk
                                       0, 0, 0, 0          // no space in output buffer to fit next chunk
                                       ])
                        );
                    },
                    _ => unreachable!(),
                }
            }
        }
    }
    #[test]
    const fn bad_process_pixels_buffer_size() {
        let input = [255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255,
                     255, 255, 255, 255];
        let both = QoiEncoder::new(&input, 2, 2, 4, 0);
        assert!(both.is_ok());
        if let Ok((encoder, _)) = both {
            let ouput = [0; 4];
            let progress = encoder.process_pixels(&input, ouput);
            assert!(progress.is_err());
            if let Err(e) = progress {
                match e {
                    QoiError::BufferTooSmall(size) => assert!(size == 4),
                    _ => unreachable!(),
                }
            }
        }
    }
}
