#!/usr/bin/env bash
# door release helper — run from the repo root on a machine that has:
#   - the gh CLI authed as the repo owner            (the public flip + tag push)
#   - an AUR account with your SSH key uploaded       (the AUR push)
#   - pacman-contrib + base-devel                     (updpkgsums, makepkg)
#
# It (1) makes the GitHub repo public once, (2) tags v<version> and pushes it, and
# (3) builds + publishes the AUR package against that tag's source tarball.
#
# Heads-up: making the repo public also exposes the git *history*, which still
# contains earlier governance commits even though that dir is now untracked going
# forward. If you need a fully clean history, scrub it with git-filter-repo first.
set -euo pipefail

REPO="satorisage/door"
VER="$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"([^"]+)".*/\1/')"
REL="$(grep -m1 '^pkgrel=' PKGBUILD | cut -d= -f2)"
echo "==> door v${VER}-${REL}  (${REPO})"

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

# --- 3. Build + publish the AUR package -------------------------------------------
WORK="$(mktemp -d)/door"
echo "==> staging AUR package in ${WORK}"
git clone "ssh://aur@aur.archlinux.org/door.git" "$WORK"   # empty clone for a new pkg is fine
cp PKGBUILD door.install "$WORK/"
cd "$WORK"

updpkgsums                              # pin sha256 to the just-published tarball
makepkg --printsrcinfo > .SRCINFO
makepkg -f --nocheck                    # local build sanity-check before publishing

git add PKGBUILD .SRCINFO door.install
git commit -m "door ${VER}-${REL}"
git push origin master

echo "==> done → https://aur.archlinux.org/packages/door"
