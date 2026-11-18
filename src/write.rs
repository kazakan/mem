//! Saving: duplicate and same-subject checks, `add`, and the JSON-lines reader for `add-batch` / `import`.

use crate::prelude::*;

pub(crate) fn json_lines(raw: &[u8]) -> Vec<Result<String, String>> {
    // Python text mode: universal newlines (\r\n and \r become \n), lines keep their newline
    let mut out = vec![];
    let mut cur: Vec<u8> = vec![];
    let mut i = 0;
    let push = |cur: &mut Vec<u8>, out: &mut Vec<Result<String, String>>| {
        let b = std::mem::take(cur);
        out.push(String::from_utf8(b).map_err(|e| format!("invalid UTF-8 at byte {}", e.utf8_error().valid_up_to())));
    };
    while i < raw.len() {
        let c = raw[i];
        if c == b'\r' || c == b'\n' {
            cur.push(b'\n');
            if c == b'\r' && i + 1 < raw.len() && raw[i + 1] == b'\n' {
                i += 1;
            }
            push(&mut cur, &mut out);
        } else {
            cur.push(c);
        }
        i += 1;
    }
    if !cur.is_empty() {
        push(&mut cur, &mut out);
    }
    out
}

pub(crate) fn truthy(v: Option<&serde_json::Value>) -> bool {
    use serde_json::Value as J;
    match v {
        None | Some(J::Null) => false,
        Some(J::Bool(b)) => *b,
        Some(J::Number(n)) => n.as_f64().map(|f| f != 0.0).unwrap_or(true),
        Some(J::String(s)) => !s.is_empty(),
        Some(J::Array(a)) => !a.is_empty(),
        Some(J::Object(o)) => !o.is_empty(),
    }
}

pub(crate) fn json_sql(v: &serde_json::Value) -> Result<Value, String> {
    use serde_json::Value as J;
    Ok(match v {
        J::Null => Value::Null,
        J::Bool(b) => Value::Integer(*b as i64),
        J::Number(n) => match n.as_i64() {
            Some(i) => Value::Integer(i),
            None => Value::Real(n.as_f64().unwrap_or(0.0)),
        },
        J::String(s) => Value::Text(s.clone()),
        _ => return Err("lists and objects cannot be stored".into()),
    })
}

/// required string field: Err("'key'") like Python's KeyError text
pub(crate) fn req<'a>(o: &'a serde_json::Map<String, serde_json::Value>, k: &str) -> Result<&'a str, String> {
    match o.get(k) {
        None => Err(format!("'{k}'")),
        Some(serde_json::Value::String(s)) => Ok(s),
        Some(_) => Err(format!("'{k}' must be a string")),
    }
}

/// supersedes: a string "id1,id2" or (Rust only) a JSON list of ids
pub(crate) fn sup_field(o: &serde_json::Map<String, serde_json::Value>) -> Result<String, String> {
    match o.get("supersedes") {
        Some(serde_json::Value::Array(a)) => a
            .iter()
            .map(|x| {
                x.as_str().map(|s| s.to_string()).ok_or_else(|| "'supersedes' list must hold id strings".to_string())
            })
            .collect::<Result<Vec<_>, _>>()
            .map(|v| v.join(",")),
        _ => Ok(opt(o, "supersedes", None)?.unwrap_or("").to_string()),
    }
}

/// optional string field with Python o.get(k, default) semantics; JSON null -> None
pub(crate) fn opt<'a>(
    o: &'a serde_json::Map<String, serde_json::Value>,
    k: &str,
    d: Option<&'a str>,
) -> Result<Option<&'a str>, String> {
    match o.get(k) {
        None => Ok(d),
        Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(format!("'{k}' must be a string")),
    }
}

/// one add-batch line. Ok(Err(..)) = bad line (reported, batch continues); Err = database error (batch stops).
pub(crate) fn batch_line(m: &Mem, project: &str, check: bool, line: &str) -> R<Result<String, String>> {
    let o: serde_json::Value = match serde_json::from_str(line) {
        Ok(v) => v,
        Err(e) => return Ok(Err(e.to_string())),
    };
    let Some(o) = o.as_object() else { return Ok(Err("not a JSON object".into())) };
    let pj = Some(project);
    if check {
        let sup_s = match sup_field(o) {
            Ok(s) => s,
            Err(e) => return Ok(Err(e)),
        };
        let sup: Vec<String> = sup_s.split(',').map(|x| py_strip(x).to_string()).filter(|x| !x.is_empty()).collect();
        let (title, summary) = match (req(o, "title"), req(o, "summary")) {
            (Ok(t), Ok(s)) => (t, s),
            (Err(e), _) | (_, Err(e)) => return Ok(Err(e)),
        };
        let kind = o.get("kind").and_then(|k| k.as_str()).unwrap_or("");
        let d = m.find_dupe(pj, title, summary, &sup)?;
        let sim: Vec<Note> =
            m.run_search(&format!("{title} {summary}"), "", pj, 3, None)?.into_iter().map(|x| x.0).collect();
        let subj: Vec<String> =
            tokens(title).into_iter().filter(|t| !VALUEISH.is_match(t) && !UPPER_VAR.is_match(t)).collect();
        let subj = subj.join(" ");
        let same: Vec<Note> = if !subj.is_empty() {
            m.run_search(&subj, "", pj, 3, None)?.into_iter().map(|x| x.0).collect()
        } else {
            vec![]
        };
        let mut msg = match &d {
            Some(d) => format!("DUPLICATE of {d}"),
            None => "new".into(),
        };
        if !sup.is_empty() {
            msg += &format!(" (replaces {})", sup.join(", "));
        }
        msg += &format!(" · {}", cut(title, 50));
        let mut seen: HashSet<String> = sup.iter().cloned().collect();
        if !truthy(o.get("keep_both")) {
            for r in m.same_subject_notes(pj, kind, title, &sup)? {
                seen.insert(r.0.clone());
                msg += &format!(
                    "\n     ! SAME SUBJECT, decide supersedes or keep_both: {} · {} — {}",
                    r.0,
                    cut(&r.2, 60),
                    cut(&r.3, 80)
                );
            }
        }
        for (tag, ms) in [("same topic", &same), ("similar", &sim)] {
            for x in ms {
                if !seen.contains(&x.id) {
                    seen.insert(x.id.clone());
                    msg += &format!("\n     ~ {tag}: {} · {}", x.id, cut(&x.title, 70));
                }
            }
        }
        Ok(Ok(msg))
    } else {
        let f = || -> Result<_, String> {
            Ok((
                req(o, "kind")?,
                req(o, "title")?,
                req(o, "summary")?,
                opt(o, "body", Some(""))?,
                opt(o, "tags", Some(""))?,
                sup_field(o)?,
                opt(o, "source", Some("user"))?,
            ))
        };
        let (kind, title, summary, body, tags, sup, source) = match f() {
            Ok(x) => x,
            Err(e) => return Ok(Err(e)),
        };
        let (msg, _) = m.do_add(
            project,
            kind,
            title,
            summary,
            body,
            tags,
            &sup,
            source,
            truthy(o.get("force")),
            truthy(o.get("keep_both")),
        )?;
        Ok(Ok(msg))
    }
}

impl Mem {
    pub(crate) fn same_subject_notes(
        &self,
        project: Option<&str>,
        kind: &str,
        title: &str,
        exclude: &[String],
    ) -> R<Vec<(String, String, String, String)>> {
        if !SUBJ_KINDS.contains(&kind) {
            return Ok(vec![]);
        }
        let sn = subj2(title);
        if sn.len() < 2 {
            return Ok(vec![]);
        }
        let toks = self.expand(sorted_vec(&sn))?;
        let mut out = vec![];
        for (r, _) in self.fts_ranked(&toks, project, None, 30)? {
            let (id, k, t, s, stt): (String, String, String, String, Option<String>) = self.db.query_row(
                "SELECT id, kind, title, summary, status FROM memory WHERE rowid_ = ?",
                [r],
                |x| Ok((cols(x, 0)?, cols(x, 1)?, cols(x, 2)?, cols(x, 3)?, col(x, 4)?)),
            )?;
            if exclude.contains(&id) || stt.as_deref() != Some("current") || !SUBJ_KINDS.contains(&k.as_str()) {
                continue;
            }
            if self.same_subject(&sn, &subj2(&t))? {
                out.push((id, k, t, s));
            }
        }
        Ok(out)
    }

    pub(crate) fn find_dupe(
        &self,
        project: Option<&str>,
        title: &str,
        summary: &str,
        exclude: &[String],
    ) -> R<Option<String>> {
        let both = format!("{title} {summary}");
        let nw = lower_set(&tokens(&both));
        for (r, _) in self.fts_ranked(&tokens(&both), project, None, 5)? {
            let (oid, ot, os_): (String, String, String) =
                self.db.query_row("SELECT id, title, summary FROM memory WHERE rowid_ = ?", [r], |x| {
                    Ok((cols(x, 0)?, cols(x, 1)?, cols(x, 2)?))
                })?;
            if exclude.contains(&oid) {
                continue;
            }
            let ow = lower_set(&tokens(&format!("{ot} {os_}")));
            let jac = !nw.is_empty()
                && !ow.is_empty()
                && (nw.intersection(&ow).count() as f64 / nw.union(&ow).count() as f64) >= 0.85;
            if py_strip(&ot).to_lowercase() == py_strip(title).to_lowercase() || jac {
                return Ok(Some(oid));
            }
        }
        Ok(None)
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) fn do_add(
        &self,
        project: &str,
        kind: &str,
        title: &str,
        summary: &str,
        body: Option<&str>,
        tags: Option<&str>,
        supersedes: &str,
        source: Option<&str>,
        force: bool,
        keep_both: bool,
    ) -> R<(String, Option<String>)> {
        let sup: Vec<String> =
            supersedes.split(',').map(|s| py_strip(s).to_string()).filter(|s| !s.is_empty()).collect();
        let pj = Some(project);
        for s in &sup {
            let r: Option<(Option<String>, Option<String>)> = self
                .db
                .query_row("SELECT status, superseded_by FROM memory WHERE id = ?", [s], |x| {
                    Ok((col(x, 0)?, col(x, 1)?))
                })
                .optional()?;
            match r {
                None => return Ok((format!("NOT SAVED: --supersedes {s} not found"), None)),
                Some((st, by)) => {
                    if st.as_deref() != Some("current") && !force {
                        let by = by.filter(|b| !b.is_empty()).map(|b| format!(" (by {b})")).unwrap_or_default();
                        return Ok((format!("NOT SAVED: {s} is already {}{by}; nothing to replace.", disp(&st)), None));
                    }
                }
            }
        }
        if !force {
            if let Some(d) = self.find_dupe(pj, title, summary, &sup)? {
                return Ok((format!("NOT SAVED: looks like a duplicate of {d}. Use `mem edit {d}` to change it, --supersedes {d} to replace it, or --force."), None));
            }
        }
        if !force && !keep_both {
            let left = self.same_subject_notes(pj, kind, title, &sup)?;
            if !left.is_empty() {
                let lst = left
                    .iter()
                    .map(|r| format!("{} · {} — {}", r.0, cut(&r.2, 70), cut(&r.3, 100)))
                    .collect::<Vec<_>>()
                    .join("\n  ");
                let what = if !sup.is_empty() {
                    "also current on the same subject (not in --supersedes)"
                } else {
                    "a current note on the same subject already exists"
                };
                return Ok((
                    format!(
                        "NOT SAVED: {what}:\n  {lst}\n  Decide: if the new statement replaces it (now / only / instead / no longer / moved to; 이제·~만·대신·더 이상·옮겼어), add its id to --supersedes; if both stay true (also / in addition; ~도·에 더해), re-run with --keep-both."
                    ),
                    None,
                ));
            }
        }
        let similar =
            if sup.is_empty() { self.run_search(&format!("{title} {summary}"), "", pj, 3, None)? } else { vec![] };
        // the model runs before the write lock is taken
        let vec = if self.embed_on { self.try_embed(&doc_text(title, summary, body)) } else { None };
        self.db.execute_batch("BEGIN IMMEDIATE")?;
        let r = (|| -> R<Result<String, String>> {
            if !force {
                if let Some(d) = self.find_dupe(pj, title, summary, &sup)? {
                    return Ok(Err(format!(
                        "NOT SAVED: looks like a duplicate of {d} (just added by another session)."
                    )));
                }
            }
            for s in &sup {
                let st: Option<String> = self
                    .db
                    .query_row("SELECT status FROM memory WHERE id = ?", [s], |x| col(x, 0))
                    .optional()?
                    .flatten();
                if st.as_deref() != Some("current") && !force {
                    return Ok(Err(format!("NOT SAVED: {s} was superseded by another session just now.")));
                }
            }
            let new_id = self.next_id(project, kind)?;
            let d = today();
            let ov = |o: Option<&str>| o.map(|s| Value::Text(s.to_string())).unwrap_or(Value::Null);
            let rid = self.insert([
                Value::Text(new_id.clone()),
                Value::Text(project.into()),
                Value::Text(kind.into()),
                Value::Text(d.clone()),
                Value::Text(title.into()),
                Value::Text(summary.into()),
                ov(body),
                ov(tags),
                Value::Text("current".into()),
                Value::Null,
                ov(source),
            ])?;
            for s in &sup {
                self.db.execute(
                    "UPDATE memory SET status='superseded', superseded_by=?, updated=? WHERE id=?",
                    params![new_id, d, s],
                )?;
            }
            if let Some(v) = &vec {
                self.put_vec(rid, v)?;
            }
            Ok(Ok(new_id))
        })();
        let new_id = match r {
            Ok(Ok(id)) => {
                self.db.execute_batch("COMMIT")?;
                id
            }
            Ok(Err(msg)) => {
                self.db.execute_batch("ROLLBACK")?;
                return Ok((msg, None));
            }
            Err(e) => {
                let _ = self.db.execute_batch("ROLLBACK");
                return Err(e);
            }
        };
        let mut msg = format!("saved {new_id}");
        if !sup.is_empty() {
            msg += &format!(" (superseded {})", sup.join(", "));
        }
        if !similar.is_empty() {
            msg += "\n  similar current notes (if yours replaces one: mem edit / --supersedes):\n  ";
            msg += &similar
                .iter()
                .map(|(m, _)| format!("{} · {}", m.id, cut(&m.title, 60)))
                .collect::<Vec<_>>()
                .join("\n  ");
        }
        Ok((msg, Some(new_id)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempStore;

    fn obj(s: &str) -> serde_json::Map<String, serde_json::Value> {
        serde_json::from_str::<serde_json::Value>(s).unwrap().as_object().unwrap().clone()
    }

    #[test]
    fn json_lines_use_universal_newlines() {
        let l = json_lines(b"a\r\nb\rc\nd");
        let l: Vec<String> = l.into_iter().map(|x| x.unwrap()).collect();
        assert_eq!(l, vec!["a\n", "b\n", "c\n", "d"]);
        let bad = json_lines(b"ok\n\xff\xfe\n");
        assert!(bad[0].is_ok() && bad[1].is_err());
    }

    #[test]
    fn json_fields_follow_python_get_semantics() {
        let o = obj(r#"{"title": "T", "body": null, "n": 3, "supersedes": ["a", "b"], "keep_both": "yes"}"#);
        assert_eq!(req(&o, "title"), Ok("T"));
        assert_eq!(req(&o, "summary"), Err("'summary'".into())); // Python's KeyError text
        assert!(req(&o, "n").is_err());
        assert_eq!(opt(&o, "body", Some("")), Ok(None));
        assert_eq!(opt(&o, "tags", Some("")), Ok(Some("")));
        assert_eq!(sup_field(&o), Ok("a,b".into()));
        assert!(truthy(o.get("keep_both")));
        assert!(!truthy(o.get("missing")));
        assert!(!truthy(Some(&serde_json::json!(0))));
        assert!(!truthy(Some(&serde_json::json!([]))));
    }

    #[test]
    fn duplicate_is_refused_unless_forced() {
        let s = TempStore::new();
        s.add("fact", "Coupons batch chunk size", "Chunk size is 1000 per run", "coupons,batch");
        let (msg, id) = s
            .mem
            .do_add(
                "pr",
                "fact",
                "Coupons batch chunk size",
                "Chunk size is 1000 per run",
                None,
                None,
                "",
                None,
                false,
                false,
            )
            .unwrap();
        assert!(id.is_none() && msg.starts_with("NOT SAVED: looks like a duplicate of pr-fact-001"), "{msg}");
        let (_, id) = s
            .mem
            .do_add(
                "pr",
                "fact",
                "Coupons batch chunk size",
                "Chunk size is 1000 per run",
                None,
                None,
                "",
                None,
                true,
                false,
            )
            .unwrap();
        assert_eq!(id.as_deref(), Some("pr-fact-002"));
    }

    #[test]
    fn same_title_is_a_duplicate_even_when_replacing_is_meant() {
        let s = TempStore::new();
        let old = s.add("convention", "Wishlist incident alert channel", "Post to #wishlist-ops", "wishlist,alert");
        let (msg, id) = s
            .mem
            .do_add(
                "pr",
                "convention",
                "Wishlist incident alert channel",
                "Now #wishlist-alerts only",
                None,
                None,
                "",
                None,
                false,
                false,
            )
            .unwrap();
        assert!(id.is_none() && msg.contains(&format!("--supersedes {old} to replace it")), "{msg}");
        let (msg, id) = s
            .mem
            .do_add(
                "pr",
                "convention",
                "Wishlist incident alert channel",
                "Now #wishlist-alerts only",
                None,
                None,
                &old,
                None,
                false,
                false,
            )
            .unwrap();
        assert!(msg.starts_with(&format!("saved {} (superseded {old})", id.unwrap())), "{msg}");
    }

    #[test]
    fn same_subject_needs_supersedes_or_keep_both() {
        let s = TempStore::new();
        let old = s.add("convention", "Wishlist incident alert channel", "Post to #wishlist-ops", "wishlist,alert");
        // different wording, same subject once the value (#...) is set aside
        let add = |sup: &str, keep: bool| {
            s.mem
                .do_add(
                    "pr",
                    "convention",
                    "Wishlist incident alert channel #wishlist-alerts",
                    "SEV2 pages go here",
                    None,
                    None,
                    sup,
                    None,
                    false,
                    keep,
                )
                .unwrap()
        };
        let (msg, id) = add("", false);
        assert!(id.is_none() && msg.contains("a current note on the same subject already exists"), "{msg}");
        assert!(msg.contains(&old));
        let (msg, id) = add(&old, false);
        assert!(msg.starts_with(&format!("saved {} (superseded {old})", id.clone().unwrap())), "{msg}");
        let st: String = s.mem.db.query_row("SELECT status FROM memory WHERE id = ?", [&old], |r| r.get(0)).unwrap();
        assert_eq!(st, "superseded");
        // replacing an already-replaced note is refused
        let (msg, id) = add(&old, false);
        assert!(id.is_none() && msg.contains("is already superseded"), "{msg}");
    }

    #[test]
    fn keep_both_saves_alongside() {
        let s = TempStore::new();
        s.add("fact", "Cart worker count", "Cart runs 8 workers", "cart");
        let add = |keep: bool| {
            s.mem
                .do_add(
                    "pr",
                    "fact",
                    "Cart worker count = 2",
                    "Two more in the EU region",
                    None,
                    None,
                    "",
                    None,
                    false,
                    keep,
                )
                .unwrap()
        };
        let (msg, id) = add(false);
        assert!(id.is_none() && msg.contains("same subject"), "{msg}");
        let (_, id) = add(true);
        assert!(id.is_some());
        let n: i64 =
            s.mem.db.query_row("SELECT count(*) FROM memory WHERE status='current'", [], |r| r.get(0)).unwrap();
        assert_eq!(n, 2);
    }

    #[test]
    fn supersedes_unknown_id_is_refused() {
        let s = TempStore::new();
        let (msg, id) =
            s.mem.do_add("pr", "fact", "X y z", "s", None, None, "pr-fact-999", None, false, false).unwrap();
        assert!(id.is_none());
        assert_eq!(msg, "NOT SAVED: --supersedes pr-fact-999 not found");
    }

    #[test]
    fn batch_line_reports_bad_lines_without_failing() {
        let s = TempStore::new();
        assert_eq!(batch_line(&s.mem, "pr", false, "[1,2]\n").unwrap(), Err("not a JSON object".into()));
        assert_eq!(batch_line(&s.mem, "pr", false, "{\"kind\":\"fact\"}\n").unwrap(), Err("'title'".into()));
        let ok = batch_line(
            &s.mem,
            "pr",
            false,
            r#"{"kind":"fact","title":"Search API latency target","summary":"p95 under 300ms"}"#,
        )
        .unwrap();
        assert!(ok.unwrap().starts_with("saved pr-fact-001"));
        let check = batch_line(
            &s.mem,
            "pr",
            true,
            r#"{"kind":"fact","title":"Search API latency target","summary":"p95 under 300ms"}"#,
        )
        .unwrap();
        assert!(check.unwrap().starts_with("DUPLICATE of pr-fact-001"));
    }
}
