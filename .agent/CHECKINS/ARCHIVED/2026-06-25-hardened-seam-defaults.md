# CHECK-IN — Hardened seam & TCB security defaults (proposed)

**Date:** 2026-06-25
**Status:** Awaiting ratification → becomes DECISION-0003 on confirm
**Gates:** M1 IPC server (the seam carries credentials; must be specified before built)
**Criticality:** Critical — this is the trust boundary (SCOPE §2)

Stephen's directive: *"select the utmost SECURE choice for all defaults … think
enterprise-level security."* These are the maximally-secure defaults for the
greeter↔core seam and the privileged daemon. Each fork resolved toward the
smaller attack surface / bounded resource / least privilege.

---

## Transport (greeter → daemon framing)

- **T1 — Length-prefixed JSON.** `u32` **big-endian** length header + `serde_json`
  body. Self-describing, journal-debuggable, serde already a dep.
- **T2 — Hard max frame 64 KiB.** Daemon reads the length, and if `len > MAX`
  closes the connection *before allocating*. Bounded allocation — an untrusted
  greeter cannot induce a large alloc.
- **T3 — `#[serde(deny_unknown_fields)]`** on every greeter→daemon type. Extra
  or malformed fields are rejected, never silently ignored.
- **T4 — Per-message read timeout.** An idle/slowloris greeter mid-frame is
  dropped, not held. No unbounded blocking read on the TCB.

## Socket & authorization

- **S1 — Pathname socket, not abstract.** `/run/doord/door.sock`. Abstract-
  namespace sockets bypass filesystem permissions → rejected. Parent dir
  `/run/doord` is `root:root` `0700`.
- **S2 — Socket perms `root:<greeter-group> 0660`.** Only root and the greeter
  user can connect at the filesystem layer.
- **S3 — `SO_PEERCRED` gate on accept.** Require `uid == greeter_uid`; any other
  peer → immediate close + journal line. Authorization check layered over S2
  (defense in depth — perms can be misconfigured; the cred check still holds).
- **S4 — Single connection.** Exactly one greeter at a time; a second concurrent
  connect is refused. One seat, one greeter — removes connection-race ambiguity
  and shrinks surface.

## Secrets hygiene (TCB)

- **C1 — Redacted secret type.** The credential field (`AuthReply.response`)
  becomes a `zeroize`-backed wrapper whose `Debug` renders `<redacted>`, never
  the contents.
  > **Code finding (actionable now):** today `Request` derives `Debug` over a
  > plaintext `response: String`. A single `{:?}`/log of a `Request` would print
  > the password to the journal. This default fixes it.
- **C2 — Zeroize on drop.** Credential buffers wiped immediately after the PAM
  step consumes them; never to disk, never logged, held only across the one PAM
  call.
- **C3 — Errors never echo frame bodies.** Framing/deserialize errors log the
  error *kind*, never the offending bytes (a malformed frame may contain a
  mistyped password).

## Daemon hardening (defense in depth)

- **H1 — `PR_SET_NO_NEW_PRIVS`** set early: no `execve` can ever gain privilege.
- **H2 — `PR_SET_DUMPABLE = 0`**: no core dumps / no `ptrace` attach of the
  credential-handling daemon.
- **H3 — Sanitized session environment** built from an explicit allowlist, never
  inherited from the daemon's own env.
- **H4 — seccomp-bpf allowlist + Landlock FS bounding** as the daemon's default
  posture. *Implemented in M5 (hardening pass)*, ratified now so the daemon is
  built toward it — syscalls aren't added casually.
- **H5 — Privilege model.** Daemon runs as root (PAM/shadow, logind, VT require
  it), does the auth conversation privileged, then at session spawn drops with
  the correct ordering: `setresgid` → `initgroups` → `setresuid`, supplementary
  groups cleared.

## Extensibility (secure-by-construction, not lax parsing)

The seam must evolve for a decade without re-opening the trust boundary. The
secure way to be extensible is **explicit version negotiation + additive typed
variants**, never "accept whatever shows up."

- **E1 — Version handshake.** `PROTOCOL_VERSION: u32` constant. The greeter's
  first message MUST be `Request::Hello { protocol_version }`; the daemon replies
  `Welcome { protocol_version }` or `Incompatible { daemon_protocol_version }`
  and closes. The seam never carries a credential before versions agree.
- **E2 — Enums are the extension point.** New flows = new `Request`/`Response`
  variants and new `AuthPrompt` kinds (PAM already drives the conversation
  shape). Adding a variant bumps `PROTOCOL_VERSION`; the handshake guarantees a
  peer never receives a variant its version can't parse. No flag soup, no
  untyped maps.
- **E3 — Strict IN, lenient OUT (reconciles T3).** `deny_unknown_fields` applies
  to **greeter→daemon** types only — untrusted input to the TCB is parsed
  strictly and rejects anything unexpected. **Daemon→greeter** types stay
  forward-lenient so a not-yet-updated greeter degrades gracefully against a
  newer daemon. Strictness is a security property; it lives on the dangerous
  direction, where it costs nothing in extensibility.

## Auth robustness

- **A1 — Minimum auth delay.** A constant floor (~1 s) on every auth *result* to
  blunt timing oracles; lockout/backoff delegated to the PAM stack
  (`pam_faillock`), not reimplemented.

## Forward — glamour with no TCB cost (noted, not ratified here)

- **G1 — Greeter GPU.** The beautiful greeter targets a GPU-accelerated Vulkan
  surface (wgpu) — the "sickest GPU" glamour — but lives entirely in the
  **unprivileged** greeter (M4), firewalled from the TCB by this seam. Toolkit
  stays a Loose decision; noted only to confirm the glamour has a home that
  costs no security.

---

## On confirm

1. Promote this to `DECISION-0003 — Hardened seam & TCB defaults` (Binding),
   update `DECISIONS/README.md` + `PROJECT-STATE.md` §1.
2. Apply **C1/C3** to the `protocol` crate now (redacted secret type, strict
   serde), mark the protocol task done.
3. Build the **IPC server** (T1–T4, S1–S4) as the next M1 task.
