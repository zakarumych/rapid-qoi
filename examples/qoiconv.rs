use std::{fs::File, io::BufReader, path::PathBuf};

use image::{DynamicImage, ImageFormat};

fn main() -> Result<(), ()> {
    let mut args = std::env::args();

    if args.len() < 2 {
        eprintln!("Usage: qoiconv <input-path> [<output-path>]");
        eprintln!("Example: qoiconv images/foo.png images.foo.qoi");
        return Err(());
    }

    args.next();

    let input = PathBuf::from(args.next().unwrap());

    let decode = input.extension().map_or(false, |ext| ext == "qoi");

    let output = args.next().map(PathBuf::from).unwrap_or_else(|| {
        if decode {
            input.with_extension("png")
        } else {
            input.with_extension("qoi")
        }
    });

    if output.exists() {
        eprintln!("Output path '{}' already occupied", output.display());
        return Err(());
    }

    if decode {
        let bytes = std::fs::read(&input)
            .map_err(|err| eprintln!("Failed to read QOI file '{}'. {:#}", input.display(), err))?;

        let (qoi, pixels) = rapid_qoi::Qoi::decode_alloc(&bytes).map_err(|err| {
            eprintln!(
                "Failed to decode QOI image '{}'. {:#?}",
                input.display(),
                err
            )
        })?;

        let format = match output.extension() {
            Some(ext) => ImageFormat::from_extension(ext).ok_or_else(|| {
                eprintln!(
                    "Failed to pick output format based on extension '{:?}'",
                    ext
                )
            })?,
            None => {
                eprintln!("Failed to pick output format without extension");
                return Err(());
            }
        };

        match qoi.color_space.alpha_srgb.is_some() {
            true => image::save_buffer_with_format(
                &output,
                &pixels,
                qoi.width,
                qoi.height,
                image::ColorType::Rgba8,
                format,
            ),
            false => image::save_buffer_with_format(
                &output,
                &pixels,
                qoi.width,
                qoi.height,
                image::ColorType::Rgb8,
                format,
            ),
        }
        .map_err(|err| {
            eprintln!(
                "Failed to save decoded image into '{}'. {:#}",
                output.display(),
                err
            )
        })?;
    } else {
        let image = image::load(
            BufReader::new(File::open(&input).unwrap()),
            image::ImageFormat::Png,
        )
        .map_err(|err| {
            eprintln!(
                "Failed to open input image '{}'. {:#}",
                input.display(),
                err
            )
        })?;

        match image {
            DynamicImage::ImageBgra8(_)
            | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageLumaA8(_)
            | DynamicImage::ImageRgba16(_)
            | DynamicImage::ImageRgba8(_) => {
                let rgba = image.to_rgba8();
                let pixels = rgba.as_raw();

                let qoi = rapid_qoi::Qoi {
                    width: rgba.width(),
                    height: rgba.height(),
                    color_space: rapid_qoi::ColorSpace::SRGBA,
                };

                let bytes = qoi.encode_alloc(pixels).map_err(|err| {
                    eprintln!(
                        "Failed to encode QOI image '{}. {:#?}",
                        input.display(),
                        err
                    )
                })?;

                std::fs::write(&output, &bytes).map_err(|err| {
                    eprintln!(
                        "Failed to write QOI image into output file {}. {:#}",
                        output.display(),
                        err
                    )
                })?;
            }
            _ => {
                let rgb = image.to_rgb8();
                let pixels = rgb.as_raw();

                let qoi = rapid_qoi::Qoi {
                    width: rgb.width(),
                    height: rgb.height(),
                    color_space: rapid_qoi::ColorSpace::SRGB,
                };

                let bytes = qoi.encode_alloc(pixels).map_err(|err| {
                    eprintln!(
                        "Failed to encode QOI image '{}. {:#?}",
                        input.display(),
                        err
                    )
                })?;

                std::fs::write(&output, &bytes).map_err(|err| {
                    eprintln!(
                        "Failed to write QOI image into output file {}. {:#}",
                        output.display(),
                        err
                    )
                })?;
            }
        }
    }

    Ok(())
}
