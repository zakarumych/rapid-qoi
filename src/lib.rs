//! QOI - The “Quite OK Image” format for fast, lossless image compression
//!
//! Dominic Szablewski - https://phoboslab.org
//!
//!
//! -- LICENSE: The MIT License(MIT)
//!
//! Copyright(c) 2021 Dominic Szablewski
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of
//! this software and associated documentation files(the "Software"), to deal in
//! the Software without restriction, including without limitation the rights to
//! use, copy, modify, merge, publish, distribute, sublicense, and / or sell copies
//! of the Software, and to permit persons to whom the Software is furnished to do
//! so, subject to the following conditions :
//! The above copyright notice and this permission notice shall be included in all
//! copies or substantial portions of the Software.
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
//! IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
//! FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.IN NO EVENT SHALL THE
//! AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
//! LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
//! OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
//! SOFTWARE.
//!
//!
//! -- About
//!
//! QOI encodes and decodes images in a lossless format. An encoded QOI image is
//! usually around 10--30% larger than a decently optimized PNG image.
//!
//! QOI outperforms simpler PNG encoders in compression ratio and performance. QOI
//! images are typically 20% smaller than PNGs written with stbi_image but 10%
//! larger than with libpng. Encoding is 25-50x faster and decoding is 3-4x faster
//! than stbi_image or libpng.
//!
//! -- Data Format
//!
//! A QOI file has a 12 byte header, followed by any number of data "chunks".
//!
//! struct qoi_header_t {
//!     char [4];              // magic bytes "qoif"
//!     unsigned short width;  // image width in pixels (BE)
//!     unsigned short height; // image height in pixels (BE)
//!     unsigned int size;     // number of data bytes following this header (BE)
//! };
//!
//! The decoder and encoder start with {r: 0, g: 0, b: 0, a: 255} as the previous
//! pixel value. Pixels are either encoded as
//!  - a run of the previous pixel
//!  - an index into a previously seen pixel
//!  - a difference to the previous pixel value in r,g,b,a
//!  - full r,g,b,a values
//!
//! A running array[64] of previously seen pixel values is maintained by the encoder
//! and decoder. Each pixel that is seen by the encoder and decoder is put into this
//! array at the position (r^g^b^a) % 64. In the encoder, if the pixel value at this
//! index matches the current pixel, this index position is written to the stream.
//!
//! Each chunk starts with a 2, 3 or 4 bit tag, followed by a number of data bits.
//! The bit length of chunks is divisible by 8 - i.e. all chunks are byte aligned.
//!
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
//!
//! The byte stream is padded with 4 zero bytes. Size the longest chunk we can
//! encounter is 5 bytes (QOI_COLOR with RGBA set), with this padding we just have
//! to check for an overrun once per decode loop iteration.
//!
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

use core::{
    mem::transmute,
    ptr::{copy_nonoverlapping, write_bytes},
};

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

pub struct UnsafeWriter {
    ptr: *mut u8,
    len: usize,
}

impl UnsafeWriter {
    #[inline(always)]
    pub unsafe fn fill(&mut self, v: u8, count: usize) {
        write_bytes(self.ptr, v, count);
        self.ptr = self.ptr.add(count);
        self.len -= count;
    }

    #[inline(always)]
    pub unsafe fn write_8(&mut self, v: u8) {
        *self.ptr = v;
        self.ptr = self.ptr.add(1);
        self.len -= 1;
    }

    #[inline(always)]
    pub unsafe fn write_16(&mut self, v: u16) {
        copy_nonoverlapping(v.to_be_bytes().as_ptr(), self.ptr, 2);
        self.ptr = self.ptr.add(2);
        self.len -= 2;
    }

    #[inline(always)]
    pub unsafe fn write_32(&mut self, v: u32) {
        copy_nonoverlapping(v.to_be_bytes().as_ptr(), self.ptr, 4);
        self.ptr = self.ptr.add(4);
        self.len -= 4;
    }
}

pub struct UnsafeReader {
    ptr: *const u8,
    len: usize,
}

impl UnsafeReader {
    #[inline(always)]
    pub unsafe fn read_8(&mut self) -> u8 {
        let v = *self.ptr;
        self.ptr = self.ptr.add(1);
        self.len -= 1;
        v
    }

    #[inline(always)]
    pub unsafe fn read_16(&mut self) -> u16 {
        let v = u16::from_be_bytes(*(self.ptr as *const [u8; 2]));
        self.ptr = self.ptr.add(2);
        self.len -= 2;
        v
    }

    #[inline(always)]
    pub unsafe fn read_32(&mut self) -> u32 {
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
    fn diff8(&self) -> bool {
        assert_eq!(self.a, 0);
        ((self.r + 1) | (self.g + 1) | (self.b + 1)) & !3 == 0

        // self.r > -2 && self.r < 3 && self.g > -2 && self.g < 3 && self.b > -2 && self.b < 3
    }

    #[inline(always)]
    fn diff16(&self) -> bool {
        assert_eq!(self.a, 0);
        ((self.r + 15) & !31) | (((self.g + 7) | (self.b + 7)) & !15) == 0

        // self.r > -16 && self.r < 17 && self.g > -8 && self.g < 9 && self.b > -8 && self.b < 9
    }

    #[inline(always)]
    fn diff24(&self) -> bool {
        ((self.r + 15) | (self.g + 15) | (self.b + 15) | (self.a + 15)) & !31 == 0

        // self.r > -16
        //     && self.r < 17
        //     && self.g > -16
        //     && self.g < 17
        //     && self.b > -16
        //     && self.b < 17
        //     && self.a > -16
        //     && self.a < 17
    }
}

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

/// QOI descriptor value.
pub struct Qoi {
    /// Width of the image.
    pub width: u32,

    /// Height of the image.
    pub height: u32,

    /// Specifies image color space.
    pub color_space: ColorSpace,
}

#[derive(Debug)]
pub enum EncodeError {
    NotEnoughPixelData,
    OutputIsTooSmall,
}

#[derive(Debug)]
pub enum DecodeError {
    DataIsTooSmall,
    InvalidMagic,
    InvalidChannelsValue,
    OutputIsTooSmall,
}

impl Qoi {
    /// Returns maximum size of the the `Qoi::encode` output size.
    pub fn encoded_size_limit(&self) -> usize {
        self.width as usize
            * self.height as usize
            * (self.color_space.alpha_srgb.is_some() as usize + 4)
            + QOI_HEADER_SIZE
            + QOI_PADDING
    }

    /// Returns decoded pixel data size.
    pub fn decoded_size(&self) -> usize {
        self.width as usize
            * self.height as usize
            * (self.color_space.alpha_srgb.is_some() as usize + 3)
    }

    /// Decodes header from QOI
    pub fn decode_header(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < QOI_HEADER_SIZE {
            return Err(DecodeError::DataIsTooSmall);
        }

        let mut reader = UnsafeReader {
            ptr: bytes.as_ptr(),
            len: bytes.len(),
        };

        let magic = unsafe { reader.read_32() };

        if magic != QOI_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }

        let w = unsafe { reader.read_32() };
        let h = unsafe { reader.read_32() };
        let channels = unsafe { reader.read_8() } as usize;
        let color_space = unsafe { reader.read_8() };

        let has_alpha = match channels {
            3 => false,
            4 => true,
            _ => return Err(DecodeError::InvalidChannelsValue),
        };

        Ok(Qoi {
            width: w,
            height: h,
            color_space: ColorSpace {
                red_srgb: (color_space & 0x8) != 0,
                green_srgb: (color_space & 0x4) != 0,
                blue_srgb: (color_space & 0x2) != 0,
                alpha_srgb: if has_alpha {
                    Some((color_space & 0x1) != 0)
                } else {
                    None
                },
            },
        })
    }

    /// Encode raw RGB or RGBA pixels into a QOI image in memory. w and h denote the
    /// width and height of the pixel data. channels must be either 3 for RGB data
    /// or 4 for RGBA.
    /// The function either returns NULL on failure (invalid parameters or malloc
    /// failed) or a pointer to the encoded data on success. On success the out_len
    /// is set to the size in bytes of the encoded data.
    /// The returned qoi data should be free()d after user.
    pub fn encode(&self, pixels: &[u8], output: &mut [u8]) -> Result<usize, EncodeError> {
        let has_alpha = self.color_space.alpha_srgb.is_some();
        let channels = has_alpha as usize + 3;

        let px_len = self.width as usize * self.height as usize * channels;
        if pixels.len() < px_len {
            return Err(EncodeError::NotEnoughPixelData);
        }

        let mut writer = UnsafeWriter {
            ptr: output.as_mut_ptr(),
            len: output.len(),
        };

        if writer.len <= QOI_HEADER_SIZE {
            return Err(EncodeError::OutputIsTooSmall);
        }

        unsafe {
            writer.write_32(QOI_MAGIC);
            writer.write_32(self.width);
            writer.write_32(self.height);
            writer.write_8(channels as u8);
            writer.write_8(
                match self.color_space.red_srgb {
                    true => 0x8,
                    false => 0,
                } | match self.color_space.green_srgb {
                    true => 0x4,
                    false => 0,
                } | match self.color_space.blue_srgb {
                    true => 0x2,
                    false => 0,
                } | match self.color_space.alpha_srgb {
                    Some(true) => 0x1,
                    None | Some(false) => 0,
                },
            );
        }

        let mut index = [Rgba::new(); 64];

        let mut run = 0u16;
        let mut px_prev = Rgba::new_opaque();
        let mut px;

        let mut chunks = pixels.chunks_exact(channels).peekable();

        while let Some(pixel) = chunks.next() {
            if writer.len <= QOI_PADDING {
                return Err(EncodeError::OutputIsTooSmall);
            }

            match has_alpha {
                true => px = Rgba::read_rgba(pixel),
                false => px = Rgba::read_rgb(pixel),
            }

            if px == px_prev {
                run += 1;

                if run == 0x2020 || chunks.peek().is_none() {
                    if run < 33 {
                        run -= 1;
                        unsafe { writer.write_8(QOI_RUN_8 | run as u8) }
                    } else {
                        run -= 33;
                        unsafe {
                            writer.write_16(((QOI_RUN_16 as u16) << 8) | run);
                        }
                    }
                    run = 0;
                }
            } else {
                if run > 0 {
                    if run < 33 {
                        run -= 1;
                        unsafe { writer.write_8(QOI_RUN_8 | run as u8) }
                    } else {
                        run -= 33;
                        unsafe {
                            writer.write_16(((QOI_RUN_16 as u16) << 8) | run);
                        }
                    }
                    run = 0;
                }

                let index_pos = qui_color_hash(px);

                if index[index_pos] == px {
                    unsafe { writer.write_8(QOI_INDEX | index_pos as u8) }
                } else {
                    index[index_pos] = px;

                    let v = px.var(&px_prev);

                    if v.a == 0 {
                        if v.diff8() {
                            unsafe {
                                writer.write_8(
                                    QOI_DIFF_8
                                        | ((v.r + 1) << 4) as u8
                                        | ((v.g + 1) << 2) as u8
                                        | (v.b + 1) as u8,
                                );
                            }
                            px_prev = px;
                            continue;
                        } else if v.diff16() {
                            unsafe {
                                writer.write_8(QOI_DIFF_16 | (v.r + 15) as u8);
                                writer.write_8(((v.g + 7) << 4) as u8 | (v.b + 7) as u8);
                            }
                            px_prev = px;
                            continue;
                        }
                    }

                    if v.diff24() {
                        unsafe {
                            writer.write_8(QOI_DIFF_24 | ((v.r + 15) >> 1) as u8);
                            writer.write_8(
                                ((v.r + 15) << 7) as u8
                                    | ((v.g + 15) << 2) as u8
                                    | ((v.b + 15) >> 3) as u8,
                            );
                            writer.write_8(((v.b + 15) << 5) as u8 | (v.a + 15) as u8);
                        }
                    } else {
                        unsafe {
                            writer.write_8(
                                QOI_COLOR
                                    | (if v.r != 0 { 8 } else { 0 })
                                    | (if v.g != 0 { 4 } else { 0 })
                                    | (if v.b != 0 { 2 } else { 0 })
                                    | (if v.a != 0 { 1 } else { 0 }),
                            );
                            if v.r != 0 {
                                writer.write_8(px.r);
                            }
                            if v.g != 0 {
                                writer.write_8(px.g);
                            }
                            if v.b != 0 {
                                writer.write_8(px.b);
                            }
                            if v.a != 0 {
                                writer.write_8(px.a);
                            }
                        }
                    }
                }
                px_prev = px;
            }
        }

        if writer.len < QOI_PADDING {
            return Err(EncodeError::OutputIsTooSmall);
        }

        unsafe { writer.fill(0, QOI_PADDING) };

        let size = unsafe { writer.ptr.offset_from(output.as_ptr()) } as usize;

        Ok(size)
    }

    /// Decode a QOI image from memory.
    pub fn decode(bytes: &[u8], output: &mut [u8]) -> Result<Qoi, DecodeError> {
        let qoi = Self::decode_header(bytes)?;
        qoi.decode_skip_header(bytes, output)?;
        Ok(qoi)
    }

    /// Decode a QOI image from memory.
    /// Skips header parsing and uses provided header value.
    pub fn decode_skip_header(&self, bytes: &[u8], output: &mut [u8]) -> Result<(), DecodeError> {
        let mut reader = UnsafeReader {
            ptr: bytes[QOI_HEADER_SIZE..].as_ptr(),
            len: bytes[QOI_HEADER_SIZE..].len(),
        };

        let px_len = self.decoded_size();

        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        if px_len > output.len() {
            return Err(DecodeError::OutputIsTooSmall);
        }

        let mut px = Rgba::new_opaque();
        let mut index = [Rgba::new(); 64];

        let has_alpha = self.color_space.alpha_srgb.is_some();
        let channels = has_alpha as usize + 3;

        let mut run = 0u16;
        let mut px_pos = 0;
        while px_pos < px_len {
            if run > 0 {
                run -= 1;
            } else if reader.len >= QOI_PADDING {
                let b1 = unsafe { reader.read_8() };

                if (b1 & QOI_MASK_2) == QOI_INDEX {
                    px = index[(b1 ^ QOI_INDEX) as usize];
                } else if (b1 & QOI_MASK_3) == QOI_RUN_8 {
                    let mut run = (b1 & 0x1f) + 1;

                    while run > 0 && px_pos < px_len {
                        run -= 1;
                        px_pos += channels;

                        match has_alpha {
                            true => px.write_rgba(unsafe {
                                output.get_unchecked_mut(px_pos..px_pos + 4)
                            }),
                            false => px
                                .write_rgb(unsafe { output.get_unchecked_mut(px_pos..px_pos + 3) }),
                        }
                    }
                } else if (b1 & QOI_MASK_3) == QOI_RUN_16 {
                    let b2 = unsafe { reader.read_8() };
                    let mut run = ((((b1 & 0x1f) as u16) << 8) | (b2 as u16)) + 33;

                    while run > 0 && px_pos < px_len {
                        run -= 1;
                        px_pos += channels;

                        match has_alpha {
                            true => px.write_rgba(unsafe {
                                output.get_unchecked_mut(px_pos..px_pos + 4)
                            }),
                            false => px
                                .write_rgb(unsafe { output.get_unchecked_mut(px_pos..px_pos + 3) }),
                        }
                    }
                } else if (b1 & QOI_MASK_2) == QOI_DIFF_8 {
                    px.r = (px.r as i16 + ((b1 >> 4) & 0x03) as i16 - 1) as u8;
                    px.g = (px.g as i16 + ((b1 >> 2) & 0x03) as i16 - 1) as u8;
                    px.b = (px.b as i16 + (b1 & 0x03) as i16 - 1) as u8;
                } else if (b1 & QOI_MASK_3) == QOI_DIFF_16 {
                    let b2 = unsafe { reader.read_8() };
                    px.r = (px.r as i16 + (b1 & 0x1f) as i16 - 15) as u8;
                    px.g = (px.g as i16 + (b2 >> 4) as i16 - 7) as u8;
                    px.b = (px.b as i16 + (b2 & 0x0f) as i16 - 7) as u8;
                } else if (b1 & QOI_MASK_4) == QOI_DIFF_24 {
                    let b2 = unsafe { reader.read_8() };
                    let b3 = unsafe { reader.read_8() };
                    px.r = (px.r as i16 + (((b1 & 0x0f) << 1) | (b2 >> 7)) as i16 - 15) as u8;
                    px.g = (px.g as i16 + ((b2 & 0x7c) >> 2) as i16 - 15) as u8;
                    px.b =
                        (px.b as i16 + (((b2 & 0x03) << 3) | ((b3 & 0xe0) >> 5)) as i16 - 15) as u8;
                    px.a = (px.a as i16 + (b3 & 0x1f) as i16 - 15) as u8;
                } else if (b1 & QOI_MASK_4) == QOI_COLOR {
                    if (b1 & 8) != 0 {
                        px.r = unsafe { reader.read_8() };
                    }
                    if (b1 & 4) != 0 {
                        px.g = unsafe { reader.read_8() };
                    }
                    if (b1 & 2) != 0 {
                        px.b = unsafe { reader.read_8() };
                    }
                    if (b1 & 1) != 0 {
                        px.a = unsafe { reader.read_8() };
                    }
                }

                index[qui_color_hash(px)] = px;
            }

            match has_alpha {
                true => px.write_rgba(unsafe { output.get_unchecked_mut(px_pos..px_pos + 4) }),
                false => px.write_rgb(unsafe { output.get_unchecked_mut(px_pos..px_pos + 3) }),
            }
            px_pos += channels;
        }

        return Ok(());
    }

    #[cfg(feature = "alloc")]
    pub fn encode_alloc(&self, pixels: &[u8]) -> Result<Vec<u8>, EncodeError> {
        let limit = self.encoded_size_limit();
        let mut output = vec![0; limit];
        let size = self.encode(pixels, &mut output)?;
        output.truncate(size);
        Ok(output)
    }

    #[cfg(feature = "alloc")]
    pub fn decode_alloc(bytes: &[u8]) -> Result<(Self, Vec<u8>), DecodeError> {
        let qoi = Self::decode_header(bytes)?;

        let size = qoi.decoded_size();
        let mut output = vec![0; size];
        let qoi = Self::decode(bytes, &mut output)?;
        Ok((qoi, output))
    }
}
