#!/usr/bin/env bash
# Usage: ./install.sh              install packages, link configs, set up plugins
#        ./install.sh --link-only  link configs only
#        ./install.sh --no-root    Linux: skip system packages and the login shell change
# Works on macOS (Homebrew) and on Ubuntu, Debian and Rocky Linux (or other apt/dnf distros).
# Safe to re-run. Replaced files are moved to ~/.dotfiles-backup/<timestamp>/.
set -euo pipefail

link_only=0 no_root=0
case "${1:-}" in
  "") ;;
  --link-only) link_only=1 ;;
  --no-root) no_root=1 ;;
  *) echo "usage: $0 [--link-only | --no-root]" >&2; exit 2 ;;
esac

repo=$(cd "$(dirname "$0")" && pwd)
backup="$HOME/.dotfiles-backup/$(date +%Y%m%d-%H%M%S)-$$"
iterm_dst="$HOME/Library/Application Support/iTerm2/DynamicProfiles/source-terminal.json"
os=$(uname)
local_bin="$HOME/.local/bin"
local_share="$HOME/.local/share"

save() { # move an existing file out of the way, keeping its relative path
  [[ -e $1 || -L $1 ]] || return 0
  local to="$backup/${1#"$HOME"/}"
  mkdir -p "$(dirname "$to")"
  mv "$1" "$to"
  echo "backed up $1 -> $to"
}

as_root() {
  if (( EUID == 0 )); then "$@"
  elif command -v sudo >/dev/null; then sudo "$@"
  else echo "need root or sudo for: $*  (or re-run with --no-root)" >&2; exit 1
  fi
}

# Print the package names in packages/<name>.txt for each name given: one per line, # comments.
# They reach a root package manager, so reject anything that is not a plain name. That covers
# options, and a trailing "-", which makes apt-get install remove the package.
read_packages() {
  local list file line
  for list in "$@"; do
    file="$repo/packages/$list.txt"
    [[ -r $file ]] || { echo "missing package list: $file" >&2; return 1; }
    while read -r line || [[ -n $line ]]; do
      line=${line%%#*}
      line=${line%"${line##*[![:space:]]}"}
      [[ -n $line ]] || continue
      [[ $line =~ ^[A-Za-z0-9]([A-Za-z0-9+._-]*[A-Za-z0-9+])?$ ]] ||
        { echo "bad package name in $file: $line" >&2; return 1; }
      echo "$line"
    done < "$file"
  done
}

# System packages for Linux, named in packages/. Tools that distros ship too old (fzf,
# starship, zoxide, antidote) are installed per user under ~/.local instead, see linux_user_tools.
linux_packages() {
  local distro names pkgs
  distro=$(. /etc/os-release && echo " ${ID:-} ${ID_LIKE:-} ")
  case $distro in
    *" debian "* | *" ubuntu "*)
      local apt=(env DEBIAN_FRONTEND=noninteractive apt-get -yq)
      names=$(read_packages common apt) || exit 1
      pkgs=($names) # validated names: no spaces or glob characters
      as_root "${apt[@]}" update
      as_root "${apt[@]}" install "${pkgs[@]}"
      if ! command -v gh >/dev/null; then
        local key=/etc/apt/keyrings/githubcli-archive-keyring.gpg
        as_root mkdir -p -m 755 /etc/apt/keyrings
        curl -fsSL https://cli.github.com/packages/githubcli-archive-keyring.gpg | as_root tee "$key" >/dev/null
        as_root chmod go+r "$key"
        echo "deb [arch=$(dpkg --print-architecture) signed-by=$key] https://cli.github.com/packages stable main" |
          as_root tee /etc/apt/sources.list.d/github-cli.list >/dev/null
        as_root "${apt[@]}" update
        as_root "${apt[@]}" install gh
      fi
      ;;
    *" rhel "* | *" fedora "* | *" centos "*)
      names=$(read_packages common dnf) || exit 1
      pkgs=($names)
      # htop lives in EPEL. Fedora has no epel-release and does not need it.
      [[ $distro == *" fedora "* && $distro != *" rhel "* ]] || as_root dnf install -y epel-release
      # Rocky ships curl-minimal, which conflicts with the curl package: only add curl if absent.
      command -v curl >/dev/null || pkgs+=(curl)
      as_root dnf install -y "${pkgs[@]}"
      if ! command -v gh >/dev/null; then
        curl -fsSL https://cli.github.com/packages/rpm/gh-cli.repo | as_root tee /etc/yum.repos.d/gh-cli.repo >/dev/null
        as_root dnf install -y gh
      fi
      ;;
    *) echo "unsupported Linux distribution:$distro(need apt or dnf)" >&2; exit 1 ;;
  esac
}

linux_user_tools() {
  local missing=()
  for c in zsh git curl tar gcc; do command -v "$c" >/dev/null || missing+=("$c"); done
  (( ${#missing[@]} == 0 )) || { echo "missing: ${missing[*]}. Install them or run without --no-root." >&2; exit 1; }
  mkdir -p "$local_bin" "$local_share"
  export PATH="$local_bin:$PATH"

  [[ -d $local_share/antidote ]] || git clone -q --depth 1 https://github.com/mattmc3/antidote "$local_share/antidote"
  # `fzf --zsh` needs fzf 0.48+, newer than Debian 12 and Ubuntu 22.04 ship.
  if [[ ! -x $local_share/fzf/bin/fzf ]]; then
    [[ -d $local_share/fzf ]] || git clone -q --depth 1 https://github.com/junegunn/fzf "$local_share/fzf"
    "$local_share/fzf/install" --bin
  fi
  ln -sf "$local_share/fzf/bin/fzf" "$local_bin/fzf"
  command -v starship >/dev/null || curl -fsSL https://starship.rs/install.sh | sh -s -- -y -b "$local_bin"
  command -v zoxide >/dev/null || curl -fsSL https://raw.githubusercontent.com/ajeetdsouza/zoxide/main/install.sh | sh -s -- --bin-dir "$local_bin"
}

# Rust for zsh-fast-alias-tips, whose crate uses edition 2024 (rustc 1.85+).
ensure_cargo() {
  export PATH="$HOME/.cargo/bin:$PATH"
  local v
  v=$(cargo --version 2>/dev/null | awk '{print $2}') || true
  if [[ -n $v ]] && printf '1.85.0\n%s\n' "$v" | sort -VC; then return; fi
  if [[ $os == Darwin ]]; then
    brew install rust
  else
    curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path
  fi
}

antidote_zsh() {
  if command -v brew >/dev/null && [[ -r $(brew --prefix)/opt/antidote/share/antidote/antidote.zsh ]]; then
    echo "$(brew --prefix)/opt/antidote/share/antidote/antidote.zsh"
  else
    echo "$local_share/antidote/antidote.zsh"
  fi
}

if (( ! link_only )); then
  case $os in
    Darwin)
      (( ! no_root )) || { echo "--no-root is for Linux" >&2; exit 2; }
      for b in /opt/homebrew/bin/brew /usr/local/bin/brew; do
        if [[ -x $b ]]; then eval "$("$b" shellenv)"; break; fi
      done
      command -v brew >/dev/null || { echo "Install Homebrew first: https://brew.sh" >&2; exit 1; }
      brew bundle --file="$repo/Brewfile"
      [[ -d /Applications/iTerm.app ]] || brew install --cask iterm2
      ;;
    Linux)
      (( no_root )) || linux_packages
      linux_user_tools
      ;;
    *) echo "unsupported OS: $os" >&2; exit 1 ;;
  esac
fi

# ~/.gitconfig is a real file that includes the tracked one, never a link to it. Tools write
# with `git config --global`, and actions/checkout copies a link as a link, so even its
# temporary writes would land in this public repo.
shared="$repo/home/.gitconfig"
gitconfig="$HOME/.gitconfig"
marker='# Managed by dotfiles install.sh. Untracked: git config --global writes here.'
if ! { [[ -f $gitconfig && ! -L $gitconfig ]] && git config -f "$gitconfig" --get-all include.path | grep -xF "$shared" >/dev/null; }; then
  # Keep an existing git identity: the old ~/.gitconfig becomes the local override.
  # Never copy a file this script wrote, which could make the local file include itself.
  if [[ -f $gitconfig && ! -L $gitconfig && ! -e $HOME/.gitconfig.local ]] && ! grep -qxF "$marker" "$gitconfig"; then
    cp "$gitconfig" "$HOME/.gitconfig.local"
    echo "copied old ~/.gitconfig to ~/.gitconfig.local"
  fi
  save "$gitconfig"
  printf '%s\n' "$marker" > "$gitconfig"
  git config -f "$gitconfig" include.path "$shared"
fi

# Every other file under home/ is linked to the same path under $HOME.
while IFS= read -r -d '' src; do
  dst="$HOME/${src#"$repo"/home/}"
  [[ -L $dst && $(readlink "$dst") == "$src" ]] && continue
  save "$dst"
  mkdir -p "$(dirname "$dst")"
  ln -s "$src" "$dst"
done < <(find "$repo/home" -type f ! -name .DS_Store ! -path "$shared" -print0)

if [[ ! -e $HOME/.zshrc.local ]]; then
  (umask 077; cp "$repo/zshrc.local.example" "$HOME/.zshrc.local")
fi

# Copied, not linked: iTerm2 may rewrite it. Backups must stay outside DynamicProfiles.
if [[ $os == Darwin ]] && ! cmp -s "$repo/iterm2/profile.json" "$iterm_dst"; then
  save "$iterm_dst"
  mkdir -p "$(dirname "$iterm_dst")"
  cp "$repo/iterm2/profile.json" "$iterm_dst"
fi

# Block commits that look like they contain secrets. The hook is a copy, so that
# checking out another branch cannot change what runs on your machine.
if [[ -e $repo/.git ]]; then
  if [[ $(git -C "$repo" config --local core.hooksPath || true) == .githooks ]]; then
    git -C "$repo" config --local --unset core.hooksPath
  fi
  hooks="$(cd "$repo" && cd "$(git rev-parse --git-common-dir)" && pwd)/hooks"
  mkdir -p "$hooks"
  install -m 755 "$repo/.githooks/pre-commit" "$hooks/pre-commit"
  if git -C "$repo" config core.hooksPath >/dev/null; then
    echo "warning: core.hooksPath is set, so the secret check will not run" >&2
  fi
fi

if (( ! link_only )); then
  # First interactive shell makes antidote clone the plugins.
  zsh -ilc true
  # zsh-fast-alias-tips needs a compiled helper.
  tips=$(zsh -c 'source "$1"; antidote path sei40kr/zsh-fast-alias-tips' zsh "$(antidote_zsh)")
  if [[ ! -x $tips/target/release/alias-matcher ]]; then
    ensure_cargo
    cargo build --release --manifest-path "$tips/Cargo.toml"
  fi

  # Linux servers usually start users in bash.
  if [[ $os == Linux ]] && (( ! no_root )); then
    zsh_path=$(command -v zsh)
    me=$(id -un)
    if [[ $(getent passwd "$me" | cut -d: -f7) != "$zsh_path" ]]; then
      as_root usermod -s "$zsh_path" "$me" || echo "warning: could not change login shell; run: chsh -s $zsh_path" >&2
    fi
  fi
fi

if [[ $os == Darwin ]]; then
  terminal_step='3. iTerm2 > Settings > Profiles > "Source Mac" > Other Actions > Set as Default.
  4. Open a new terminal window.'
else
  terminal_step='3. Log out and back in (or run: exec zsh -l). The prompt uses no special
     font; the font is set on the machine you connect from.'
fi
cat <<MSG

Done. Next:
  1. Put secrets and machine-specific settings in ~/.zshrc.local
     (if an old .zshrc was backed up, copy what you still need from it).
  2. Set your git identity in ~/.gitconfig.local (see README).
  $terminal_step
MSG
