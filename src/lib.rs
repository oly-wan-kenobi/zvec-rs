//! Safe, idiomatic Rust bindings to [zvec], Alibaba's lightweight
//! in-process vector database.
//!
//! # At a glance
//!
//! - **Raw FFI** lives in [`sys`] — every zvec C symbol, generated at
//!   build time by `bindgen` from a pinned `c_api.h`.
//! - **Safe wrappers** at the crate root cover the full public C API:
//!   [`CollectionSchema`] / [`FieldSchema`], [`IndexParams`],
//!   [`VectorQuery`] / [`GroupByVectorQuery`], [`Collection`] (lifecycle
//!   + DDL + DML + DQL), [`Doc`] / [`DocRef`], [`CollectionStats`].
//! - **Retrieval helpers**: [`HybridSearch`] fuses multi-query results;
//!   [`rerank::RrfReRanker`] / [`rerank::WeightedReRanker`] are reusable
//!   for any `Vec<Hit>`.
//! - **Opt-in niceties** (cargo features): `tokio`, `derive`, `serde-json`,
//!   `half`, and a zero-setup `bundled` install path.
//!
//! Pinned zvec version: **v0.3.1**. The C header is vendored at
//! `vendor/c_api.h`.
//!
//! # Quickstart
//!
//! ```no_run
//! # fn main() -> zvec::Result<()> {
//! use zvec::{Collection, CollectionSchema, Doc, FieldSchema, MetricType, VectorQuery};
//!
//! let schema = CollectionSchema::builder("docs")
//!     .field(FieldSchema::string("id").invert_index(true, false))
//!     .field(
//!         FieldSchema::vector_fp32("embedding", 3)
//!             .hnsw(16, 200)
//!             .metric(MetricType::Cosine),
//!     )
//!     .build()?;
//!
//! let collection = Collection::create_and_open("./my_coll", &schema, None)?;
//!
//! let mut doc = Doc::new()?;
//! doc.set_pk("doc1")?;
//! doc.add_string("id", "doc1")?;
//! doc.add_vector_fp32("embedding", &[0.1, 0.2, 0.3])?;
//! collection.insert(&[&doc])?;
//! collection.flush()?;
//!
//! let q = VectorQuery::builder()
//!     .field("embedding")
//!     .vector_fp32(&[0.1, 0.2, 0.3])
//!     .topk(10)
//!     .build()?;
//! for row in collection.query(&q)?.iter() {
//!     println!("{:?} score={}", row.pk_copy(), row.score());
//! }
//! # Ok(())
//! # }
//! ```
//!
//! Each `set_*` / `add_*` method returns `Result`; the typical code path
//! uses `?` on every call. Errors carry zvec's own last-error message
//! via [`ZvecError::message`] when one is set.
//!
//! End-to-end recipes (semantic search, hybrid search, JSON ingest,
//! derive macros) live under
//! [`examples/`](https://github.com/oly-wan-kenobi/zvec-rs/tree/main/examples).
//!
//! # Cargo features
//!
//! | Feature       | Adds |
//! |---------------|------|
//! | `bundled`     | Fetches upstream's PyPI wheel at build time — no external zvec setup required. |
//! | `derive`      | `#[derive(IntoDoc)]` / `#[derive(FromDoc)]` for struct ↔ [`Doc`] conversion. |
//! | `tokio`       | [`AsyncCollection`] — every op runs in `tokio::task::spawn_blocking`. |
//! | `serde-json`  | [`Doc::from_json`] — build a [`Doc`] from a `serde_json::Value` + schema. |
//! | `half`        | [`Doc::add_vector_fp16`], [`DocRef::get_vector_fp16`], [`VectorQuery::set_query_vector_fp16`] accept `&[half::f16]`. |
//! | `pkg-config`  | Probe for a system `zvec_c_api.pc` in addition to env-var discovery. |
//!
//! # Install paths
//!
//! `build.rs` resolves `libzvec_c_api` in this order:
//!
//! 1. `ZVEC_LIB_DIR` — explicit library directory.
//! 2. `ZVEC_ROOT` — install prefix (`$ZVEC_ROOT/lib`, `$ZVEC_ROOT/lib64`).
//! 3. `bundled` feature — download + extract upstream's PyPI wheel.
//! 4. `pkg-config` (if the `pkg-config` feature is on).
//! 5. The system linker's defaults.
//!
//! `ZVEC_STATIC=1` switches to static linking. `ZVEC_INCLUDE_DIR` /
//! `ZVEC_ROOT` redirect bindgen at an installed header instead of the
//! vendored copy.
//!
//! # Error handling
//!
//! Every fallible call returns a [`Result`], which is a type alias for
//! `std::result::Result<T, ZvecError>`. A [`ZvecError`] carries:
//!
//! - [`ZvecError::code`] — a strongly-typed [`ErrorCode`] mirroring
//!   zvec's `ZVEC_ERROR_*` constants, with an `Other(i32)` fallback for
//!   forward compatibility.
//! - [`ZvecError::message`] — the last-error message from the C API,
//!   when zvec attached one.
//!
//! # Thread safety
//!
//! - [`Collection`] is `Send + Sync`. Share one across threads via
//!   `Arc<Collection>`; no dedicated sharing type is needed.
//! - Pure builders / snapshots ([`CollectionSchema`], [`FieldSchema`],
//!   [`IndexParams`], [`HnswQueryParams`], [`CollectionOptions`],
//!   [`CollectionStats`], …) are `Send + Sync`.
//! - Types with mutable C-side state and no documented thread-safe reads
//!   ([`Doc`], [`VectorQuery`], [`GroupByVectorQuery`], [`DocSet`]) are
//!   `Send` only.
//!
//! ```no_run
//! # fn main() -> zvec::Result<()> {
//! use std::sync::Arc;
//! use std::thread;
//! # use zvec::{Collection, CollectionSchema};
//! # let schema: CollectionSchema = unreachable!();
//! let collection = Arc::new(Collection::create_and_open("./coll", &schema, None)?);
//! let handles: Vec<_> = (0..4)
//!     .map(|_| {
//!         let c = Arc::clone(&collection);
//!         thread::spawn(move || c.flush())
//!     })
//!     .collect();
//! for h in handles { let _ = h.join(); }
//! # Ok(())
//! # }
//! ```
//!
//! # Where to go next
//!
//! - [`Collection`] — lifecycle, DDL, DML, DQL.
//! - [`HybridSearch`] — fused multi-query search.
//! - [`rerank`] — reciprocal-rank and weighted fusion over any
//!   `Vec<Hit>`.
//! - [`IntoDoc`] / [`FromDoc`] (feature `derive`) — struct ↔ `Doc`
//!   conversion.
//! - [`AsyncCollection`] (feature `tokio`) — async wrapper.
//! - [`Doc::from_json`] (feature `serde-json`) — JSON → `Doc`.
//!
//! [zvec]: https://github.com/alibaba/zvec

#![cfg_attr(docsrs, feature(doc_cfg))]

pub mod sys;

#[cfg(feature = "tokio")]
mod async_collection;
mod builder;
mod collection;
mod config;
mod doc;
mod error;
mod ffi_util;
mod from_doc;
mod hybrid;
mod index_params;
mod into_doc;
mod options;
mod query;
mod query_params;
pub mod rerank;
mod schema;
#[cfg(feature = "serde-json")]
mod serde_json_bridge;
mod stats;
mod types;
mod version;

#[cfg(feature = "tokio")]
#[cfg_attr(docsrs, doc(cfg(feature = "tokio")))]
pub use async_collection::AsyncCollection;
pub use builder::{CollectionSchemaBuilder, FieldSchemaBuilder, VectorQueryBuilder};
pub use collection::{Collection, DocSet, WriteResult, WriteSummary};
pub use config::{initialize, is_initialized, shutdown, Config, LogConfig};
pub use doc::{Doc, DocRef};
pub use error::{clear_last_error, ErrorCode, Result, ZvecError};
pub use from_doc::FromDoc;
pub use hybrid::HybridSearch;
pub use index_params::IndexParams;
pub use into_doc::IntoDoc;
pub use options::CollectionOptions;
pub use query::{GroupByVectorQuery, VectorQuery};
pub use query_params::{FlatQueryParams, HnswQueryParams, IvfQueryParams};
pub use schema::{CollectionSchema, FieldSchema, FieldSchemaRef};
pub use stats::CollectionStats;
pub use types::{DataType, DocOperator, IndexType, LogLevel, LogType, MetricType, QuantizeType};
pub use version::{check_version, version, version_major, version_minor, version_patch};

/// Re-exports of the derive macros from the `zvec-derive` companion
/// crate. Available with the `derive` cargo feature.
#[cfg(feature = "derive")]
#[cfg_attr(docsrs, doc(cfg(feature = "derive")))]
pub use zvec_derive::{FromDoc, IntoDoc};
