//! Environment variables, dates, the default DB path and the call log.

use crate::prelude::*;

pub(crate) fn env_flag(k: &str) -> bool {
    std::env::var_os(k).map(|v| !v.is_empty()).unwrap_or(false)
}

pub(crate) fn env_int(k: &str, d: i64) -> i64 {
    match std::env::var(k) {
        Ok(v) => match v.trim().parse() {
            Ok(n) => n,
            Err(_) => {
                eprintln!("mem: error: {k}={v:?} is not an integer");
                std::process::exit(1)
            }
        },
        Err(_) => d,
    }
}

pub(crate) fn today() -> String {
    chrono::Local::now().date_naive().format("%Y-%m-%d").to_string()
}

/// Where the store is and which rule chose it (shown by `mem where`).
pub(crate) fn db_path() -> String {
    db_location().0
}

/// Store resolution, first match wins (docs/DECISIONS.md, D-17):
/// 1. `MEM_DB`
/// 2. a project-local binary: the exe sits in a folder named `.bin`, or `<exe dir>/../.mem` already exists
///    → `<exe dir>/../.mem/mem.db` (the reference layout `<project>/.bin/mem`)
/// 3. an installed binary (winget, PATH): the nearest `.mem/mem.db` in the current folder or a parent
/// 4. none yet: a new `.mem/mem.db` at the nearest folder holding `.git`, else in the current folder
pub(crate) fn db_location() -> (String, String) {
    use std::path::{Path, PathBuf};
    if let Some(p) = std::env::var_os("MEM_DB").filter(|p| !p.is_empty()) {
        return (p.to_string_lossy().into_owned(), "MEM_DB".into());
    }
    let s = |p: PathBuf| p.to_string_lossy().into_owned();
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf)) {
        let in_bin = dir.file_name().is_some_and(|n| n == ".bin");
        if let Some(base) = dir.parent() {
            if in_bin || base.join(".mem").is_dir() {
                return (s(base.join(".mem").join("mem.db")), "next to the binary (<project>/.bin)".into());
            }
        }
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    if let Some(d) = cwd.ancestors().find(|d| d.join(".mem").join("mem.db").is_file()) {
        return (s(d.join(".mem").join("mem.db")), "nearest .mem/ from the current folder".into());
    }
    match cwd.ancestors().find(|d| d.join(".git").exists()) {
        Some(root) => (s(root.join(".mem").join("mem.db")), "new store at the git root".into()),
        None => (s(cwd.join(".mem").join("mem.db")), "new store in the current folder".into()),
    }
}

/// `mem where`: the resolved store, model and build, without opening or creating anything.
pub(crate) fn where_text() -> String {
    let (db, why) = db_location();
    let model = model_path(&db);
    let exists = |p: &str| std::path::Path::new(p).exists();
    let emb = if !cfg!(feature = "embed") {
        "off (this build has no embed feature)"
    } else if env_flag("MEM_NO_EMBED") {
        "off (MEM_NO_EMBED)"
    } else if exists(&model) {
        "on"
    } else {
        "off (no model file)"
    };
    format!(
        "db: {db} ({})\n  chosen by: {why}\nmodel: {model} ({})\nembeddings: {emb}\nversion: {} ({})",
        if exists(&db) { "exists" } else { "created on first use" },
        if exists(&model) { "found" } else { "not found" },
        env!("CARGO_PKG_VERSION"),
        if cfg!(feature = "embed") { "embed build" } else { "keyword-only build" },
    )
}

pub(crate) fn json_str(s: &str) -> String {
    serde_json::to_string(s).unwrap()
}

pub(crate) fn log_call(db: &str, argv: &[String], out_chars: usize) {
    let dir = std::path::Path::new(db).parent().map(|p| p.to_path_buf()).unwrap_or_default();
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs_f64()).unwrap_or(0.0);
    let line = format!(
        "{{\"t\": {}, \"argv\": [{}], \"out_chars\": {}}}\n",
        py_float_repr(t),
        argv.iter().map(|a| json_str(a)).collect::<Vec<_>>().join(", "),
        out_chars
    );
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(dir.join("calls.log")) {
        let _ = f.write_all(line.as_bytes());
    }
}
