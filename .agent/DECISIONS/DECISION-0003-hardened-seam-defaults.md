# DECISION-0003 — Hardened seam & TCB security defaults

**Status:** Binding
**Date:** 2026-06-25
**Ratified:** 2026-06-25
**Project:** door

## Context

The greeter↔core IPC seam IS the trust boundary (SCOPE §2 / Principle 2), and
D-0001 made it bespoke — so door owns its security review end to end. Before the
seam carries a single credential it must be specified: framing, socket
authorization, secrets handling, and the daemon's hardening posture. Stephen's
directive: *"select the utmost SECURE choice for all defaults — enterprise-level
security"* and *"it needs to be as extensible as possible."* These two pull in
the same direction once extensibility is done by **version negotiation**, not by
lax parsing. This decision gates M1.

## Decision

The maximally-secure default at every fork on the seam and in the daemon. Each
resolved toward smaller attack surface, bounded resources, and least privilege.

### Transport
- **T1** Length-prefixed JSON: `u32` **big-endian** length + `serde_json` body.
- **T2** Hard max frame **64 KiB**; the daemon rejects `len > MAX` *before*
  allocating — bounded allocation against an untrusted greeter.
- **T3** `#[serde(deny_unknown_fields)]` on greeter→daemon types (see E3).
- **T4** Per-message read timeout; an idle/slowloris peer mid-frame is dropped.

### Socket & authorization
- **S1** Pathname socket `/run/doord/door.sock` (not abstract — abstract sockets
  bypass filesystem permissions). Parent `/run/doord` is `root:root 0700`.
- **S2** Socket perms `root:<greeter-group> 0660`.
- **S3** `SO_PEERCRED` gate on accept: require `uid == greeter_uid`; any other
  peer is closed + logged. Layered over S2 (defense in depth).
- **S4** Single greeter connection at a time; concurrent connects refused.

### Secrets hygiene (TCB)
- **C1** The credential field (`AuthReply.response`) is a `zeroize`-backed
  `Secret` whose `Debug` renders `<redacted>` and which zeroizes on drop.
  *(Closes a live finding: `Request` derived `Debug` over a plaintext `String`,
  so any `{:?}` of a `Request` would have logged the password.)*
- **C2** Credential buffers zeroized immediately after the PAM step; never to
  disk, never logged, held only across the one PAM call.
- **C3** Framing/deserialize errors log the error *kind*, never the frame bytes.

### Daemon hardening (defense in depth)
- **H1** `PR_SET_NO_NEW_PRIVS`. **H2** `PR_SET_DUMPABLE = 0`.
- **H3** Sanitized session environment from an explicit allowlist.
- **H4** seccomp-bpf allowlist + Landlock FS bounding as default posture —
  *implemented in M5*, ratified now so the daemon is built toward it.
- **H5** Privilege drop ordering at session spawn:
  `setresgid` → `initgroups` → `setresuid`, supplementary groups cleared.

### Auth robustness
- **A1** Constant minimum auth-delay floor (~1 s) on every result to blunt timing
  oracles; lockout/backoff delegated to PAM (`pam_faillock`), not reimplemented.

### Extensibility (secure-by-construction)
- **E1** `PROTOCOL_VERSION: u32`. The greeter's first message MUST be
  `Hello { protocol_version }`; the daemon answers `Welcome` or `Incompatible`
  and closes. No credential crosses before versions agree.
- **E2** New flows are additive enum variants (`Request`/`Response`/`AuthPrompt`)
  guarded by a `PROTOCOL_VERSION` bump; the handshake guarantees no peer is sent
  a variant its version can't parse.
- **E3** Strict IN, lenient OUT: `deny_unknown_fields` on greeter→daemon types
  only (untrusted input to the TCB); daemon→greeter types stay forward-lenient
  so an older greeter degrades gracefully against a newer daemon.

## Alternatives considered

- **Newline-delimited JSON / binary (postcard).** JSONL needs a manual line cap
  (implicit framing) and binary is opaque at the seam; length-prefixed JSON gives
  declared bounded framing *and* journal-debuggability. Rejected.
- **Abstract-namespace socket.** No filesystem permissions — any process in the
  namespace can connect. Rejected for a pathname socket with strict perms + the
  peercred gate.
- **Extensibility via untyped maps / `flatten` / lenient parsing everywhere.**
  Re-opens the trust boundary to arbitrary fields. Rejected for version-gated
  additive variants (E1–E3).

## Consequences

- The `protocol` crate gains: `PROTOCOL_VERSION`, the `Hello`/`Welcome`/
  `Incompatible` handshake, a redacted `Secret` type (zeroize), `deny_unknown_fields`
  on the inbound direction, and shared length-prefixed framing helpers
  (`MAX_FRAME_BYTES`, frame read/write) so both sides share one wire impl.
- `doord` gains the IPC server: pathname socket with S1/S2 perms, `SO_PEERCRED`
  authorization, single-connection sequential accept, per-message timeout.
- H1–H2/H5 land in the daemon as the privileged core is built; **H4 (seccomp +
  Landlock) is M5**, named here so syscalls are not added casually before then.
- Durable rule: any change that widens what the seam accepts, relaxes a socket
  permission, or adds a daemon syscall requires a superseding decision.
