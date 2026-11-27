#!/usr/bin/env bash
# All checks. Uses temporary directories only; touches nothing in your real HOME.
set -euo pipefail
repo=$(cd "$(dirname "$0")" && pwd)

# The installer: unit tests and end-to-end tests against a temporary HOME.
cargo test --quiet --locked --manifest-path "$repo/Cargo.toml"
bash -n "$repo/bootstrap.sh"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
mkdir "$work/repo"
for item in home iterm2 .githooks README.md export-iterm.sh; do cp -R "$repo/$item" "$work/repo/"; done
fail() { echo "FAIL: $1" >&2; exit 1; }

# The whole tree, as it would be committed, passes the secret check.
cp -R "$repo/." "$work/all"
rm -rf "$work/all/.git" "$work/all/target"
(cd "$work/all" && git init -q . && git add -A && .githooks/pre-commit) || fail "hook rejects clean tree"

# Secret hook: clean tree passes, a planted token is blocked.
cd "$work/repo"
git init -q .
git add -A
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
