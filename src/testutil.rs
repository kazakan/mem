//! Test helpers: a throwaway store in a temp folder (removed on drop).

use crate::prelude::*;
use std::sync::atomic::{AtomicUsize, Ordering};

static N: AtomicUsize = AtomicUsize::new(0);

pub(crate) struct TempStore {
    pub(crate) dir: std::path::PathBuf,
    pub(crate) mem: Mem,
}

impl TempStore {
    pub(crate) fn new() -> TempStore {
        let dir = std::env::temp_dir().join(format!(
            "mem-unit-{}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst),
            std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mem.db");
        let mut mem = Mem::open(path.to_str().unwrap()).unwrap();
        // unit tests must not depend on the caller's MEM_* environment
        mem.embed_on = false;
        mem.no_syn = false;
        mem.no_group = false;
        mem.card_lines = 6;
        mem.topic_show = 1500;
        mem.sim_show = 8;
        TempStore { dir, mem }
    }

    /// add a note with --force semantics (no duplicate / same-subject refusal); returns its id
    pub(crate) fn add(&self, kind: &str, title: &str, summary: &str, tags: &str) -> String {
        let (msg, id) =
            self.mem.do_add("pr", kind, title, summary, Some(""), Some(tags), "", Some("user"), true, true).unwrap();
        id.unwrap_or_else(|| panic!("not saved: {msg}"))
    }

    /// register synonyms like `mem syn add a b c` (stores start with none)
    pub(crate) fn syn(&self, words: &[&str]) {
        for a in words {
            for b in words {
                if a != b {
                    self.mem.db.execute("INSERT OR IGNORE INTO synonym VALUES (?,?)", [a, b]).unwrap();
                }
            }
        }
    }
}

impl Drop for TempStore {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
