//! `Mem`: an open store plus settings; embedding helpers and the low-level insert.

use crate::prelude::*;

pub(crate) struct Mem {
    pub(crate) db: Connection,
    pub(crate) no_syn: bool,
    pub(crate) no_group: bool,
    pub(crate) card_lines: i64,
    pub(crate) topic_show: i64,
    pub(crate) sim_show: i64,
    /// model.gguf present, MEM_NO_EMBED unset, and built with `--features embed`
    pub(crate) embed_on: bool,
    pub(crate) model: String,
    pub(crate) mode: String,
    #[cfg(feature = "embed")]
    pub(crate) emb: std::cell::RefCell<Option<crate::emb::Embedder>>,
    /// set when the model failed to load: search and add go on without embeddings (warned once on stderr)
    pub(crate) emb_broken: std::cell::Cell<bool>,
    /// word → representative of its synonym group, filled on demand (same-subject checks)
    pub(crate) canon: std::cell::RefCell<HashMap<String, String>>,
}

/// Python: f"title: {title} | text: {summary} {body}"[:4000]  (None prints as "None")
pub(crate) fn doc_text(title: &str, summary: &str, body: Option<&str>) -> String {
    cut(&format!("title: {title} | text: {summary} {}", body.unwrap_or("None")), 4000).to_string()
}

impl Mem {
    /// Open (or create) the store at `path` with settings from the environment (MEM_* variables).
    pub(crate) fn open(path: &str) -> R<Mem> {
        let embed_on = embed_enabled(path);
        let db = connect(path, embed_on)?;
        Ok(Mem {
            db,
            embed_on,
            model: model_path(path),
            mode: std::env::var("MEM_MODE").unwrap_or_else(|_| "interleave".into()),
            #[cfg(feature = "embed")]
            emb: std::cell::RefCell::new(None),
            emb_broken: std::cell::Cell::new(false),
            canon: std::cell::RefCell::new(HashMap::new()),
            no_syn: env_flag("MEM_NO_SYN"),
            no_group: env_flag("MEM_NO_GROUP"),
            card_lines: env_int("MEM_CARD", 6),
            topic_show: env_int("MEM_TOPIC_SHOW", 1500),
            sim_show: env_int("MEM_SIMSHOW", 8),
        })
    }

    #[cfg(feature = "embed")]
    pub(crate) fn embed(&self, text: &str) -> R<Vec<u8>> {
        let mut e = self.emb.borrow_mut();
        if e.is_none() {
            *e = Some(crate::emb::Embedder::load(&self.model).map_err(err)?);
        }
        e.as_mut().unwrap().embed(text).map_err(err)
    }

    #[cfg(not(feature = "embed"))]
    pub(crate) fn embed(&self, _text: &str) -> R<Vec<u8>> {
        Err(err("embeddings are not part of this build (build with --features embed)".into()))
    }

    /// embed, or (model unusable) warn once and return None so the caller can go on with FTS only
    pub(crate) fn try_embed(&self, text: &str) -> Option<Vec<u8>> {
        if self.emb_broken.get() {
            return None;
        }
        match self.embed(text) {
            Ok(v) => Some(v),
            Err(e) => {
                self.emb_broken.set(true);
                eprintln!("mem: warning: embeddings off for this call ({e}); using keyword search only. Fix the model file, then run `mem index`.");
                None
            }
        }
    }

    pub(crate) fn put_vec(&self, rid: i64, b: &[u8]) -> R<()> {
        self.db.execute("DELETE FROM memory_vec WHERE rowid = ?", [rid])?; // vec0 has no INSERT OR REPLACE
        self.db
            .execute("INSERT INTO memory_vec(rowid, embedding) VALUES (?, vec_quantize_binary(?))", params![rid, b])?;
        self.db
            .execute("INSERT OR REPLACE INTO memory_emb VALUES (?, vec_quantize_int8(?, 'unit'))", params![rid, b])?;
        Ok(())
    }

    pub(crate) fn use_sem(&self) -> R<bool> {
        Ok(self.embed_on && self.db.query_row("SELECT count(*) FROM memory_emb", [], |r| r.get::<_, i64>(0))? > 0)
    }

    pub(crate) fn next_id(&self, project: &str, kind: &str) -> R<String> {
        let n: i64 = self.db.query_row("SELECT coalesce(max(rowid_), 0) + 1 FROM memory", [], |r| r.get(0))?;
        Ok(format!("{}-{}-{:03}", cut(project, 2), cut(kind, 4), n))
    }

    /// vals: id, project, kind, date, title, summary, body, tags, status, superseded_by, source
    pub(crate) fn insert(&self, v: [Value; 11]) -> R<i64> {
        let rid: i64 = self.db.query_row("SELECT coalesce(max(rowid_), 0) + 1 FROM memory", [], |r| r.get(0))?;
        let [id, project, kind, date, title, summary, body, tags, status, sup, source] = v;
        self.db.execute(
            "INSERT INTO memory(id, rowid_, project, kind, date, title, summary, body, tags, status, superseded_by, source, updated) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
            params![id, rid, project, kind, date.clone(), title.clone(), summary.clone(), body.clone(), tags.clone(), status, sup, source, date],
        )?;
        self.db.execute(
            "INSERT INTO fw3(rowid, title, summary, body, tags, kind) VALUES (?,?,?,?,?,?)",
            params![rid, title, summary, body, tags, kind],
        )?;
        Ok(rid)
    }
}
