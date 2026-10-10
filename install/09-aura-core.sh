#!/usr/bin/env bash
# 09-aura-core — make Aura the core of the OS, not an app inside it.
# If /etc/aura/model.env exists (aura-os-brain ran during firstboot), this
# wires the brain into aura-code for the user: exports the endpoint/model/key
# from the user's profile (private, 0600), writes aura-code's global config,
# and arms the first-chat takeover so the first desktop login opens Aura.
# Without model.env the step is a no-op — the OS still boots, Aura just asks
# for her brain on first chat. Idempotent; safe to re-run.
set -euo pipefail

if [[ $EUID -ne 0 ]]; then
    exec sudo -E "$(readlink -f "$0")" "$@"
fi

[[ -n ${AURA_USER:-} ]] || { echo "09-aura-core: AURA_USER not set, skipping"; exit 0; }
user=$AURA_USER
home=$(getent passwd "$user" | cut -d: -f6)
repo=$(cd "$(dirname "$0")/.." && pwd)
envfile=/etc/aura/model.env

if [[ ! -r $envfile ]]; then
    echo "09-aura-core: no /etc/aura/model.env yet — user will meet aura-os-brain instead"
    exit 0
fi

# shellcheck disable=SC1090
source "$envfile"

# ── user-private env for every login shell (aura-term shells included) ─────
env_dir="$home/.config/aura-code"
install -d -o "$user" -g "$user" -m 700 "$env_dir"
cat > "$env_dir/model.env" <<EOF
# Written by install/09-aura-core.sh from aura-os-brain answers. Private.
export AURA_BASE_URL=$AURA_BASE_URL
export AURA_MODEL=$AURA_MODEL
export AURA_API_KEY=$AURA_API_KEY
EOF
chown "$user:$user" "$env_dir/model.env"; chmod 600 "$env_dir/model.env"

profile="$home/.profile"
if [[ -f $profile ]]; then
    grep -q 'aura-code/model.env' "$profile" || \
        printf '\n# Aura — her brain (endpoint/model/key)\n[ -r "$HOME/.config/aura-code/model.env" ] && . "$HOME/.config/aura-code/model.env"\n' >> "$profile"
else
    printf '# Aura — her brain (endpoint/model/key)\n[ -r "$HOME/.config/aura-code/model.env" ] && . "$HOME/.config/aura-code/model.env"\n' > "$profile"
fi
chown "$user:$user" "$profile"

# ── aura-code global config, so the picker and `aura` agree with the wizard ─
now=$(date -u +%FT%TZ)
cat > "$env_dir/config.json" <<EOF
{
  "provider": "OpenAI-compatible",
  "apiKeyEnv": "AURA_API_KEY",
  "defaultModel": "$AURA_MODEL",
  "baseUrl": "$AURA_BASE_URL",
  "createdAt": "$now",
  "updatedAt": "$now"
}
EOF
chown "$user:$user" "$env_dir/config.json"; chmod 600 "$env_dir/config.json"

# ── the first-chat takeover, armed for the first desktop login ─────────────
install -m 755 "$repo/bin/aura-os-first-chat" /usr/local/bin/aura-os-first-chat
cfg="$home/.config/sway/config.d"
install -d -o "$user" -g "$user" "$cfg"
install -o "$user" -g "$user" -m 644 \
    "$repo/config/sway/config.d/84-aura-first-chat.conf" \
    "$cfg/84-aura-first-chat.conf"

echo "09-aura-core: Aura is pre-configured (model $AURA_MODEL) and takes over at first login"
