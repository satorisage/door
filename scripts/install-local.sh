#!/usr/bin/env bash
# door — compile from the working tree and install to the live system.
#
# This is the dev-machine counterpart to the PKGBUILD: it builds the workspace
# in release mode and installs the exact same files into the exact same paths as
# `package()`, but straight from your local checkout instead of a release tarball.
# Use `makepkg`/`release.sh` for a real distributable package; use this to iterate.
#
# Like the package, this installs door DISABLED: it never enables a unit and never
# touches your active display manager. Flipping doord on stays a separate, manual,
# reversible step (see door.install / README).
#
# Usage:
#   scripts/install-local.sh            # build (release) + install
#   scripts/install-local.sh --debug    # build (debug) + install, faster compile
#   scripts/install-local.sh --no-build # install whatever is already built
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
PKGNAME=door

PROFILE=release
PROFILE_DIR=release
DO_BUILD=1
for arg in "$@"; do
    case "$arg" in
        --debug)    PROFILE=dev;     PROFILE_DIR=debug ;;
        --no-build) DO_BUILD=0 ;;
        -h|--help)  sed -n '2,18p' "$0"; exit 0 ;;
        *) echo "unknown flag: $arg" >&2; exit 2 ;;
    esac
done

# --- 1. compile -------------------------------------------------------------------
if [[ $DO_BUILD -eq 1 ]]; then
    echo "==> cargo build --profile $PROFILE --workspace"
    cargo build --profile "$PROFILE" --workspace
fi

BIN="$ROOT/target/$PROFILE_DIR"
for b in doord door-greeter door-settings; do
    [[ -x "$BIN/$b" ]] || { echo "missing binary: $BIN/$b (build first)" >&2; exit 1; }
done

# --- 2. install (root) ------------------------------------------------------------
# Everything below writes under / and needs root. Re-exec the install half via sudo
# so the build half above runs as the unprivileged user.
echo "==> installing to / (sudo)"
sudo env ROOT="$ROOT" BIN="$BIN" PKGNAME="$PKGNAME" bash -euo pipefail -s <<'INSTALL'
cd "$ROOT"

# Binaries: privileged daemon + unprivileged greeter + settings editor.
install -Dm755 "$BIN/doord"         /usr/bin/doord
install -Dm755 "$BIN/door-greeter"  /usr/bin/door-greeter
install -Dm755 "$BIN/door-settings" /usr/bin/door-settings

# PAM services.
install -Dm644 dist/pam.d/doord        /etc/pam.d/doord
install -Dm644 dist/pam.d/door-greeter /etc/pam.d/door-greeter

# systemd unit (installed, NOT enabled) + greeter system user.
install -Dm644 dist/systemd/doord.service /usr/lib/systemd/system/doord.service
install -Dm644 dist/sysusers.d/door.conf  /usr/lib/sysusers.d/door.conf

# Greeter theme: default + wallpaper + presets, world-readable under /usr/share/door/.
install -Dm644 dist/door/greeter.toml  /usr/share/door/greeter.toml
install -Dm644 dist/door/wallpaper.png /usr/share/door/wallpaper.png
for preset in dist/door/presets/*.toml; do
    install -Dm644 "$preset" "/usr/share/door/presets/$(basename "$preset")"
done

# Settings editor launcher.
install -Dm644 dist/door/door-settings.desktop /usr/share/applications/door-settings.desktop

install -Dm644 LICENSE "/usr/share/licenses/$PKGNAME/LICENSE"

# Register the greeter user and reload units — same as the package post_install.
# Nothing is enabled; your active display manager is untouched.
systemd-sysusers door.conf >/dev/null 2>&1 || true
systemctl daemon-reload     >/dev/null 2>&1 || true
INSTALL

cat <<EOF

==> done. door is installed but DISABLED — your current login manager is untouched.

   Make door active (reversible), as root:
     systemctl status display-manager.service | head -1   # note current DM
     systemctl disable <current-dm>.service                # keep installed = fallback
     systemctl enable  --now doord.service

   Revert (from a text VT — Ctrl+Alt+F2 — or SSH):
     systemctl disable --now doord.service
     systemctl enable  --now <current-dm>.service
EOF
