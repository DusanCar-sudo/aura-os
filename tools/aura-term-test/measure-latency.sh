#!/bin/sh
# measure-latency.sh aura|foot cat|bash [N]: key-to-frame latency.
# One window (server mode, --config=/dev/null) runs `cat` (kernel echo) or
# `bash --norc` (readline echo). wtype types N chars 120 ms apart; the
# server logs Wayland traffic (WAYLAND_DEBUG, us timestamps). Sample =
# key press event received -> next wl_surface.commit of the window, i.e.
# the frame that shows the glyph. Compositor scan-out is not included and
# is the same for both terminals.
case $1 in
aura) SRV=aura-termd; CLI=aura-term ;;
foot) SRV=foot;       CLI=footclient ;;
*) echo "usage: $0 aura|foot cat|bash [N]" >&2; exit 2 ;;
esac
mode=${2:-cat}; N=${3:-200}
~/aura-term-test/hl.sh stop; ~/aura-term-test/hl.sh start
eval $(~/aura-term-test/hl.sh env)
export LANG=C.UTF-8
log=$XDG_RUNTIME_DIR/wl.log
WAYLAND_DEBUG=client $SRV --server --config=/dev/null --log-level=error >/dev/null 2>$log &
srv=$!; sleep 1
if [ $mode = cat ]; then setsid -f $CLI cat </dev/null >/dev/null 2>&1
else setsid -f $CLI bash --norc -i </dev/null >/dev/null 2>&1; fi
sleep 2
# Type N printable chars in ONE wtype (one virtual keyboard), 120 ms apart
chars=$(head -c $((N * 20)) /dev/urandom | tr -dc 'a-z' | head -c $N)
wtype -s 500 -d 120 "$chars"
sleep 1
~/aura-term-test/hl.sh kill $CLI; kill $srv; wait $srv 2>/dev/null
awk '
function t(s,  a) { gsub(/[\[\]]/, "", s); split(s, a, ":"); return (a[1]*3600 + a[2]*60 + a[3]) * 1000 }
/wl_keyboard#[0-9]+\.key\(.*, 1\)$/ { if (!pending) { k = t($1); pending = 1 } next }
/-> wl_surface#3\.commit\(\)/ { if (pending) { printf "%.3f\n", t($1) - k; pending = 0 } }
' $log | sort -n > $XDG_RUNTIME_DIR/lat.txt
n=$(wc -l < $XDG_RUNTIME_DIR/lat.txt)
awk -v n=$n -v who="$1/$mode" '
{ v[NR] = $1; s += $1 }
END {
  if (n == 0) { print who ": no samples"; exit 1 }
  printf "%-10s n=%d  min %.2f  median %.2f  mean %.2f  p95 %.2f  p99 %.2f  max %.2f ms\n", who, n,
    v[1], v[int((n+1)/2)], s/n, v[int(n*0.95)], v[int(n*0.99)], v[n]
}' $XDG_RUNTIME_DIR/lat.txt
~/aura-term-test/hl.sh stop
