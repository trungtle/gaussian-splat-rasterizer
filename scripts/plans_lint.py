#!/usr/bin/env python3
"""Validate roadmap task headers and print the next actionable tasks.

Copy this file into the target project's scripts/ directory. It expects task
blocks in <roadmap-dir>/*.md, where <roadmap-dir> is auto-discovered from the
candidates below (relative to the repo root, assumed to be the parent of the
directory holding this script) or passed explicitly with --dir.

Task block format (documented in the project's ROADMAP.md):

    <!--task
    id: AREA-001
    status: todo|doing|blocked|done
    epic: <slug>
    deps: [ID, ID]
    cl: <commit hash, required when status: done>
    blocked_by: <text, required when status: blocked>
    -->
    ### AREA-001 — Title

Invariants enforced:
  - ids unique, match ^[A-Z]+-[A-Z0-9]+$
  - status in {todo, doing, blocked, done}
  - done    => cl non-empty
  - blocked => blocked_by non-empty
  - every deps entry references an id that exists somewhere in the roadmap

Exit codes: 0 = clean, 1 = violations found, 2 = no roadmap files found.
"""
import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
# Ordered by preference; first existing non-empty dir wins.
CANDIDATE_DIRS = [
    "docs/private/roadmap",
    "docs/roadmap",
    "doc/roadmap",
]
ID_RE = re.compile(r"^[A-Z]+-[A-Z0-9]+$")
VALID_STATUS = {"todo", "doing", "blocked", "done"}


def source_files(explicit_dir):
    dirs = [Path(explicit_dir)] if explicit_dir else [ROOT / d for d in CANDIDATE_DIRS]
    for d in dirs:
        if d.is_dir():
            files = sorted(d.glob("*.md"))
            if files:
                return files
    return []


def parse(text, fname):
    tasks = []
    for block in re.findall(r"<!--task\s*(.*?)-->", text, re.DOTALL):
        fields = {"_file": fname}
        for line in block.strip().splitlines():
            if ":" in line:
                k, _, v = line.partition(":")
                fields[k.strip()] = v.strip()
        raw_deps = fields.get("deps", "").strip().strip("[]").strip()
        fields["deps"] = [d.strip() for d in raw_deps.split(",") if d.strip()]
        tasks.append(fields)
    return tasks


def main():
    ap = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    ap.add_argument("--dir", help="roadmap directory (overrides auto-discovery)")
    args = ap.parse_args()

    files = source_files(args.dir)
    if not files:
        tried = args.dir or ", ".join(CANDIDATE_DIRS)
        print(f"error: no roadmap files found (tried: {tried})", file=sys.stderr)
        return 2

    tasks = []
    for f in files:
        tasks.extend(parse(f.read_text(encoding="utf-8"), f.name))

    ids = [t.get("id", "") for t in tasks]
    errors = []
    seen = set()
    for t in tasks:
        tid = t.get("id", "")
        where = t["_file"]
        if not ID_RE.match(tid):
            errors.append(f"{where}: bad or missing id: {tid!r}")
            continue
        if tid in seen:
            errors.append(f"{where}: duplicate id: {tid}")
        seen.add(tid)
        status = t.get("status", "")
        if status not in VALID_STATUS:
            errors.append(f"{tid} ({where}): invalid status {status!r}")
        if status == "done" and not t.get("cl"):
            errors.append(f"{tid} ({where}): status done but no cl")
        if status == "blocked" and not t.get("blocked_by"):
            errors.append(f"{tid} ({where}): status blocked but no blocked_by")
        for d in t["deps"]:
            if d not in ids:
                errors.append(f"{tid} ({where}): dep {d} does not exist")

    done = {t["id"] for t in tasks if t.get("status") == "done"}
    actionable = [
        t["id"] for t in tasks
        if t.get("status") == "todo" and all(d in done for d in t["deps"])
    ]

    by_status = {s: sum(1 for t in tasks if t.get("status") == s) for s in VALID_STATUS}
    print(f"{len(tasks)} tasks in {len(files)} file(s): "
          + ", ".join(f"{k}={by_status[k]}" for k in ("todo", "doing", "blocked", "done")))
    if actionable:
        print("next actionable: " + ", ".join(actionable[:5])
              + (" ..." if len(actionable) > 5 else ""))

    if errors:
        print(f"\n{len(errors)} violation(s):", file=sys.stderr)
        for e in errors:
            print("  - " + e, file=sys.stderr)
        return 1
    print("OK — all task headers valid")
    return 0


if __name__ == "__main__":
    sys.exit(main())
