# DECISION-0010 — greeter config tooling: a standalone Rust editor + a shared theme crate (not a Qt/QML KCM)

**Status:** Binding
**Date:** 2026-06-27
**Ratified:** 2026-06-27
**Project:** door
**Relates to:** D-0002 (Rust for the whole stack) — this stays inside it; D-0006/0007 (greeter toolkit is iced).

## Context

The greeter theme (M4) is a TOML config at `/etc/door/greeter.toml`. The ask was to
expose it in a settings UI ("the Plasma settings panel"). A true System Settings
panel on Plasma 6 means a **KDE KCM** — a QML (± C++) plugin packaged via
`kpackagetool6`, with a polkit/KAuth helper to write the root-owned file. That
would add Qt/QML to a deliberately Rust-only stack (D-0002), for a tool outside the
TCB (the editor is unprivileged; it is not doord or the greeter).

## Decision

**Ship a standalone, pure-Rust settings editor (`door-settings`, iced), not a KCM.**
The greeter config schema moves into a shared **`door-theme`** crate so the greeter
(renders it) and the editor (edits it) share one source of truth.

- `door-theme`: `Color` (parse / `to_hex` / iced conversions), `Theme` (+ defaults,
  merge, `load`), and `to_config_string` (writes a documented `greeter.toml`).
- `door-settings`: an iced form over the theme. **Preview** launches the *real*
  `door-greeter` in dev mode against a draft config (a true preview — no UI
  duplication). **Save** writes to `/etc/door/greeter.toml` via `pkexec` (the file
  is root-owned because the greeter is pre-login, so no per-user config can work).
  A `.desktop` puts it under Settings in the app menu.

D-0002 is unchanged: the whole stack — runtime *and* this tool — stays Rust.

## Consequences

- New workspace crates: `door-theme` (shared lib) and `door-settings` (bin). The
  greeter's `mod theme` is removed and replaced by the crate (one schema, one place).
- The editor is not embedded *inside* the System Settings window (KDE only hosts
  KCMs); it is a standalone window launched from the menu / addable as a shortcut.
  Accepted tradeoff for staying Rust and out of the TCB.
- Save relies on `pkexec install` (generic polkit prompt). A dedicated polkit
  `.policy` + helper for a friendlier prompt is a later refinement — queued.
- Live in-window preview (re-rendering the greeter inside the editor) is deliberately
  *not* done; launching the real greeter is the preview. Revisit only if needed.

## Alternatives considered

- **QML KCM in System Settings** (the literal "settings panel"). Rejected: adds
  Qt/QML to the Rust-only stack (D-0002) plus a KAuth/polkit helper, for an
  out-of-TCB tool. The owner chose to stay Rust.
- **Per-user `~/.config/door` written by the tool with no polkit.** Rejected: the
  greeter runs pre-login as the `door-greeter` system user — there is no logged-in
  user at greet time, so a per-user path is never read. Root-write is unavoidable.
- **Duplicate the theme parsing in the editor.** Rejected: two sources of truth for
  the schema would drift (Principle 7); hence the shared `door-theme` crate.
