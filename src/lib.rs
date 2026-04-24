//! Rust bindings to [zvec], an in-process vector database by Alibaba.
//!
//! This crate exposes the full zvec C API as raw FFI in [`sys`], plus a small
//! set of safe wrappers for version metadata and error reporting. The safe
//! surface is intentionally thin — the zvec C API has hundreds of functions,
//! and wrapping each one safely is left to consumers who need it.
//!
//! # Linking
//!
//! The crate expects a prebuilt `libzvec_c_api` (produced by zvec's own CMake
//! build) to be available at link time. It finds the library through, in
//! order:
//!
//! 1. `ZVEC_LIB_DIR` — explicit directory containing `libzvec_c_api`.
//! 2. `ZVEC_ROOT` — an install prefix; uses `$ZVEC_ROOT/lib` (+ `lib64`).
//! 3. `pkg-config` with the `pkg-config` cargo feature enabled.
//! 4. The system linker's default search paths.
//!
//! Set `ZVEC_STATIC=1` to request static linking instead of the default
//! dynamic linking. Set `ZVEC_INCLUDE_DIR` or `ZVEC_ROOT` to use a header
//! from an installed copy rather than the one vendored in this crate.
//!
//! [zvec]: https://github.com/alibaba/zvec

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod sys;

mod error;
mod version;

pub use error::{ErrorCode, ZvecError};
pub use version::{version, version_major, version_minor, version_patch, check_version};
