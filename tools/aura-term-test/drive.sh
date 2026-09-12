#!/bin/sh
# drive.sh <shell-cmd...>: fresh aura-term window in the headless sway, run the block test
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
pkill -x aura-term; pkill -x aura-termd; sleep 0.3
pgrep -x aura-termd >/dev/null || { setsid -f aura-termd --server --log-level=warning >$XDG_RUNTIME_DIR/termd.log 2>&1; sleep 0.5; }
setsid -f aura-term -e "$@" </dev/null >/dev/null 2>&1; sleep 1.5
t() { wtype -s 300 -d 15 "$1" -k Return; sleep ${2:-0.6}; }
t true; t "ls /nope"; t "sleep 1.3; false" 2; t "(exit 42)"; t ""; t "echo hello; echo world"
grim /tmp/shot.png
# Ctrl+Shift+Y -> clipboard
wtype -M ctrl -M shift -k y -m shift -m ctrl; sleep 0.4
printf 'clipboard: [%s]\n' "$(wl-paste -n 2>&1)"
