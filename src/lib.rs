//! QOI - The "Quite OK Image" format for fast, lossless image compression
//!
//! https://phoboslab.org
//!
//! QOI encodes and decodes images in a lossless format. Compared to stb_image and
//! stb_image_write QOI offers 20x-50x faster encoding, 3x-4x faster decoding and
//! 20% better compression.
//!
//!
//! -- Data Format
//! A QOI file has a 14 byte header, followed by any number of data "chunks" and an
//! 8-byte end marker.
//! struct qoi_header_t {
//!     char     magic[4];   // magic bytes "qoif"
//!     uint32_t width;      // image width in pixels (BE)
//!     uint32_t height;     // image height in pixels (BE)
//!     uint8_t  channels;   // 3 = RGB, 4 = RGBA
//!     uint8_t  colorspace; // 0 = sRGB with linear alpha, 1 = all channels linear
//! };
//! Images are encoded from top to bottom, left to right. The decoder and encoder
//! start with {r: 0, g: 0, b: 0, a: 255} as the previous pixel value. An image is
//! complete when all pixels specified by width * height have been covered.
//! Pixels are encoded as
//!  - a run of the previous pixel
//!  - an index into an array of previously seen pixels
//!  - a difference to the previous pixel value in r,g,b
//!  - full r,g,b or r,g,b,a values
//! The color channels are assumed to not be premultiplied with the alpha channel
//! ("un-premultiplied alpha").
//! A running array[64] (zero-initialized) of previously seen pixel values is
//! maintained by the encoder and decoder. Each pixel that is seen by the encoder
//! and decoder is put into this array at the position formed by a hash function of
//! the color value. In the encoder, if the pixel value at the index matches the
//! current pixel, this index position is written to the stream as QOI_OP_INDEX.
//! The hash function for the index is:
//!     index_position = (r * 3 + g * 5 + b * 7 + a * 11) % 64
//! Each chunk starts with a 2- or 8-bit tag, followed by a number of data bits. The
//! bit length of chunks is divisible by 8 - i.e. all chunks are byte aligned. All
//! values encoded in these data bits have the most significant bit on the left.
//! The 8-bit tags have precedence over the 2-bit tags. A decoder must check for the
//! presence of an 8-bit tag first.
//! The byte stream's end is marked with 7 0x00 bytes followed a single 0x01 byte.
//! The possible chunks are:
//! .- QOI_OP_INDEX ----------.
//! |         Byte[0]         |
//! |  7  6  5  4  3  2  1  0 |
//! |-------+-----------------|
//! |  0  0 |     index       |
//! `-------------------------`
//! 2-bit tag b00
//! 6-bit index into the color index array: 0..63
//! A valid encoder must not issue 7 or more consecutive QOI_OP_INDEX chunks to the
//! index 0, to avoid confusion with the 8 byte end marker.
//! .- QOI_OP_DIFF -----------.
//! |         Byte[0]         |
//! |  7  6  5  4  3  2  1  0 |
//! |-------+-----+-----+-----|
//! |  0  1 |  dr |  dg |  db |
//! `-------------------------`
//! 2-bit tag b01
//! 2-bit   red channel difference from the previous pixel between -2..1
//! 2-bit green channel difference from the previous pixel between -2..1
//! 2-bit  blue channel difference from the previous pixel between -2..1
//! The difference to the current channel values are using a wraparound operation,
//! so "1 - 2" will result in 255, while "255 + 1" will result in 0.
//! Values are stored as unsigned integers with a bias of 2. E.g. -2 is stored as
//! 0 (b00). 1 is stored as 3 (b11).
//! The alpha value remains unchanged from the previous pixel.
//! .- QOI_OP_LUMA -------------------------------------.
//! |         Byte[0]         |         Byte[1]         |
//! |  7  6  5  4  3  2  1  0 |  7  6  5  4  3  2  1  0 |
//! |-------+-----------------+-------------+-----------|
//! |  1  0 |  green diff     |   dr - dg   |  db - dg  |
//! `---------------------------------------------------`
//! 2-bit tag b10
//! 6-bit green channel difference from the previous pixel -32..31
//! 4-bit   red channel difference minus green channel difference -8..7
//! 4-bit  blue channel difference minus green channel difference -8..7
//! The green channel is used to indicate the general direction of change and is
//! encoded in 6 bits. The red and blue channels (dr and db) base their diffs off
//! of the green channel difference and are encoded in 4 bits. I.e.:
//!     dr_dg = (last_px.r - cur_px.r) - (last_px.g - cur_px.g)
//!     db_dg = (last_px.b - cur_px.b) - (last_px.g - cur_px.g)
//! The difference to the current channel values are using a wraparound operation,
//! so "10 - 13" will result in 253, while "250 + 7" will result in 1.
//! Values are stored as unsigned integers with a bias of 32 for the green channel
//! and a bias of 8 for the red and blue channel.
//! The alpha value remains unchanged from the previous pixel.
//! .- QOI_OP_RUN ------------.
//! |         Byte[0]         |
//! |  7  6  5  4  3  2  1  0 |
//! |-------+-----------------|
//! |  1  1 |       run       |
//! `-------------------------`
//! 2-bit tag b11
//! 6-bit run-length repeating the previous pixel: 1..62
//! The run-length is stored with a bias of -1. Note that the run-lengths 63 and 64
//! (b111110 and b111111) are illegal as they are occupied by the QOI_OP_RGB and
//! QOI_OP_RGBA tags.
//! .- QOI_OP_RGB ------------------------------------------.
//! |         Byte[0]         | Byte[1] | Byte[2] | Byte[3] |
//! |  7  6  5  4  3  2  1  0 | 7 .. 0  | 7 .. 0  | 7 .. 0  |
//! |-------------------------+---------+---------+---------|
//! |  1  1  1  1  1  1  1  0 |   red   |  green  |  blue   |
//! `-------------------------------------------------------`
//! 8-bit tag b11111110
//! 8-bit   red channel value
//! 8-bit green channel value
//! 8-bit  blue channel value
//! The alpha value remains unchanged from the previous pixel.
//! .- QOI_OP_RGBA ---------------------------------------------------.
//! |         Byte[0]         | Byte[1] | Byte[2] | Byte[3] | Byte[4] |
//! |  7  6  5  4  3  2  1  0 | 7 .. 0  | 7 .. 0  | 7 .. 0  | 7 .. 0  |
//! |-------------------------+---------+---------+---------+---------|
//! |  1  1  1  1  1  1  1  1 |   red   |  green  |  blue   |  alpha  |
//! `-----------------------------------------------------------------`
//! 8-bit tag b11111111
//! 8-bit   red channel value
//! 8-bit green channel value
//! 8-bit  blue channel value
//! 8-bit alpha channel value
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::{
    fmt::{self, Display},
    mem::transmute,
    ptr::copy_nonoverlapping,
};

mod decode;
mod encode;

const QOI_OP_INDEX: u8 = 0x00; /* 00xxxxxx */
const QOI_OP_DIFF: u8 = 0x40; /* 01xxxxxx */
const QOI_OP_LUMA: u8 = 0x80; /* 10xxxxxx */
const QOI_OP_RUN: u8 = 0xc0; /* 11xxxxxx */
const QOI_OP_RGB: u8 = 0xfe; /* 11111110 */
const QOI_OP_RGBA: u8 = 0xff; /* 11111111 */

#[inline(always)]
const fn qui_color_hash(c: Rgba) -> usize {
    (c.r.wrapping_mul(3)
        .wrapping_add(c.g.wrapping_mul(5))
        .wrapping_add(c.b.wrapping_mul(7).wrapping_add(c.a.wrapping_mul(11)))
        & 63) as usize
}

const QOI_MAGIC: u32 = u32::from_be_bytes(*b"qoif");
const QOI_HEADER_SIZE: usize = 14;
const QOI_PADDING: usize = 8;

struct UnsafeWriter {
    ptr: *mut u8,
    len: usize,
}

impl UnsafeWriter {
    #[inline(always)]
    unsafe fn pad(&mut self) {
        copy_nonoverlapping([0, 0, 0, 0, 0, 0, 0, 1].as_ptr(), self.ptr, 8);
        self.ptr = self.ptr.add(8);
        self.len -= 8;
    }

    #[inline(always)]
    unsafe fn write_8(&mut self, v: u8) {
        *self.ptr = v;
        self.ptr = self.ptr.add(1);
        self.len -= 1;
    }

    #[inline(always)]
    unsafe fn write_16(&mut self, v: u16) {
        copy_nonoverlapping(v.to_be_bytes().as_ptr(), self.ptr, 2);
        self.ptr = self.ptr.add(2);
        self.len -= 2;
    }

    #[inline(always)]
    unsafe fn write_32(&mut self, v: u32) {
        copy_nonoverlapping(v.to_be_bytes().as_ptr(), self.ptr, 4);
        self.ptr = self.ptr.add(4);
        self.len -= 4;
    }
}

struct UnsafeReader {
    ptr: *const u8,
    len: usize,
}

impl UnsafeReader {
    #[inline(always)]
    unsafe fn read_8(&mut self) -> u8 {
        let v = *self.ptr;
        self.ptr = self.ptr.add(1);
        self.len -= 1;
        v
    }

    #[inline(always)]
    unsafe fn read_32(&mut self) -> u32 {
        let v = u32::from_be_bytes(*(self.ptr as *const [u8; 4]));
        self.ptr = self.ptr.add(4);
        self.len -= 4;
        v
    }

    #[inline(always)]
    unsafe fn read_rgb(&mut self, c: &mut Rgba) {
        copy_nonoverlapping(self.ptr, c as *mut _ as *mut u8, 3);
        self.ptr = self.ptr.add(3);
        self.len -= 3;
    }

    #[inline(always)]
    unsafe fn read_rgba(&mut self, c: &mut Rgba) {
        copy_nonoverlapping(self.ptr, c as *mut _ as *mut u8, 4);
        self.ptr = self.ptr.add(4);
        self.len -= 4;
    }
}

#[repr(C)]
#[derive(Clone, Copy, Eq)]
struct Rgba {
    r: u8,
    g: u8,
    b: u8,
    a: u8,
}

impl PartialEq for Rgba {
    #[inline(always)]
    fn eq(&self, other: &Self) -> bool {
        self.u32() == other.u32()
    }
}

impl Rgba {
    #[inline(always)]
    const fn u32(&self) -> u32 {
        unsafe { transmute(*self) }
    }

    #[inline(always)]
    const fn new() -> Self {
        Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        }
    }

    #[inline(always)]
    const fn new_opaque() -> Self {
        Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 255,
        }
    }

    #[inline(always)]
    fn read_rgba(bytes: &[u8]) -> Self {
        let mut v = [0; 4];
        v.copy_from_slice(&bytes[..4]);
        unsafe { transmute(v) }
    }

    #[inline(always)]
    fn read_rgb(bytes: &[u8], a: u8) -> Self {
        let mut v = [0, 0, 0, a];
        v[..3].copy_from_slice(&bytes[..3]);
        unsafe { transmute(v) }
    }

    #[inline(always)]
    fn write_rgba(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&u32::to_le_bytes(unsafe { transmute(*self) }))
    }

    #[inline(always)]
    fn write_rgb(&self, bytes: &mut [u8]) {
        bytes.copy_from_slice(&u32::to_le_bytes(unsafe { transmute(*self) })[..3])
    }

    #[inline(always)]
    fn var(&self, prev: &Self) -> Var {
        debug_assert_eq!(self.a, prev.a);

        let r = self.r.wrapping_sub(prev.r);
        let g = self.g.wrapping_sub(prev.g);
        let b = self.b.wrapping_sub(prev.b);

        Var { r, g, b }
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
    fn luma(&self) -> Option<u16> {
        let r = self.r.wrapping_add(8).wrapping_sub(self.g);
        let g = self.g.wrapping_add(32);
        let b = self.b.wrapping_add(8).wrapping_sub(self.g);

        if (r | b) & 0xF0 == 0 && g & 0xC0 == 0 {
            Some(((QOI_OP_LUMA | g) as u16) << 8 | (r << 4 | b) as u16)
        } else {
            None
        }
    }
}

pub enum Colors {
    Srgb,
    SrgbLinA,
    Rgb,
    Rgba,
}

impl Colors {
    pub fn has_alpha(&self) -> bool {
        match self {
            Colors::Rgb | Colors::Srgb => false,
            Colors::Rgba | Colors::SrgbLinA => true,
        }
    }
}

/// QOI descriptor value.
pub struct Qoi {
    /// Width of the image.
    pub width: u32,

    /// Height of the image.
    pub height: u32,

    /// Specifies image color space.
    pub colors: Colors,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EncodeError {
    NotEnoughPixelData,
    OutputIsTooSmall,
}

impl Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::NotEnoughPixelData => {
                f.write_str("Pixels buffer does not contain enough data")
            }
            EncodeError::OutputIsTooSmall => {
                f.write_str("Output buffer is too small to fit encoded image")
            }
        }
    }
}

#[cfg(feature = "std")]
impl std::error::Error for EncodeError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DecodeError {
    DataIsTooSmall,
    InvalidMagic,
    InvalidChannelsValue,
    InvalidColorSpaceValue,
    OutputIsTooSmall,
}

impl Display for DecodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DecodeError::DataIsTooSmall => f.write_str("Encoded data slice is too small"),
            DecodeError::InvalidMagic => {
                f.write_str("Encoded data does not start with 'qoif' code")
            }
            DecodeError::InvalidChannelsValue => f.write_str("Numbers of channels is not 3 or 4"),
            DecodeError::InvalidColorSpaceValue => f.write_str("Colorspace is not 0 or 1"),
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

#[inline(always)]
fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}
