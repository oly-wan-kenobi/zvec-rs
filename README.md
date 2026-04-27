# zvec-rs

[![CI](https://github.com/oly-wan-kenobi/zvec-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/oly-wan-kenobi/zvec-rs/actions/workflows/ci.yml)
[![docs.rs](https://img.shields.io/docsrs/zvec?label=docs.rs)](https://docs.rs/zvec)
[![crates.io](https://img.shields.io/crates/v/zvec.svg?label=crates.io)](https://crates.io/crates/zvec)
[![Downloads](https://img.shields.io/crates/d/zvec.svg?label=downloads)](https://crates.io/crates/zvec)
[![Dependencies](https://deps.rs/crate/zvec/latest/status.svg)](https://deps.rs/crate/zvec)
[![OpenSSF Scorecard](https://api.securityscorecards.dev/projects/github.com/oly-wan-kenobi/zvec-rs/badge)](https://securityscorecards.dev/viewer/?uri=github.com/oly-wan-kenobi/zvec-rs)
[![License: Apache-2.0](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
![MSRV 1.86](https://img.shields.io/badge/MSRV-1.86-informational.svg)

Safe, idiomatic Rust bindings to [zvec](https://github.com/alibaba/zvec) —
Alibaba's lightweight, in-process vector database.

- **Full coverage** of upstream's C API: schemas, index params, DDL/DML/DQL,
  hybrid retrieval, result fusion, stats.
- **RAII everywhere:** every owning wrapper frees its C-side handle on drop,
  every fallible call returns `Result<T, ZvecError>`.
- **Three ways to link:** zero-setup `bundled` feature (pulls upstream's
  PyPI wheel), reproducible source build via a helper script, or point at an
  existing install.
- **Optional niceties:** `tokio` async wrapper, `#[derive(IntoDoc)]` /
  `#[derive(FromDoc)]` macros, JSON ingest, half-precision vectors.

Pinned zvec version: **v0.3.1**.

---

## Contents

- [Quickstart](#quickstart)
- [Install](#install)
  - [A. `bundled` feature (recommended)](#a-bundled-feature-recommended)
  - [B. Build from source](#b-build-from-source)
  - [C. Point at an existing install](#c-point-at-an-existing-install)
- [Cargo features](#cargo-features)
- [Environment variables](#environment-variables)
- [Examples](#examples)
- [API overview](#api-overview)
- [Thread safety](#thread-safety)
- [Comparison to `igobypenn/zvec-rust-binding`](#comparison-to-igobypennzvec-rust-binding)
- [Contributing](#contributing)
- [License](#license)

---

## Quickstart

```toml
[dependencies]
zvec = { version = "0.1", features = ["bundled"] }
```

```rust
use zvec::{Collection, CollectionSchema, Doc, FieldSchema, MetricType, VectorQuery};

fn main() -> zvec::Result<()> {
    // Describe the collection: a string `id` field with an inverted index,
    // plus a 3-D fp32 vector field indexed with HNSW (M=16, efConstruction=200)
    // using cosine similarity.
    let schema = CollectionSchema::builder("docs")
        .field(FieldSchema::string("id").invert_index(true, false))
        .field(
            FieldSchema::vector_fp32("embedding", 3)
                .hnsw(16, 200)
                .metric(MetricType::Cosine),
        )
        .build()?;

    // Create the on-disk collection at `./my_coll` and open a handle to it.
    // `None` accepts the default `CollectionOptions`.
    let collection = Collection::create_and_open("./my_coll", &schema, None)?;

    // Build a single document. Every doc needs a primary key (`set_pk`);
    // other fields are added by name and must match the schema's types.
    let mut doc = Doc::new()?;
    doc.set_pk("doc1")?;
    doc.add_string("id", "doc1")?;
    doc.add_vector_fp32("embedding", &[0.1, 0.2, 0.3])?;

    // Insert the doc and flush so it is durable and visible to queries.
    collection.insert(&[&doc])?;
    collection.flush()?;

    // Run a top-10 nearest-neighbour search against the `embedding` field.
    let q = VectorQuery::builder()
        .field("embedding")
        .vector_fp32(&[0.1, 0.2, 0.3])
        .topk(10)
        .build()?;

    // Iterate the result set. `DocSet` frees its C-side buffer on drop.
    for row in collection.query(&q)?.iter() {
        println!("{:?} score={}", row.pk_copy(), row.score());
    }

    Ok(())
}
```

Longer walk-throughs live under [`examples/`](examples/); see
[Examples](#examples) below for a tour.

---

## Install

zvec-rs **links** against a prebuilt `libzvec_c_api`; it does not compile
zvec from source during `cargo build`. Pick one of three paths:

| Path | When | First-build time | Network |
|---|---|---|---|
| **`bundled` feature** | Dev, CI, anything where a small download is fine | ~30 s | Yes (PyPI wheel) |
| **Source build helper** | Targets upstream doesn't ship a wheel for; strict supply-chain requirements | 20–30 min (cached afterwards) | Yes (git clone + submodules) |
| **External prebuilt** | zvec already installed on the system | 0 s | No |

### A. `bundled` feature (recommended)

```toml
[dependencies]
zvec = { version = "0.1", features = ["bundled"] }
```

`build.rs` downloads upstream's pinned PyPI wheel for the current target,
verifies its SHA-256, extracts `libzvec_c_api` + `c_api.h`, and wires up
the linker (rpath included) so the resulting binary works out of the box.

If your target isn't in the wheel matrix (see
[Supported targets](#supported-targets)), `build.rs` emits a `cargo:warning`
and falls back to discovery via env vars / pkg-config.

### B. Build from source

```sh
./scripts/build-zvec.sh "$PWD/zvec-install"

export ZVEC_ROOT="$PWD/zvec-install"
export LD_LIBRARY_PATH="$ZVEC_ROOT/lib${LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}"
cargo build
```

The script clones upstream at `v0.3.1`, runs CMake, and installs a flat
`{lib,include}` prefix. Requires CMake ≥ 3.13, a C++17 compiler, git,
patch, and libclang.

Script overrides: `ZVEC_REF`, `ZVEC_REPO`, `ZVEC_SRC_DIR`,
`ZVEC_BUILD_DIR`, `CMAKE_GENERATOR`, `JOBS`.

### C. Point at an existing install

```sh
# Install prefix (expects $ZVEC_ROOT/lib + $ZVEC_ROOT/include/zvec/c_api.h):
export ZVEC_ROOT=/opt/zvec

# Or just the library dir; the header is picked up from vendor/c_api.h:
export ZVEC_LIB_DIR=/opt/zvec/lib

# Or via pkg-config:
cargo build --features pkg-config
```

### Supported targets (bundled)

The wheel matrix upstream ships; the extracted library is
Python-independent.

- `x86_64-unknown-linux-gnu`
- `aarch64-unknown-linux-gnu`
- `aarch64-apple-darwin`
- `x86_64-pc-windows-msvc`

Any platform zvec's CMake build targets works via paths B or C.

---

## Cargo features

| Feature       | Effect |
|---------------|--------|
| *(default)*   | Expects `libzvec_c_api` on the linker path via env vars / pkg-config / system paths. |
| `bundled`     | Download + extract upstream's PyPI wheel at build time. |
| `derive`      | `#[derive(IntoDoc)]` / `#[derive(FromDoc)]` for struct ↔ `Doc` conversion. |
| `tokio`       | `AsyncCollection` — every op runs in `tokio::task::spawn_blocking`. |
| `serde-json`  | `Doc::from_json(&Value, &schema)` for JSON → `Doc` ingest. |
| `half`        | `Doc::{add,get}_vector_fp16` + `VectorQuery::set_query_vector_fp16` take `&[half::f16]`. |
| `pkg-config`  | Probe for a system `zvec_c_api.pc` in addition to the env-var dance. |

---

## Environment variables

Read by `build.rs`:

| Variable                     | Purpose |
|------------------------------|---------|
| `ZVEC_ROOT`                  | Install prefix (`lib/` + `include/zvec/`). |
| `ZVEC_LIB_DIR`               | Directory containing `libzvec_c_api`. |
| `ZVEC_INCLUDE_DIR`           | Directory containing `zvec/c_api.h` (defaults to vendored copy). |
| `ZVEC_STATIC=1`              | Link `zvec_c_api` statically. |
| `ZVEC_BUNDLED_WHEEL_URL`     | Custom wheel URL (requires `ZVEC_BUNDLED_WHEEL_SHA256`). |
| `ZVEC_BUNDLED_WHEEL_SHA256`  | Expected SHA-256 for the URL override. |
| `ZVEC_BUNDLED_WHEEL_PATH`    | Local `.whl` file to use instead of downloading. |

---

## Examples

All examples live under [`examples/`](examples/). Run any of them with
`--features bundled` (or one of the other install paths).

| Example | Shows |
|---------|-------|
| [`version`](examples/version.rs) | Print the linked zvec's version. |
| [`basic`](examples/basic.rs) | Port of zvec's own `basic_example.c`: schema, insert, flush, query. |
| [`semantic_search`](examples/semantic_search.rs) | Index a small corpus + run a cosine query over 4-D embeddings. |
| [`hybrid_search`](examples/hybrid_search.rs) | Two vector queries (title vs. body) fused with Reciprocal Rank Fusion. |
| [`json_ingest`](examples/json_ingest.rs) | Feed `serde_json::Value`s into a collection via `Doc::from_json`. |
| [`derive`](examples/derive.rs) | `#[derive(IntoDoc)]` / `#[derive(FromDoc)]` round-trip. |

```sh
cargo run --example basic           --features bundled
cargo run --example semantic_search --features bundled
cargo run --example hybrid_search   --features bundled
cargo run --example json_ingest     --features "bundled serde-json"
cargo run --example derive          --features "bundled derive"
```

---

## API overview

All safe wrappers re-export at the crate root. Full rustdoc:
[docs.rs/zvec](https://docs.rs/zvec).

- **[`Collection`](https://docs.rs/zvec/latest/zvec/struct.Collection.html)** —
  create/open/flush/optimize; DDL (`create_index`, `drop_index`,
  `add_column`, `drop_column`, `alter_column`); DML (`insert`, `update`,
  `upsert`, `delete`, `delete_by_filter`, and `*_with_results` variants);
  DQL (`query`, `fetch`) returning a `DocSet` that frees its C-side buffer
  on drop.
- **[`CollectionSchema`](https://docs.rs/zvec/latest/zvec/struct.CollectionSchema.html)** - [`FieldSchema`](https://docs.rs/zvec/latest/zvec/struct.FieldSchema.html)
  schema construction, validation, field enumeration; both have a
  `builder()` API with typed shorthands
  (`FieldSchema::vector_fp32(...).hnsw(16, 200).metric(Cosine)`).
- **Index / query params** — [`IndexParams`](https://docs.rs/zvec/latest/zvec/struct.IndexParams.html)
  (HNSW / IVF / inverted), plus query-side
  [`HnswQueryParams`](https://docs.rs/zvec/latest/zvec/struct.HnswQueryParams.html) /
  [`IvfQueryParams`](https://docs.rs/zvec/latest/zvec/struct.IvfQueryParams.html) /
  [`FlatQueryParams`](https://docs.rs/zvec/latest/zvec/struct.FlatQueryParams.html).
- **[`VectorQuery`](https://docs.rs/zvec/latest/zvec/struct.VectorQuery.html)** —
  all fields/knobs from the C API plus a [`VectorQuery::builder()`](https://docs.rs/zvec/latest/zvec/struct.VectorQueryBuilder.html).
- **[`GroupByVectorQuery`](https://docs.rs/zvec/latest/zvec/struct.GroupByVectorQuery.html)** —
  configuration surface only; zvec 0.3.1 doesn't ship an executor C
  function for it yet (see the type's rustdoc for upstream context).
- **[`Doc`](https://docs.rs/zvec/latest/zvec/struct.Doc.html)** /
  **[`DocRef`](https://docs.rs/zvec/latest/zvec/struct.DocRef.html)** —
  typed `add_*` / `get_*` for every zvec data type, plus `serialize` /
  `deserialize` / `validate` / `to_detail_string`.
- **Retrieval helpers** — [`HybridSearch`](https://docs.rs/zvec/latest/zvec/struct.HybridSearch.html)
  fuses N queries, [`rerank::RrfReRanker`](https://docs.rs/zvec/latest/zvec/rerank/struct.RrfReRanker.html)
  and [`rerank::WeightedReRanker`](https://docs.rs/zvec/latest/zvec/rerank/struct.WeightedReRanker.html)
  work over any `Vec<Hit>`.
- **Struct ↔ `Doc`** (feature `derive`) —
  `#[derive(IntoDoc)]` / `#[derive(FromDoc)]` with `pk`, `skip`,
  `rename = "..."`, and vector-type hints (`vector_fp32`, `vector_fp64`,
  `vector_int8`, `vector_int16`, `binary`).
- **[`AsyncCollection`](https://docs.rs/zvec/latest/zvec/struct.AsyncCollection.html)**
  (feature `tokio`) — every op wrapped in `tokio::task::spawn_blocking`.
- **[`Doc::from_json`](https://docs.rs/zvec/latest/zvec/struct.Doc.html#method.from_json)**
  (feature `serde-json`) — JSON → `Doc` using the schema for type resolution.
- **Errors + utilities** — [`ErrorCode`](https://docs.rs/zvec/latest/zvec/enum.ErrorCode.html) /
  [`ZvecError`](https://docs.rs/zvec/latest/zvec/struct.ZvecError.html),
  [`version()`](https://docs.rs/zvec/latest/zvec/fn.version.html),
  [`Config`](https://docs.rs/zvec/latest/zvec/struct.Config.html) +
  [`initialize`](https://docs.rs/zvec/latest/zvec/fn.initialize.html) for
  optional global setup.

---

## Thread safety

- `Send + Sync` — `Collection`, pure builders and snapshots (`CollectionSchema`,
  `FieldSchema`, `IndexParams`, `HnswQueryParams`, `IvfQueryParams`,
  `FlatQueryParams`, `CollectionOptions`, `CollectionStats`, `Config`,
  `LogConfig`, `FieldSchemaRef<'_>`, `DocRef<'_>`).
- `Send` only — types with mutable C-side state and no documented
  thread-safe reads (`Doc`, `VectorQuery`, `GroupByVectorQuery`, `DocSet`).

Sharing a collection across threads is just `Arc<Collection>`.

---

## Contributing

- `cargo fmt --all` and
  `cargo clippy --all-targets --no-deps -- -D warnings` are enforced in CI.
- Integration tests (`tests/integration.rs`) exercise the full public
  surface; they need a working `libzvec_c_api`. Easiest:
  `cargo test --features bundled`.
- When bumping the pinned zvec version, update: `vendor/c_api.h`,
  `ZVEC_REF` in `scripts/build-zvec.sh`, the wheel pins in `build.rs`,
  both `.github/workflows/*.yml`, and `CHANGELOG.md`.

## License

Apache-2.0, matching upstream zvec.
