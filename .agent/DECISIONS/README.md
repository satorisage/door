# Project Decisions

Append-only register of architectural decisions. Each decision lives in
its own file at `.agent/DECISIONS/DECISION-XXXX-slug.md` and progresses
through a defined lifecycle.

`.agent/PROJECT-SCOPE.md` (`## Hard constraints`) governs binding force:
decisions marked **Binding** here apply until explicitly superseded by a
new dated decision.

## Lifecycle

Each decision has a **Status**:

- **Proposed** — under discussion, not yet ratified. Implementation should
  not depend on this decision until ratified.
- **Binding** — ratified, in effect. Cited by file:section in other
  artifacts. Changing direction requires a superseding decision, not an
  edit.
- **Superseded by D-YYYY** — was Binding, replaced by a later decision.
  The original file stays; the index row notes the supersession.

## Three-surface sync at ratification

When a decision moves Proposed → Binding, three surfaces update in
lockstep:

1. **The decision file** — `Status:` Binding, add `Ratified:` date.
2. **This index** — update the row; if it supersedes an earlier decision,
   mark the earlier row "Superseded by D-XXXX (date)".
3. **`PROJECT-STATE.md`** — clear any "deferred until D-XXXX ratified"
   notes referencing this decision.

Missing any of the three is drift. The `decision-log/` tool
automates this.

## Index

| ID | Date | Title |
|---|---|---|

(Add a row per decision in numeric order. Mark superseded decisions
inline in the title cell: `— **Superseded by 0NNN (YYYY-MM-DD)**`.)
| 0001 | 2026-06-25 | [Fully self-contained: bespoke IPC protocol, no greetd](DECISION-0001-bespoke-no-greetd.md) |
| 0002 | 2026-06-25 | [Rust for the whole stack (doord + door-greeter)](DECISION-0002-rust-stack.md) |
| 0003 | 2026-06-25 | [Hardened seam & TCB security defaults](DECISION-0003-hardened-seam-defaults.md) |
| 0004 | 2026-06-25 | [logind session registration via pam_systemd](DECISION-0004-logind-session-pam-systemd.md) — *leadership clause superseded by 0005 (2026-06-26)* |
| 0005 | 2026-06-26 | [Per-login session worker is the logind session leader](DECISION-0005-per-login-session-worker.md) |
| 0006 | 2026-06-26 | [Greeter UI toolkit: Iced + iced_layershell](DECISION-0006-greeter-toolkit-iced.md) — *surface clause superseded by 0007 (2026-06-26); Iced stands* |
| 0007 | 2026-06-26 | [Greeter host: cage + plain-iced fullscreen toplevel](DECISION-0007-greeter-host-cage-toplevel.md) |
| 0008 | 2026-06-26 | [doord owns the greeter lifecycle (handoff + re-greet)](DECISION-0008-doord-owns-greeter-lifecycle.md) |
| 0009 | 2026-06-27 | [Session lifetime tied to doord; seat freed by killing the compositor process group](DECISION-0009-session-lifetime-tied-to-doord.md) |
