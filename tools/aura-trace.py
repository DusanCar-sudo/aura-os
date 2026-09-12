#!/usr/bin/env python3
"""Summarize an Aura session on this repo: cost, context growth, and
rule-relevant actions from her fact log. Read-only.

    tools/aura-trace.py              latest session
    tools/aura-trace.py <session-id> a specific one
    tools/aura-trace.py --facts 40   also print the last 40 facts
"""
import json, re, sys
from pathlib import Path

SESS = Path.home() / ".aura/sessions/_run_media_dusan_DATA1_aura-os"

# Patterns that matter for AURA.md rules; each hit is worth a look.
FLAGS = {
    "raw sudo":        r"\bsudo (?!.*aura-os-)",
    "reboot/poweroff": r"\b(reboot|poweroff|shutdown)\b",
    "restart session": r"systemctl (restart|stop) (greetd|sddm|display-manager)",
    "rollback":        r"snapper .*rollback|rollback",
    "host path":       r"(/etc/|/usr/)(?!.*(guest|VM|vm))",
    "secret touch":    r"\.env\b|token|api[_-]?key",
    "web fetch":       r"https?://|GitHub API|fetched",
    "error/retry":     r"\b(error|failed|No permissions|404|retry|re-ran)\b",
}

def main():
    args = sys.argv[1:]
    nfacts = int(args[args.index("--facts") + 1]) if "--facts" in args else 0
    ids = [a for a in args if not a.startswith("--") and not a.isdigit()]
    files = sorted(SESS.glob("*.json"), key=lambda p: p.stat().st_mtime)
    main_files = [f for f in files if ".run" not in f.name]
    if not main_files:
        sys.exit(f"no sessions in {SESS}")
    sess = SESS / f"{ids[0]}.json" if ids else main_files[-1]
    d = json.loads(sess.read_text())
    u = d.get("usage", {})
    turns = u.get("turns", [])
    inp, cached = u.get("inputTokens", 0), u.get("cachedTokens", 0)
    peak = max((t["inputTokens"] for t in turns), default=0)

    print(f"session   {d['id']}  ({d.get('title','')[:60]})")
    print(f"turns     {len(turns)}   peak context {peak:,} tokens")
    print(f"tokens    in {inp:,}  out {u.get('outputTokens',0):,}  "
          f"cache {cached / inp:.0%}  cost ${u.get('costUsd',0):.2f}" if inp else "tokens    -")

    fl = SESS / sess.name.replace(".json", ".run.factlog.json")
    facts = json.loads(fl.read_text())["bullets"] if fl.exists() else []
    facts = [f if isinstance(f, str) else json.dumps(f) for f in facts]
    print(f"facts     {len(facts)}")
    print("\nflags (count, last example):")
    for name, pat in FLAGS.items():
        hits = [f for f in facts if re.search(pat, f, re.I)]
        if hits:
            print(f"  {name:16} {len(hits):3}  {hits[-1].strip('- ')[:110]}")
    if nfacts:
        print(f"\nlast {nfacts} facts:")
        for f in facts[-nfacts:]:
            print(" ", f.strip()[:160])

if __name__ == "__main__":
    main()
