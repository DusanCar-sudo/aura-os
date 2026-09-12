#!/bin/sh
# Private headless sway for aura-term tests; never touches the real session.
# hl.sh start | stop | env | kill NAME...   (HL_RES=WxH, default 1280x720; HL_EXTRA=file)
#
# The VM is shared with Aura's real session: never pkill by name. "kill"
# only signals processes whose environment has this test runtime dir.
set -eu
RT=/run/user/$(id -u)/aura-term-test

kill_mine() {
    for name in "$@"; do
        for pid in $(pgrep -x "$name" || true); do
            if tr '\0' '\n' < /proc/$pid/environ 2>/dev/null | grep -qx "XDG_RUNTIME_DIR=$RT"; then
                kill "$pid" 2>/dev/null || true
            fi
        done
    done
}

case ${1:-} in
start)
    mkdir -p -m 700 "$RT"
    printf "output HEADLESS-1 resolution ${HL_RES:-1280x720}\\ndefault_border pixel 1\\nfont monospace 9\\n" > "$RT/sway.conf"
    # HL_EXTRA: extra sway config appended (e.g. a config.d drop-in under test)
    [ -n "${HL_EXTRA:-}" ] && cat "$HL_EXTRA" >> "$RT/sway.conf"
    XDG_RUNTIME_DIR=$RT WLR_RENDERER=pixman WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
        setsid -f sway -c "$RT/sway.conf" --unsupported-gpu >"$RT/sway.log" 2>&1
    for i in $(seq 50); do [ -S "$RT/wayland-1" ] && break; sleep 0.1; done
    sleep 0.5
    ;;
stop)
    kill_mine aura-term aura-termd footclient foot wtype swaymsg
    pkill -f "sway -c $RT/sway.conf" || true
    sleep 0.5; rm -rf "$RT"
    ;;
kill)
    shift
    kill_mine "$@"
    ;;
env)
    echo "export XDG_RUNTIME_DIR=$RT WAYLAND_DISPLAY=wayland-1 SWAYSOCK=\$(ls $RT/sway-ipc.*.sock)"
    ;;
esac
