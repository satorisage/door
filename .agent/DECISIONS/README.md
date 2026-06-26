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
