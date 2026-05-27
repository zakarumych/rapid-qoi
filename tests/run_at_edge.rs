//! Regression test for the `QOI_OP_RUN` clamp.
//!
//! When an image is decoded chunk-by-chunk (e.g. one row of output at a time),
//! a `QOI_OP_RUN` chunk can describe more pixels than fit in the remaining
//! `pixels` slice for the current chunk. The run arm in `decode_range` must
//! clamp the split to the slice length and carry the remainder over (via
//! `*prun`) into the next call, exactly like the already-clamped carried-over
//! path does. Without the clamp, `pixels.split_at_mut(run)` panics with
//! `mid > len`.

use rapid_qoi::{Pixel, Qoi};

/// Build a tiny QOI (RGB) whose payload is one `QOI_OP_RGB` followed by a long
/// `QOI_OP_RUN` that spans the whole rest of the image — so when decoded one
/// row at a time the run reaches past each row's `pixels` buffer edge.
fn run_at_edge_qoi(width: u32, height: u32) -> Vec<u8> {
    let mut q = Vec::new();
    q.extend_from_slice(b"qoif");
    q.extend_from_slice(&width.to_be_bytes());
    q.extend_from_slice(&height.to_be_bytes());
    q.push(3); // channels: RGB
    q.push(0); // colorspace: sRGB

    // First pixel via QOI_OP_RGB (black), then runs covering the remainder.
    q.extend_from_slice(&[0xFE, 0, 0, 0]);
    let mut remaining = (width * height - 1) as usize;
    while remaining > 0 {
        let chunk = remaining.min(62); // QOI_OP_RUN encodes 1..=62
        q.push(0b1100_0000 | (chunk as u8 - 1));
        remaining -= chunk;
    }

    // QOI end marker.
    q.extend_from_slice(&[0, 0, 0, 0, 0, 0, 0, 1]);
    q
}

#[test]
fn run_reaching_chunk_edge_does_not_panic() {
    // 8x8 RGB: each row is 8 pixels; a run of up to 62 pixels spans many rows.
    let (w, h) = (8u32, 8u32);
    let bytes = run_at_edge_qoi(w, h);

    let qoi = Qoi::decode_header(&bytes).expect("header");
    assert_eq!(qoi.width, w);
    assert_eq!(qoi.height, h);

    let channels = qoi.colors.channels();
    let row_bytes = w as usize * channels;
    let total = row_bytes * h as usize;
    let payload = &bytes[14..];

    // Decode one row at a time through the chunked entry point.
    let mut output = vec![0u8; total];
    let mut index = [<[u8; 3] as Pixel>::new(); 64];
    let mut px = <[u8; 3] as Pixel>::new_opaque();
    let mut run = 0usize;
    let mut offset = 0usize;

    for r in 0..h as usize {
        let rs = r * row_bytes;
        let re = rs + row_bytes;
        let consumed = Qoi::decode_range::<3>(
            &mut index,
            &mut px,
            &mut run,
            &payload[offset..],
            &mut output[rs..re],
        )
        .expect("decode_range row");
        offset += consumed;
    }

    // The whole image is black (RGB 0,0,0): the RGB op set black, the run repeats it.
    assert!(output.iter().all(|&b| b == 0), "all pixels must be black");

    // Cross-check against a single whole-buffer decode of the same bytes.
    let (_q, whole) = Qoi::decode_alloc(&bytes).expect("decode_alloc");
    assert_eq!(
        output, whole,
        "chunked decode must match whole-image decode"
    );
}

#[test]
fn run_at_edge_matches_per_pixel_rows() {
    // A run that ends exactly on a row boundary, then continues into the next.
    let (w, h) = (4u32, 4u32);
    let bytes = run_at_edge_qoi(w, h);
    let (_q, whole) = Qoi::decode_alloc(&bytes).expect("decode_alloc");

    let channels = 3usize;
    let row_bytes = w as usize * channels;
    let payload = &bytes[14..];
    let mut output = vec![0u8; row_bytes * h as usize];
    let mut index = [<[u8; 3] as Pixel>::new(); 64];
    let mut px = <[u8; 3] as Pixel>::new_opaque();
    let mut run = 0usize;
    let mut offset = 0usize;
    for r in 0..h as usize {
        let rs = r * row_bytes;
        let re = rs + row_bytes;
        let consumed = Qoi::decode_range::<3>(
            &mut index,
            &mut px,
            &mut run,
            &payload[offset..],
            &mut output[rs..re],
        )
        .expect("decode_range row");
        offset += consumed;
    }
    assert_eq!(output, whole);
}
