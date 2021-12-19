//! QOI - The “Quite OK Image” format for fast, lossless image compression
//!
//! This crate is Rust rewrite, not bindings to reference implementation.
//!
//! # About
//!
//! QOI encodes and decodes images in a lossless format. An encoded QOI image is
//! usually around 10--30% larger than a decently optimized PNG image.
//!
//! QOI outperforms simpler PNG encoders in compression ratio and performance. QOI
//! images are typically 20% smaller than PNGs written with stbi_image but 10%
//! larger than with libpng. Encoding is 25-50x faster and decoding is 3-4x faster
//! than stbi_image or libpng.
//!
//! # Data Format
//!
//! A QOI file has a 12 byte header, followed by any number of data "chunks".
//!
//! ```
//! struct QoiHeader {
//!     magic: [u8; 4],     // magic bytes "qoif"
//!     width: u32,         // image width in pixels (BE)
//!     height: u32,        // image height in pixels (BE)
//!     channels: u8,       // must be 3 (RGB) or 4 (RGBA)
//!     colorspace: u8,     // a bitmap 0000rgba where
//!                         //   - a zero bit indicates sRGBA,
//!                         //   - a one bit indicates linear (user interpreted)
//!                         //   colorspace for each channel
//! };
//! ```
//!
//! The decoder and encoder start with `{r: 0, g: 0, b: 0, a: 255}` as the previous
//! pixel value. Pixels are either encoded as
//!  * a run of the previous pixel
//!  * an index into a previously seen pixel
//!  * a difference to the previous pixel value in r,g,b,a
//!  * full r,g,b,a values
//!
//! A running array[64] of previously seen pixel values is maintained by the encoder
//! and decoder. Each pixel that is seen by the encoder and decoder is put into this
//! array at the position `(r^g^b^a) % 64`. In the encoder, if the pixel value at this
//! index matches the current pixel, this index position is written to the stream.
//!
//! Each chunk starts with a 2, 3 or 4 bit tag, followed by a number of data bits.
//! The bit length of chunks is divisible by 8 - i.e. all chunks are byte aligned.
//!
//! ```
//! QOI_INDEX {
//!     u8 tag  :  2;   // b00
//!     u8 idx  :  6;   // 6-bit index into the color index array: 0..63
//! }
//!
//! QOI_RUN_8 {
//!     u8 tag  :  3;   // b010
//!     u8 run  :  5;   // 5-bit run-length repeating the previous pixel: 1..32
//! }
//!
//! QOI_RUN_16 {
//!     u8 tag  :  3;   // b011
//!     u16 run : 13;   // 13-bit run-length repeating the previous pixel: 33..8224
//! }
//!
//! QOI_DIFF_8 {
//!     u8 tag  :  2;   // b10
//!     u8 dr   :  2;   // 2-bit   red channel difference: -1..2
//!     u8 dg   :  2;   // 2-bit green channel difference: -1..2
//!     u8 db   :  2;   // 2-bit  blue channel difference: -1..2
//! }
//!
//! QOI_DIFF_16 {
//!     u8 tag  :  3;   // b110
//!     u8 dr   :  5;   // 5-bit   red channel difference: -15..16
//!     u8 dg   :  4;   // 4-bit green channel difference:  -7.. 8
//!     u8 db   :  4;   // 4-bit  blue channel difference:  -7.. 8
//! }
//!
//! QOI_DIFF_24 {
//!     u8 tag  :  4;   // b1110
//!     u8 dr   :  5;   // 5-bit   red channel difference: -15..16
//!     u8 dg   :  5;   // 5-bit green channel difference: -15..16
//!     u8 db   :  5;   // 5-bit  blue channel difference: -15..16
//!     u8 da   :  5;   // 5-bit alpha channel difference: -15..16
//! }
//!
//! QOI_COLOR {
//!     u8 tag  :  4;   // b1111
//!     u8 has_r:  1;   //   red byte follows
//!     u8 has_g:  1;   // green byte follows
//!     u8 has_b:  1;   //  blue byte follows
//!     u8 has_a:  1;   // alpha byte follows
//!     u8 r;           //   red value if has_r == 1: 0..255
//!     u8 g;           // green value if has_g == 1: 0..255
//!     u8 b;           //  blue value if has_b == 1: 0..255
//!     u8 a;           // alpha value if has_a == 1: 0..255
//! }
//! ```
//!
//! The byte stream is padded with 4 zero bytes. Size the longest chunk we can
//! encounter is 5 bytes (QOI_COLOR with RGBA set), with this padding we just have
//! to check for an overrun once per decode loop iteration.
//!
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "alloc")]
extern crate alloc;

use core::{
    fmt::{self, Display},
    mem::transmute,
    ptr::{copy_nonoverlapping, write_bytes},
};

mod decode;
mod encode;

pub struct ColorSpace {
    /// Red channel color space of the image.
    pub red_srgb: bool,

    /// Green channel color space of the image.
    pub green_srgb: bool,

    /// Blue channel color space of the image.
    pub blue_srgb: bool,

    /// Alpha channel color space of the image.
    /// `None` means channel does not exists.
    pub alpha_srgb: Option<bool>,
}

impl ColorSpace {
    pub const SRGB: Self = ColorSpace {
        red_srgb: true,
        green_srgb: true,
        blue_srgb: true,
        alpha_srgb: None,
    };

    pub const SRGBA: Self = ColorSpace {
        red_srgb: true,
        green_srgb: true,
        blue_srgb: true,
        alpha_srgb: Some(true),
    };

    pub const SRGB_LINA: Self = ColorSpace {
        red_srgb: true,
        green_srgb: true,
        blue_srgb: true,
        alpha_srgb: Some(false),
    };

    pub const RGB: Self = ColorSpace {
        red_srgb: false,
        green_srgb: false,
        blue_srgb: false,
        alpha_srgb: None,
    };

    pub const RGBA: Self = ColorSpace {
        red_srgb: false,
        green_srgb: false,
        blue_srgb: false,
        alpha_srgb: Some(false),
    };
}

const QOI_INDEX: u8 = 0x00; // 00xxxxxx
const QOI_RUN_8: u8 = 0x40; // 010xxxxx
const QOI_RUN_16: u8 = 0x60; // 011xxxxx
const QOI_DIFF_8: u8 = 0x80; // 10xxxxxx
const QOI_DIFF_16: u8 = 0xc0; // 110xxxxx
const QOI_DIFF_24: u8 = 0xe0; // 1110xxxx
const QOI_COLOR: u8 = 0xf0; // 1111xxxx

const QOI_MASK_2: u8 = 0xc0; // 11000000
const QOI_MASK_3: u8 = 0xe0; // 11100000
const QOI_MASK_4: u8 = 0xf0; // 11110000

#[inline(always)]
const fn qui_color_hash(c: Rgba) -> usize {
    ((c.r ^ c.g ^ c.b ^ c.a) & 63) as usize
}

const QOI_MAGIC: u32 = u32::from_be_bytes(*b"qoif");
const QOI_HEADER_SIZE: usize = 14;
const QOI_PADDING: usize = 4;

struct UnsafeWriter {
    ptr: *mut u8,
    len: usize,
}

impl UnsafeWriter {
    #[inline(always)]
    unsafe fn fill(&mut self, v: u8, count: usize) {
        write_bytes(self.ptr, v, count);
        self.ptr = self.ptr.add(count);
        self.len -= count;
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

    #[inline(always)]
    unsafe fn skip_8(&mut self) -> *mut u8 {
        let ptr = self.ptr;
        self.ptr = self.ptr.add(1);
        self.len -= 1;
        ptr
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
    unsafe fn read_16(&mut self) -> u16 {
        let v = u16::from_be_bytes(*(self.ptr as *const [u8; 2]));
        self.ptr = self.ptr.add(2);
        self.len -= 2;
        v
    }

    #[inline(always)]
    unsafe fn read_32(&mut self) -> u32 {
        let v = u32::from_be_bytes(*(self.ptr as *const [u8; 4]));
        self.ptr = self.ptr.add(4);
        self.len -= 4;
        v
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
    fn read_rgb(bytes: &[u8]) -> Self {
        let mut v = [255; 4];
        v.copy_from_slice(&bytes[..3]);
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
        let r = self.r as i16 - prev.r as i16;
        let g = self.g as i16 - prev.g as i16;
        let b = self.b as i16 - prev.b as i16;
        let a = self.a as i16 - prev.a as i16;

        Var { r, g, b, a }
    }
}

#[derive(Clone, Copy)]
#[repr(C)]
struct Var {
    r: i16,
    g: i16,
    b: i16,
    a: i16,
}

impl Var {
    #[inline(always)]
    fn diff8(&self) -> Option<u8> {
        assert_eq!(self.a, 0);
        let r = self.r + 2;
        let g = self.g + 2;
        let b = self.b + 2;
        if (r | g | b) & !3 == 0 {
            Some(QOI_DIFF_8 | (r << 4) as u8 | (g << 2) as u8 | b as u8)
        } else {
            None
        }
    }

    #[inline(always)]
    fn diff16(&self) -> Option<u16> {
        assert_eq!(self.a, 0);
        ((self.r + 15) & !31) | (((self.g + 7) | (self.b + 7)) & !15) == 0
    }

    #[inline(always)]
    fn diff24(&self) -> Option<[u8; 3]> {
        let r = self.r + 16;
        let g = self.g + 16;
        let b = self.b + 16;
        let a = self.a + 16;
        if (r | g | b | a) & !31 == 0 {
            Some([
                QOI_DIFF_24 | ((r >> 1) as u8),
                ((r << 7) as u8) | ((g << 2) as u8) | ((b >> 3) as u8),
                ((b << 5) as u8) | (a as u8),
            ])
        } else {
            None
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
    pub color_space: ColorSpace,
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
