use core::convert::TryInto;

use super::*;

#[cfg(feature = "alloc")]
use alloc::{vec, vec::Vec};

impl Qoi {
    /// Returns decoded pixel data size for the image.
    #[inline(always)]
    pub fn decoded_size(&self) -> usize {
        self.width as usize * self.height as usize * (self.colors.has_alpha() as usize + 3)
    }

    /// Reads header from encoded QOI image.\
    /// Returned header can be analyzed before proceeding parsing with [`Qoi::decode_skip_header`].
    pub fn decode_header(bytes: &[u8]) -> Result<Self, DecodeError> {
        if bytes.len() < QOI_HEADER_SIZE {
            return Err(DecodeError::DataIsTooSmall);
        }

        let magic = u32::from_be_bytes(bytes[0..4].try_into().unwrap());
        if magic != QOI_MAGIC {
            return Err(DecodeError::InvalidMagic);
        }

        let w = u32::from_be_bytes(bytes[4..8].try_into().unwrap());
        let h = u32::from_be_bytes(bytes[8..12].try_into().unwrap());

        let channels = bytes[12];
        let colors = bytes[13];

        Ok(Qoi {
            width: w,
            height: h,
            colors: match (channels, colors) {
                (3, 0) => Colors::Srgb,
                (4, 0) => Colors::SrgbLinA,
                (3, 1) => Colors::Rgb,
                (4, 1) => Colors::Rgba,
                (_, 0 | 1) => return Err(DecodeError::InvalidChannelsValue),
                (_, _) => return Err(DecodeError::InvalidColorSpaceValue),
            },
        })
    }

    /// Decode a QOI image from bytes slice.\
    /// Decoded raw RGB or RGBA pixels are written into `output` slice.
    ///
    /// On success this function returns `Ok(qoi)` with `qoi` describing image dimensions and color space.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    #[inline(always)]
    pub fn decode(bytes: &[u8], output: &mut [u8]) -> Result<Self, DecodeError> {
        let qoi = Self::decode_header(bytes)?;
        qoi.decode_skip_header(bytes, output)?;
        Ok(qoi)
    }

    /// Decode a QOI image from bytes slice.\
    /// Skips header reading, uses provided `Qoi` value instead.\
    /// Decoded raw RGB or RGBA (depending on `self.colors` value) pixels are written into `output` slice.
    ///
    /// On success this function returns `Ok(())`.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
    #[inline(always)]
    pub fn decode_skip_header(&self, bytes: &[u8], output: &mut [u8]) -> Result<(), DecodeError> {
        match self.colors.has_alpha() {
            true => self.decode_impl::<Rgba>(bytes, output),
            false => self.decode_impl::<Rgb>(bytes, output),
        }
    }

    fn decode_impl<P>(&self, bytes: &[u8], output: &mut [u8]) -> Result<(), DecodeError>
    where
        P: Pixel,
    {
        debug_assert_eq!(self.colors.has_alpha(), P::HAS_ALPHA);

        let px_len = self.decoded_size();

        if self.width == 0 || self.height == 0 {
            return Ok(());
        }

        if px_len > output.len() {
            return Err(DecodeError::OutputIsTooSmall);
        }

        let mut px = P::new_opaque();
        let mut index = [P::new(); 64];

        let mut rest = &bytes[QOI_HEADER_SIZE..];
        let mut chunks = output[..px_len].chunks_exact_mut(P::CHANNELS);

        loop {
            match chunks.next() {
                None => {
                    cold();
                    break;
                }
                Some(chunk) => {
                    if likely(rest.len() & !7 != 0) {
                        match rest {
                            [0b11111110, b2, b3, b4, tail @ ..] => {
                                rest = tail;
                                px.set_r(*b2);
                                px.set_g(*b3);
                                px.set_b(*b4);
                            }
                            [0b11111111, b2, b3, b4, _, tail @ ..] if !P::HAS_ALPHA => {
                                cold();
                                rest = tail;
                                px.set_r(*b2);
                                px.set_g(*b3);
                                px.set_b(*b4);
                            }
                            [0b11111111, b2, b3, b4, b5, tail @ ..] => {
                                rest = tail;
                                px.set_r(*b2);
                                px.set_g(*b3);
                                px.set_b(*b4);
                                px.set_a(*b5);
                            }
                            [b1 @ 0b00000000..=0b00111111, tail @ ..] => {
                                rest = tail;
                                px = index[*b1 as usize];
                                px.write(chunk);

                                continue;
                            }
                            [b1 @ 0b01000000..=0b01111111, tail @ ..] => {
                                rest = tail;
                                px.set_r(px.r().wrapping_add(((b1 >> 4) & 0x03).wrapping_sub(2)));
                                px.set_g(px.g().wrapping_add(((b1 >> 2) & 0x03).wrapping_sub(2)));
                                px.set_b(px.b().wrapping_add((b1 & 0x03).wrapping_sub(2)));
                            }
                            [b1 @ 0b10000000..=0b10111111, b2, tail @ ..] => {
                                rest = tail;
                                let vg = (b1 & 0x3f).wrapping_sub(32);
                                let vr = ((b2 >> 4) & 0x0f).wrapping_sub(8).wrapping_add(vg);
                                let vb = (b2 & 0x0f).wrapping_sub(8).wrapping_add(vg);

                                px.set_r(px.r().wrapping_add(vr));
                                px.set_g(px.g().wrapping_add(vg));
                                px.set_b(px.b().wrapping_add(vb));
                            }
                            [b1 @ 0b11000000..=0b11111111, tail @ ..] => {
                                rest = tail;
                                let run = (b1 & 0x3f) as usize;
                                likely(chunks.len() >= run);

                                px.write(chunk);
                                chunks.by_ref().take(run).for_each(|chunk| px.write(chunk));

                                continue;
                            }
                            _ => {
                                cold();
                                unreachable!()
                            }
                        }
                    } else {
                        return Err(DecodeError::DataIsTooSmall);
                    }

                    index[px.hash() as usize] = px;

                    px.write(chunk);
                }
            }
        }

        Ok(())
    }

    /// Decode a QOI image from bytes slice.\
    /// Skips header reading, uses provided `Qoi` value instead.\
    /// Decoded raw RGB or RGBA pixels are written into allocated `Vec`.
    ///
    /// On success this function returns `Ok((qoi, vec))` with `qoi` describing image dimensions and color space and `vec` containing raw pixels data.\
    /// On failure this function returns `Err(err)` with `err` describing cause of the error.
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
