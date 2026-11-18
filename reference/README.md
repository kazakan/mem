# Reference implementation

- `mem53.py` — the Python implementation (v5.3) the Rust port follows, with later changes kept in step with
  the Rust code: no built-in synonyms (D-20), synonym-aware same-subject checks (D-21). It is the behavioral spec for the keyword
  path: `tests/fixtures/golden.json` holds its output, and the Rust binary must match it byte for byte
  (docs/DECISIONS.md D-12). Run it with `python3 reference/mem53.py <command>`; embeddings additionally need
  `llama-cpp-python`, `sqlite-vec` and `numpy`.
- `SKILL_mem53.md` — the skill text agents were tested with. SKILL.md at the root is this text with a generic
  project name and four additions (`mem where`, one-letter words, synonyms, handoffs).
