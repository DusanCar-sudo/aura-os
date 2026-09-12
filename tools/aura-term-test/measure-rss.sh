#!/bin/sh
# measure-rss.sh aura|foot [tiled|ws]: server-mode memory with 0..5 idle
# windows, in a fresh private headless sway (1280x720), both with
# --config=/dev/null. tiled: all windows split one screen; ws: each window
# full-size on its own workspace (worst case for buffer memory).
# Prints server RSS/PSS (kB) and the sum of client RSS after each window.
case $1 in
aura) SRV=aura-termd; CLI=aura-term ;;
foot) SRV=foot;       CLI=footclient ;;
*) echo "usage: $0 aura|foot [tiled|ws]" >&2; exit 2 ;;
esac
layout=${2:-tiled}
~/aura-term-test/hl.sh stop; ~/aura-term-test/hl.sh start
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
$SRV --server --config=/dev/null --log-level=error >/dev/null 2>&1 &
srv=$!; sleep 1
kb() { awk -v k="$2:" '$1==k {print $2}' /proc/$1/smaps_rollup; }
mine() { for p in $(pgrep -x $1); do tr '\0' '\n' < /proc/$p/environ | grep -qx "XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR" && echo $p; done; }
clients() { s=0; for p in $(mine $CLI); do s=$((s + $(kb $p Rss))); done; echo $s; }
printf '%-8s %9s %9s %11s\n' windows srv_rss srv_pss clients_rss
printf '%-8s %9s %9s %11s\n' 0 $(kb $srv Rss) $(kb $srv Pss) 0
for n in 1 2 3 4 5; do
    [ $layout = ws ] && swaymsg -q workspace $n
    setsid -f $CLI bash --norc -i </dev/null >/dev/null 2>&1
    sleep 2
    printf '%-8s %9s %9s %11s\n' $n $(kb $srv Rss) $(kb $srv Pss) $(clients)
done
# idle CPU of the server over 10 s with 5 windows (event-driven: should be 0)
t0=$(awk '{print $14+$15}' /proc/$srv/stat); sleep 10; t1=$(awk '{print $14+$15}' /proc/$srv/stat)
echo "idle cpu ticks in 10s: $((t1 - t0))"
~/aura-term-test/hl.sh kill $CLI; kill $srv; wait $srv 2>/dev/null
~/aura-term-test/hl.sh stop
