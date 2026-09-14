#!/usr/bin/env bash
# 09-dusan-apps — Dusan's three companions from his GitHub, built from
# source into /usr/local/bin so they never fight pacman:
#   aura-mic      `dic`: speak, the words land in the focused window
#   aura-pulse    the cockpit: telemetry, AI clipboard vault, benchmarks
#   powerboard    TUI hardware/power/cost monitor (installed as aura-flux)
# Clones live in /opt/aura-apps. Safe to re-run: each build is skipped
# while its binary is newer than the clone; aura-pulse is pinned to a
# tag (AURA_PULSE_TAG), the others track master. Keys are never baked
# in: dic reads PARAKEET_BASE_URL or an API key from the environment.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

REPO="${REPO_DIR:-$(cd "$(dirname "$(readlink -f "$0")")/.." && pwd)}"
GH=https://github.com/DusanCar-sudo
opt=/opt/aura-apps
bin=/usr/local/bin
pulse_tag="${AURA_PULSE_TAG:-v0.4.3}"

# Build tools once: node (dic), rust + webkit (Tauri), cmake
# (powerboard). --needed keeps this a no-op later. alsa-utils (dic's
# recorder) comes with 01-packages now; wl-clipboard has always been.
pacman -S --needed --noconfirm git nodejs npm rust webkit2gtk-4.1 \
    cmake gcc pkgconf ydotool >/dev/null

clone() { # clone <repo> <ref> — fresh shallow, or refresh what is there
    local dir="$opt/$1" ref="$2"
    if [[ -d $dir/.git ]]; then
        git -C "$dir" fetch --depth 1 origin "$ref" >/dev/null
        git -C "$dir" reset --hard FETCH_HEAD >/dev/null
    else
        install -d "$opt"
        git clone --depth 1 -b "$ref" "$GH/$1" "$dir" >/dev/null
        echo "09-dusan-apps: cloned $1 ($ref)"
    fi
}

# ---- aura-mic (dic) ----------------------------------------------------
# npm-global like aura-code (06): its bins are aura-mic and dic. No git
# tags — the version comes from package.json, like npm would report it.
clone aura-mic master
mic_version="$(python3 -c "import json;print(json.load(open('$opt/aura-mic/package.json'))['version'])")"
have="$(npm ls -g --depth=0 --json 2>/dev/null | jq -r '.dependencies["aura-mic"].version // empty')"
if [[ $have == "$mic_version" ]]; then
    echo "09-dusan-apps: up to date: aura-mic $have (dic)"
else
    (cd "$opt/aura-mic" && npm ci --no-fund --no-audit >/dev/null && npm run build >/dev/null)
    npm install -g --no-fund --no-audit "$opt/aura-mic" >/dev/null
    echo "09-dusan-apps: installed aura-mic $mic_version (was: ${have:-none})"
    echo "09-dusan-apps: dic needs a provider: PARAKEET_BASE_URL, XIAOMI_API_KEY, OPENAI_API_KEY or GROQ_API_KEY"
fi

# ---- aura-pulse --------------------------------------------------------
# Tauri 2 app, built from the tag: vite builds the frontend, then the
# local tauri CLI compiles the Rust release binary. `--no-bundle`: no
# installer packages, Arch does not need them. (There is no tauri:build
# npm script at v0.4.3 — the CLI is called directly.)
clone aura-pulse "$pulse_tag"
if [[ ! -x $bin/aura-pulse ]] || [[ -n $(find "$opt/aura-pulse/src" "$opt/aura-pulse/src-tauri/src" \
        -newer "$bin/aura-pulse" -print -quit 2>/dev/null) ]]; then
    echo "09-dusan-apps: building aura-pulse $pulse_tag — cargo release, a few minutes"
    # tauri build runs `npm run build` (vite) itself: beforeBuildCommand.
    (cd "$opt/aura-pulse" && npm ci --no-fund --no-audit >/dev/null \
        && npm run tauri -- build --no-bundle >/dev/null)
    install -m 755 "$opt/aura-pulse/src-tauri/target/release/aura-pulse" "$bin/aura-pulse"
    echo "09-dusan-apps: installed $bin/aura-pulse"
else
    echo "09-dusan-apps: up to date: $bin/aura-pulse"
fi

# ---- powerboard --------------------------------------------------------
# ftxui is not packaged for Arch: a small FetchContent block pins it at
# configure time (tools/powerboard-fetch-ftxui.patch, kept next to this
# step). Without it CMake stops at `find_package(ftxui CONFIG REQUIRED)`.
clone powerboard master
if [[ ! -x $bin/powerboard ]] || [[ -n $(find "$opt/powerboard/src" \
        -newer "$bin/powerboard" -print -quit 2>/dev/null) ]]; then
    if ! grep -q FetchContent "$opt/powerboard/CMakeLists.txt"; then
        git -C "$opt/powerboard" apply "$REPO/tools/powerboard-fetch-ftxui.patch"
        echo "09-dusan-apps: patched powerboard to fetch ftxui"
    fi
    echo "09-dusan-apps: building powerboard"
    # -DCMAKE_INSTALL_PREFIX=/usr/local: the project installs with
    # GNUInstallDirs, so the binary lands in /usr/local/bin (our PATH).
    cmake -S "$opt/powerboard" -B "$opt/powerboard/build" \
        -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=/usr/local >/dev/null
    cmake --build "$opt/powerboard/build" -j"$(nproc)" >/dev/null
    cmake --install "$opt/powerboard/build" >/dev/null
    echo "09-dusan-apps: installed $bin/powerboard (+ the Powerboard launcher)"
else
    echo "09-dusan-apps: up to date: $bin/powerboard"
fi

# ---- ydotoold (dic typing into other windows) ---------------------------
# Wayland has no global injection: ydotoold owns /dev/uinput and dic
# talks to its socket. Arch ships it as a user unit, started in the
# user's own systemd instance (lingering not required: the desktop
# session keeps it alive while logged in).
for u in "${AURA_USER:?AURA_USER is not set — run this through install.sh}"; do
    if systemctl --machine="$u@" --user enable --now ydotool.service 2>/dev/null; then
        echo "09-dusan-apps: enabled ydotool.service for $u"
    else
        echo "09-dusan-apps: ydotool.service left off — it starts at next login" >&2
    fi
done
