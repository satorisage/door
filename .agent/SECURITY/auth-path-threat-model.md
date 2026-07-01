# Threat model — the authentication path

**Scope of this model:** everything from the greeter asking to authenticate a
user, through the IPC seam, to the privileged daemon running the PAM
conversation and returning a verdict. It covers `BeginAuth` → prompt/reply
round-trips → `AuthSuccess` / `AuthFailure`. It does **not** yet cover session
spawn and handoff (that arrives with the session milestone); the privilege-drop
mechanism that handoff will use is reviewed here because it is already written.

This is the security definition-of-done for the privileged-core skeleton. It
states what the auth path defends against, *how* (mapped to the ratified
controls), and — honestly — what it does **not** defend against.

---

## 1. Assets

- **A1 — User credentials** (passwords, OTPs) in transit greeter→daemon and
  during the PAM call. Highest value.
- **A2 — Root authority** held by the daemon. The whole architecture exists to
  keep A2 from being reachable through the greeter.
- **A3 — Authentication verdict integrity.** "Did this user authenticate?" must
  be decided only by PAM in the daemon, never asserted by the greeter.
- **A4 — Account-existence information.** Whether a username exists is
  privileged; leaking it aids targeted attack.

## 2. Trust boundaries

- **TB1 — The IPC seam (primary).** Unprivileged greeter ↔ privileged daemon.
  Everything crossing greeter→daemon is untrusted input to the TCB.
- **TB2 — The PAM/kernel boundary.** The daemon ↔ libpam/`/etc/shadow`/kernel.
  Trusted, but the daemon must invoke it correctly.

## 3. Attacker personas

- **P1 — Shoulder-surfer / opportunist at the physical screen.** Can type, can
  watch. No code execution.
- **P2 — Compromised greeter.** The unprivileged greeter process is fully
  attacker-controlled and sends arbitrary bytes over the seam. **This is the
  load-bearing persona** — the architecture's core claim is that P2 does not
  become root.
- **P3 — Local unprivileged process** owned by another user (or nobody),
  attempting to reach the daemon socket or observe the auth.
- **P4 — Evil-maid with transient physical access** (no persistence assumed in
  this milestone).

## 4. Threats → controls (what we defend, and how)

| # | Threat | Persona | Control (ratified in D-0003 unless noted) |
|---|---|---|---|
| T1 | Greeter sends a giant/garbage frame to exhaust or confuse the daemon | P2 | Length-prefixed framing, **64 KiB cap checked before allocation**; strict `deny_unknown_fields` on inbound types; decode errors never echo bytes. (`protocol::frame`) |
| T2 | A non-greeter local process connects to the socket and tries to auth or scrape prompts | P3 | Pathname socket, `root:greeter 0660` under a `0700` dir; **`SO_PEERCRED` uid gate on accept** — kernel-attested uid must equal the greeter uid, independent of socket perms. (`ipc::peer_cred`) |
| T3 | Greeter forges a "logged in" state to skip auth | P2 | The greeter **cannot** assert success: only the daemon's PAM result produces `AuthSuccess`. The verdict lives behind TB1. (`pam::PamAuthenticator`) |
| T4 | Credential leaks into logs / core dumps / freed memory | P2/P4 | `Secret` type: redacted `Debug`, zeroized on drop; widened to `CString` only inside the conversation callback. `PR_SET_DUMPABLE=0` blocks core dumps & ptrace of the daemon. Detailed PAM errors are journaled but credentials never are. (`protocol::secret`, `hardening`) |
| T5 | Username enumeration via timing (unknown user fails faster than wrong password) | P1/P2 | **Minimum failure delay** floor on the failure path only; PAM itself prompts even for unknown users. Verdict to the greeter is the coarse "Authentication failed", never the PAM detail. (`ipc::pad_failure`, T4) |
| T6 | Online password brute force | P1/P2 | Delegated to the PAM stack (`pam_faillock` via `system-local-login`); door adds the per-attempt delay floor. Door does not reimplement lockout. |
| T7 | Privilege escalation from a spawned child back to root | P2 | `PR_SET_NO_NEW_PRIVS` set at startup; privilege-drop ordering `initgroups → setresgid → setresuid` with post-drop verification and a hard refusal to run a session as uid 0. (`hardening`, `privdrop`) |
| T8 | Environment-injection (`LD_PRELOAD`, hostile `PATH`) into the user session via the daemon's inherited env | P2/P4 | Session env built from an **explicit allowlist**, never inherited from the daemon. (`privdrop::sanitized_env`) |
| T9 | A wedged/silent greeter pins the daemon mid-read | P2/P3 | Per-message read timeout; single sequential connection so one peer can't starve a second. (`ipc`, `READ_TIMEOUT`) |
| T10 | Protocol confusion: a credential or action sent before any handshake | P2 | Mandatory `Hello`/`Welcome` version handshake; no request is honored before it, and no credential should cross until versions agree. (`ipc::handshake`) |

## 5. Explicitly NOT defended in this milestone (honest bounds)

- **N1 — A root-equivalent local attacker.** If the attacker is already root (or
  can `ptrace` as root, or read kernel memory), door offers no defense — it is
  not a goal. The boundary is greeter→root, not root→root.
- **N2 — Malicious PAM modules or a tampered PAM stack.** door trusts
  `/etc/pam.d/doord` and the modules it includes (TB2). Integrity of that stack
  is the OS's responsibility.
- **N3 — Offline attacks on `/etc/shadow`.** Out of scope; door never sees the
  hash, only PAM's verdict.
- **N4 — The plaintext's transient lifetime inside libpam.** Once handed to PAM
  as a `CString`, the secret's lifetime is libpam's; door zeroizes its own
  `Secret` copy but cannot zeroize libpam's internal buffers.
- **N5 — Hardware keyloggers / physical TEMPEST / camera over the shoulder.**
  Physical-layer capture of what the user types is not addressed (P1 can read
  the screen; door masks secret input but cannot stop a camera).
- **N6 — Side channels finer than the failure-delay floor** (cache timing,
  power analysis). Not modeled.
- **N7 — Denial of service from the legitimate greeter user.** If the greeter
  user is hostile it can refuse to present a login; availability against an
  authorized-but-malicious greeter is not a goal (it would defeat the seat).
- **N8 — Session spawn / handoff.** The drop-to-user and session-launch path is
  built next; its threats (respawn loops, handoff races, seat/VT ownership) are
  modeled when that lands. The drop *mechanism* is reviewed above (T7/T8).

## 6. Residual risks accepted

- The min-failure-delay (T5) blunts but does not eliminate timing channels; a
  patient attacker with many samples may still extract signal. Accepted —
  `pam_faillock` (T6) bounds attempt volume.
- N4: a transient plaintext copy exists inside libpam. Accepted as intrinsic to
  using PAM.

## 7. Verification status

- T1, T3, T5(structure), T10 — exercised by tests
  (`protocol::frame::tests`, `doord::ipc::tests`, `doord/tests/ipc_smoke.rs`):
  oversized-frame rejection, no-echo, handshake gating, correct/wrong/cancelled
  conversation outcomes.
- T2, T4(dumpable), T7, T9 — mechanism present and code-reviewed; **live
  verification (peercred rejection of a foreign uid, the root privilege drop,
  the real PAM conversation) requires a root run and is the next demonstrable
  step.**
- T8 — `sanitized_env` allowlist unit-tested (no inherited `LD_PRELOAD`).
