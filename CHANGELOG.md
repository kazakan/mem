# Changelog

## Unreleased

- Fix: after `import`, `add` could fail on every try with `UNIQUE constraint failed: memory.id` when an imported
  note already had the next id; the new id now skips numbers in use (D-23).
- README.md is now in English (D-24).
- License: MIT. Published as https://github.com/kazakan/mem; winget identifier `kazakan.mem` (D-25).
- Release archives include `LICENSE` and `THIRD_PARTY_NOTICES.txt` (third-party license notices).

## 0.0.1 — 2026-11-18

First version of the Rust CLI, ported from the Python `mem` v5.3.

- Commands: brief, search, list, get, add, add-batch (`--check`), edit, retire, syn, import, index, where.
- Output was verified byte-identical to Python v5.3 on the keyword (FTS) path: 145 golden cases, write
  replays of 1,300+ real agent calls, and 1,800 fuzz cases (except hash-random ties in `list`, see
  docs/DECISIONS.md D-12).
- Optional `--features embed`: EmbeddingGemma through llama.cpp, sqlite-vec binary search + int8 rerank,
  interleaved with keyword results. Falls back to keyword search when the model cannot be loaded.
- New in the Rust port: `mem where`, and store discovery for installed binaries (nearest `.mem/` upward,
  else the git root) so a winget/PATH install works per project (D-17).
- No built-in synonyms: new stores start with an empty synonym table and agents add the project's words with
  `mem syn add` (SKILL.md); the reference's seed list mixed in test-domain words (D-20).
- The Python code is no longer part of the project; `tests/fixtures/golden.json` is kept as an output snapshot
  (`MEM_BLESS=1` to update) (D-22).
- The same-subject check and `⚠ SAME SUBJECT` treat registered synonyms as one word, so a Korean title for an
  existing English subject is held for supersede / keep-both (D-21).
- Packaging: GitHub Actions CI and tag-driven release builds (Windows x64/arm64, Linux musl, macOS),
  winget manifest templates.

Known issue, not changed yet: one-syllable words (찜, 팀, C) and some two-syllable Korean words that end in
a particle-like syllable (평가, 회의) are dropped from queries and titles; some English stopwords carry
meaning (it, may, no). See docs/DECISIONS.md D-14.
