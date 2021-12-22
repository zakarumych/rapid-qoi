/*

Simple benchmark suite for png, stbi and qoi

Requires libpng, "stb_image.h" and "stb_image_write.h"
Compile with:
    gcc qoibench.c -std=gnu99 -lpng -O3 -o qoibench

Dominic Szablewski - https://phoboslab.org


-- LICENSE: The MIT License(MIT)

Copyright(c) 2021 Dominic Szablewski

Permission is hereby granted, free of charge, to any person obtaining a copy of
this software and associated documentation files(the "Software"), to deal in
the Software without restriction, including without limitation the rights to
use, copy, modify, merge, publish, distribute, sublicense, and / or sell copies
of the Software, and to permit persons to whom the Software is furnished to do
so, subject to the following conditions :
The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.
THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.

*/

use std::{
    fs::File,
    io::BufReader,
    path::Path,
    time::{Duration, Instant},
};

fn ns() -> Instant {
    Instant::now()
}

// -----------------------------------------------------------------------------
// benchmark runner

struct BenchmarkLibResult {
    size: u64,
    encode_time: Duration,
    decode_time: Duration,
}

struct BenchmarkResult {
    px: u64,
    w: u32,
    h: u32,
    qoi: BenchmarkLibResult,
    qoi_rs: BenchmarkLibResult,
    rapid_qoi: BenchmarkLibResult,
}

#[inline(never)]
fn benchmark_fn(runs: u32, avg_time: &mut Duration, mut f: impl FnMut()) {
    f();
    let time_start = ns();
    for _ in 0..runs {
        f();
    }
    let time = time_start.elapsed();
    *avg_time = time / runs;
}

fn benchmark_image(path: &Path, runs: u32) -> BenchmarkResult {
    let image = image::load(
        BufReader::new(File::open(path).unwrap()),
        image::ImageFormat::Png,
    )
    .unwrap();

    let image = image.to_rgba8();

    let w = image.width();
    let h = image.height();

    let pixels = image.as_raw();

    // Load the encoded PNG, encoded QOI and raw pixels into memory
    let encoded = rapid_qoi::Qoi {
        width: w,
        height: h,
        colors: rapid_qoi::Colors::SrgbLinA,
    }
    .encode_alloc(pixels)
    .unwrap();

    let encoded_qoi =
        qoi::QoiEncode::qoi_encode_to_vec(pixels, w, h, qoi::Channels::Four, 0).unwrap();

    let image_qoi_rs = qoi_rs::Image {
        pixels: pixels.clone().into_boxed_slice(),
        width: w as u16,
        height: h as u16,
    };

    let encoded_qoi_rs = qoi_rs::encode(image_qoi_rs, 4).unwrap();

    let mut res = BenchmarkResult {
        px: image.width() as u64 * image.height() as u64,
        w,
        h,
        qoi: BenchmarkLibResult {
            size: 0,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
        },
        qoi_rs: BenchmarkLibResult {
            size: 0,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
        },
        rapid_qoi: BenchmarkLibResult {
            size: 0,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
        },
    };

    // Decoding

    benchmark_fn(runs, &mut res.qoi.decode_time, || {
        qoi::QoiDecode::qoi_decode_to_vec(&encoded_qoi, Some(qoi::Channels::Four)).unwrap();
    });

    benchmark_fn(runs, &mut res.qoi_rs.decode_time, || {
        qoi_rs::decode(&encoded_qoi_rs, 4).unwrap();
    });

    benchmark_fn(runs, &mut res.rapid_qoi.decode_time, || {
        rapid_qoi::Qoi::decode_alloc(&encoded).unwrap();
    });

    // Encoding

    let size = &mut res.qoi.size;
    benchmark_fn(runs, &mut res.qoi.encode_time, || {
        let encoded =
            qoi::QoiEncode::qoi_encode_to_vec(pixels, w as u32, h as u32, qoi::Channels::Four, 0xf)
                .unwrap();
        *size = encoded.len() as u64;
    });

    let size = &mut res.qoi_rs.size;
    benchmark_fn(runs, &mut res.qoi_rs.encode_time, || {
        let image_qoi_rs = qoi_rs::Image {
            pixels: pixels.clone().into_boxed_slice(),
            width: w as u16,
            height: h as u16,
        };

        let encoded_qoi_rs = qoi_rs::encode(image_qoi_rs, 4).unwrap();
        *size = encoded_qoi_rs.len() as u64;
    });

    let size = &mut res.rapid_qoi.size;
    benchmark_fn(runs, &mut res.rapid_qoi.encode_time, || {
        let q = rapid_qoi::Qoi {
            width: w,
            height: h,
            colors: rapid_qoi::Colors::SrgbLinA,
        };
        let encoded = q.encode_alloc(pixels).unwrap();
        *size = encoded.len() as u64;
    });

    res
}

fn benchmark_print_result(head: &str, res: &BenchmarkResult) {
    let px = res.px as f64;
    println!("## {} size: {}x{}", head, res.w, res.h);
    println!("          decode ms   encode ms   decode mpps   encode mpps   size kb");
    println!(
        "qoi:       {:8.3}    {:8.3}      {:8.3}      {:8.3}  {:8}",
        res.qoi.decode_time.as_secs_f64() * 1000.0,
        res.qoi.encode_time.as_secs_f64() * 1000.0,
        if res.qoi.decode_time.is_zero() {
            0.0
        } else {
            px / (res.qoi.decode_time.as_secs_f64() * 1000_000.0)
        },
        if res.qoi.encode_time.is_zero() {
            0.0
        } else {
            px / (res.qoi.encode_time.as_secs_f64() * 1000_000.0)
        },
        res.qoi.size / 1024,
    );
    println!(
        "qoi_rs:    {:8.3}    {:8.3}      {:8.3}      {:8.3}  {:8}",
        res.qoi_rs.decode_time.as_secs_f64() * 1000.0,
        res.qoi_rs.encode_time.as_secs_f64() * 1000.0,
        if res.qoi_rs.decode_time.is_zero() {
            0.0
        } else {
            px / (res.qoi_rs.decode_time.as_secs_f64() * 1000_000.0)
        },
        if res.qoi_rs.encode_time.is_zero() {
            0.0
        } else {
            px / (res.qoi_rs.encode_time.as_secs_f64() * 1000_000.0)
        },
        res.qoi_rs.size / 1024,
    );
    println!(
        "rapid_qoi: {:8.3}    {:8.3}      {:8.3}      {:8.3}  {:8}",
        res.rapid_qoi.decode_time.as_secs_f64() * 1000.0,
        res.rapid_qoi.encode_time.as_secs_f64() * 1000.0,
        if res.rapid_qoi.decode_time.is_zero() {
            0.0
        } else {
            px / (res.rapid_qoi.decode_time.as_secs_f64() * 1000_000.0)
        },
        if res.rapid_qoi.encode_time.is_zero() {
            0.0
        } else {
            px / (res.rapid_qoi.encode_time.as_secs_f64() * 1000_000.0)
        },
        res.rapid_qoi.size / 1024,
    );
    println!();
}

fn main() -> Result<(), ()> {
    let mut args = std::env::args();

    if args.len() < 3 {
        eprintln!("Usage: qoibench <iterations> <directory>");
        eprintln!("Example: qoibench 10 images/textures/");
        return Err(());
    }

    let mut totals = BenchmarkResult {
        px: 0,
        w: 0,
        h: 0,
        qoi: BenchmarkLibResult {
            size: 0,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
        },
        qoi_rs: BenchmarkLibResult {
            size: 0,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
        },
        rapid_qoi: BenchmarkLibResult {
            size: 0,
            encode_time: Duration::ZERO,
            decode_time: Duration::ZERO,
        },
    };

    args.next();
    let mut runs = args.next().unwrap().parse().unwrap();
    if runs < 1 {
        runs = 1;
    }

    let dirpath = args.next().unwrap();

    let dir = std::fs::read_dir(&dirpath).expect("Couldn't open directory");

    println!("## Benchmarking {}/*.png -- {} runs", dirpath, runs);
    println!();

    let mut count = 0;
    for path in dir {
        let path = path.unwrap();
        if path.file_type().unwrap().is_file() {
            let filepath = Path::new(&dirpath).join(path.file_name());

            if filepath.extension().map_or(false, |e| e == "png") {
                count += 1;
                let res = benchmark_image(&filepath, runs);
                benchmark_print_result(filepath.to_string_lossy().as_ref(), &res);

                totals.px += res.px;

                totals.qoi.encode_time += res.qoi.encode_time;
                totals.qoi.decode_time += res.qoi.decode_time;
                totals.qoi.size += res.qoi.size;

                totals.qoi_rs.encode_time += res.qoi_rs.encode_time;
                totals.qoi_rs.decode_time += res.qoi_rs.decode_time;
                totals.qoi_rs.size += res.qoi_rs.size;

                totals.rapid_qoi.encode_time += res.rapid_qoi.encode_time;
                totals.rapid_qoi.decode_time += res.rapid_qoi.decode_time;
                totals.rapid_qoi.size += res.rapid_qoi.size;
            }
        }
    }

    totals.px /= count as u64;

    totals.qoi.encode_time /= count;
    totals.qoi.decode_time /= count;
    totals.qoi.size /= count as u64;

    totals.qoi_rs.encode_time /= count;
    totals.qoi_rs.decode_time /= count;
    totals.qoi_rs.size /= count as u64;

    totals.rapid_qoi.encode_time /= count;
    totals.rapid_qoi.decode_time /= count;
    totals.rapid_qoi.size /= count as u64;

    benchmark_print_result("Totals (AVG)", &totals);
    Ok(())
}
