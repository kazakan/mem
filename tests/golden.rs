//! Byte-for-byte compatibility with the Python reference implementation (`reference/mem53.py`).
//! tests/fixtures/golden.json holds the reference's output for real agent queries on tests/fixtures/shoplite.db.

mod common;
use common::*;

#[test]
fn matches_python_reference_output() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/golden.json");
    let g: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap();
    let s = Store::from_fixture("shoplite.db");
    let mut failed = vec![];
    let cases = g["cases"].as_array().unwrap();
    for c in cases {
        let args: Vec<&str> = c["args"].as_array().unwrap().iter().map(|a| a.as_str().unwrap()).collect();
        let o = s.run(&args);
        if o.code != c["code"].as_i64().unwrap() as i32 || o.out != c["out"].as_str().unwrap() {
            failed.push(format!("{args:?}\n--- expected\n{}\n--- got\n{}{}", c["out"].as_str().unwrap(), o.out, o.err));
        }
    }
    assert!(failed.is_empty(), "{} of {} cases differ; first:\n{}", failed.len(), cases.len(), failed[0]);
}
