//! Raw FFI bindings to the zvec C API.
//!
//! Generated at build time by [`bindgen`] from `vendor/c_api.h` (or from the
//! system header if `ZVEC_INCLUDE_DIR` / `ZVEC_ROOT` is set). Every symbol
//! here is `unsafe` to call — consult the upstream C API documentation for
//! ownership, lifetime, and threading contracts.

#![allow(non_camel_case_types)]
#![allow(non_snake_case)]
#![allow(non_upper_case_globals)]
#![allow(dead_code)]
#![allow(clippy::all)]

include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
