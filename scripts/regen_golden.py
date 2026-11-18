#!/usr/bin/env python3
"""Re-run the Python reference on every case in tests/fixtures/golden.json and rewrite the expected outputs.

Use only when the reference behavior changed on purpose (record it in docs/DECISIONS.md), e.g. after editing
reference/mem53.py to match an intended Rust change. Cases stay the same; add new ones with --add '<json argv>'.

    python3 scripts/regen_golden.py                      # refresh outputs
    python3 scripts/regen_golden.py --add '["search", "--project", "shoplite", "--q", "찜 알림"]'
    cargo test --release --test golden

Needs only Python 3 (keyword path; MEM_NO_EMBED=1). `list` cases whose output depends on Python's hash seed
are rejected, because the Rust tie rule differs there (docs/DECISIONS.md D-12).
"""
import json, os, shutil, subprocess, sys, tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
FX = os.path.join(ROOT, "tests", "fixtures", "shoplite.db")
GOLD = os.path.join(ROOT, "tests", "fixtures", "golden.json")
PY = os.path.join(ROOT, "reference", "mem53.py")


def run(args, seed="0"):
    d = tempfile.mkdtemp(prefix="mem-golden-")
    try:
        shutil.copy(FX, os.path.join(d, "mem.db"))
        env = dict(os.environ, MEM_DB=os.path.join(d, "mem.db"), MEM_NO_EMBED="1", PYTHONHASHSEED=seed)
        p = subprocess.run([sys.executable, PY] + args, capture_output=True, text=True, env=env)
        return p.returncode, p.stdout
    finally:
        shutil.rmtree(d, ignore_errors=True)


def main():
    gold = json.load(open(GOLD, encoding="utf-8"))
    cases = [c["args"] for c in gold["cases"]]
    if len(sys.argv) == 3 and sys.argv[1] == "--add":
        cases.append(json.loads(sys.argv[2]))
    out = []
    for a in cases:
        code, so = run(a)
        if a and a[0] == "list" and any(run(a, s) != (code, so) for s in ("1", "2", "3")):
            sys.exit(f"hash-seed dependent in Python, not a stable golden case: {a}")
        out.append({"args": a, "code": code, "out": so})
    gold["cases"] = out
    with open(GOLD, "w", encoding="utf-8") as f:
        json.dump(gold, f, ensure_ascii=False, indent=1)
    print(f"{len(out)} cases written to {os.path.relpath(GOLD, ROOT)}")


if __name__ == "__main__":
    main()
