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

## Requirements

- A prebuilt `libzvec_c_api` on the link path.
- `clang` / `libclang` for `bindgen` at build time.

## Getting `libzvec_c_api`

The easiest source is the official Python wheel — it ships a fully built
`libzvec_c_api.so` (plus the static archives and headers) inside `lib/`:

```sh
pip download zvec --no-deps --only-binary=:all: --dest zvec-wheel
cd zvec-wheel && unzip -q zvec-*.whl -d extracted
export ZVEC_LIB_DIR="$PWD/extracted/lib"
export LD_LIBRARY_PATH="$ZVEC_LIB_DIR:${LD_LIBRARY_PATH:-}"
```

Alternatives:

- Build zvec from source via its CMake (heavy: pulls in RocksDB, Arrow,
  Protobuf, glog, gflags, ANTLR, LZ4, CRoaring, …). Install, then point
  `ZVEC_ROOT` at the prefix.
- Link against a system-installed zvec. `pkg-config` lookup is available
  behind the optional `pkg-config` cargo feature.

## Build-time configuration

`build.rs` locates the library in this order:

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

Each command needs `ZVEC_LIB_DIR` and `LD_LIBRARY_PATH` set as above.

## Layout

- `vendor/c_api.h` — pinned copy of zvec's public C API.
- `build.rs` — bindgen + linker discovery.
- `src/sys.rs` — raw FFI.
- `src/{collection,doc,schema,query,query_params,index_params,options,stats,config,types,error,version,ffi_util}.rs`
  — safe wrappers.
- `examples/` — `version` (minimal) and `basic` (port of the upstream C
  example).
- `tests/integration.rs` — end-to-end roundtrip tests.

## License

Apache-2.0, matching upstream zvec.
