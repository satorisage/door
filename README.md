# door

A beautiful, security-first **login manager** (display manager) for Linux.

door is the program that owns the screen before any user session exists and
decides which session begins — a *full* login manager (it owns authentication,
seat/VT, and the session handoff), paired with a Wayland-native greeter that is
genuinely gorgeous. The headline is the security model, not the wallpaper.

## Architecture (the cornerstone)

door is **privilege-separated**, deliberately, in the SDDM/greetd lineage:

- **`doord`** — a small *privileged* daemon: PAM auth, `logind` (and optional
  `seatd`) seat/VT management, session discovery + spawn, and a local IPC server.
  This is the trusted computing base; it stays minimal, drops privileges, and is
  the only thing that ever touches credentials or root.
- **`door-greeter`** — an *unprivileged* Wayland UI process. It renders the
  beautiful part and holds no authority beyond "ask `doord` to try these
  credentials" over a peer-cred-checked Unix socket. A compromised greeter is
  not root.

The whole project is governed under `.agent/` (the dotagent system). Start with
[`.agent/PROJECT-SCOPE.md`](.agent/PROJECT-SCOPE.md) and
[`.agent/REPORTS/project-brief.md`](.agent/REPORTS/project-brief.md).

## Status

Pre-implementation. M0: scope locked; two Critical architecture decisions open
(broker strategy, implementation language) — see `.agent/CHECKINS/`.

> "The door to your system."
