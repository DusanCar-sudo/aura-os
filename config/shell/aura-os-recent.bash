# Aura OS: log commands and folders for the sidebar's "recent" list.
# Source from ~/.bashrc. Commands starting with a space are not logged
# (like HISTCONTROL=ignorespace); secrets are masked by aura-os-recent.
[[ $- == *i* ]] || return 0
[[ -n ${__aura_recent_loaded-} ]] && return 0
__aura_recent_loaded=1
__aura_recent_pwd=$PWD
__aura_recent_last=""

__aura_recent_hook() {
    local line; line="$(HISTTIMEFORMAT= history 1)"
    line="${line#*[0-9]  }"
    if [[ -n $line && $line != "$__aura_recent_last" && $line != " "* ]]; then
        __aura_recent_last=$line
        case $line in
            cd|cd\ *|ls|ls\ *|clear|exit|pwd) ;;   # folders cover these
            *) (aura-os-recent log '$' "$line" &) 2>/dev/null ;;
        esac
    fi
    if [[ $PWD != "$__aura_recent_pwd" ]]; then
        __aura_recent_pwd=$PWD
        [[ $PWD != "$HOME" ]] && (aura-os-recent log d "$PWD" &) 2>/dev/null
    fi
}
PROMPT_COMMAND="__aura_recent_hook${PROMPT_COMMAND:+;$PROMPT_COMMAND}"
