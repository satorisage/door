# DECISION-0002 — Rust for the whole stack (doord + door-greeter)

**Status:** Binding
**Date:** 2026-06-25
**Ratified:** 2026-06-25
**Project:** door

## Context

PROJECT-SCOPE makes "memory-safe language for the privileged path" a hard
constraint — the privileged daemon is door's entire security argument, so it
must not carry a use-after-free bug class. CHECKIN-0002 left the specific
language open. Stephen confirmed Rust.

## Decision

**Rust** implements the whole stack: `doord` (privileged daemon) and
`door-greeter` (unprivileged UI), in a single Cargo workspace with a shared
`protocol` crate.

1. Memory safety holds across the trust boundary, not just inside the daemon.
2. One toolchain, one workspace, shared types for the IPC seam.
3. The greeter's UI **toolkit** (GTK4 / Qt-QML / Iced / bespoke wgpu) is a
   *separate, Material* decision, deferred — this decision fixes the language,
   not the rendering stack.

## Alternatives considered

- **C** — the SDDM/PAM tradition with every binding available, but reintroduces
  exactly the memory-bug class door exists to avoid in its most security-critical
  code. Rejected: contradicts the hard constraint.
- **Zig** — small binaries and clean C interop, but safety is opt-in (not
  guaranteed) and the `logind`/Wayland ecosystem is thin. Weaker fit for a
  security-first TCB. Rejected.
- **Go** — memory-safe, but a GC + larger runtime in a pre-session context and
  CGo-heavy PAM bindings. Rejected for the privileged path.

## Consequences

- Project is a **Cargo workspace**: `doord`, `door-greeter`, `protocol` crates.
- Library direction (to be pinned as work reaches each): `pam`/`pam-client` for
  the PAM conversation, `zbus` for `logind`, `wayland-client`/`smithay`-adjacent
  crates for the greeter, a seccomp/landlock crate for sandboxing the daemon.
- Establishes the toolchain for all milestones; CI and packaging assume Rust.
- Durable rule: new privileged-path code is Rust; an FFI/`unsafe` boundary to a
  C library (e.g. PAM) is allowed but must be wrapped and kept minimal.
