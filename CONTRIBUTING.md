# Contributing to door

Thanks for your interest! door is early-alpha, so the most useful contributions right
now are bug reports (with reproductions) and small, focused PRs.

## Building

A standard Rust workspace. You need a recent stable toolchain, plus these system
packages (names are Debian/Ubuntu; Arch has the equivalents):

```sh
# Arch: pam, wayland, clang are usually already present
sudo apt-get install libpam0g-dev libwayland-dev pkg-config clang libclang-dev

cargo build --workspace
cargo test --workspace
```

The greeter (`door-greeter`) and settings editor (`door-settings`) are Wayland apps;
to run the greeter outside a real login, use `DOORD_GREETER_DEV=1` (nested) against a
running compositor. `door-settings` runs as a normal window.

## Before you open a PR

Run the same checks CI does:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

PRs must be **format-clean, clippy-clean, and tests green.**

## Ground rules

- **Privilege separation is the architecture.** `doord` is the trusted computing base
  (PAM, logind, seat/VT, spawn); the greeter is unprivileged and must never gain
  authority beyond asking `doord` to try credentials. Changes that blur that line need
  a strong justification.
- **Don't break the revert.** Installing must stay disabled-by-default; enabling must
  stay reversible. Don't add anything that touches the active DM on install.
- **Scope:** Wayland sessions, Arch/systemd. X11 session *launching* is out of scope
  for now (door starts no X server).
- Keep new dependencies on the pre-auth surface justified — it's a login screen.

## Repo note

The `.agent/` directory is the maintainer's internal project-governance tooling. It's
not part of the build and you can ignore it (it's excluded from release tarballs).
