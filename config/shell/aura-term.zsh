# aura-term shell integration for zsh.
# Source from ~/.zshrc:  . /usr/local/share/aura-term/shell/aura-term.zsh
#
# Emits OSC 133 command-block marks (A prompt, B command line, C output,
# D;exit finished) and OSC 7 (cwd), which aura-term uses for block
# status, jumping (Ctrl+Shift+Up/Down), copying (Ctrl+Shift+Y) and the
# pane status files. Other terminals ignore these sequences.

[[ -o interactive ]] || return 0
(( ${+__aura_term_loaded} )) && return 0
typeset -g __aura_term_loaded=1

__aura_term_osc7() {
    emulate -L zsh
    setopt extendedglob
    local LC_ALL=C
    printf '\e]7;file://%s%s\e\\' "$HOST" \
        "${PWD//(#m)[^a-zA-Z0-9\/._~-]/%${(l:2::0:)$(( [##16] #MATCH ))}}"
}

__aura_term_precmd() {
    local ret=$?
    if (( ${+__aura_term_ran} )); then
        printf '\e]133;D;%s\e\\' $ret
        unset __aura_term_ran
    fi
    __aura_term_osc7
    # Re-wrap PS1 if a prompt framework replaced it
    if [[ $PS1 != *'133;A'* ]]; then
        PS1=$'%{\e]133;A\e\\%}'"$PS1"$'%{\e]133;B\e\\%}'
    fi
    return $ret
}

__aura_term_preexec() {
    typeset -g __aura_term_ran=1
    printf '\e]133;C\e\\'
}

# First in line, so precmd sees the command's exit status
precmd_functions=(__aura_term_precmd ${precmd_functions[@]})
preexec_functions+=(__aura_term_preexec)

# :a — one-off Aura Code request. It may use normal Aura tools, but the
# explicit --no-session flag keeps this terminal helper out of chat history.
:a() {
    if (( $# == 0 )); then
        print -u2 'usage: :a <topic>     e.g.  :a install ripgrep'
        return 2
    fi
    if ! (( $+commands[aura] )); then
        print -u2 ':a: aura-code is not installed (install it with install/06-aura-code.sh)'
        return 127
    fi
    print -P '\n%F{cyan}Aura Code%f  %F{242}one-off request · no session%f\n'
    command aura --no-session --cwd "$PWD" "$*"
}
