#!/usr/bin/env python3
"""mem v3 — shared project memory CLI (FTS + synonyms + optional EmbeddingGemma hybrid)."""
JOSA = ['으로부터', '에서부터', '이었나요', '되었나요', '되었는가', '되었으며', '하였나요', '하였는가', '하였으며', '하였는지', '으로서', '으로써', '에게서', '이라고', '이라는', '이라면', '에서는', '에서도', '에서의', '으로는', '으로도', '에게는', '까지는', '부터는', '였나요', '인가요', '이었던', '되었나', '되었고', '되었다', '되나요', '하였고', '하였다', '했나요', '했는가', '적으로', '인가', '이었', '였던', '되어', '되는', '했던', '했다', '하는', '하여', '해서', '하고', '하면', '하려', '한다', '이라', '에서', '에게', '한테', '으로', '께서', '까지', '부터', '마다', '조차', '처럼', '보다', '이나', '이며', '이고', '에는', '에도', '와의', '과의', '라는', '라고', '적인', '들의', '들이', '들은', '들을', '된', '될', '할', '한', '은', '는', '이', '가', '을', '를', '의', '에', '와', '과', '도', '로', '만', '요']
KO_STOP = ['각각', '것', '경우', '관련', '그', '그러한', '그리고', '내용', '누구', '누구인가', '누구인가요', '대한', '대해', '되나요', '등', '따라', '따르면', '따른', '또한', '몇', '무슨', '무엇', '무엇이고', '무엇이며', '무엇인가', '무엇인가요', '무엇인지', '및', '서술해주세요', '설명', '설명하세요', '설명하시오', '설명해', '설명해주세요', '수', '어느', '어디', '어디로', '어떠', '어떠한', '어떤', '어떻게', '언제', '언제인가', '얼마', '얼마나', '위한', '위해', '이', '이러한', '이를', '인가요', '있나요', '있는', '있는지', '있으며', '저', '중', '통해', '해당']
EN_STOP = ['a', 'about', 'after', 'also', 'an', 'and', 'any', 'are', 'as', 'at', 'be', 'been', 'before', 'being', 'by', 'can', 'could', 'did', 'do', 'does', 'done', 'down', 'during', 'for', 'from', 'had', 'has', 'have', 'he', 'her', 'here', 'his', 'how', 'i', 'if', 'in', 'into', 'is', 'it', 'its', 'just', 'many', 'may', 'me', 'might', 'more', 'most', 'much', 'must', 'my', 'no', 'not', 'of', 'on', 'only', 'or', 'other', 'our', 'out', 'over', 'own', 'same', 'shall', 'she', 'should', 'so', 'some', 'such', 'than', 'that', 'the', 'their', 'them', 'then', 'there', 'these', 'they', 'this', 'those', 'to', 'too', 'up', 'very', 'was', 'we', 'were', 'what', 'when', 'where', 'which', 'while', 'who', 'whom', 'whose', 'why', 'will', 'with', 'would', 'yes', 'you', 'your']

# ---- mem v3: 에이전트 공유 메모리 CLI ----
# v5.3: 검색 결과 밑에 같은 모듈의 다른 현재 메모를 한 줄씩 붙임(모듈 카드)
# v5.2: 같은 주제(값만 다른) 현재 노트가 있으면 add가 --supersedes / --keep-both 중 고르게 함, 검색에서 같은 주제 현재 노트 2개를 ⚠로 표시
# v2 대비: 동의어 확장(한↔영), 비슷한 제목 묶기, FTS+의미 검색 합치기(교차 배치), list / edit / retire,
#          --supersedes 여러 개, 중복 저장 막기, add-batch, source(user|agent), kind 색인
import collections, argparse, json, math, os, re, sqlite3, struct, sys, time
from datetime import date

HERE = os.path.dirname(os.path.abspath(__file__))
DB = os.environ.get("MEM_DB") or os.path.join(os.path.dirname(HERE), ".mem", "mem.db")
LOG = os.path.join(os.path.dirname(DB), "calls.log")
MODEL = os.environ.get("MEM_MODEL") or os.path.join(os.path.dirname(DB), "model.gguf")
HANGUL = re.compile(r"[가-힣]")
TOK = re.compile(r"[0-9A-Za-z가-힣][0-9A-Za-z가-힣\-\.'·_]*")
NO_SYN = bool(os.environ.get("MEM_NO_SYN"))
NO_GROUP = bool(os.environ.get("MEM_NO_GROUP"))
_llm = None

SCHEMA = """
CREATE TABLE IF NOT EXISTS memory(id TEXT PRIMARY KEY, rowid_ INTEGER UNIQUE, project TEXT, kind TEXT, date TEXT, title TEXT,
  summary TEXT, body TEXT, tags TEXT, status TEXT DEFAULT 'current', superseded_by TEXT);
CREATE TABLE IF NOT EXISTS synonym(term TEXT, alt TEXT, PRIMARY KEY(term, alt));
"""
FTS3 = "CREATE VIRTUAL TABLE IF NOT EXISTS fw3 USING fts5(title, summary, body, tags, kind, content='', prefix='2 3', tokenize='porter unicode61')"
W = "8.0, 5.0, 1.0, 3.0, 2.0"   # bm25 가중치: title, summary, body, tags, kind

# 동의어는 코드에 넣지 않는다: 에이전트가 프로젝트에서 쓰는 말을 mem syn add 로 넣는다 (docs/DECISIONS.md D-20).


def connect():
    os.makedirs(os.path.dirname(DB), exist_ok=True)
    db = sqlite3.connect(DB, timeout=30, isolation_level=None)   # 트랜잭션은 직접 연다 (쓰기는 BEGIN IMMEDIATE)
    db.execute("PRAGMA journal_mode=WAL")
    db.execute("PRAGMA busy_timeout=30000")

    def needs():
        have = {r[0] for r in db.execute("SELECT name FROM sqlite_master")}
        cols = {r[1] for r in db.execute("PRAGMA table_info(memory)")} if "memory" in have else set()
        return not ({"memory", "synonym", "fw3"} <= have and {"source", "updated", "reason"} <= cols)
    if needs():
        db.execute("BEGIN IMMEDIATE")
        try:
            for stmt in SCHEMA.strip().split(";"):
                if stmt.strip():
                    db.execute(stmt)
            cols = {r[1] for r in db.execute("PRAGMA table_info(memory)")}
            for c, ddl in (("source", "TEXT DEFAULT 'user'"), ("updated", "TEXT"), ("reason", "TEXT")):
                if c not in cols:
                    db.execute(f"ALTER TABLE memory ADD COLUMN {c} {ddl}")
            if not db.execute("SELECT 1 FROM sqlite_master WHERE name='fw3'").fetchone():
                db.execute(FTS3)
                for rid, t, s_, b, tg, k in db.execute("SELECT rowid_, title, summary, body, tags, kind FROM memory").fetchall():
                    db.execute("INSERT INTO fw3(rowid, title, summary, body, tags, kind) VALUES (?,?,?,?,?,?)", (rid, t, s_, b or "", tg or "", k))
            db.execute("COMMIT")
        except Exception:
            db.execute("ROLLBACK")
            raise
    if embed_enabled():
        import sqlite_vec
        db.enable_load_extension(True)
        sqlite_vec.load(db)
        if not db.execute("SELECT 1 FROM sqlite_master WHERE name='memory_emb'").fetchone():
            db.execute("BEGIN IMMEDIATE")
            db.execute("CREATE VIRTUAL TABLE IF NOT EXISTS memory_vec USING vec0(embedding bit[768])")
            db.execute("CREATE TABLE IF NOT EXISTS memory_emb(rowid_ INTEGER PRIMARY KEY, e BLOB)")
            db.execute("COMMIT")
    return db


def embed_enabled():
    return os.path.exists(MODEL) and not os.environ.get("MEM_NO_EMBED")


def embed(text):
    global _llm
    if _llm is None:
        from llama_cpp import Llama
        _llm = Llama(model_path=MODEL, embedding=True, n_ctx=2048, n_batch=2048, n_threads=os.cpu_count(), verbose=False)
    v = _llm.embed(text)
    if v and isinstance(v[0], list):
        v = v[0]
    n = math.sqrt(sum(x * x for x in v)) or 1.0
    return struct.pack(f"{len(v)}f", *[x / n for x in v])


def doc_text(title, summary, body):
    return f"title: {title} | text: {summary} {body}"[:4000]


def put_vec(db, rid, title, summary, body, b=None):
    b = b or embed(doc_text(title, summary, body))
    db.execute("DELETE FROM memory_vec WHERE rowid = ?", (rid,))      # vec0는 INSERT OR REPLACE를 못 한다
    db.execute("INSERT INTO memory_vec(rowid, embedding) VALUES (?, vec_quantize_binary(?))", (rid, b))
    db.execute("INSERT OR REPLACE INTO memory_emb VALUES (?, vec_quantize_int8(?, 'unit'))", (rid, b))


def strip_ko(t):
    if HANGUL.search(t):
        for s in JOSA:
            if t.endswith(s) and len(t) - len(s) >= 1:
                return t[: -len(s)]
    return t


def tokens(text):
    out = []
    for t in TOK.findall(text or ""):
        t = t.strip(".-'·_")
        if not t or t.lower() in EN_STOP or t in KO_STOP:
            continue
        s = strip_ko(t)
        if s and s not in KO_STOP and len(s) >= 2:
            out.append(s)
    return list(dict.fromkeys(out))


def expand(db, toks):
    if NO_SYN:
        return toks
    out = list(toks)
    for t in toks:
        out += [a for (a,) in db.execute("SELECT alt FROM synonym WHERE term = ?", (t.lower(),))]
    return list(dict.fromkeys(out))


def qe(t):
    """FTS 검색어. 짧은 영문 단어(3자 이하)는 앞부분 일치를 쓰지 않는다: 'log'*가 login을, 'api'*가 apis…를 끌어오는 것을 막기 위해."""
    q = '"' + t.replace('"', '""') + '"'
    return q if re.fullmatch(r"[A-Za-z]{1,3}", t) else q + "*"


def query_tokens(db, q, kw):
    qt = tokens(q)
    return qt, expand(db, list(dict.fromkeys(qt + [t for k in re.split(r"[,\n]", kw or "") for t in tokens(k)])))


def fts_ranked(db, toks, project, kind=None, limit=60):
    if not toks:
        return []
    sql = (f"SELECT fw3.rowid, -bm25(fw3, {W}) FROM fw3 JOIN memory m ON m.rowid_ = fw3.rowid "
           "WHERE fw3 MATCH ? AND m.status = 'current'" + (" AND m.project = ?" if project else "") +
           (" AND m.kind IN (%s)" % ",".join("?" * len(kind)) if kind else "") + f" ORDER BY bm25(fw3, {W}) LIMIT ?")
    return db.execute(sql, [" OR ".join(qe(t) for t in toks)] + ([project] if project else []) + (kind or []) + [limit]).fetchall()


def sem_ranked(db, q, project, kind=None, limit=60):
    qb = embed(f"task: search result | query: {q}")
    cand = [r for (r,) in db.execute("SELECT rowid FROM memory_vec WHERE embedding MATCH vec_quantize_binary(?) AND k = 150", (qb,))]
    if not cand:
        return []
    sql = (f"SELECT e.rowid_, 1 - vec_distance_cosine(vec_int8(e.e), vec_quantize_int8(?, 'unit')) FROM memory_emb e "
           f"JOIN memory m ON m.rowid_ = e.rowid_ WHERE e.rowid_ IN ({','.join('?' * len(cand))}) AND m.status = 'current'"
           + (" AND m.project = ?" if project else "") + (" AND m.kind IN (%s)" % ",".join("?" * len(kind)) if kind else ""))
    return sorted(db.execute(sql, [qb] + cand + ([project] if project else []) + (kind or [])).fetchall(), key=lambda x: -x[1])[:limit]


def interleave(*lists):
    out, seen = [], set()
    for i in range(max((len(l) for l in lists), default=0)):
        for l in lists:
            if i < len(l) and l[i] not in seen:
                seen.add(l[i]); out.append(l[i])
    return out


def title_set(t):
    return {x.lower() for x in tokens(t)}


SHAPE_SPLIT = re.compile(r"[^0-9a-z가-힣]+")


def shape(t):
    """제목 모양: 소문자, 기호(#-_ 등)로 쪼갬, 불용어 빼기, 숫자 섞인 말은 <n>."""
    out = set()
    for w in SHAPE_SPLIT.split((t or "").lower()):
        w = strip_ko(w)
        if len(w) < 2 or w in EN_STOP or w in KO_STOP:
            continue
        out.add("<n>" if re.search(r"\d", w) else w)
    return out


def same_shape(a, b):
    if not a or not b:
        return False
    c = len(a & b); j = c / len(a | b)
    return j >= 0.5 or (c >= 3 and j >= 0.4)


def group_similar(db, order, limit, window=60):
    """v4: 같은 종류(kind)이고 제목 모양이 같은 노트는 한 자리로 묶는다(대표 1개 + 나머지는 제목만).
    같은 꼴의 구체적인 메모 수십 개가 상위를 다 차지해 일반 규칙이 밀려나는 문제(비교 실험 A1·A3)를 막는다.
    묶음의 대표뿐 아니라 앞쪽 구성원과도 비교한다(예: '#x-ops'와 '#wishlist-alerts'처럼 끝이 조금 다른 것)."""
    rows = {r: db.execute("SELECT id, kind, date, project, title, summary, source, body FROM memory WHERE rowid_ = ?", (r,)).fetchone() for r in order[:window]}
    picked, groups, shp = [], {}, {}
    for r in order[:window]:
        shp[r] = shape(rows[r][4])
        lead = None
        if not NO_GROUP:
            for p in picked:
                if rows[p][1] == rows[r][1] and any(same_shape(shp[r], shp[x]) for x in [p] + groups.get(p, [])[:4]):
                    lead = p; break
        if lead is not None:
            groups.setdefault(lead, []).append(r)
        else:
            picked.append(r)
    # 크게 묶인 묶음(5개 이상)이 있으면 그만큼 자리를 하나씩 더 준다(최대 +2): 묶음이 차지한 상위권 대신 다음 노트가 보이도록
    extra = min(2, sum(1 for p in picked[:limit] if len(groups.get(p, [])) >= 5))
    return [(rows[p], [rows[g] for g in groups.get(p, [])]) for p in picked[:limit + extra]]


def group_rows(rows, key):
    """목록용: 제목 모양이 같은 행을 한 묶음으로 (대표는 처음 나온 행). 단어 → 묶음 색인으로 후보만 비교."""
    groups, index = [], {}
    for r in rows:
        sh = shape(key(r))
        cand = {}
        for w in sh:
            for gi in index.get(w, ()):
                cand[gi] = cand.get(gi, 0) + 1
        hit = next((gi for gi, c in sorted(cand.items(), key=lambda x: -x[1]) if c >= 2 and groups[gi][0][1] == r[1] and same_shape(sh, groups[gi][2])), None)
        if hit is None:
            groups.append((r, [], sh))
            for w in sh:
                index.setdefault(w, []).append(len(groups) - 1)
        else:
            groups[hit][1].append(r)
    return [(r, m) for r, m, _ in groups]


MODE = os.environ.get("MEM_MODE", "interleave")   # interleave(기본) | fusion | fusion+fts2


def fused(db, f, sem, qt):
    """v2의 상대 점수 합산: FTS 점수는 1등 대비 비율, 의미 점수는 최소-최대 정규화, 가중치 w = 0.2 + 0.4c."""
    c = 0.0
    if f and qt:
        top = f[0][0]
        c = sum(1 for t in qt if db.execute("SELECT 1 FROM fw3 WHERE fw3 MATCH ? AND rowid = ?", (qe(t), top)).fetchone()) / len(qt)
    w = 0.2 + 0.4 * c
    tot = {}
    if f:
        hi = max(x for _, x in f)
        for r, x in f:
            tot[r] = w * (x / hi if hi > 0 else 1.0)
    if sem:
        hi = max(x for _, x in sem); lo = min(x for _, x in sem)
        for r, x in sem:
            tot[r] = tot.get(r, 0.0) + (1 - w) * ((x - lo) / (hi - lo) if hi > lo else 1.0)
    return sorted(tot, key=lambda r: -tot[r])


def run_search(db, q, kw="", project=None, limit=5, kind=None):
    qt, toks = query_tokens(db, q, kw)
    f = fts_ranked(db, toks, project, kind)
    use_sem = embed_enabled() and db.execute("SELECT count(*) FROM memory_emb").fetchone()[0] > 0
    if not use_sem:
        order = [r for r, _ in f]
    else:
        sem = sem_ranked(db, q + " " + (kw or ""), project, kind)
        if MODE == "interleave":
            order = interleave([r for r, _ in f], [r for r, _ in sem])
        else:
            order = fused(db, f, sem, [t for t in qt])
            if MODE == "fusion+fts2":   # FTS 1·2등은 늘 상위 limit 안에 들게 한다
                must = [r for r, _ in f[:2] if r not in order[:limit]]
                if must:
                    head = [r for r in order[:limit] if r not in must][: limit - len(must)]
                    order = head + must + [r for r in order if r not in head and r not in must]
    res = group_similar(db, order, limit)
    pins = [] if (kind and "topic" not in kind) else topic_pins(db, toks, project)
    shown = {m[0] for m, _ in res}
    pinned = [(p, []) for p in pins if p[0] not in shown]
    return pinned + res


def topic_pins(db, toks, project, most=2):
    """주제 메모(topic)는 몇 개뿐이라, 제목 단어가 검색어(동의어 포함)와 겹치면 결과 맨 앞에 덧붙인다(자리는 따로)."""
    qs = {t.lower() for t in toks if len(t) >= 2}
    out = []
    for row in db.execute("SELECT id, kind, date, project, title, summary, source, body FROM memory "
                          "WHERE kind='topic' AND status='current' AND (? IS NULL OR project=?)", (project, project)):
        words = shape(row[4]) - {"rules", "rule", "규칙"}
        hit = sum(1 for w in words if any(q == w or (min(len(q), len(w)) >= 5 and (q.startswith(w[:5]) and w.startswith(q[:5]))) for q in qs))
        if hit:
            out.append((hit, row))
    return [r for _, r in sorted(out, key=lambda x: -x[0])[:most]]


VALUEISH = re.compile(r"[0-9=:/#%@]")
GENERIC = {"최신", "변경", "기준", "규칙", "latest", "new", "update", "updated", "changed", "rule", "rules", "정책", "결정"}


def subject(title):
    """제목에서 값처럼 보이는 단어와 흔한 말을 뺀 주제 단어."""
    return {t.lower() for t in tokens(title) if not VALUEISH.search(t) and not re.fullmatch(r"[A-Z_]{2,}", t)} - GENERIC


VALUE_CHUNK = re.compile(r"[=:/#%@]|^v?\d+([.,]\d+)?[a-z가-힣]{0,3}$|^[A-Z][A-Z0-9]*_[A-Z0-9_]+$", re.I)


def subj2(title):
    """v5.2: 제목에서 값 덩어리(=·:·/·#·%·@가 든 말, '3초'·'300ms'·'v2' 같은 수, USE_SQLITE 같은 변수)를 통째로 빼고
    남은 주제 단어. 'i18n-api'·'2단계인증' 같은 이름은 남긴다. 영어 복수 s는 뗀다(tests → test)."""
    out = set()
    for c in (title or "").split():
        c = c.strip("()[],.'\"·")
        if not c or VALUE_CHUNK.search(c):
            continue
        for w in TOK.findall(c):
            w = w.strip(".-'·_").lower()
            if HANGUL.search(w):
                s2 = strip_ko(w)
                w = s2 if len(s2) >= 2 else w
            if len(w) < 2 or w in EN_STOP or w in KO_STOP or w in GENERIC:
                continue
            if re.fullmatch(r"[a-z]{4,}s", w):
                w = w[:-1]
            out.add(w)
    return out


CONN = None


_CANON = {}


def canon_word(db, w):
    """동의어 묶음의 대표 단어: 동의어로 이어진 단어들(최대 50개) 중 가장 작은 문자열 (docs/DECISIONS.md D-21)."""
    if NO_SYN or db is None:
        return w
    if w not in _CANON:
        seen, queue = {w}, [w]
        while queue and len(seen) < 50:
            t = queue.pop(0)
            for (a,) in db.execute("SELECT alt FROM synonym WHERE term = ? ORDER BY alt", (t,)).fetchall():
                if a not in seen and len(seen) < 50:
                    seen.add(a)
                    queue.append(a)
        _CANON[w] = min(seen)
    return _CANON[w]


def same_subj(a, b, db=None):
    """같은 주제 = 값을 뺀 주제 단어가 똑같다('Tax tests need USE_SQLITE=1' ↔ 'Tax tests need REDIS_URL').
    동의어로 넣은 말은 같은 단어로 본다('Wishlist alert channel' ↔ '위시리스트 알림 채널', D-21).
    'Search module owner' ↔ 'Catalog module owner'처럼 모듈 이름 하나만 다른 형제 노트는 다른 주제다."""
    ca = {canon_word(db, w) for w in a}
    cb = {canon_word(db, w) for w in b}
    return len(ca) >= 2 and ca == cb


def num_of(i):
    m = re.search(r"(\d+)$", i or "")
    return int(m.group(1)) if m else 0


SUBJ_KINDS = ("decision", "convention", "gotcha", "fact")


def same_subject_notes(db, project, kind, title, exclude=()):
    """v5.2: 새 노트와 주제가 같은 현재 노트(값만 다른 것). 저장할 때 대체할지 둘 다 둘지 정하게 한다."""
    if kind not in SUBJ_KINDS:
        return []
    sn = subj2(title)
    if len(sn) < 2:
        return []
    out = []
    for r, _ in fts_ranked(db, expand(db, list(sn)), project, limit=30):
        row = db.execute("SELECT id, kind, title, summary, status FROM memory WHERE rowid_ = ?", (r,)).fetchone()
        if row[0] in exclude or row[4] != "current" or row[1] not in SUBJ_KINDS:
            continue
        if same_subj(sn, subj2(row[2]), db):
            out.append(row)
    return out


def conflicts(res):
    """같이 나온 현재 결정·규칙 중 주제가 겹치는데 서로 대체 관계가 아닌 쌍 → 사용자 확인이 필요할 수 있다."""
    rows = [m for m, _ in res if m[1] in ("decision", "convention", "gotcha")]
    flag = {}
    for i, a in enumerate(rows):
        for b in rows[i + 1:]:
            sa, sb = subject(a[4]) | subject(a[5][:80]), subject(b[4]) | subject(b[5][:80])
            inter = sa & sb
            if len(inter) >= 2 and len(inter) / max(1, min(len(sa), len(sb))) >= 0.5:
                flag.setdefault(a[0], []).append(b[0]); flag.setdefault(b[0], []).append(a[0])
    return flag


TOPIC_SHOW = int(os.environ.get("MEM_TOPIC_SHOW", "1500"))


CARD_LINES = int(os.environ.get("MEM_CARD", "6"))   # 모듈 하나에 붙이는 줄 수
CARD_MODS = 2                                        # 카드를 붙이는 모듈 수


def _tags(t):
    return [x.strip().lower() for x in (t or "").split(",") if x.strip()]


def module_of(title, tags):
    """노트의 모듈 = 제목에도 나오는 첫 태그(Wishlist … → wishlist). 없으면 None."""
    words = {w.lower() for w in re.findall(r"[\w가-힣.-]+", title or "")}
    for t in _tags(tags):
        if t in words:
            return t
    return None


def query_modules(db, toks, project):
    """질문에 이름이 나온 모듈(wishlist, 찜 …). 그 태그를 단 노트 대부분이 같은 모듈일 때만 인정하고,
    질문 단어가 가장 많이 가리킨 모듈만 고른다('찜 장애 알림'이면 찜, 흔한 말 '알림'이 가리키는 알림센터는 뺌)."""
    score = collections.Counter()
    for t in dict.fromkeys(x.lower() for x in toks if len(x) >= 2):
        rows = db.execute("SELECT title, tags FROM memory WHERE status = 'current' AND kind NOT IN ('topic', 'handoff') "
                          "AND (? IS NULL OR project = ?) AND (',' || lower(replace(tags, ' ', '')) || ',') LIKE ? LIMIT 80",
                          (project, project, f"%,{t},%")).fetchall()
        c = collections.Counter(module_of(ti, tg) for ti, tg in rows)
        c.pop(None, None)
        if c:
            m, n = c.most_common(1)[0]
            if n >= 3 and n >= 0.6 * len(rows):
                score[m] += 2 if t == m else 1     # 모듈 이름 그대로(wishlist)면 별칭(찜)보다 무겁게
    if not score:
        return []
    top = max(score.values())
    return [m for m, v in score.most_common() if v == top]


def module_cards(db, res, q, kw, project):
    """v5.3: 결과 노트와 같은 모듈의 다른 현재 메모를 질문과 겹치는 것부터 몇 줄 고른다.
    파일을 통째로 읽는 대신, 검색에서 놓친 같은 모듈 사실을 싸게 같이 보여 주려는 것."""
    shown = set()
    for m, sims in res:
        shown.add(m[0]); shown.update(s[0] for s in sims)
    _, toks = query_tokens(db, q, kw)
    rank = {r: i for i, (r, _) in enumerate(fts_ranked(db, toks, project, limit=400))}
    cards, used = {}, set()
    # 질문에 이름이 나온 모듈 먼저, 그다음 결과 노트의 모듈
    cand = [(None, mq) for mq in query_modules(db, toks, project)]   # 질문에 모듈 이름이 있을 때만 붙인다
    for anchor, mod in cand:
        if len(cards) >= CARD_MODS:
            break
        if not mod or mod in used:
            continue
        used.add(mod)
        rows = db.execute("SELECT rowid_, id, title, summary FROM memory WHERE status = 'current' AND kind NOT IN ('topic', 'handoff') "
                          "AND (? IS NULL OR project = ?) AND (',' || lower(replace(tags, ' ', '')) || ',') LIKE ?",
                          (project, project, f"%,{mod.replace(' ', '')},%")).fetchall()
        rows = [r for r in rows if r[1] not in shown]
        if not rows:
            continue
        rows.sort(key=lambda r: (rank.get(r[0], 10 ** 6), -r[0]))
        cards[anchor or f"q:{mod}"] = (mod, rows[:CARD_LINES], len(rows))
        shown.update(r[1] for r in rows[:CARD_LINES])
    return cards


def fmt(res, sim_show=int(os.environ.get("MEM_SIMSHOW", "8")), cards=None):
    cards = cards or {}
    out = []
    cf = conflicts(res)
    tops = [m for m, _ in res]
    same_top = {}
    for i_, a in enumerate(tops):
        for b in tops[i_ + 1:]:
            if a[1] in SUBJ_KINDS and b[1] in SUBJ_KINDS and same_subj(subj2(a[4]), subj2(b[4]), CONN):
                same_top.setdefault(a[0], []).append(b[0]); same_top.setdefault(b[0], []).append(a[0])
    for i, (m, sims) in enumerate(res, 1):
        id_, kind, d, proj, title, summary, src = m[:7]
        tag = " · agent" if src == "agent" else ""
        out.append(f"[{i}] {id_} · {kind} · {d}{tag}\n    {title}\n    ↳ {summary[:220]}")
        if kind == "topic" and len(m) > 7 and m[7]:
            body = m[7] if len(m[7]) <= TOPIC_SHOW else m[7][:TOPIC_SHOW] + f" … (mem get {id_})"
            out += ["      " + ln for ln in body.splitlines() if ln.strip()]
        if id_ in cf:
            top_same = [o for o in cf[id_] if o in same_top.get(id_, [])]
            rest = [o for o in cf[id_] if o not in top_same]
            if rest:
                out.append(f"    ! overlaps with {', '.join(rest)} (both current, neither supersedes the other) — if they disagree, ask the user")
        for o in same_top.get(id_, []):
            newer = o if num_of(o) > num_of(id_) else id_
            out.append(f"    ⚠ {o} — SAME SUBJECT, also current (newer: {newer}). If the newer says now/only/instead, it replaces the older.")
        if sims:
            lead_s = subj2(title)
            same = [s for s in sims if same_subj(lead_s, subj2(s[4]), CONN)]
            for s in same:
                newer = s[0] if num_of(s[0]) > num_of(id_) else id_
                out.append(f"    ⚠ {s[0]} · {s[4][:70]} — SAME SUBJECT, also current (newer: {newer}). If the newer says now/only/instead, it replaces the older.\n      ↳ {s[5][:160]}")
            sims = [s for s in sims if s not in same]
        if sims:
            out.append(f"    + {len(sims)} more of the same pattern:")
            out += [f"      {s[0]} · {s[4][:70]}" for s in sims[:sim_show]]
            if len(sims) > sim_show:
                out.append(f"      … {len(sims) - sim_show} more (narrow the search to see them)")
        if id_ in cards:
            mod, rows, total = cards[id_]
            out.append(f"    other current notes on '{mod}' ({len(rows)} of {total}, most relevant first):")
            out += [f"      · {r[3][:100]} ({r[1]})" for r in rows]
    for key, (mod, rows, total) in cards.items():
        if key.startswith("q:"):
            out.append(f"[{mod}] other current notes on '{mod}' ({len(rows)} of {total}, most relevant first):")
            out += [f"      · {r[3][:100]} ({r[1]})" for r in rows]
    return "\n".join(out)


def next_id(db, project, kind):
    n = db.execute("SELECT coalesce(max(rowid_), 0) + 1 FROM memory").fetchone()[0]
    return f"{project[:2]}-{kind[:4]}-{n:03d}"


def insert(db, n):
    rid = db.execute("SELECT coalesce(max(rowid_), 0) + 1 FROM memory").fetchone()[0]
    db.execute("INSERT INTO memory(id, rowid_, project, kind, date, title, summary, body, tags, status, superseded_by, source, updated) "
               "VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)",
               (n["id"], rid, n["project"], n["kind"], n["date"], n["title"], n["summary"], n.get("body", ""), n.get("tags", ""),
                n.get("status", "current"), n.get("superseded_by"), n.get("source", "user"), n["date"]))
    db.execute("INSERT INTO fw3(rowid, title, summary, body, tags, kind) VALUES (?,?,?,?,?,?)",
               (rid, n["title"], n["summary"], n.get("body", ""), n.get("tags", ""), n["kind"]))
    return rid


def reindex(db, rid, old, new):
    db.execute("INSERT INTO fw3(fw3, rowid, title, summary, body, tags, kind) VALUES ('delete',?,?,?,?,?,?)", (rid,) + tuple(x or "" for x in old))
    db.execute("INSERT INTO fw3(rowid, title, summary, body, tags, kind) VALUES (?,?,?,?,?,?)", (rid,) + tuple(x or "" for x in new))


def norm_words(s):
    return {x.lower() for x in tokens(s)}


def find_dupe(db, project, title, summary, exclude=()):
    """제목이 같거나 내용 단어가 85% 이상 겹치는 현재 노트 (대체하려는 노트는 뺀다)."""
    nw = norm_words(title + " " + summary)
    for r, _ in fts_ranked(db, tokens(title + " " + summary), project, limit=5):
        oid, ot, os_ = db.execute("SELECT id, title, summary FROM memory WHERE rowid_ = ?", (r,)).fetchone()
        if oid in exclude:
            continue
        ow = norm_words(ot + " " + os_)
        if ot.strip().lower() == title.strip().lower() or (nw and ow and len(nw & ow) / len(nw | ow) >= 0.85):
            return oid
    return None


def do_add(db, project, kind, title, summary, body="", tags="", supersedes="", source="user", force=False, keep_both=False):
    sup = [s.strip() for s in (supersedes or "").split(",") if s.strip()]
    for s in sup:
        r = db.execute("SELECT status, superseded_by FROM memory WHERE id = ?", (s,)).fetchone()
        if not r:
            return f"NOT SAVED: --supersedes {s} not found", None
        if r[0] != "current" and not force:
            return f"NOT SAVED: {s} is already {r[0]}" + (f" (by {r[1]})" if r[1] else "") + "; nothing to replace.", None
    if not force:
        d = find_dupe(db, project, title, summary, exclude=sup)
        if d:
            return f"NOT SAVED: looks like a duplicate of {d}. Use `mem edit {d}` to change it, --supersedes {d} to replace it, or --force.", None
    if not force and not keep_both:
        left = same_subject_notes(db, project, kind, title, exclude=sup)
        if left:
            lst = "\n  ".join(f"{r[0]} · {r[2][:70]} — {r[3][:100]}" for r in left)
            what = "also current on the same subject (not in --supersedes)" if sup else "a current note on the same subject already exists"
            return (f"NOT SAVED: {what}:\n  {lst}\n"
                    f"  Decide: if the new statement replaces it (now / only / instead / no longer / moved to; 이제·~만·대신·더 이상·옮겼어), "
                    f"add its id to --supersedes; if both stay true (also / in addition; ~도·에 더해), re-run with --keep-both."), None
    similar = [] if sup else run_search(db, title + " " + summary, "", project, 3)
    vec = embed(doc_text(title, summary, body)) if embed_enabled() else None   # 모델 계산은 잠그기 전에
    db.execute("BEGIN IMMEDIATE")
    try:
        if not force:
            d = find_dupe(db, project, title, summary, exclude=sup)      # 그사이 다른 에이전트가 같은 걸 넣었는지
            if d:
                db.execute("ROLLBACK")
                return f"NOT SAVED: looks like a duplicate of {d} (just added by another session).", None
        for s_ in sup:
            if db.execute("SELECT status FROM memory WHERE id = ?", (s_,)).fetchone()[0] != "current" and not force:
                db.execute("ROLLBACK")
                return f"NOT SAVED: {s_} was superseded by another session just now.", None
        new_id = next_id(db, project, kind)
        rid = insert(db, dict(id=new_id, project=project, kind=kind, date=date.today().isoformat(), title=title, summary=summary,
                              body=body, tags=tags, source=source))
        for s_ in sup:
            db.execute("UPDATE memory SET status='superseded', superseded_by=?, updated=? WHERE id=?", (new_id, date.today().isoformat(), s_))
        if vec:
            put_vec(db, rid, title, summary, body, vec)
        db.execute("COMMIT")
    except Exception:
        db.execute("ROLLBACK")
        raise
    msg = f"saved {new_id}" + (f" (superseded {', '.join(sup)})" if sup else "")
    if similar:
        msg += "\n  similar current notes (if yours replaces one: mem edit / --supersedes):\n  " + \
               "\n  ".join(f"{m[0]} · {m[4][:60]}" for m, _ in similar)
    return msg, new_id


def main(argv):
    p = argparse.ArgumentParser(prog="mem", description="shared project memory (decisions, conventions, gotchas, facts, handoffs)")
    sub = p.add_subparsers(dest="cmd", required=True)
    b = sub.add_parser("brief", help="session start: counts, latest handoffs, recently changed notes")
    b.add_argument("--project", required=True)
    s = sub.add_parser("search", help="search current notes")
    s.add_argument("--q", required=True, help="the request or question, as-is")
    s.add_argument("--kw", default="", help="extra keywords, comma separated (synonyms, English/Korean spellings)")
    s.add_argument("--project")
    s.add_argument("--kind", help="only these kinds, comma separated")
    s.add_argument("--limit", type=int, default=int(os.environ.get("MEM_LIMIT", "5")))
    l = sub.add_parser("list", help="list notes (for overviews like 'all current conventions')")
    l.add_argument("--project")
    l.add_argument("--kind", help="comma separated")
    l.add_argument("--status", default="current", help="current|superseded|retired|all")
    l.add_argument("--tag")
    l.add_argument("--source", help="user|agent")
    l.add_argument("--full", action="store_true", help="also print summaries")
    l.add_argument("--limit", type=int, default=200)
    g = sub.add_parser("get", help="print full notes")
    g.add_argument("ids", nargs="+")
    a = sub.add_parser("add", help="save a new note")
    for k in ("project", "kind", "title", "summary"):
        a.add_argument(f"--{k}", required=True)
    a.add_argument("--body", default="")
    a.add_argument("--tags", default="")
    a.add_argument("--supersedes", default="", help="id(s) this note replaces, comma separated")
    a.add_argument("--source", default="user", choices=["user", "agent"], help="user = the user said it; agent = your own choice")
    a.add_argument("--force", action="store_true", help="save even if it looks like a duplicate")
    a.add_argument("--keep-both", action="store_true", help="a current note on the same subject stays true too (the user added, not replaced)")
    ab = sub.add_parser("add-batch", help="save many notes from a JSON-lines file ('-' = stdin)")
    ab.add_argument("--project", required=True)
    ab.add_argument("--check", action="store_true", help="dry run: show duplicates and similar current notes for each line, save nothing")
    ab.add_argument("file")
    e = sub.add_parser("edit", help="change a note in place (small fixes; use add --supersedes when a decision changes)")
    e.add_argument("id")
    for k in ("title", "summary", "body", "tags", "kind"):
        e.add_argument(f"--{k}")
    e.add_argument("--append", help="append text to the body")
    r = sub.add_parser("retire", help="hide notes that are wrong, junk or duplicates (kept for history)")
    r.add_argument("ids", nargs="+")
    r.add_argument("--reason", required=True)
    sy = sub.add_parser("syn", help="manage search synonyms: mem syn add <word> <word> ...")
    sy.add_argument("action", choices=["add", "show"])
    sy.add_argument("words", nargs="*")
    sub.add_parser("import").add_argument("path")
    sub.add_parser("index", help="embed notes that have no embedding yet")
    args = p.parse_args(argv)
    db = connect()
    global CONN
    CONN = db
    out = ""
    locked = args.cmd in ("import", "index", "edit", "retire") or (args.cmd == "syn" and args.action == "add")
    if locked:
        db.execute("BEGIN IMMEDIATE")
    if args.cmd == "import":
        with open(args.path, encoding="utf-8") as f:
            for line in f:
                if line.strip():
                    insert(db, json.loads(line))
        out = "imported"
    elif args.cmd == "index":
        todo = db.execute("SELECT rowid_, title, summary, body FROM memory WHERE rowid_ NOT IN (SELECT rowid_ FROM memory_emb)").fetchall()
        for rid, t, su, bo in todo:
            put_vec(db, rid, t, su, bo)
        out = f"indexed {len(todo)}"
    elif args.cmd == "brief":
        P = args.project
        kinds = db.execute("SELECT kind, count(*) FROM memory WHERE project=? AND status='current' GROUP BY kind ORDER BY 2 DESC", (P,)).fetchall()
        hand = db.execute("SELECT id, date, title, summary, body FROM memory WHERE project=? AND kind='handoff' AND status='current' "
                          "ORDER BY rowid_ DESC LIMIT 2", (P,)).fetchall()
        recent = db.execute("SELECT id, kind, title FROM memory WHERE project=? AND kind != 'handoff' AND (status='current' OR updated=?) "
                            "ORDER BY coalesce(updated, date) DESC, rowid_ DESC LIMIT 8", (P, date.today().isoformat())).fetchall()
        lines = [f"# {P}: {sum(n for _, n in kinds)} current notes (" + ", ".join(f"{k} {n}" for k, n in kinds) + ")",
                 "Before deciding: mem search --project %s --q ... --kw ... · rules overview: mem list --project %s --kind topic,convention,decision,gotcha" % (P, P)]
        topics = db.execute("SELECT id, title, summary FROM memory WHERE project=? AND kind='topic' AND status='current' ORDER BY title", (P,)).fetchall()
        if topics:
            lines.append("## Topic notes (project-wide rules, one note per subject; read the ones for your area: mem get <id>)")
            lines += [f"- {i} · {t} — {su[:90]}" for i, t, su in topics[:40]]
        if hand:
            lines.append("## Latest handoffs")
            lines += [f"- {i} · {d} · {t}\n  {su}\n  {(bo or '')[:500]}" for i, d, t, su, bo in hand]
        lines.append("## Recently changed")
        lines += [f"- {i} · {k} · {t}" for i, k, t in recent]
        out = "\n".join(lines)[:4500]
    elif args.cmd == "search":
        kind = [k.strip() for k in args.kind.split(",")] if args.kind else None
        res = run_search(db, args.q, args.kw, args.project, args.limit, kind)
        cards = module_cards(db, res, args.q, args.kw, args.project) if res and CARD_LINES > 0 else {}
        out = fmt(res, cards=cards) if res else "No results. Try other keywords (synonyms, English and Korean spellings), or mem list."
    elif args.cmd == "list":
        sql = "SELECT id, kind, date, title, summary, status, source FROM memory WHERE 1=1"
        a2 = []
        if args.project:
            sql += " AND project = ?"; a2.append(args.project)
        if args.status != "all":
            sql += " AND status = ?"; a2.append(args.status)
        if args.kind:
            ks = [k.strip() for k in args.kind.split(",")]
            sql += " AND kind IN (%s)" % ",".join("?" * len(ks)); a2 += ks
        if args.tag:
            sql += " AND (',' || replace(tags, ' ', ',') || ',') LIKE ?"; a2.append(f"%,{args.tag},%")
        if args.source:
            sql += " AND coalesce(source, 'user') = ?"; a2.append(args.source)
        total = db.execute(sql.replace("SELECT id, kind, date, title, summary, status, source", "SELECT count(*)"), a2).fetchone()[0]
        rows = db.execute(sql + " ORDER BY kind, title", a2).fetchall()
        groups = group_rows(rows, key=lambda r: r[3]) if not NO_GROUP else [(r, []) for r in rows]
        shown = groups[:args.limit]
        n_shown = sum(1 + len(g) for _, g in shown)
        lines = [f"{total} notes" + (f", same-pattern notes on one line ({len(groups)} lines)" if len(groups) < len(rows) else "") +
                 (f" (showing {n_shown})" if n_shown < total else "") +
                 ("  · tip: add/add-batch already check duplicates and show similar notes; use search for a topic" if total > 150 else "")]
        for (i, k, d, t, su, st, src), mem in shown:
            cut = 400 if len(shown) <= 60 else 110      # 목록이 길면 요약을 줄여 출력이 커지지 않게
            lines.append(f"{i} · {k}{' · ' + st if st != 'current' else ''}{' · agent' if src == 'agent' else ''} · {t}" + (f"\n    {su[:cut]}" if args.full else ""))
            if mem:
                lines.append(f"    + {len(mem)} same pattern, e.g. " + "; ".join(f"{m[0]} {m[3][:60]}" for m in mem[:3]) +
                             (" …" if len(mem) > 3 else ""))
        out = "\n".join(lines)
    elif args.cmd == "get":
        parts = []
        for id_ in args.ids:
            r = db.execute("SELECT id, project, kind, date, status, superseded_by, title, summary, body, tags, source, reason FROM memory WHERE id=?", (id_,)).fetchone()
            if not r:
                parts.append(f"{id_}: not found"); continue
            i, pj, k, d, st, sup, t, su, bo, tg, src, rs = r
            parts.append(f"{i} · {k} · {d} · {pj} · {st}{' → ' + sup if sup else ''}{' (' + rs + ')' if rs else ''} · source {src or 'user'}\n{t}\n{su}\n{bo}\ntags: {tg}")
        out = "\n\n".join(parts)
    elif args.cmd == "add":
        out, _ = do_add(db, args.project, args.kind, args.title, args.summary, args.body, args.tags, args.supersedes, args.source, args.force, args.keep_both)
    elif args.cmd == "add-batch":
        f = sys.stdin if args.file == "-" else open(args.file, encoding="utf-8")
        lines = []
        for n, line in enumerate(f, 1):
            if not line.strip():
                continue
            try:
                o = json.loads(line)
                if args.check:
                    sup = [x.strip() for x in (o.get("supersedes") or "").split(",") if x.strip()]
                    d = find_dupe(db, args.project, o["title"], o["summary"], exclude=sup)
                    sim = [m for m, _ in run_search(db, o["title"] + " " + o["summary"], "", args.project, 3)]
                    # 같은 주제, 다른 값: 값처럼 보이는 단어(숫자·기호·대문자 변수)를 빼고 한 번 더 찾는다 → 바뀐 결정의 옛 노트
                    subj = " ".join(t for t in tokens(o["title"]) if not re.search(r"[0-9=:/#%@]", t) and not re.fullmatch(r"[A-Z_]{2,}", t))
                    same = [m for m, _ in run_search(db, subj, "", args.project, 3)] if subj else []
                    msg = (f"DUPLICATE of {d}" if d else "new") + (f" (replaces {', '.join(sup)})" if sup else "") + " · " + o["title"][:50]
                    seen = set(sup)
                    if not o.get("keep_both"):
                        for r in same_subject_notes(db, args.project, o.get("kind", ""), o["title"], exclude=sup):
                            seen.add(r[0])
                            msg += f"\n     ! SAME SUBJECT, decide supersedes or keep_both: {r[0]} · {r[2][:60]} — {r[3][:80]}"
                    for tag, ms in (("same topic", same), ("similar", sim)):
                        for m in ms:
                            if m[0] not in seen:
                                seen.add(m[0]); msg += f"\n     ~ {tag}: {m[0]} · {m[4][:70]}"
                else:
                    msg, _ = do_add(db, args.project, o["kind"], o["title"], o["summary"], o.get("body", ""), o.get("tags", ""),
                                    o.get("supersedes", ""), o.get("source", "user"), bool(o.get("force")), bool(o.get("keep_both")))
            except (KeyError, json.JSONDecodeError) as ex:
                msg = f"NOT SAVED: bad line ({ex})"
            lines.append(f"{n}: {msg}")
        out = "\n".join(lines)
    elif args.cmd == "edit":
        r = db.execute("SELECT rowid_, title, summary, body, tags, kind FROM memory WHERE id = ?", (args.id,)).fetchone()
        if not r:
            out = f"{args.id}: not found"
        else:
            rid, old = r[0], list(r[1:])
            new = [args.title or old[0], args.summary or old[1], (args.body if args.body is not None else old[2]) or "", args.tags if args.tags is not None else old[3], args.kind or old[4]]
            if args.append:
                new[2] = (new[2] + "\n" if new[2] else "") + args.append
            db.execute("UPDATE memory SET title=?, summary=?, body=?, tags=?, kind=?, updated=? WHERE rowid_=?", new + [date.today().isoformat(), rid])
            reindex(db, rid, old, new)
            if embed_enabled():
                put_vec(db, rid, new[0], new[1], new[2])
            out = f"edited {args.id}"
    elif args.cmd == "retire":
        done = []
        for i in args.ids:
            c = db.execute("UPDATE memory SET status='retired', reason=?, updated=? WHERE id=?", (args.reason, date.today().isoformat(), i)).rowcount
            done.append(i if c else f"{i} (not found)")
        out = "retired " + ", ".join(done)
    elif args.cmd == "syn":
        if args.action == "add" and len(args.words) >= 2:
            ws = [w.lower() for w in args.words]
            db.executemany("INSERT OR IGNORE INTO synonym VALUES (?,?)", [(a, b) for a in ws for b in ws if a != b])
            out = "synonyms: " + " = ".join(ws)
        else:
            rows = db.execute("SELECT term, group_concat(alt, ' ') FROM synonym GROUP BY term ORDER BY term").fetchall()
            out = "\n".join(f"{t}: {a}" for t, a in rows) or "no synonyms yet. Add words that mean the same thing in this project: mem syn add <word> <word> ..."
    if locked:
        db.execute("COMMIT")
    print(out)
    try:
        with open(LOG, "a", encoding="utf-8") as f:
            f.write(json.dumps({"t": time.time(), "argv": argv, "out_chars": len(out)}, ensure_ascii=False) + "\n")
    except OSError:
        pass


if __name__ == "__main__":
    main(sys.argv[1:])
