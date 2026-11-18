//! Command dispatch: opens the store, runs one command, returns its output text.

use crate::prelude::*;

/// rowid_, title, summary, body of a note without an embedding
type IndexRow = (i64, Option<String>, Option<String>, Option<String>);

pub(crate) fn run(path: &str, cmd: Cmd, partial: &mut String) -> Result<String, Box<dyn std::error::Error>> {
    let m = Mem::open(path)?;
    let locked = matches!(cmd, Cmd::Import { .. } | Cmd::Index | Cmd::Edit { .. } | Cmd::Retire { .. })
        || matches!(&cmd, Cmd::Syn { action, .. } if action == "add");
    if locked {
        m.db.execute_batch("BEGIN IMMEDIATE")?;
    }
    let r = run_cmd(&m, cmd, partial);
    match r {
        Ok(out) => {
            if locked {
                m.db.execute_batch("COMMIT")?;
            }
            Ok(out)
        }
        Err(e) => {
            if locked {
                let _ = m.db.execute_batch("ROLLBACK");
            }
            Err(e)
        }
    }
}

pub(crate) fn run_cmd(m: &Mem, cmd: Cmd, partial: &mut String) -> Result<String, Box<dyn std::error::Error>> {
    let db = &m.db;
    Ok(match cmd {
        Cmd::Import { path } => {
            let raw = std::fs::read(&path)?;
            for (n, line) in json_lines(&raw).into_iter().enumerate() {
                let line = line.map_err(|e| format!("line {}: {e}", n + 1))?;
                if py_strip(&line).is_empty() {
                    continue;
                }
                let o: serde_json::Value = serde_json::from_str(&line).map_err(|e| format!("line {}: {e}", n + 1))?;
                let o = o.as_object().ok_or_else(|| format!("line {}: not a JSON object", n + 1))?;
                let g = |k: &str, d: Option<Value>| -> Result<Value, String> {
                    match o.get(k) {
                        Some(v) => json_sql(v).map_err(|e| format!("line {}: {k}: {e}", n + 1)),
                        None => d.ok_or_else(|| format!("line {}: missing '{k}'", n + 1)),
                    }
                };
                // fields are read in column order, so the first missing one is reported (as Python's KeyError)
                m.insert([
                    g("id", None)?,
                    g("project", None)?,
                    g("kind", None)?,
                    g("date", None)?,
                    g("title", None)?,
                    g("summary", None)?,
                    g("body", Some(Value::Text(String::new())))?,
                    g("tags", Some(Value::Text(String::new())))?,
                    g("status", Some(Value::Text("current".into())))?,
                    g("superseded_by", Some(Value::Null))?,
                    g("source", Some(Value::Text("user".into())))?,
                ])?;
            }
            "imported".into()
        }
        Cmd::Where => where_text(),
        Cmd::Index => {
            if !m.embed_on {
                return Err(if cfg!(feature = "embed") {
                    format!("index: no model at {} (or MEM_NO_EMBED is set)", m.model).into()
                } else {
                    "index: this build has no embeddings (FTS only); build with --features embed".into()
                });
            }
            let todo: Vec<IndexRow> = db
                .prepare("SELECT rowid_, title, summary, body FROM memory WHERE rowid_ NOT IN (SELECT rowid_ FROM memory_emb)")?
                .query_map([], |r| Ok((r.get(0)?, col(r, 1)?, col(r, 2)?, col(r, 3)?)))?
                .collect::<R<_>>()?;
            for (rid, t, su, bo) in &todo {
                let v = m.embed(&doc_text(disp(t), disp(su), bo.as_deref()))?;
                m.put_vec(*rid, &v)?;
            }
            format!("indexed {}", todo.len())
        }
        Cmd::Brief { project: p } => {
            let kinds: Vec<(String, i64)> = db
                .prepare("SELECT kind, count(*) FROM memory WHERE project=? AND status='current' GROUP BY kind ORDER BY 2 DESC")?
                .query_map([&p], |r| Ok((disp(&col(r, 0)?).to_string(), r.get(1)?)))?
                .collect::<R<_>>()?;
            let hand: Vec<[Option<String>; 5]> = db
                .prepare("SELECT id, date, title, summary, body FROM memory WHERE project=? AND kind='handoff' AND status='current' ORDER BY rowid_ DESC LIMIT 2")?
                .query_map([&p], |r| Ok([col(r, 0)?, col(r, 1)?, col(r, 2)?, col(r, 3)?, col(r, 4)?]))?
                .collect::<R<_>>()?;
            let recent: Vec<[Option<String>; 3]> = db
                .prepare("SELECT id, kind, title FROM memory WHERE project=? AND kind != 'handoff' AND (status='current' OR updated=?) ORDER BY coalesce(updated, date) DESC, rowid_ DESC LIMIT 8")?
                .query_map(params![p, today()], |r| Ok([col(r, 0)?, col(r, 1)?, col(r, 2)?]))?
                .collect::<R<_>>()?;
            let total: i64 = kinds.iter().map(|k| k.1).sum();
            let mut lines = vec![
                format!("# {p}: {total} current notes ({})", kinds.iter().map(|(k, n)| format!("{k} {n}")).collect::<Vec<_>>().join(", ")),
                format!("Before deciding: mem search --project {p} --q ... --kw ... · rules overview: mem list --project {p} --kind topic,convention,decision,gotcha"),
            ];
            let topics: Vec<[Option<String>; 3]> = db
                .prepare("SELECT id, title, summary FROM memory WHERE project=? AND kind='topic' AND status='current' ORDER BY title")?
                .query_map([&p], |r| Ok([col(r, 0)?, col(r, 1)?, col(r, 2)?]))?
                .collect::<R<_>>()?;
            if !topics.is_empty() {
                lines.push("## Topic notes (project-wide rules, one note per subject; read the ones for your area: mem get <id>)".into());
                for [i, t, su] in topics.iter().take(40) {
                    lines.push(format!("- {} · {} — {}", disp(i), disp(t), cut(su.as_deref().unwrap_or(""), 90)));
                }
            }
            if !hand.is_empty() {
                lines.push("## Latest handoffs".into());
                for [i, d, t, su, bo] in &hand {
                    lines.push(format!(
                        "- {} · {} · {}\n  {}\n  {}",
                        disp(i),
                        disp(d),
                        disp(t),
                        disp(su),
                        cut(bo.as_deref().unwrap_or(""), 500)
                    ));
                }
            }
            lines.push("## Recently changed".into());
            for [i, k, t] in &recent {
                lines.push(format!("- {} · {} · {}", disp(i), disp(k), disp(t)));
            }
            cut(&lines.join("\n"), 4500).to_string()
        }
        Cmd::Search { q, kw, project, kind, limit } => {
            let limit = limit.unwrap_or_else(|| env_int("MEM_LIMIT", 5));
            let kind: Option<Vec<String>> =
                kind.filter(|k| !k.is_empty()).map(|k| k.split(',').map(|x| py_strip(x).to_string()).collect());
            let res = m.run_search(&q, &kw, project.as_deref(), limit, kind.as_deref())?;
            if res.is_empty() {
                "No results. Try other keywords (synonyms, English and Korean spellings), or mem list.".into()
            } else {
                let cards =
                    if m.card_lines > 0 { m.module_cards(&res, &q, &kw, project.as_deref())? } else { IndexMap::new() };
                m.fmt(&res, &cards)?
            }
        }
        Cmd::List { project, kind, status, tag, source, full, limit } => {
            let mut sql = String::from("SELECT id, kind, date, title, summary, status, source FROM memory WHERE 1=1");
            let mut a: Vec<String> = vec![];
            if let Some(p) = project.filter(|p| !p.is_empty()) {
                sql += " AND project = ?";
                a.push(p);
            }
            if status != "all" {
                sql += " AND status = ?";
                a.push(status);
            }
            if let Some(k) = kind.filter(|k| !k.is_empty()) {
                let ks: Vec<String> = k.split(',').map(|x| py_strip(x).to_string()).collect();
                sql += &format!(" AND kind IN ({})", vec!["?"; ks.len()].join(","));
                a.extend(ks);
            }
            if let Some(t) = tag.filter(|t| !t.is_empty()) {
                sql += " AND (',' || replace(tags, ' ', ',') || ',') LIKE ?";
                a.push(format!("%,{t},%"));
            }
            if let Some(s) = source.filter(|s| !s.is_empty()) {
                sql += " AND coalesce(source, 'user') = ?";
                a.push(s);
            }
            let total: i64 = db.query_row(
                &sql.replace("SELECT id, kind, date, title, summary, status, source", "SELECT count(*)"),
                params_from_iter(&a),
                |r| r.get(0),
            )?;
            let rows: Vec<ListRow> = db
                .prepare(&(sql + " ORDER BY kind, title"))?
                .query_map(params_from_iter(&a), |r| {
                    Ok(ListRow {
                        id: col(r, 0)?,
                        kind: col(r, 1)?,
                        title: col(r, 3)?,
                        summary: col(r, 4)?,
                        status: col(r, 5)?,
                        source: col(r, 6)?,
                    })
                })?
                .collect::<R<_>>()?;
            let nrows = rows.len();
            let groups = if !m.no_group { group_rows(rows) } else { rows.into_iter().map(|r| (r, vec![])).collect() };
            let ng = groups.len();
            let shown = &groups[..py_end(ng, limit)];
            let n_shown: usize = shown.iter().map(|(_, g)| 1 + g.len()).sum();
            let mut head = format!("{total} notes");
            if ng < nrows {
                head += &format!(", same-pattern notes on one line ({ng} lines)");
            }
            if (n_shown as i64) < total {
                head += &format!(" (showing {n_shown})");
            }
            if total > 150 {
                head +=
                    "  · tip: add/add-batch already check duplicates and show similar notes; use search for a topic";
            }
            let mut lines = vec![head];
            let cutn = if shown.len() <= 60 { 400 } else { 110 };
            for (r, mem) in shown {
                let st = disp(&r.status);
                let mut ln = format!(
                    "{} · {}{}{} · {}",
                    disp(&r.id),
                    disp(&r.kind),
                    if st != "current" { format!(" · {st}") } else { String::new() },
                    if r.source.as_deref() == Some("agent") { " · agent" } else { "" },
                    disp(&r.title)
                );
                if full {
                    ln += &format!("\n    {}", cut(r.summary.as_deref().unwrap_or(""), cutn));
                }
                lines.push(ln);
                if !mem.is_empty() {
                    let ex = mem
                        .iter()
                        .take(3)
                        .map(|x| format!("{} {}", disp(&x.id), cut(x.title.as_deref().unwrap_or(""), 60)))
                        .collect::<Vec<_>>()
                        .join("; ");
                    lines.push(format!(
                        "    + {} same pattern, e.g. {}{}",
                        mem.len(),
                        ex,
                        if mem.len() > 3 { " …" } else { "" }
                    ));
                }
            }
            lines.join("\n")
        }
        Cmd::Get { ids } => {
            let mut parts = vec![];
            let mut st = db.prepare("SELECT id, project, kind, date, status, superseded_by, title, summary, body, tags, source, reason FROM memory WHERE id=?")?;
            for id in ids {
                let r: Option<Vec<Option<String>>> =
                    st.query_row([&id], |r| (0..12).map(|i| col(r, i)).collect::<R<Vec<_>>>()).optional()?;
                match r {
                    None => parts.push(format!("{id}: not found")),
                    Some(v) => {
                        let sup =
                            v[5].as_deref().filter(|s| !s.is_empty()).map(|s| format!(" → {s}")).unwrap_or_default();
                        let rs =
                            v[11].as_deref().filter(|s| !s.is_empty()).map(|s| format!(" ({s})")).unwrap_or_default();
                        let src = v[10].as_deref().filter(|s| !s.is_empty()).unwrap_or("user");
                        parts.push(format!(
                            "{} · {} · {} · {} · {}{}{} · source {}\n{}\n{}\n{}\ntags: {}",
                            disp(&v[0]),
                            disp(&v[2]),
                            disp(&v[3]),
                            disp(&v[1]),
                            disp(&v[4]),
                            sup,
                            rs,
                            src,
                            disp(&v[6]),
                            disp(&v[7]),
                            disp(&v[8]),
                            disp(&v[9])
                        ));
                    }
                }
            }
            parts.join("\n\n")
        }
        Cmd::Add { project, kind, title, summary, body, tags, supersedes, source, force, keep_both } => {
            m.do_add(
                &project,
                &kind,
                &title,
                &summary,
                Some(&body),
                Some(&tags),
                &supersedes,
                Some(&source),
                force,
                keep_both,
            )?
            .0
        }
        Cmd::AddBatch { project, check, file } => {
            let raw = if file == "-" {
                let mut b = vec![];
                std::io::stdin().read_to_end(&mut b)?;
                b
            } else {
                std::fs::read(&file)?
            };
            let mut lines: Vec<String> = vec![];
            for (i, line) in json_lines(&raw).into_iter().enumerate() {
                let n = i + 1;
                let msg = match line {
                    Err(e) => format!("NOT SAVED: bad line ({e})"),
                    Ok(line) if py_strip(&line).is_empty() => continue,
                    Ok(line) => match batch_line(m, &project, check, &line) {
                        Ok(Ok(msg)) => msg,
                        Ok(Err(bad)) => format!("NOT SAVED: bad line ({bad})"),
                        Err(e) => {
                            *partial = lines.join("\n");
                            return Err(format!("line {n}: {e} (lines before it were processed as shown)").into());
                        }
                    },
                };
                lines.push(format!("{n}: {msg}"));
            }
            lines.join("\n")
        }
        Cmd::Edit { id, title, summary, body, tags, kind, append } => {
            let r: Option<(i64, [Option<String>; 5])> = db
                .query_row("SELECT rowid_, title, summary, body, tags, kind FROM memory WHERE id = ?", [&id], |r| {
                    Ok((r.get(0)?, [col(r, 1)?, col(r, 2)?, col(r, 3)?, col(r, 4)?, col(r, 5)?]))
                })
                .optional()?;
            match r {
                None => format!("{id}: not found"),
                Some((rid, old)) => {
                    let ne = |o: Option<String>| o.filter(|s| !s.is_empty());
                    let mut new: [Option<String>; 5] = [
                        ne(title).or(old[0].clone()),
                        ne(summary).or(old[1].clone()),
                        Some(match body {
                            Some(b) => b,
                            None => old[2].clone().unwrap_or_default(),
                        }),
                        match tags {
                            Some(t) => Some(t),
                            None => old[3].clone(),
                        },
                        ne(kind).or(old[4].clone()),
                    ];
                    let new_body_empty = new[2].as_deref().unwrap_or("").is_empty();
                    if let Some(a) = append.filter(|a| !a.is_empty()) {
                        let b = new[2].clone().unwrap_or_default();
                        new[2] = Some(if new_body_empty { a } else { format!("{b}\n{a}") });
                    }
                    db.execute(
                        "UPDATE memory SET title=?, summary=?, body=?, tags=?, kind=?, updated=? WHERE rowid_=?",
                        params![new[0], new[1], new[2], new[3], new[4], today(), rid],
                    )?;
                    let e = |x: &Option<String>| x.clone().unwrap_or_default();
                    db.execute(
                        "INSERT INTO fw3(fw3, rowid, title, summary, body, tags, kind) VALUES ('delete',?,?,?,?,?,?)",
                        params![rid, e(&old[0]), e(&old[1]), e(&old[2]), e(&old[3]), e(&old[4])],
                    )?;
                    db.execute(
                        "INSERT INTO fw3(rowid, title, summary, body, tags, kind) VALUES (?,?,?,?,?,?)",
                        params![rid, e(&new[0]), e(&new[1]), e(&new[2]), e(&new[3]), e(&new[4])],
                    )?;
                    if m.embed_on {
                        match m.try_embed(&doc_text(disp(&new[0]), disp(&new[1]), new[2].as_deref())) {
                            Some(v) => m.put_vec(rid, &v)?,
                            None => {
                                // the old vector no longer matches the text: drop it so `mem index` redoes it
                                db.execute("DELETE FROM memory_vec WHERE rowid = ?", [rid])?;
                                db.execute("DELETE FROM memory_emb WHERE rowid_ = ?", [rid])?;
                            }
                        }
                    }
                    format!("edited {id}")
                }
            }
        }
        Cmd::Retire { ids, reason } => {
            let mut done = vec![];
            for i in ids {
                let c = db.execute(
                    "UPDATE memory SET status='retired', reason=?, updated=? WHERE id=?",
                    params![reason, today(), i],
                )?;
                done.push(if c > 0 { i } else { format!("{i} (not found)") });
            }
            format!("retired {}", done.join(", "))
        }
        Cmd::Syn { action, words } => {
            if action == "add" && words.len() >= 2 {
                let ws: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
                let mut st = db.prepare("INSERT OR IGNORE INTO synonym VALUES (?,?)")?;
                for a in &ws {
                    for b in &ws {
                        if a != b {
                            st.execute(params![a, b])?;
                        }
                    }
                }
                format!("synonyms: {}", ws.join(" = "))
            } else {
                let rows: Vec<(Option<String>, Option<String>)> = db
                    .prepare("SELECT term, group_concat(alt, ' ') FROM synonym GROUP BY term ORDER BY term")?
                    .query_map([], |r| Ok((col(r, 0)?, col(r, 1)?)))?
                    .collect::<R<_>>()?;
                if rows.is_empty() {
                    "no synonyms yet. Add words that mean the same thing in this project: mem syn add <word> <word> ..."
                        .into()
                } else {
                    rows.iter().map(|(t, a)| format!("{}: {}", disp(t), disp(a))).collect::<Vec<_>>().join("\n")
                }
            }
        }
    })
}

/// list: group rows whose title shape matches (first row of a group leads); ties go to the earliest group
pub(crate) fn group_rows(rows: Vec<ListRow>) -> Vec<(ListRow, Vec<ListRow>)> {
    let mut groups: Vec<(ListRow, Vec<ListRow>, HashSet<String>)> = vec![];
    let mut index: HashMap<String, Vec<usize>> = HashMap::new();
    for r in rows {
        let sh = shape(&r.title.clone().unwrap_or_default());
        let mut cand: HashMap<usize, usize> = HashMap::new();
        for w in &sh {
            for gi in index.get(w).into_iter().flatten() {
                *cand.entry(*gi).or_default() += 1;
            }
        }
        let mut cs: Vec<(usize, usize)> = cand.into_iter().collect();
        cs.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
        let hit = cs
            .into_iter()
            .find(|(gi, c)| *c >= 2 && groups[*gi].0.kind == r.kind && same_shape(&sh, &groups[*gi].2))
            .map(|x| x.0);
        match hit {
            None => {
                for w in &sh {
                    index.entry(w.clone()).or_default().push(groups.len());
                }
                groups.push((r, vec![], sh));
            }
            Some(gi) => groups[gi].1.push(r),
        }
    }
    groups.into_iter().map(|(r, m, _)| (r, m)).collect()
}

#[derive(Clone)]
pub(crate) struct ListRow {
    pub(crate) id: Option<String>,
    pub(crate) kind: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) summary: Option<String>,
    pub(crate) status: Option<String>,
    pub(crate) source: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: &str, kind: &str, title: &str) -> ListRow {
        ListRow {
            id: Some(id.into()),
            kind: Some(kind.into()),
            title: Some(title.into()),
            summary: Some(String::new()),
            status: Some("current".into()),
            source: None,
        }
    }

    #[test]
    fn list_groups_same_pattern_titles_of_the_same_kind() {
        let g = group_rows(vec![
            row("1", "fact", "Cart worker count"),
            row("2", "fact", "Inventory worker count"),
            row("3", "fact", "Refund log format"),
            row("4", "decision", "Wishlist worker count"),
        ]);
        let shape: Vec<(String, usize)> = g.iter().map(|(r, m)| (r.id.clone().unwrap(), m.len())).collect();
        assert_eq!(shape, vec![("1".into(), 1), ("3".into(), 0), ("4".into(), 0)]);
    }
}
