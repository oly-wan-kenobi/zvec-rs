# zvec

Rust bindings to [zvec](https://github.com/alibaba/zvec), Alibaba's
lightweight in-process vector database.

The crate has two layers:

- **Raw FFI** (`zvec::sys`) generated at build time by `bindgen` from a pinned
  `c_api.h`.
- **Safe wrappers** over the full public C API — schemas, index parameters,
  collections, documents, queries, stats, config.

## Quickstart

```rust
use zvec::{
    Collection, CollectionSchema, DataType, Doc, FieldSchema, IndexParams,
    IndexType, MetricType, VectorQuery,
};

fn main() -> zvec::Result<()> {
    let mut schema = CollectionSchema::new("docs")?;

    let mut invert = IndexParams::new(IndexType::Invert)?;
    invert.set_invert_params(true, false)?;
    let mut id = FieldSchema::new("id", DataType::String, false, 0)?;
    id.set_index_params(&invert)?;
    schema.add_field(&id)?;

    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;
    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;

    let collection = Collection::create_and_open("./my_coll", &schema, None)?;

    let mut doc = Doc::new()?;
    doc.set_pk("doc1")?;
    doc.add_string("id", "doc1")?;
    doc.add_vector_fp32("embedding", &[0.1, 0.2, 0.3])?;
    collection.insert(&[&doc])?;
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[0.1, 0.2, 0.3])?;
    q.set_topk(10)?;
    for row in collection.query(&q)?.iter() {
        println!("{:?} score={}", row.pk_copy(), row.score());
    }
    Ok(())
}
```

See `examples/basic.rs` for the Rust port of zvec's own `basic_example.c`.

## Zero-setup builds with the `bundled` feature

The simplest way to try the crate is to enable the `bundled` cargo feature,
which downloads the upstream zvec PyPI wheel at build time, verifies its
SHA-256, extracts `libzvec_c_api` + `c_api.h` into `$OUT_DIR`, and wires the
linker to point at it. No external zvec build needed.

```toml
[dependencies]
zvec = { version = "0.1", features = ["bundled"] }
```

```sh
cargo test --features bundled
```

Supported targets (matching the wheels upstream publishes):

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Any other target falls through to the source-build / env-var path below.

Overrides:

- `ZVEC_BUNDLED_WHEEL_URL` + `ZVEC_BUNDLED_WHEEL_SHA256` — fetch a different
  wheel (e.g. a newer release or a mirror).
- `ZVEC_BUNDLED_WHEEL_PATH` — skip the network and use a local wheel file
  (useful for air-gapped or TLS-restricted builds).

## Building `libzvec_c_api` from source

The crate links against `libzvec_c_api` — a shared library produced by zvec's
own CMake build. Use the helper script in this repo; it pins zvec to the
version whose header is vendored under `vendor/c_api.h`:

```sh
# Clones alibaba/zvec @ v0.3.1 with submodules, builds the zvec_c_api target,
# and installs to ./zvec-install/{lib,include}.
./scripts/build-zvec.sh "$PWD/zvec-install"

export ZVEC_ROOT="$PWD/zvec-install"
export LD_LIBRARY_PATH="$ZVEC_ROOT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cargo test
```

What you need on the build host:

- `cmake ≥ 3.13`, `ninja` (or `make`), a C++17 compiler, `git`, `patch`.
- `libclang` / `clang` (for `bindgen` when compiling this crate).
- Roughly **20–30 minutes** for a clean source build: zvec vendors RocksDB,
  Apache Arrow, Protobuf, glog, gflags, ANTLR, LZ4, CRoaring, and RaBitQ as
  git submodules and compiles them all.

Overrides the script respects:

| Variable          | Default                              | Purpose                          |
|-------------------|--------------------------------------|----------------------------------|
| `ZVEC_REF`        | `v0.3.1`                             | Git ref to check out             |
| `ZVEC_REPO`       | `https://github.com/alibaba/zvec`    | Upstream repository URL          |
| `ZVEC_SRC_DIR`    | (clone into `.zvec-build-work/`)     | Use an existing checkout         |
| `ZVEC_BUILD_DIR`  | `.zvec-build-work/build`             | CMake build directory            |
| `CMAKE_GENERATOR` | `Unix Makefiles`                     | e.g. `Ninja`                     |
| `JOBS`            | `nproc`                              | Parallel build jobs              |

If you already have zvec installed in a custom prefix, skip the script and
point `ZVEC_ROOT` (or `ZVEC_LIB_DIR` / `ZVEC_INCLUDE_DIR` individually) at it.

## Build-time configuration for the crate

`build.rs` locates `libzvec_c_api` in this order:

1. `ZVEC_LIB_DIR` — directory containing the library.
2. `ZVEC_ROOT` — install prefix; adds `$ZVEC_ROOT/lib` (and `lib64` if present)
   to the link search path.
3. `pkg-config` when the `pkg-config` cargo feature is enabled.
4. The system linker's default search paths.

Other env vars:

- `ZVEC_STATIC=1` — link statically.
- `ZVEC_INCLUDE_DIR` — generate bindings against an installed header instead
  of `vendor/c_api.h`.

## Running the examples and tests

```sh
# Version info.
cargo run --example version

# Port of basic_example.c (creates a collection in $TMPDIR).
cargo run --example basic

# Integration tests.
cargo test
```

Each command needs `ZVEC_ROOT` (or `ZVEC_LIB_DIR`) and `LD_LIBRARY_PATH` set
as above.

## Layout

- `vendor/c_api.h` — pinned copy of zvec's public C API.
- `scripts/build-zvec.sh` — reproducible source build helper.
- `build.rs` — bindgen + linker discovery.
- `src/sys.rs` — raw FFI.
- `src/{collection,doc,schema,query,query_params,index_params,options,stats,config,types,error,version,ffi_util}.rs`
  — safe wrappers.
- `examples/` — `version` (minimal) and `basic` (port of the upstream C
  example).
- `tests/integration.rs` — end-to-end roundtrip tests.
- `.github/workflows/ci.yml` — builds zvec from source, caches the install
  prefix, then runs fmt/clippy/tests.

## License

Apache-2.0, matching upstream zvec.
