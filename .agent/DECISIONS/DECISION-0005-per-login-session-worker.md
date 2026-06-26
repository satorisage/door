# DECISION-0005 — Per-login session worker is the logind session leader

**Status:** Binding
**Date:** 2026-06-26
**Ratified:** 2026-06-26
**Project:** door
**Supersedes:** the leadership clause of D-0004 (plan A, "daemon = session
leader") and its accepted bound N5.

## Context

D-0004 chose `pam_systemd` for logind registration and, for the *process
leadership* sub-fork, plan A: the long-lived daemon holds the PAM context and is
itself the logind session leader. The first live run (2026-06-26, on real VT
tty4/seat0) proved the rest of D-0004 correct — uid/gid drop, env merge, VT/tty
handoff, and the logind registration all worked — **but exposed a functional
defect in plan A** (see `CHECKINS/2026-06-26-logind-leader-lifecycle.md`):

`pam_systemd` migrates the **calling process** into the new session's cgroup
scope. With plan A the caller is the long-lived daemon, so:

- the daemon (pid 18968) was moved into `session-16.scope` and stayed there;
- `pam_close_session` ran, yet logind kept the session because its leader (the
  daemon) never exits — `loginctl` still listed session 16, scope = `{daemon}`;
- the daemon, now *in* a session, cannot open another — `pam_systemd` skips/
  refuses a second `open_session` ("already in a session").

So **plan A registers exactly one logind session per daemon lifetime** — fatal
for a login manager that must re-greet and re-login. N5 ("daemon in scope") was
mis-rated as merely weaker isolation; it is a lifecycle break.

## Decision

**The logind session leader is a per-login worker process, not the daemon
(greetd's model).** For each greeter connection the daemon **re-execs itself** as
a short-lived session worker (`/proc/self/exe session-worker`) connected only by a
control socketpair. The worker:

1. owns the **entire** PAM transaction for that login — `authenticate` +
   `acct_mgmt` + `open_session` + `close_session` + `pam_end` — in one process, so
   the context is never forked mid-transaction and auth→session module data (e.g.
   `pam_gnome_keyring`/`pam_kwallet` keyring unlock) is preserved;
2. is the process that calls `open_session`, so **it** is the logind leader;
3. spawns the session (`spawn::launch`) as its child, waits on it, then closes the
   PAM session and exits.

When the worker exits the session scope empties and logind reaps the session; the
**daemon never enters any session scope** and can serve login after login.

### Trust-boundary placement

All greeter wire-protocol framing stays **solely in the daemon** (D-0003: the seam
is the trust boundary). The worker never reads a greeter byte — the daemon
re-execs it with the greeter socket closed (it is `O_CLOEXEC`, so `exec` drops it;
only the control socket fd is passed). The PAM **conversation** is proxied: the
worker emits structured `Prompt`/`Info`/`Error` events over the control socket; the
daemon turns them into `AuthPrompt` frames to the greeter and relays the
`AuthReply` back as a control `Reply` (carrying the zeroizing `Secret`). The
worker speaks only a small, private daemon↔worker codec, never the wire protocol.

### Identity binding preserved

The worker authenticates a username and starts the session **as that same user**;
the daemon's `Start` relay carries only a `session_id`, never an identity (S2
unchanged). The daemon gates `Start` on the worker having reported auth success.

## Alternatives considered

- **B-lite (fork the leader, keep auth in the daemon).** The child opens the
  session on a **fork-copied** libpam handle (libpam is not contractually
  fork-safe mid-transaction) and then `exec`s away, so no process cleanly calls
  `close_session` — every session module's close hook is skipped and teardown
  leans on logind's implicit reaping. Rejected: fragile, and abandons clean close
  in the root TCB.
- **A + daemon re-exec between sessions.** Keep daemon-as-leader, accept one
  session per lifetime, re-exec the daemon after each session. Rejected: leaves a
  sticky `session-N.scope` until re-exec, a restart race window, and turns
  re-greet into a process bounce.
- **Two PAM transactions (auth in daemon, fresh session-only open in a child).**
  Correct on lifecycle and lighter than full B, but splits auth and session into
  separate transactions, losing auth→session continuity (keyring unlock). Rejected
  for a real login manager.

## Consequences

- New `worker` module (the re-exec'd session worker) owns the PAM context and the
  control codec; `main.rs` dispatches `session-worker` mode.
- `pam.rs` daemon side: the in-process `PamLogin` is replaced by a worker-backed
  `Login` impl that forks/re-execs the worker and proxies the conversation. The
  `Login`/`LoginFactory` trait seam and the in-process `ScriptedLogin` test double
  are kept (the production impl swaps; tests are unchanged in shape).
- `pam_systemd` now migrates the **worker** (correct) into the session scope; the
  session closes when the worker exits.
- Threat model: **S12 rewritten** (close happens in the worker, which both opens
  and closes the session and exits when the session ends), and **N5 removed** —
  the daemon is no longer in any session scope.
- The `O_CLOEXEC` greeter socket becoming unreachable in the worker is a
  *strengthened* boundary (a new control, not just a bound): the worker physically
  cannot touch greeter bytes.
- D-0004 otherwise stands: `pam_systemd`, the PAM-env merge (R3), seat/VT sourcing
  (R4), and the VT/controlling-tty handoff (R5) are unchanged.
