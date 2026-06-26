# DECISION-0006 — Greeter UI toolkit: Iced + iced_layershell

**Status:** Binding
**Date:** 2026-06-26
**Ratified:** 2026-06-26
**Project:** door

## Context

M3 (the minimal functional greeter) is gated on choosing `door-greeter`'s UI
toolkit (a Material decision parked in ROADMAP `## Loose`). The greeter is a
**Wayland-native, pre-session, unprivileged** client: it must be a
`wlr-layer-shell` surface (a fullscreen overlay that owns the top layer and grabs
the keyboard before any user session), runs as a system user with no `$HOME` and
no user session bus, and renders only system-wide world-readable assets
(PROJECT-SCOPE). It must reach **two** goals: M3 wants *functional fast* (session
picker, password field, power controls, ugly is fine), M4 wants *beautiful and
animated*. **D-0002 (Binding) — "Rust for the whole stack" — is the decisive
constraint.**

## Decision

**Iced** (pure-Rust, Elm-architecture, `wgpu`/GPU-backed) for `door-greeter`, with
**`iced_layershell`** (verified v0.18.1, the waycrate ecosystem) providing the
`wlr-layer-shell` surface. A thin protocol-client layer sits over the existing
`protocol` crate; blocking socket I/O is bridged to Iced's runtime via a
subscription fed from a background reader (the greeter's request/response flow is
synchronous, Iced's loop is async).

## Rationale

- **Honors D-0002.** Native Rust, no C toolkit and no immature Rust↔C++/QML
  bridge. One language across daemon and greeter.
- **Lean untrusted surface.** The greeter is the attacker-facing half
  (PROJECT-SCOPE); a Rust-crate dep tree is far smaller attack surface than the
  full GTK or Qt runtimes.
- **Functional-fast for M3.** Built-in widgets (text input, list/picker, buttons)
  reach a working picker + password + power quickly.
- **GPU path for M4.** `wgpu` backing gives the animation/compositing headroom the
  "beautiful default" needs, without committing to build a renderer by hand.
- **Low pre-session friction.** Iced makes no session/settings-daemon assumption,
  unlike GTK4 (which expects a session/dconf); right for a no-`$HOME` greeter.
- **Layer-shell is real and maintained.** `iced_layershell` is used by shipping
  layer-shell apps (bars, widgets), so the load-bearing requirement is met by an
  existing integration rather than bespoke work.

## Alternatives considered

- **GTK4 + gtk4-layer-shell.** Mature, with the gtkgreet (greetd) precedent and a
  solid layer-shell binding. Rejected: heavy C dependency tree, assumes a
  session/settings daemon (real pre-session friction), and the weakest animation
  story for the M4 vision — and only `gtk4-rs` bindings, not native Rust.
- **Bespoke wgpu + smithay-client-toolkit.** Leanest and maximum control over the
  beautiful default, native Rust, layer-shell owned directly. Rejected *for now*:
  rebuilds an entire toolkit (text layout, input, widgets, animation), the slowest
  path to a functional M3. Remains the fallback if Iced is ever outgrown.
- **Qt-QML (Qt Quick).** Best-in-class animation/theming and a layer-shell-qt
  binding (SDDM precedent). **Rejected on D-0002:** QML is JS/C++, the Rust path
  (cxx-qt) is immature, and it pulls the entire Qt runtime — a poor fit for a
  Rust-whole-stack, lean, untrusted greeter.

## Consequences

- `door-greeter` gains dependencies: `iced` + `iced_layershell` (compatible
  versions pinned at integration), plus the existing `protocol` crate; a small
  client module wraps the socket conversation (`Hello`/`Welcome`, `ListSessions`,
  `BeginAuth` + the `AuthPrompt`/`AuthReply` conversation, `Start`, `Power`).
- The greeter holds **no credential beyond submit** and starts no session (it only
  relays to the daemon) — the privilege-separation invariant is unchanged; the
  toolkit choice does not touch the trust boundary.
- All greeter assets (theme, font, cursor, shaders) install system-wide and
  world-readable, or the greeter falls back to stock (PROJECT-SCOPE).
- Revisiting Iced (e.g. if `iced_layershell` proves unworkable for the pre-session
  overlay, or M4 outgrows Iced) requires a superseding decision — the natural
  successor is bespoke `wgpu` + `smithay-client-toolkit`.
