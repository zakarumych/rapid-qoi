use std::path::PathBuf;

use image::{DynamicImage, ImageFormat};

enum Format {
    Qoi,
    Raw,
    Image(image::ImageFormat),
}

fn main() -> Result<(), ()> {
    let mut args = std::env::args();

    if args.len() < 2 {
        eprintln!("Usage: qoiconv <input-path> [<output-path>]");
        eprintln!("Example: qoiconv images/foo.png images.foo.qoi");
        return Err(());
    }

    args.next();

    let input = PathBuf::from(args.next().unwrap());

    let input_format = match input.extension() {
        Some(ext) => match ext {
            _ if ext == "qoi" => Format::Qoi,
            _ if ext == "raw" => panic!("RAW is unsupported as input format"),
            _ => Format::Image(ImageFormat::from_extension(ext).ok_or_else(|| {
                eprintln!(
                    "Failed to pick output format based on extension '{:?}'",
                    ext
                )
            })?),
        },
        None => {
            eprintln!("Failed to pick output format without extension");
            return Err(());
        }
    };

    let output = args.next().map(PathBuf::from).unwrap_or_else(|| {
        if let Format::Qoi = input_format {
            input.with_extension("png")
        } else {
            input.with_extension("qoi")
        }
    });

    if output.exists() {
        eprintln!("Output path '{}' already occupied", output.display());
        return Err(());
    }

    let output_format = match output.extension() {
        Some(ext) => match ext {
            _ if ext == "qoi" => Format::Qoi,
            _ if ext == "raw" => Format::Raw,
            _ => Format::Image(ImageFormat::from_extension(ext).ok_or_else(|| {
                eprintln!(
                    "Failed to pick output format based on extension '{:?}'",
                    ext
                )
            })?),
        },
        None => {
            eprintln!("Failed to pick output format without extension");
            return Err(());
        }
    };

    let bytes = std::fs::read(&input)
        .map_err(|err| eprintln!("Failed to read QOI file '{}'. {:#}", input.display(), err))?;

    let dynamic_image = match input_format {
        Format::Qoi => {
            let (qoi, pixels) = rapid_qoi::Qoi::decode_alloc(&bytes).map_err(|err| {
                eprintln!(
                    "Failed to decode QOI image '{}'. {:#?}",
                    input.display(),
                    err
                )
            })?;

            match qoi.colors.has_alpha() {
                true => image::DynamicImage::ImageRgba8(
                    image::RgbaImage::from_raw(qoi.width, qoi.height, pixels).unwrap(),
                ),
                false => image::DynamicImage::ImageRgb8(
                    image::RgbImage::from_raw(qoi.width, qoi.height, pixels).unwrap(),
                ),
            }
        }
        Format::Raw => unreachable!(),
        Format::Image(format) => {
            image::load_from_memory_with_format(&bytes, format).map_err(|err| {
                eprintln!(
                    "Failed to open input image '{}'. {:#}",
                    input.display(),
                    err
                )
            })?
        }
    };

    match output_format {
        Format::Qoi => match dynamic_image {
            DynamicImage::ImageLuma16(_)
            | DynamicImage::ImageLuma8(_)
            | DynamicImage::ImageLumaA16(_)
            | DynamicImage::ImageLumaA8(_)
            | DynamicImage::ImageRgba16(_)
            | DynamicImage::ImageRgba8(_) => {
                let rgba = dynamic_image.to_rgba8();
                let pixels = rgba.as_raw();

                let qoi = rapid_qoi::Qoi {
                    width: rgba.width(),
                    height: rgba.height(),
                    colors: rapid_qoi::Colors::SrgbLinA,
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
                let rgb = dynamic_image.to_rgb8();
                let pixels = rgb.as_raw();

                let qoi = rapid_qoi::Qoi {
                    width: rgb.width(),
                    height: rgb.height(),
                    colors: rapid_qoi::Colors::Srgb,
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
        },

        Format::Raw => {
            std::fs::write(&output, &dynamic_image.as_bytes()).map_err(|err| {
                eprintln!(
                    "Failed to write RAW image into output file {}. {:#}",
                    output.display(),
                    err
                )
            })?;
        }

        Format::Image(format) => {
            dynamic_image
                .save_with_format(&output, format)
                .map_err(|err| {
                    eprintln!(
                        "Failed to save image into '{}'. {:#}",
                        output.display(),
                        err
                    )
                })?;
        }
    }

    Ok(())
}
