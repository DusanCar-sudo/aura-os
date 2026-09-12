#!/bin/sh
# measure-launch.sh aura|foot [N]: launch-to-window latency in server mode.
# t0 = just before exec of the client; t1 = sway's "window new" IPC event,
# which sway sends when the view maps (first buffer committed).
case $1 in
aura) SRV=aura-termd; CLI=aura-term ;;
foot) SRV=foot;       CLI=footclient ;;
*) echo "usage: $0 aura|foot [N]" >&2; exit 2 ;;
esac
N=${2:-20}
~/aura-term-test/hl.sh stop; ~/aura-term-test/hl.sh start
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
$SRV --server --config=/dev/null --log-level=error >/dev/null 2>&1 &
srv=$!; sleep 1
ts=$XDG_RUNTIME_DIR/launch; : > $ts.start; : > $ts.new
swaymsg -r -m -t subscribe '["window"]' | while IFS= read -r ev; do
    case $ev in *'"change": "new"'*) date +%s%N >> $ts.new ;; esac
done &
mon=$!; sleep 0.5
$CLI true </dev/null >/dev/null 2>&1; sleep 1     # warm-up (fonts), not counted
: > $ts.new
for i in $(seq $N); do
    date +%s%N >> $ts.start
    $CLI sleep 0.4 </dev/null >/dev/null 2>&1 &
    sleep 1
done
kill $mon 2>/dev/null; ~/aura-term-test/hl.sh kill swaymsg
paste $ts.start $ts.new | awk '$2 > 0 { printf "%.2f\n", ($2 - $1) / 1e6 }' | sort -n > $ts.ms
awk -v who="$1" '{ v[NR] = $1; s += $1 } END {
  printf "%-5s launch->mapped  n=%d  min %.1f  median %.1f  mean %.1f  p95 %.1f  max %.1f ms\n",
    who, NR, v[1], v[int((NR+1)/2)], s/NR, v[int(NR*0.95)], v[NR] }' $ts.ms
kill $srv; wait $srv 2>/dev/null
~/aura-term-test/hl.sh stop
