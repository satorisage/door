#!/usr/bin/env bash
# door release helper — run from the repo root on a machine that has:
#   - the gh CLI authed as the repo owner            (the public flip + tag push)
#   - an AUR account with your SSH key uploaded       (the AUR push)
#   - pacman-contrib + base-devel                     (updpkgsums, makepkg)
#
# It (1) makes the GitHub repo public once, (2) tags v<version> and pushes it,
# (2b) publishes a GitHub Release with notes, and (3) builds + publishes the AUR
# package against that tag's source tarball.
#
# Publish-safety invariants (so a stale/mismatched package can never reach users):
#   - Cargo.toml version MUST equal PKGBUILD pkgver           (preflight, below)
#   - updpkgsums MUST pin a real sha256 (never 'SKIP')        (step 3)
#   - the committed .SRCINFO MUST match ${VER}-${REL}         (step 3)
# aurweb indexes .SRCINFO, not PKGBUILD — a drift there silently ships the wrong
# version, which is exactly what these asserts prevent.
#
# Heads-up: making the repo public also exposes the git *history*, which still
# contains earlier governance commits even though that dir is now untracked going
# forward. If you need a fully clean history, scrub it with git-filter-repo first.
set -euo pipefail

REPO="satorisage/door"
VER="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
REL="$(grep -m1 '^pkgrel=' PKGBUILD | cut -d= -f2)"
echo "==> door v${VER}-${REL}  (${REPO})"

# --- 0. Preflight: Cargo.toml and PKGBUILD versions must agree --------------------
# The one that bit us: VER (message/tag) came from Cargo.toml while the published
# content came from PKGBUILD. If they drift, you publish a commit labelled with the
# new version whose .SRCINFO holds the old one. Refuse to start on a mismatch.
PKGVER="$(grep -m1 '^pkgver=' PKGBUILD | cut -d= -f2)"
if [ "$VER" != "$PKGVER" ]; then
    echo "!! version drift: Cargo.toml=${VER} but PKGBUILD pkgver=${PKGVER}." >&2
    echo "   Bump both together (they must match). Aborting before anything ships." >&2
    exit 1
fi

# --- 1. Make the repo public (one-time; no-op if it already is) -------------------
vis="$(gh repo view "$REPO" --json visibility -q .visibility)"
if [ "$vis" = "PUBLIC" ]; then
    echo "    repo already public"
else
    read -rp "    Make ${REPO} PUBLIC? (hard to fully undo) [y/N] " ok
    [ "$ok" = "y" ] || { echo "    aborted"; exit 1; }
    gh repo edit "$REPO" --visibility public --accept-visibility-change-consequences
fi

# --- 2. Push the source + tag the release -----------------------------------------
git push origin HEAD:master
if git rev-parse "v${VER}" >/dev/null 2>&1; then
    echo "==> tag v${VER} already exists, skipping"
else
    git tag -a "v${VER}" -m "door v${VER}"
    git push origin "v${VER}"
fi

# --- 2b. Publish a GitHub Release with notes --------------------------------------
# The AUR only pulls the tag's source tarball — it carries no release notes. The
# GitHub Release is where notes live (and what an AUR user sees on clicking
# through). Notes come from dist/release-notes/v<ver>.md if present, else GitHub
# auto-generates them from the commits since the last tag. Idempotent.
NOTES="dist/release-notes/v${VER}.md"
if gh release view "v${VER}" >/dev/null 2>&1; then
    echo "==> GitHub Release v${VER} already exists, skipping"
elif [ -f "$NOTES" ]; then
    gh release create "v${VER}" --title "door v${VER}" --notes-file "$NOTES"
else
    echo "    (no $NOTES — auto-generating notes from commits)"
    gh release create "v${VER}" --title "door v${VER}" --generate-notes
fi

# --- 3. Build + publish the AUR package -------------------------------------------
WORK="$(mktemp -d)/door"
echo "==> staging AUR package in ${WORK}"
git clone "ssh://aur@aur.archlinux.org/door.git" "$WORK"   # empty clone for a new pkg is fine
cp PKGBUILD door.install "$WORK/"
cd "$WORK"

updpkgsums                              # pin sha256 to the just-published tarball
if grep -q "sha256sums=('SKIP')" PKGBUILD; then
    echo "!! updpkgsums left the checksum as 'SKIP' (tarball not fetched?). Aborting." >&2
    exit 1
fi

makepkg --printsrcinfo > .SRCINFO       # regenerate the file aurweb actually indexes
SRCVER="$(grep -m1 'pkgver =' .SRCINFO | sed -E 's/.*=[[:space:]]*//')"
SRCREL="$(grep -m1 'pkgrel =' .SRCINFO | sed -E 's/.*=[[:space:]]*//')"
if [ "$SRCVER" != "$VER" ] || [ "$SRCREL" != "$REL" ]; then
    echo "!! .SRCINFO says ${SRCVER}-${SRCREL} but releasing ${VER}-${REL} — stale .SRCINFO." >&2
    echo "   Refusing to push a mismatched package. Aborting." >&2
    exit 1
fi

makepkg -f --nocheck                    # local build sanity-check before publishing

git add PKGBUILD .SRCINFO door.install
git commit -m "door ${VER}-${REL}"
git push origin master

# --- 3b. Confirm aurweb reindexed (so 'it published but paru shows old' is answered) ---
echo "==> pushed door ${VER}-${REL}; waiting for aurweb to reindex (paru reads this)…"
for _ in $(seq 1 20); do
    live="$(curl -s "https://aur.archlinux.org/rpc/v5/info?arg[]=door" \
            | grep -oE '"Version":"[^"]+"' | head -1 || true)"
    if [ "$live" = "\"Version\":\"${VER}-${REL}\"" ]; then
        echo "    aurweb RPC now serves ${VER}-${REL} ✓"
        break
    fi
    sleep 6
done

cat <<EOF
==> done → https://aur.archlinux.org/packages/door
    Get it with:
        paru -Sy         # refresh AUR metadata — paru caches it, so a bare 'paru' can show the old version
        paru -S door     # upgrade/build ${VER} (or run this alone to build straight from the AUR git now)
    If the "New Version" column still lags for a minute or two, that's cgit/RPC
    cache, not a failed publish — the AUR git repo (above) is the source of truth.
EOF
