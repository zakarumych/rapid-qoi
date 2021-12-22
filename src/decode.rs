use super::*;

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

impl Qoi {
    /// Returns decoded pixel data size.
    #[inline(always)]
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
    #[inline(always)]
    pub fn decode(bytes: &[u8], output: &mut [u8]) -> Result<Qoi, DecodeError> {
        let qoi = Self::decode_header(bytes)?;
        qoi.decode_skip_header(bytes, output)?;
        Ok(qoi)
    }

    /// Decode a QOI image from memory.
    /// Skips header parsing and uses provided header value.
    #[inline(always)]
    pub fn decode_skip_header(&self, bytes: &[u8], output: &mut [u8]) -> Result<(), DecodeError> {
        match self.color_space.alpha_srgb.is_some() {
            true => self.decode_impl::<true>(bytes, output),
            false => self.decode_impl::<false>(bytes, output),
        }
    }

    fn decode_impl<const HAS_ALPHA: bool>(
        &self,
        bytes: &[u8],
        output: &mut [u8],
    ) -> Result<(), DecodeError> {
        assert_eq!(HAS_ALPHA, self.color_space.alpha_srgb.is_some());

        let px_len = self.decoded_size();

        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        if px_len > output.len() {
            return Err(DecodeError::OutputIsTooSmall);
        }

        let mut reader = UnsafeReader {
            ptr: bytes[QOI_HEADER_SIZE..].as_ptr(),
            len: bytes[QOI_HEADER_SIZE..].len(),
        };

        let mut px = Rgba::new_opaque();
        let mut index = [Rgba::new(); 64];

        let channels = HAS_ALPHA as usize + 3;

        let mut px_pos = 0;
        while px_pos < px_len {
            if likely(reader.len > QOI_PADDING) {
                let b1 = unsafe { reader.read_8() };

                match b1 {
                    0b11111110 => unsafe {
                        reader.read_rgb(&mut px);
                    },
                    0b11111111 => unsafe {
                        reader.read_rgba(&mut px);
                    },
                    0b00000000..=0b00111111 => {
                        px = index[b1 as usize];
                    }
                    0b01000000..=0b01111111 => {
                        px.r = px.r.wrapping_add(((b1 >> 4) & 0x03).wrapping_sub(2));
                        px.g = px.g.wrapping_add(((b1 >> 2) & 0x03).wrapping_sub(2));
                        px.b = px.b.wrapping_add((b1 & 0x03).wrapping_sub(2));
                    }
                    0b10000000..=0b10111111 => {
                        let b2 = unsafe { reader.read_8() };

                        let vg = (b1 & 0x3f).wrapping_sub(32);
                        let vr = ((b2 >> 4) & 0x0f).wrapping_sub(8).wrapping_add(vg);
                        let vb = (b2 & 0x0f).wrapping_sub(8).wrapping_add(vg);

                        px.r = px.r.wrapping_add(vr);
                        px.g = px.g.wrapping_add(vg);
                        px.b = px.b.wrapping_add(vb);
                    }
                    0b11000000.. => {
                        let mut run = (b1 & 0x3f) + 1;

                        while run > 0 {
                            run -= 1;
                            if unlikely(px_pos >= px_len) {
                                return Err(DecodeError::OutputIsTooSmall);
                            }
                            match HAS_ALPHA {
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
                    }
                }
                index[qui_color_hash(px)] = px;
            } else {
                return Err(DecodeError::DataIsTooSmall);
            }

            match HAS_ALPHA {
                true => px.write_rgba(unsafe { output.get_unchecked_mut(px_pos..px_pos + 4) }),
                false => px.write_rgb(unsafe { output.get_unchecked_mut(px_pos..px_pos + 3) }),
            }
            px_pos += channels;
        }

        Ok(())
    }

    #[cfg(feature = "alloc")]
    #[inline(always)]
    pub fn decode_alloc(bytes: &[u8]) -> Result<(Self, Vec<u8>), DecodeError> {
        let qoi = Self::decode_header(bytes)?;

        let size = qoi.decoded_size();
        let mut output = vec![0; size];
        let qoi = Self::decode(bytes, &mut output)?;
        Ok((qoi, output))
    }
}
