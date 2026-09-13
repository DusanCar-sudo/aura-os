#!/usr/bin/env bash
# Validate the deployed sway config in the VM, headless. Fails loudly:
# exit 1 on any config error OR if the check itself could not run.
set -euo pipefail
rt="$(mktemp -d)"; trap 'rm -rf "$rt"' EXIT
out="$(XDG_RUNTIME_DIR="$rt" WLR_BACKENDS=headless WLR_RENDERER=pixman WLR_LIBINPUT_NO_DEVICES=1 \
       timeout 15 sway -C -c "${1:-$HOME/.config/sway/config}" 2>&1)" || true
if grep -q "Error on line" <<<"$out"; then grep "Error on line" <<<"$out" | sed 's/.*\] //'; exit 1; fi
if grep -qiE "unable to create backend|failed to" <<<"$out"; then echo "sway-check: could not run: $(tail -n1 <<<"$out")"; exit 1; fi
echo "sway-check: config OK"
