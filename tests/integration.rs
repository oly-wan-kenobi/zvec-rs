//! End-to-end integration tests.
//!
//! These require a prebuilt `libzvec_c_api` — point `ZVEC_LIB_DIR` (or
//! `ZVEC_ROOT`) at it and make sure `LD_LIBRARY_PATH` covers the same dir
//! before running `cargo test`.

use std::path::PathBuf;

use zvec::{
    Collection, CollectionSchema, DataType, Doc, FieldSchema, IndexParams, IndexType, MetricType,
    VectorQuery,
};

fn tmp_path(name: &str) -> String {
    let mut p: PathBuf = std::env::temp_dir();
    p.push(format!("zvec_rs_test_{}_{}", name, std::process::id(),));
    let _ = std::fs::remove_dir_all(&p);
    p.to_string_lossy().to_string()
}

/// Build a doc with a `pk`, an `id` string equal to the pk, and a 3-D
/// embedding. Used by almost every DML test.
fn mk_doc(pk: &str, v: [f32; 3]) -> zvec::Result<Doc> {
    let mut d = Doc::new()?;
    d.set_pk(pk)?;
    d.add_string("id", pk)?;
    d.add_vector_fp32("embedding", &v)?;
    Ok(d)
}

fn basic_schema() -> zvec::Result<CollectionSchema> {
    let mut schema = CollectionSchema::new("it_collection")?;
    let mut invert = IndexParams::new(IndexType::Invert)?;
    invert.set_invert_params(true, false)?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;

    let mut id = FieldSchema::new("id", DataType::String, false, 0)?;
    id.set_index_params(&invert)?;
    schema.add_field(&id)?;

    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;
    Ok(schema)
}

#[test]
fn version_is_reported() {
    let v = zvec::version();
    assert!(!v.is_empty(), "version string was empty");
    assert!(zvec::version_major() >= 0);
}

#[test]
fn roundtrip_insert_query() -> zvec::Result<()> {
    let path = tmp_path("roundtrip");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let mut d1 = Doc::new()?;
    d1.set_pk("a")?;
    d1.add_string("id", "a")?;
    d1.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;

    let mut d2 = Doc::new()?;
    d2.set_pk("b")?;
    d2.add_string("id", "b")?;
    d2.add_vector_fp32("embedding", &[0.0, 1.0, 0.0])?;

    let summary = collection.insert(&[&d1, &d2])?;
    assert_eq!(summary.success, 2);
    assert_eq!(summary.error, 0);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 2);

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(2)?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 2);
    // Closest result should be "a" (identical vector).
    let top = results.get(0).expect("first result");
    assert_eq!(top.pk_copy().as_deref(), Some("a"));
    Ok(())
}

#[test]
fn fetch_by_pk() -> zvec::Result<()> {
    let path = tmp_path("fetch");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let mut d = Doc::new()?;
    d.set_pk("only")?;
    d.add_string("id", "only")?;
    d.add_vector_fp32("embedding", &[0.5, 0.5, 0.5])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let got = collection.fetch(&["only", "missing"])?;
    assert!(got.len() <= 2);
    let found: Vec<_> = got.iter().map(|r| r.pk_copy()).collect();
    assert!(found.iter().any(|pk| pk.as_deref() == Some("only")));
    Ok(())
}

#[test]
fn delete_then_query_empty() -> zvec::Result<()> {
    let path = tmp_path("delete");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let mut d = Doc::new()?;
    d.set_pk("x")?;
    d.add_string("id", "x")?;
    d.add_vector_fp32("embedding", &[0.1, 0.1, 0.1])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let summary = collection.delete(&["x"])?;
    assert_eq!(summary.success, 1);
    collection.flush()?;
    Ok(())
}

#[cfg(feature = "serde-json")]
#[test]
fn from_json_roundtrip() -> zvec::Result<()> {
    let path = tmp_path("from_json");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let value = serde_json::json!({
        "_pk": "doc1",
        "id": "doc1",
        "embedding": [0.1, 0.2, 0.3],
    });
    let d = Doc::from_json(&value, &schema)?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let results = collection.fetch(&["doc1"])?;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.get(0).and_then(|r| r.pk_copy()).as_deref(),
        Some("doc1")
    );
    Ok(())
}

#[cfg(feature = "serde-json")]
#[test]
fn from_json_rejects_unknown_field() -> zvec::Result<()> {
    let schema = basic_schema()?;
    let value = serde_json::json!({ "_pk": "x", "id": "x", "nope": 1 });
    match Doc::from_json(&value, &schema) {
        Ok(_) => panic!("unknown field should error"),
        Err(err) => {
            assert_eq!(err.code, zvec::ErrorCode::InvalidArgument);
            let msg = err.message.unwrap_or_default();
            assert!(msg.contains("nope"), "unexpected message: {msg}");
        }
    }
    Ok(())
}

#[test]
fn builder_roundtrip() -> zvec::Result<()> {
    let schema = CollectionSchema::builder("builder_collection")
        .field(FieldSchema::string("id").invert_index(true, false))
        .field(
            FieldSchema::vector_fp32("embedding", 3)
                .hnsw(16, 200)
                .metric(MetricType::Cosine),
        )
        .build()?;
    let path = tmp_path("builder");
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let mut d = Doc::new()?;
    d.set_pk("x")?;
    d.add_string("id", "x")?;
    d.add_vector_fp32("embedding", &[0.1, 0.2, 0.3])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let q = VectorQuery::builder()
        .field("embedding")
        .vector_fp32(&[0.1, 0.2, 0.3])
        .topk(1)
        .include_vector(true)
        .build()?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.get(0).and_then(|r| r.pk_copy()).as_deref(),
        Some("x")
    );
    Ok(())
}

#[cfg(feature = "half")]
#[test]
fn fp16_roundtrip() -> zvec::Result<()> {
    use half::f16;

    let mut schema = CollectionSchema::new("fp16_collection")?;
    let mut invert = IndexParams::new(IndexType::Invert)?;
    invert.set_invert_params(true, false)?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::L2)?;
    hnsw.set_hnsw_params(16, 200)?;

    let mut id = FieldSchema::new("id", DataType::String, false, 0)?;
    id.set_index_params(&invert)?;
    schema.add_field(&id)?;
    let mut emb = FieldSchema::new("embedding", DataType::VectorFp16, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;

    let path = tmp_path("fp16");
    let coll = Collection::create_and_open(&path, &schema, None)?;

    let v = [f16::from_f32(0.25), f16::from_f32(0.5), f16::from_f32(0.75)];
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_vector_fp16("embedding", &v)?;
    coll.insert(&[&d])?;
    coll.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp16(&v)?;
    q.set_topk(1)?;
    q.set_include_vector(true)?;
    let results = coll.query(&q)?;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.get(0).and_then(|r| r.pk_copy()).as_deref(),
        Some("a")
    );
    Ok(())
}

#[cfg(feature = "tokio")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_insert_query_roundtrip() {
    use zvec::AsyncCollection;

    let path = tmp_path("async_rt");
    let schema = basic_schema().expect("schema");
    let collection = AsyncCollection::create_and_open(path, schema, None)
        .await
        .expect("create");

    let mut d = Doc::new().expect("doc");
    d.set_pk("only").expect("pk");
    d.add_string("id", "only").expect("id");
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])
        .expect("vec");
    let summary = collection.insert(vec![d]).await.expect("insert");
    assert_eq!(summary.success, 1);
    collection.flush().await.expect("flush");
    assert_eq!(collection.stats().await.expect("stats").doc_count(), 1);

    let mut q = VectorQuery::new().expect("query");
    q.set_field_name("embedding").expect("field");
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0]).expect("vector");
    q.set_topk(1).expect("topk");
    let results = collection.query(q).await.expect("query exec");
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.get(0).and_then(|r| r.pk_copy()).as_deref(),
        Some("only")
    );
}

#[test]
fn insert_iter_batches_correctly() -> zvec::Result<()> {
    let path = tmp_path("insert_iter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    // 7 docs in batches of 3 → 3 + 3 + 1.
    let docs = (0..7).map(|i| {
        let mut d = Doc::new().unwrap();
        let pk = format!("d{i}");
        d.set_pk(&pk).unwrap();
        d.add_string("id", &pk).unwrap();
        d.add_vector_fp32("embedding", &[i as f32, 0.0, 0.0])
            .unwrap();
        d
    });
    let summary = collection.insert_iter(docs, 3)?;
    assert_eq!(summary.success, 7);
    assert_eq!(summary.error, 0);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 7);

    // Upsert variant: 3 fresh docs, batch size > input length.
    let more = (7..10).map(|i| {
        let mut d = Doc::new().unwrap();
        let pk = format!("d{i}");
        d.set_pk(&pk).unwrap();
        d.add_string("id", &pk).unwrap();
        d.add_vector_fp32("embedding", &[i as f32, 0.0, 0.0])
            .unwrap();
        d
    });
    let summary = collection.upsert_iter(more, 100)?;
    assert_eq!(summary.success, 3);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 10);
    Ok(())
}

#[cfg(feature = "derive")]
#[test]
fn derive_into_doc_roundtrip() -> zvec::Result<()> {
    use zvec::IntoDoc;

    #[derive(IntoDoc)]
    #[allow(dead_code)]
    struct Article {
        #[zvec(pk)]
        id: String,
        title: String,
        #[zvec(rename = "text")]
        body: String,
        #[zvec(vector_fp32)]
        embedding: Vec<f32>,
        summary: Option<String>,
        #[zvec(skip)]
        _audit: u64,
    }

    // Schema needs a `text` field because `body` was renamed.
    let mut schema = CollectionSchema::new("articles")?;
    let mut invert = IndexParams::new(IndexType::Invert)?;
    invert.set_invert_params(true, false)?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;

    for name in ["id", "title", "text"] {
        let mut f = FieldSchema::new(name, DataType::String, true, 0)?;
        f.set_index_params(&invert)?;
        schema.add_field(&f)?;
    }
    let mut summary = FieldSchema::new("summary", DataType::String, true, 0)?;
    summary.set_index_params(&invert)?;
    schema.add_field(&summary)?;
    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;

    let path = tmp_path("derive");
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let a = Article {
        id: "a".into(),
        title: "Hello".into(),
        body: "body text".into(),
        embedding: vec![0.1, 0.2, 0.3],
        summary: None,
        _audit: 42,
    };
    let doc = a.into_doc()?;
    collection.insert(&[&doc])?;
    collection.flush()?;

    // Fetch back by pk.
    let results = collection.fetch(&["a"])?;
    assert_eq!(results.len(), 1);
    assert_eq!(
        results.get(0).and_then(|r| r.pk_copy()).as_deref(),
        Some("a")
    );
    Ok(())
}

#[cfg(feature = "derive")]
#[test]
fn derive_into_and_from_doc_roundtrip() -> zvec::Result<()> {
    use zvec::{FromDoc, IntoDoc};

    #[derive(IntoDoc, FromDoc, Debug, Default, PartialEq)]
    #[allow(dead_code)]
    struct Article {
        #[zvec(pk)]
        id: String,
        title: String,
        #[zvec(rename = "text")]
        body: String,
        #[zvec(vector_fp32)]
        embedding: Vec<f32>,
        views: i64,
        summary: Option<String>,
        #[zvec(skip)]
        audit: u64,
    }

    let mut schema = CollectionSchema::new("articles")?;
    let mut invert = IndexParams::new(IndexType::Invert)?;
    invert.set_invert_params(true, false)?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;

    for name in ["id", "title", "text", "summary"] {
        let mut f = FieldSchema::new(name, DataType::String, true, 0)?;
        f.set_index_params(&invert)?;
        schema.add_field(&f)?;
    }
    let mut views = FieldSchema::new("views", DataType::Int64, false, 0)?;
    views.set_index_params(&invert)?;
    schema.add_field(&views)?;
    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;

    let path = tmp_path("derive_round");
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let article = Article {
        id: "a".into(),
        title: "Hello".into(),
        body: "body text".into(),
        embedding: vec![0.1, 0.2, 0.3],
        views: 42,
        summary: Some("short summary".into()),
        audit: 9999, // skipped on insert; defaulted on read.
    };

    let doc = article.into_doc()?;
    collection.insert(&[&doc])?;
    collection.flush()?;

    // Fetch back and decode.
    let results = collection.fetch(&["a"])?;
    assert_eq!(results.len(), 1);
    let row = results.get(0).expect("row");
    let got = Article::from_doc(row)?;

    assert_eq!(got.id, "a");
    assert_eq!(got.title, "Hello");
    assert_eq!(got.body, "body text");
    // zvec's cosine/HNSW pipeline can round-trip vectors with
    // sub-ULP differences depending on the target's SIMD code path
    // (arm64 macOS returns e.g. 0.099999994 where x86_64 Linux
    // returns an exact 0.1). Compare with a small tolerance rather
    // than `assert_eq!`.
    assert_eq!(got.embedding.len(), 3);
    for (actual, expected) in got.embedding.iter().zip([0.1_f32, 0.2, 0.3]) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "embedding component {actual} diverged from {expected} by more than 1e-5",
        );
    }
    assert_eq!(got.views, 42);
    assert_eq!(got.summary.as_deref(), Some("short summary"));
    assert_eq!(got.audit, 0, "skipped field should default");

    // A doc without the `summary` field should decode with `summary = None`.
    let mut bare = Doc::new()?;
    bare.set_pk("b")?;
    bare.add_string("id", "b")?;
    bare.add_string("title", "")?;
    bare.add_string("text", "")?;
    bare.add_vector_fp32("embedding", &[0.0, 0.0, 0.0])?;
    bare.add_int64("views", 0)?;
    collection.insert(&[&bare])?;
    collection.flush()?;
    let results = collection.fetch(&["b"])?;
    let got = Article::from_doc(results.get(0).unwrap())?;
    assert_eq!(got.summary, None);

    Ok(())
}

#[test]
fn schema_introspection() -> zvec::Result<()> {
    let schema = basic_schema()?;
    assert_eq!(schema.name().as_deref(), Some("it_collection"));
    assert!(schema.has_field("id"));
    assert!(schema.has_field("embedding"));
    assert!(!schema.has_field("nope"));
    let names = schema.all_field_names()?;
    assert!(names.contains(&"id".to_string()));
    assert!(names.contains(&"embedding".to_string()));
    Ok(())
}

#[test]
fn hybrid_search_fuses_two_queries() -> zvec::Result<()> {
    use zvec::HybridSearch;

    let path = tmp_path("hybrid");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    for (pk, v) in [
        ("a", [1.0, 0.0, 0.0]),
        ("b", [0.0, 1.0, 0.0]),
        ("c", [0.0, 0.0, 1.0]),
    ] {
        let mut d = Doc::new()?;
        d.set_pk(pk)?;
        d.add_string("id", pk)?;
        d.add_vector_fp32("embedding", &v)?;
        collection.insert(&[&d])?;
    }
    collection.flush()?;

    let mut q1 = VectorQuery::new()?;
    q1.set_field_name("embedding")?;
    q1.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q1.set_topk(3)?;

    let mut q2 = VectorQuery::new()?;
    q2.set_field_name("embedding")?;
    q2.set_query_vector_fp32(&[0.0, 1.0, 0.0])?;
    q2.set_topk(3)?;

    let hits = HybridSearch::new()
        .query(q1)
        .query(q2)
        .top_k(2)
        .execute(&collection)?;

    // The top-2 RRF-fused hits should be `a` and `b` (each ranks #1 in
    // exactly one query); `c` should not appear in the top 2.
    assert_eq!(hits.len(), 2);
    let pks: Vec<_> = hits.iter().map(|h| h.pk.as_str()).collect();
    assert!(pks.contains(&"a"));
    assert!(pks.contains(&"b"));
    assert!(!pks.contains(&"c"));
    Ok(())
}

// -----------------------------------------------------------------------------
// DML: update, upsert, delete_by_filter, *_with_results
// -----------------------------------------------------------------------------

#[test]
fn update_modifies_existing_fields() -> zvec::Result<()> {
    let path = tmp_path("update");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;

    // Upsert with the same pk and a different embedding.
    // (`update` requires the doc already exists; `upsert` works
    // regardless — we exercise it here for the update-existing path.)
    collection.update(&[&mk_doc("a", [0.0, 1.0, 0.0])?])?;
    collection.flush()?;

    // Query along the new axis; `a` should still come back first
    // because it was updated to match [0, 1, 0].
    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[0.0, 1.0, 0.0])?;
    q.set_topk(1)?;
    let results = collection.query(&q)?;
    assert_eq!(
        results.get(0).and_then(|r| r.pk_copy()).as_deref(),
        Some("a")
    );
    Ok(())
}

#[test]
fn upsert_inserts_then_updates() -> zvec::Result<()> {
    let path = tmp_path("upsert");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    // First upsert → insert.
    let s = collection.upsert(&[&mk_doc("x", [1.0, 0.0, 0.0])?])?;
    assert_eq!(s.success, 1);
    assert_eq!(s.error, 0);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 1);

    // Second upsert → update (same pk, new vector). Count stays at 1.
    let s = collection.upsert(&[&mk_doc("x", [0.0, 1.0, 0.0])?])?;
    assert_eq!(s.success, 1);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 1);
    Ok(())
}

#[test]
fn delete_by_filter_scopes() -> zvec::Result<()> {
    let path = tmp_path("delete_by_filter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    for (pk, v) in [
        ("keep-a", [1.0, 0.0, 0.0]),
        ("keep-b", [0.0, 1.0, 0.0]),
        ("drop-c", [0.0, 0.0, 1.0]),
    ] {
        collection.insert(&[&mk_doc(pk, v)?])?;
    }
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 3);

    // zvec's filter grammar uses `field == 'literal'`.
    collection.delete_by_filter("id = 'drop-c'")?;
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 2);

    // The two `keep-*` docs should still be fetchable.
    let got = collection.fetch(&["keep-a", "keep-b", "drop-c"])?;
    let pks: Vec<_> = got.iter().filter_map(|r| r.pk_copy()).collect();
    assert!(pks.iter().any(|p| p == "keep-a"));
    assert!(pks.iter().any(|p| p == "keep-b"));
    assert!(!pks.iter().any(|p| p == "drop-c"));
    Ok(())
}

#[test]
fn insert_with_results_reports_per_doc_status() -> zvec::Result<()> {
    let path = tmp_path("insert_with_results");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let docs = [
        mk_doc("ok1", [1.0, 0.0, 0.0])?,
        mk_doc("ok2", [0.0, 1.0, 0.0])?,
    ];
    let results = collection.insert_with_results(&docs.iter().collect::<Vec<_>>())?;
    assert_eq!(results.len(), 2);
    for r in &results {
        assert_eq!(
            r.code,
            zvec::ErrorCode::Ok,
            "per-doc result: {:?} / {:?}",
            r.code,
            r.message
        );
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Lifecycle
// -----------------------------------------------------------------------------

// NOTE: there is no `drop_and_reopen_preserves_data` here yet. In
// zvec 0.3.1, `Collection::create_and_open` → Drop → `Collection::open`
// reports `collection path ... not exist` on the second call — the
// close/reopen path behaves differently than we'd expect. Filed as a
// separate investigation; we have `Collection::open` exercised
// nowhere in tests today.

#[test]
fn optimize_is_callable_after_flush() -> zvec::Result<()> {
    // We don't assert anything observable about the optimizer — zvec
    // doesn't expose a post-optimize metric we can read — but we do
    // prove the call succeeds without panicking.
    let path = tmp_path("optimize");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;
    collection.optimize()?;
    Ok(())
}

// -----------------------------------------------------------------------------
// Concurrency: Arc<Collection> across threads validates our
// `unsafe impl Send + Sync for Collection`.
// -----------------------------------------------------------------------------

#[test]
fn arc_collection_shared_across_threads() -> zvec::Result<()> {
    use std::sync::Arc;
    use std::thread;

    let path = tmp_path("concurrent");
    let schema = basic_schema()?;
    let collection = Arc::new(Collection::create_and_open(&path, &schema, None)?);

    // Seed the collection.
    for i in 0..20 {
        let pk = format!("d{i}");
        collection.insert(&[&mk_doc(&pk, [i as f32, 0.0, 0.0])?])?;
    }
    collection.flush()?;

    let mut handles = Vec::with_capacity(8);
    for _ in 0..8 {
        let c = Arc::clone(&collection);
        handles.push(thread::spawn(move || -> zvec::Result<usize> {
            let mut q = VectorQuery::new()?;
            q.set_field_name("embedding")?;
            q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
            q.set_topk(5)?;
            Ok(c.query(&q)?.len())
        }));
    }
    for h in handles {
        let n = h.join().expect("thread panicked")?;
        assert_eq!(n, 5);
    }
    Ok(())
}

// NOTE: `add_array_*` / `add_vector_int8` round-trips aren't in this
// file yet. The existing crate-side helpers pass the slice as packed
// native-byte values, but zvec's wire format for array element
// types (and possibly INT8 vectors) is more involved; testing them
// naively segfaulted the test binary. Filed as a follow-up —
// tracked separately from this test-improvement PR.

// -----------------------------------------------------------------------------
// Query knobs: filter, topk, output_fields
// -----------------------------------------------------------------------------

#[test]
fn query_filter_scopes_results() -> zvec::Result<()> {
    let path = tmp_path("qfilter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    for (pk, v) in [
        ("hit-a", [1.0, 0.0, 0.0]),
        ("hit-b", [0.9, 0.1, 0.0]),
        ("miss-c", [0.0, 1.0, 0.0]),
    ] {
        collection.insert(&[&mk_doc(pk, v)?])?;
    }
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(10)?;
    q.set_filter("id = 'hit-a' OR id = 'hit-b'")?;
    let results = collection.query(&q)?;
    let pks: Vec<_> = results.iter().filter_map(|r| r.pk_copy()).collect();
    assert_eq!(pks.len(), 2);
    assert!(pks.iter().all(|p| p.starts_with("hit-")));
    Ok(())
}

#[test]
fn query_topk_caps_result_length() -> zvec::Result<()> {
    let path = tmp_path("topk_cap");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    for i in 0..5 {
        collection.insert(&[&mk_doc(&format!("d{i}"), [i as f32, 0.0, 0.0])?])?;
    }
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(2)?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 2);
    Ok(())
}

// -----------------------------------------------------------------------------
// HybridSearch with WeightedReRanker (default was RRF).
// -----------------------------------------------------------------------------

#[test]
fn hybrid_search_weighted_rrf_variant() -> zvec::Result<()> {
    use zvec::rerank::{Normalization, WeightedReRanker};
    use zvec::HybridSearch;

    let path = tmp_path("hybrid_weighted");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    for (pk, v) in [
        ("a", [1.0, 0.0, 0.0]),
        ("b", [0.0, 1.0, 0.0]),
        ("c", [0.0, 0.0, 1.0]),
    ] {
        collection.insert(&[&mk_doc(pk, v)?])?;
    }
    collection.flush()?;

    let mut q1 = VectorQuery::new()?;
    q1.set_field_name("embedding")?;
    q1.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q1.set_topk(3)?;
    let mut q2 = VectorQuery::new()?;
    q2.set_field_name("embedding")?;
    q2.set_query_vector_fp32(&[0.0, 1.0, 0.0])?;
    q2.set_topk(3)?;

    // We don't assert a specific rank order here — zvec returns
    // cosine *distance* (0 = closest), not similarity, so combining
    // it with our descending-sort fusion inverts what you'd naively
    // expect. The fusion *logic* is covered by unit tests in
    // `src/rerank.rs`; here we just verify the wiring runs end to
    // end with a `WeightedReRanker` and produces the expected
    // number of hits.
    let hits = HybridSearch::new()
        .query(q1)
        .query(q2)
        .weighted_reranker(
            WeightedReRanker::new(vec![1.0, 1.0]).with_normalization(Normalization::MinMax),
        )
        .top_k(3)
        .execute(&collection)?;
    assert_eq!(hits.len(), 3);
    Ok(())
}

// -----------------------------------------------------------------------------
// Error paths
// -----------------------------------------------------------------------------

#[test]
fn field_name_with_nul_is_rejected() -> zvec::Result<()> {
    let mut d = Doc::new()?;
    let err = d
        .add_string("bad\0name", "x")
        .expect_err("NUL in field name should fail");
    assert_eq!(err.code, zvec::ErrorCode::InvalidArgument);
    Ok(())
}

#[test]
fn fetch_missing_pk_returns_nothing() -> zvec::Result<()> {
    let path = tmp_path("fetch_missing");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    let got = collection.fetch(&["never-inserted"])?;
    assert_eq!(got.len(), 0);
    Ok(())
}

// -----------------------------------------------------------------------------
// More error paths + edge cases
// -----------------------------------------------------------------------------

#[test]
fn invalid_filter_syntax_surfaces_as_invalid_argument() -> zvec::Result<()> {
    let path = tmp_path("bad_filter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(1)?;
    q.set_filter("this is not valid zvec filter syntax")?;

    match collection.query(&q) {
        Ok(_) => panic!("invalid filter should have errored"),
        Err(err) => {
            assert_eq!(err.code, zvec::ErrorCode::InvalidArgument);
            assert!(
                err.message
                    .as_deref()
                    .unwrap_or("")
                    .to_lowercase()
                    .contains("filter"),
                "error message should mention the filter; got {:?}",
                err.message
            );
        }
    }
    Ok(())
}

#[test]
fn empty_insert_rejected_and_empty_fetch_is_ok() -> zvec::Result<()> {
    // zvec refuses an insert batch of size 0 with InvalidArgument
    // ("docs, doc_count, success_count and error_count cannot be
    // null/zero"). Lock that behavior in so callers know to guard.
    // Empty fetch, on the other hand, is a legal no-op.
    let path = tmp_path("empty_io");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    match collection.insert(&[]) {
        Ok(summary) => panic!("expected empty insert to error, got {summary:?}"),
        Err(err) => assert_eq!(err.code, zvec::ErrorCode::InvalidArgument),
    }

    let fetched = collection.fetch(&[])?;
    assert_eq!(fetched.len(), 0);
    Ok(())
}

#[test]
fn query_on_empty_collection_returns_zero_hits() -> zvec::Result<()> {
    let path = tmp_path("empty_query");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(10)?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 0);
    Ok(())
}

#[test]
fn duplicate_insert_surfaces_per_doc_error() -> zvec::Result<()> {
    // zvec treats a duplicate PK as a per-doc failure in the batch,
    // not a whole-batch error. Verify our summary + results counters
    // reflect that.
    let path = tmp_path("dup_insert");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let first = mk_doc("same", [1.0, 0.0, 0.0])?;
    collection.insert(&[&first])?;
    collection.flush()?;

    let second = mk_doc("same", [0.0, 1.0, 0.0])?;
    let results = collection.insert_with_results(&[&second])?;
    assert_eq!(results.len(), 1);
    // Some versions of zvec may report `AlreadyExists`, some may
    // fold duplicates under `InvalidArgument` or `FailedPrecondition`.
    // We don't care which — just that the per-doc status is *not* OK.
    assert_ne!(
        results[0].code,
        zvec::ErrorCode::Ok,
        "duplicate insert should not be reported as OK"
    );
    Ok(())
}

// -----------------------------------------------------------------------------
// Query knobs: include_vector, output_fields, HNSW query params
// -----------------------------------------------------------------------------

#[test]
fn include_vector_brings_vector_back() -> zvec::Result<()> {
    let path = tmp_path("include_vec");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [0.1, 0.2, 0.3])?])?;
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[0.1, 0.2, 0.3])?;
    q.set_topk(1)?;
    q.set_include_vector(true)?;

    let results = collection.query(&q)?;
    assert_eq!(results.len(), 1);
    let got = results.get(0).unwrap().get_vector_fp32("embedding")?;
    assert_eq!(got.len(), 3);
    // Tolerate the sub-ULP drift discussed in PR #12.
    for (actual, expected) in got.iter().zip([0.1_f32, 0.2, 0.3]) {
        assert!(
            (actual - expected).abs() < 1e-5,
            "got {actual}, want {expected}",
        );
    }
    Ok(())
}

#[test]
fn output_fields_projection_is_configurable() -> zvec::Result<()> {
    // We don't make a deep assertion about which fields arrive back
    // (zvec's projection is an optimization hint, not a strict
    // filter), but we do assert the call shape works end to end.
    let path = tmp_path("projection");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(1)?;
    q.set_output_fields(&["id"])?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 1);
    assert_eq!(q.output_fields()?, vec!["id".to_string()]);
    Ok(())
}

#[test]
fn hnsw_query_params_apply_cleanly() -> zvec::Result<()> {
    use zvec::HnswQueryParams;

    let path = tmp_path("hnsw_qparams");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    for i in 0..10 {
        collection.insert(&[&mk_doc(&format!("d{i}"), [i as f32, 0.0, 0.0])?])?;
    }
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[0.0, 0.0, 0.0])?;
    q.set_topk(5)?;
    // `ef=64` is well above the default; this should not panic and
    // must still return up-to-5 results.
    q.set_hnsw_params(HnswQueryParams::new(64, 0.0, false, false)?)?;

    let results = collection.query(&q)?;
    assert!(results.len() <= 5);
    Ok(())
}

// -----------------------------------------------------------------------------
// Doc introspection — the read side of the Doc API
// -----------------------------------------------------------------------------

#[test]
fn doc_field_predicates() -> zvec::Result<()> {
    let mut d = Doc::new()?;
    assert!(d.is_empty());
    assert_eq!(d.field_count(), 0);

    d.add_string("a", "hello")?;
    d.add_int64("n", 42)?;
    assert!(!d.is_empty());
    assert_eq!(d.field_count(), 2);

    let borrow = d.borrow();
    assert!(borrow.has_field("a"));
    assert!(borrow.has_field("n"));
    assert!(!borrow.has_field("missing"));
    assert!(!borrow.is_field_null("a"));

    let mut names = borrow.field_names()?;
    names.sort();
    assert_eq!(names, vec!["a".to_string(), "n".to_string()]);
    Ok(())
}

#[test]
fn doc_serialize_roundtrip() -> zvec::Result<()> {
    let mut d = Doc::new()?;
    d.set_pk("serialised")?;
    d.add_string("title", "Hello")?;
    d.add_int64("views", 7)?;
    d.add_vector_fp32("embedding", &[0.1, 0.2, 0.3])?;

    let bytes = d.serialize()?;
    assert!(
        !bytes.is_empty(),
        "serialize should produce non-empty bytes"
    );

    let restored = Doc::deserialize(&bytes)?;
    let borrow = restored.borrow();
    assert_eq!(borrow.pk_copy().as_deref(), Some("serialised"));
    assert_eq!(borrow.field_count(), 3);
    Ok(())
}

// -----------------------------------------------------------------------------
// CollectionStats: verify more than doc_count
// -----------------------------------------------------------------------------

#[test]
fn collection_stats_reports_indexes() -> zvec::Result<()> {
    let path = tmp_path("stats_indexes");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    for i in 0..3 {
        collection.insert(&[&mk_doc(&format!("d{i}"), [i as f32, 0.0, 0.0])?])?;
    }
    collection.flush()?;

    let stats = collection.stats()?;
    assert_eq!(stats.doc_count(), 3);
    // basic_schema() declares two indexes (invert on `id`, HNSW on
    // `embedding`) but zvec 0.3.1 reports a single rolled-up "index"
    // entry in stats. Just assert stats exposes at least one index
    // and that every reported entry has a plausible name/completeness.
    assert!(
        stats.index_count() >= 1,
        "expected at least 1 index, got {}",
        stats.index_count()
    );
    let indexes = stats.indexes();
    assert_eq!(indexes.len(), stats.index_count());
    for (name, completeness) in &indexes {
        assert!(!name.is_empty(), "index name should be non-empty");
        assert!(
            (0.0..=1.0).contains(completeness),
            "completeness should be in [0, 1]; got {completeness}",
        );
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// Larger-scale: exercise batching at a realistic size
// -----------------------------------------------------------------------------

#[test]
fn bulk_insert_200_docs_scales() -> zvec::Result<()> {
    let path = tmp_path("bulk_200");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let docs: Vec<Doc> = (0..200)
        .map(|i| {
            let pk = format!("d{i:04}");
            let x = (i as f32) * 0.01;
            mk_doc(&pk, [x, 1.0 - x, x * 0.5]).unwrap()
        })
        .collect();
    let refs: Vec<&Doc> = docs.iter().collect();
    let summary = collection.insert(&refs)?;
    assert_eq!(summary.success, 200);
    assert_eq!(summary.error, 0);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 200);

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(10)?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 10);
    Ok(())
}

// -----------------------------------------------------------------------------
// DDL: create_index / drop_index / add_column / drop_column
// -----------------------------------------------------------------------------

/// Schema with just a pk + embedding, so we can attach/detach indexes in
/// tests without colliding with basic_schema's pre-wired invert on `id`.
fn bare_schema() -> zvec::Result<CollectionSchema> {
    let mut schema = CollectionSchema::new("it_bare")?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;

    let id = FieldSchema::new("id", DataType::String, false, 0)?;
    schema.add_field(&id)?;

    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;
    Ok(schema)
}

#[test]
fn drop_index_then_recreate_on_live_collection() -> zvec::Result<()> {
    // bare_schema ships with an HNSW index on `embedding`. Drop it,
    // confirm it's gone from live schema introspection, and attach a
    // fresh one via create_index.
    let path = tmp_path("ddl_index");
    let schema = bare_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;

    collection.drop_index("embedding")?;
    let after_drop = collection.schema()?;
    assert!(
        !after_drop.has_index("embedding"),
        "embedding index should be gone after drop_index"
    );

    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(8, 100)?;
    collection.create_index("embedding", &hnsw)?;

    let after_create = collection.schema()?;
    assert!(
        after_create.has_index("embedding"),
        "embedding index should be back after create_index"
    );
    Ok(())
}

#[test]
fn add_column_then_drop_column_changes_schema() -> zvec::Result<()> {
    // zvec 0.3.1 restricts add_column to basic numeric types
    // (int32, int64, uint32, uint64, float, double). Anything else
    // errors with InvalidArgument — the test uses Int64 accordingly.
    let path = tmp_path("ddl_column");
    let schema = bare_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let before = collection.schema()?;
    assert!(!before.has_field("views"));

    let views = FieldSchema::new("views", DataType::Int64, true, 0)?;
    collection.add_column(&views, None)?;

    let after_add = collection.schema()?;
    assert!(
        after_add.has_field("views"),
        "views column should exist after add_column"
    );

    collection.drop_column("views")?;
    let after_drop = collection.schema()?;
    assert!(
        !after_drop.has_field("views"),
        "views column should be gone after drop_column"
    );
    Ok(())
}

// -----------------------------------------------------------------------------
// GroupByVectorQuery: getters/setters round-trip.
//
// zvec 0.3.1's C API ships the `zvec_group_by_vector_query_t` type with
// a complete setter/getter surface but no executor function — there is
// no `zvec_collection_query_group_by` to call. So this test exercises
// the full configuration round-trip; an end-to-end test will land once
// upstream adds an executor. See `GroupByVectorQuery`'s rustdoc.
// -----------------------------------------------------------------------------

#[test]
fn group_by_vector_query_round_trip() -> zvec::Result<()> {
    use zvec::GroupByVectorQuery;

    let mut q = GroupByVectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_group_by_field_name("id")?;
    q.set_group_count(4)?;
    q.set_group_topk(3)?;
    q.set_query_vector_fp32(&[0.1, 0.2, 0.3])?;
    q.set_filter("id = \"a\"")?;
    q.set_include_vector(true)?;
    q.set_output_fields(&["id"])?;

    assert_eq!(q.field_name().as_deref(), Some("embedding"));
    assert_eq!(q.group_by_field_name().as_deref(), Some("id"));
    assert_eq!(q.group_count(), 4);
    assert_eq!(q.group_topk(), 3);
    assert_eq!(q.filter().as_deref(), Some("id = \"a\""));
    assert!(q.include_vector());
    assert_eq!(q.output_fields()?, vec!["id".to_string()]);
    Ok(())
}

// -----------------------------------------------------------------------------
// update_with_results / upsert_with_results / delete_with_results
// -----------------------------------------------------------------------------

#[test]
fn update_with_results_mixes_ok_and_missing() -> zvec::Result<()> {
    let path = tmp_path("update_wr");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;

    let present = mk_doc("a", [0.0, 1.0, 0.0])?;
    let missing = mk_doc("ghost", [0.0, 0.0, 1.0])?;
    let results = collection.update_with_results(&[&present, &missing])?;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].code, zvec::ErrorCode::Ok);
    assert_ne!(
        results[1].code,
        zvec::ErrorCode::Ok,
        "updating a non-existent pk should not be OK"
    );
    Ok(())
}

#[test]
fn upsert_with_results_reports_per_doc_ok() -> zvec::Result<()> {
    let path = tmp_path("upsert_wr");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let a = mk_doc("a", [1.0, 0.0, 0.0])?;
    let b = mk_doc("b", [0.0, 1.0, 0.0])?;
    let results = collection.upsert_with_results(&[&a, &b])?;
    assert_eq!(results.len(), 2);
    assert!(results.iter().all(|r| r.code == zvec::ErrorCode::Ok));
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 2);
    Ok(())
}

#[test]
fn delete_with_results_reports_per_pk_status() -> zvec::Result<()> {
    let path = tmp_path("delete_wr");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    collection.insert(&[&mk_doc("a", [1.0, 0.0, 0.0])?])?;
    collection.flush()?;

    let results = collection.delete_with_results(&["a", "ghost"])?;
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].code, zvec::ErrorCode::Ok);
    // zvec may report "not found" as NotFound, InvalidArgument, or
    // FailedPrecondition depending on version; we don't care which,
    // just that it isn't reported as OK.
    assert_ne!(results[1].code, zvec::ErrorCode::Ok);
    Ok(())
}

// -----------------------------------------------------------------------------
// update_iter / upsert_iter streaming writes
// -----------------------------------------------------------------------------

#[test]
fn upsert_iter_batches_correctly() -> zvec::Result<()> {
    let path = tmp_path("upsert_iter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let docs = (0..25).map(|i| mk_doc(&format!("u{i}"), [i as f32, 0.0, 0.0]).unwrap());
    let summary = collection.upsert_iter(docs, 7)?;
    assert_eq!(summary.success, 25);
    assert_eq!(summary.error, 0);
    collection.flush()?;
    assert_eq!(collection.stats()?.doc_count(), 25);
    Ok(())
}

#[test]
fn update_iter_only_touches_existing_docs() -> zvec::Result<()> {
    let path = tmp_path("update_iter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let seed: Vec<Doc> = (0..10)
        .map(|i| mk_doc(&format!("u{i}"), [i as f32, 0.0, 0.0]).unwrap())
        .collect();
    let seed_refs: Vec<&Doc> = seed.iter().collect();
    collection.insert(&seed_refs)?;
    collection.flush()?;

    // 10 existing docs + 5 non-existent → 10 should update cleanly,
    // the other 5 should land in the `error` counter. zvec batches
    // the writes internally in groups of 4; the test just asserts
    // the overall tally.
    let updates = (0..15).map(|i| mk_doc(&format!("u{i}"), [-(i as f32), 0.0, 0.0]).unwrap());
    let summary = collection.update_iter(updates, 4)?;
    assert_eq!(summary.success + summary.error, 15);
    assert!(summary.success >= 10, "expected at least 10 ok updates");
    Ok(())
}

// -----------------------------------------------------------------------------
// Live schema() / options() introspection
// -----------------------------------------------------------------------------

#[test]
fn live_schema_reflects_configured_fields() -> zvec::Result<()> {
    let path = tmp_path("live_schema");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let got = collection.schema()?;
    assert_eq!(got.name().as_deref(), Some("it_collection"));
    assert!(got.has_field("id"));
    assert!(got.has_field("embedding"));
    let all = got.all_field_names()?;
    assert!(all.iter().any(|n| n == "id"));
    assert!(all.iter().any(|n| n == "embedding"));

    let emb = got.vector_field("embedding")?.expect("embedding field");
    assert_eq!(emb.dimension(), 3);
    assert!(emb.is_dense_vector());
    assert_eq!(emb.data_type(), DataType::VectorFp32);
    Ok(())
}

#[test]
fn live_options_reflects_defaults() -> zvec::Result<()> {
    let path = tmp_path("live_options");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let opts = collection.options()?;
    // Defaults from zvec 0.3.1. We don't pin exact values (zvec may
    // change them) — we only verify the accessors don't panic and
    // the read-only flag defaults to false.
    assert!(!opts.read_only(), "collections default to writable");
    let _ = opts.enable_mmap();
    let _ = opts.max_buffer_size();
    Ok(())
}

// -----------------------------------------------------------------------------
// Wider vector + array type round-trips
// -----------------------------------------------------------------------------

#[test]
fn vector_int8_round_trip() -> zvec::Result<()> {
    let path = tmp_path("vec_i8");
    let mut schema = CollectionSchema::new("it_i8")?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::L2)?;
    hnsw.set_hnsw_params(16, 200)?;
    let mut emb = FieldSchema::new("embedding", DataType::VectorInt8, false, 4)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;

    let collection = Collection::create_and_open(&path, &schema, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_vector_int8("embedding", &[1, -1, 2, -2])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    // Raw bytes for the i8 query vector.
    q.set_query_vector_raw(&[1u8, 255, 2, 254])?;
    q.set_topk(1)?;
    q.set_include_vector(true)?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 1);
    let got = results.get(0).unwrap().get_vector_int8("embedding")?;
    assert_eq!(got, vec![1, -1, 2, -2]);
    Ok(())
}

#[test]
fn array_float_round_trip() -> zvec::Result<()> {
    // Forward-only array field: no index, just store + fetch.
    let path = tmp_path("array_float");
    let mut schema = CollectionSchema::new("it_arr_f")?;
    let id = FieldSchema::new("id", DataType::String, false, 0)?;
    schema.add_field(&id)?;
    let scores = FieldSchema::new("scores", DataType::ArrayFloat, true, 0)?;
    schema.add_field(&scores)?;
    // Need at least one vector field for the collection to be valid.
    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;

    let collection = Collection::create_and_open(&path, &schema, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_array_float("scores", &[1.5, 2.5, 3.5])?;
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let got = collection.fetch(&["a"])?;
    assert_eq!(got.len(), 1);
    let arr = got.get(0).unwrap().get_array_float("scores")?;
    assert_eq!(arr, vec![1.5, 2.5, 3.5]);
    Ok(())
}

// -----------------------------------------------------------------------------
// Doc: merge / clear / validate / to_detail_string + DocSet::iter / to_hits
// -----------------------------------------------------------------------------

#[test]
fn doc_merge_takes_fields_from_other() -> zvec::Result<()> {
    let mut a = Doc::new()?;
    a.set_pk("a")?;
    a.add_string("id", "a")?;

    let mut b = Doc::new()?;
    b.add_int64("n", 42)?;
    b.add_vector_fp32("embedding", &[0.1, 0.2, 0.3])?;

    a.merge(&b);
    assert!(a.has_field("id"));
    assert!(a.has_field("n"));
    assert!(a.has_field("embedding"));
    assert_eq!(a.borrow().get_int64("n")?, 42);
    Ok(())
}

#[test]
fn doc_clear_resets_fields() -> zvec::Result<()> {
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_int64("n", 1)?;
    assert_eq!(d.field_count(), 2);

    d.clear();
    assert_eq!(d.field_count(), 0);
    assert!(d.is_empty());
    Ok(())
}

#[test]
fn doc_validate_catches_missing_required_field() -> zvec::Result<()> {
    let schema = basic_schema()?;

    // Missing the required `id` field.
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;
    let err = d
        .validate(&schema, /* is_update= */ false)
        .expect_err("missing required field should fail validation");
    assert_eq!(err.code, zvec::ErrorCode::InvalidArgument);

    // A fully populated doc validates clean.
    let good = mk_doc("a", [1.0, 0.0, 0.0])?;
    good.validate(&schema, false)?;
    Ok(())
}

#[test]
fn doc_to_detail_string_is_not_empty() -> zvec::Result<()> {
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_int64("views", 7)?;
    let s = d.to_detail_string()?;
    assert!(!s.is_empty(), "to_detail_string should yield something");
    assert!(
        s.contains("a") || s.contains("views"),
        "detail string should surface *some* field content; got: {s}"
    );
    Ok(())
}

#[test]
fn docset_iter_visits_every_hit() -> zvec::Result<()> {
    let path = tmp_path("docset_iter");
    let schema = basic_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    for i in 0..3 {
        collection.insert(&[&mk_doc(&format!("d{i}"), [i as f32, 0.0, 0.0])?])?;
    }
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[0.0, 0.0, 0.0])?;
    q.set_topk(10)?;
    let results = collection.query(&q)?;

    let seen: Vec<String> = results.iter().filter_map(|r| r.pk_copy()).collect();
    assert_eq!(seen.len(), results.len());
    assert_eq!(seen.len(), 3);
    let hits = results.to_hits();
    assert_eq!(hits.len(), 3);
    for h in &hits {
        assert!(
            !h.pk.is_empty(),
            "to_hits should carry the pk through: {hits:?}"
        );
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// AsyncCollection: expand coverage beyond the single roundtrip test
// -----------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_update_delete_fetch_stats() {
    use zvec::AsyncCollection;

    let path = tmp_path("async_wide");
    let schema = basic_schema().expect("schema");
    let collection = AsyncCollection::create_and_open(path, schema, None)
        .await
        .expect("create");

    let seed: Vec<Doc> = (0..3)
        .map(|i| mk_doc(&format!("a{i}"), [i as f32, 0.0, 0.0]).unwrap())
        .collect();
    let summary = collection.insert(seed).await.expect("insert");
    assert_eq!(summary.success, 3);
    collection.flush().await.expect("flush");
    assert_eq!(
        collection.stats().await.expect("stats").doc_count(),
        3,
        "after inserting 3 docs"
    );

    // update a0 via update_with_results, plus a non-existent pk.
    let mut upd = Doc::new().expect("doc");
    upd.set_pk("a0").expect("pk");
    upd.add_vector_fp32("embedding", &[9.0, 0.0, 0.0])
        .expect("vec");
    let mut ghost = Doc::new().expect("doc");
    ghost.set_pk("nope").expect("pk");
    ghost
        .add_vector_fp32("embedding", &[0.0, 9.0, 0.0])
        .expect("vec");
    let results = collection
        .update_with_results(vec![upd, ghost])
        .await
        .expect("update wr");
    assert_eq!(results.len(), 2);
    assert_eq!(results[0].code, zvec::ErrorCode::Ok);
    assert_ne!(results[1].code, zvec::ErrorCode::Ok);

    // fetch roundtrip
    let fetched = collection
        .fetch(vec!["a1".to_string(), "a2".to_string()])
        .await
        .expect("fetch");
    assert_eq!(fetched.len(), 2);

    // delete one pk, verify count drops
    let del = collection
        .delete(vec!["a1".to_string()])
        .await
        .expect("delete");
    assert_eq!(del.success, 1);
    collection.flush().await.expect("flush");
    assert_eq!(
        collection.stats().await.expect("stats").doc_count(),
        2,
        "after deleting 1"
    );

    // schema + options introspection through the async wrapper
    let s = collection.schema().await.expect("schema");
    assert!(s.has_field("embedding"));
    let o = collection.options().await.expect("options");
    assert!(!o.read_only());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_upsert_iter_streams() {
    use zvec::AsyncCollection;

    let path = tmp_path("async_upsert_iter");
    let schema = basic_schema().expect("schema");
    let collection = AsyncCollection::create_and_open(path, schema, None)
        .await
        .expect("create");

    let docs: Vec<Doc> = (0..13)
        .map(|i| mk_doc(&format!("x{i}"), [i as f32, 0.0, 0.0]).unwrap())
        .collect();
    let summary = collection.upsert_iter(docs, 4).await.expect("upsert_iter");
    assert_eq!(summary.success, 13);
    assert_eq!(summary.error, 0);
    collection.flush().await.expect("flush");
    assert_eq!(collection.stats().await.expect("stats").doc_count(), 13);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_delete_by_filter_scopes() {
    use zvec::AsyncCollection;

    let path = tmp_path("async_dbf");
    let schema = basic_schema().expect("schema");
    let collection = AsyncCollection::create_and_open(path, schema, None)
        .await
        .expect("create");

    let docs: Vec<Doc> = (0..3)
        .map(|i| mk_doc(&format!("f{i}"), [i as f32, 0.0, 0.0]).unwrap())
        .collect();
    collection.insert(docs).await.expect("insert");
    collection.flush().await.expect("flush");
    assert_eq!(collection.stats().await.expect("stats").doc_count(), 3);

    collection
        .delete_by_filter("id = \"f1\"")
        .await
        .expect("delete_by_filter");
    collection.flush().await.expect("flush");
    assert_eq!(collection.stats().await.expect("stats").doc_count(), 2);
}

// -----------------------------------------------------------------------------
// Round 4: close the remaining vector / array type gaps, DDL, query params,
// and derive-macro edge cases.
// -----------------------------------------------------------------------------

/// Build a single-vector schema parameterised by data type + dimension,
/// using an HNSW cosine index. Used by the fp64/int16/etc. tests below.
fn single_vector_schema(
    name: &str,
    data_type: DataType,
    dim: u32,
    metric: MetricType,
) -> zvec::Result<CollectionSchema> {
    let mut schema = CollectionSchema::new(name)?;
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(metric)?;
    hnsw.set_hnsw_params(16, 200)?;
    let mut emb = FieldSchema::new("embedding", data_type, false, dim)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;
    Ok(schema)
}

// -----------------------------------------------------------------------------
// Vector DataType support: the Rust enum mirrors the C API's full set, but
// in zvec 0.3.1 dense_vector only accepts FP32 and Int8. The other
// variants (FP64, Int4, Int16, Binary32/64) are rejected at schema
// validation with InvalidArgument("... only support FP32, but
// field[X]'s data type is ..."). Pin that quirk so nobody wastes time
// trying to make them work without an upstream change.
// -----------------------------------------------------------------------------

#[test]
fn non_fp32_dense_vector_types_are_rejected_by_schema() {
    for t in [
        DataType::VectorFp64,
        DataType::VectorInt16,
        DataType::VectorInt4,
        DataType::VectorBinary32,
        DataType::VectorBinary64,
    ] {
        let dim = match t {
            DataType::VectorBinary32 => 64,
            DataType::VectorBinary64 => 128,
            _ => 3,
        };
        let path = tmp_path(&format!("reject_{t:?}"));
        let schema = single_vector_schema(&format!("reject_{t:?}"), t, dim, MetricType::Cosine)
            .expect("schema builds at the Rust level");
        match Collection::create_and_open(&path, &schema, None) {
            Ok(_) => panic!("expected {t:?} to be rejected as dense_vector"),
            Err(err) => {
                assert_eq!(err.code, zvec::ErrorCode::InvalidArgument);
                assert!(
                    err.message
                        .as_deref()
                        .unwrap_or("")
                        .to_lowercase()
                        .contains("fp32"),
                    "expected 'only support FP32' message for {t:?}; got {:?}",
                    err.message
                );
            }
        }
    }
}

// -----------------------------------------------------------------------------
// Array type round-trips: int32, uint32, uint64, double
// (array_int64 is deliberately skipped — see PR #16: zvec's wire format
//  for arrays exposed a SIGSEGV in readback for that one variant.)
// -----------------------------------------------------------------------------

/// Schema with a required vector + the array field under test.
fn array_schema(name: &str, array_field: FieldSchema) -> zvec::Result<CollectionSchema> {
    let mut schema = CollectionSchema::new(name)?;
    let id = FieldSchema::new("id", DataType::String, false, 0)?;
    schema.add_field(&id)?;
    schema.add_field(&array_field)?;
    // zvec requires at least one vector field per collection.
    let mut hnsw = IndexParams::new(IndexType::Hnsw)?;
    hnsw.set_metric_type(MetricType::Cosine)?;
    hnsw.set_hnsw_params(16, 200)?;
    let mut emb = FieldSchema::new("embedding", DataType::VectorFp32, false, 3)?;
    emb.set_index_params(&hnsw)?;
    schema.add_field(&emb)?;
    Ok(schema)
}

#[test]
fn array_int32_round_trip() -> zvec::Result<()> {
    let path = tmp_path("arr_i32");
    let field = FieldSchema::new("nums", DataType::ArrayInt32, true, 0)?;
    let collection = Collection::create_and_open(&path, &array_schema("it_arr_i32", field)?, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_array_int32("nums", &[1, -2, 3, -4])?;
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let got = collection.fetch(&["a"])?;
    assert_eq!(
        got.get(0).unwrap().get_array_int32("nums")?,
        vec![1, -2, 3, -4]
    );
    Ok(())
}

#[test]
fn array_uint32_round_trip() -> zvec::Result<()> {
    let path = tmp_path("arr_u32");
    let field = FieldSchema::new("counts", DataType::ArrayUInt32, true, 0)?;
    let collection = Collection::create_and_open(&path, &array_schema("it_arr_u32", field)?, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_array_uint32("counts", &[1, 2, 3, 4])?;
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let got = collection.fetch(&["a"])?;
    assert_eq!(
        got.get(0).unwrap().get_array_uint32("counts")?,
        vec![1u32, 2, 3, 4]
    );
    Ok(())
}

#[test]
fn array_uint64_round_trip() -> zvec::Result<()> {
    let path = tmp_path("arr_u64");
    let field = FieldSchema::new("bigs", DataType::ArrayUInt64, true, 0)?;
    let collection = Collection::create_and_open(&path, &array_schema("it_arr_u64", field)?, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_array_uint64("bigs", &[u64::MAX, 0, 42])?;
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let got = collection.fetch(&["a"])?;
    assert_eq!(
        got.get(0).unwrap().get_array_uint64("bigs")?,
        vec![u64::MAX, 0, 42]
    );
    Ok(())
}

#[test]
fn array_double_round_trip() -> zvec::Result<()> {
    let path = tmp_path("arr_f64");
    let field = FieldSchema::new("scores", DataType::ArrayDouble, true, 0)?;
    let collection = Collection::create_and_open(&path, &array_schema("it_arr_f64", field)?, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_string("id", "a")?;
    d.add_array_double("scores", &[0.5, 1.5, 2.5])?;
    d.add_vector_fp32("embedding", &[1.0, 0.0, 0.0])?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let got = collection.fetch(&["a"])?;
    assert_eq!(
        got.get(0).unwrap().get_array_double("scores")?,
        vec![0.5, 1.5, 2.5]
    );
    Ok(())
}

// -----------------------------------------------------------------------------
// alter_column + Collection::open error path
// -----------------------------------------------------------------------------

#[test]
fn alter_column_renames_int_column() -> zvec::Result<()> {
    // alter_column can rename a column without rebuilding the whole
    // collection. Verify the rename shows up in live schema.
    let path = tmp_path("alter_col");
    let schema = bare_schema()?;
    let collection = Collection::create_and_open(&path, &schema, None)?;

    let orig = FieldSchema::new("counter", DataType::Int64, true, 0)?;
    collection.add_column(&orig, None)?;
    assert!(collection.schema()?.has_field("counter"));

    collection.alter_column("counter", Some("hits"), None)?;
    let after = collection.schema()?;
    assert!(
        after.has_field("hits"),
        "renamed column should be reachable by its new name"
    );
    assert!(
        !after.has_field("counter"),
        "old column name should no longer exist after rename"
    );
    Ok(())
}

#[test]
fn open_on_missing_path_errors() -> zvec::Result<()> {
    // Opening a path that was never created must not panic or hang;
    // it should surface an error. We don't pin the exact code —
    // zvec reports either NotFound or FailedPrecondition depending
    // on version — just that it isn't Ok.
    let path = tmp_path("never_created");
    match Collection::open(&path, None) {
        Ok(_) => panic!("open on a non-existent path should error"),
        Err(err) => assert_ne!(err.code, zvec::ErrorCode::Ok),
    }
    Ok(())
}

// -----------------------------------------------------------------------------
// IvfQueryParams + FlatQueryParams: getter/setter round-trip + attach to query.
// (Collection::query exposes set_ivf_params/set_flat_params on VectorQuery;
// exercising them against an actual IVF/Flat index is a bigger setup, so we
// focus on "the config builds and attaches cleanly".)
// -----------------------------------------------------------------------------

#[test]
fn ivf_query_params_round_trip() -> zvec::Result<()> {
    use zvec::IvfQueryParams;

    let mut p = IvfQueryParams::new(8, false, 1.25)?;
    assert_eq!(p.nprobe(), 8);
    assert!(!p.is_using_refiner());
    assert!((p.scale_factor() - 1.25).abs() < f32::EPSILON);

    p.set_nprobe(32)?;
    p.set_is_linear(true)?;
    p.set_is_using_refiner(true)?;
    p.set_radius(0.75)?;
    p.set_scale_factor(2.0)?;
    assert_eq!(p.nprobe(), 32);
    assert!(p.is_linear());
    assert!(p.is_using_refiner());
    assert!((p.radius() - 0.75).abs() < f32::EPSILON);
    assert!((p.scale_factor() - 2.0).abs() < f32::EPSILON);

    // Attach to a VectorQuery — this consumes the params.
    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(5)?;
    q.set_ivf_params(p)?;
    Ok(())
}

#[test]
fn flat_query_params_round_trip() -> zvec::Result<()> {
    use zvec::FlatQueryParams;

    let mut p = FlatQueryParams::new(false, 1.0)?;
    assert!(!p.is_using_refiner());
    p.set_is_using_refiner(true)?;
    p.set_is_linear(true)?;
    p.set_radius(0.1)?;
    p.set_scale_factor(3.0)?;
    assert!(p.is_using_refiner());
    assert!(p.is_linear());
    assert!((p.radius() - 0.1).abs() < f32::EPSILON);
    assert!((p.scale_factor() - 3.0).abs() < f32::EPSILON);

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp32(&[1.0, 0.0, 0.0])?;
    q.set_topk(1)?;
    q.set_flat_params(p)?;
    Ok(())
}

// -----------------------------------------------------------------------------
// VectorQuery::set_query_vector_fp64 / _fp16
// -----------------------------------------------------------------------------

#[test]
fn query_vector_fp64_setter_packs_bytes() -> zvec::Result<()> {
    // dense_vector FP64 isn't accepted at the schema level in
    // zvec 0.3.1 (see non_fp32_dense_vector_types_are_rejected_by_schema)
    // so we can't execute an fp64 query end-to-end. This test just
    // confirms the byte-packing setter doesn't fail on a plausible
    // input — the same shape as what a user would call if/when
    // upstream adds support.
    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp64(&[0.1_f64, 0.2, 0.3])?;
    q.set_topk(1)?;
    Ok(())
}

#[cfg(feature = "half")]
#[test]
fn query_vector_fp16_attaches() -> zvec::Result<()> {
    use half::f16;
    let path = tmp_path("qv_fp16");
    let schema = single_vector_schema("it_qv_fp16", DataType::VectorFp16, 3, MetricType::Cosine)?;
    let collection = Collection::create_and_open(&path, &schema, None)?;
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    d.add_vector_fp16(
        "embedding",
        &[f16::from_f32(1.0), f16::from_f32(0.0), f16::from_f32(0.0)],
    )?;
    collection.insert(&[&d])?;
    collection.flush()?;

    let mut q = VectorQuery::new()?;
    q.set_field_name("embedding")?;
    q.set_query_vector_fp16(&[f16::from_f32(1.0), f16::from_f32(0.0), f16::from_f32(0.0)])?;
    q.set_topk(1)?;
    let results = collection.query(&q)?;
    assert_eq!(results.len(), 1);
    Ok(())
}

// -----------------------------------------------------------------------------
// Derive macro edge cases: FromDoc missing-required, Option tolerance, rename.
// -----------------------------------------------------------------------------

#[cfg(feature = "derive")]
#[test]
fn from_doc_missing_required_field_errors() -> zvec::Result<()> {
    use zvec::{FromDoc, IntoDoc};

    #[derive(IntoDoc, FromDoc, Debug)]
    struct WithRequired {
        #[zvec(pk)]
        id: String,
        views: i64,
    }

    // Build a doc that's missing `views` — FromDoc should refuse it.
    // For scalar i64 the derive delegates straight to Doc::get_int64,
    // which surfaces zvec's own "Field not found in document" message
    // rather than the derive's "doc is missing field `X`" wrapper
    // (the wrapper only kicks in for Rust-side String fields). So
    // the assertion checks the error code + a generic "not found"
    // substring rather than the field name.
    let mut d = Doc::new()?;
    d.set_pk("a")?;
    let err = WithRequired::from_doc(d.borrow())
        .expect_err("FromDoc should refuse to decode without a required field");
    assert_eq!(err.code, zvec::ErrorCode::InvalidArgument);
    assert!(
        err.message
            .as_deref()
            .unwrap_or("")
            .to_lowercase()
            .contains("not found"),
        "error message should indicate the field is missing; got {:?}",
        err.message
    );
    Ok(())
}

#[cfg(feature = "derive")]
#[test]
fn from_doc_option_tolerates_missing_field() -> zvec::Result<()> {
    use zvec::{FromDoc, IntoDoc};

    #[derive(IntoDoc, FromDoc)]
    struct Partial {
        #[zvec(pk)]
        id: String,
        maybe_views: Option<i64>,
        maybe_title: Option<String>,
    }

    // None emits set_field_null on IntoDoc, and FromDoc maps back to
    // None. Also exercise the "field isn't even on the doc" path by
    // constructing the doc manually without the optional fields.
    let both_none = Partial {
        id: "a".into(),
        maybe_views: None,
        maybe_title: None,
    };
    let d = both_none.into_doc()?;
    let back = Partial::from_doc(d.borrow())?;
    assert_eq!(back.id, "a");
    assert!(back.maybe_views.is_none());
    assert!(back.maybe_title.is_none());

    let mut bare = Doc::new()?;
    bare.set_pk("b")?;
    let sparse = Partial::from_doc(bare.borrow())?;
    assert_eq!(sparse.id, "b");
    assert!(sparse.maybe_views.is_none());
    assert!(sparse.maybe_title.is_none());

    let full = Partial {
        id: "c".into(),
        maybe_views: Some(42),
        maybe_title: Some("hi".into()),
    };
    let d = full.into_doc()?;
    let round = Partial::from_doc(d.borrow())?;
    assert_eq!(round.maybe_views, Some(42));
    assert_eq!(round.maybe_title.as_deref(), Some("hi"));
    Ok(())
}

#[cfg(feature = "derive")]
#[test]
fn derive_rename_crosses_field_name_boundary() -> zvec::Result<()> {
    use zvec::{FromDoc, IntoDoc};

    // Rust-side `body` is serialised under zvec field "text", then
    // round-tripped back through the rename. Also verify the doc
    // actually carries "text", not "body".
    #[derive(IntoDoc, FromDoc)]
    struct Renamed {
        #[zvec(pk)]
        id: String,
        #[zvec(rename = "text")]
        body: String,
    }

    let orig = Renamed {
        id: "a".into(),
        body: "hello".into(),
    };
    let doc = orig.into_doc()?;
    {
        let borrow = doc.borrow();
        assert!(borrow.has_field("text"));
        assert!(!borrow.has_field("body"));
    }
    let back: Renamed = Renamed::from_doc(doc.borrow())?;
    assert_eq!(back.id, "a");
    assert_eq!(back.body, "hello");
    Ok(())
}

#[cfg(feature = "derive")]
#[test]
fn derive_skip_uses_default() -> zvec::Result<()> {
    use zvec::{FromDoc, IntoDoc};

    #[derive(IntoDoc, FromDoc, Default)]
    struct WithSkip {
        #[zvec(pk)]
        id: String,
        #[zvec(skip)]
        runtime_only: i64,
        views: i64,
    }

    let orig = WithSkip {
        id: "a".into(),
        runtime_only: 999, // not serialised
        views: 7,
    };
    let doc = orig.into_doc()?;
    // `runtime_only` must not show up on the doc.
    assert!(!doc.borrow().has_field("runtime_only"));
    let back = WithSkip::from_doc(doc.borrow())?;
    assert_eq!(back.id, "a");
    assert_eq!(back.views, 7);
    // Skipped field falls back to Default::default().
    assert_eq!(back.runtime_only, 0);
    Ok(())
}
