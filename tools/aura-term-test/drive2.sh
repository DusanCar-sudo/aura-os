#!/bin/sh
# drive2.sh: jumps, wrapped command, reflow (run after drive.sh, same window)
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
t() { wtype -s 150 -d 15 "$1" -k Return; sleep ${2:-0.6}; }
k() { wtype -s 150 -M ctrl -M shift -k "$1" -m shift -m ctrl; sleep 0.4; }
t "seq 100" 1; t "echo $(printf 'x%.0s' $(seq 190)); false" 1; t "seq 100" 1
k Up; k Up; grim /tmp/j1.png            # two blocks up: second 'seq 100' prompt... 
k Up; grim /tmp/j2.png                  # three up
k Down; k Down; k Down; grim /tmp/j3.png  # back to bottom
setsid -f aura-term -e sleep 30 </dev/null >/dev/null 2>&1; sleep 1.5; grim /tmp/j4.png   # split -> reflow
swaymsg -q '[app_id=aura-term] focus'; pkill -f "^aura-term -e sleep 30$"; sleep 1; grim /tmp/j5.png  # un-split -> reflow back
