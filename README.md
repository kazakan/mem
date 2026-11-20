# mem

**Project memory** that AI agents share across sessions. Decisions, conventions, gotchas, facts and handoffs are
stored in one local SQLite file and searched with a small CLI.

- No server, no network, no external API. One command is one process (a search takes about 10 ms on 5,000 notes).
- Keyword search (SQLite FTS5) with Korean particle handling and synonyms. Embeddings (EmbeddingGemma) are an
  optional build.
- Saving is blocked when a second note on the same subject would also stay "current" (supersede / keep-both),
  and similar notes are folded into one group in search results.
- Agent instructions: [SKILL.md](SKILL.md). Design and decision records: [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md),
  [docs/DECISIONS.md](docs/DECISIONS.md).

Version 0.0.1. A Rust port of the Python `mem` v5.3. Its keyword-search output was verified byte-identical to
v5.3, after which the Python version was retired (D-22).

## Install

### winget (Windows, once published)

```powershell
winget install kazakan.mem
mem where
```

### Release archives

Unpack `mem-<version>-<target>.zip|tar.gz` from a GitHub release and put `mem` (`mem.exe`) on your PATH, or copy
it into each project as `<project>/.bin/mem`. The embedding builds are the `mem-embed-*` archives.

### Build from source

```sh
cargo build --release                      # keyword-search build, target/release/mem (about 5 MB)
cargo build --release --features embed     # embedding build (compiles llama.cpp too: needs CMake and a C++ compiler)
```

Always turn on CPU optimizations for the embedding build. Without them llama.cpp is built for baseline SSE2 and
runs several times slower.

```sh
RUSTFLAGS="-C target-cpu=native" cargo build --release --features embed       # this machine only
RUSTFLAGS="-C target-cpu=x86-64-v3" cargo build --release --features embed    # any AVX2 machine
```

On Windows the embedding build links llama.cpp against the same static C runtime as mem; the settings for that
are in `.cargo/config.toml`, so nothing extra is needed.

## Where the store lives

Each call picks the store (an SQLite file) in this order. `mem where` shows the result; `where` creates nothing.

1. the `MEM_DB` environment variable
2. if the binary is inside `<project>/.bin/` (or a `.mem/` exists above the binary's folder) → `<project>/.mem/mem.db`
3. if installed on PATH (winget etc.) → the first `.mem/mem.db` found walking up from the current folder
4. if there is none yet → a new `.mem/mem.db` in the nearest parent folder that has `.git` (else the current folder)

The same folder holds `calls.log` (a call log, one JSON line per call) and the optional embedding model
`model.gguf`. One store can hold several projects, separated by `--project`.

## What a note looks like

| Field | Meaning |
|---|---|
| id | `<first 2 letters of project>-<first 4 letters of kind>-<number>`, e.g. `sh-deci-003` |
| kind | `decision`, `convention`, `gotcha`, `fact`, `handoff`, `topic` (free text, but these six are used) |
| title | the subject (what it is about, not the value), e.g. `Wishlist alert channel` |
| summary | a one-line summary (the value), e.g. `goes to #shop-alerts` |
| body | details (optional) |
| tags | comma separated; module names here are used by module cards |
| status | `current` / `superseded` (replaced by a newer note) / `retired` (wrong or duplicate, hidden) |
| source | `user` (the user said it) / `agent` (the agent decided it) |

A `topic` note collects a subject's project-wide rules in one note (title `<subject> rules`, body one
`- rule (since <date>)` line per rule).

## Commands

Every command writes its result to stdout and errors to stderr as one `mem: error: ...` line. Exit codes: 0
success, 1 runtime error, 2 argument error. Option names can be abbreviated (`--proj`). Values that start with
`-` (`"- item"`) are accepted.

### brief — start-of-session summary

```sh
mem brief --project shop
```

Current note counts (by kind), the topic notes, the 2 latest handoffs, and the 8 most recently changed notes.

```
# shop: 2 current notes (decision 1, convention 1)
Before deciding: mem search --project shop --q ... --kw ... · rules overview: mem list --project shop --kind topic,convention,decision,gotcha
## Recently changed
- sh-deci-003 · decision · Wishlist alert channel
- sh-conv-002 · convention · Cart error code prefix
- sh-deci-001 · decision · Wishlist alert channel
```

### search — search current notes

```sh
mem search --project shop --q "where do wishlist alerts go?" --kw "wishlist,alert,channel,알림"
mem search --project shop --q "찜 알림은 어디로 보내?" --kw "wishlist,alert,channel,알림"    # Korean works the same way
```

| Option | Description |
|---|---|
| `--q` (required) | the user's request or question, as is |
| `--kw` | extra keywords, comma separated: key terms, synonyms, both English and Korean spellings |
| `--project` | this project only (default: all) |
| `--kind` | these kinds only, comma separated |
| `--limit` | number of result groups (default 5, `MEM_LIMIT`) |

Output: notes by rank (title, summary; topic notes with their body). Notes of the same pattern (such as one rule
per module) are folded under the first one as titles only (at most `MEM_SIMSHOW`). A topic note the query
touches is put first, and when the query names a module, that module's other current notes follow one line each.
Two current notes on the same subject get `⚠ … SAME SUBJECT`, and current notes whose content overlaps get an
`! overlaps with …` line. Use `get` for the full text.

```
[1] sh-deci-003 · decision · 2026-11-18
    Wishlist alert channel
    ↳ Now goes to #shop-alerts
```

Searchable words: 2 characters or more, Korean particles (은/는/이/가/을/를/의/에/에서/로/으로/와/과/도/만 …)
stripped, common stopwords dropped. Synonyms added by agents (`mem syn show`) widen the search. **Known
limitation**: one-character words (찜, 팀, C) and some two-syllable Korean words that end like a particle (평가,
회의) are currently dropped from queries. Give another spelling in `--kw` as well (찜 → wishlist). Proposed fix:
docs/DECISIONS.md D-14.

### list — overview listing

```sh
mem list --project shop --kind topic,convention,decision,gotcha
mem list --project shop --status superseded --full
```

| Option | Description |
|---|---|
| `--project` | this project only |
| `--kind` | comma separated |
| `--status` | `current` (default) / `superseded` / `retired` / `all` |
| `--tag` | notes with this tag |
| `--source` | `user` / `agent` |
| `--full` | print summaries too |
| `--limit` | maximum lines (default 200) |

Notes of the same pattern are folded into one line (`+ 16 same pattern, e.g. …`). Use `search` to find a subject.

### get — show notes in full

```sh
mem get sh-deci-003 sh-conv-002
```

```
sh-deci-003 · decision · 2026-11-18 · shop · current · source user
Wishlist alert channel
Now goes to #shop-alerts

tags: wishlist,alert
```

After the status on the first line, a replaced note shows `superseded → <new id>` and a hidden note
`retired (<reason>)`.

### add — save a note

```sh
mem add --project shop --kind decision --title "Wishlist alert channel" \
        --summary "Wishlist incidents go to #wishlist-alerts" --tags "wishlist,alert"
```

| Option | Description |
|---|---|
| `--project`, `--kind`, `--title`, `--summary` (required) | |
| `--body` | details |
| `--tags` | comma separated |
| `--supersedes <id>[,<id>…]` | old notes this one replaces (they become `superseded`) |
| `--source user\|agent` | default `user`; use `agent` for what the agent decided itself |
| `--force` | save even if it looks like a duplicate |
| `--keep-both` | keep both even though a current note on the same subject exists (an added fact) |

Checks before saving:

- **Duplicate**: same title or nearly the same content → `NOT SAVED: looks like a duplicate of <id>` → choose
  `mem edit`, `--supersedes` or `--force`.
- **Same subject** (decision/convention/gotcha/fact): if a current note on the same subject exists, `NOT SAVED: …`
  lists the candidates. Titles are the same subject when their words, minus values, are the same; words registered
  as synonyms count as the same word (Wishlist alert channel = 위시리스트 알림 채널). A title with one extra word is a
  different subject. If something changed (now, only, instead, no longer, moved to; 이제, ~만, 대신, 더 이상,
  옮겼어), use `--supersedes <id>`; if something was added (also, in addition; ~도, 에 더해), use `--keep-both`.
- On success: `saved <id>`, or `saved <id> (superseded <id>)` after a replacement, followed by a few lines of
  similar notes if there are any.

### add-batch — many notes at once

```sh
mem add-batch --check --project shop notes.jsonl    # preview: saves nothing
mem add-batch --project shop notes.jsonl            # save
cat notes.jsonl | mem add-batch --project shop -    # from stdin
```

The file has one JSON object per line:

```json
{"kind": "fact", "title": "Cart worker count", "summary": "2", "tags": "cart"}
{"kind": "decision", "title": "Wishlist alert channel", "summary": "#shop-alerts", "supersedes": "sh-deci-001"}
{"kind": "fact", "title": "Cart retry policy", "summary": "3 times", "keep_both": true, "source": "agent"}
```

Fields: `kind`, `title`, `summary` (required), `body`, `tags`, `supersedes` (a string `"a,b"` or a list
`["a","b"]`), `source`, `keep_both`, `force`. `--check` shows per line: `new` / `DUPLICATE of …`,
`! SAME SUBJECT …` (supersedes or keep_both must be decided), `~ same topic` / `~ similar`. A bad line is marked
`NOT SAVED: bad line (…)` and the batch goes on with the next line.

```
1: new · Cart worker count
     ~ same topic: sh-conv-002 · Cart error code prefix
2: DUPLICATE of sh-conv-002 · Cart error code prefix
     ! SAME SUBJECT, decide supersedes or keep_both: sh-conv-002 · Cart error code prefix — Cart error codes start with CART-
```

### edit — small corrections

```sh
mem edit sh-conv-002 --summary "Cart error codes start with CART- (e.g. CART-404)"
mem edit sh-conv-002 --append "- payments follow the same rule since 2026-11"
```

Options: `--title`, `--summary`, `--body`, `--tags`, `--kind`, `--append` (adds a line at the end of the body).
When a decision changed, do not edit: create a new note with `add --supersedes` (the history is kept).

### retire — hide wrong or duplicate notes

```sh
mem retire sh-fact-010 sh-fact-011 --reason "duplicate of sh-fact-009"
```

Notes are never deleted; they stay as `retired` (visible with `list --status retired` and `get`).

### syn — search synonyms

```sh
mem syn show                                # one line of synonyms per word
mem syn add wishlist 위시리스트 찜목록        # register as synonyms of each other (stored lowercase)
```

A new store has no synonyms. Every project uses its own words, so none are built in: agents add the project's
other spellings (English/Korean, abbreviations, module nicknames) as they meet them, following SKILL.md (D-20).
Synonyms apply to the whole store (all projects). They widen searches and are used by the same-subject check when
saving (D-21). One-character words are dropped from queries (D-14), so use words of 2 or more characters in
`syn add`.

### import — move or merge stores

```sh
mem import notes.jsonl
```

One JSON object per line with every column as is (ids are kept): `id`, `project`, `kind`, `date`, `title`,
`summary` (required), `body`, `tags`, `status` (default current), `superseded_by`, `source` (default user). It
runs as one transaction, so if it fails partway nothing is imported.

### index — build embeddings (embedding build only)

```sh
mem index      # embeds every note that has no embedding yet. Output: indexed <count>
```

With a model (`model.gguf`) present, `add` stores the embedding when it saves. Use `index` when turning
embeddings on for the first time, or to fill in notes saved by a keyword-only build.

### where — which store is used

```sh
mem where
```

```
db: /home/me/repo/.mem/mem.db (exists)
  chosen by: nearest .mem/ from the current folder
model: /home/me/repo/.mem/model.gguf (not found)
embeddings: off (this build has no embed feature)
version: 0.0.1 (keyword-only build)
```

## Embeddings (optional)

Build with `--features embed` and put EmbeddingGemma 300M Q8_0 GGUF (about 330 MB, downloaded separately) at
`.mem/model.gguf`, or point `MEM_MODEL` at it. Search then interleaves keyword results with semantic results. If
the model cannot be loaded, that call prints one warning and falls back to keyword search. Cost: model load
about 0.3 s, a search about 0.4 s, memory about 400 MB. From 500 to several thousand notes keyword search alone
is accurate enough, so embeddings are off by default (D-06).

## Environment variables

| Variable | Default | Description |
|---|---|---|
| `MEM_DB` | (rules above) | path of the store file |
| `MEM_MODEL` | `<store folder>/model.gguf` | embedding model |
| `MEM_NO_EMBED` | | any value turns embeddings off |
| `MEM_MODE` | `interleave` | how embeddings are combined: `interleave`, `fusion`, `fusion+fts2` (experimental) |
| `MEM_LIMIT` | 5 | number of search result groups |
| `MEM_NO_SYN` | | any value turns synonym expansion off |
| `MEM_NO_GROUP` | | any value turns same-pattern folding off |
| `MEM_CARD` | 6 | lines in a module card |
| `MEM_TOPIC_SHOW` | 1500 | maximum characters of a topic note's body shown in search |
| `MEM_SIMSHOW` | 8 | same-pattern notes folded under a search result |
| `MEM_TIMING` | | any value prints embedding timings (stderr) |

## Development

```sh
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release                       # unit + CLI integration + golden (145 output snapshots)
MEM_BLESS=1 cargo test --release --test golden   # update the snapshot after an intended output change (review the diff, then commit)
MEM_TEST_MODEL=/path/embeddinggemma-300M-Q8_0.gguf cargo test --release --features embed
```

Agents changing this code should read [AGENTS.md](AGENTS.md) first. Releasing: [RELEASING.md](RELEASING.md).

## License

MIT, see [LICENSE](LICENSE). The optional EmbeddingGemma model is not part of mem; it is downloaded separately
under its own terms.
