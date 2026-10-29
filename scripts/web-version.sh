#!/bin/sh
# Write web/version.json: the date and short hash of the last commit, shown
# on the page so a visitor can tell which build is running. "dirty" is set
# when the working tree had uncommitted changes (local builds only).
cd "$(dirname "$0")/.." || exit 1
date=$(git log -1 --format=%cd --date=format:'%Y-%m-%d %H:%M' 2>/dev/null)
hash=$(git log -1 --format=%h 2>/dev/null)
dirty=false
[ -n "$(git status --porcelain --untracked-files=no 2>/dev/null)" ] && dirty=true
printf '{"date": "%s", "commit": "%s", "dirty": %s}\n' "$date" "$hash" "$dirty" > web/version.json
