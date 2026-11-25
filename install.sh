#!/usr/bin/env bash
# Usage: ./install.sh              install packages, link configs, set up plugins
#        ./install.sh --link-only  link configs only
# Safe to re-run. Replaced files are moved to ~/.dotfiles-backup/<timestamp>/.
set -euo pipefail

case "${1:-}" in
  "") link_only=0 ;;
  --link-only) link_only=1 ;;
  *) echo "usage: $0 [--link-only]" >&2; exit 2 ;;
esac

repo=$(cd "$(dirname "$0")" && pwd)
backup="$HOME/.dotfiles-backup/$(date +%Y%m%d-%H%M%S)-$$"
iterm_dst="$HOME/Library/Application Support/iTerm2/DynamicProfiles/source-terminal.json"

save() { # move an existing file out of the way, keeping its relative path
  [[ -e $1 || -L $1 ]] || return 0
  local to="$backup/${1#"$HOME"/}"
  mkdir -p "$(dirname "$to")"
  mv "$1" "$to"
  echo "backed up $1 -> $to"
}

if (( ! link_only )); then
  [[ $(uname) == Darwin ]] || { echo "macOS only" >&2; exit 1; }
  for b in /opt/homebrew/bin/brew /usr/local/bin/brew; do
    if [[ -x $b ]]; then eval "$("$b" shellenv)"; break; fi
  done
  command -v brew >/dev/null || { echo "Install Homebrew first: https://brew.sh" >&2; exit 1; }
  brew bundle --file="$repo/Brewfile"
  [[ -d /Applications/iTerm.app ]] || brew install --cask iterm2
fi

# Keep an existing git identity: the old ~/.gitconfig becomes the local override.
if [[ -f $HOME/.gitconfig && ! -L $HOME/.gitconfig && ! -e $HOME/.gitconfig.local ]]; then
  cp "$HOME/.gitconfig" "$HOME/.gitconfig.local"
  echo "copied old ~/.gitconfig to ~/.gitconfig.local"
fi

# Every file under home/ is linked to the same path under $HOME.
while IFS= read -r -d '' src; do
  dst="$HOME/${src#"$repo"/home/}"
  [[ -L $dst && $(readlink "$dst") == "$src" ]] && continue
  save "$dst"
  mkdir -p "$(dirname "$dst")"
  ln -s "$src" "$dst"
done < <(find "$repo/home" -type f ! -name .DS_Store -print0)

if [[ ! -e $HOME/.zshrc.local ]]; then
  (umask 077; cp "$repo/zshrc.local.example" "$HOME/.zshrc.local")
fi

# Copied, not linked: iTerm2 may rewrite it. Backups must stay outside DynamicProfiles.
if ! cmp -s "$repo/iterm2/profile.json" "$iterm_dst"; then
  save "$iterm_dst"
  mkdir -p "$(dirname "$iterm_dst")"
  cp "$repo/iterm2/profile.json" "$iterm_dst"
fi

# Block commits that look like they contain secrets.
if [[ -e $repo/.git ]]; then git -C "$repo" config core.hooksPath .githooks; fi

if (( ! link_only )); then
  # First interactive shell makes antidote clone the plugins.
  zsh -ilc true
  # zsh-fast-alias-tips needs a compiled helper.
  tips=$(zsh -c 'source "$(brew --prefix)/opt/antidote/share/antidote/antidote.zsh"; antidote path sei40kr/zsh-fast-alias-tips')
  if [[ ! -x $tips/target/release/alias-matcher ]]; then
    export PATH="$HOME/.cargo/bin:$PATH"
    command -v cargo >/dev/null || brew install rust
    cargo build --release --manifest-path "$tips/Cargo.toml"
  fi
fi

cat <<MSG

Done. Next:
  1. Put secrets and machine-specific settings in ~/.zshrc.local
     (if an old .zshrc was backed up, copy what you still need from it).
  2. Set your git identity in ~/.gitconfig.local (see README).
  3. iTerm2 > Settings > Profiles > "Source Mac" > Other Actions > Set as Default.
  4. Open a new terminal window.
MSG
