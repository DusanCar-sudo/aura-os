# aura-term shell integration for bash (>= 5.1).
# Source from ~/.bashrc:  . /usr/local/share/aura-term/shell/aura-term.bash
#
# Emits OSC 133 command-block marks (A prompt, B command line, C output,
# D;exit finished) and OSC 7 (cwd), which aura-term uses for block
# status, jumping (Ctrl+Shift+Up/Down), copying (Ctrl+Shift+Y) and the
# pane status files. Other terminals ignore these sequences.

[[ $- == *i* ]] || return 0
[[ -n ${__aura_term_loaded-} ]] && return 0
__aura_term_loaded=1

__aura_term_osc7() {
    local LC_ALL=C s=$PWD out= c i
    for (( i = 0; i < ${#s}; i++ )); do
        c=${s:i:1}
        case $c in
            [a-zA-Z0-9/._~-]) out+=$c ;;
            *) printf -v c '%%%02X' "'$c"; out+=$c ;;
        esac
    done
    printf '\e]7;file://%s%s\e\\' "${HOSTNAME-}" "$out"
}

__aura_term_precmd() {
    local ret=$?
    # PS0 sets __aura_term_ran when a command actually ran
    if [[ -n ${__aura_term_ran+x} ]]; then
        printf '\e]133;D;%s\e\\' "$ret"
        unset __aura_term_ran
    fi
    __aura_term_osc7
    # Re-wrap PS1 if a prompt framework replaced it
    if [[ $PS1 != *'133;A'* ]]; then
        PS1='\[\e]133;A\e\\\]'"$PS1"'\[\e]133;B\e\\\]'
    fi
    return $ret
}

# ${__aura_term_ran=} assigns in the current shell and expands to nothing
PS0='\[\e]133;C\e\\\]${__aura_term_ran=}'"${PS0-}"
PROMPT_COMMAND=(__aura_term_precmd "${PROMPT_COMMAND[@]}")

# Terminal-only theme (aura-term-theme): re-apply the saved choice in
# every new aura-term window, on top of the system theme.
if [[ ${TERMINFO-} == */aura-term/* ]] && command -v aura-term-theme >/dev/null 2>&1; then
    aura-term-theme --apply-saved
fi

# The small Aura banner atop a new aura-term window: who we are, what we
# do. Once per window (not in shells started inside it), only in
# aura-term, never when bash runs a command. AURA_TERM_BANNER=0 turns it off.
if [[ ( ${TERMINFO-} == */aura-term/* || ${TERM-} == foot ) && -z ${AURA_TERM_BANNER_SHOWN-} && ${AURA_TERM_BANNER:-1} != 0 ]]; then
    export AURA_TERM_BANNER_SHOWN=1
    for __aura_logo in aura-os-logo "$HOME/.local/bin/aura-os-logo"; do
        if command -v "$__aura_logo" >/dev/null 2>&1; then
            "$__aura_logo" banner 2>/dev/null
            break
        fi
    done
    unset __aura_logo
fi

# a — ask Aura in plain words, answered right here in the terminal:
#   a how do i change themes      (or: a: how do i change themes)
# Read-only: Aura never runs anything herself. If she suggests one
# command, Enter runs it, q skips it.
a() {
    if (($# == 0)); then
        echo 'usage: a <question>     e.g.  a how do i change themes' >&2
        return 2
    fi
    local c f ctx=
    for c in $( { compgen -c aura-os-; compgen -c aura-term-; } | grep -v -e '\.bak$' -e '^aura-term-ask$' | sort -u); do
        f=$(command -v "$c") || continue
        ctx+="$c: $(sed -n '2,5{/^#/p}' "$f" 2>/dev/null | sed 's/^# \{0,1\}//' | tr '\n' ' ')"$'\n'
        ctx+="$(grep -E "^#[[:space:]]+$c( |$)" "$f" 2>/dev/null | sed 's/^#[[:space:]]*/    /')"$'\n'
    done
    mkdir -p "${XDG_RUNTIME_DIR:-/tmp}/aura-os/ask"
    jq -n --arg q "$*" --arg c "$ctx" --arg d "$PWD" '{question:$q,context:$c,cwd:$d}' |
        aura-term-ask --inline
}

# :a — a one-off Aura Code request. Unlike `a`, this is allowed to use Aura's
# normal tools (including installing something) and explicitly disables
# session persistence. Aura still asks for approval where her policy requires.
:a() {
    if (($# == 0)); then
        echo 'usage: :a <topic>     e.g.  :a install ripgrep' >&2
        return 2
    fi
    if ! command -v aura >/dev/null 2>&1; then
        echo ':a: aura-code is not installed (install it with install/06-aura-code.sh)' >&2
        return 127
    fi
    printf '\n\033[1;38;2;0;229;208mAura Code\033[0m  \033[2mone-off request · no session\033[0m\n\n'
    aura --no-session --cwd "$PWD" "$*"
}
# Keep the old spelling as a compatibility alias.
a:() { a "$@"; }
