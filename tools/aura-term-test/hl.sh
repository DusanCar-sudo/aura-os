#!/bin/sh
# Private headless sway for aura-term tests; never touches the real session.
# hl.sh start | stop | env
set -eu
RT=/run/user/$(id -u)/aura-term-test
case ${1:-} in
start)
    mkdir -p -m 700 "$RT"
    printf 'output HEADLESS-1 resolution 1280x720\ndefault_border pixel 1\nfont monospace 9\n' > "$RT/sway.conf"
    XDG_RUNTIME_DIR=$RT WLR_RENDERER=pixman WLR_BACKENDS=headless WLR_LIBINPUT_NO_DEVICES=1 \
        setsid -f sway -c "$RT/sway.conf" --unsupported-gpu >"$RT/sway.log" 2>&1
    for i in $(seq 50); do [ -S "$RT/wayland-1" ] && break; sleep 0.1; done
    sleep 0.5
    ;;
stop)
    pkill -f "sway -c $RT/sway.conf" || true
    sleep 0.5; rm -rf "$RT"
    ;;
env)
    echo "export XDG_RUNTIME_DIR=$RT WAYLAND_DISPLAY=wayland-1 SWAYSOCK=\$(ls $RT/sway-ipc.*.sock)"
    ;;
esac
