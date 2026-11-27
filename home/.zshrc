# Tracked in a PUBLIC repo. Secrets and per-machine settings go in ~/.zshrc.local.
export PATH="$HOME/.local/bin:$PATH"

# bun (completions are loaded after compinit by source-terminal.zsh)
export BUN_INSTALL="$HOME/.bun"
export PATH="$BUN_INSTALL/bin:$PATH"

if (( $+commands[brew] )); then
  export CFLAGS="-I$(brew --prefix)/include"
  export LDFLAGS="-L$(brew --prefix)/lib"
fi

source "$HOME/.config/zsh/source-terminal.zsh"

export PATH="$HOME/flutter/bin:$PATH"
export PATH="/Library/TeX/texbin:$PATH"

if [[ -r "$HOME/.zshrc.local" ]]; then source "$HOME/.zshrc.local"; fi
