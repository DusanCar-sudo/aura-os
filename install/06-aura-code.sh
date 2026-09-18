#!/usr/bin/env bash
# 06-aura-code — Aura herself: the aura CLI (aura-code on npm) plus Node.
# Pinned; bump AURA_CODE_VERSION to update. The provider and API key are
# per user: run `aura setup` after install (never baked into the image).
# node-pty's build script is not run: only `aura serve`'s web terminal
# needs it, and it loads lazily. Safe to re-run.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

want="${AURA_CODE_VERSION:-0.19.0}"
pacman -S --needed --noconfirm nodejs npm >/dev/null

have="$(npm ls -g --depth=0 --json 2>/dev/null | jq -r '.dependencies["aura-code"].version // empty')"
if [[ $have == "$want" ]]; then
    echo "06-aura-code: up to date: aura-code $have"
else
    npm install -g --no-fund --no-audit "aura-code@$want" >/dev/null
    echo "06-aura-code: installed aura-code $want (was: ${have:-none})"
fi
echo "06-aura-code: next, as the user: aura setup"
