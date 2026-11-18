//! Output snapshot. tests/fixtures/golden.json holds the expected output of real agent queries (search variants,
//! get, brief, syn, list) on tests/fixtures/shoplite.db. It was first produced by the Python original (mem v5.3),
//! which the Rust port matched byte for byte (docs/DECISIONS.md D-12, D-22); since then it is the output of the
//! last intended change.
//!
//! After an intended output change: `MEM_BLESS=1 cargo test --release --test golden`, review
//! `git diff tests/fixtures/golden.json`, and record why in docs/DECISIONS.md.

mod common;
use common::*;

#[test]
fn output_matches_snapshot() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden.json");
    let mut g: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let bless = std::env::var_os("MEM_BLESS").is_some_and(|v| !v.is_empty());
    let s = Store::from_fixture("shoplite.db");
    let mut failed = vec![];
    let cases = g["cases"].as_array_mut().unwrap();
    let n = cases.len();
    for c in cases.iter_mut() {
        let args: Vec<String> = c["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap().to_string()).collect();
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        let o = s.run(&args);
        if bless {
            c["code"] = o.code.into();
            c["out"] = o.out.clone().into();
        } else if o.code != c["code"].as_i64().unwrap() as i32 || o.out != c["out"].as_str().unwrap() {
            failed.push(format!("{args:?}\n--- expected\n{}\n--- got\n{}{}", c["out"].as_str().unwrap(), o.out, o.err));
        }
    }
    if bless {
        // same layout as the file had: one-space indent, UTF-8 kept as is
        let mut buf = vec![];
        let mut ser =
            serde_json::Serializer::with_formatter(&mut buf, serde_json::ser::PrettyFormatter::with_indent(b" "));
        serde::Serialize::serialize(&g, &mut ser).unwrap();
        std::fs::write(&path, buf).unwrap();
        eprintln!("blessed {n} cases into {}", path.display());
        return;
    }
    assert!(failed.is_empty(), "{} of {n} cases differ (MEM_BLESS=1 to accept); first:\n{}", failed.len(), failed[0]);
}
