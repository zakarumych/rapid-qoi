use super::*;

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

impl Qoi {
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

        let mut px_pos = 0;
        while px_pos < px_len {
            if likely(reader.len > QOI_PADDING) {
                let b1 = unsafe { reader.read_8() };

                if (b1 & QOI_MASK_2) == QOI_INDEX {
                    px = index[(b1 ^ QOI_INDEX) as usize];
                } else {
                    if (b1 & QOI_MASK_3) == QOI_RUN_8 {
                        let run = (b1 & 0x1f) + 1;

                        for _ in 0..run {
                            if unlikely(px_pos >= px_len) {
                                return Err(DecodeError::RunOutOfPixels);
                            }
                            match has_alpha {
                                true => px.write_rgba(unsafe {
                                    output.get_unchecked_mut(px_pos..px_pos + 4)
                                }),
                                false => px.write_rgb(unsafe {
                                    output.get_unchecked_mut(px_pos..px_pos + 3)
                                }),
                            }
                            px_pos += channels;
                        }
                        continue;
                    } else if (b1 & QOI_MASK_3) == QOI_RUN_16 {
                        let b2 = unsafe { reader.read_8() };
                        let run = ((((b1 & 0x1f) as u16) << 8) | (b2 as u16)) + 33;

                        for _ in 0..run {
                            if unlikely(px_pos >= px_len) {
                                return Err(DecodeError::RunOutOfPixels);
                            }
                            match has_alpha {
                                true => px.write_rgba(unsafe {
                                    output.get_unchecked_mut(px_pos..px_pos + 4)
                                }),
                                false => px.write_rgb(unsafe {
                                    output.get_unchecked_mut(px_pos..px_pos + 3)
                                }),
                            }
                            px_pos += channels;
                        }
                        continue;
                    } else if (b1 & QOI_MASK_2) == QOI_DIFF_8 {
                        px.r = px.r.wrapping_add(((b1 >> 4) & 0x03).wrapping_sub(2));
                        // px.r = ((px.r as i16) + (((b1 >> 4) & 0x03) as i16 - 1)) as u8;
                        px.g = px.g.wrapping_add(((b1 >> 2) & 0x03).wrapping_sub(2));
                        // px.g = ((px.g as i16) + (((b1 >> 2) & 0x03) as i16 - 1)) as u8;
                        px.b = px.b.wrapping_add((b1 & 0x03).wrapping_sub(2));
                        // px.b = ((px.b as i16) + ((b1 & 0x03) as i16 - 1)) as u8;
                    } else if (b1 & QOI_MASK_3) == QOI_DIFF_16 {
                        let b2 = unsafe { reader.read_8() };
                        px.r = px.r.wrapping_add((b1 & 0x1f).wrapping_sub(16));
                        px.g = px.g.wrapping_add((b2 >> 4).wrapping_sub(8));
                        px.b = px.b.wrapping_add((b2 & 0x0f).wrapping_sub(8));
                    } else if (b1 & QOI_MASK_4) == QOI_DIFF_24 {
                        let b2 = unsafe { reader.read_8() };
                        let b3 = unsafe { reader.read_8() };
                        px.r =
                            px.r.wrapping_add((((b1 & 0x0f) << 1) | (b2 >> 7)).wrapping_sub(16));
                        px.g = px.g.wrapping_add(((b2 & 0x7c) >> 2).wrapping_sub(16));
                        px.b = px.b.wrapping_add(
                            (((b2 & 0x03) << 3) | ((b3 & 0xe0) >> 5)).wrapping_sub(16),
                        );
                        px.a = px.a.wrapping_add((b3 & 0x1f).wrapping_sub(16));
                    } else {
                        cold();
                        debug_assert_eq!((b1 & QOI_MASK_4), QOI_COLOR);
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
    pub fn decode_alloc(bytes: &[u8]) -> Result<(Self, Vec<u8>), DecodeError> {
        let qoi = Self::decode_header(bytes)?;

        let size = qoi.decoded_size();
        let mut output = vec![0; size];
        let qoi = Self::decode(bytes, &mut output)?;
        Ok((qoi, output))
    }
}
