# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed
- `decode_range`: clamp the `QOI_OP_RUN` split to the remaining output length so
  chunk-by-chunk decoding (e.g. one row of output per call) no longer panics with
  `mid > len` when a run reaches past the current chunk's buffer edge. The
  remainder carries over via `*prun` into the next call, matching the
  already-clamped carried-over-run path.

## [0.5.0] - 2021-12-29

### Added
Methods for more flexible usage

## [0.4.3] - 2021-12-23

### Changed
`rapid-qoi` now follows finialized QOI spec.
No unsafe code.
Perf improvements.
