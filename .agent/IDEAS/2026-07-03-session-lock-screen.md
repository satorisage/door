# Idea — door as the session lock screen too (2026-07-03)

**Status:** raw idea inbox — pre-decision, no scope commitment. Session locking is
currently **Out of scope** (PROJECT-SCOPE `## Out of scope`): *"door is login-only
for v1. Locking is a candidate (the same privilege seam could serve it) but is not
committed; needs a check-in to bring in."* This file captures the design + open
forks so a future DECISION can graduate it. Owner chose (2026-07-03) to **capture &
park**, not commit.

Grounded in door's real architecture: **doord** (root; PAM, IPC server, logind,
VT/seat), **door-greeter** (unprivileged; `cage` + plain iced fullscreen toplevel,
D-0007), **door-theme** (shared theme/shader engine).

## Premise — why door is well-positioned

A lock screen is, mechanically, *most of what door already does*: cover the screen,
take a password (or FIDO2 / 2FA), verify it via PAM, reveal on success. door already
owns the hard, security-critical half:

- **The auth engine** in doord — full PAM conversation, multi-prompt, **FIDO2/2FA**,
  zeroizing secrets, hardware-proven. A locker is this same conversation *minus*
  session-spawn/handoff.
- **The IPC seam** (Unix socket, peer-cred checked) + **door-theme** (the whole
  beautiful shader/card surface). A door locker could look **identical to your login
  screen** for free — the actual product hook.

## The fundamental difference (why it's genuinely a new module, not a flag)

1. **Opposite threat model.** The greeter runs *pre-session*, as the greeter system
   user, on an empty VT door owns — a bug exposes a blank pre-auth screen. A locker
   runs *on top of a live, authenticated session* holding the user's unlocked keys,
   agents, and apps — the evil-maid / walk-up surface against an already-unlocked
   machine. A locker bypass/crash exposes real secrets. This earns its **own threat
   model** (Principle 7), not a reuse of the auth-path one.

2. **door does not own the session's compositor.** door hands off to the user's
   compositor (Hyprland/KDE/sway/…) and steps back — the greeter's `cage`-hosted
   surface is gone once the session starts. A secure locker therefore **cannot be the
   greeter reused**; it must be a Wayland client speaking **`ext-session-lock-v1`** to
   *the user's running compositor* (the modern protocol whose whole point is that a
   locker crash does **not** reveal the session — the compositor keeps the screen
   blanked). doord (a persistent system daemon) stays available over IPC for the PAM
   side. Clean shape: a new **`door-lock`** binary = `ext-session-lock-v1` client +
   door-theme rendering + the existing doord auth seam.

3. **Nastier lockout asymmetry** (Reversibility, scope Principle 4). A broken *login*
   locks you out at boot → TTY-recoverable. A broken *lock* either **fails to lock**
   (session silently exposed — a security failure) or **fails to unlock** (trapped out
   of a live session with unsaved work; no clean TTY escape that doesn't kill the
   session). Recovery story must be designed up front (VT-switch escape hatch? a
   `doord`-side "kill the lock" break-glass? both have their own risks).

## Architecture sketch (one candidate)

```
user session (Hyprland/KDE/sway) --launches--> door-lock (unprivileged)
     door-lock  --ext-session-lock-v1-->  the user's compositor  (owns the blank/lock surface)
     door-lock  --IPC (existing seam)-->  doord (root)  --PAM reauth of THIS user--> allow/deny
     door-lock  --door-theme-->  renders the same card/shader look as the greeter
```

- doord grows a **reauth request** (verify the *already-logged-in* user's password) —
  distinct from the login flow, which spawns a session. The peer-cred check + the
  "which user may this locker authenticate as" binding become load-bearing (a locker
  for user A must not authenticate as / brute-force user B).
- `door-lock` holds no authority beyond "ask doord to verify these creds for my own
  uid," mirroring the greeter's posture (privilege separation, hard constraint).

## Open forks (decide at ratification — do NOT resolve here)

- **F-lock-1 — protocol.** `ext-session-lock-v1` only (correct, secure, portable across
  wlroots/KWin), vs also supporting compositors that lack it (KDE < X, exotic WMs)? Lean:
  `ext-session-lock-v1`-only; name unsupported compositors as an honest bound.
- **F-lock-2 — who reauths.** New doord `Reauth` IPC verb vs a separate tiny locker-auth
  daemon vs `pam` directly in `door-lock` (rejected — puts PAM in an unprivileged
  intra-session process; violates the privilege-separation constraint). Lean: doord verb.
- **F-lock-3 — idle/trigger integration.** Does door ship the idle-timeout/`loginctl
  lock-session` wiring (a `door-idle`/logind `Lock` signal listener) or only the lock
  *surface*, leaving triggering to the user's existing idle daemon? Lean: surface first,
  trigger later.
- **F-lock-4 — recovery / break-glass.** How to escape a wedged locker without exposing
  the session or losing work. Critical; needs its own mini threat model.
- **F-lock-5 — scope of "session."** Single-seat only (matches the v1 single-seat
  constraint), multi-monitor lock surfaces (per-output, echoing the parked per-monitor
  wallpaper decision D-0006/D-0007).
- **F-lock-6 — DPMS / media keys / notifications while locked.** What's allowed on the
  lock surface (shoulder-surf disclosure — same class as the D-0018 pre-auth indicators).

## Reused vs new (leverage summary)

| Reused (already built) | New (this module) |
|---|---|
| doord PAM engine (multi-prompt, FIDO2, zeroize) | `ext-session-lock-v1` client integration |
| IPC seam + peer-cred auth | doord `Reauth` verb (verify, don't spawn) |
| door-theme shader/card surface | per-compositor support + honest bounds |
| privilege-separation posture | lock-specific threat model + break-glass recovery |

## Graduation path (if pursued)

Idea → a **DECISION** (module shape + threat model) + a **PROJECT-SCOPE amendment**
(move "session locking" from Out-of-scope to a committed milestone, dated) →
ROADMAP milestone (`door-lock` + doord `Reauth` + per-compositor matrix) → build
behind the usual reversibility/verification discipline. Critical (privilege boundary
+ new lockout mode), so it pre-ratifies before code. Archive this file to
`IDEAS/ARCHIVED/` on graduation with a pointer to the DECISION.
