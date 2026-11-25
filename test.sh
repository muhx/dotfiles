#!/usr/bin/env bash
# Self-check against a throwaway HOME. Touches nothing in your real one.
set -euo pipefail
repo=$(cd "$(dirname "$0")" && pwd)
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
cp -R "$repo/." "$work/repo"
rm -rf "$work/repo/.git"
mkdir "$work/home"
printf 'old zshrc\n' > "$work/home/.zshrc"
printf '[user]\n\tname = Old Name\n' > "$work/home/.gitconfig"
fail() { echo "FAIL: $1" >&2; exit 1; }
run() { HOME="$work/home" "$work/repo/install.sh" --link-only >/dev/null; }

run
[[ -L $work/home/.zshrc && -L $work/home/.config/starship.toml ]] || fail "not linked"
grep -q 'old zshrc' "$work"/home/.dotfiles-backup/*/.zshrc || fail "no backup"
[[ $(HOME="$work/home" git config --global --includes user.name) == "Old Name" ]] || fail "git identity lost"
[[ $(stat -f %Lp "$work/home/.zshrc.local") == 600 ]] || fail ".zshrc.local mode"
echo 'keep me' > "$work/home/.zshrc.local"
count=$(find "$work/home/.dotfiles-backup" -type f -o -type l | wc -l)
run
[[ $(find "$work/home/.dotfiles-backup" -type f -o -type l | wc -l) == "$count" ]] || fail "second run not idempotent"
grep -q 'keep me' "$work/home/.zshrc.local" || fail ".zshrc.local overwritten"

# Secret hook: clean tree passes, a planted token is blocked.
cd "$work/repo"
git init -q .
git add -A
.githooks/pre-commit || fail "hook rejects clean tree"
printf 'export DEMO_API_%s=abcdefgh12345678\n' KEY > planted
git add planted
if .githooks/pre-commit 2>/dev/null; then fail "hook missed a secret"; fi

for f in home/.zshenv home/.zprofile home/.zshrc home/.config/zsh/source-terminal.zsh; do zsh -n "$f"; done
echo PASS
