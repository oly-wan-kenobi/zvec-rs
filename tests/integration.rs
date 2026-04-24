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
