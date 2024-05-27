# A const QOI (Quite Okay Image) decoding and encoding library
A safe, 0 dependency, no_std streaming decoder/encoder library for the QOI (Quite Okay Image) format.

## Motivation
I wanted to understand the QOI specification and implement a decoder and encoder in a const context.
Every public and private function is const including all the tests.
This project helped me to better understand bitwise operations.

## Implementation
Both the decoder and the encoder maintain an array of 64 previously seen pixels and a seperate single previous pixel value.

### Decoder
This was the easier part of the specification to figure out and implement. After parsing the 14 byte header the steps for processing the input bytes as data chunks is straightforward:
- maintain an index for the input data
- maintain a count of processed pixels to know when finished (initially calculated from header width and height)
- match on the input byte to figure out which QOI chunk tag you are dealing with (there is no overlap)
- get the extra bytes from input for chunks that are more than 1 byte (adjust input index accordingly)
- create or get RGBA pixel values depending on chunk (previous pixel run, index into previously seen pixels array, bitwise operations on diff/luma or getting the bytes from the input)

For a streaming decoder the output goes into an intermediary buffer which requires additional considerations:
- need to keep track of output buffer space
- need to handle QOI run chunks between calls to process chunks when output buffer is full

### Encoder
I found this more difficult to figure out and implement. There are some comparisons to make:
- compare the previous pixel value with the current input pixel value
- compare the current input pixel value with the pixel value in the previously seen pixels array

Based on those comparisons you select the appropriate QOI chunk to encode the pixel data into. The order is as follows:
- run chunk (if previous, current and index pixel are same)
- index chunk (if current and index pixel are same)
- diff chunk (if current RGB values are within small acceptable range compared to previous pixel and alpha the same)
- luma chunk (if current RGB values are within larger acceptable range compared to previous pixel and alpha the same)
- rgb chunk (if alpha the same and outside acceptable luma range)
- rgba chunk (if alpha is different)

Special consideration has to be taken for the very first pixel value from the input:
- previously seen pixels are all zero initialised which means all RGBA values are 0, 0, 0, 0
- the default previous pixel value has RGBA values of 0, 0, 0, 255
- using the above order when encountering a first pixel value of 0, 0, 0, 255 will issue a diff chunk instead of a run chunk because it is not already in seen pixels

## Usage
Please see https://crates.io/crates/const_qoi and the documentation section below.

## Documentation
Documentation can be found here: https://docs.rs/const_qoi

## Acknowledgements
- QOI specification: https://qoiformat.org/qoi-specification.pdf
