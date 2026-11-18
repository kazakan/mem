//! `mem search`: FTS ranking, optional semantic merge, grouping, topic pins, module cards, output format.

use crate::prelude::*;

pub(crate) const W: &str = "8.0, 5.0, 1.0, 3.0, 2.0"; // bm25 weights: title, summary, body, tags, kind

pub(crate) type Hit = (Note, Vec<Note>);

pub(crate) type Card = (String, Vec<(i64, String, String, String)>, usize);

pub(crate) fn interleave(a: &[i64], b: &[i64]) -> Vec<i64> {
    let mut out = vec![];
    let mut seen = HashSet::new();
    for i in 0..a.len().max(b.len()) {
        for l in [a, b] {
            if let Some(&x) = l.get(i) {
                if seen.insert(x) {
                    out.push(x);
                }
            }
        }
    }
    out
}

pub(crate) fn conflicts(res: &[Hit]) -> IndexMap<String, Vec<String>> {
    let rows: Vec<&Note> = res
        .iter()
        .map(|(m, _)| m)
        .filter(|m| ["decision", "convention", "gotcha"].contains(&m.kind.as_str()))
        .collect();
    let mut flag: IndexMap<String, Vec<String>> = IndexMap::new();
    for (i, a) in rows.iter().enumerate() {
        for b in &rows[i + 1..] {
            let sa: HashSet<String> = subject(&a.title).union(&subject(cut(&a.summary, 80))).cloned().collect();
            let sb: HashSet<String> = subject(&b.title).union(&subject(cut(&b.summary, 80))).cloned().collect();
            let inter = sa.intersection(&sb).count();
            if inter >= 2 && inter as f64 / (sa.len().min(sb.len()).max(1)) as f64 >= 0.5 {
                flag.entry(a.id.clone()).or_default().push(b.id.clone());
                flag.entry(b.id.clone()).or_default().push(a.id.clone());
            }
        }
    }
    flag
}

impl Mem {
    pub(crate) fn sem_ranked(
        &self,
        q: &str,
        project: Option<&str>,
        kind: Option<&[String]>,
        limit: usize,
    ) -> R<Option<Vec<(i64, f64)>>> {
        let Some(qb) = self.try_embed(&format!("task: search result | query: {q}")) else { return Ok(None) };
        let cand: Vec<i64> = self
            .db
            .prepare_cached("SELECT rowid FROM memory_vec WHERE embedding MATCH vec_quantize_binary(?) AND k = 150")?
            .query_map(params![qb], |r| r.get(0))?
            .collect::<R<_>>()?;
        if cand.is_empty() {
            return Ok(Some(vec![]));
        }
        let project = project.filter(|p| !p.is_empty());
        let kind = kind.filter(|k| !k.is_empty());
        let mut sql = format!(
            "SELECT e.rowid_, 1 - vec_distance_cosine(vec_int8(e.e), vec_quantize_int8(?, 'unit')) FROM memory_emb e \
             JOIN memory m ON m.rowid_ = e.rowid_ WHERE e.rowid_ IN ({}) AND m.status = 'current'",
            vec!["?"; cand.len()].join(",")
        );
        let mut args: Vec<Value> = vec![Value::Blob(qb)];
        args.extend(cand.iter().map(|c| Value::Integer(*c)));
        if let Some(p) = project {
            sql += " AND m.project = ?";
            args.push(Value::Text(p.to_string()));
        }
        if let Some(k) = kind {
            sql += &format!(" AND m.kind IN ({})", vec!["?"; k.len()].join(","));
            args.extend(k.iter().map(|x| Value::Text(x.clone())));
        }
        let mut rows: Vec<(i64, f64)> = self
            .db
            .prepare(&sql)?
            .query_map(params_from_iter(args), |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<R<_>>()?;
        rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        rows.truncate(limit);
        Ok(Some(rows))
    }

    /// v2's relative-score fusion: FTS score as a share of the top one, semantic score min-max, w = 0.2 + 0.4c
    pub(crate) fn fused(&self, f: &[(i64, f64)], sem: &[(i64, f64)], qt: &[String]) -> R<Vec<i64>> {
        let mut c = 0.0;
        if !f.is_empty() && !qt.is_empty() {
            let top = f[0].0;
            let mut hit = 0;
            for t in qt {
                if self
                    .db
                    .query_row("SELECT 1 FROM fw3 WHERE fw3 MATCH ? AND rowid = ?", params![qe(t), top], |_| Ok(()))
                    .optional()?
                    .is_some()
                {
                    hit += 1;
                }
            }
            c = hit as f64 / qt.len() as f64;
        }
        let w = 0.2 + 0.4 * c;
        let mut tot: IndexMap<i64, f64> = IndexMap::new();
        if !f.is_empty() {
            let hi = f.iter().map(|x| x.1).fold(f64::MIN, f64::max);
            for (r, x) in f {
                tot.insert(*r, w * if hi > 0.0 { x / hi } else { 1.0 });
            }
        }
        if !sem.is_empty() {
            let hi = sem.iter().map(|x| x.1).fold(f64::MIN, f64::max);
            let lo = sem.iter().map(|x| x.1).fold(f64::MAX, f64::min);
            for (r, x) in sem {
                *tot.entry(*r).or_insert(0.0) += (1.0 - w) * if hi > lo { (x - lo) / (hi - lo) } else { 1.0 };
            }
        }
        let mut v: Vec<(i64, f64)> = tot.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        Ok(v.into_iter().map(|x| x.0).collect())
    }

    pub(crate) fn expand(&self, toks: Vec<String>) -> R<Vec<String>> {
        if self.no_syn {
            return Ok(toks);
        }
        let mut out = toks.clone();
        let mut st = self.db.prepare_cached("SELECT alt FROM synonym WHERE term = ?")?;
        for t in &toks {
            let alts = st.query_map([t.to_lowercase()], |r| cols(r, 0))?.collect::<R<Vec<_>>>()?;
            out.extend(alts);
        }
        Ok(dedup(out))
    }

    /// Representative of a word's synonym group: the smallest word reachable through `synonym` (at most 50
    /// visited), so "alert", "알림" and "notification" compare equal once an agent registered them (D-21).
    pub(crate) fn canon_word(&self, w: &str) -> R<String> {
        if self.no_syn {
            return Ok(w.to_string());
        }
        if let Some(c) = self.canon.borrow().get(w) {
            return Ok(c.clone());
        }
        let mut seen: HashSet<String> = HashSet::from([w.to_string()]);
        let mut queue = std::collections::VecDeque::from([w.to_string()]);
        let mut st = self.db.prepare_cached("SELECT alt FROM synonym WHERE term = ? ORDER BY alt")?;
        while seen.len() < 50 {
            let Some(t) = queue.pop_front() else { break };
            for a in st.query_map([&t], |r| cols(r, 0))?.collect::<R<Vec<String>>>()? {
                if seen.len() < 50 && seen.insert(a.clone()) {
                    queue.push_back(a);
                }
            }
        }
        // String order is byte order of UTF-8, which is code point order, as Python's min()
        let c = seen.into_iter().min().unwrap();
        self.canon.borrow_mut().insert(w.to_string(), c.clone());
        Ok(c)
    }

    /// Same subject: equal subject words (`subj2`) after mapping each word to its synonym group (D-08, D-21).
    pub(crate) fn same_subject(&self, a: &HashSet<String>, b: &HashSet<String>) -> R<bool> {
        let canon = |s: &HashSet<String>| s.iter().map(|w| self.canon_word(w)).collect::<R<HashSet<String>>>();
        Ok(same_subj(&canon(a)?, &canon(b)?))
    }

    pub(crate) fn query_tokens(&self, q: &str, kw: &str) -> R<(Vec<String>, Vec<String>)> {
        let qt = tokens(q);
        let mut all = qt.clone();
        for k in KW_SPLIT.split(kw) {
            all.extend(tokens(k));
        }
        let all = dedup(all);
        Ok((qt, self.expand(all)?))
    }

    pub(crate) fn fts_ranked(
        &self,
        toks: &[String],
        project: Option<&str>,
        kind: Option<&[String]>,
        limit: i64,
    ) -> R<Vec<(i64, f64)>> {
        if toks.is_empty() {
            return Ok(vec![]);
        }
        let project = project.filter(|p| !p.is_empty());
        let kind = kind.filter(|k| !k.is_empty());
        let mut sql = format!("SELECT fw3.rowid, -bm25(fw3, {W}) FROM fw3 JOIN memory m ON m.rowid_ = fw3.rowid WHERE fw3 MATCH ? AND m.status = 'current'");
        let mut args: Vec<Value> = vec![Value::Text(toks.iter().map(|t| qe(t)).collect::<Vec<_>>().join(" OR "))];
        if let Some(p) = project {
            sql += " AND m.project = ?";
            args.push(Value::Text(p.to_string()));
        }
        if let Some(k) = kind {
            sql += &format!(" AND m.kind IN ({})", vec!["?"; k.len()].join(","));
            args.extend(k.iter().map(|x| Value::Text(x.clone())));
        }
        sql += &format!(" ORDER BY bm25(fw3, {W}) LIMIT ?");
        args.push(Value::Integer(limit));
        let mut st = self.db.prepare_cached(&sql)?;
        let rows = st
            .query_map(params_from_iter(args), |r| Ok((r.get::<_, i64>(0)?, r.get::<_, f64>(1)?)))?
            .collect::<R<Vec<_>>>()?;
        Ok(rows)
    }

    pub(crate) fn note(&self, rid: i64) -> R<Note> {
        let mut st = self.db.prepare_cached(&format!("SELECT {NOTE_COLS} FROM memory WHERE rowid_ = ?"))?;
        st.query_row([rid], note_from)
    }

    pub(crate) fn group_similar(&self, order: &[i64], limit: i64) -> R<Vec<Hit>> {
        let win: Vec<i64> = order.iter().take(60).copied().collect();
        let mut rows: HashMap<i64, Note> = HashMap::new();
        for r in &win {
            rows.insert(*r, self.note(*r)?);
        }
        let mut picked: Vec<i64> = vec![];
        let mut groups: HashMap<i64, Vec<i64>> = HashMap::new();
        let mut shp: HashMap<i64, HashSet<String>> = HashMap::new();
        for &r in &win {
            shp.insert(r, shape(&rows[&r].title));
            let mut lead = None;
            if !self.no_group {
                for &p in &picked {
                    if rows[&p].kind == rows[&r].kind {
                        let mut cand = vec![p];
                        cand.extend(
                            groups.get(&p).map(|g| g.iter().take(4).copied().collect::<Vec<_>>()).unwrap_or_default(),
                        );
                        if cand.iter().any(|x| same_shape(&shp[&r], &shp[x])) {
                            lead = Some(p);
                            break;
                        }
                    }
                }
            }
            match lead {
                Some(p) => groups.entry(p).or_default().push(r),
                None => picked.push(r),
            }
        }
        let gl = |p: &i64| groups.get(p).map(|g| g.len()).unwrap_or(0);
        let extra = picked[..py_end(picked.len(), limit)].iter().filter(|p| gl(p) >= 5).count().min(2) as i64;
        let end = py_end(picked.len(), limit + extra);
        Ok(picked[..end]
            .iter()
            .map(|p| {
                (
                    rows[p].clone(),
                    groups.get(p).map(|g| g.iter().map(|x| rows[x].clone()).collect()).unwrap_or_default(),
                )
            })
            .collect())
    }

    pub(crate) fn topic_pins(&self, toks: &[String], project: Option<&str>) -> R<Vec<Note>> {
        let qs: HashSet<String> = toks.iter().filter(|t| clen(t) >= 2).map(|t| t.to_lowercase()).collect();
        let mut out: Vec<(usize, Note)> = vec![];
        let mut st = self.db.prepare_cached(&format!(
            "SELECT {NOTE_COLS} FROM memory WHERE kind='topic' AND status='current' AND (? IS NULL OR project=?)"
        ))?;
        let rows = st.query_map(params![project, project], note_from)?.collect::<R<Vec<_>>>()?;
        for row in rows {
            let mut words = shape(&row.title);
            for x in ["rules", "rule", "규칙"] {
                words.remove(x);
            }
            let hit = words
                .iter()
                .filter(|w| {
                    qs.iter().any(|q| {
                        q == *w || (clen(q).min(clen(w)) >= 5 && q.starts_with(cut(w, 5)) && w.starts_with(cut(q, 5)))
                    })
                })
                .count();
            if hit > 0 {
                out.push((hit, row));
            }
        }
        out.sort_by_key(|x| std::cmp::Reverse(x.0)); // stable, like Python's sorted(key=-hit)
        Ok(out.into_iter().take(2).map(|x| x.1).collect())
    }

    pub(crate) fn run_search(
        &self,
        q: &str,
        kw: &str,
        project: Option<&str>,
        limit: i64,
        kind: Option<&[String]>,
    ) -> R<Vec<Hit>> {
        let (qt, toks) = self.query_tokens(q, kw)?;
        let f = self.fts_ranked(&toks, project, kind, 60)?;
        let fids: Vec<i64> = f.iter().map(|x| x.0).collect();
        let sem = if self.use_sem()? { self.sem_ranked(&format!("{q} {kw}"), project, kind, 60)? } else { None };
        let order: Vec<i64> = if let Some(sem) = sem {
            if self.mode == "interleave" {
                interleave(&fids, &sem.iter().map(|x| x.0).collect::<Vec<_>>())
            } else {
                let mut order = self.fused(&f, &sem, &qt)?;
                if self.mode == "fusion+fts2" {
                    // FTS #1 and #2 always land inside the top `limit`
                    let top = &order[..py_end(order.len(), limit)];
                    let must: Vec<i64> = fids.iter().take(2).copied().filter(|r| !top.contains(r)).collect();
                    if !must.is_empty() {
                        let head: Vec<i64> = top.iter().copied().filter(|r| !must.contains(r)).collect();
                        let head: Vec<i64> = head[..py_end(head.len(), limit - must.len() as i64)].to_vec();
                        let rest: Vec<i64> =
                            order.iter().copied().filter(|r| !head.contains(r) && !must.contains(r)).collect();
                        order = head.into_iter().chain(must).chain(rest).collect();
                    }
                }
                order
            }
        } else {
            fids
        };
        let res = self.group_similar(&order, limit)?;
        let kind_ne = kind.filter(|k| !k.is_empty());
        let pins = match kind_ne {
            Some(k) if !k.iter().any(|x| x == "topic") => vec![],
            _ => self.topic_pins(&toks, project)?,
        };
        let shown: HashSet<String> = res.iter().map(|(m, _)| m.id.clone()).collect();
        let mut out: Vec<Hit> = pins.into_iter().filter(|p| !shown.contains(&p.id)).map(|p| (p, vec![])).collect();
        out.extend(res);
        Ok(out)
    }

    /// One scan of the current notes' tags instead of one LIKE scan per query word (same rows, same order).
    /// Rows stay in table order, which is the order the LIKE scans returned them in.
    pub(crate) fn tag_scan(&self, project: Option<&str>) -> R<Vec<(i64, String)>> {
        let mut st = self.db.prepare_cached(
            "SELECT rowid, tags FROM memory WHERE status = 'current' AND kind NOT IN ('topic', 'handoff') \
             AND (? IS NULL OR project = ?) AND tags IS NOT NULL ORDER BY rowid",
        )?;
        let mut out = vec![];
        let mut rows = st.query(params![project, project])?;
        while let Some(r) = rows.next()? {
            // SQL: ',' || lower(replace(tags, ' ', '')) || ','  (lower() is ASCII-only in SQLite)
            if let Some(t) = vref_str(r.get_ref(1)?) {
                let mut norm = t.replace(' ', "");
                norm.make_ascii_lowercase();
                out.push((r.get::<_, i64>(0)?, norm));
            }
        }
        Ok(out)
    }

    /// table rowids whose tags hold the piece `t` (like `LIKE '%,t,%'`); falls back to SQL when t has LIKE wildcards
    pub(crate) fn tagged(
        &self,
        post: &HashMap<&str, Vec<i64>>,
        t: &str,
        project: Option<&str>,
        limit: Option<usize>,
    ) -> R<Vec<i64>> {
        if !t.contains('%') && !t.contains('_') {
            let v = post.get(t).map(|v| v.as_slice()).unwrap_or(&[]);
            return Ok(v.iter().take(limit.unwrap_or(usize::MAX)).copied().collect());
        }
        let sql = format!(
            "SELECT rowid FROM memory WHERE status = 'current' AND kind NOT IN ('topic', 'handoff') \
             AND (? IS NULL OR project = ?) AND (',' || lower(replace(tags, ' ', '')) || ',') LIKE ?{}",
            if limit.is_some() { " LIMIT 80" } else { "" }
        );
        let mut st = self.db.prepare_cached(&sql)?;
        let v = st
            .query_map(params![project, project, format!("%,{t},%")], |r| r.get::<_, i64>(0))?
            .collect::<R<Vec<_>>>()?;
        Ok(v)
    }

    pub(crate) fn query_modules(
        &self,
        toks: &[String],
        project: Option<&str>,
        post: &HashMap<&str, Vec<i64>>,
    ) -> R<Vec<String>> {
        let mut score: IndexMap<String, i64> = IndexMap::new();
        let ts: IndexSet<String> = toks.iter().filter(|x| clen(x) >= 2).map(|x| x.to_lowercase()).collect();
        let mut get = self.db.prepare_cached("SELECT title, tags FROM memory WHERE rowid = ?")?;
        for t in ts {
            let ids = self.tagged(post, &t, project, Some(80))?;
            let mut c: IndexMap<String, usize> = IndexMap::new();
            for rid in &ids {
                let (ti, tg) = get.query_row([rid], |r| Ok((cols(r, 0)?, cols(r, 1)?)))?;
                if let Some(m) = module_of(&ti, &tg) {
                    *c.entry(m).or_default() += 1;
                }
            }
            if !c.is_empty() {
                let mx = *c.values().max().unwrap();
                let (m, n) = c.iter().find(|(_, v)| **v == mx).map(|(k, v)| (k.clone(), *v)).unwrap();
                if n >= 3 && n as f64 >= 0.6 * ids.len() as f64 {
                    *score.entry(m.clone()).or_default() += if t == m { 2 } else { 1 };
                }
            }
        }
        if score.is_empty() {
            return Ok(vec![]);
        }
        let top = *score.values().max().unwrap();
        Ok(score.into_iter().filter(|(_, v)| *v == top).map(|(k, _)| k).collect())
    }

    pub(crate) fn module_cards(
        &self,
        res: &[Hit],
        q: &str,
        kw: &str,
        project: Option<&str>,
    ) -> R<IndexMap<String, Card>> {
        let mut shown: HashSet<String> = HashSet::new();
        for (m, sims) in res {
            shown.insert(m.id.clone());
            shown.extend(sims.iter().map(|s| s.id.clone()));
        }
        let (_, toks) = self.query_tokens(q, kw)?;
        let scan = self.tag_scan(project)?;
        let mut post: HashMap<&str, Vec<i64>> = HashMap::new();
        for (rid, norm) in &scan {
            let mut seen: HashSet<&str> = HashSet::new();
            for piece in norm.split(',') {
                if !piece.is_empty() && seen.insert(piece) {
                    post.entry(piece).or_default().push(*rid);
                }
            }
        }
        let mods = self.query_modules(&toks, project, &post)?;
        if mods.is_empty() {
            return Ok(IndexMap::new());
        }
        let mut rank: HashMap<i64, usize> = HashMap::new();
        for (i, (r, _)) in self.fts_ranked(&toks, project, None, 400)?.into_iter().enumerate() {
            rank.insert(r, i);
        }
        let mut cards: IndexMap<String, Card> = IndexMap::new();
        let mut used: HashSet<String> = HashSet::new();
        let mut get_id = self.db.prepare_cached("SELECT rowid_, id FROM memory WHERE rowid = ?")?;
        let mut get_ts = self.db.prepare_cached("SELECT title, summary FROM memory WHERE rowid_ = ?")?;
        for m in mods {
            if cards.len() >= 2 {
                break;
            }
            if m.is_empty() || used.contains(&m) {
                continue;
            }
            used.insert(m.clone());
            let mut rows: Vec<(i64, String)> = vec![];
            for rid in self.tagged(&post, &m.replace(' ', ""), project, None)? {
                let (r_, id) = get_id.query_row([rid], |r| Ok((r.get::<_, i64>(0)?, cols(r, 1)?)))?;
                if !shown.contains(&id) {
                    rows.push((r_, id));
                }
            }
            if rows.is_empty() {
                continue;
            }
            rows.sort_by_key(|r| (rank.get(&r.0).copied().unwrap_or(1_000_000), -r.0));
            let total = rows.len();
            let mut take = vec![];
            for (r_, id) in rows.into_iter().take(self.card_lines as usize) {
                let (t, su) = get_ts.query_row([r_], |r| Ok((cols(r, 0)?, cols(r, 1)?)))?;
                take.push((r_, id, t, su));
            }
            shown.extend(take.iter().map(|r| r.1.clone()));
            cards.insert(format!("q:{m}"), (m, take, total));
        }
        Ok(cards)
    }

    pub(crate) fn fmt(&self, res: &[Hit], cards: &IndexMap<String, Card>) -> R<String> {
        let mut out: Vec<String> = vec![];
        let cf = conflicts(res);
        let tops: Vec<&Note> = res.iter().map(|(m, _)| m).collect();
        let mut same_top: IndexMap<String, Vec<String>> = IndexMap::new();
        for (i, a) in tops.iter().enumerate() {
            for b in &tops[i + 1..] {
                if SUBJ_KINDS.contains(&a.kind.as_str())
                    && SUBJ_KINDS.contains(&b.kind.as_str())
                    && self.same_subject(&subj2(&a.title), &subj2(&b.title))?
                {
                    same_top.entry(a.id.clone()).or_default().push(b.id.clone());
                    same_top.entry(b.id.clone()).or_default().push(a.id.clone());
                }
            }
        }
        let empty: Vec<String> = vec![];
        for (i, (m, sims)) in res.iter().enumerate() {
            let tag = if m.source.as_deref() == Some("agent") { " · agent" } else { "" };
            out.push(format!(
                "[{}] {} · {} · {}{}\n    {}\n    ↳ {}",
                i + 1,
                m.id,
                m.kind,
                disp(&m.date),
                tag,
                m.title,
                cut(&m.summary, 220)
            ));
            if m.kind == "topic" {
                if let Some(b) = m.body.as_deref().filter(|b| !b.is_empty()) {
                    let body = if clen(b) as i64 <= self.topic_show {
                        b.to_string()
                    } else {
                        format!("{} … (mem get {})", cut_py(b, self.topic_show), m.id)
                    };
                    for ln in py_splitlines(&body) {
                        if !py_strip(ln).is_empty() {
                            out.push(format!("      {ln}"));
                        }
                    }
                }
            }
            let st_ids = same_top.get(&m.id).unwrap_or(&empty);
            if let Some(cfl) = cf.get(&m.id) {
                let rest: Vec<&String> = cfl.iter().filter(|o| !st_ids.contains(o)).collect();
                if !rest.is_empty() {
                    out.push(format!(
                        "    ! overlaps with {} (both current, neither supersedes the other) — if they disagree, ask the user",
                        rest.iter().map(|s| s.as_str()).collect::<Vec<_>>().join(", ")
                    ));
                }
            }
            for o in st_ids {
                let newer = if num_cmp_gt(&num_of(o), &num_of(&m.id)) { o } else { &m.id };
                out.push(format!("    ⚠ {o} — SAME SUBJECT, also current (newer: {newer}). If the newer says now/only/instead, it replaces the older."));
            }
            let mut sims: Vec<&Note> = sims.iter().collect();
            if !sims.is_empty() {
                let lead_s = subj2(&m.title);
                let mut same: Vec<&Note> = vec![];
                for s in sims.iter().copied() {
                    if self.same_subject(&lead_s, &subj2(&s.title))? {
                        same.push(s);
                    }
                }
                for s in &same {
                    let newer = if num_cmp_gt(&num_of(&s.id), &num_of(&m.id)) { &s.id } else { &m.id };
                    out.push(format!(
                        "    ⚠ {} · {} — SAME SUBJECT, also current (newer: {}). If the newer says now/only/instead, it replaces the older.\n      ↳ {}",
                        s.id, cut(&s.title, 70), newer, cut(&s.summary, 160)
                    ));
                }
                let same_ids: HashSet<&str> = same.iter().map(|s| s.id.as_str()).collect();
                sims.retain(|s| !same_ids.contains(s.id.as_str()));
            }
            if !sims.is_empty() {
                out.push(format!("    + {} more of the same pattern:", sims.len()));
                let end = py_end(sims.len(), self.sim_show);
                for s in &sims[..end] {
                    out.push(format!("      {} · {}", s.id, cut(&s.title, 70)));
                }
                if sims.len() as i64 > self.sim_show {
                    out.push(format!(
                        "      … {} more (narrow the search to see them)",
                        sims.len() as i64 - self.sim_show
                    ));
                }
            }
            if let Some((md, rows, total)) = cards.get(&m.id) {
                out.push(format!(
                    "    other current notes on '{md}' ({} of {total}, most relevant first):",
                    rows.len()
                ));
                for r in rows {
                    out.push(format!("      · {} ({})", cut(&r.3, 100), r.1));
                }
            }
        }
        for (key, (md, rows, total)) in cards {
            if key.starts_with("q:") {
                out.push(format!(
                    "[{md}] other current notes on '{md}' ({} of {total}, most relevant first):",
                    rows.len()
                ));
                for r in rows {
                    out.push(format!("      · {} ({})", cut(&r.3, 100), r.1));
                }
            }
        }
        Ok(out.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::TempStore;

    fn ids(res: &[Hit]) -> Vec<String> {
        res.iter().map(|(m, _)| m.id.clone()).collect()
    }

    #[test]
    fn interleave_alternates_and_dedups() {
        assert_eq!(interleave(&[1, 2, 3], &[2, 9]), vec![1, 2, 9, 3]);
        assert_eq!(interleave(&[], &[4]), vec![4]);
    }

    #[test]
    fn title_matches_rank_first_and_synonyms_expand() {
        let s = TempStore::new();
        s.add("fact", "Order export format", "CSV with a header row", "orders");
        s.add("gotcha", "Refund retry limit", "Retries stop after 3 attempts", "refund");
        s.add("fact", "Retry dashboard", "Mentions refund once", "ops");
        let res = s.mem.run_search("refund retry", "", Some("pr"), 5, None).unwrap();
        assert_eq!(ids(&res)[0], "pr-gotc-002");
        // Korean words find the English note only once an agent registered them as synonyms
        assert!(s.mem.run_search("환불 재시도", "", Some("pr"), 5, None).unwrap().is_empty());
        s.syn(&["환불", "refund"]);
        s.syn(&["재시도", "retry"]);
        let res = s.mem.run_search("환불 재시도", "", Some("pr"), 5, None).unwrap();
        assert_eq!(ids(&res)[0], "pr-gotc-002");
        assert!(s.mem.run_search("nothing matches this", "", Some("pr"), 5, None).unwrap().is_empty());
    }

    #[test]
    fn synonym_groups_have_one_representative_even_through_chains() {
        let mut s = TempStore::new();
        s.syn(&["alert", "알림"]);
        s.syn(&["알림", "notification"]); // added later, linked only through 알림
        for w in ["alert", "알림", "notification"] {
            assert_eq!(s.mem.canon_word(w).unwrap(), "alert");
        }
        assert_eq!(s.mem.canon_word("channel").unwrap(), "channel");
        let a: HashSet<String> = ["wishlist", "notification", "channel"].map(String::from).into();
        let b: HashSet<String> = ["wishlist", "알림", "channel"].map(String::from).into();
        assert!(s.mem.same_subject(&a, &b).unwrap());
        // two words that collapse into one group are not enough for a subject
        let c: HashSet<String> = ["alert", "알림"].map(String::from).into();
        assert!(!s.mem.same_subject(&c, &c).unwrap());
        s.mem.no_syn = true;
        s.mem.canon.borrow_mut().clear();
        assert!(!s.mem.same_subject(&a, &b).unwrap());
    }

    #[test]
    fn same_pattern_notes_fold_under_one_hit() {
        let s = TempStore::new();
        for m in ["cart", "inventory", "wishlist", "coupons"] {
            s.add("fact", &format!("{m} API owner team"), &format!("{m} API is owned by team {m}"), m);
        }
        let res = s.mem.run_search("API owner team", "", Some("pr"), 5, None).unwrap();
        assert_eq!(res.len(), 1);
        assert_eq!(res[0].1.len(), 3);
        let out = s.mem.fmt(&res, &IndexMap::new()).unwrap();
        assert!(out.contains("+ 3 more of the same pattern:"), "{out}");
    }

    #[test]
    fn topic_notes_are_pinned_with_their_body() {
        let s = TempStore::new();
        for i in 0..8 {
            s.add(
                "fact",
                &format!("Payments alert channel {i}"),
                "alerting goes to #payments-alerts",
                "payments,alert",
            );
        }
        let (_, id) = s
            .mem
            .do_add(
                "pr",
                "topic",
                "Alerting rules",
                "Project-wide rules",
                Some("- Page on-call after 5 minutes (since 2026-01-01)"),
                Some(""),
                "",
                None,
                true,
                true,
            )
            .unwrap();
        let id = id.unwrap();
        // with limit 1 the folded payments group takes the slot; the pin still brings the topic note in
        let res = s.mem.run_search("alerting channel", "", Some("pr"), 1, None).unwrap();
        assert!(ids(&res).contains(&id), "{:?}", ids(&res));
        // a query that only names the topic: pinned in front of everything
        let res = s.mem.run_search("alerting", "", Some("pr"), 5, Some(&["topic".to_string()])).unwrap();
        assert_eq!(ids(&res)[0], id);
        let out = s.mem.fmt(&res, &IndexMap::new()).unwrap();
        assert!(out.contains("      - Page on-call after 5 minutes"), "{out}");
    }

    #[test]
    fn two_current_notes_on_one_subject_are_flagged() {
        let s = TempStore::new();
        s.add("convention", "Wishlist incident alert channel", "#wishlist-ops", "wishlist");
        s.add("convention", "Wishlist incident alert channel", "now #wishlist-alerts only", "wishlist");
        let res = s.mem.run_search("wishlist incident alert channel", "", Some("pr"), 5, None).unwrap();
        let out = s.mem.fmt(&res, &IndexMap::new()).unwrap();
        assert!(out.contains("SAME SUBJECT, also current (newer: pr-conv-002)"), "{out}");
    }

    #[test]
    fn module_cards_list_other_notes_of_the_named_module() {
        let s = TempStore::new();
        s.add("fact", "Wishlist owner team", "fulfillment", "wishlist");
        s.add("fact", "Wishlist deploy day", "Tuesday", "wishlist,deploy");
        s.add("fact", "Wishlist cache TTL", "60s", "wishlist,cache");
        s.add("fact", "Wishlist worker count", "8", "wishlist");
        let res = s.mem.run_search("wishlist owner", "", Some("pr"), 1, None).unwrap();
        let cards = s.mem.module_cards(&res, "wishlist owner", "", Some("pr")).unwrap();
        let out = s.mem.fmt(&res, &cards).unwrap();
        assert!(out.contains("[wishlist] other current notes on 'wishlist'"), "{out}");
    }

    #[test]
    fn retired_and_superseded_notes_are_not_searched() {
        let s = TempStore::new();
        let a = s.add("fact", "Search API timeout", "2 seconds", "search");
        s.mem.db.execute("UPDATE memory SET status='retired' WHERE id=?", [&a]).unwrap();
        assert!(s.mem.run_search("search timeout", "", Some("pr"), 5, None).unwrap().is_empty());
    }
}
