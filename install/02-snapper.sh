#!/usr/bin/env bash
# 02-snapper — snapper on /, so every change can be rolled back.
# Creates the root config once and leaves a "base-clean" snapshot when
# the system has none. Safe to re-run.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

if [[ $(findmnt -no FSTYPE /) != btrfs ]]; then
    echo "02-snapper: / is not btrfs — skipping snapper setup" >&2
    exit 0
fi

if pacman -Qq snapper >/dev/null 2>&1; then
    echo "02-snapper: snapper already installed"
else
    # firstboot can race the network (DNS once died mid-install): retry
    # a few times before giving up, so snapshots are never silently skipped
    ok=0
    for i in 1 2 3 4 5; do
        if pacman -S --needed --noconfirm snapper; then ok=1; break; fi
        echo "02-snapper: attempt $i failed — waiting for the network" >&2
        sleep 10
    done
    if (( ! ok )); then
        echo "02-snapper: could not install snapper after 5 tries" >&2
        exit 1
    fi
    echo "02-snapper: installed snapper"
fi

if [[ -e /etc/snapper/configs/root ]]; then
    echo "02-snapper: snapper config 'root' already exists"
else
    snapper -c root create-config /
    echo "02-snapper: created snapper config 'root'"
fi

# "current" is always listed as 0 — a real snapshot has a number >= 1
if snapper -c root list | tail -n +3 | grep -Eq '^[[:space:]]*[1-9]'; then
    echo "02-snapper: snapshots exist — keeping them"
else
    snapper -c root create -c number -d "base-clean"
    echo "02-snapper: created first snapshot: base-clean"
fi
