# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- GitLab CI pipeline with 6 stages: check, test, secret-detection, security, build, release.
- Cross-compiled release binaries for Linux x86_64, Linux ARM64, and Windows x86_64 (MinGW).
- Local podman-based testing via `scripts/Containerfile.*`.
- License audit with `cargo-deny` and vulnerability scanning with `cargo-audit`.
- Dual licensing under MIT OR Apache-2.0.

### Changed

- Switched `matrix-sdk` from `native-tls` to `rustls-tls` + `bundled-sqlite` for portable cross-compilation.
- Pinned Rust toolchain to 1.93.1 to work around matrix-sdk query depth overflow.

## [0.1.0] - 2026-04-27

### Added

- Initial release of `mxsend`.
- Command-line interface for sending Matrix messages.
- Support for direct messages (DM) and room messages.
- Optional end-to-end encryption (E2EE) via recovery key.
- Environment variable configuration (`MXSEND_*`).
- Cross-platform support (Linux x86_64/ARM64, Windows x86_64).

## [0.1.0] - 2026-04-27

### Added

- First stable release.
