//! SQLite: schema, migrations, connection setup, row helpers, the `Note` row type.

use crate::prelude::*;

pub(crate) type R<T> = rusqlite::Result<T>;

pub(crate) fn err(msg: String) -> rusqlite::Error {
    rusqlite::Error::ToSqlConversionFailure(msg.into())
}

pub(crate) const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS memory(id TEXT PRIMARY KEY, rowid_ INTEGER UNIQUE, project TEXT, kind TEXT, date TEXT, title TEXT,\n  summary TEXT, body TEXT, tags TEXT, status TEXT DEFAULT 'current', superseded_by TEXT)",
    "CREATE TABLE IF NOT EXISTS synonym(term TEXT, alt TEXT, PRIMARY KEY(term, alt))",
];

pub(crate) const FTS3: &str = "CREATE VIRTUAL TABLE IF NOT EXISTS fw3 USING fts5(title, summary, body, tags, kind, content='', prefix='2 3', tokenize='porter unicode61')";

pub(crate) fn vref_str(v: ValueRef) -> Option<String> {
    match v {
        ValueRef::Null => None,
        ValueRef::Integer(i) => Some(i.to_string()),
        ValueRef::Real(f) => Some(py_float_repr(f)),
        ValueRef::Text(t) => Some(String::from_utf8_lossy(t).into_owned()),
        ValueRef::Blob(b) => Some(format!("b{:?}", String::from_utf8_lossy(b))),
    }
}

pub(crate) fn col(r: &rusqlite::Row, i: usize) -> R<Option<String>> {
    Ok(vref_str(r.get_ref(i)?))
}

pub(crate) fn cols(r: &rusqlite::Row, i: usize) -> R<String> {
    Ok(col(r, i)?.unwrap_or_default())
}

pub(crate) fn disp(o: &Option<String>) -> &str {
    o.as_deref().unwrap_or("None")
}

#[derive(Clone, Debug)]
pub(crate) struct Note {
    pub(crate) id: String,
    pub(crate) kind: String,
    pub(crate) date: Option<String>,
    pub(crate) title: String,
    pub(crate) summary: String,
    pub(crate) source: Option<String>,
    pub(crate) body: Option<String>,
}

pub(crate) const NOTE_COLS: &str = "id, kind, date, project, title, summary, source, body";

pub(crate) fn note_from(r: &rusqlite::Row) -> R<Note> {
    Ok(Note {
        id: cols(r, 0)?,
        kind: cols(r, 1)?,
        date: col(r, 2)?,
        title: cols(r, 4)?,
        summary: cols(r, 5)?,
        source: col(r, 6)?,
        body: col(r, 7)?,
    })
}

pub(crate) fn model_path(db: &str) -> String {
    match std::env::var("MEM_MODEL") {
        Ok(m) if !m.is_empty() => m,
        _ => std::path::Path::new(db)
            .parent()
            .map(|p| p.join("model.gguf"))
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned(),
    }
}

pub(crate) fn embed_enabled(db: &str) -> bool {
    cfg!(feature = "embed") && std::path::Path::new(&model_path(db)).exists() && !env_flag("MEM_NO_EMBED")
}

#[cfg(feature = "embed")]
pub(crate) fn register_vec() {
    // sqlite-vec is compiled in; register it for every connection opened from here on
    unsafe {
        #[allow(clippy::missing_transmute_annotations)]
        rusqlite::ffi::sqlite3_auto_extension(Some(std::mem::transmute(sqlite_vec::sqlite3_vec_init as *const ())));
    }
}

pub(crate) fn connect(path: &str, embed_on: bool) -> R<Connection> {
    #[cfg(feature = "embed")]
    register_vec();
    if let Some(dir) = std::path::Path::new(path).parent() {
        if !dir.as_os_str().is_empty() {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    let db = Connection::open(path)?;
    db.busy_timeout(std::time::Duration::from_secs(30))?;
    let _: String = db.query_row("PRAGMA journal_mode=WAL", [], |r| r.get(0))?;
    let needs = |db: &Connection| -> R<bool> {
        let have: HashSet<String> =
            db.prepare("SELECT name FROM sqlite_master")?.query_map([], |r| cols(r, 0))?.collect::<R<_>>()?;
        let cols_: HashSet<String> = if have.contains("memory") {
            db.prepare("PRAGMA table_info(memory)")?.query_map([], |r| cols(r, 1))?.collect::<R<_>>()?
        } else {
            HashSet::new()
        };
        let ok = ["memory", "synonym", "fw3"].iter().all(|t| have.contains(*t))
            && ["source", "updated", "reason"].iter().all(|c| cols_.contains(*c));
        Ok(!ok)
    };
    if needs(&db)? {
        db.execute_batch("BEGIN IMMEDIATE")?;
        let r = (|| -> R<()> {
            for s in SCHEMA {
                db.execute_batch(s)?;
            }
            let cols_: HashSet<String> =
                db.prepare("PRAGMA table_info(memory)")?.query_map([], |r| cols(r, 1))?.collect::<R<_>>()?;
            for (c, ddl) in [("source", "TEXT DEFAULT 'user'"), ("updated", "TEXT"), ("reason", "TEXT")] {
                if !cols_.contains(c) {
                    db.execute_batch(&format!("ALTER TABLE memory ADD COLUMN {c} {ddl}"))?;
                }
            }
            if db.query_row("SELECT 1 FROM sqlite_master WHERE name='fw3'", [], |_| Ok(())).optional()?.is_none() {
                db.execute_batch(FTS3)?;
                let rows: Vec<(Value, Value, Value, Value, Value, Value)> = db
                    .prepare("SELECT rowid_, title, summary, body, tags, kind FROM memory")?
                    .query_map([], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)))?
                    .collect::<R<_>>()?;
                for (rid, t, s, b, tg, k) in rows {
                    let b = if matches!(b, Value::Null) { Value::Text(String::new()) } else { b };
                    let tg = if matches!(tg, Value::Null) { Value::Text(String::new()) } else { tg };
                    db.execute(
                        "INSERT INTO fw3(rowid, title, summary, body, tags, kind) VALUES (?,?,?,?,?,?)",
                        params![rid, t, s, b, tg, k],
                    )?;
                }
            }
            Ok(())
        })();
        match r {
            Ok(()) => db.execute_batch("COMMIT")?,
            Err(e) => {
                let _ = db.execute_batch("ROLLBACK");
                return Err(e);
            }
        }
    }
    if embed_on
        && db.query_row("SELECT 1 FROM sqlite_master WHERE name='memory_emb'", [], |_| Ok(())).optional()?.is_none()
    {
        db.execute_batch("BEGIN IMMEDIATE")?;
        db.execute_batch("CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding bit[768])")?;
        db.execute_batch("CREATE TABLE IF NOT EXISTS memory_emb(rowid_ INTEGER PRIMARY KEY, e BLOB)")?;
        db.execute_batch("COMMIT")?;
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(name: &str) -> (std::path::PathBuf, String) {
        let dir = std::env::temp_dir().join(format!("mem-db-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("mem.db").to_string_lossy().into_owned();
        (dir, p)
    }

    #[test]
    fn fresh_store_gets_schema_and_no_synonyms() {
        // synonyms come from the agents (mem syn add), not from the code: docs/DECISIONS.md D-20
        let (dir, p) = temp_db("fresh");
        let db = connect(&p, false).unwrap();
        let n: i64 = db.query_row("SELECT count(*) FROM synonym", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 0);
        let mode: String = db.query_row("PRAGMA journal_mode", [], |r| r.get(0)).unwrap();
        assert_eq!(mode, "wal");
        drop(db);
        // reopening an up-to-date store with an empty synonym table must not rerun the setup transaction
        let db = connect(&p, false).unwrap();
        db.execute_batch("BEGIN; INSERT INTO synonym VALUES ('a','b'); ROLLBACK;").unwrap();
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn old_store_is_migrated_and_reindexed() {
        let (dir, p) = temp_db("old");
        {
            let db = Connection::open(&p).unwrap();
            db.execute_batch(
                "CREATE TABLE memory(id TEXT PRIMARY KEY, rowid_ INTEGER UNIQUE, project TEXT, kind TEXT, date TEXT, title TEXT,
                 summary TEXT, body TEXT, tags TEXT, status TEXT DEFAULT 'current', superseded_by TEXT);
                 INSERT INTO memory VALUES ('pr-fact-001', 1, 'pr', 'fact', '2026-01-01', 'Legacy note title', 'old summary', NULL, NULL, 'current', NULL);",
            )
            .unwrap();
        }
        let db = connect(&p, false).unwrap();
        let cols: Vec<String> = db
            .prepare("PRAGMA table_info(memory)")
            .unwrap()
            .query_map([], |r| r.get(1))
            .unwrap()
            .map(|x| x.unwrap())
            .collect();
        for c in ["source", "updated", "reason"] {
            assert!(cols.contains(&c.to_string()), "{c}");
        }
        let hit: i64 = db.query_row("SELECT rowid FROM fw3 WHERE fw3 MATCH 'legacy'", [], |r| r.get(0)).unwrap();
        assert_eq!(hit, 1);
        drop(db);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn values_print_like_python() {
        assert_eq!(disp(&None), "None");
        assert_eq!(vref_str(ValueRef::Integer(7)), Some("7".into()));
        assert_eq!(vref_str(ValueRef::Null), None);
    }
}
