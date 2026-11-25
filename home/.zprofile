# Homebrew is not on PATH on a fresh Mac (Apple Silicon or Intel).
for brew_bin in /opt/homebrew/bin/brew /usr/local/bin/brew; do
  [[ -x $brew_bin ]] && { eval "$($brew_bin shellenv)"; break; }
done
unset brew_bin

# OrbStack command-line tools (no-op if not installed).
source ~/.orbstack/shell/init.zsh 2>/dev/null || :

export PATH="$HOME/.local/bin:$PATH"
