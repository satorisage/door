# DECISION-0018 — pre-auth surface stance, round 2: graduate keyboard-layout + battery; user-list stays parked; no-network reaffirmed absolute + a verification engagement

**Status:** Binding
**Date:** 2026-07-03
**Ratified:** 2026-07-03
**Project:** door
**Supersedes (in part):** DECISION-0012 (its indicator-parking clause — the
keyboard-layout and battery items graduate here; user-list and the network
*indicator* remain parked). D-0012's caps-lock ruling and its F1/F2 gating are
unchanged.
**Relates to:** Scope Principle 1 (security dominates elegance) + Principle 7
(threat-model-first) + the `No network, ever` hard constraint + the
privilege-separation hard constraint. Resolves CHECK-IN 0003's five owner
decisions (the four pre-auth-surface ones; per-monitor wallpaper is windowing —
tracked separately against D-0006/D-0007, not here).

## Context

D-0012 parked "battery / network / keyboard-layout indicators" and the user
list, each requiring "its own dated threat-model + decision before it can
graduate (it does not get 'barreled')." The owner reviewed the parked set and
made per-item calls. This decision records them and the threat model each
graduating item ships under. All graduating UI is **default-off** — the shipped
built-in look is unchanged unless a theme opts in.

## Decision

Per-item disposition of the D-0012 parked pre-auth set:

1. **Keyboard-layout indicator — AUTHORIZE (default-off).** A purely local read
   of the greeter's *own* xkb layout state — no daemon protocol, no privileged
   path, no network. Directly defeats the classic "password fails silently
   because the active layout isn't the one the user is typing for" login trap.
   Ships behind a theme key defaulting off; the built-in default look is
   unchanged.

2. **Battery indicator — AUTHORIZE (default-off).** A purely local read of
   `/sys/class/power_supply/*` (world-readable sysfs) — no daemon change, no
   privileged path, no network. Useful at a laptop lid-open login. Default-off.

3. **User list + avatars — PARKED (unchanged from D-0012).** Still costs a new
   *privileged* daemon IPC message to enumerate users **and** discloses valid
   usernames to anyone at the pre-auth screen (shoulder-surf / evil-maid recon).
   The cost/benefit did not change; it stays behind the username field.

4. **Network indicator — DECLINED; `No network, ever` REAFFIRMED ABSOLUTE.** The
   owner considered lifting the network hard constraint (guarded by a pentest)
   and instead chose to keep it **absolute and provable-by-construction**
   (Principle 9: the strongest claim is the one provable without running
   anything — there is no socket code on the pre-auth surface, so its remote
   attack surface is *provably* empty; a pentest evidences the presence of bugs,
   never their absence, and would trade an absolute headline for a best-effort
   one that decays every commit). **In place of a network indicator, a red-team
   verification engagement is commissioned** to prove the no-network claim holds
   (see Consequences → `.agent/SECURITY/`). Should a connectivity *indicator*
   ever be wanted, the only in-bounds path remains the F3 helper-file pattern
   (a post-login helper writes `/run/door/net.json`; the greeter only *reads* a
   local file, never touches dbus/NetworkManager pre-auth) — parked, not built.

## Consequences

- **New ROADMAP work (default-off greeter indicators), each Minor, TCB-neutral,
  naga-irrelevant:**
  - *Keyboard-layout indicator* — a `show_kb_layout` theme key (door-theme,
    default `false`) + a local xkb read + a card row in `door-greeter/src/app.rs`
    (mirroring the D-0012 caps-lock row pattern: slips in only when shown, no
    empty gap). door-settings gets the toggle.
  - *Battery indicator* — a `show_battery` theme key (door-theme, default
    `false`) + a `/sys/class/power_supply` read + a card row; `None` (no battery,
    e.g. a desktop) hides it rather than asserting a state it cannot know.
    door-settings gets the toggle.
  Both preserve the byte-identical-default invariant (off by default).
- **New security workstream:** `.agent/SECURITY/no-network-verification-engagement.md`
  scopes an owner-run (CRTO/OSCP) red-team engagement to *prove* `No network,
  ever` holds — no egress, no ingress, no leak through the IPC seam or the
  session-spawn environment. Its findings file a threat-model report under
  `.agent/SECURITY/`. This *strengthens* the no-network headline rather than
  spending it.
- **Unchanged:** the pre-auth daemon↔greeter IPC surface (no new message);
  user-list + network *indicator* stay parked; D-0012's caps-lock indicator and
  F1 (`.wgsl`) / F2 (video) gating stand.
- **Not covered here:** per-monitor wallpaper (CHECK-IN 0003 item 5) is a
  windowing decision against D-0006/D-0007 — owner chose the documented
  primary-output-only v1 bound now, with a supersession revisit parked to
  ~2026-07-17. Tracked in ROADMAP `## Backlog`, not in this decision.
