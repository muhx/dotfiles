#!/usr/bin/env bash
# Build rstuck from this clone, install it to ~/.local/bin and run `rstuck apply`.
# Usage: ./bootstrap.sh [--link-only | --no-root] [--dry-run]
# This is only what a machine needs before rstuck can run. Everything else is in rstuck.
# --link-only, --no-root and --dry-run never install system packages.
set -euo pipefail

usage="usage: $0 [--link-only | --no-root] [--dry-run]"
may_install=1 dry_run=0
for arg in "$@"; do
  case $arg in
    --link-only | --no-root) may_install=0 ;;
    --dry-run) may_install=0 dry_run=1 ;;
    -h | --help) echo "$usage"; exit 0 ;;
    *) echo "$usage" >&2; exit 2 ;;
  esac
done

repo=$(cd "$(dirname "$0")" && pwd)
os=$(uname)

as_root() {
  if (( EUID == 0 )); then "$@"
  elif command -v sudo >/dev/null; then sudo "$@"
  else echo "need root or sudo for: $*" >&2; exit 1
  fi
}

# gcc without the C library headers cannot link, so try it.
can_link() {
  local dir ok=0
  dir=$(mktemp -d)
  echo 'int main(void){return 0;}' | cc -x c - -o "$dir/out" >/dev/null 2>&1 || ok=1
  rm -rf "$dir"
  return $ok
}

have_prerequisites() { command -v git >/dev/null && command -v curl >/dev/null && can_link; }

if [[ $os == Darwin ]]; then
  for b in /opt/homebrew/bin/brew /usr/local/bin/brew; do
    if [[ -x $b ]]; then eval "$("$b" shellenv)"; break; fi
  done
  command -v brew >/dev/null || { echo "Install Homebrew first: https://brew.sh" >&2; exit 1; }
elif ! have_prerequisites; then
  (( may_install )) || { echo "need git, curl and a working C compiler first" >&2; exit 1; }
  if command -v apt-get >/dev/null; then
    as_root env DEBIAN_FRONTEND=noninteractive apt-get -yq update
    as_root env DEBIAN_FRONTEND=noninteractive apt-get -yq install git curl ca-certificates gcc libc6-dev
  elif command -v dnf >/dev/null; then
    # Rocky ships curl-minimal, which conflicts with the curl package: only add curl if absent.
    pkgs=(git gcc)
    command -v curl >/dev/null || pkgs+=(curl)
    as_root dnf install -y "${pkgs[@]}"
  else
    echo "unsupported Linux distribution (need apt or dnf)" >&2; exit 1
  fi
fi

# rstuck needs Rust 1.85+ (rust-version in Cargo.toml).
export PATH="$HOME/.cargo/bin:$PATH"
cargo_ok() {
  local v
  v=$(cargo --version 2>/dev/null | awk '{print $2}') || return 1
  [[ $v =~ ^([0-9]+)\.([0-9]+) ]] || return 1
  (( BASH_REMATCH[1] > 1 || BASH_REMATCH[2] >= 85 ))
}
if ! cargo_ok; then
  (( ! dry_run )) || { echo "need cargo 1.85+ first; run without --dry-run to install it" >&2; exit 1; }
  if [[ $os == Darwin ]]; then
    brew install rust
  else
    curl -fsSL https://sh.rustup.rs | sh -s -- -y -q --profile minimal --no-modify-path
  fi
  hash -r
  cargo_ok || { echo "cargo 1.85+ is still not on PATH (an older one may come first)" >&2; exit 1; }
fi

if (( dry_run )); then
  # Build inside the clone only; do not install the binary.
  exec cargo run --quiet --release --locked --manifest-path "$repo/Cargo.toml" -- --repo "$repo" apply "$@"
fi
cargo install --quiet --locked --force --path "$repo" --root "$HOME/.local"
exec "$HOME/.local/bin/rstuck" --repo "$repo" apply "$@"
