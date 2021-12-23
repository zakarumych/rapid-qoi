//! QOI - The "Quite OK Image" format for fast, lossless image compression
//!
//! <https://phoboslab.org>
//!
//! QOI encodes and decodes images in a lossless format. Compared to stb_image and
//! stb_image_write QOI offers 20x-50x faster encoding, 3x-4x faster decoding and
//! 20% better compression.
//!
//!
//! # Data Format
//!
//! A QOI file has a 14 byte header, followed by any number of data "chunks" and an
//! 8-byte end marker.
//!
//! ```rust
//! #[repr(C)]
//! struct QoiHeader {
//!     magic: [u8; 4], // magic bytes "qoif"
//!     width: u32,     // image width in pixels (BE)
//!     height: u32,    // image height in pixels (BE)
//!     channels: u8,   // 3 = RGB, 4 = RGBA
//!     colorspace: u8, // 0 = sRGB with linear alpha, 1 = all channels linear
//! }
//! ```
//! Images are encoded from top to bottom, left to right. The decoder and encoder
//! start with `{r: 0, g: 0, b: 0, a: 255}` as the previous pixel value. An image is
//! complete when all pixels specified by width * height have been covered.\
//! Pixels are encoded as
//!  * a run of the previous pixel
//!  * an index into an array of previously seen pixels
//!  * a difference to the previous pixel value in r,g,b
//!  * full r,g,b or r,g,b,a values
//!
//! The color channels are assumed to not be premultiplied with the alpha channel
//! ("un-premultiplied alpha").
//!
//! A running `array: [u32; 64]` (zero-initialized) of previously seen pixel values is
//! maintained by the encoder and decoder. Each pixel that is seen by the encoder
//! and decoder is put into this array at the position formed by a hash function of
//! the color value. In the encoder, if the pixel value at the index matches the
//! current pixel, this index position is written to the stream as QOI_OP_INDEX.
//!
//! The hash function for the index is:
//! ```rust,ignore
//! index_position = (r * 3 + g * 5 + b * 7 + a * 11) % 64
//! ```
//! Each chunk starts with a 2- or 8-bit tag, followed by a number of data bits. The
//! bit length of chunks is divisible by 8 - i.e. all chunks are byte aligned. All
//! values encoded in these data bits have the most significant bit on the left.\
//! The 8-bit tags have precedence over the 2-bit tags. A decoder must check for the
//! presence of an 8-bit tag first.
//!
//! The byte stream's end is marked with 7 `0x00` bytes followed a single `0x01` byte.
//! The possible chunks are:
//! ```text
//! .- QOI_OP_INDEX ----------.
//! |         Byte[0]         |
//! |  7  6  5  4  3  2  1  0 |
//! |-------+-----------------|
//! |  0  0 |     index       |
//! `-------------------------`
//! ```
//! 2-bit tag `0b00`\
//! 6-bit index into the color index array: `0..=63`\
//! A valid encoder must not issue 7 or more consecutive `QOI_OP_INDEX` chunks to the
//! index 0, to avoid confusion with the 8 byte end marker.
//! ```text
//! .- QOI_OP_DIFF -----------.
//! |         Byte[0]         |
//! |  7  6  5  4  3  2  1  0 |
//! |-------+-----+-----+-----|
//! |  0  1 |  dr |  dg |  db |
//! `-------------------------`
//! ```
//! 2-bit tag `0b01`\
//! 2-bit   red channel difference from the previous pixel between `-2..=1`\
//! 2-bit green channel difference from the previous pixel between `-2..=1`\
//! 2-bit  blue channel difference from the previous pixel between `-2..=1`\
//! The difference to the current channel values are using a wraparound operation,
//! so `1 - 2` will result in `255`, while `255 + 1` will result in `0`.\
//! Values are stored as unsigned integers with a bias of 2. E.g. `-2` is stored as
//! `0` (`0b00`). `1` is stored as `3` (`0b11`).\
//! The alpha value remains unchanged from the previous pixel.
//! ```text
//! .- QOI_OP_LUMA -------------------------------------.
//! |         Byte[0]         |         Byte[1]         |
//! |  7  6  5  4  3  2  1  0 |  7  6  5  4  3  2  1  0 |
//! |-------+-----------------+-------------+-----------|
//! |  1  0 |  green diff     |   dr - dg   |  db - dg  |
//! `---------------------------------------------------`
//! ```
//! 2-bit tag `0b10`\
//! 6-bit green channel difference from the previous pixel `-32..=31`\
//! 4-bit   red channel difference minus green channel difference `-8..=7`\
//! 4-bit  blue channel difference minus green channel difference `-8..=7`\
//! The green channel is used to indicate the general direction of change and is
//! encoded in 6 bits. The red and blue channels (dr and db) base their diffs off
//! of the green channel difference and are encoded in 4 bits. I.e.:
//! ```rust,ignore
//! dr_dg = (last_px.r - cur_px.r) - (last_px.g - cur_px.g)
//! db_dg = (last_px.b - cur_px.b) - (last_px.g - cur_px.g)
//! ```
//! The difference to the current channel values are using a wraparound operation,
//! so `10 - 13` will result in `253`, while `250 + 7` will result in `1`.\
//! Values are stored as unsigned integers with a bias of 32 for the green channel
//! and a bias of 8 for the red and blue channel.\
//! The alpha value remains unchanged from the previous pixel.
//! ```text
//! .- QOI_OP_RUN ------------.
//! |         Byte[0]         |
//! |  7  6  5  4  3  2  1  0 |
//! |-------+-----------------|
//! |  1  1 |       run       |
//! `-------------------------`
//! ```
//! 2-bit tag `0b11`\
//! 6-bit run-length repeating the previous pixel: `1..=62`\
//! The run-length is stored with a bias of -1. Note that the run-lengths 63 and 64
//! (`0b111110` and `0b111111`) are illegal as they are occupied by the `QOI_OP_RGB` and
//! `QOI_OP_RGBA` tags.
//! ```text
//! .- QOI_OP_RGB ------------------------------------------.
//! |         Byte[0]         | Byte[1] | Byte[2] | Byte[3] |
//! |  7  6  5  4  3  2  1  0 | 7 .. 0  | 7 .. 0  | 7 .. 0  |
//! |-------------------------+---------+---------+---------|
//! |  1  1  1  1  1  1  1  0 |   red   |  green  |  blue   |
//! `-------------------------------------------------------`
//! ```
//! 8-bit tag `0b11111110`\
//! 8-bit   red channel value\
//! 8-bit green channel value\
//! 8-bit  blue channel value\
//! The alpha value remains unchanged from the previous pixel.
//! ```text
//! .- QOI_OP_RGBA ---------------------------------------------------.
//! |         Byte[0]         | Byte[1] | Byte[2] | Byte[3] | Byte[4] |
//! |  7  6  5  4  3  2  1  0 | 7 .. 0  | 7 .. 0  | 7 .. 0  | 7 .. 0  |
//! |-------------------------+---------+---------+---------+---------|
//! |  1  1  1  1  1  1  1  1 |   red   |  green  |  blue   |  alpha  |
//! `-----------------------------------------------------------------`
//! ```
//! 8-bit tag `0b11111111`\
//! 8-bit   red channel value\
//! 8-bit green channel value\
//! 8-bit  blue channel value\
//! 8-bit alpha channel value
#![forbid(unsafe_code)]
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::fmt::{self, Display};

mod decode;
mod encode;

const QOI_OP_INDEX: u8 = 0x00; /* 00xxxxxx */
const QOI_OP_DIFF: u8 = 0x40; /* 01xxxxxx */
const QOI_OP_LUMA: u8 = 0x80; /* 10xxxxxx */
const QOI_OP_RUN: u8 = 0xc0; /* 11xxxxxx */
const QOI_OP_RGB: u8 = 0xfe; /* 11111110 */
const QOI_OP_RGBA: u8 = 0xff; /* 11111111 */

const QOI_MAGIC: u32 = u32::from_be_bytes(*b"qoif");
const QOI_HEADER_SIZE: usize = 14;
const QOI_PADDING: usize = 8;

trait Pixel: Copy + Eq {
    const HAS_ALPHA: bool;
    const CHANNELS: usize;

    fn new() -> Self;

    fn new_opaque() -> Self;

    fn read(&mut self, bytes: &[u8]);

    fn write(&self, bytes: &mut [u8]);

    fn var(&self, prev: &Self) -> Var;

    fn r(&self) -> u8;

    fn g(&self) -> u8;

    fn b(&self) -> u8;

    fn a(&self) -> u8;

    fn set_r(&mut self, r: u8);

    fn set_g(&mut self, g: u8);

    fn set_b(&mut self, b: u8);

    fn set_a(&mut self, a: u8);

    fn hash(&self) -> u8;
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Rgb {
    rgb: [u8; 3],
}

impl Pixel for Rgb {
    const HAS_ALPHA: bool = false;
    const CHANNELS: usize = 3;

    #[inline(always)]
    fn new() -> Self {
        Rgb { rgb: [0; 3] }
    }

    #[inline(always)]
    fn new_opaque() -> Self {
        Rgb { rgb: [0; 3] }
    }

    #[inline(always)]
    fn read(&mut self, bytes: &[u8]) {
        self.rgb.copy_from_slice(bytes);
    }

    #[inline(always)]
    fn write(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&self.rgb)
    }

    #[inline(always)]
    fn var(&self, prev: &Self) -> Var {
        let r = self.r().wrapping_sub(prev.r());
        let g = self.g().wrapping_sub(prev.g());
        let b = self.b().wrapping_sub(prev.b());

        Var { r, g, b }
    }

    #[inline(always)]
    fn r(&self) -> u8 {
        self.rgb[0]
    }

    #[inline(always)]
    fn g(&self) -> u8 {
        self.rgb[1]
    }

    #[inline(always)]
    fn b(&self) -> u8 {
        self.rgb[2]
    }

    #[inline(always)]
    fn a(&self) -> u8 {
        255
    }

    #[inline(always)]
    fn set_r(&mut self, r: u8) {
        self.rgb[0] = r;
    }

    #[inline(always)]
    fn set_g(&mut self, g: u8) {
        self.rgb[1] = g;
    }

    #[inline(always)]
    fn set_b(&mut self, b: u8) {
        self.rgb[2] = b;
    }

    #[inline(always)]
    fn set_a(&mut self, a: u8) {
        debug_assert_eq!(a, 255);
    }

    #[inline(always)]
    fn hash(&self) -> u8 {
        let [r, g, b] = self.rgb;
        r.wrapping_mul(3)
            .wrapping_add(g.wrapping_mul(5))
            .wrapping_add(b.wrapping_mul(7).wrapping_add(245))
            & 63
    }
}

#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
struct Rgba {
    rgba: [u8; 4],
}

impl Pixel for Rgba {
    const HAS_ALPHA: bool = true;
    const CHANNELS: usize = 4;

    #[inline(always)]
    fn new() -> Self {
        Rgba { rgba: [0; 4] }
    }

    #[inline(always)]
    fn new_opaque() -> Self {
        Rgba { rgba: [0, 0, 0, 1] }
    }

    #[inline(always)]
    fn read(&mut self, bytes: &[u8]) {
        self.rgba.copy_from_slice(bytes);
    }

    #[inline(always)]
    fn write(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&self.rgba)
    }

    #[inline(always)]
    fn var(&self, prev: &Self) -> Var {
        debug_assert_eq!(self.a(), prev.a());

        let r = self.r().wrapping_sub(prev.r());
        let g = self.g().wrapping_sub(prev.g());
        let b = self.b().wrapping_sub(prev.b());

        Var { r, g, b }
    }

    #[inline(always)]
    fn r(&self) -> u8 {
        self.rgba[0]
    }

    #[inline(always)]
    fn g(&self) -> u8 {
        self.rgba[1]
    }

    #[inline(always)]
    fn b(&self) -> u8 {
        self.rgba[2]
    }

    #[inline(always)]
    fn a(&self) -> u8 {
        self.rgba[3]
    }

    #[inline(always)]
    fn set_r(&mut self, r: u8) {
        self.rgba[0] = r;
    }

    #[inline(always)]
    fn set_g(&mut self, g: u8) {
        self.rgba[1] = g;
    }

    #[inline(always)]
    fn set_b(&mut self, b: u8) {
        self.rgba[2] = b;
    }

    #[inline(always)]
    fn set_a(&mut self, a: u8) {
        self.rgba[3] = a;
    }

    #[inline(always)]
    fn hash(&self) -> u8 {
        let [r, g, b, a] = self.rgba;
        r.wrapping_mul(3)
            .wrapping_add(g.wrapping_mul(5))
            .wrapping_add(b.wrapping_mul(7).wrapping_add(a.wrapping_mul(11)))
            & 63
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Var {
    r: u8,
    g: u8,
    b: u8,
}

impl Var {
    #[inline(always)]
    fn diff(&self) -> Option<u8> {
        let r = self.r.wrapping_add(2);
        let g = self.g.wrapping_add(2);
        let b = self.b.wrapping_add(2);
        if (r | g | b) & !3 == 0 {
            Some(QOI_OP_DIFF | (r << 4) as u8 | (g << 2) as u8 | b as u8)
        } else {
            None
        }
    }

    #[inline(always)]
    fn luma(&self) -> Option<[u8; 2]> {
        let r = self.r.wrapping_add(8).wrapping_sub(self.g);
        let g = self.g.wrapping_add(32);
        let b = self.b.wrapping_add(8).wrapping_sub(self.g);

        if (r | b) & 0xF0 == 0 && g & 0xC0 == 0 {
            Some([QOI_OP_LUMA | g, r << 4 | b])
        } else {
            None
        }
    }
}

/// Image color space variants.
pub enum Colors {
    /// SRGB color channels.
    Srgb,

    /// SRGB color channels and linear alpha channel.
    SrgbLinA,

    /// Lineear color channels.
    Rgb,

    /// Linear color and alpha channels.
    Rgba,
}

impl Colors {
    /// Returns `true` if color space has alpha channel.
    /// Returns `false` otherwise.
    #[inline(always)]
    pub const fn has_alpha(&self) -> bool {
        match self {
            Colors::Rgb | Colors::Srgb => false,
            Colors::Rgba | Colors::SrgbLinA => true,
        }
    }
}

/// QOI descriptor value.\
/// This value is parsed from image header during decoding.\
/// Or provided by caller to drive encoding.
pub struct Qoi {
    /// Width of the image in pixels.
    pub width: u32,

    /// Height of the image in pixels.
    pub height: u32,

    /// Specifies image color space.
    pub colors: Colors,
}

/// Errors that may occur during image encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EncodeError {
    /// Pixels buffer is too small for the image.
    NotEnoughPixelData,

    /// Output buffer is too small to fit encoded image.
    OutputIsTooSmall,
}

impl Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::NotEnoughPixelData => f.write_str("Pixels buffer is too small for image"),
            EncodeError::OutputIsTooSmall => {
                f.write_str("Output buffer is too small to fit encoded image")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EncodeError {}

/// Errros that may occur during image decoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecodeError {
    /// Buffer does not contain enough encoded data to decode the image.
    DataIsTooSmall,

    /// Encoded header contains invalid magic value.\
    /// First four bytes must contain `b"qoif"`.\
    /// This usually indicates that buffer does not contain QOI image.
    InvalidMagic,

    /// Encoded header contains invalud channels number.\
    /// QOI supports only images with `3` or `4` channels.\
    /// Any other value cannot be produced by valid encoder.
    InvalidChannelsValue,

    /// Encoded header contains invalud color space value.'
    /// QOI supports only images with SRGB color channels and linear alpha (if present) denoted by `0` and all linear channels denoted by `1`.\
    /// Any other value cannot be produced by valid encoder.
    InvalidColorSpaceValue,

    /// Output buffer is too small to fit decoded image.
    OutputIsTooSmall,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::DataIsTooSmall => {
                f.write_str("Buffer does not contain enough encoded data to decode whole image")
            }
            DecodeError::InvalidMagic => f.write_str("Encoded header contains invalid magic value"),
            DecodeError::InvalidChannelsValue => {
                f.write_str("Encoded header contains invalud channels number. Must be 3 or 4")
            }
            DecodeError::InvalidColorSpaceValue => {
                f.write_str("Encoded header contains invalud color space value. Must be 0 or 1")
            }
            DecodeError::OutputIsTooSmall => {
                f.write_str("Output buffer is too small to fit decoded image")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for DecodeError {}

#[inline(always)]
#[cold]
fn cold() {}

#[inline(always)]
fn likely(b: bool) -> bool {
    if !b {
        cold();
    }
    b
}
