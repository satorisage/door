# DECISION-0001 — Fully self-contained: bespoke IPC protocol, no greetd

**Status:** Binding
**Date:** 2026-06-25
**Ratified:** 2026-06-25
**Project:** door

## Context

door is a full, security-first login manager (PROJECT-SCOPE §Overview). The
open question (CHECKIN-0001) was the broker strategy: run greetd and ship only a
greeter; build our own daemon but speak greetd's wire protocol for greeter
compatibility; or define our own protocol. Stephen's directive — "no greetd,
we're writing the whole thing" — settles it. The IPC seam between the
unprivileged greeter and the privileged core IS the trust boundary (Principle 2,
scope principle #2), so its design is load-bearing and Critical.

## Decision

door is **fully self-contained with a bespoke IPC protocol**. No greetd at
runtime, and no greetd wire-format compatibility.

1. `doord` (privileged) owns the entire pre-session stage end to end: PAM auth,
   `logind`/seat/VT, session discovery + spawn, and the IPC server.
2. The greeter↔core protocol is **door's own**, designed for door's needs
   (multi-prompt/2FA conversations, session metadata, animation/state hints),
   carried over a local Unix socket with peer-credential checks.
3. We do **not** constrain the protocol to greetd's model for the sake of
   interop with existing greeters; door is replacing that layer, not joining it.

## Alternatives considered

- **Run greetd, ship only a greeter.** Smallest TCB we'd own, but it makes door
  a greeter, not a login manager — contradicts the locked vision. Rejected by
  the vision and by Stephen directly.
- **Own daemon, greetd-compatible wire format.** Would give a free test harness
  (tuigreet/gtkgreet as throwaway clients) and a proven message design, at the
  cost of being permanently constrained to greetd's conversation model and
  carrying compat for an ecosystem we intend to replace. Rejected: the
  constraint isn't worth a test-only convenience.

## Consequences

- **A bespoke protocol crate** is part of the architecture: a shared `protocol`
  crate defines the message types used by both `doord` and `door-greeter`.
- **We own the protocol's security review.** The seam must be specified and
  threat-modeled before it carries credentials (gates M1).
- **No free fallback greeters** — we build a minimal test/CLI greeter ourselves
  as a harness (folds into M3).
- Supersedes the "greetd IPC compatibility" item previously preserved under
  PROJECT-SCOPE §"Planned but not yet specified" — that item is now decided OUT.
- Durable rule: door takes **no dependency on greetd** (runtime or protocol);
  any proposal to interoperate requires a superseding decision.
