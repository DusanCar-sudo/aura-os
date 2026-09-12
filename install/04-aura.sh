#!/usr/bin/env bash
# 04-aura — install the aura-os-* commands into /usr/local/bin.
# Safe to re-run: only files that differ are copied.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"

for src in "$REPO"/bin/aura-os-*; do
    dst="/usr/local/bin/$(basename "$src")"
    if [[ ! -e $dst ]] || ! cmp -s "$src" "$dst"; then
        install -m 755 "$src" "$dst"
        echo "04-aura: installed $dst"
    else
        echo "04-aura: up to date: $dst"
    fi
done
