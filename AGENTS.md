# Working on mem (for coding agents)

mem is a local, single-binary memory store for AI agents (Rust, SQLite FTS5, optional llama.cpp embeddings).
Users and other agents call it from the shell many times per session, so it must stay **light, local and
predictable**. Read in this order before changing anything:

1. `docs/ARCHITECTURE.md` — how it works, module map, tests.
2. `docs/DECISIONS.md` — why; constraints marked *owner decision* need the owner's OK to change.
3. The module you touch, and its unit tests.

## Ground rules

- No network code, no server, no background process. One command = one short process.
- Output text is an interface: agents act on its wording. Any output change must be intended, reflected in
  `reference/mem53.py` (or recorded as a deliberate difference) and in `tests/fixtures/golden.json`
  (`python3 scripts/regen_golden.py`), and recorded in `docs/DECISIONS.md`.
- Keep the keyword build small and fast; anything heavy goes behind a cargo feature like `embed`.
- Korean and English are first-class; language rules live in `src/text.rs`. Other languages: add switchable rules.
- Never delete notes; status changes only (superseded / retired).
- Known open issue: short words (D-14). Don't fix it on the side — it changes output; propose and get a decision.

## Commands

```sh
cargo fmt --check
cargo clippy --release --all-targets -- -D warnings
cargo test --release                       # unit + CLI integration + golden
# embedding build (compiles llama.cpp; set a CPU target or it is very slow)
RUSTFLAGS="-C target-cpu=native" MEM_TEST_MODEL=/path/embeddinggemma-300M-Q8_0.gguf \
  cargo test --release --features embed
```

## Adding or changing a command

1. `src/cli.rs`: the `Cmd` variant with doc comments (they become `--help`); add value options to `VALUE_OPTS`.
2. `src/commands.rs`: the `run_cmd` arm; add it to the `locked` list in `run` if it writes.
3. Tests: a unit test next to the logic, a CLI test in `tests/cli.rs`.
4. Docs: README.md (Korean command reference), SKILL.md if agents should use it, ARCHITECTURE.md, a DECISIONS entry.
5. CHANGELOG.md.

## Style

rustfmt (`max_width = 120`), clippy clean with `-D warnings`, comments in English, explain *why* not *what*.
Shared imports come from `crate::prelude`. Errors: `rusqlite::Error` inside the store (`db::err`), boxed errors at
the command level, one line on stderr.

## Releasing

See RELEASING.md (version in Cargo.toml + tag `vX.Y.Z`; CI builds and publishes; winget via `wingetcreate`).
