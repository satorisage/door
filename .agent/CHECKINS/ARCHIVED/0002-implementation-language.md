# CHECK-IN 0002 — Implementation language for the privileged path (Critical, open)

**Status:** open — blocks M0 done-when.
**Criticality:** Critical (hard constraint: memory-safe privileged path).

## The question

What language implements `doord` (the TCB)? The scope mandates **memory-safe**
for the privileged path; this check-in ratifies the specific choice and whether
the greeter shares it.

- **Rust** (presumptive) — memory-safe, no GC, mature PAM/`logind`/Wayland
  crates, and the lingua franca of this exact domain (greetd, ReGreet, several
  greeters are Rust). Removes the use-after-free bug class from the TCB.
- **Alternatives considered:** C (the SDDM/PAM tradition, but hands back the bug
  class door exists to avoid — rejected for the privileged path); Go (memory-safe
  but GC + larger runtime in a pre-session context); Zig (memory-safe-ish, far
  smaller ecosystem for PAM/logind).

## Why it's Critical

The privileged daemon is the entire security argument. The language choice is a
hard constraint in scope; getting it wrong undermines the project's reason to
exist over just-configuring-greetd.

## Recommendation (for ratification)

**Rust** for `doord` and `door-greeter` both — one toolchain, memory safety
across the seam, best-in-domain library support. Greeter toolkit (GTK4 / Qt-QML /
Iced / bespoke wgpu) is a *separate, Material* decision, not this one.

**Resolution:** _pending — graduates to DECISION-0002 on ratification._
