#!/usr/bin/env bash
# Save the current default iTerm2 profile into iterm2/profile.json.
# Only allowlisted settings are exported, because this repo is public.
# Triggers and Keyboard Map can run commands or send text, so the committed values
# are kept unless you pass --include-bindings. Review `git diff` before committing.
set -euo pipefail
case "${1:-}" in
  "" | --include-bindings) ;;
  *) echo "usage: $0 [--include-bindings]" >&2; exit 2 ;;
esac
repo=$(cd "$(dirname "$0")" && pwd)
tmp=$(mktemp)
trap 'rm -f "$tmp"' EXIT
defaults export com.googlecode.iterm2 - | /usr/bin/python3 -I -c '
import json, plistlib, sys
committed_path, flag = sys.argv[1], sys.argv[2]
d = plistlib.loads(sys.stdin.buffer.read())
live = next(p for p in d["New Bookmarks"] if p["Guid"] == d["Default Bookmark Guid"])
with open(committed_path) as f:
    committed = json.load(f)["Profiles"][0]

TEXT = {"Custom Locale", "Default Bookmark", "Description", "Guid", "Jobs to Ignore",
        "Name", "Non Ascii Font", "Normal Font", "Terminal Type"}
FIXED = {"Command": "/bin/zsh -l", "Custom Command": "Yes", "Custom Directory": "No",
         "Background Image Location": "", "Shortcut": ""}
BINDINGS = ("Triggers", "Keyboard Map")
COLOR = {"Red Component", "Green Component", "Blue Component", "Alpha Component", "Color Space"}

out, dropped = dict(FIXED), []
for key, value in live.items():
    if key in FIXED or key in BINDINGS:
        continue
    is_color = isinstance(value, dict) and "Red Component" in value and set(value) <= COLOR
    if key in TEXT or isinstance(value, (bool, int, float)) or is_color:
        out[key] = value
    else:
        dropped.append(key)
source = live if flag else committed
for key in BINDINGS:
    if key in source:
        out[key] = source[key]
    if not flag and live.get(key) != committed.get(key):
        print(f"kept committed {key}; review yours, then use --include-bindings", file=sys.stderr)
if dropped:
    print("not exported: " + ", ".join(sorted(dropped)), file=sys.stderr)
print(json.dumps({"Profiles": [out]}, indent=2, sort_keys=True))
' "$repo/iterm2/profile.json" "${1:-}" > "$tmp"
cp "$tmp" "$repo/iterm2/profile.json"
echo "wrote iterm2/profile.json"
