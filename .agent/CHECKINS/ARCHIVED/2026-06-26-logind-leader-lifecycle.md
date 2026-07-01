# Check-in — plan A (daemon = logind leader) breaks the multi-login lifecycle

**Date:** 2026-06-26
**Severity:** Critical (session-handoff contract / VT-seat ownership)
**Status:** open — blocks declaring M2 done

## What the live run proved (the win)

D-0004's wiring works end to end for one login. On a real VT (tty4, seat0), after
a real PAM auth, doord opened a logind session via `pam_systemd` and spawned the
chosen session: `loginctl` showed **session 16 on seat0/vc4**, Type=wayland,
Class=user, Service=doord, running as `uid=1000(stephen)` (user's groups, not
root's), on `/dev/tty4` (controlling-tty handoff), with `XDG_SESSION_ID=16`,
`XDG_SEAT=seat0`, `XDG_VTNR=4`, `XDG_RUNTIME_DIR=/run/user/1000`, `LD_PRELOAD`
unset, sanitized PATH. The privilege drop, env merge (R3), VT/tty handoff
(S10/S11), and logind registration (S9) are all confirmed live.

## The defect

After the session process exited, doord called `pam_close_session`
(`pam_unix(doord:session): session closed for user stephen`) — yet **session 16
did not go away**. `systemctl status session-16.scope` shows it still alive,
containing **the daemon itself** (pid 18968):

```
CGroup: /user.slice/user-1000.slice/session-16.scope
        └─18968 /home/stephen/Projects/door/target/debug/doord
```

`pam_systemd` migrates the **calling process** (the daemon, since plan A makes the
daemon the PAM caller / session leader) into the new session's scope. Because the
daemon is long-lived and is the leader, logind keeps the session open regardless
of `pam_close_session`. Worse, the daemon is now *in* a session, so the next
`open_session` it makes will be skipped/refused by `pam_systemd` ("already running
in a session"). **A long-lived daemon as leader can register exactly one logind
session per process lifetime** — incompatible with a login manager that must
re-greet and re-login.

This is the N5 bound from D-0004, but its severity was mis-rated: it is not merely
weaker daemon↔session isolation, it is a functional break of the multi-login
lifecycle.

## Options

- **B — Per-login worker is the leader (greetd model).** Fork a per-login child
  that owns the PAM transaction (or at least `open_session` + the session exec) so
  the **child** is the session leader. When the session exits the scope empties and
  logind removes the session; the daemon never enters any session scope and can
  serve login after login. This is what D-0004 listed as plan B. Larger refactor:
  the PAM session lifecycle moves out of the daemon into a forked process.
- **B-lite — Fork only the leader, keep auth in the daemon.** Daemon authenticates
  (as now), then `fork`s; the child calls `open_session` (child = leader) →
  setsid/tty/privdrop → exec. Daemon `waitpid`s. Session closes when the leader
  (child) exits — the normal logind model — so explicit `pam_close_session` from
  the daemon's context no longer applies (changes S12). Smaller than full B but
  changes the close semantics and uses a manual `fork` rather than `Command`.
- **A + re-exec — keep daemon as leader, re-exec the daemon between sessions.**
  Accept one session per daemon lifetime and have the daemon re-exec itself after a
  session ends so the fresh process is outside any session scope. Smallest code
  change; but it makes the re-greet path a process restart and leaves a window /
  sticky-session edge cases. Weakest.

## Recommendation

Move to **B-lite** (or full **B**): the leader must be a per-login process, not the
daemon. The live run vindicates the plan-B alternative. Reopen the D-0004
leadership decision (it is Binding and Critical) before implementing.

---

**Resolved 2026-06-26 → D-0005** (per-login session worker = leader). Archived.
