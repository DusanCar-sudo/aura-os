#!/bin/sh
# drive-ask.sh stub|fake|fake-q: ls /nope -> Ctrl+Shift+A -> answer split
#   fake: Enter runs the suggestion (touch /tmp/aura-suggest-ran); fake-q: q must not
# The answer window is started by `swaymsg exec`, i.e. with sway's PATH:
# "fake" restarts the headless sway with the fake aura sidecar on it.
mode=${1:-stub}
case $mode in fake*) export PATH=$HOME/aura-term-test/bin:$PATH ;; esac
rm -f /tmp/aura-suggest-ran
~/aura-term-test/hl.sh stop; ~/aura-term-test/hl.sh start
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
setsid -f aura-termd --server --log-level=info >$XDG_RUNTIME_DIR/termd.log 2>&1; sleep 0.5
cd ~; setsid -f aura-term -e bash --rcfile ~/aura-term-test/bashrc </dev/null >/dev/null 2>&1; sleep 1.5
t() { wtype -s 300 -d 15 "$1" -k Return; sleep ${2:-0.6}; }
k() { wtype -s 300 -M ctrl -M shift -k "$1" -m shift -m ctrl; sleep ${2:-0.5}; }
t "ls /nope"
ls $XDG_RUNTIME_DIR/aura-os/ask/ 2>/dev/null
k a 2.5
echo "windows: $(swaymsg -t get_tree | grep -o '"app_id": "[^"]*"' | tr '\n' ' ')"
echo "ask dir after: $(ls $XDG_RUNTIME_DIR/aura-os/ask/ 2>/dev/null | tr '\n' ' ')"
grim /tmp/ask1.png
if [ "$mode" = fake ]; then
    wtype -s 300 -k Return; sleep 1; grim /tmp/ask2.png     # Enter: run suggestion
    wtype -s 300 -k Return; sleep 1                         # Enter: close
elif [ "$mode" = fake-q ]; then
    wtype -s 300 q -k Return; sleep 1                       # q: close without running
else
    wtype -s 300 -k Return; sleep 1                         # Enter: close
fi
echo "suggestion ran: $([ -e /tmp/aura-suggest-ran ] && echo yes || echo no)"
echo "windows after: $(swaymsg -t get_tree | grep -o '"app_id": "[^"]*"' | tr '\n' ' ')"
grep -i "ask" $XDG_RUNTIME_DIR/termd.log | head -5
