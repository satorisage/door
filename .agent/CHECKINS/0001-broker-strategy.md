# CHECK-IN 0001 — Broker strategy (Critical, open)

**Status:** open — blocks M0 done-when.
**Criticality:** Critical (IPC protocol + privilege boundary).

## The question

Does `doord` speak the **existing greetd IPC protocol**, or define **its own**?
Three positions:

1. **Speak greetd's protocol, build our own daemon.** Instant compatibility with
   existing greeters (tuigreet, gtkgreet, ReGreet) and a battle-tested message
   design; door-greeter is then "just another greetd greeter." Constrains us to
   greetd's model.
2. **Run greetd itself, ship only the greeter.** Smallest TCB we own (greetd is
   already audited) — but then door is a *greeter*, not a *login manager*, which
   contradicts the locked vision (full manager).
3. **Define door's own protocol.** Full control over the seam (richer 2FA flows,
   session metadata, animation hints); we own the design and its security review.

## Why it's Critical

The IPC seam *is* the trust boundary (Principle 2). Its shape decides what the
unprivileged greeter can ask the privileged core to do, and is the hardest thing
to change later. It also decides whether door is a manager or a greeter — i.e.
whether the vision holds.

## Recommendation (for ratification)

Lean **1** (own daemon, greetd-compatible protocol): keeps the full-manager
vision, gets a proven message design and a free ecosystem of fallback greeters
to test against, and leaves "extend the protocol" as a deliberate, reviewed step
rather than a blank-page security exercise. Revisit if greetd's model blocks a
required auth flow.

**Resolution:** _pending — graduates to DECISION-0001 on ratification._
