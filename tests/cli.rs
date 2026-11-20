//! End-to-end behaviour of the `mem` binary: one process per command, as agents use it.

mod common;
use common::*;

#[test]
fn version_is_0_0_1() {
    let s = Store::new();
    let o = s.run(&["--version"]);
    assert_eq!(o.code, 0);
    assert_eq!(o.out.trim(), "mem 0.0.1");
}

#[test]
fn fresh_store_has_brief_and_no_synonyms_until_agents_add_them() {
    let s = Store::new();
    let o = s.run(&["brief", "--project", "demo"]);
    assert_eq!(o.code, 0, "{}", o.err);
    assert!(o.out.starts_with("# demo: 0 current notes ()"), "{}", o.out);
    assert!(s.db().exists());
    assert!(s.run(&["syn", "show"]).out.starts_with("no synonyms yet."));
    assert_eq!(
        s.run(&["syn", "add", "Wishlist", "위시리스트", "찜목록"]).out.trim(),
        "synonyms: wishlist = 위시리스트 = 찜목록"
    );
    let syn = s.run(&["syn", "show"]);
    assert!(
        syn.out.lines().any(|l| l == "위시리스트: wishlist 찜목록" || l == "위시리스트: 찜목록 wishlist"),
        "{}",
        syn.out
    );
}

#[test]
fn add_then_get_search_and_list() {
    let s = Store::new();
    let id = s.add(
        "convention",
        "Wishlist incident alert channel",
        "SEV2 alerts go to #wishlist-alerts",
        &["--tags", "wishlist,alert"],
    );
    assert_eq!(id, "de-conv-001");
    let g = s.run(&["get", &id]);
    assert!(g.out.starts_with("de-conv-001 · convention · "), "{}", g.out);
    assert!(g.out.contains(
        "· demo · current · source user\nWishlist incident alert channel\nSEV2 alerts go to #wishlist-alerts"
    ));
    assert!(g.out.ends_with("tags: wishlist,alert\n"));
    let q = s.run(&["search", "--project", "demo", "--q", "where do wishlist alerts go?", "--kw", "wishlist,alert"]);
    assert!(q.out.starts_with("[1] de-conv-001 · convention · "), "{}", q.out);
    assert!(q.out.contains("    ↳ SEV2 alerts go to #wishlist-alerts"));
    let l = s.run(&["list", "--project", "demo", "--full"]);
    assert!(
        l.out.starts_with("1 notes\nde-conv-001 · convention · Wishlist incident alert channel\n    SEV2 alerts"),
        "{}",
        l.out
    );
    assert_eq!(s.run(&["get", "nope"]).out.trim(), "nope: not found");
}

#[test]
fn korean_query_finds_korean_note_through_josa_and_synonyms() {
    let s = Store::new();
    let id = s.add("gotcha", "환불 재시도 제한", "환불은 3번까지만 재시도한다", &[]);
    let q = s.run(&["search", "--project", "demo", "--q", "환불을 몇 번 재시도하나요?"]);
    assert!(q.out.starts_with(&format!("[1] {id}")), "{}", q.out);
    // no synonym yet: the English query misses the Korean note
    assert!(s.run(&["search", "--project", "demo", "--q", "refund retries"]).out.starts_with("No results"));
    // an agent registers the project's words once; every later search uses them
    s.run(&["syn", "add", "환불", "refund"]);
    s.run(&["syn", "add", "재시도", "retry", "retries"]);
    let q = s.run(&["search", "--project", "demo", "--q", "refund retries"]);
    assert!(q.out.starts_with(&format!("[1] {id}")), "{}", q.out);
}

#[test]
fn duplicate_is_refused_and_force_saves() {
    let s = Store::new();
    let id = s.add("fact", "Coupons batch chunk size", "1000 rows per chunk", &[]);
    let o = s.run(&[
        "add",
        "--project",
        "demo",
        "--kind",
        "fact",
        "--title",
        "coupons batch chunk size",
        "--summary",
        "1000 rows per chunk",
    ]);
    assert!(o.out.starts_with(&format!("NOT SAVED: looks like a duplicate of {id}")), "{}", o.out);
    let o = s.run(&[
        "add",
        "--project",
        "demo",
        "--kind",
        "fact",
        "--title",
        "Coupons batch chunk size",
        "--summary",
        "1000 rows per chunk",
        "--force",
    ]);
    assert_eq!(o.code, 0);
    assert!(o.out.starts_with("saved de-fact-002"));
}

#[test]
fn supersede_hides_the_old_note() {
    let s = Store::new();
    let old = s.add("fact", "Search API latency target", "p95 under 500ms", &["--tags", "search"]);
    let o = s.run(&[
        "add",
        "--project",
        "demo",
        "--kind",
        "fact",
        "--title",
        "Search API latency target",
        "--summary",
        "p95 under 300ms",
        "--supersedes",
        &old,
    ]);
    assert!(o.out.starts_with(&format!("saved de-fact-002 (superseded {old})")), "{}", o.out);
    let q = s.run(&["search", "--project", "demo", "--q", "search latency target"]);
    assert!(q.out.contains("p95 under 300ms") && !q.out.contains("p95 under 500ms"), "{}", q.out);
    let g = s.run(&["get", &old]);
    assert!(g.out.contains("· superseded → de-fact-002 ·"), "{}", g.out);
    let l = s.run(&["list", "--status", "superseded"]);
    assert!(l.out.contains(&old));
}

#[test]
fn same_subject_must_choose_and_keep_both_is_flagged_in_search() {
    let s = Store::new();
    let a = s.add("fact", "Cart worker count", "Cart runs 8 workers", &["--tags", "cart"]);
    let args = [
        "add",
        "--project",
        "demo",
        "--kind",
        "fact",
        "--title",
        "Cart worker count = 2",
        "--summary",
        "Two more in the EU region",
    ];
    let o = s.run(&args);
    assert!(o.out.starts_with("NOT SAVED: a current note on the same subject already exists:"), "{}", o.out);
    assert!(o.out.contains(&a) && o.out.contains("--keep-both"));
    let mut keep = args.to_vec();
    keep.push("--keep-both");
    let b = s.run(&keep).saved_id();
    let q = s.run(&["search", "--project", "demo", "--q", "cart worker count"]);
    assert!(q.out.contains(&format!("SAME SUBJECT, also current (newer: {b})")), "{}", q.out);
}

#[test]
fn same_subject_check_treats_registered_synonyms_as_one_word() {
    let s = Store::new();
    let a = s.add("convention", "Wishlist alert channel", "#wish-alerts", &[]);
    let args = [
        "add",
        "--project",
        "demo",
        "--kind",
        "convention",
        "--title",
        "위시리스트 알림 채널",
        "--summary",
        "#shop-ops",
    ];
    // without synonyms the Korean title is a different subject (literal words)
    assert!(s.run(&args).out.starts_with("saved "));
    let s = Store::new();
    let a2 = s.add("convention", "Wishlist alert channel", "#wish-alerts", &[]);
    assert_eq!(a, a2);
    s.run(&["syn", "add", "wishlist", "위시리스트"]);
    s.run(&["syn", "add", "alert", "알림", "notification"]);
    s.run(&["syn", "add", "channel", "채널"]);
    let o = s.run(&args);
    assert!(o.out.starts_with("NOT SAVED: a current note on the same subject already exists:"), "{}", o.out);
    assert!(o.out.contains(&a2));
    // MEM_NO_SYN turns synonyms off here too
    let lit = s.run_env(
        &[
            "add",
            "--project",
            "demo",
            "--kind",
            "convention",
            "--title",
            "Wishlist notification channel",
            "--summary",
            "#x",
        ],
        &[("MEM_NO_SYN", "1")],
    );
    assert!(lit.out.starts_with("saved "), "{}", lit.out);
    // and search marks the pair that is now current twice
    let q = s.run(&["search", "--project", "demo", "--q", "wishlist alert channel"]);
    assert!(q.out.contains("SAME SUBJECT, also current"), "{}", q.out);
}

#[test]
fn add_batch_check_then_save_with_bad_lines_reported() {
    let s = Store::new();
    let old = s.add("fact", "Image cache TTL", "300 seconds", &["--tags", "media,cache"]);
    let file = s.write(
        "notes.jsonl",
        &format!(
            "{}\n\n{}\n[1, 2]\n{}\n{}\n",
            r#"{"kind": "fact", "title": "Catalog cache TTL", "summary": "60 seconds", "tags": "catalog,cache"}"#,
            r#"{"kind": "fact", "title": "Image cache TTL", "summary": "600 seconds", "supersedes": ["OLD"]}"#
                .replace("OLD", &old),
            r#"{"kind": "fact", "summary": "no title"}"#,
            r#"{"kind": "fact", "title": "Catalog cache TTL", "summary": "60 seconds"}"#
        ),
    );
    let c = s.run(&["add-batch", "--project", "demo", "--check", &file]);
    assert_eq!(c.code, 0, "{}", c.err);
    assert!(c.out.contains("1: new · Catalog cache TTL"), "{}", c.out);
    assert!(c.out.contains(&format!("3: new (replaces {old}) · Image cache TTL")), "{}", c.out);
    assert!(c.out.contains("4: NOT SAVED: bad line (not a JSON object)"));
    assert!(c.out.contains("5: NOT SAVED: bad line ('title')"));
    assert_eq!(s.run(&["list"]).out.lines().next(), Some("1 notes"), "check must not save");
    let o = s.run(&["add-batch", "--project", "demo", &file]);
    let lines: Vec<&str> = o.out.lines().filter(|l| l.chars().next().is_some_and(|c| c.is_ascii_digit())).collect();
    assert!(lines[0].starts_with("1: saved de-fact-002"), "{}", o.out);
    assert!(lines[1].starts_with(&format!("3: saved de-fact-003 (superseded {old})")), "{}", o.out);
    assert!(lines[4].starts_with("6: NOT SAVED: looks like a duplicate of de-fact-002"), "{}", o.out);
    let stdin = s.run_stdin(
        &["add-batch", "--project", "demo", "-"],
        r#"{"kind": "gotcha", "title": "Webhook signature header", "summary": "X-Sig, HMAC-SHA256"}"#,
    );
    assert!(stdin.out.starts_with("1: saved de-gotc-004"), "{}", stdin.out);
}

#[test]
fn edit_retire_and_append() {
    let s = Store::new();
    let id = s.add("fact", "Coupons deploy day", "Tuesday", &["--tags", "coupons"]);
    assert_eq!(s.run(&["edit", &id, "--summary", "Thursday"]).out.trim(), format!("edited {id}"));
    assert_eq!(s.run(&["edit", &id, "--append", "moved after the Q3 freeze"]).out.trim(), format!("edited {id}"));
    let g = s.run(&["get", &id]);
    assert!(g.out.contains("\nThursday\nmoved after the Q3 freeze\n"), "{}", g.out);
    assert!(s.run(&["search", "--q", "coupons thursday"]).out.starts_with(&format!("[1] {id}")));
    assert_eq!(s.run(&["edit", "zzz", "--title", "x"]).out.trim(), "zzz: not found");
    assert_eq!(
        s.run(&["retire", &id, "zzz", "--reason", "wrong"]).out.trim(),
        format!("retired {id}, zzz (not found)")
    );
    assert!(s.run(&["search", "--q", "coupons thursday"]).out.starts_with("No results."));
    let l = s.run(&["list", "--status", "all"]);
    assert!(l.out.contains(&format!("{id} · fact · retired · Coupons deploy day")), "{}", l.out);
    assert!(s.run(&["get", &id]).out.contains("· retired (wrong) ·"));
}

#[test]
fn import_keeps_ids_and_dates() {
    let s = Store::new();
    let f = s.write(
        "imp.jsonl",
        r#"{"id": "x-fact-042", "project": "demo", "kind": "fact", "date": "2025-01-02", "title": "Imported note", "summary": "from another store"}"#,
    );
    assert_eq!(s.run(&["import", &f]).out.trim(), "imported");
    assert!(s.run(&["get", "x-fact-042"]).out.starts_with("x-fact-042 · fact · 2025-01-02 · demo · current"));
    let bad = s.write("bad.jsonl", r#"{"id": "x-1", "kind": "fact"}"#);
    let o = s.run(&["import", &bad]);
    assert_eq!(o.code, 1);
    assert!(o.err.starts_with("mem: error: line 1: missing 'project'"), "{}", o.err);
}

#[test]
fn add_after_import_skips_ids_already_taken() {
    // imported notes keep their ids but get new rowids, so the next number can already be in use
    let s = Store::new();
    let f = s.write(
        "imp.jsonl",
        r#"{"id": "de-fact-002", "project": "demo", "kind": "fact", "date": "2025-01-02", "title": "Imported note", "summary": "from another store"}"#,
    );
    assert_eq!(s.run(&["import", &f]).out.trim(), "imported");
    assert_eq!(s.add("fact", "Coupons batch chunk size", "1000 rows per chunk", &[]), "de-fact-003");
    assert_eq!(s.add("fact", "Refund retry limit", "3 retries", &[]), "de-fact-004");
}

#[test]
fn values_starting_with_a_dash_and_argument_errors() {
    let s = Store::new();
    let id = s.add(
        "topic",
        "Testing rules",
        "Project-wide test rules",
        &["--body", "- Run unit tests before push (since 2026-01-01)"],
    );
    assert!(s.run(&["get", &id]).out.contains("\n- Run unit tests before push (since 2026-01-01)\n"));
    let o =
        s.run(&["add", "--project", "demo", "--kind", "fact", "--title", "t", "--summary", "s", "--tags", "--force"]);
    assert_eq!(o.code, 2);
    assert!(o.err.contains("argument --tags: expected one argument"));
    assert_eq!(s.run(&["search"]).code, 2);
    assert_eq!(s.run(&["frobnicate"]).code, 2);
    let o = s.run_env(&["search", "--q", "x"], &[("MEM_LIMIT", "abc")]);
    assert_eq!(o.code, 1);
    assert!(o.err.contains("MEM_LIMIT"));
}

#[cfg(not(feature = "embed"))]
#[test]
fn index_needs_the_embed_build() {
    let s = Store::new();
    let o = s.run(&["index"]);
    assert_eq!(o.code, 1);
    assert!(o.err.contains("--features embed"), "{}", o.err);
}

#[test]
fn every_call_is_logged_next_to_the_db() {
    let s = Store::new();
    s.run(&["search", "--q", "찜 \"quoted\""]);
    let log = std::fs::read_to_string(s.dir.join("calls.log")).unwrap();
    let last: serde_json::Value = serde_json::from_str(log.lines().last().unwrap()).unwrap();
    assert_eq!(last["argv"], serde_json::json!(["search", "--q", "찜 \"quoted\""]));
    assert!(last["out_chars"].as_u64().unwrap() > 0);
}

/// Run a copy of the binary placed at `exe` from `cwd`, without MEM_DB.
fn run_copy(exe: &std::path::Path, cwd: &std::path::Path, args: &[&str]) -> Out {
    if !exe.exists() {
        std::fs::create_dir_all(exe.parent().unwrap()).unwrap();
        std::fs::copy(bin(), exe).unwrap();
    }
    // another test thread may fork while the copy is open ("text file busy"): retry briefly
    let o = (0..50)
        .find_map(|_| {
            match std::process::Command::new(exe)
                .args(args)
                .current_dir(cwd)
                .env_remove("MEM_DB")
                .env("MEM_NO_EMBED", "1")
                .output()
            {
                Ok(o) => Some(o),
                Err(e) if e.raw_os_error() == Some(26) => {
                    std::thread::sleep(std::time::Duration::from_millis(100));
                    None
                }
                Err(e) => panic!("{e}"),
            }
        })
        .expect("could not run the copied binary");
    Out {
        code: o.status.code().unwrap_or(-1),
        out: String::from_utf8_lossy(&o.stdout).into_owned(),
        err: String::from_utf8_lossy(&o.stderr).into_owned(),
    }
}

fn exe_name() -> std::ffi::OsString {
    std::path::Path::new(bin()).file_name().unwrap().to_os_string()
}

#[test]
fn default_db_is_dot_mem_next_to_dot_bin() {
    let s = Store::new();
    let proj = s.dir.join("proj");
    let exe = proj.join(".bin").join(exe_name());
    let elsewhere = s.dir.join("elsewhere");
    std::fs::create_dir_all(&elsewhere).unwrap();
    assert_eq!(run_copy(&exe, &elsewhere, &["brief", "--project", "p"]).code, 0);
    assert!(proj.join(".mem").join("mem.db").exists());
    assert!(!elsewhere.join(".mem").exists());
}

#[test]
fn installed_binary_finds_the_nearest_store_upward() {
    let s = Store::new();
    let exe = s.dir.join("tools").join(exe_name()); // like a winget/PATH install: not in a .bin folder
    let repo = s.dir.join("repo");
    let deep = repo.join("src").join("module");
    std::fs::create_dir_all(&deep).unwrap();
    std::fs::create_dir_all(repo.join(".git")).unwrap();
    // no store yet: a new one goes to the git root, even when called from a subfolder
    let w = run_copy(&exe, &deep, &["where"]);
    // compare the tail only: the temp folder may be spelled differently by the child (macOS /private/var symlink)
    let tail = std::path::Path::new("repo").join(".mem").join("mem.db");
    assert!(w.out.contains(&*tail.to_string_lossy()), "{}", w.out);
    assert!(w.out.contains("created on first use") && w.out.contains("git root"), "{}", w.out);
    assert!(!repo.join(".mem").exists(), "where must not create anything");
    let id = run_copy(
        &exe,
        &deep,
        &["add", "--project", "demo", "--kind", "fact", "--title", "Found upward", "--summary", "x"],
    )
    .saved_id();
    assert!(repo.join(".mem").join("mem.db").exists());
    // later calls from anywhere inside the repo use the same store
    assert!(run_copy(&exe, &repo.join("src"), &["get", &id]).out.contains("Found upward"));
    let w = run_copy(&exe, &deep, &["where"]);
    assert!(w.out.contains("(exists)") && w.out.contains("nearest .mem/"), "{}", w.out);
    // outside any repo and store: the current folder
    let loose = s.dir.join("loose");
    std::fs::create_dir_all(&loose).unwrap();
    assert!(run_copy(&exe, &loose, &["where"]).out.contains("new store in the current folder"));
}

#[test]
fn where_reports_mem_db_and_build() {
    let s = Store::new();
    let o = s.run(&["where"]);
    assert_eq!(o.code, 0);
    assert!(o.out.starts_with(&format!("db: {} (", s.db().display())), "{}", o.out);
    assert!(o.out.contains("chosen by: MEM_DB"));
    assert!(o.out.contains("version: 0.0.1"));
}

#[test]
fn concurrent_writers_lose_nothing() {
    let s = Store::new();
    s.run(&["brief", "--project", "demo"]); // create the schema once
    let threads: Vec<_> = (0..6)
        .map(|w| {
            let db = s.db();
            std::thread::spawn(move || {
                for i in 0..8 {
                    let o = std::process::Command::new(bin())
                        .args([
                            "add",
                            "--project",
                            "demo",
                            "--kind",
                            "fact",
                            "--title",
                            &format!("Writer {w} note {i} qz{w}x{i}"),
                            "--summary",
                            &format!("value {w}{i}"),
                            "--force",
                        ])
                        .env("MEM_DB", &db)
                        .env("MEM_NO_EMBED", "1")
                        .output()
                        .unwrap();
                    assert!(String::from_utf8_lossy(&o.stdout).starts_with("saved "), "{:?}", o);
                }
            })
        })
        .collect();
    for t in threads {
        t.join().unwrap();
    }
    let l = s.run(&["list", "--limit", "1000"]);
    assert!(l.out.starts_with("48 notes"), "{}", l.out);
    let ids: std::collections::HashSet<&str> = l.out.lines().skip(1).filter_map(|x| x.split(" · ").next()).collect();
    assert!(!ids.is_empty());
}
