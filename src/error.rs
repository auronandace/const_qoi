/// The possible errors when decoding or encoding QOI files.
#[allow(clippy::module_name_repetitions)]
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum QoiError {
    /// The width or height of the header is `0`. Shows the encountered values.
    InvalidWidthHeight(u32, u32), // TODO rename
    /// The provided output buffer is too small for the encoder. Shows the size in bytes.
    BufferTooSmall(usize),
    /// The magic bytes of the header are incorrect. Correct value is: "qoif" ([`113`, `111`, `105`, `102`]). Shows the encountered values.
    InvalidMagicBytes(u8, u8, u8, u8), // TODO rename
    /// The channels value of the header is incorrect. Correct values are: `3` (RGB) or `4` (RGBA). Shows the encountered value.
    InvalidChannelsValue(u8), // TODO rename
    /// The colorspace value of the header is incorrect. Correct values are: `0` (sRGB with linear alpha) or `1` (all channels linear). Shows the encountered value.
    InvalidColorspaceValue(u8), // TODO rename
    /// The specified width and height do not match the input pixel data. Shows specified width and height and actual pixel amount.
    InputHeaderMismatch(u32, u32, u64),
    /// The input data is not divisible by specified channels. Shows total size of input data in bytes and specified channels.
    IncorrectInputData(usize, u8),
    Refactor, // TODO placeholder for refactoring errors
    /// The buffer size is less than `16` or is not divisible by `4`. Shows the size of buffer.
    BadBufferSize(usize),
    /// The input for the header is not `14` bytes. Shows the size of input.
    BadHeaderSize(usize),
    /// The input is empty or too large. Shows input size and maximum accepted input size (specified at creation).
    BadInputSize(usize, usize),
    /// The amount of bytes from the current input that should constitute the end marker are too many. Shows the amount.
    BadEndMarkerSize(usize),
    /// The bytes for the end marker are incorrect. Shows the detected bytes.
    BadEndMarkerBytes([u8; 8]),
}

#[allow(clippy::many_single_char_names)]
impl core::fmt::Display for QoiError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::InvalidWidthHeight(w, h) => write!(f, "Width or height cannot be 0: detected {w} width and {h} height"),
            Self::BufferTooSmall(size) => write!(f, "Output buffer size for encoder must be at least 5 bytes, detected {size} bytes"),
            Self::InvalidMagicBytes(a, b, c, d) => write!(f, "Invalid magic bytes: {a}, {b}, {c}, {d}"),
            Self::InvalidChannelsValue(v) => write!(f, "Invalid channels value: {v}"),
            Self::InvalidColorspaceValue(v) => write!(f, "Invalid colorspace value: {v}"),
            Self::InputHeaderMismatch(w, h, i) => write!(f, "Specified {w} width and {h} height but input contains {i} pixels."),
            Self::IncorrectInputData(size, channels) => write!(f, "Malformed input: input data of {size} bytes detected which cannot represent {channels} byte pixels"),
            Self::Refactor => write!(f, "Not implemented!"), // FIXME
            Self::BadBufferSize(size) => write!(f, "Buffer size must be 16 bytes or greater and divisible by 4, detected buffer size of {size} bytes"),
            Self::BadHeaderSize(size) => write!(f, "Header size must be 14 bytes, detected header input of {size} bytes"),
            Self::BadInputSize(input, max) => write!(f, "Input cannot be empty or exceed initial buffer size ({max}), detected input size of {input} bytes"),
            Self::BadEndMarkerSize(size) => write!(f, "Wrong amount of bytes for end marker, detected {size} bytes"),
            Self::BadEndMarkerBytes(end) => write!(f, "Wrong bytes for end marker, detected: {}, {}, {}, {}, {}, {}, {}, {}", end[0], end[1], end[2], end[3], end[4], end[5], end[6], end[7]),
        }
    }
}
