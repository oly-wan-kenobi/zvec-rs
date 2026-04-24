//! Cookbook: `#[derive(IntoDoc)]` / `#[derive(FromDoc)]`.
//!
//! Shows the round trip: a user struct → `Doc` (IntoDoc) → inserted,
//! queried, then read back into the same user struct (FromDoc) — so
//! you don't hand-write `Doc::add_*` / `get_*` for every field.
//!
//! Run with:
//!   cargo run --example derive --features "bundled derive"

#[cfg(not(feature = "derive"))]
fn main() {
    eprintln!("re-run with --features \"bundled derive\"");
}

#[cfg(feature = "derive")]
fn main() -> zvec::Result<()> {
    use zvec::{
        Collection, CollectionSchema, FieldSchema, FromDoc, IntoDoc, MetricType, VectorQuery,
    };

    // Fields map 1:1 to zvec's doc surface. The attribute-hinted ones
    // are `Vec<_>`s where the same Rust type can land in multiple
    // zvec DataTypes (here: `Vec<f32>` → VectorFp32).
    #[derive(IntoDoc, FromDoc, Debug)]
    struct Article {
        #[zvec(pk)]
        id: String,
        title: String,
        #[zvec(vector_fp32)]
        embedding: Vec<f32>,
        // Optional fields read back as `None` when the doc didn't set them.
        summary: Option<String>,
    }

    // Schema built via the builder API; the field names have to match
    // the struct's serialised names (overridable with
    // `#[zvec(rename = "...")]`).
    let schema = CollectionSchema::builder("articles")
        .field(FieldSchema::string("id").invert_index(true, false))
        .field(FieldSchema::string("title").nullable(true))
        .field(FieldSchema::string("summary").nullable(true))
        .field(
            FieldSchema::vector_fp32("embedding", 3)
                .hnsw(16, 200)
                .metric(MetricType::Cosine),
        )
        .build()?;

    let path = tmp("zvec_cookbook_derive");
    let collection = Collection::create_and_open(&path, &schema, None)?;

    // Write side: IntoDoc replaces a pile of `add_*` boilerplate.
    let articles = [
        Article {
            id: "a".into(),
            title: "Rust ownership".into(),
            embedding: vec![0.9, 0.1, 0.0],
            summary: Some("a short primer".into()),
        },
        Article {
            id: "b".into(),
            title: "Vector databases".into(),
            embedding: vec![0.1, 0.9, 0.0],
            summary: None,
        },
    ];
    let docs: Vec<_> = articles
        .iter()
        .map(Article::into_doc)
        .collect::<zvec::Result<_>>()?;
    let refs: Vec<_> = docs.iter().collect();
    collection.insert(&refs)?;
    collection.flush()?;

    // Read side: FromDoc replaces a pile of `get_*` boilerplate.
    // `query` returns hits with pk + score but (by default) strips
    // non-projected fields, so we query for the top matches, then
    // `fetch` the full rows by pk.
    let q = VectorQuery::builder()
        .field("embedding")
        .vector_fp32(&[0.1, 0.9, 0.0])
        .topk(5)
        .build()?;
    let hits = collection.query(&q)?;
    let pks: Vec<String> = hits.iter().filter_map(|h| h.pk_copy()).collect();
    let pk_refs: Vec<&str> = pks.iter().map(String::as_str).collect();
    let rows = collection.fetch(&pk_refs)?;
    for row in rows.iter() {
        let article = Article::from_doc(row)?;
        println!("{:?}", article);
    }
    Ok(())
}

#[cfg(feature = "derive")]
fn tmp(name: &str) -> String {
    let mut p = std::env::temp_dir();
    p.push(name);
    let _ = std::fs::remove_dir_all(&p);
    p.to_string_lossy().to_string()
}
