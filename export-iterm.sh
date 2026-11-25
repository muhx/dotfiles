#!/usr/bin/env bash
# Save the current default iTerm2 profile into iterm2/profile.json.
# Review `git diff` before committing: profiles can hold commands, triggers and hosts.
set -euo pipefail
repo=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
defaults export com.googlecode.iterm2 - | /usr/bin/python3 -I -c '
import json, plistlib, sys
d = plistlib.loads(sys.stdin.buffer.read())
p = next(p for p in d["New Bookmarks"] if p["Guid"] == d["Default Bookmark Guid"])
# Drop machine-specific keys.
for k in ("Dynamic Profile Filename", "Working Directory", "Tags"):
    p.pop(k, None)
p["Custom Directory"] = "No"  # start in the home directory
print(json.dumps({"Profiles": [p]}, indent=2, sort_keys=True))
' > "$tmp"
cp "$tmp" "$repo/iterm2/profile.json"
echo "wrote iterm2/profile.json"
