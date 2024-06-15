use crate::{
    buffer::{QoiInputBuffer, QoiOutputBuffer},
    consts::{DEFAULT_PIXEL, END_MARKER, ZERO_PIXEL},
    error::QoiError,
    header::{QoiHeader, QoiHeaderInternal},
    pixel::Pixel,
    utils::is_identical,
};

/// Indicates the progress of decoding.
pub enum QoiDecoderProgress<const N: usize> {
    /// Returns [`QoiDecoderReady`] to indicate the decoder is ready for more input bytes.
    /// May return the amount of bytes with the array of bytes to take them from.
    AwaitInput(QoiDecoderReady<N>, Option<([u8; N], usize)>),
    /// Returns [`QoiDecoder`] to indicate the decoder still has input bytes left to process.
    /// May return the amount of bytes with the array of bytes to take them from.
    ContinueProcessing(QoiDecoder<N>, Option<([u8; N], usize)>),
    /// Indicates the decoder has finished processing all bytes.
    /// May return the amount of bytes with the array of bytes to take them from.
    Finished(Option<([u8; N], usize)>),
}

/// A streaming decoder for the QOI image format.
#[allow(clippy::module_name_repetitions)]
pub struct QoiDecoder<const N: usize> {
    data: QoiDecoderInternal<N>,
}

impl<const N: usize> QoiDecoder<N> {
    /// Generates a [`QoiDecoderReady`] and a [`QoiHeader`] from the first `14` bytes of a QOI image.
    ///
    /// # Errors
    ///
    /// Will return `Err` if buffer is less than `16` bytes and not divisible by 4 or `header_input` is
    /// malformed in the following ways:
    ///
    /// 1: The `header_input` size is not `14` bytes.\
    /// 2: The magic bytes are incorrect (they should be "qoif" ([`113`, `111`, `105`, `102`])).\
    /// 3: The width and/or height values are `0`.\
    /// 4: The channels value is not `3` (RGB) or `4` (RGBA).\
    /// 5: The colorspace value is not `0` (sRGB with linear alpha) or `1` (all channels linear).
    pub const fn init(header_input: &[u8], buffer: [u8; N]) -> Result<(QoiHeader, QoiDecoderReady<N>), QoiError> {
        if N < 16 && N % 4 != 0 {Err(QoiError::BadBufferSize(buffer.len()))}
        else if header_input.len() != 14 {Err(QoiError::BadHeaderSize(header_input.len()))} else {
            match QoiHeaderInternal::extract(header_input) {
                Ok(header) => {
                    let pixel_amount = (header.width as u64) * (header.height as u64);
                    Ok((header.public(), QoiDecoderReady {data: QoiDecoderInternal::new(pixel_amount, buffer)}))
                },
                Err(e) => Err(e),
            }
        }
    }
    /// Instructs the decoder to process any previous input bytes that have not already been processed.
    ///
    /// # Errors
    ///
    /// Will return `Err` if too many bytes are detected for the end marker or if the end marker contains
    /// incorrect bytes. Correct end marker bytes should be [`0`, `0`, `0`, `0`, `0`, `0`, `0`, `1`].
    #[inline]
    pub const fn continue_processing(self) -> Result<QoiDecoderProgress<N>, QoiError> {
        self.data.continue_processing()
    }
}

/// A streaming decoder for the QOI image format.
pub struct QoiDecoderReady<const N: usize> {
    data: QoiDecoderInternal<N>,
}

impl<const N: usize> QoiDecoderReady<N> {
    /// Instructs the decoder to process new `input` bytes.
    ///
    /// # Errors
    ///
    /// Will return `Err` if `input` is empty, `input` is greater than initial buffer size,
    /// too many bytes are detected for the end marker or if the end marker contains incorrect bytes.
    /// Correct end marker bytes should be [`0`, `0`, `0`, `0`, `0`, `0`, `0`, `1`].
    #[inline]
    pub const fn input_bytes_to_process(mut self, input: &[u8]) -> Result<QoiDecoderProgress<N>, QoiError> {
        if input.is_empty() || input.len() > N {Err(QoiError::BadInputSize(input.len(), N))} else {
            self.data.input = self.data.input.write_bytes(input);
            self.data.continue_processing()
        }
    }
}

struct QoiDecoderInternal<const N: usize> {
    input: QoiInputBuffer<N>,            // stores chunk/end marker bytes and used for input index
    output: QoiOutputBuffer<N>,          // stores pixel bytes and used for output index
    seen_pixels: [Pixel; 64],            // for comparison for determining chunk, see QOI spec
    previous_pixel: Pixel,               // for comparison for determining chunk, see QOI spec
    pixel_amount: u64,                   // keeps track of pixels to process, always decrements
    run_amount: u8,                      // keeps track of processing run chunk when output buffer full
    partial_chunk: Option<PartialChunk>, // holds partial chunk data for processing when input full
}

impl<const N: usize> QoiDecoderInternal<N> {
    const fn new(pixel_amount: u64, data: [u8; N]) -> Self {
        Self {
            input: QoiInputBuffer {data, capacity: 0, index: 0},
            output: QoiOutputBuffer {data, space: N}, seen_pixels: [ZERO_PIXEL; 64],
            previous_pixel: DEFAULT_PIXEL,
            pixel_amount,
            run_amount: 0,
            partial_chunk: None,
        }
    }
    #[inline]
    const fn continue_processing(mut self) -> Result<QoiDecoderProgress<N>, QoiError> {
        self = self.process_chunks();
        let amount = N - self.output.space;
        let output; (self.output, output) = self.output.get_all_bytes();
        let output = if amount == 0 {None} else {Some((output, amount))};
        if self.all_pixels_processed() {
            let input_left = self.input.capacity - self.input.index;
            if input_left > 8 {Err(QoiError::BadEndMarkerSize(input_left))} else if input_left == 8 {
                let end = self.get_end_bytes_from_input();
                if is_identical(&end, &END_MARKER) {Ok(QoiDecoderProgress::Finished(output))}
                else {Err(QoiError::BadEndMarkerBytes(end))}
            } else {
                Ok(QoiDecoderProgress::AwaitInput(QoiDecoderReady {data: self}, output))
            }
        } else if self.all_input_processed() {
            Ok(QoiDecoderProgress::AwaitInput(QoiDecoderReady {data: self}, output))
        } else {
            Ok(QoiDecoderProgress::ContinueProcessing(QoiDecoder {data: self}, output))
        }
    }
    #[inline]
    const fn process_chunks(mut self) -> Self {
        while self.pixel_amount != 0 && !self.all_input_processed() && self.output.space != 0 {
            let tag; if let Some(partial) = self.partial_chunk {tag = partial.get_tag();} else {
                if self.run_amount > 0 {tag = self.input.data[self.input.index];}
                else {(self.input, tag) = self.input.read_byte();}
                self.partial_chunk = None;
            }
            let mut current_pixel = self.previous_pixel;
            let mut run = false;
            match tag {
                255 | 254 => { // QOI_OP_RGBA: 8bit tag (11111111) | QOI_OP_RGB: 8bit tag (11111110)
                    let should_break; (self, current_pixel, should_break) = self.process_pixel(tag, current_pixel);
                    if should_break {break;}
                },
                192..=253 => { // QOI_OP_RUN: 2bit tag (11), 6bit val (000000), bias -1 (0 means 1)
                    if self.run_amount == 0 {self.run_amount = (tag & 0x3f) + 1;} // clear tag with bitwise AND, include bias
                    while self.run_amount != 0 && self.output.space != 0 {
                        self.output = self.output.append_bytes(&current_pixel.to_array());
                        self.pixel_amount -= 1;
                        self.run_amount -= 1;
                    }
                    run = true;
                },
                128..=191 => { // QOI_OP_LUMA: 2bit tag (10), 6bit val (000000) green diff, bias 32 (0 means -32)
                    if self.all_input_processed() {self.partial_chunk = Some(PartialChunk::OneByte(tag)); break;}
                    let green_diff = (tag & 0x3f).wrapping_sub(32); // clear tag with bitwise AND, include bias
                    let from_green = green_diff.wrapping_sub(8); // include bias, used for red and blue diff calcs
                    let red_and_blue; (self.input, red_and_blue) = self.input.read_byte();
                    current_pixel.red = current_pixel.red.wrapping_add(from_green.wrapping_add((red_and_blue >> 4) & 0x0f));
                    current_pixel.green = current_pixel.green.wrapping_add(green_diff);
                    current_pixel.blue = current_pixel.blue.wrapping_add(from_green.wrapping_add(red_and_blue & 0x0f));
                    self.partial_chunk = None;
                },
                64..=127 => { // QOI_OP_DIFF: 2bit tag (01), 3x2bit vals (00) rgb diffs, bias 2 (0 means -2)
                    current_pixel.red = current_pixel.red.wrapping_add((tag >> 4) & 0x03).wrapping_sub(2);
                    current_pixel.green = current_pixel.green.wrapping_add((tag >> 2) & 0x03).wrapping_sub(2);
                    current_pixel.blue = current_pixel.blue.wrapping_add(tag & 0x03).wrapping_sub(2);
                },
                0..=63 => { // QOI_OP_INDEX:  2bit tag (00), 6bit val (000000)
                    current_pixel = self.seen_pixels[tag as usize];
                },
            }
            if !run {
                self.pixel_amount -= 1;
                self.output = self.output.append_bytes(&current_pixel.to_array());
            }
            let index = current_pixel.calculate_hash_index();
            self.seen_pixels[index] = current_pixel;
            self.previous_pixel = current_pixel;
        }
        self
    }
    #[inline]
    const fn process_pixel(mut self, tag: u8, mut current_pixel: Pixel) -> (Self, Pixel, bool) {
        let mut should_break = false;
        if self.all_input_processed() {
            self.partial_chunk = Some(PartialChunk::OneByte(tag));
            should_break = true;
        } else {
            let one; (self.input, one) = self.input.read_byte();
            if let Some(part) = self.partial_chunk {
                if tag == 255 {(self, current_pixel, should_break) = self.process_rgba(tag, current_pixel, one, part);}
                else {(self, current_pixel, should_break) = self.process_rgb(current_pixel, one, part);}
            } else {
                if (self.input.capacity - self.input.index) < {if tag == 255 {4} else {3}} {should_break = true;}
                if self.all_input_processed() {
                    self.partial_chunk = Some(PartialChunk::TwoBytes(tag, one));
                } else {
                    let two; (self.input, two) = self.input.read_byte();
                    if self.all_input_processed() {
                        self.partial_chunk = Some(PartialChunk::ThreeBytes(tag, one, two));
                    } else {
                        let three; (self.input, three) = self.input.read_byte();
                        if tag == 255 {
                            if self.all_input_processed() {
                                self.partial_chunk = Some(PartialChunk::FourBytes(tag, one, two, three));
                            } else {
                                let four; (self.input, four) = self.input.read_byte();
                                current_pixel = Pixel::new(one, two, three, four);
                            }
                        } else {current_pixel = Pixel::new(one, two, three, current_pixel.alpha);}
                    }
                }
            }
        }
        (self, current_pixel, should_break)
    }
    #[inline]
    const fn process_rgba(mut self, tag: u8, mut current_pixel: Pixel, one: u8, part: PartialChunk) -> (Self, Pixel, bool) {
        let mut should_break = false;
        if let PartialChunk::FourBytes(_, red, green, blue) = part {
            current_pixel = Pixel::new(red, green, blue, one);
        } else if self.all_input_processed() {
            self.partial_chunk = part.add_byte(one);
            should_break = true;
        } else {
            let two; (self.input, two) = self.input.read_byte();
            if let PartialChunk::ThreeBytes(_, red, green) = part {
                current_pixel = Pixel::new(red, green, one, two);
            } else if self.all_input_processed() {
                self.partial_chunk = part.add_byte(one);
                self.partial_chunk = part.add_byte(two);
                should_break = true;
            } else {
                let three; (self.input, three) = self.input.read_byte();
                if let PartialChunk::TwoBytes(_, red) = part {
                    current_pixel = Pixel::new(red, one, two, three);
                } else if self.all_input_processed() {
                    self.partial_chunk = Some(PartialChunk::FourBytes(tag, one, two, three));
                    should_break = true;
                } else {
                    let four; (self.input, four) = self.input.read_byte();
                    current_pixel = Pixel::new(one, two, three, four);
                }
            }
        }
        (self, current_pixel, should_break)
    }
    #[inline]
    const fn process_rgb(mut self, mut current_pixel: Pixel, one: u8, part: PartialChunk) -> (Self, Pixel, bool) {
        let mut should_break = false;
        if let PartialChunk::ThreeBytes(_, red, green) = part {
            current_pixel = Pixel::new(red, green, one, current_pixel.alpha);
        } else if self.all_input_processed() {
            self.partial_chunk = part.add_byte(one);
            should_break = true;
        } else {
            let two; (self.input, two) = self.input.read_byte();
            if let PartialChunk::TwoBytes(_, red) = part {
                current_pixel = Pixel::new(red, one, two, current_pixel.alpha);
            } else if self.all_input_processed() {
                self.partial_chunk = part.add_byte(one);
                self.partial_chunk = part.add_byte(two);
                should_break = true;
            } else {
                let three; (self.input, three) = self.input.read_byte();
                current_pixel = Pixel::new(one, two, three, current_pixel.alpha);
            }
        }
        (self, current_pixel, should_break)
    }
    #[inline]
    const fn all_pixels_processed(&self) -> bool {
        self.pixel_amount == 0
    }
    #[inline]
    const fn all_input_processed(&self) -> bool {
        self.input.index == self.input.capacity
    }
    #[inline]
    const fn get_end_bytes_from_input(&self) -> [u8; 8] {
        let mut end = [0; 8];
        let mut index = 0;
        let mut data_index = self.input.index;
        while index < 8 {
            end[index] = self.input.data[data_index];
            index += 1;
            data_index += 1;
        }
        end
    }
}

#[derive(Clone, Copy)]
enum PartialChunk {
    OneByte(u8),
    TwoBytes(u8, u8),
    ThreeBytes(u8, u8, u8),
    FourBytes(u8, u8, u8, u8),
}

impl PartialChunk {
    #[inline]
    const fn get_tag(self) -> u8 {
        match self {
            Self::OneByte(tag) | Self::TwoBytes(tag, _) | Self::ThreeBytes(tag, _, _) | Self::FourBytes(tag, _, _, _) => tag,
        }
    }
    #[allow(clippy::match_wildcard_for_single_variants)]
    #[inline]
    const fn add_byte(self, byte: u8) -> Option<Self> {
        match self {
            Self::OneByte(tag) => Some(Self::TwoBytes(tag, byte)),
            Self::TwoBytes(tag, one) => Some(Self::ThreeBytes(tag, one, byte)),
            Self::ThreeBytes(tag, one, two) => Some(Self::FourBytes(tag, one, two, byte)),
            _ => unreachable!(), // for code coverage
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{buffer::{QoiInputBuffer, QoiOutputBuffer}, decoder::PartialChunk, error::QoiError, utils::is_identical};
    use super::{QoiDecoder, QoiDecoderInternal, QoiDecoderProgress};
    #[test]
    const fn good_init() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let buffer = [0; 16];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_ok());
        if let Ok((header, decoder)) = both {
            assert!(header.width() == 2);
            assert!(header.height() == 4);
            assert!(header.channels() == 4);
            assert!(header.colorspace() == 0);
            assert!(is_identical(&decoder.data.input.data, &buffer));
            assert!(decoder.data.input.capacity == 0);
            assert!(decoder.data.input.index == 0);
            assert!(is_identical(&decoder.data.output.data, &buffer));
            assert!(decoder.data.output.space == buffer.len());
            let mut index = 0;
            while index < 64 {
                assert!(decoder.data.seen_pixels[index].red == 0);
                assert!(decoder.data.seen_pixels[index].green == 0);
                assert!(decoder.data.seen_pixels[index].blue == 0);
                assert!(decoder.data.seen_pixels[index].alpha == 0);
                index += 1;
            }
            assert!(decoder.data.previous_pixel.red == 0);
            assert!(decoder.data.previous_pixel.green == 0);
            assert!(decoder.data.previous_pixel.blue == 0);
            assert!(decoder.data.previous_pixel.alpha == 255);
            assert!(decoder.data.pixel_amount == 8);
            assert!(decoder.data.run_amount == 0);
            assert!(decoder.data.partial_chunk.is_none());
        }
    }
    #[test]
    const fn bad_init_buffer_size() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let buffer = [0; 15];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_err());
        if let Err(QoiError::BadBufferSize(size)) = both {assert!(size == 15);} else {assert!(false);}
    }
    #[test]
    const fn bad_init_header_size() {
        let header_input = [113, 110, 105, 102, // incorrect magic bytes
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0,                  // colorspace (0 = sRGB with linear alpha)
                            0];                 // extra byte (incorrect)
        let buffer = [0; 16];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_err());
        if let Err(QoiError::BadHeaderSize(size)) = both {assert!(size == 15);} else {assert!(false);}
    }
    #[test]
    const fn bad_init_header() {
        let header_input = [113, 110, 105, 102, // incorrect magic bytes
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let buffer = [0; 16];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_err());
        if let Err(QoiError::InvalidMagicBytes(a, b, c, d)) = both { // TODO rename error
            assert!(a == 113);
            assert!(b == 110);
            assert!(c == 105);
            assert!(d == 102);
        } else {assert!(false);}
    }
    #[test]
    const fn good_continue_processing_end_bytes_finished() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [0, 0, 0, 0, 0, 0, 0, 1], capacity: 8, index: 0},
            output: QoiOutputBuffer {data: [0; 8], space: 8},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 0,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::Finished(output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn good_continue_processing_end_bytes_unfinished() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [0, 0, 0, 0, 0, 0, 0, 0], capacity: 7, index: 0},
            output: QoiOutputBuffer {data: [0; 8], space: 8},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 0,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn good_continue_processing_await_input() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [177, 0, 0, 0], capacity: 1, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_pixel_one_byte() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 0, 0, 0], capacity: 1, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_pixel_two_bytes() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 0, 0], capacity: 2, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_pixel_three_bytes() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 0], capacity: 3, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_pixel_four_bytes() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: None,
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_one_part() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::OneByte(255)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_some());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_one_part_add_bytes() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 0], capacity: 3, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::OneByte(255)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_two_parts() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::TwoBytes(255, 255)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::ContinueProcessing(_, output)) = progress {assert!(output.is_some());}
        else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_two_parts_add_bytes() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 0, 0], capacity: 2, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::TwoBytes(255, 255)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_three_parts() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::ThreeBytes(255, 255, 44)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::ContinueProcessing(_, output)) = progress {assert!(output.is_some());}
        else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_three_parts_add_byte() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 0, 0, 0], capacity: 1, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::ThreeBytes(255, 255, 44)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgba_four_parts() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::FourBytes(255, 255, 44, 68)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::ContinueProcessing(_, output)) = progress {assert!(output.is_some());}
        else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgb_one_part() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::OneByte(254)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::ContinueProcessing(_, output)) = progress {assert!(output.is_some());}
        else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgb_one_part_add_bytes() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 0, 0], capacity: 2, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::OneByte(254)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgb_two_parts() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::TwoBytes(254, 77)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::ContinueProcessing(_, output)) = progress {assert!(output.is_some());}
        else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgb_two_parts_add_byte() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 0, 0, 0], capacity: 1, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::TwoBytes(254, 77)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::AwaitInput(_, output)) = progress {assert!(output.is_none());} else {assert!(false);}
    }
    #[test]
    const fn infallible_process_rgb_three_parts() {
        let dec = QoiDecoder {data: QoiDecoderInternal {
            input: QoiInputBuffer {data: [255, 55, 76, 99], capacity: 4, index: 0},
            output: QoiOutputBuffer {data: [0; 4], space: 4},
            seen_pixels: [crate::consts::ZERO_PIXEL; 64],
            previous_pixel: crate::consts::DEFAULT_PIXEL,
            pixel_amount: 20,
            run_amount: 0,
            partial_chunk: Some(PartialChunk::ThreeBytes(254, 77, 88)),
        }};
        let progress = dec.continue_processing();
        assert!(progress.is_ok());
        if let Ok(QoiDecoderProgress::ContinueProcessing(_, output)) = progress {assert!(output.is_some());}
        else {assert!(false);}
    }
    #[test]
    const fn good_input_bytes_to_process_continue_processing() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let input = [254, 255, 255, 255,        // RGB chunk
                     127,                       // Diff chunk (r+1, g+1, b+1)
                     128, 55,                   // Luma chunk (r-37, g-32, b-33)
                     38,                        // Index chunk
                     255, 255, 255, 255, 255,   // RGBA chunk
                     194,                       // Run chunk (amount 3)
                     0, 0, 0, 0, 0, 0, 0, 1];   // end marker
        let buffer = [0; 24];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_ok());
        if let Ok((_, decoder)) = both {
            let progress = decoder.input_bytes_to_process(&input);
            assert!(progress.is_ok());
            if let Ok(QoiDecoderProgress::ContinueProcessing(dec, output)) = progress {
                assert!(is_identical(
                    &dec.data.input.data,
                    &[254, 255, 255, 255,      // RGB chunk
                      127,                     // Diff chunk (r+1, g+1, b+1)
                      128, 55,                 // Luma chunk (r-37, g-32, b-33)
                      38,                      // Index chunk
                      255, 255, 255, 255, 255, // RGBA chunk
                      194,                     // Run chunk (amount 3)
                      0, 0, 0, 0, 0, 0, 0, 1,  // end marker
                      0, 0]                    // empty space
                ));
                assert!(dec.data.input.capacity == 22);
                assert!(dec.data.input.index == 14);
                assert!(is_identical(
                    &dec.data.output.data,
                    &[255, 255, 255, 255, // pixel from RGB chunk
                      0, 0, 0, 255,       // pixel from Diff chunk
                      219, 224, 223, 255, // pixel from Luma chunk
                      255, 255, 255, 255, // pixel from Index chunk
                      255, 255, 255, 255, // pixel from RGBA chunk
                      255, 255, 255, 255] // 1st pixel from Run chunk
                ));
                assert!(dec.data.output.space == 24); // assumed empty after processing
                let mut index = 0;
                while index < 64 {
                    if index == 38 { // rgb/rgba chunk
                        assert!(dec.data.seen_pixels[index].red == 255);
                        assert!(dec.data.seen_pixels[index].green == 255);
                        assert!(dec.data.seen_pixels[index].blue == 255);
                        assert!(dec.data.seen_pixels[index].alpha == 255);
                    } else if index == 63 { // luma chunk
                        assert!(dec.data.seen_pixels[index].red == 219);
                        assert!(dec.data.seen_pixels[index].green == 224);
                        assert!(dec.data.seen_pixels[index].blue == 223);
                        assert!(dec.data.seen_pixels[index].alpha == 255);
                    } else if index == 53 { // diff chunk
                        assert!(dec.data.seen_pixels[index].red == 0);
                        assert!(dec.data.seen_pixels[index].green == 0);
                        assert!(dec.data.seen_pixels[index].blue == 0);
                        assert!(dec.data.seen_pixels[index].alpha == 255);
                    } else {
                        assert!(dec.data.seen_pixels[index].red == 0);
                        assert!(dec.data.seen_pixels[index].green == 0);
                        assert!(dec.data.seen_pixels[index].blue == 0);
                        assert!(dec.data.seen_pixels[index].alpha == 0);
                    }
                    index += 1;
                }
                assert!(dec.data.previous_pixel.red == 255);
                assert!(dec.data.previous_pixel.green == 255);
                assert!(dec.data.previous_pixel.blue == 255);
                assert!(dec.data.previous_pixel.alpha == 255);
                assert!(dec.data.pixel_amount == 2);
                assert!(dec.data.run_amount == 2);
                assert!(dec.data.partial_chunk.is_none());
                assert!(output.is_some());
                if let Some((bytes, amount)) = output {
                    assert!(is_identical(&bytes,&dec.data.output.data));
                    assert!(amount == 24);
                }
            } else {assert!(false);}
        }
    }
    #[test]
    const fn good_input_bytes_to_process_finish_with_output() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let input = [255, 255, 255, 255, 255,   // RGBA chunk
                     198,                       // Run chunk (amount 7)
                     0, 0, 0, 0, 0, 0, 0, 1];   // end marker
        let buffer = [0; 32];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_ok());
        if let Ok((_, decoder)) = both {
            let progress = decoder.input_bytes_to_process(&input);
            assert!(progress.is_ok());
            if let Ok(QoiDecoderProgress::Finished(output)) = progress {
                assert!(output.is_some());
                if let Some((bytes, amount)) = output {
                    assert!(is_identical(&bytes, &[255; 32])); // 8x pixels [255, 255, 255, 255]
                    assert!(amount == 32);
                }
            } else {assert!(false);}
        }
    }
    #[test]
    const fn bad_input_bytes_to_process_bad_input_size() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let input = [];
        let buffer = [0; 32];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_ok());
        if let Ok((_, decoder)) = both {
            let progress = decoder.input_bytes_to_process(&input);
            assert!(progress.is_err());
            if let Err(QoiError::BadInputSize(actual, max_expected)) = progress {
                assert!(actual == 0); // input should not be empty
                assert!(max_expected == 32);
            } else {assert!(false);}
        }
    }
    #[test]
    const fn bad_input_bytes_to_process_bad_end_marker_size() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let input = [255, 255, 255, 255, 255,   // RGBA chunk
                     198,                       // Run chunk (amount 7)
                     0, 0, 0, 0, 0, 0, 0, 1,    // end marker
                     0];                        // extra byte (incorrect)
        let buffer = [0; 32];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_ok());
        if let Ok((_, decoder)) = both {
            let progress = decoder.input_bytes_to_process(&input);
            assert!(progress.is_err());
            if let Err(QoiError::BadEndMarkerSize(size)) = progress {assert!(size == 9);} else {assert!(false);}
        }
    }
    #[test]
    const fn bad_input_bytes_to_process_bad_end_marker_bytes() {
        let header_input = [113, 111, 105, 102, // magic bytes (qoif)
                            0, 0, 0, 2,         // width (4xu8 into 1xu32 big endian: 2)
                            0, 0, 0, 4,         // height (4xu8 into 1xu32 big endian: 4)
                            4,                  // channels (4 = RGBA)
                            0];                 // colorspace (0 = sRGB with linear alpha)
        let input = [255, 255, 255, 255, 255,   // RGBA chunk
                     198,                       // Run chunk (amount 7)
                     0, 0, 0, 0, 4, 0, 0, 1];   // end marker (incorrect values)
        let buffer = [0; 32];
        let both = QoiDecoder::init(&header_input, buffer);
        assert!(both.is_ok());
        if let Ok((_, decoder)) = both {
            let progress = decoder.input_bytes_to_process(&input);
            assert!(progress.is_err());
            if let Err(QoiError::BadEndMarkerBytes(bytes)) = progress {
                assert!(is_identical(&bytes, &[0, 0, 0, 0, 4, 0, 0, 1]));
            } else {assert!(false);}
        }
    }
}
