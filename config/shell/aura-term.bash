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
