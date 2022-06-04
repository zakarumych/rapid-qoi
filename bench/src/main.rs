//! Simple benchmark suite for qoi crates

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
    count: u32,
    px: u64,
    w: u32,
    h: u32,
    qoi: BenchmarkLibResult,
    rapid_qoi: BenchmarkLibResult,
}

#[inline(never)]
fn benchmark_fn(runs: u32, avg_time: &mut Duration, mut f: impl FnMut()) {
    f();

    let mut time = Duration::ZERO;
    for _ in 0..runs {
        let time_start = ns();
        f();
        time += time_start.elapsed();
    }

    *avg_time = time / runs;
}

fn benchmark_image(path: &Path, runs: u32) -> BenchmarkResult {
    let mut res = BenchmarkResult {
        count: 0,
        px: 0,
        w: 0,
        h: 0,
        qoi: BenchmarkLibResult {
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

    let image = match image::load(
        BufReader::new(File::open(path).unwrap()),
        image::ImageFormat::Png,
    ) {
        Ok(image) => image,
        Err(err) => {
            eprintln!("Failed to open PNG image {} : {:#}", path.display(), err);
            return res;
        }
    };

    let image = image.to_rgba8();

    let w = image.width();
    let h = image.height();

    res.count = 1;
    res.px = image.width() as u64 * image.height() as u64;
    res.w = w;
    res.h = h;

    let pixels = image.as_raw();

    // Load the encoded PNG, encoded QOI and raw pixels into memory
    let encoded = rapid_qoi::Qoi {
        width: w,
        height: h,
        colors: rapid_qoi::Colors::SrgbLinA,
    }
    .encode_alloc(pixels)
    .unwrap();

    // Decoding

    benchmark_fn(runs, &mut res.qoi.decode_time, || {
        qoi::decode_to_vec(&encoded).unwrap();
    });

    benchmark_fn(runs, &mut res.rapid_qoi.decode_time, || {
        rapid_qoi::Qoi::decode_alloc(&encoded).unwrap();
    });

    // Encoding

    let size = &mut res.qoi.size;
    benchmark_fn(runs, &mut res.qoi.encode_time, || {
        let encoded = qoi::encode_to_vec(pixels, w as u32, h as u32).unwrap();
        *size = encoded.len() as u64;
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

fn benchmark_print_result(res: &BenchmarkResult) {
    let px = res.px as f64;
    println!("          decode ms   encode ms   decode mpps   encode mpps   size kb");
    println!(
        "qoi:       {:8.3}    {:8.3}      {:8.3}      {:8.3}  {:8}",
        res.qoi.decode_time.as_secs_f64() * 1000.0,
        res.qoi.encode_time.as_secs_f64() * 1000.0,
        if res.qoi.decode_time.is_zero() {
            0.0
        } else {
            px / (res.qoi.decode_time.as_secs_f64() * 1_000_000.0)
        },
        if res.qoi.encode_time.is_zero() {
            0.0
        } else {
            px / (res.qoi.encode_time.as_secs_f64() * 1_000_000.0)
        },
        res.qoi.size / 1024,
    );
    println!(
        "rapid_qoi: {:8.3}    {:8.3}      {:8.3}      {:8.3}  {:8}",
        res.rapid_qoi.decode_time.as_secs_f64() * 1000.0,
        res.rapid_qoi.encode_time.as_secs_f64() * 1000.0,
        if res.rapid_qoi.decode_time.is_zero() {
            0.0
        } else {
            px / (res.rapid_qoi.decode_time.as_secs_f64() * 1_000_000.0)
        },
        if res.rapid_qoi.encode_time.is_zero() {
            0.0
        } else {
            px / (res.rapid_qoi.encode_time.as_secs_f64() * 1_000_000.0)
        },
        res.rapid_qoi.size / 1024,
    );
    println!();
}

fn benchmark_directory(dirpath: &Path, runs: u32, grand_total: &mut BenchmarkResult) {
    println!(
        "## Benchmarking {}/*.png -- {} runs",
        dirpath.display(),
        runs
    );

    let dir = std::fs::read_dir(dirpath).expect("Couldn't open directory");

    let mut dir_total = BenchmarkResult {
        count: 0,
        px: 0,
        w: 0,
        h: 0,
        qoi: BenchmarkLibResult {
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

    for path in dir {
        let path = path.unwrap();
        let ft = path.file_type().unwrap();
        if ft.is_file() {
            let filepath = Path::new(dirpath).join(path.file_name());

            if filepath.extension().map_or(false, |e| e == "png") {
                let res = benchmark_image(&filepath, runs);

                dir_total.count += res.count;
                dir_total.px += res.px;

                dir_total.qoi.encode_time += res.qoi.encode_time;
                dir_total.qoi.decode_time += res.qoi.decode_time;
                dir_total.qoi.size += res.qoi.size;

                dir_total.rapid_qoi.encode_time += res.rapid_qoi.encode_time;
                dir_total.rapid_qoi.decode_time += res.rapid_qoi.decode_time;
                dir_total.rapid_qoi.size += res.rapid_qoi.size;

                grand_total.qoi.encode_time += res.qoi.encode_time;
                grand_total.qoi.decode_time += res.qoi.decode_time;
                grand_total.qoi.size += res.qoi.size;

                grand_total.rapid_qoi.encode_time += res.rapid_qoi.encode_time;
                grand_total.rapid_qoi.decode_time += res.rapid_qoi.decode_time;
                grand_total.rapid_qoi.size += res.rapid_qoi.size;

                grand_total.count += res.count;
                grand_total.px += res.px;

                // println!("## {} size: {}x{}\n", filepath.display(), res.w, res.h);
                // benchmark_print_result(&res);
            }
        } else if ft.is_dir() {
            let subdirpath = Path::new(dirpath).join(path.file_name());
            benchmark_directory(&subdirpath, runs, grand_total);
        }
    }

    if dir_total.count > 0 {
        dir_total.px /= dir_total.count as u64;

        dir_total.qoi.encode_time /= dir_total.count;
        dir_total.qoi.decode_time /= dir_total.count;
        dir_total.qoi.size /= dir_total.count as u64;

        dir_total.rapid_qoi.encode_time /= dir_total.count;
        dir_total.rapid_qoi.decode_time /= dir_total.count;
        dir_total.rapid_qoi.size /= dir_total.count as u64;

        println!("## Total for {}\n", dirpath.display());
        benchmark_print_result(&dir_total);
    }
}

fn main() -> Result<(), ()> {
    let mut args = std::env::args();

    if args.len() < 3 {
        eprintln!("Usage: qoibench <iterations> <directory>");
        eprintln!("Example: qoibench 10 images/textures/");
        return Err(());
    }

    args.next();
    let mut runs = args.next().unwrap().parse().unwrap();
    if runs < 1 {
        runs = 1;
    }

    let dirpath = args.next().unwrap();

    let mut grand_total = BenchmarkResult {
        count: 0,
        px: 0,
        w: 0,
        h: 0,
        qoi: BenchmarkLibResult {
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

    benchmark_directory(dirpath.as_ref(), runs, &mut grand_total);

    println!();

    if grand_total.count > 0 {
        grand_total.px /= grand_total.count as u64;

        grand_total.qoi.encode_time /= grand_total.count;
        grand_total.qoi.decode_time /= grand_total.count;
        grand_total.qoi.size /= grand_total.count as u64;

        grand_total.rapid_qoi.encode_time /= grand_total.count;
        grand_total.rapid_qoi.decode_time /= grand_total.count;
        grand_total.rapid_qoi.size /= grand_total.count as u64;

        println!("# Grand total for {}\n", dirpath);
        benchmark_print_result(&grand_total);
    }

    Ok(())
}
