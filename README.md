# zvec

Rust bindings to [zvec](https://github.com/alibaba/zvec), an in-process vector
database by Alibaba.

The crate exposes the complete zvec C API as raw FFI (`zvec::sys`) generated
at build time by `bindgen`, plus a small set of safe wrappers for version
metadata and error reporting. The safe surface is intentionally minimal —
zvec's C API has hundreds of functions and wrapping each one safely is left
to consumers that need it.

## Status

Early work in progress. The raw FFI should be complete; the safe wrappers
are not.

## Requirements

- A prebuilt `libzvec_c_api` from zvec's own CMake build. Building zvec from
  source pulls in heavy third-party dependencies (RocksDB, Arrow, Protobuf,
  gflags, glog, …) and is not attempted from `build.rs`.
- `clang` / `libclang` for `bindgen` at build time.

## Linking

`build.rs` locates `libzvec_c_api` in the following order:

1. `ZVEC_LIB_DIR` — directory containing the library.
2. `ZVEC_ROOT` — install prefix; adds `$ZVEC_ROOT/lib` (and `lib64` if present)
   to the link search path.
3. `pkg-config` when the `pkg-config` cargo feature is enabled.
4. The system linker's default search paths.

Set `ZVEC_STATIC=1` to request static linking. Set `ZVEC_INCLUDE_DIR` (or
`ZVEC_ROOT`) to generate bindings against an installed header instead of the
`vendor/c_api.h` copy shipped with this crate.

## Example

```rust
fn main() {
    println!("zvec version: {}", zvec::version());
}
```

Run it against an installed zvec:

```sh
ZVEC_ROOT=/opt/zvec cargo run --example version
```

## Layout

- `vendor/c_api.h` — pinned copy of zvec's C API header.
- `src/sys.rs` — `include!` of the bindgen output.
- `src/error.rs`, `src/version.rs` — safe wrappers.
- `build.rs` — bindgen + linker discovery.

## License

Apache-2.0, matching upstream zvec.
