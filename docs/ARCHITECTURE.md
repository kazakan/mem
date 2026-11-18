# Architecture

For agents changing this code. Read this, then docs/DECISIONS.md (why things are the way they are), then the
module you touch. The user-facing command reference is README.md (Korean); the agent instructions are SKILL.md.

## 1. What mem is, and the constraints that shape it

`mem` is a command-line tool that AI coding agents call to share project knowledge across sessions:
decisions, conventions, gotchas, facts, handoffs and project-wide rule notes ("topics").

Hard constraints (from the owner; do not trade them away without asking):

| Constraint | Consequence in the code |
|---|---|
| Local only: no external API, no MCP server, no network | SQLite file + CLI; agents learn the commands from SKILL.md |
| Light: a CLI call per action, fast to start | one process per command, no daemon, keyword search by default, embeddings optional |
| Must work with 500+ notes (thousands) | ranked FTS5 search, grouping of same-pattern notes, small bounded outputs |
| Korean and English first; other languages later, switchable | Korean particle (josa) stripping, KO/EN stopwords, agent-added synonyms; language rules live in `text.rs` |
| Several agents at once | SQLite WAL, `BEGIN IMMEDIATE` for writes, duplicate checks repeated inside the write lock |
| Output goes into an agent's context window | every output is capped and folded (see §6); tokens are a cost |

## 2. Big picture

```
agent ──(shell)──> mem <command> ──> resolve store (util::db_location)
                                 ──> open SQLite (db::connect: WAL, schema, migrations)
                                 ──> run one command (commands::run_cmd)
                                        search ─> search.rs (tokens → FTS5 [+ vectors] → group → pins → cards → text)
                                        add    ─> write.rs  (checks → [embed] → BEGIN IMMEDIATE → insert/supersede)
                                 ──> print result, append one JSON line to calls.log
```

There is no state outside the SQLite file, `calls.log` and the optional model file, all in one `.mem/` folder.

## 3. Store location (`util::db_location`, D-16, D-17)

First match wins:

1. `MEM_DB`.
2. Project-local binary: the executable is in a folder named `.bin`, or `<exe dir>/../.mem` exists →
   `<exe dir>/../.mem/mem.db`. This is the reference layout (`<project>/.bin/mem`).
3. Installed binary (winget, PATH): the nearest `.mem/mem.db` walking up from the current folder.
4. Otherwise a new store at the nearest ancestor holding `.git`, else in the current folder.

`mem where` prints the result and the rule that chose it, and creates nothing. Every other command creates the
store on first use. `calls.log` and `model.gguf` sit next to `mem.db` (`MEM_MODEL` overrides the model path).

## 4. Data model (`db.rs`)

```sql
memory(id TEXT PRIMARY KEY, rowid_ INTEGER UNIQUE, project, kind, date, title, summary, body, tags,
       status DEFAULT 'current', superseded_by, source, reason, updated)   -- last three added by migration
synonym(term, alt, PRIMARY KEY(term, alt))                                  -- both directions stored
fw3 USING fts5(title, summary, body, tags, kind, content='', prefix='2 3', tokenize='porter unicode61')
-- embed builds only:
memory_vec USING vec0(embedding bit[768])      -- binary-quantized, for the first pass
memory_emb(rowid_ INTEGER PRIMARY KEY, e BLOB)  -- int8 vector, for the rerank
```

- `id` = `<project[:2]>-<kind[:4]>-<rowid_ zero-padded to 3>`, e.g. `sh-deci-003`. `rowid_` = max + 1.
- `fw3` is **contentless** (`content=''`): it stores only the index, rowid = `memory.rowid_`. Rows are inserted
  with the note and never updated in place; `edit` deletes and re-inserts the FTS row with the old values
  (contentless tables need the old values to delete). Keep `insert` and `edit` in sync with this rule.
- Status: `current` → `superseded` (with `superseded_by`) or `retired` (with `reason`). Nothing is deleted.
- `kind` is free text; the six used kinds are decision, convention, gotcha, fact, handoff, topic.
  `SUBJ_KINDS` (decision, convention, gotcha, fact) take part in the same-subject rule.
- `connect` sets `busy_timeout` 30 s and WAL, creates tables, adds missing columns (migration from older stores),
  and in embed builds registers sqlite-vec and creates the vector tables.
- `synonym` starts empty. Agents fill it with the project's own words (`mem syn add`, instructed by SKILL.md);
  the code ships no vocabulary (D-20). Stores created before 0.0.1 keep the synonyms they were seeded with.

## 5. Language handling (`text.rs`)

- **Tokens** (`tokens`): regex words `[0-9A-Za-z가-힣][0-9A-Za-z가-힣\-.'·_]*`, trimmed of `.-'·_`, minus
  English and Korean stopwords, Korean particles stripped (`strip_ko`, longest-first list `JOSA`), and only
  tokens of **2+ characters** kept. Order-preserving dedup.
- **FTS query**: tokens of `--q` and `--kw` (split on `,` and newlines), expanded with `synonym`, each quoted and
  given a prefix `*` (except 1–3-letter ASCII words), joined with `OR`, ranked by
  `bm25(fw3, 8, 5, 1, 3, 2)` (title, summary, body, tags, kind). The trigram index was removed (D-03).
- **Shape** (`shape`): the set of title words with numbers replaced by `<n>`. Two titles have the same shape when
  Jaccard ≥ 0.5, or ≥ 3 shared words and Jaccard ≥ 0.4. Used to fold "one per module" notes.
- **Subject** (`subj2`): title words that are not values (no `=:/#%@`, not version-like numbers, not CONSTANT_CASE),
  particles stripped, plural `s` dropped, generic words removed. Two notes are on the same subject when their
  subjects are equal after mapping each word to its synonym group's representative (`Mem::canon_word`: smallest
  word reachable through `synonym`, ≤ 50 visited, cached) and have ≥ 2 words (D-21). Used by the write check and
  the `⚠ SAME SUBJECT` line.
- **Module** (`module_of`): the first tag that also appears as a word in the title (e.g. title "Cart worker
  count", tags "cart,ops" → `cart`).
- Known gap (D-14): one-character words and some two-syllable Korean words that end in a particle-like syllable
  (평가 → 평, 회의 → 회) are dropped; some English stopwords carry meaning (it, may, no). A unit test
  (`known_issue_short_words_are_dropped`) pins the current behavior so a fix is a deliberate change.

## 6. Search (`search.rs`, `Mem::run_search`, `fmt`)

1. `query_tokens`: tokens of q, tokens of each kw piece → dedup → synonyms.
2. `fts_ranked`: top 60 current notes (project / kind filters in SQL).
3. Embed builds with vectors present (`use_sem`): `sem_ranked` embeds `task: search result | query: <q> <kw>`,
   takes k = 150 by Hamming distance on `memory_vec`, reranks by int8 cosine on `memory_emb`, keeps 60.
   `MEM_MODE=interleave` (default) alternates FTS and vector lists, deduplicated; `fusion` / `fusion+fts2` are
   experimental score fusions kept for comparison.
4. `group_similar`: walks the first 60, folding a note under an earlier pick of the same kind with the same
   shape (compared with the pick and its first 4 members). Returns `limit` groups, plus up to 2 extra when large
   groups (≥ 5 members) take slots.
5. `topic_pins`: up to 2 current topic notes whose title words (minus "rules/규칙") match query tokens are put
   in front, unless already in the results or `--kind` excludes topics.
6. `fmt`: `[n] id · kind · date`, title, `↳ summary` (cut 220), topic bodies in full up to `MEM_TOPIC_SHOW`,
   folded members (up to `MEM_SIMSHOW`), `! overlaps with` (`conflicts`: decision/convention/gotcha pairs
   sharing ≥ 2 subject words), `⚠ … SAME SUBJECT` (same subject as above, synonym-aware).
7. Module cards (`module_cards`): if most notes tagged with a query word belong to one module (≥ 3 and ≥ 60 %),
   list that module's other current notes, one line each, ranked by FTS, `MEM_CARD` lines, at most 2 modules.
   Tags are read with one scan (`tag_scan`) instead of one `LIKE` per word.

Output is plain text tuned for agents: short lines, ids first, nothing decorative. Changing wording changes
what agents do; treat output text as an interface (golden tests catch accidental changes).

## 7. Writes (`write.rs`)

`add` (`Mem::do_add`), in order:

1. each `--supersedes` id must exist and be current (else `NOT SAVED`), unless `--force`;
2. duplicate check (`find_dupe`): among the FTS top 5, same title (case-insensitive) or token Jaccard ≥ 0.85
   → `NOT SAVED … duplicate`, unless `--force`;
3. same-subject check (`same_subject_notes`, only SUBJ_KINDS): a current note with the same subject (synonym-aware,
   §5) that is not being superseded → `NOT SAVED`, listing them, until the agent picks `--supersedes` or `--keep-both` (D-08);
4. similar notes (search top 3) are collected for the reply;
5. the embedding (if on) is computed **before** taking the lock — the model is slow, the lock is not held for it;
6. `BEGIN IMMEDIATE`; the duplicate and supersede checks run again (another session may have written since);
   insert into `memory` + `fw3`, mark superseded notes, store vectors; `COMMIT`.

`add-batch` runs the same per line. `--check` prints per line: new / DUPLICATE, `! SAME SUBJECT` (must decide),
`~ same topic` / `~ similar`. A bad line prints `NOT SAVED: bad line (…)` and the batch continues; a database
error stops the batch, printing the lines done so far (`partial`) before the error. `supersedes` may be a string
`"a,b"` or a list. `edit`, `retire`, `syn add`, `import` and `index` run inside one `BEGIN IMMEDIATE` transaction
each (`commands::run`).

## 8. Concurrency

WAL lets readers run during a write. Writers serialize on `BEGIN IMMEDIATE` with a 30 s busy timeout. Checks that
decide whether to write are repeated inside the transaction. The integration test
`concurrent_writers_lose_nothing` runs 6 processes × 8 adds and checks every note landed.

## 9. Embeddings (feature `embed`, `emb.rs`, D-06, D-13)

- Model: EmbeddingGemma 300M, Q8_0 GGUF (~330 MB), not shipped. Loaded lazily on the first call that needs it
  (≈ 0.3 s), through `llama-cpp-2` (llama.cpp compiled from source, static libstdc++), n_ctx 2048, pooled by the
  model's own pooling setting, L2-normalized, stored little-endian f32 → quantized by sqlite-vec (`vec_quantize_binary`,
  `vec_quantize_int8(…, 'unit')`).
- Document text: `title: <title> | text: <summary> <body>` (cut 4000 chars); query text:
  `task: search result | query: <q> <kw>` (EmbeddingGemma prompt formats).
- Failure handling (`try_embed`): if the model cannot be loaded or run, the call prints one warning
  (`embeddings off for this call`) and continues with keyword search; `add` saves without a vector. `mem index`
  fills missing vectors later. No model file → embeddings silently off.
- Building: llama.cpp is compiled with the CPU flags of `-C target-cpu`; without one it targets baseline x86-64
  (SSE2) and is several times slower. `build.rs` links `shim/glibc_compat.c` on linux-gnu so binaries built on
  newer glibc run on 2.35 (C23 `__isoc23_*` names, `arc4random`).
- Vectors are close to, not bit-identical with, the Python reference's (llama-cpp-python); they become identical
  when this crate is patched to the llama.cpp revision that llama-cpp-python 0.3.36 vendors (verified on the
  owner's PC). Search rankings were the same in those tests.

## 10. Compatibility with the Python reference (`pystr.rs`, D-10)

`reference/mem53.py` is the behavioral spec. On the keyword path the Rust output is byte-identical, so string
handling copies Python semantics where output depends on it: slicing by code points (`cut`, `cut_py`, `py_end`
for negative/oversized ends), `str.strip`/`split`/`splitlines` whitespace sets, `repr(float)` in the call log,
argparse prefix matching and its rule that a value may not look like an option (`cli::argparse_violation`).
Deliberate differences (D-12): ties in `list` grouping go to the earliest group (Python's choice depends on the
hash seed), and error cases that crash Python (bad JSON line, broken pipe, broken model) produce one-line errors
or continue.

When behavior should change, change it here and in the golden file together, and record a decision.

## 11. Rust module map

| Module | Responsibility |
|---|---|
| `main.rs` | argument pre-check, clap parsing, `where`, run, print, call log, exit codes (0 ok, 1 error, 2 usage) |
| `cli.rs` | `Cli`/`Cmd` definitions, `VALUE_OPTS`, argparse compatibility check |
| `commands.rs` | `run` (transaction wrapper) and `run_cmd` (one arm per command; brief, list, get, edit, retire, syn, import, index) |
| `search.rs` | ranking, grouping, topic pins, conflicts, module cards, output format |
| `write.rs` | duplicate / same-subject checks, `do_add`, add-batch line handling, JSON field helpers |
| `mem.rs` | `Mem` (connection + settings from env), embedding helpers, `insert`, `next_id` |
| `db.rs` | schema, migrations, connection, `Note` row type, value helpers |
| `text.rs` | tokens, josa, stopwords, shape, subject, module |
| `pystr.rs` | Python string semantics |
| `util.rs` | env helpers, date, store resolution, `where`, call log |
| `emb.rs` | llama.cpp embedder (feature `embed`) |
| `testutil.rs` | `TempStore` for unit tests |

Shared names come in through `crate::prelude` (std, rusqlite, regex, clap, indexmap, and every module's
`pub(crate)` items). Errors inside the store are `rusqlite::Error` (`db::err` wraps messages); commands return
`Box<dyn Error>`; `main` prints `mem: error: <message>`.

## 12. Tests

| Layer | Where | What it guards |
|---|---|---|
| Unit (35) | `#[cfg(test)]` in each module | tokens/josa/shape/subject rules, Python string semantics, schema+migration, ranking, grouping, pins, write checks |
| CLI integration (18) | `tests/cli.rs` | every command end to end through the binary, argument errors, store discovery, call log, concurrent writers |
| Golden (145 cases) | `tests/golden.rs` + `tests/fixtures/{shoplite.db,golden.json}` | byte-identical output to the Python reference on a 577-note store (search variants, get, brief, syn, list) |
| Embeddings (3) | `tests/embed.rs`, `--features embed`, `MEM_TEST_MODEL=<gguf>` | index, semantic hit for a paraphrase, vector written on add, broken/missing model fallback |
| Differential (outside the repo) | Python harness used during the port | 1,358 replayed real agent writes and 1,800 fuzz cases compared with the Python reference; final databases identical |

`cargo test --release` runs the first three (the embed tests skip without the feature or the model). With a model,
build the embed tests with `RUSTFLAGS="-C target-cpu=native"`, otherwise indexing the fixture takes ~13 minutes.
`scripts/regen_golden.py` re-runs the Python reference (`reference/mem53.py`, plain Python 3 on the keyword path)
on the same cases — only when an output change is intended and recorded in DECISIONS.md.

## 13. Performance (measured during the port)

| Case | Python reference | Rust |
|---|---|---|
| search, 5k notes, keyword (container) | 58 ms | 8.2 ms |
| search, 5k notes, keyword (owner's PC, 2 vCPU VM) | 75 ms | 9.5 ms |
| search with embeddings (PC) | 799 ms | 408–434 ms |
| peak memory with embeddings (PC) | 671 MB | 407–431 MB |

Keyword builds start in a few milliseconds; most of a search is SQLite work. With embeddings the model load
(~0.3 s) dominates, which is why embeddings are off unless a model file is present.

## 14. Packaging

`Cargo.toml` release profile: LTO, one codegen unit, stripped. `.cargo/config.toml` links the MSVC C runtime
statically. `.github/workflows/ci.yml` (fmt, clippy -D warnings, tests on 3 OSes, embed build on Linux) and
`release.yml` (tag `vX.Y.Z` → archives for Windows x64/arm64, Linux musl, macOS x64/arm64, optional embed
archives, SHA256SUMS, GitHub release, winget PR). Manifest templates in `packaging/winget/`. Steps in RELEASING.md.
