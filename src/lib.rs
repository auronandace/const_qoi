//! # A const QOI (Quite Okay Image) decoding and encoding library
//!
//! This crate provides the building blocks for decoding and encoding QOI formatted images.
//!
//! This is a safe `#![no_std]` crate that does not require [alloc] and has no dependencies.
//!
//! ## Motivation
//!
//! I wanted to understand the [QOI specification] and implement a decoder and encoder in a [const context].
//! Every public and private function is const including all the tests.
//! This project helped me to better understand bitwise operations.
//!
//! ## Usage
//!
//! You need to provide a buffer and use a loop to drive forward the decoding/encoding process.\
//! Your buffer will be consumed and returned so you can retrieve the bytes from it.\
//! When decoding your returned buffer will contain bytes that represent RGBA pixel data.\
//! When encoding your returned buffer will contain bytes that represent QOI data chunks.
//!
//! ### Decoding
//!
//! Below is an example of a simple decoder.
//!
//! Be careful using the width and height values from the header when calculating the pixel amount.
//! The [QOI specification] states they are stored as unsigned 32bit integers in the header.
//! This makes the maximum size of a QOI image in pixels [`u32::MAX`] multiplied by [`u32::MAX`].
//! The result of that calculation will safely fit within a [`u64`].
//! Also keep in mind that casting values to [`usize`] may cause truncation depending on the target architecture.
//! You should always perform proper error handling when converting or casting between integer types.
//!
//! ```
//! let (mut decoder, header) = QoiDecoder::init(input)?;
//! if let Some(pixel_amount) = (header.width() as usize).checked_mul(header.height() as usize) {
//!     // 1 pixel is 4 bytes (red, green, blue, alpha)
//!     let mut output = Vec::with_capacity(pixel_amount * 4); // usize may truncate
//!     loop {
//!         match decoder.process_chunks(input, [0; 1024])? {
//!             QoiDecoderProgress::Unfinished((dec, buffer)) => {
//!                 decoder = dec;
//!                 buffer
//!                     .into_iter()
//!                     .for_each(|byte| output.push(byte));
//!             },
//!             QoiDecoderProgress::Finished((buffer, empty)) => {
//!                 buffer
//!                     .into_iter()
//!                     .take(buffer.len() - empty) // output buffer may not be full
//!                     .for_each(|byte| output.push(byte));
//!                 break;
//!             },
//!         }
//!     }
//!     // output is now filled with 4 byte pixel (RGBA) values
//! }
//! ```
//!
//! ### Encoding
//!
//! Below is an example of a simple encoder.
//!
//! ```
//! let (mut encoder, header) = QoiEncoder::new(input, width, height, channels, colorspace)?;
//! let mut output = Vec::new();
//! header.to_u8().into_iter().for_each(|byte| output.push(byte)); // adding 14 byte header
//! loop {
//!     match encoder.process_pixels(input, [0; 1000])? {
//!         QoiEncoderProgress::Unfinished(enc, buffer, empty) => {
//!             encoder = enc;
//!             buffer
//!                 .into_iter()
//!                 .take(buffer.len() - empty)
//!                 .for_each(|byte| output.push(byte));
//!         },
//!         QoiEncoderProgress::Finished(buffer, empty) => {
//!             buffer
//!                 .into_iter()
//!                 .take(buffer.len() - empty)
//!                 .for_each(|byte| output.push(byte));
//!             [0, 0, 0, 0, 0, 0, 0, 1]
//!                 .into_iter()
//!                 .for_each(|byte| output.push(byte)); // adding 8 byte end marker
//!             break;
//!         },
//!     }
//! }
//! // output is now a valid QOI image ready to be written to a file
//! ```
//!
//! [alloc]: <https://doc.rust-lang.org/alloc/index.html>
//! [const context]: <https://doc.rust-lang.org/reference/const_eval.html>
//! [QOI specification]: <https://qoiformat.org/qoi-specification.pdf>
#![no_std]
#![forbid(unsafe_code)]

mod buffer;
mod consts;
mod decoder;
mod encoder;
mod error;
mod header;
mod pixel;
mod utils;

pub use crate::decoder::{QoiDecoder, QoiDecoderProgress};
pub use crate::encoder::{QoiEncoder, QoiEncoderProgress};
pub use crate::error::QoiError;
pub use crate::header::QoiHeader;
