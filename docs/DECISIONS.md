# Decisions

Why mem is built the way it is. One entry per decision: context, decision, consequences. "Owner" = the person who
commissioned mem; entries marked *owner decision* were made by them and should not be reversed without asking.
Add new entries at the end (D-22, …); when a decision is replaced, mark the old one `superseded by D-xx` instead
of deleting it — the same rule mem applies to its own notes.

Status: **accepted** (in the code), **proposed** (agreed problem, fix not chosen), **superseded**.

---

## D-01 Local SQLite file + CLI + skill; no server, no MCP, no external API — accepted, *owner decision*

**Context.** Several coding agents (different tools, sessions, sometimes at once) need shared, persistent project
knowledge, like a personal RAG that is read and written continuously. The owner wants nothing to leave the machine.
Plain Markdown files were considered and rejected: no ranking, no duplicate checks, no concurrent-write safety.

**Decision.** One SQLite file per store; a CLI that runs one command per process; agents learn it from a skill
file (SKILL.md) rather than an MCP server. No network code at all. The `sqlite-memory` extension was evaluated
and not used.

**Consequences.** Any agent that can run a shell command can use mem. Startup cost matters (→ D-12, D-06).
Concurrency is SQLite's (→ ARCHITECTURE §8).

## D-02 Notes are small, typed records; title is the subject, summary is the value — accepted

**Context.** Agents read search output inside their context window; long or chatty records cost tokens and
attention, and contradictions hide in prose.

**Decision.** A note has kind (decision, convention, gotcha, fact, handoff, topic), title (the subject, e.g.
"Wishlist alert channel"), summary (the value, one line), optional body and tags, status and source
(user/agent). Several projects can share a store; `--project` scopes every command and prefixes ids.

**Consequences.** Same-subject detection (D-08) and grouping (D-07) work on titles. SKILL.md tells agents how
to write titles; the quality of memory depends on it.

## D-03 Keyword search with SQLite FTS5 as the core; no trigram index — accepted, *owner decision (trigram)*

**Context.** Must stay fast and light at 500+ (thousands of) notes, Korean and English mixed.

**Decision.** FTS5 contentless table `fw3` (title, summary, body, tags, kind), `porter unicode61` tokenizer,
prefix indexes 2 and 3, query = OR of prefix terms, ranked by `bm25` with weights title 8, summary 5, body 1,
tags 3, kind 2. A trigram index was tried and removed: it made no measurable difference in the tests.

**Consequences.** Korean words are matched as whole tokens or prefixes, so particles must be stripped on the
query side (D-04). Substring matches inside Korean compounds are not found (see D-14).

## D-04 Korean handling: particle stripping, stopwords, synonyms, agent-supplied keywords — accepted (seed part superseded by D-20)

**Context.** Korean attaches particles (은/는/이/가/을/를/에서/으로/만/도 …) to words; users mix Korean and English
terms for the same thing (알림 / alert).

**Decision.** Query and title tokens strip a longest-first list of particles, drop KO/EN stopwords and keep
tokens of 2+ characters (`text.rs`). A synonym table expands queries both ways (originally seeded with 58
groups; since D-20 it starts empty and agents fill it with `mem syn add`). The agent passes `--kw` with 3–6 extra
keywords (synonyms, both spellings) — the agent knows the domain better than any static table, and it costs
nothing at search time. Focus is Korean and English;
other languages are to be added later as switchable rules (*owner decision*).

**Consequences.** Good recall for normal words without a morphological analyzer. Short words suffer (D-14).

## D-05 Bounded, folded output — accepted

**Context.** Token use was measured against other approaches during design; output size is the main cost of a
memory tool for agents.

**Decision.** search returns 5 groups by default, summaries cut at 220 characters, bodies only for topic notes;
brief is capped at 4500 characters; list prints titles unless `--full`; every add/add-batch reply is a few lines.
Every call is logged with its output size (`calls.log`, D-16) so the cost can be measured on real use.

## D-06 Embeddings optional, off unless a model is present; interleaved with keyword results — accepted

**Context.** EmbeddingGemma 300M (local, CPU) catches paraphrases keyword search misses, but costs ~0.3 s model load,
~0.4 s per search and ~400 MB memory per call on the owner's PC (no GPU). On the test stores keyword search with
`--kw` was already accurate.

**Decision.** A separate build (`--features embed`) and a model file switch embeddings on; otherwise mem is
keyword-only. When on, the FTS list and the vector list are interleaved (FTS #1, vector #1, FTS #2, …,
deduplicated) instead of score fusion: no score calibration, and the best keyword hits always stay at the top.
Vectors: binary quantized for a k=150 first pass (sqlite-vec), int8 cosine rerank. If the model fails, the call
falls back to keyword search with a one-line warning. Fusion modes stay behind `MEM_MODE` for experiments.

## D-07 Fold same-pattern notes in search and list — accepted

**Context.** Real stores contain families of notes that differ only by module ("Cart error code prefix",
"Auth error code prefix", …). With hundreds of such notes, a search fills its 5 slots with one family.

**Decision.** Notes of the same kind whose titles have the same shape (word sets with numbers normalized,
Jaccard ≥ 0.5, or ≥ 3 shared and ≥ 0.4) are folded under the first one; members are listed by title. list folds
them onto one line. Large groups can earn up to 2 extra slots.

## D-08 One subject, one current note: supersede or keep both, decided at write time — accepted

**Context.** The main failure of agent memory is two current notes that disagree ("alerts go to #a", later
"alerts go to #b"), after which agents pick either. Users signal replacement with words like now / only /
instead / 이제 / ~만 / 대신 / 더 이상 / 옮겼어, and addition with also / ~도 / 에 더해.

**Decision.** `add` of a decision/convention/gotcha/fact refuses (`NOT SAVED`) while another current note has
the same subject (same set of ≥ 2 non-value title words; registered synonyms count as one word since D-21), until the agent chooses `--supersedes <id>` (old note
becomes `superseded`, history kept) or `--keep-both`. Duplicates (same title, or token Jaccard ≥ 0.85) are refused
unless `--force`. Checks repeat inside the write transaction. Search marks leftovers with `⚠ … SAME SUBJECT`.
Notes are never deleted; wrong ones are `retired` with a reason.

## D-09 Project-wide rules live in topic notes — accepted

**Context.** Rules that apply to every module (review rules, testing rules) were saved as many small notes and
missed by searches about a specific module.

**Decision.** Kind `topic`: one note per subject, title `<Subject> rules`, body one rule per line with a
since-date; changes supersede the whole topic note. brief lists all topic notes; search pins up to 2 matching
topic notes in front of the results, with their body.

## D-10 Module cards: small search, plus one line per related note — accepted

**Context.** An agent searching "wishlist alert" also needs the wishlist module's other rules (owner, deploy day),
which it did not ask for. Raising the result limit costs many tokens.

**Decision.** When most notes tagged with a query word belong to one module, search appends that module's other
current notes, one line each (`MEM_CARD`, default 6; at most 2 modules). Implemented with one tag scan.

## D-11 add-batch with a dry run — accepted

**Context.** At the end of a session agents save many notes at once; one bad line or an unnoticed duplicate made
batches unreliable.

**Decision.** `add-batch` takes JSON lines (file or stdin). `--check` saves nothing and reports per line:
new / duplicate / same subject (must decide) / similar. In save mode a malformed line is reported and skipped;
the rest continue.

## D-12 Rust port, byte-identical to the Python reference on the keyword path — accepted

**Context.** The reference implementation is Python (`reference/mem53.py`, v5.3). Startup and search time
mattered for a tool called many times per session (5k notes: Python 58–75 ms per search; Rust 8–10 ms).

**Decision.** Port to Rust and keep output identical byte for byte on the keyword path, verified by golden tests
(145 cases), replay of 1,358 real agent writes and 1,800 fuzz cases. Python string semantics are reproduced
where output depends on them (`pystr.rs`). Deliberate differences:
- `list` grouping ties go to the earliest group (Python's pick depends on the hash seed, so it is not stable);
- inputs that crash Python give one-line errors or continue: a bad add-batch line, a closed pipe, a broken model
  file; `supersedes` in add-batch may also be a JSON list.

**Consequences.** The Python file is the spec for behavior changes: change both or record why not
(`scripts/regen_golden.py` refreshes the expected outputs).

## D-13 Embed build: llama.cpp from source, static, portable to older glibc — accepted

**Decision.** `llama-cpp-2`/`llama-cpp-sys-2` 0.1.161 with static libstdc++, `sqlite-vec` 0.1.9 compiled in.
`build.rs` links small shims (`shim/glibc_compat.c`) on Linux so binaries built against newer glibc run on 2.35.
Builds must set a CPU target (`-C target-cpu=native` or `x86-64-v3`); without it llama.cpp runs at baseline speed.

**Consequences.** Vectors are close to but not bit-identical with the Python build's, because the llama.cpp
versions differ; patching to the revision llama-cpp-python 0.3.36 vendors makes them identical (verified on the
owner's PC). Search results matched in those tests. Re-index (`mem index` on a fresh vector table) if the model or
llama.cpp version changes enough to matter.

## D-14 Short words are dropped from queries and titles — **proposed** (fix not chosen; owner to decide)

**Problem.** Tokens shorter than 2 characters are discarded after particle stripping, so:
- one-syllable Korean nouns: 찜, 팀, 잡, 큐, 키, 넬, 널, 눈 — and single letters like `C` — are unsearchable;
- two-syllable words ending in a syllable that is also a particle lose it and then fall under the limit:
  평가 → 평, 결과 → 결, 권한 → 권, 필요 → 필, 회의 → 회, 단가 → 단;
- English stopwords include words that carry meaning in this domain: it/IT, may, down, before, after, can, will, no.

The same tokenizer feeds the same-subject check (D-08): "찜 알림 채널" and "팀 알림 채널" both reduce to {알림, 채널},
so saving the second is refused as the same subject (checked); a note titled only with dropped words gets no subject.
`known_issue_short_words_are_dropped` in `text.rs` pins today's behavior.

**Mitigations today.** `--kw` with another spelling (찜 → wishlist, 위시리스트), agent-added synonyms, the SKILL.md line
telling agents so. Project scoping reduces noise but not the miss: the risk is an agent not finding a rule and
writing a duplicate or contradicting note.

**Comparison.** Basic Memory 1.0.0 indexes Hangul as unigrams and bigrams, so it finds one-syllable words, at
the price of substring noise (찜 matches every word containing 찜); it has no stemming and requires all words
(AND), so a query such as "retry policy" fails in text mode unless both words appear as written, and short
prefixes such as "C" match widely.

**Options (not chosen).**
1. Do not strip a particle when only one character would remain (평가 stays 평가).
2. Keep one-character Hangul words, matched exactly (no prefix `*`), plus their particle forms (찜이, 찜을 …).
3. A short stoplist for one-character words that are noise; trim the English stopword list to function words.
4. Optionally fall back to a substring scan (`LIKE`) for one-character words, scoped to the project.

Any of these changes output, so golden cases and the Python reference must change with it (D-12).

## D-15 Command-line behavior compatible with argparse — accepted

**Decision.** Long options may be abbreviated (`--proj`), values may start with `-` (`--body "- item"`), a value
that looks like an option is an error (`argument --tags: expected one argument`), exit codes 0 / 1 (error) /
2 (usage), errors as `mem: error: <message>` on stderr. Same as the Python reference, which agents were tested with.

## D-16 Store layout `.mem/` next to `.bin/`, with a call log — accepted

**Decision.** In a project, the binary lives at `<project>/.bin/mem` and the store at `<project>/.mem/mem.db`
(beside it `calls.log`, `model.gguf`). `MEM_DB` overrides. Every call appends `{t, argv, out_chars}` to
`calls.log`, used to measure how agents use mem and what it costs in output size.

## D-17 Installed binaries find the store from the current folder; `mem where` — accepted (new in 0.0.1)

**Context.** A winget/PATH install puts one `mem.exe` in a shared folder; the D-16 rule (store next to the
binary) would send every project to one store inside the install folder.

**Decision.** Resolution order: `MEM_DB`; project-local binary (exe in `.bin`, or `../.mem` exists) → D-16 path;
else the nearest `.mem/mem.db` upward from the current folder; else a new store at the git root, else the current
folder. `mem where` prints the choice without creating anything.

**Consequences.** `.bin` layouts behave exactly as before. Agents must run mem from inside the project (they do).
Running from an unrelated folder creates a store there — `mem where` makes that visible.

## D-18 Distribution: GitHub releases + winget (portable zip) — accepted

**Decision.** Tag `vX.Y.Z` builds archives for Windows x64/arm64 (static C runtime), Linux x86_64 (musl, static),
macOS x64/arm64, with README, SKILL.md and CHANGELOG. winget gets the Windows keyword build as a zip with a nested
portable `mem.exe` and the command alias `mem`; the first submission is manual (`wingetcreate`), later ones by the
release workflow. Embed builds are attached as optional `mem-embed-*` archives, not in winget; the model is never
redistributed. crates.io publishing is off.

**Open.** License not chosen yet (needed before the first public release); winget identifier and GitHub owner
are placeholders.

## D-19 Version 0.0.1; documentation languages — accepted, *owner decision (version)*

**Decision.** The Rust project starts at 0.0.1 (the Python reference's v5.3 numbering is its own history).
README.md is in Korean for the owner; SKILL.md, ARCHITECTURE.md and this file are in English for agents (and
because agents were tested with the English skill text).

## D-20 No built-in synonyms; agents register the project's words — accepted, *owner decision*

**Context.** The reference seeded every new store with 58 synonym groups. Most were general development terms
(alert/알림, retry/재시도), but some came from the test project, an online shop (refund/환불, order/주문,
payment/결제, inventory/재고, customer/고객, vendor/연동사): vocabulary of one domain shipped as product code,
which flattered the test results and adds noise elsewhere (order as "sort order" pulling in purchase-order notes).
Which words are synonyms depends on the project, and the agents working in it know them.

**Decision.** The code ships no synonyms; `synonym` starts empty. SKILL.md tells agents to register words that
mean the same thing in the project (`mem syn add`, e.g. English/Korean spellings, abbreviations, module nicknames),
and `--kw` stays for one-off terms. `syn show` on an empty table says how to add them. The Python reference was
changed the same way, so the golden outputs (whose fixture store carries its own synonym rows) are unchanged.

**Consequences.**
- New stores search with fewer expansions until agents add words; existing stores keep their seeded rows.
- Synonyms apply to the whole store (every project in it).
- One-letter words are dropped before expansion (D-14), so `syn add 찜 wishlist` does not help a query "찜".
- The same-subject check (D-08) compared title words literally, so registered synonyms did not help it;
  fixed by D-21.

## D-21 Same-subject checks compare words through synonym groups — accepted, *owner decision*

**Context.** The same-subject check (D-08) found candidates with synonym-expanded search but then required the
title word sets to be literally equal. Registered synonyms therefore did not stop "위시리스트 알림 채널" next to
"Wishlist alert channel", or "Wishlist notification channel"; only the "similar notes" hint after saving showed
the old note, and acting on it was left to the agent.

Embeddings were considered instead and rejected: on EmbeddingGemma title vectors, the Korean translation of
"Wishlist alert channel" scored 0.805, about the same as the *different* subject "Cart alert channel" (0.796) and
"장바구니 알림 채널" (0.752). Stores are full of same-pattern families across modules (D-07), which embeddings rate as
near-identical, so no threshold separates "same subject" from "sibling module". Embeddings are also optional.

**Decision.** Before comparing, each subject word is mapped to its synonym group's representative: the smallest
string among the words reachable through `synonym` (breadth-first, at most 50, cached per process). Equal mapped
sets with ≥ 2 members are the same subject. Applies to the write check (`add`, `add-batch --check`) and to the
`⚠ SAME SUBJECT` lines in search. `MEM_NO_SYN` turns it off with the rest of the synonym use. Implemented in the
Python reference (`canon_word`, `same_subj`) and Rust (`Mem::canon_word`, `Mem::same_subject`) identically.

**Evidence.** Replaying 604 real agent adds on the 5k-note store (which carries the old seeded synonyms): 5 more
adds were held as same subject, all of them a Korean title for an existing English note (e.g. "fraud 캐시 TTL 60초"
next to "Fraud cache TTL"); no other outcome changed. Golden outputs were unchanged; Rust and the Python reference
agree on the write replays and 1,800 fuzz cases.

**Not covered.** Titles that add or drop a word ("Wishlist incident alert channel") are still a different
subject; relaxing to subset matching was discussed and not adopted, because it would also hold sibling notes such
as "Cart worker count" and "Cart worker count limit". Words with no registered synonym (질문 / questions) stay
different until an agent adds them.
