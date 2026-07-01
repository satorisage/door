# DECISION-0008 — doord owns the greeter lifecycle (handoff + re-greet)

**Status:** Binding
**Date:** 2026-06-26
**Ratified:** 2026-06-26
**Project:** door
**Supersedes:** the `door-greeter.service` approach (systemd launching the
greeter) from the M6 packaging scaffold. Only `doord.service` remains.

## Context

M6's live install needs a working greeter↔session VT handoff and a re-greet loop
— the deferred N1/N2 lifecycle (M2 threat model §5). The packaging scaffold's
provisional `door-greeter.service` (systemd launches the greeter independently)
cannot do it race-free:

- **Handoff race:** the session worker grabs the VT (DRM master) while `cage` may
  still hold it.
- **Re-greet:** systemd/logind cannot express "run the greeter, *except* while a
  user session holds the VT" — only doord knows when the session starts and ends.

The re-greet requirement decides the owner: whoever brings the greeter *back*
after logout must be doord, so doord should launch it in the first place. This is
greetd's model.

## Decision

**doord owns the greeter lifecycle.** The privileged daemon runs a top-level
**login loop**:

1. **Greet** — launch the greeter. doord opens a *passwordless* PAM/logind session
   for the greeter user (PAM service `door-greeter`) so the host compositor gets
   seat0 DRM/input access, drops to the greeter user, and forks
   `cage -- door-greeter` on the seat VT. The PAM session is held open for the
   greeter's life. (Same machinery as the M2 session worker — a re-exec'd,
   single-threaded process that opens a PAM session, privilege-drops, forks the
   target, and closes the session on exit — minus authentication.)
2. **Serve** — the greeter (door-greeter inside cage) connects to the socket; doord
   serves the login as today (peercred gate, handshake, the per-login auth worker).
3. **Handoff** — on a successful `Start`, doord **terminates the greeter and waits
   for it to exit** (cage releases DRM/VT) **before** the session worker spawns the
   user session onto the now-free VT. Order is load-bearing: greeter dies → VT
   freed → session takes it. No two owners of the VT at once.
4. **Wait + re-greet** — doord waits for the user session (the session worker) to
   exit (logout); the loop repeats → the greeter comes back. A greeter that dies
   *without* a login (crash, power action) also falls through to re-greet, giving
   crash resilience.

systemd runs only `doord.service`; there is no `door-greeter.service`.

## Consequences

- New greeter-launch path in doord (a passwordless `door-greeter` PAM session +
  privilege drop to the greeter user + fork `cage -- door-greeter`), reusing the
  worker/privdrop/PAM machinery (D-0004/D-0005). The greeter user is named via
  `DOORD_GREETER_USER` and resolved (config already does the uid/gid; add the name
  + the launch command).
- `ipc::serve` becomes the **login loop** (greet → accept → serve → on `Start`
  kill-greeter-then-spawn → wait → re-greet) rather than a bare accept loop.
- `dist/systemd/door-greeter.service` is **removed**; the PKGBUILD installs only
  `doord.service`. `dist/pam.d/door-greeter` stays — it is the passwordless PAM
  service doord uses for the greeter session.
- Threat model: a new control covering the handoff ordering (greeter terminated +
  reaped before the session takes the VT) and the greeter session's seat access;
  the deferred N1/N2 move from "deferred" to "modeled".
- Greeter config: `DOORD_GREETER_CMD` (default `cage -- /usr/bin/door-greeter`) and
  the greeter VT come from existing seat/VT config.
- Live install (M6) validates the full greet→login→session→logout→re-greet cycle
  on a real VT with the tested revert in hand.

## Alternatives considered

- **systemd + logind arbitrate** (keep `door-greeter.service`; rely on logind
  session-switching + cage device pause/resume + a restart policy). Rejected:
  depends on brittle cage/logind edge behavior, and re-greet-after-logout cannot be
  expressed in units. (See D-0008's context.)
- **A dedicated greeter-manager process** separate from doord. Rejected for v1:
  adds a process and an IPC seam for no benefit — doord already holds the seat/VT
  authority and the session lifecycle, so it is the right owner.
