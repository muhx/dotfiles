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

# A scan that cannot run must block the commit.
git rm -q --cached planted
if GIT_INDEX_FILE="$work/repo/README.md" .githooks/pre-commit 2>/dev/null; then fail "hook passed on scan error"; fi
# JSON-style secrets and any email are blocked.
for sample in '"API_%s": "abcdefgh12345678"' 'someone@%s.io'; do
  # shellcheck disable=SC2059
  printf "$sample\n" KEY > planted
  git add planted
  if .githooks/pre-commit 2>/dev/null; then fail "hook missed: $sample"; fi
  git rm -qf planted
done

# The installed hook is a copy, and the old hooksPath setting is removed.
git config core.hooksPath .githooks
run
cmp -s .githooks/pre-commit .git/hooks/pre-commit && [[ -x .git/hooks/pre-commit ]] || fail "hook not installed"
if git config --local core.hooksPath >/dev/null; then fail "hooksPath still set"; fi

# Export keeps only allowlisted settings. `defaults` is stubbed with a fake profile.
mkdir "$work/bin"
/usr/bin/python3 -I -c '
import plistlib, sys
p = {"Guid": "g", "Name": "n", "Command": "ssh private-host", "Initial Text": "private-text",
     "Some Future Key": "private-value", "Columns": 80, "Triggers": [{"parameter": "private-trigger"}]}
sys.stdout.buffer.write(plistlib.dumps({"Default Bookmark Guid": "g", "New Bookmarks": [p]}))
' > "$work/fake.plist"
printf '#!/bin/sh\ncat "%s"\n' "$work/fake.plist" > "$work/bin/defaults"
chmod +x "$work/bin/defaults"
PATH="$work/bin:$PATH" ./export-iterm.sh >/dev/null 2>&1
if grep -q private iterm2/profile.json; then fail "export leaked a private value"; fi
grep -q '"Columns": 80' iterm2/profile.json || fail "export lost a setting"
PATH="$work/bin:$PATH" ./export-iterm.sh --include-bindings >/dev/null 2>&1
grep -q private-trigger iterm2/profile.json || fail "--include-bindings ignored"

for f in home/.zshenv home/.zprofile home/.zshrc home/.config/zsh/source-terminal.zsh; do zsh -n "$f"; done
echo PASS
