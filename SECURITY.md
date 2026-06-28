# Security Policy

door is a login manager: a privileged daemon (`doord`) that owns PAM authentication,
the seat/VT, and session handoff. Security reports are taken seriously.

## Reporting a vulnerability

**Please do not open a public issue for security vulnerabilities.**

Use **GitHub's private vulnerability reporting** instead:
*Security → Report a vulnerability* on
<https://github.com/satorisage/door/security/advisories/new>.

If that's unavailable, email **stephen.redding31@gmail.com** with the details and
`door security` in the subject. Expect an acknowledgement within a few days.

Please include: affected version/commit, your environment (distro, session, whether
door was the active DM), and a clear reproduction or the threat scenario.

## Scope

Highest-value reports concern the **trusted computing base** — `doord` and the
greeter↔daemon IPC:

- Privilege escalation, or anything that lets the *unprivileged* greeter gain
  authority it shouldn't (the greeter must never be able to become root).
- Authentication bypass, credential leakage, or env/seat/VT handling flaws.
- IPC issues: peer-credential checks, socket permissions, message handling.

The greeter's *appearance* (shaders, themes) is not security-sensitive on its own,
but a crash/DoS of the greeter that blocks login is still worth reporting.

## Status

door is **early-alpha (v0.1.x)** and has not had an independent security audit. Run
it knowing that; the install-disabled-by-default design and the tested two-command
revert exist so a problem can't lock you out of your machine.
