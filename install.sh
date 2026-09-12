#!/usr/bin/env bash
# Aura OS installer — entry point.
#
# Runs the numbered steps in install/ in order. Every step is safe to
# re-run, so this script is safe to re-run too.
#
# Usage:
#   ./install.sh          run everything
#   ./install.sh 02 04    run only steps 02.. and 04..
set -euo pipefail

REPO_DIR="$(cd "$(dirname "$(readlink -f "${BASH_SOURCE[0]}")")" && pwd)"
export REPO_DIR

# The steps change the system, so make sure we are root. Keep the real
# invoking user around: it is who the desktop gets installed for.
if [[ $EUID -ne 0 ]]; then
    export AURA_USER="${AURA_USER:-$USER}"
    exec sudo -E bash "$REPO_DIR/install.sh" "$@"
fi

export AURA_USER="${AURA_USER:-${SUDO_USER:-$(getent passwd 1000 | cut -d: -f1)}}"
if [[ -z $AURA_USER || $AURA_USER == root ]]; then
    echo "install.sh: cannot work out the desktop user; set AURA_USER" >&2
    exit 1
fi

# Collect the steps: everything, or only what was asked for.
steps=()
if [[ $# -gt 0 ]]; then
    for want in "$@"; do
        matched=("$REPO_DIR"/install/"$want"*.sh)
        if [[ -e ${matched[0]} ]]; then
            steps+=("${matched[@]}")
        else
            echo "install.sh: no step matching '$want' in install/" >&2
            exit 1
        fi
    done
else
    for step in "$REPO_DIR"/install/[0-9][0-9]-*.sh; do
        if [[ -e $step ]]; then
            steps+=("$step")
        fi
    done
fi

for step in "${steps[@]}"; do
    echo
    echo "==> $(basename "$step")"
    bash "$step"
done

echo
echo "install.sh: done"
