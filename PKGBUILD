# Maintainer: satorisage
# door — a self-contained, reversible Wayland login manager.
#
# Installs DISABLED by default (M6 / PROJECT-SCOPE): the package never enables any
# unit and never touches the active display manager. Making door the active DM is
# a deliberate, reversible step the admin runs separately, with a tested TTY revert
# in hand — see the post-install note. The previous DM is left installed as the
# fallback.
pkgname=door
pkgver=0.0.0
# rel 2 (2026-06-27): ships the live-enable lockout fixes — reversible enable
# plus a tested two-command TTY revert, proven on hardware.
pkgrel=2
pkgdesc="Self-contained reversible Wayland login manager (privileged doord + unprivileged greeter)"
arch=('x86_64')
url="https://github.com/satorisage/door"
license=('MPL-2.0')
# Runtime: PAM, logind (systemd), and the greeter host compositor (cage).
depends=('pam' 'systemd' 'cage')
makedepends=('cargo')
backup=('etc/pam.d/doord' 'etc/pam.d/door-greeter')
install="${pkgname}.install"
options=('!debug')
# For a tagged release, add source=(...) + sha256sums and a fixed checkout; this
# builds the working tree for now (run `makepkg` from the repo root).

build() {
    cd "$startdir"
    cargo build --release --workspace
}

check() {
    cd "$startdir"
    cargo test --release --workspace
}

package() {
    cd "$startdir"

    # Binaries: the privileged daemon, the unprivileged greeter, and the (also
    # unprivileged) settings editor.
    install -Dm755 target/release/doord         "$pkgdir/usr/bin/doord"
    install -Dm755 target/release/door-greeter  "$pkgdir/usr/bin/door-greeter"
    install -Dm755 target/release/door-settings "$pkgdir/usr/bin/door-settings"

    # PAM: the login service (doord) and the passwordless greeter session service.
    install -Dm644 dist/pam.d/doord            "$pkgdir/etc/pam.d/doord"
    install -Dm644 dist/pam.d/door-greeter     "$pkgdir/etc/pam.d/door-greeter"

    # systemd unit (installed, NOT enabled) + the greeter system user. doord owns
    # the greeter lifecycle (D-0008), so there is no separate greeter unit.
    install -Dm644 dist/systemd/doord.service "$pkgdir/usr/lib/systemd/system/doord.service"
    install -Dm644 dist/sysusers.d/door.conf  "$pkgdir/usr/lib/sysusers.d/door.conf"

    # Greeter theme: the packaged default look + its wallpaper, world-readable under
    # /usr/share/door/. Admins customize by copying greeter.toml to /etc/door/.
    install -Dm644 dist/door/greeter.toml  "$pkgdir/usr/share/door/greeter.toml"
    install -Dm644 dist/door/wallpaper.png "$pkgdir/usr/share/door/wallpaper.png"

    # Settings editor launcher (appears under Settings in the app menu).
    install -Dm644 dist/door/door-settings.desktop \
        "$pkgdir/usr/share/applications/door-settings.desktop"

    install -Dm644 LICENSE "$pkgdir/usr/share/licenses/$pkgname/LICENSE" 2>/dev/null || true
}
