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

use core::{
    convert::TryInto,
    fmt::{self, Display},
};

mod decode;
mod encode;

pub use decode::DecodeError;
pub use encode::EncodeError;

const QOI_OP_INDEX: u8 = 0x00; /* 00xxxxxx */
const QOI_OP_DIFF: u8 = 0x40; /* 01xxxxxx */
const QOI_OP_LUMA: u8 = 0x80; /* 10xxxxxx */
const QOI_OP_RUN: u8 = 0xc0; /* 11xxxxxx */
const QOI_OP_RGB: u8 = 0xfe; /* 11111110 */
const QOI_OP_RGBA: u8 = 0xff; /* 11111111 */

const QOI_MAGIC: u32 = u32::from_be_bytes(*b"qoif");
const QOI_HEADER_SIZE: usize = 14;
const QOI_PADDING: usize = 8;

/// Trait for pixel types.
/// Supports byte operations, channels accessing and modifying.
pub trait Pixel: Copy + Eq {
    const HAS_ALPHA: bool;

    fn new() -> Self;

    fn new_opaque() -> Self;

    fn read(&mut self, bytes: &[u8]);

    fn write(&self, bytes: &mut [u8]);

    fn var(&self, prev: &Self) -> Var;

    fn rgb(&self) -> [u8; 3];

    fn rgba(&self) -> [u8; 4];

    fn r(&self) -> u8;

    fn g(&self) -> u8;

    fn b(&self) -> u8;

    fn a(&self) -> u8;

    fn set_r(&mut self, r: u8);

    fn set_g(&mut self, g: u8);

    fn set_b(&mut self, b: u8);

    fn set_a(&mut self, a: u8);

    fn set_rgb(&mut self, r: u8, g: u8, b: u8);

    fn set_rgba(&mut self, r: u8, g: u8, b: u8, a: u8);

    fn add_rgb(&mut self, r: u8, g: u8, b: u8);

    fn hash(&self) -> u8;
}

/// Thee channel pixel type.
/// Typically channels are Red, Green and Blue.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub rgb: [u8; 3],
}

impl Pixel for Rgb {
    const HAS_ALPHA: bool = false;

    #[inline]
    fn new() -> Self {
        Rgb { rgb: [0; 3] }
    }

    #[inline]
    fn new_opaque() -> Self {
        Rgb { rgb: [0; 3] }
    }

    #[inline]
    fn read(&mut self, bytes: &[u8]) {
        self.rgb.copy_from_slice(bytes);
    }

    #[inline]
    fn write(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&self.rgb)
    }

    #[inline]
    fn var(&self, prev: &Self) -> Var {
        let r = self.rgb[0].wrapping_sub(prev.rgb[0]);
        let g = self.rgb[1].wrapping_sub(prev.rgb[1]);
        let b = self.rgb[2].wrapping_sub(prev.rgb[2]);

        Var { r, g, b }
    }

    #[inline]
    fn r(&self) -> u8 {
        self.rgb[0]
    }

    #[inline]
    fn g(&self) -> u8 {
        self.rgb[1]
    }

    #[inline]
    fn b(&self) -> u8 {
        self.rgb[2]
    }

    #[inline]
    fn rgb(&self) -> [u8; 3] {
        self.rgb
    }

    #[inline]
    fn rgba(&self) -> [u8; 4] {
        unreachable!()
    }

    #[inline]
    fn a(&self) -> u8 {
        255
    }

    #[inline]
    fn set_r(&mut self, r: u8) {
        self.rgb[0] = r;
    }

    #[inline]
    fn set_g(&mut self, g: u8) {
        self.rgb[1] = g;
    }

    #[inline]
    fn set_b(&mut self, b: u8) {
        self.rgb[2] = b;
    }

    #[inline]
    fn set_a(&mut self, a: u8) {
        debug_assert_eq!(a, 255);
    }

    #[inline]
    fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.rgb[0] = r;
        self.rgb[1] = g;
        self.rgb[2] = b;
    }

    #[inline]
    fn set_rgba(&mut self, r: u8, g: u8, b: u8, a: u8) {
        debug_assert_eq!(a, 255);

        self.rgb[0] = r;
        self.rgb[1] = g;
        self.rgb[2] = b;
    }

    #[inline]
    fn add_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.rgb[0] = self.rgb[0].wrapping_add(r);
        self.rgb[1] = self.rgb[1].wrapping_add(g);
        self.rgb[2] = self.rgb[2].wrapping_add(b);
    }

    #[inline]
    fn hash(&self) -> u8 {
        let [r, g, b] = self.rgb;
        r.wrapping_mul(3)
            .wrapping_add(g.wrapping_mul(5))
            .wrapping_add(b.wrapping_mul(7).wrapping_add(245))
            & 63
    }
}

/// Four channel pixel type.
/// Typically channels are Red, Green, Blue and Alpha.
#[repr(transparent)]
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    pub rgba: [u8; 4],
}

impl Pixel for Rgba {
    const HAS_ALPHA: bool = true;

    #[inline]
    fn new() -> Self {
        Rgba { rgba: [0; 4] }
    }

    #[inline]
    fn new_opaque() -> Self {
        Rgba { rgba: [0, 0, 0, 1] }
    }

    #[inline]
    fn read(&mut self, bytes: &[u8]) {
        match bytes.try_into() {
            Ok(rgba) => {
                self.rgba = rgba;
            }
            _ => unreachable(),
        }
    }

    #[inline]
    fn write(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&self.rgba)
    }

    #[inline]
    fn var(&self, prev: &Self) -> Var {
        let [r, g, b, a] = self.rgba;
        let [pr, pg, pb, pa] = prev.rgba;
        debug_assert_eq!(a, pa);

        let r = r.wrapping_sub(pr);
        let g = g.wrapping_sub(pg);
        let b = b.wrapping_sub(pb);

        Var { r, g, b }
    }

    #[inline]
    fn r(&self) -> u8 {
        self.rgba[0]
    }

    #[inline]
    fn g(&self) -> u8 {
        self.rgba[1]
    }

    #[inline]
    fn b(&self) -> u8 {
        self.rgba[2]
    }

    #[inline]
    fn rgb(&self) -> [u8; 3] {
        let [r, g, b, _] = self.rgba;
        [r, g, b]
    }

    #[inline]
    fn rgba(&self) -> [u8; 4] {
        self.rgba
    }

    #[inline]
    fn a(&self) -> u8 {
        self.rgba[3]
    }

    #[inline]
    fn set_r(&mut self, r: u8) {
        self.rgba[0] = r;
    }

    #[inline]
    fn set_g(&mut self, g: u8) {
        self.rgba[1] = g;
    }

    #[inline]
    fn set_b(&mut self, b: u8) {
        self.rgba[2] = b;
    }

    #[inline]
    fn set_a(&mut self, a: u8) {
        self.rgba[3] = a;
    }

    #[inline]
    fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.rgba = [r, g, b, self.rgba[3]];
    }

    #[inline]
    fn set_rgba(&mut self, r: u8, g: u8, b: u8, a: u8) {
        self.rgba = [r, g, b, a];
    }

    #[inline]
    fn add_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.rgba[0] = self.rgba[0].wrapping_add(r);
        self.rgba[1] = self.rgba[1].wrapping_add(g);
        self.rgba[2] = self.rgba[2].wrapping_add(b);
    }

    #[inline]
    fn hash(&self) -> u8 {
        let [r, g, b, a] = self.rgba;
        r.wrapping_mul(3)
            .wrapping_add(g.wrapping_mul(5))
            .wrapping_add(b.wrapping_mul(7).wrapping_add(a.wrapping_mul(11)))
            & 63
    }
}

/// Color variance value.
/// Wrapping difference between two pixels.
#[derive(Clone, Copy)]
#[repr(C)]
pub struct Var {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Var {
    #[inline]
    fn diff(&self) -> Option<u8> {
        let r = self.r.wrapping_add(2);
        let g = self.g.wrapping_add(2);
        let b = self.b.wrapping_add(2);

        match r | g | b {
            0x00..=0x03 => Some(QOI_OP_DIFF | (r << 4) as u8 | (g << 2) as u8 | b as u8),
            _ => None,
        }
    }

    #[inline]
    fn luma(&self) -> Option<[u8; 2]> {
        let r = self.r.wrapping_add(8).wrapping_sub(self.g);
        let g = self.g.wrapping_add(32);
        let b = self.b.wrapping_add(8).wrapping_sub(self.g);

        match (r | b, g) {
            (0x00..=0x0F, 0x00..=0x3F) => Some([QOI_OP_LUMA | g, r << 4 | b]),
            _ => None,
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
    #[inline]
    pub const fn has_alpha(&self) -> bool {
        match self {
            Colors::Rgb | Colors::Srgb => false,
            Colors::Rgba | Colors::SrgbLinA => true,
        }
    }

    /// Returns `true` if color space has alpha channel.
    /// Returns `false` otherwise.
    #[inline]
    pub const fn channels(&self) -> usize {
        match self {
            Colors::Rgb | Colors::Srgb => 3,
            Colors::Rgba | Colors::SrgbLinA => 4,
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

#[inline]
#[cold]
fn cold() {}

#[inline]
fn likely(b: bool) -> bool {
    if !b {
        cold();
    }
    b
}

/// Next best thing after `core::hint::unreachable_unchecked()`
/// If happens to be called this will stall CPU, instead of causing UB.
#[inline]
#[cold]
fn unreachable() -> ! {
    loop {}
}
