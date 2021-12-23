use super::*;

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

impl Qoi {
    /// Encode raw RGB or RGBA pixels into a QOI image in memory.\
    /// Encoded image is written into `output` slice.
    ///
    /// On success this function returns `Ok(())`.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    fn encode(&self, pixels: &[u8], output: &mut [u8]) -> Result<usize, EncodeError> {
        match self.colors.has_alpha() {
            true => self.encode_impl::<Rgba>(pixels, output),
            false => self.encode_impl::<Rgb>(pixels, output),
        }
    }

    fn encode_impl<P>(&self, pixels: &[u8], output: &mut [u8]) -> Result<usize, EncodeError>
    where
        P: Pixel,
    {
        debug_assert_eq!(self.colors.has_alpha(), P::HAS_ALPHA);

        let px_len = self.width as usize * self.height as usize * P::CHANNELS;

        let pixels = match pixels.get(..px_len) {
            None => {
                cold();
                return Err(EncodeError::NotEnoughPixelData);
            }
            Some(pixels) => pixels,
        };

        if output.len() <= QOI_HEADER_SIZE {
            return Err(EncodeError::OutputIsTooSmall);
        }

        output[0..4].copy_from_slice(&QOI_MAGIC.to_be_bytes());
        output[4..8].copy_from_slice(&self.width.to_be_bytes());
        output[8..12].copy_from_slice(&self.height.to_be_bytes());

        match self.colors {
            Colors::Rgb => {
                output[12] = 3;
                output[13] = 1;
            }
            Colors::Rgba => {
                output[12] = 4;
                output[13] = 1;
            }
            Colors::Srgb => {
                output[12] = 3;
                output[13] = 0;
            }
            Colors::SrgbLinA => {
                output[12] = 4;
                output[13] = 0;
            }
        }

        let mut index = [P::new(); 64];

        let mut run = 0u16;
        let mut px_prev = P::new_opaque();
        let mut px = px_prev;

        let mut rest = &mut output[QOI_HEADER_SIZE..];

        let mut chunks = pixels.chunks_exact(P::CHANNELS);

        loop {
            match chunks.next() {
                None => {
                    cold();
                    break;
                }
                Some(chunk) => {
                    if likely(rest.len() & !7 != 0) {
                        px.read(chunk);

                        if px == px_prev {
                            run += 1;

                            if run == 62 || chunks.len() == 0 {
                                cold();
                                rest[0] = QOI_OP_RUN | (run - 1) as u8;
                                rest = &mut rest[1..];
                                run = 0;
                            }
                        } else {
                            if run > 0 {
                                rest[0] = QOI_OP_RUN | (run - 1) as u8;
                                rest = &mut rest[1..];
                                run = 0;
                            }

                            match rest {
                                [b1, b2, b3, b4, b5, ..] => {
                                    let index_pos = px.hash();

                                    if index[index_pos as usize] == px {
                                        *b1 = QOI_OP_INDEX | index_pos as u8;
                                        rest = &mut rest[1..];
                                    } else {
                                        index[index_pos as usize] = px;

                                        if P::HAS_ALPHA && px_prev.a() != px.a() {
                                            cold();
                                            *b1 = QOI_OP_RGBA;
                                            *b2 = px.r();
                                            *b3 = px.g();
                                            *b4 = px.b();
                                            *b5 = px.a();
                                            rest = &mut rest[5..];
                                        } else {
                                            let v = px.var(&px_prev);

                                            if let Some(diff) = v.diff() {
                                                *b1 = diff;
                                                rest = &mut rest[1..];
                                            } else if let Some([lu, ma]) = v.luma() {
                                                *b1 = lu;
                                                *b2 = ma;
                                                rest = &mut rest[2..];
                                            } else {
                                                *b1 = QOI_OP_RGB;
                                                *b2 = px.r();
                                                *b3 = px.g();
                                                *b4 = px.b();
                                                rest = &mut rest[4..];
                                            }
                                        }
                                    }
                                    px_prev = px;
                                }
                                _ => {
                                    cold();
                                    unreachable!()
                                }
                            }
                        }
                    } else {
                        return Err(EncodeError::OutputIsTooSmall);
                    }
                }
            }
        }

        if rest.len() < QOI_PADDING {
            return Err(EncodeError::OutputIsTooSmall);
        }

        rest[..7].fill(0);
        rest[7] = 1;

        let tail = rest.len() - QOI_PADDING;
        let size = output.len() - tail;

        Ok(size)
    }

    /// Returns maximum size of the `Qoi::encode` output size.\
    /// Using smaller slice may cause `Qoi::encode` to return `Err(EncodeError::OutputIsTooSmall)`.
    pub fn encoded_size_limit(&self) -> usize {
        self.width as usize * self.height as usize * (self.colors.has_alpha() as usize + 4)
            + QOI_HEADER_SIZE
            + QOI_PADDING
    }

    /// Encode raw RGB or RGBA pixels into a QOI image in memory.\
    /// Encoded image is written into allocated `Vec`.
    ///
    /// On success this function returns `Ok(vec)` with `vec` containing encoded image.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    #[cfg(feature = "alloc")]
    pub fn encode_alloc(&self, pixels: &[u8]) -> Result<Vec<u8>, EncodeError> {
        let limit = self.encoded_size_limit();
        let mut output = vec![0; limit];
        let size = self.encode(pixels, &mut output)?;
        output.truncate(size);
        Ok(output)
    }
}
