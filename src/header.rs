use crate::{
    consts::MAGIC_BYTES,
    error::QoiError,
    utils::{array_from_input, is_identical}
};

/// The header data of a QOI image.
#[allow(clippy::module_name_repetitions)]
#[derive(Clone, Copy)]
pub struct QoiHeader {
    data: QoiHeaderInternal,
}

impl QoiHeader {
    /// The magic bytes of a QOI image. They should always be "qoif" ([`113`, `111`, `105`, `102`]).
    #[must_use]
    pub const fn magic_bytes(&self) -> [u8; 4] {
        self.data.magic_bytes
    }
    /// The width of a QOI image in pixels.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.data.width
    }
    /// The height of a QOI image in pixels.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.data.height
    }
    /// The channels of a QOI image. Valid values: `3` (RGB) or `4` (RGBA).
    #[must_use]
    pub const fn channels(&self) -> u8 {
        self.data.channels
    }
    /// The colorspace of a QOI image. Valid values: `0` (sRGB with linear alpha) or `1` (all channels linear).
    #[must_use]
    pub const fn colorspace(&self) -> u8 {
        self.data.colorspace
    }
    /// Convert the header to an array of bytes.
    ///
    /// A convenience method for extracting all the bytes from the header.
    #[must_use]
    pub const fn to_u8(self) -> [u8; 14] {
        let mut output = [0; 14];
        let magic = self.magic_bytes();
        output[0] = magic[0];
        output[1] = magic[1];
        output[2] = magic[2];
        output[3] = magic[3];
        let width = self.width().to_be_bytes();
        output[4] = width[0];
        output[5] = width[1];
        output[6] = width[2];
        output[7] = width[3];
        let height = self.height().to_be_bytes();
        output[8] = height[0];
        output[9] = height[1];
        output[10] = height[2];
        output[11] = height[3];
        output[12] = self.channels();
        output[13] = self.colorspace();
        output
    }
}

#[derive(Clone, Copy)]
pub struct QoiHeaderInternal {
    pub magic_bytes: [u8; 4],
    pub width: u32,
    pub height: u32,
    pub channels: u8,
    pub colorspace: u8,
}

impl QoiHeaderInternal {
    pub const fn new(width: u32, height: u32, channels: u8, colorspace: u8) -> Self {
        Self {magic_bytes: MAGIC_BYTES, width, height, channels, colorspace}
    }
    pub const fn extract(input: &[u8]) -> Result<Self, QoiError> {
        let magic_bytes: [u8; 4] = array_from_input(input, 0);
        if !is_identical(&magic_bytes, &MAGIC_BYTES) {
            return Err(QoiError::InvalidMagicBytes(magic_bytes[0], magic_bytes[1], magic_bytes[2], magic_bytes[3]));
        }
        let width: [u8; 4] = array_from_input(input, 4);
        let width = u32::from_be_bytes(width);
        let height: [u8; 4] = array_from_input(input, 8);
        let height = u32::from_be_bytes(height);
        if width == 0 || height == 0 {return Err(QoiError::InvalidWidthHeight(width, height));}
        let channels = input[12];
        if channels != 3 && channels != 4 {return Err(QoiError::InvalidChannelsValue(channels));}
        let colorspace = input[13];
        if colorspace != 0 && colorspace != 1 {return Err(QoiError::InvalidColorspaceValue(colorspace));}
        Ok(Self {magic_bytes, width, height, channels, colorspace})
    }
    pub const fn public(self) -> QoiHeader {
        QoiHeader {data: self}
    }
}

#[cfg(test)]
mod tests {
    use crate::{error::QoiError, utils::is_identical};
    use super::{QoiHeader, QoiHeaderInternal};
    #[test]
    const fn infallible_new() {
        let (width, height, channels, colorspace) = (2, 2, 4, 0);
        let header = QoiHeaderInternal::new(width, height, channels, colorspace);
        assert!(is_identical(&header.magic_bytes, &[113, 111, 105, 102]));
        assert!(header.width == 2);
        assert!(header.height == 2);
        assert!(header.channels == 4);
        assert!(header.colorspace == 0);
    }
    #[test]
    const fn infallible_to_u8() {
        let input = [113, 111, 105, 102,      // magic bytes
                     0, 0, 0, 2,              // width
                     0, 0, 0, 4,              // height
                     4,                       // channels
                     0];                      // colorspace
        let internal = QoiHeaderInternal::extract(&input);
        assert!(internal.is_ok());
        if let Ok(internal) = internal {
            assert!(is_identical(&internal.magic_bytes, &[113, 111, 105, 102]));
            assert!(internal.width == 2);
            assert!(internal.height == 4);
            assert!(internal.channels == 4);
            assert!(internal.colorspace == 0);
            let header = QoiHeader{data: internal};
            assert!(is_identical(&header.to_u8(), &[113, 111, 105, 102, // magic bytes
                                                    0, 0, 0, 2,         // width
                                                    0, 0, 0, 4,         // height
                                                    4,                  // channels
                                                    0])                 // colorspace
            );
        }
    }
    #[test]
    const fn good_extract() {
        let input = [113, 111, 105, 102,      // magic bytes
                     0, 0, 0, 2,              // width
                     0, 0, 0, 4,              // height
                     4,                       // channels
                     0];                      // colorspace
        let header = QoiHeaderInternal::extract(&input);
        assert!(header.is_ok());
        if let Ok(header) = header {
            assert!(is_identical(&header.magic_bytes, &[113, 111, 105, 102]));
            assert!(header.width == 2);
            assert!(header.height == 4);
            assert!(header.channels == 4);
            assert!(header.colorspace == 0);
        }
    }
    #[test]
    const fn bad_magic_bytes() {
        let input = [112, 111, 105, 102,      // magic bytes (incorrect)
                     0, 0, 0, 2,              // width
                     0, 0, 0, 4,              // height
                     4,                       // channels
                     0];                      // colorspace
        let header = QoiHeaderInternal::extract(&input);
        assert!(header.is_err());
        if let Err(QoiError::InvalidMagicBytes(q, o, i, f)) = header {
            assert!(q == 112);
            assert!(o == 111);
            assert!(i == 105);
            assert!(f == 102);
        }
    }
    #[test]
    const fn bad_width_height() {
        let input = [113, 111, 105, 102,      // magic bytes
                     0, 0, 0, 0,              // width (incorrect)
                     0, 0, 0, 0,              // height (incorrect)
                     4,                       // channels
                     0];                      // colorspace
        let header = QoiHeaderInternal::extract(&input);
        assert!(header.is_err());
        if let Err(QoiError::InvalidWidthHeight(width, height)) = header {
            assert!(width == 0);
            assert!(height == 0);
        }
    }
    #[test]
    const fn bad_channels() {
        let input = [113, 111, 105, 102,      // magic bytes
                     0, 0, 0, 2,              // width
                     0, 0, 0, 4,              // height
                     9,                       // channels (incorrect)
                     0];                      // colorspace
        let header = QoiHeaderInternal::extract(&input);
        assert!(header.is_err());
        if let Err(QoiError::InvalidChannelsValue(channels)) = header {
            assert!(channels == 9);
        }
    }
    #[test]
    const fn bad_colorspace() {
        let input = [113, 111, 105, 102,      // magic bytes
                     0, 0, 0, 2,              // width
                     0, 0, 0, 4,              // height
                     4,                       // channels
                     9];                      // colorspace (incorrect)
        let header = QoiHeaderInternal::extract(&input);
        assert!(header.is_err());
        if let Err(QoiError::InvalidColorspaceValue(colorspace)) = header {
            assert!(colorspace == 9);
        }
    }
}
