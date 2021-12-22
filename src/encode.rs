use super::*;

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

impl Qoi {
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

        let pixels = match pixels.get(..px_len) {
            None => return Err(EncodeError::NotEnoughPixelData),
            Some(pixels) => pixels,
        };

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

        let mut chunks = pixels.chunks_exact(channels);

        while let Some(pixel) = chunks.next() {
            if unlikely(writer.len <= QOI_PADDING) {
                return Err(EncodeError::OutputIsTooSmall);
            }

            match has_alpha {
                true => px = Rgba::read_rgba(pixel),
                false => px = Rgba::read_rgb(pixel),
            }

            if px == px_prev {
                run += 1;

                if run == 62 || chunks.len() == 0 {
                    unsafe { writer.write_8(QOI_OP_RUN | (run - 1) as u8) }
                    run = 0;
                }
            } else {
                if run > 0 {
                    unsafe { writer.write_8(QOI_OP_RUN | (run - 1) as u8) }
                    run = 0;
                }

                let index_pos = qui_color_hash(px);

                if index[index_pos] == px {
                    unsafe { writer.write_8(QOI_OP_INDEX | index_pos as u8) }
                } else {
                    index[index_pos] = px;

                    if px_prev.a == px.a {
                        let v = px.var(&px_prev);

                        if let Some(diff) = v.diff() {
                            unsafe {
                                writer.write_8(diff);
                            }
                        } else if let Some(luma) = v.luma() {
                            unsafe {
                                writer.write_16(luma);
                            }
                        } else {
                            unsafe {
                                writer.write_8(QOI_OP_RGB);
                                writer.write_8(px.r);
                                writer.write_8(px.g);
                                writer.write_8(px.b);
                            }
                        }
                    } else {
                        unsafe {
                            writer.write_8(QOI_OP_RGBA);
                            writer.write_8(px.r);
                            writer.write_8(px.g);
                            writer.write_8(px.b);
                            writer.write_8(px.a);
                        }
                    }
                }
                px_prev = px;
            }
        }

        if writer.len < QOI_PADDING {
            return Err(EncodeError::OutputIsTooSmall);
        }

        unsafe { writer.pad() };

        let size = output.len() - writer.len;

        Ok(size)
    }

    /// Returns maximum size of the the `Qoi::encode` output size.
    pub fn encoded_size_limit(&self) -> usize {
        self.width as usize
            * self.height as usize
            * (self.color_space.alpha_srgb.is_some() as usize + 4)
            + QOI_HEADER_SIZE
            + QOI_PADDING
    }

    #[cfg(feature = "alloc")]
    pub fn encode_alloc(&self, pixels: &[u8]) -> Result<Vec<u8>, EncodeError> {
        let limit = self.encoded_size_limit();
        let mut output = vec![0; limit];
        let size = self.encode(pixels, &mut output)?;
        output.truncate(size);
        Ok(output)
    }
}
