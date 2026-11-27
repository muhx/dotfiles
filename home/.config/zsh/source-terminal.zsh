# Source-Mac terminal setup. Loaded once per interactive shell.
[[ -o interactive ]] || return
(( ${+_SOURCE_TERMINAL_LOADED} )) && return
typeset -g _SOURCE_TERMINAL_LOADED=1

typeset -gU path fpath
path=("$HOME/.local/bin" $path)
export EDITOR=vim
zmodload zsh/complist

# Starship owns the prompt; plugins remain in the supplied order.
ZSH_THEME=""
zstyle ':omz:update' mode disabled
zstyle ':zephyr:plugin:prompt' skip yes
zstyle ':zephyr:plugin:history' histfile "$HOME/.zsh_history"
zstyle ':zephyr:plugin:history' histsize 10000
zstyle ':zephyr:plugin:history' savehist 10000

# Homebrew's antidote on macOS, a per-user clone on Linux (see install.sh).
() {
  local f
  for f in ${commands[brew]:+"$(brew --prefix)/opt/antidote/share/antidote/antidote.zsh"} \
           "$HOME/.local/share/antidote/antidote.zsh"; do
    [[ -r $f ]] && { source "$f"; return; }
  done
  print -u2 "antidote not found; run ~/dotfiles/install.sh"
}
# Used by `conditional:` in .zsh_plugins.txt.
is-macos() { [[ $OSTYPE == darwin* ]] }
(( ${+functions[antidote]} )) && antidote load "$HOME/.zsh_plugins.txt"

# belak completion calls ez-compinit's wrapper once. Do not let use-omz
# initialize it again on the first prompt. Replay its earlier compdefs.
add-zsh-hook -d precmd ensure-compinit-during-precmd
if (( ${+functions[compinit_deferred]} )); then
  unfunction compinit_deferred
fi
() {
  local cmpcmd
  for cmpcmd in "${__compcmd_queue[@]}"; do eval "$cmpcmd"; done
}
unset __compcmd_queue
unfunction queued_compcmd 2>/dev/null

# Existing Bun installation: register its completions after compinit.
[[ -s "$HOME/.bun/_bun" ]] && source "$HOME/.bun/_bun"
source <(fzf --zsh)

bindkey -M menuselect '^I' menu-complete
bindkey '^[[A' history-substring-search-up
bindkey '^[[B' history-substring-search-down

HISTFILE="$HOME/.zsh_history"
HISTSIZE=10000
SAVEHIST=10000
unsetopt INC_APPEND_HISTORY INC_APPEND_HISTORY_TIME
setopt HIST_IGNORE_DUPS SHARE_HISTORY APPEND_HISTORY
export EDITOR=vim
alias ll='ls -lAh'
alias gs='git status'
alias gd='git diff'
alias gl='git log --oneline --graph --decorate -20'
unalias gsd 2>/dev/null

export STARSHIP_CONFIG="$HOME/.config/starship.toml"
eval "$(starship init zsh)"
