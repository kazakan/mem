//! Embedding build only (`cargo test --features embed`). Needs the EmbeddingGemma GGUF file:
//! set MEM_TEST_MODEL=/path/to/embeddinggemma-300M-Q8_0.gguf, otherwise these tests skip themselves.
#![cfg(feature = "embed")]

mod common;
use common::*;

fn model() -> Option<String> {
    let m = std::env::var("MEM_TEST_MODEL").ok().filter(|m| std::path::Path::new(m).exists());
    if m.is_none() {
        eprintln!("skipped: set MEM_TEST_MODEL to an EmbeddingGemma .gguf to run embedding tests");
    }
    m
}

#[test]
fn index_then_semantic_search() {
    let Some(m) = model() else { return };
    let s = Store::from_fixture("shoplite.db");
    let env = [("MEM_MODEL", m.as_str()), ("MEM_NO_EMBED", "")];
    let o = s.run_env(&["index"], &env);
    assert_eq!(o.out.trim(), "indexed 577", "{}", o.err);
    assert_eq!(s.run_env(&["index"], &env).out.trim(), "indexed 0");
    // a paraphrase with no shared keywords still finds the wishlist alert notes through the vectors
    let q = s.run_env(
        &["search", "--project", "shoplite", "--q", "where should outage pages for saved-for-later items be posted"],
        &env,
    );
    assert_eq!(q.code, 0, "{}", q.err);
    assert!(q.out.to_lowercase().contains("wishlist"), "{}", q.out);
    // a new note gets its vector in the same transaction
    let id = s
        .run_env(
            &[
                "add",
                "--project",
                "shoplite",
                "--kind",
                "fact",
                "--title",
                "Gift card expiry",
                "--summary",
                "Gift cards expire after 5 years",
            ],
            &env,
        )
        .saved_id();
    let q = s.run_env(&["search", "--project", "shoplite", "--q", "how long are vouchers valid"], &env);
    assert!(q.out.contains(&id), "{}", q.out);
}

#[test]
fn broken_model_falls_back_to_keyword_search() {
    let Some(m) = model() else { return };
    let s = Store::from_fixture("shoplite.db");
    let bad = s.write("broken.gguf", "not a model");
    let env = [("MEM_MODEL", bad.as_str()), ("MEM_NO_EMBED", "")];
    // vectors exist (made with the good model), so search tries the model and must fall back
    assert_eq!(s.run_env(&["index"], &[("MEM_MODEL", m.as_str()), ("MEM_NO_EMBED", "")]).code, 0);
    let q = s.run_env(&["search", "--project", "shoplite", "--q", "wishlist alert channel"], &env);
    assert_eq!(q.code, 0);
    assert!(q.err.contains("embeddings off for this call"), "{}", q.err);
    assert!(q.out.starts_with("[1] "), "{}", q.out);
    let a = s.run_env(
        &[
            "add",
            "--project",
            "shoplite",
            "--kind",
            "fact",
            "--title",
            "Fallback note",
            "--summary",
            "saved without a vector",
        ],
        &env,
    );
    assert!(a.out.starts_with("saved "), "{}", a.out);
}

#[test]
fn missing_model_means_keyword_search_without_warnings() {
    let s = Store::from_fixture("shoplite.db");
    let q = s.run_env(
        &["search", "--project", "shoplite", "--q", "wishlist alert channel"],
        &[("MEM_MODEL", "/nonexistent/model.gguf"), ("MEM_NO_EMBED", "")],
    );
    assert_eq!(q.code, 0);
    assert!(q.err.is_empty(), "{}", q.err);
}
