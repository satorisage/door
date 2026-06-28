#!/usr/bin/env bash
# One-shot helper: stop tracking the local governance files (.agent/ + CLAUDE.md) but
# KEEP them on disk, then push. Run this ON the machine you want to keep the files on
# — `git rm --cached` preserves the working-tree copies only where it runs. Afterwards,
# delete this script (the last echo tells you how).
#
# After this, those files are local-only: still on disk here for dev, gitignored, and
# never in future commits/pushes. (History still contains them — scrub with
# git-filter-repo later if you ever need a fully clean public history.)
set -euo pipefail

[ -d .git ] && [ -d .agent ] || {
    echo "Run from the door repo root (a clone that still has .git and .agent)."
    exit 1
}

# 1. Ignore them going forward (idempotent — won't duplicate lines).
for entry in '.agent/' 'CLAUDE.md'; do
    grep -qxF "$entry" .gitignore 2>/dev/null || echo "$entry" >> .gitignore
done

# 2. Stop tracking them — but the files stay on disk on THIS machine.
git rm -r --cached --ignore-unmatch .agent CLAUDE.md >/dev/null

# 3. Commit + push only if something actually changed.
git add .gitignore
if git diff --cached --quiet; then
    echo "Nothing to untrack — already local-only?"
    exit 0
fi
git commit -m "untrack local governance (.agent, CLAUDE.md); keep on disk"
git push

echo
echo "Done — .agent/ and CLAUDE.md are now local-only (still on disk, no longer tracked)."
echo "Clean up this helper next:"
echo "    git rm untrack-agent.sh && git commit -m 'remove helper' && git push"
