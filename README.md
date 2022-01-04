# rapid-qoi

[![crates](https://img.shields.io/crates/v/rapid-qoi.svg?style=for-the-badge&label=rapid-qoi)](https://crates.io/crates/rapid-qoi)
[![docs](https://img.shields.io/badge/docs.rs-rapid--qoi-66c2a5?style=for-the-badge&labelColor=555555&logoColor=white)](https://docs.rs/rapid-qoi)
[![actions](https://img.shields.io/github/workflow/status/zakarumych/rapid-qoi/badge/master?style=for-the-badge)](https://github.com/zakarumych/rapid-qoi/actions?query=workflow%3ARust)
[![MIT/Apache](https://img.shields.io/badge/license-MIT%2FApache-blue.svg?style=for-the-badge)](COPYING)
![loc](https://img.shields.io/tokei/lines/github/zakarumych/rapid-qoi?style=for-the-badge)

Fast implementation of QOI format.
Reference implementation is here 'https://github.com/phoboslab/qoi'

`rapid-qoi` is
* no deps
* no std
* no unsafe
* tiny
* fast to build (0.8 sec clean build on i9)
* one of the most efficient implementations of QOI encoder and decoder.

  ```
  # Grand total for qoi benchmark suite
  # https://qoiformat.org/benchmark/qoi_benchmark_suite.tar

            decode ms   encode ms   decode mpps   encode mpps
  ## Intel i9
  qoi:          2.009       2.706        231.01        171.52
  rapid_qoi:    1.404       2.520        330.72        184.23
  
  ## Apple M1
  qoi:          1.676       2.088        277.01        222.26
  rapid_qoi:    1.124       1.776        413.13        261.40
  ```
  See [benches](./benches) for full reports.\
  Run `cargo run --release -p bench -- [iterations] [path]`
  

## License

Licensed under either of

* Apache License, Version 2.0, ([license/APACHE](license/APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
* MIT license ([license/MIT](license/MIT) or http://opensource.org/licenses/MIT)

at your option.

## Contributions

Unless you explicitly state otherwise, any contribution intentionally submitted for inclusion in the work by you, as defined in the Apache-2.0 license, shall be dual licensed as above, without any additional terms or conditions.
