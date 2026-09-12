#!/bin/sh
# drive-split.sh: Ctrl+Alt+arrows splits in the private headless sway.
# A in /tmp; Right -> B; (in B) Down -> C; (in C) Left -> D; (in D) Up -> E.
~/aura-term-test/hl.sh stop; ~/aura-term-test/hl.sh start
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
setsid -f aura-termd --server --log-level=warning >$XDG_RUNTIME_DIR/termd.log 2>&1; sleep 0.5
cd /tmp; setsid -f aura-term bash --norc -i </dev/null >/dev/null 2>&1; sleep 1.5
s() { wtype -s 300 -M ctrl -M alt -k "$1" -m alt -m ctrl; sleep 1.2; }
s Right; s Down; s Left; s Up
swaymsg -t get_tree | python3 -c '
import json, sys
def walk(n, depth=0):
    if n.get("app_id"):
        r = n["rect"]
        print("  %-16s x=%4d y=%4d %4dx%-4d %s" % (n.get("tag") or "-", r["x"], r["y"], r["width"], r["height"], "FOCUSED" if n["focused"] else ""))
    for c in n.get("nodes", []): walk(c, depth + 1)
walk(json.load(sys.stdin))'
echo "pane cwds: $(cat $XDG_RUNTIME_DIR/aura-os/panes/*.json | grep -o '"cwd":"[^"]*"' | sort | uniq -c | tr '\n' ' ')"
grim /tmp/split.png
grep -v "keyboard_leave\|toplevel-icon" $XDG_RUNTIME_DIR/termd.log | head -5
~/aura-term-test/hl.sh stop
