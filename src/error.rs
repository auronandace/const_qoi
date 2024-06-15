/// The possible errors when decoding or encoding QOI files.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QoiError {
    /// The input slice of bytes is too small to be a valid QOI image for decoding. Shows the size in bytes.
    InputTooSmall(usize),
    /// The width or height of the header is `0`. Shows the encountered values.
    InvalidWidthHeight(u32, u32),
    /// The provided output buffer is incorrectly sized for the decoder. Shows the size in bytes.
    IncorrectBufferSize(usize),
    /// The provided output buffer is too small for the encoder. Shows the size in bytes.
    BufferTooSmall(usize),
    /// The magic bytes of the header are incorrect. Correct value is: "qoif" ([`113`, `111`, `105`, `102`]). Shows the encountered values.
    InvalidMagicBytes(u8, u8, u8, u8), // TODO rename
    /// The channels value of the header is incorrect. Correct values are: `3` (RGB) or `4` (RGBA). Shows the encountered value.
    InvalidChannelsValue(u8),
    /// The colorspace value of the header is incorrect. Correct values are: `0` (sRGB with linear alpha) or `1` (all channels linear). Shows the encountered value.
    InvalidColorspaceValue(u8),
    /// The `8` byte end marker is incorrect. Correct values are: `0`, `0`, `0`, `0`, `0`, `0`, `0`, `1`. Shows the encountered bytes.
    InvalidEndMarker(u8, u8, u8, u8, u8, u8, u8, u8),
    /// The input slice of bytes is missing required bytes in the last chunk. Shows last `5` bytes before the `8` byte end marker and the amount of end marker bytes misinterpereted as chunk data bytes.
    EndAsChunksFinished([u8; 5], usize),
    /// The input slice of bytes is missing required bytes in the last chunk and more chunks are expected. Shows the amount of chunks missing, the last `5` bytes before the `8` byte end marker and the amount of end marker bytes misinterpereted as chunk data bytes.
    EndAsChunksUnfinished(u64, [u8; 5], usize),
    /// The header didn't specify enough pixels. The input slice of bytes contains more chunks to process. Shows expected pixels and amount of bytes left to process as chunks before `8` byte end marker.
    MoreDataBeforeEnd(u64, usize),
    /// The header specified too many pixels. The input slice of bytes doesn't contain enough chunks to process. Shows expected pixels and processed pixels.
    IncorrectPixelAmount(u64, u64),
    /// The specified width and height do not match the input pixel data. Shows specified width and height and actual pixel amount.
    InputHeaderMismatch(u32, u32, u64),
    /// The input data is not divisible by specified channels. Shows total size of input data in bytes and specified channels.
    IncorrectInputData(usize, u8),
    //Refactor, // TODO placeholder for refactoring errors
    BadBufferSize(usize),
    BadHeaderSize(usize),
    BadInputSize(usize, usize),
    BadEndMarkerSize(usize),
    BadEndMarkerBytes([u8; 8]),
}

#[allow(clippy::many_single_char_names)]
impl core::fmt::Display for QoiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InputTooSmall(amount) => write!(f, "Insufficient input: must be more than 22 bytes, detected {amount} bytes"),
            Self::InvalidWidthHeight(w, h) => write!(f, "Width or height cannot be 0: detected {w} width and {h} height"),
            Self::IncorrectBufferSize(size) => write!(f, "Output buffer size for decoder must be divisible by 4, detected buffer size of {size} bytes"),
            Self::BufferTooSmall(size) => write!(f, "Output buffer size for encoder must be at least 5 bytes, detected {size} bytes"),
            Self::InvalidMagicBytes(a, b, c, d) => write!(f, "Invalid magic bytes: {a}, {b}, {c}, {d}"),
            Self::InvalidChannelsValue(v) => write!(f, "Invalid channels value: {v}"),
            Self::InvalidColorspaceValue(v) => write!(f, "Invalid colorspace value: {v}"),
            Self::InvalidEndMarker(a, b, c, d, e, g, h, i) => write!(f, "Invalid end marker: {a}, {b}, {c}, {d}, {e}, {g}, {h}, {i}"),
            Self::EndAsChunksFinished(l, amount) => write!(f, "Malformed input: the final chunk is incomplete and has used {amount} end marker bytes as chunk data, last five bytes before end marker: {}, {}, {}, {}, {}", l[0], l[1], l[2], l[3], l[4]),
            Self::EndAsChunksUnfinished(missing, l, amount) => write!(f, "Malformed input: {missing} more chunks are expected to complete the pixel data, the final chunk is also incomplete and has used {amount} end marker bytes as chunk data, last five bytes before end marker: {}, {}, {}, {}, {}", l[0], l[1], l[2], l[3], l[4]),
            Self::MoreDataBeforeEnd(h, cbl) => write!(f, "Malformed input: header specified {h} pixels but found {cbl} chunk bytes left to process from input before 8 byte end marker"),
            Self::IncorrectPixelAmount(h, a) => write!(f, "Malformed input: header specified {h} pixels but only encountered {a} pixels"),
            Self::InputHeaderMismatch(w, h, i) => write!(f, "Specified {w} width and {h} height but input contains {i} pixels."),
            Self::IncorrectInputData(size, channels) => write!(f, "Malformed input: input data of {size} bytes detected which cannot represent {channels} byte pixels"),
            //Self::Refactor => write!(f, "Not implemented!"), // FIXME
            Self::BadBufferSize(size) => write!(f, "Buffer size must be 16 bytes or greater and divisible by 4, detected buffer size of {size} bytes"),
            Self::BadHeaderSize(size) => write!(f, "Header size must be 14 bytes, detected header input of {size} bytes"),
            Self::BadInputSize(input, max) => write!(f, "Input cannot be empty or exceed initial buffer size ({max}), detected input size of {input} bytes"),
            Self::BadEndMarkerSize(size) => write!(f, "Wrong amount of bytes for end marker, detected {size} bytes"),
            Self::BadEndMarkerBytes(end) => write!(f, "Wrong bytes for end marker, detected: {}, {}, {}, {}, {}, {}, {}, {}", end[0], end[1], end[2], end[3], end[4], end[5], end[6], end[7]),
        }
    }
}
