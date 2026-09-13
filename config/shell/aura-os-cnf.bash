# aura-os-cnf.bash — "command not found" that points somewhere.
# Aura OS installs only what a developer actually uses; everything else
# is one command away. bash calls this for any unknown command.
# Needs the pacman files database (install/01-packages.sh runs pacman -Fy).

command_not_found_handle() {
    local cmd=$1 pkgs=""
    # never recurse: a missing pacman would call this handler again
    [[ -x /usr/bin/pacman ]] && \
        pkgs="$(/usr/bin/pacman -Fq "/usr/bin/$cmd" 2>/dev/null | sed 's|.*/||' | sort -u | head -n 3)"
    if [[ -n $pkgs ]]; then
        printf 'aura: %s is not installed — it comes with: %s\n' "$cmd" "$(echo $pkgs)" >&2
        printf '      aura-os-install %s\n' "${pkgs%%$'\n'*}" >&2
    else
        printf 'aura: %s: no package has it. Ask Aura for a replacement:\n' "$cmd" >&2
        printf '      aura "what replaces %s on Arch?"\n' "$cmd" >&2
    fi
    return 127
}
