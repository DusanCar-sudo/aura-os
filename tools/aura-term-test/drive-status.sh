#!/bin/sh
# drive-status.sh: milestone 3 pane-status test in the private headless sway
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8 PATH=$HOME/aura-term-test/bin:$PATH
P=$XDG_RUNTIME_DIR/aura-os/panes
rm -f /tmp/aura-notify.log
rm -rf ~/aura-term-test/repo && git init -q -b main ~/aura-term-test/repo
pkill -x aura-term; pkill -x aura-termd; sleep 0.3
setsid -f aura-termd --server --config ~/aura-term-test/notify-test.ini --log-level=warning >$XDG_RUNTIME_DIR/termd.log 2>&1; sleep 0.5
cd ~/aura-term-test/repo
setsid -f aura-term -e bash --rcfile ~/aura-term-test/bashrc </dev/null >/dev/null 2>&1; sleep 1.5
t() { wtype -s 300 -d 15 "$1" -k Return; sleep ${2:-0.6}; }
s() { printf '%-34s ' "$1"; cat $P/*.json 2>/dev/null || echo "(no file)"; }
s "open in repo"
t "sleep 12; printf '\\a'" 2;           s "sleep 12 running"
sleep 11;                                s "after bell+D (12s)"
wtype -s 300 -k space; sleep 0.4;         s "key press"
wtype -k BackSpace; t "true";             s "quick command"
wtype -s 300 "zzzq"; wtype -k Tab; sleep 0.5; wtype -k ctrl+u 2>/dev/null; wtype -M ctrl -k u -m ctrl; s "bell at prompt"
t "claude" 1.2;                           s "claude started"
sleep 1.5;                                s "claude rang"
t "y" 0.8;                                s "answered"
sleep 2.5;                                s "claude exited"
wtype -s 300 -k space; wtype -k BackSpace; sleep 0.3
t "printf '\\033]9;Build finished\\a'; sleep 1" 0.5; s "OSC 9 plain"
sleep 1; t "printf '\\033]777;notify;Deploy;Approve release?\\a'; sleep 1" 0.5; s "OSC 777 question"
sleep 1; wtype -s 300 -k space; wtype -k BackSpace; t "cd /tmp" 0.5; s "cd /tmp"
ls $P; t "exit" 1;                        s "window closed"
echo "--- notifications:"; cat /tmp/aura-notify.log 2>/dev/null
echo "--- termd log:"; cat $XDG_RUNTIME_DIR/termd.log
