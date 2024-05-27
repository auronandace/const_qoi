pub const MAGIC_BYTES: [u8; 4] = [b'q', b'o', b'i', b'f'];
pub const END_MARKER: [u8; 8] = [0, 0, 0, 0, 0, 0, 0, 1];
pub const DEFAULT_PIXEL: crate::pixel::Pixel = crate::pixel::Pixel::new(0, 0, 0, 255);
pub const ZERO_PIXEL: crate::pixel::Pixel = crate::pixel::Pixel::new(0, 0, 0, 0);
