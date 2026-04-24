# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] — 2026-04-24

First public release on crates.io.

### Added

- Full safe wrappers over zvec's C API (v0.3.1 pinned): schemas,
  index parameters, collections, documents, queries, stats, config.
- Three install paths: `bundled` cargo feature (PyPI wheel),
  `scripts/build-zvec.sh` (source build), or external
  `ZVEC_ROOT` / `ZVEC_LIB_DIR` / `pkg-config`.
- Builder APIs: `CollectionSchema::builder()`, `FieldSchema::{string,
  vector_fp32, …}.hnsw(...).metric(...)`, `VectorQuery::builder()`.
- Fusion helpers: `zvec::rerank::{RrfReRanker, WeightedReRanker}` and
  `HybridSearch` for running N queries and fusing results.
- Streaming writes: `Collection::insert_iter` / `update_iter` /
  `upsert_iter`.
- Optional `serde-json` feature: `Doc::from_json(&Value, &schema)`.
- Optional `half` feature: `Doc::add_vector_fp16(&[half::f16])` etc.
- Optional `tokio` feature: `AsyncCollection` with every op wrapped
  in `spawn_blocking`.
- Optional `derive` feature: `#[derive(IntoDoc)]` and
  `#[derive(FromDoc)]` proc macros, from the sibling `zvec-derive`
  subcrate.
- Cookbook examples (`examples/{basic,version,semantic_search,
  hybrid_search,json_ingest}.rs`).
- docs.rs rendering: crate-root landing doc, per-feature `doc(cfg)`
  badges, zero-warning `cargo doc --all-features`.
- Two-workflow CI: every PR runs the bundled-feature matrix (Linux +
  macOS); a weekly cron validates `scripts/build-zvec.sh`.

[Unreleased]: https://github.com/oly-wan-kenobi/zvec-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/oly-wan-kenobi/zvec-rs/releases/tag/v0.1.0
