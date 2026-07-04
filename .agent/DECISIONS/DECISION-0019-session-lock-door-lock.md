# DECISION-0019 — Session lock screen: door-lock as an ext-session-lock-v1 client + a doord Reauth verb (M10)

**Status:** Binding
**Reversibility:** costly (new binary + a new privileged IPC verb + a scope
amendment; the *code* is additive and behind its own binary, but the privilege
boundary and the new lockout mode are load-bearing once shipped)
**Date:** 2026-07-03
**Ratified:** 2026-07-03
**Project:** door
**Relates to:** the `## Out of scope` "Locking is a candidate… needs a check-in
to bring in" clause it graduates; the privilege-separation hard constraint;
`No network, ever`; D-0004/D-0005 (PAM + the per-login worker/spawner model the
Reauth verb reuses); D-0006/D-0007 (the greeter's cage host + primary-output
bound this deliberately does *not* inherit); D-0012/D-0018 (the pre-auth
disclosure posture the lock surface mirrors); Scope Principles 1 (security
dominates), 2 (privilege separation), 4 (reversibility), 7 (threat-model-first).
Graduates `.agent/IDEAS/2026-07-03-session-lock-screen.md` (→ ARCHIVED on ratify).

## Context

door already owns the hard, security-critical half of a lock screen: the doord
PAM engine (multi-prompt, FIDO2/2FA, zeroizing secrets, hardware-proven), the
peer-cred-checked IPC seam, and the door-theme shader/card surface. A locker is
mechanically *most of what door already does* — cover the screen, take a
credential, verify it via PAM, reveal on success — **minus** session-spawn.
Reusing that engine lets a door locker look **identical to the login screen** for
free, which is the actual product hook.

But a locker is **not the greeter reused**, for three architectural reasons that
earn it its own module and its own threat model rather than a flag:

1. **Opposite threat model.** The greeter runs *pre-session*, as the greeter
   system user, on an empty VT door owns — a bug exposes a blank pre-auth
   screen. A locker runs *on top of a live, authenticated session* holding the
   user's unlocked keys, agents, and apps — the evil-maid / walk-up surface
   against an already-unlocked machine. A locker bypass or crash exposes real
   secrets.

2. **door does not own the session's compositor.** door hands off to the user's
   compositor (Hyprland/KDE/sway/…) and steps back; the greeter's cage-hosted
   surface is gone once the session starts. A secure locker must therefore be a
   Wayland client speaking to *the user's running compositor*, not a
   door-owned cage toplevel.

3. **Nastier lockout asymmetry.** A broken *login* locks you out at boot →
   TTY-recoverable. A broken *lock* either **fails to lock** (session silently
   exposed — a security failure) or **fails to unlock** (trapped out of a live
   session with unsaved work). The recovery story must be designed up front, not
   patched after.

The owner reviewed the design and its six open forks (2026-07-03) and ratified
the module shape below. This decision is **Critical** (privilege boundary + new
lockout mode); it pre-ratifies before any code (nothing is built yet).

## Decision

Ship a session lock screen as a new **`door-lock`** binary — an
`ext-session-lock-v1` Wayland client that renders the door-theme surface and
authenticates by asking doord (over the existing seam) to **reauthenticate the
caller's own uid**, never to spawn a session. Concretely:

**Module shape.**
```
user session (Hyprland/KDE/sway) --launches--> door-lock (unprivileged)
  door-lock --ext-session-lock-v1--> the user's compositor  (owns the blank/lock surface)
  door-lock --IPC (existing seam)--> doord (root)  --PAM reauth of THIS user--> allow/deny
  door-lock --door-theme-->          renders the same card/shader look as the greeter
```
`door-lock` holds no authority beyond "ask doord to verify these credentials for
my *own* uid," mirroring the greeter's posture (privilege separation, hard
constraint). It touches no PAM itself.

The six forks resolve as (owner-ratified 2026-07-03):

1. **(F-lock-1 — protocol) `ext-session-lock-v1` ONLY.** The modern protocol
   whose whole point is crash-safety: the *compositor* owns the blanking, so a
   locker crash does **not** reveal the session. Supported across wlroots
   compositors (sway, Hyprland, river, …) and KWin (Plasma 5.27+). GNOME/Mutter
   (ships its own locker) and exotic WMs lacking the protocol are an **honest,
   named bound** (Scope Principle 6/9) — door-lock does not lock them. No
   insecure layer-shell fallback is shipped (a fallback overlay would reveal the
   session on crash, trading away the one guarantee that makes a locker safe —
   rejected under Tenet 1).

2. **(F-lock-2 — who reauths) a new doord `Reauth` IPC verb.** doord grows a
   verify-only verb: it runs the PAM conversation for the credentials supplied,
   **pinned to the socket peer-cred uid**, and returns allow/deny — it never
   spawns a session, never enters a session scope. One privileged daemon, one
   audited PAM engine (reusing the FIDO2/multi-prompt/min-failure-delay path),
   one seam. Putting PAM in door-lock is **rejected** (PAM in an unprivileged
   intra-session process violates privilege separation); a *second* privileged
   locker-auth daemon is **rejected** (duplicate TCB to audit for no gain). The
   **uid binding is load-bearing**: a locker running as user A may authenticate
   *only* as user A — Reauth derives the target uid from `SO_PEERCRED`, never
   from a client-supplied username, so it cannot be turned into a cross-user
   brute-force oracle.

3. **(F-lock-3 — trigger) surface first, trigger later.** v1 ships the lock
   *surface* only; triggering is left to the user's existing idle stack
   (hypridle, swayidle, `loginctl lock-session`, which door-lock can be wired
   to). A door-shipped idle/logind-`Lock`-signal listener is a **Backlog**
   candidate, not v1 — every compositor ecosystem already has idle wiring;
   shipping ours duplicates it and grows scope before the surface is proven.

4. **(F-lock-4 — break-glass recovery) TTY recovery, NO daemon unlock verb.**
   Recovery rests on the protocol's crash-safety floor (a wedged/crashed
   door-lock leaves the compositor blanking the screen — the session stays
   protected) plus a **documented two-command TTY recovery** (VT-switch → log in
   → kill/restart door-lock; compositors accept a fresh lock client), matching
   the installer-revert discipline (Principle 4). A doord-side force-unlock verb
   is **rejected**: it would be a standing root-reachable unlock lever — new
   attack surface on the very thing being secured. (Mini threat model below.)

5. **(F-lock-5 — scope) single-seat, all outputs.** Single-seat matches the v1
   single-seat constraint. Unlike the greeter (cage fullscreens one output — the
   primary-only bound, D-0006/D-0007), `ext-session-lock-v1` hands the locker
   **every** output and the compositor blanks anything left uncovered, so
   per-output lock surfaces are the protocol's natural shape with no cage/TCB
   conflict. door-lock renders the themed sky on all monitors — leapfrogging the
   greeter's parked per-monitor limitation, safely (uncovered edge cases blank
   rather than expose).

6. **(F-lock-6 — disclosure) mirror the greeter posture exactly.** The locked
   surface shows themed sky + clock + the password/FIDO2 prompt, plus the
   D-0018 default-off indicators (keyboard-layout, battery) carried over — and
   **no** notifications, media controls, or session content, ever. One
   disclosure policy across both auth surfaces (shoulder-surf class, same as the
   D-0018 pre-auth indicators). A richer locked surface (notification counts,
   media) is out of scope; revisiting it needs its own DECISION.

## Lock-specific threat model

*What door-lock defends against, and what it explicitly does not (Principle 7).*

- **Defends: walk-up / evil-maid against an unlocked session.** An attacker at
  the physical keyboard of a locked-but-logged-in machine cannot reach the
  session without the user's credential (or second factor). PAM reauth runs in
  **root doord**, never in door-lock; door-lock never sees a success it can
  forge (the greeter's `AuthSuccess`-forgery guard extends to Reauth).
- **Defends: locker compromise ≠ session exposure.** Because the *compositor*
  owns the lock surface (ext-session-lock-v1), a crashed/exploited/killed
  door-lock leaves the screen blanked — the failure mode is "stays locked," not
  "reveals." This is the structural reason the protocol was chosen over a
  layer-shell overlay.
- **Defends: cross-user auth abuse.** Reauth's uid is peer-cred-derived, so a
  locker (or any client on the seam) cannot ask doord to verify or brute-force a
  *different* user's password. Reuses the greeter's min-failure-delay against
  online guessing.
- **Does NOT defend: a compromised live session.** door-lock fronts a session
  that already holds unlocked secrets; malware *inside* that session (before
  lock) is out of scope — the locker is an access-control surface at the glass,
  not a session sandbox. (The session's own sandboxing is a separate concern.)
- **Does NOT defend: compositors without ext-session-lock-v1.** GNOME/Mutter and
  exotic WMs are a named bound (F-lock-1); door-lock declines to lock them rather
  than ship a fallback that would weaken the crash-safety guarantee.
- **Does NOT defend: DMA/cold-boot/physical-RAM attacks.** Out of scope for a
  software locker; the threat is the walk-up keyboard, not a bench attacker.
- **Residual — the wedged-unlock trap.** If door-lock fails to *unlock* (accepts
  no valid credential), the recovery is the documented TTY path (F-lock-4); there
  is deliberately no root break-glass lever, trading a small recovery-friction
  cost for removing a standing unlock attack surface.

## Alternatives considered

- **Reuse the greeter as the locker.** Rejected: the greeter is a cage-hosted
  toplevel on a door-owned VT, gone once the session starts; it does not own (and
  must not seize) the user's live compositor. A locker must be a client *of* that
  compositor. Different host, different threat model, different lifetime → a
  different module (Principle 6/7).
- **Layer-shell overlay locker (works everywhere).** Rejected under Tenet 1: an
  overlay does not get the compositor's crash-safety guarantee — if it dies, the
  session is revealed. The wider compositor reach is not worth trading away the
  single property that makes a locker trustworthy.
- **PAM directly in door-lock.** Rejected: puts the credential-verifying code in
  an unprivileged intra-session process, violating the privilege-separation hard
  constraint.
- **A separate locker-auth daemon.** Rejected: a second privileged process with
  its own PAM stack, socket, and sandbox tiers to re-earn and re-audit, for no
  capability the Reauth verb doesn't already provide.
- **A doord force-unlock / break-glass verb.** Rejected: a standing
  root-reachable unlock lever is exactly the attack surface a locker exists to
  deny; the protocol's crash-safety + TTY recovery cover the recovery need
  without it.
- **Ship idle-trigger wiring in v1.** Deferred (not rejected): the surface is the
  novel, security-critical part; triggering is a solved problem in every
  compositor ecosystem. Kept as a Backlog follow-on to avoid scope-creeping the
  first milestone.

## Consequences

- **Scope amendment (dated).** PROJECT-SCOPE `## Out of scope` "session locking"
  clause is superseded: session locking moves from candidate to a **committed
  milestone (M10)**, per this decision, dated 2026-07-03. The amendment names the
  ext-session-lock-v1-only bound as the honest limit.
- **New ROADMAP milestone M10 — door-lock** (Criticality: **Critical** — new
  privilege verb + new lockout mode). Task tree:
  - **doord `Reauth` verb** — verify-only PAM for the peer-cred uid; no spawn, no
    session scope; reuses the worker/spawner + FIDO2 + min-failure-delay path;
    uid-binding + no-forgery tests. `depends:` M1/M2 (the PAM + IPC seam).
  - **`door-lock` binary** — an `ext-session-lock-v1` client (per-output surfaces,
    single seat) rendering door-theme, driving the Reauth verb, mirroring the
    greeter's disclosure posture (default-off kb-layout/battery carried over).
    `depends:` Reauth verb; door-theme.
  - **honest-bounds + recovery docs** — the unsupported-compositor list and the
    two-command TTY break-glass recovery, installer-printed in the door-lock
    spirit of Principle 4.
  - **lock threat-model file** under `.agent/SECURITY/` (this section, expanded
    on build).
  - Backlog follow-on: idle/`loginctl lock-session` trigger wiring (F-lock-3);
    richer locked-surface disclosure would need its own DECISION (F-lock-6).
- **New durable rule:** any credential path added to the seam derives its target
  uid from `SO_PEERCRED`, never from a client-supplied identity (the Reauth
  uid-binding invariant — a locker for user A authenticates only as A).
- **Idea graduated:** `.agent/IDEAS/2026-07-03-session-lock-screen.md` moves to
  `IDEAS/ARCHIVED/` with a pointer to this decision.
- **Unchanged:** the greeter, its cage host, and the primary-output bound
  (D-0006/D-0007) — door-lock's all-outputs behavior is a property of
  ext-session-lock-v1, not a supersession of the greeter's windowing. The
  `No network, ever` constraint binds door-lock identically (local seam only).
