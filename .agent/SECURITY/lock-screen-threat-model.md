# Threat model — the session lock screen (door-lock)

**Scope of this model:** the `door-lock` session locker — an
`ext-session-lock-v1` Wayland client of the user's running compositor that
blanks a live, authenticated session and reveals it again only on a PAM
reauthentication of the caller's *own* uid, performed by a verify-only doord
`Reauth` IPC verb (per **DECISION-0019**, M10). It picks up where the greeter's
auth/spawn models leave off and covers the **opposite** surface: not a pre-auth
login on an empty VT door owns (`auth-path-threat-model.md`,
`session-spawn-threat-model.md`), but the **walk-up / evil-maid** surface against
a machine that is already logged in — the locker sits *on top of* a session
holding the user's unlocked keys, agents, and running apps.

**Ratified shape (2026-07-03, D-0019 — Binding, Critical):**

- **Protocol: `ext-session-lock-v1` ONLY** (F-lock-1). The *compositor* owns the
  blank/lock surface, so a crashed/killed/exploited door-lock leaves the screen
  blanked, never revealed. No insecure layer-shell fallback is shipped.
- **Auth: a verify-only doord `Reauth` verb** (F-lock-2). doord runs the PAM
  conversation for the supplied credentials **pinned to the socket peer-cred
  uid**, returns allow/deny, and never opens a session or enters a session scope.
  PAM never runs in door-lock.
- **Recovery: TTY, no daemon unlock lever** (F-lock-4). The crash-safety floor +
  a documented two-command VT recovery; a root-reachable force-unlock verb is
  deliberately **not** built (it would be the exact attack surface a locker
  exists to deny).
- **Disclosure: mirror the greeter** (F-lock-6). Themed sky + clock + prompt +
  the D-0018 default-off indicators; never notifications, media, or session
  content.

This model is the M10 "lock threat-model file" deliverable — the D-0019
"Lock-specific threat model" section expanded to a standalone document. The
daemon-side `Reauth` verb is being built in parallel; §8 holds the
`file:line` enforcement citations, filled when that branch integrates.

Serves Scope Principle 1 (security dominates), 2 (privilege separation), 7
(threat-model-first), and the hard constraints (privilege separation
non-negotiable; `No network, ever`; secrets zeroized).

---

## 1. Assets

The defining property of this surface: unlike the greeter, whose failure exposes
a *blank pre-auth screen*, a locker fails onto **real, unlocked secrets**. The
assets are everything a live session already holds decrypted at the moment it is
locked.

- **A1 — The live session itself.** Interactive control of an authenticated
  session running as the user. Reaching it *is* the breach; every other asset
  follows from it.
- **A2 — Unlocked key material and agents.** SSH/GPG agents with keys already
  loaded, an unlocked keyring/secret-service, mounted encrypted volumes, logged-in
  application credentials, browser sessions. All decrypted and resident before the
  lock — the locker is the only thing between a walk-up attacker and them.
- **A3 — On-screen content.** Whatever was displayed when the session locked
  (documents, chats, terminals). The lock surface must cover **every** output and
  the compositor must blank anything uncovered — no monitor may keep painting the
  session behind the lock.
- **A4 — The uid-verification authority (small, but abusable).** The ability to
  ask root doord "verify this credential." Narrow by construction (verify-only,
  own-uid), but if it could be widened it becomes a brute-force / cross-user
  oracle — so its *narrowness* is itself an asset to protect (§7).

## 2. Adversary model

- **P1 — The walk-up attacker (load-bearing persona).** Physically at the
  keyboard of a locked-but-logged-in machine. Can type, plug in USB, mash input,
  crash or `SIGKILL` door-lock if they find a way, VT-switch, and wait. Cannot
  present the user's credential or second factor. The claim under test: **P1
  cannot reach A1–A3 without the user's credential**, and cannot turn A4 into an
  oracle.
- **P2 — The evil maid.** P1 with unsupervised time and the intent to leave the
  session reachable later (tamper, then walk away). Same input-plane powers as P1;
  the added concern is a *persistent* bypass, not a one-shot reveal.

**Explicitly out of the adversary model** (Principle 6/9 — honest bounds; each is
elaborated as a bound in §5):

- **An attacker who already has code running *inside* the session before it
  locks.** The locker fronts a session that is already trusted-as-the-user; it is
  an access-control surface at the glass, not a session sandbox. Malware present
  pre-lock is P-in-session, out of scope.
- **The bench / DMA / cold-boot attacker.** Physical RAM extraction, DMA over a
  debug/Thunderbolt port, cold-boot key recovery. This is a hardware/firmware
  threat class; a software locker cannot and does not address it. The modeled
  threat is the **walk-up keyboard**, not a disassembled machine.

## 3. Trust boundaries

- **TB1 — The compositor owns the lock surface (`ext-session-lock-v1`).** door-lock
  requests the lock; the **compositor** enforces it and holds the blank. This is
  the structural crash-safety boundary: if door-lock crashes, is killed, wedges,
  or is exploited, the compositor keeps the session blanked — the failure mode is
  "stays locked," never "reveals." door-lock is *not* trusted to keep the screen
  covered; the compositor is. This is why the protocol was chosen over a
  layer-shell overlay (which reveals the session if the overlay process dies).
- **TB2 — door-lock is unprivileged and holds no authority beyond "verify these
  creds for my own uid."** It runs inside the user session, as the user. It has no
  root, touches no PAM, and cannot decide an auth verdict — it can only *ask*
  doord and *relay* the yes/no. It mirrors the greeter's posture: an untrusted
  presenter in front of a privileged verifier.
- **TB3 — PAM runs only in root doord, behind the `Reauth` verb.** The one
  audited PAM engine (FIDO2 / multi-prompt / min-failure-delay, reused from the
  login path) lives in the privileged daemon. door-lock never links PAM, never
  reads `/etc/shadow`, never drives `/dev/hidraw*`. Credential verification is on
  the far side of the IPC seam from the locker.
- **TB4 — The IPC seam is untrusted input to the TCB.** door-lock's `Reauth`
  request is untrusted bytes reaching root doord, exactly as the greeter's frames
  are. The seam's peer-cred check is the load-bearing control: the *target uid* is
  derived from `SO_PEERCRED` on the connection, never from anything in the request
  body (§7, RI1).

## 4. What door-lock defends against

| # | Threat | Control |
|---|---|---|
| D1 | **Walk-up / evil-maid reaches the session (P1/P2).** An attacker at the keyboard of a locked, logged-in machine tries to reach A1–A3 without the credential. | **PAM reauth in root doord, never in door-lock.** The screen stays covered by the compositor (TB1) until doord returns an allow for a correct credential (+ second factor, if configured). door-lock never sees a success it could forge — the greeter's `AuthSuccess`-forgery guard extends to `Reauth` (RI4). No credential → no reveal. |
| D2 | **Locker compromise → session exposure.** door-lock is exploited, crashed, or `SIGKILL`ed by the attacker to try to tear down the cover. | **Structural, via the protocol (TB1).** Because the *compositor* owns the lock surface, a dead/exploited door-lock leaves the screen blanked. Locker compromise ≠ session exposure by construction — the worst a broken locker does is *stay locked* (a §6 availability cost), not reveal. This is the single property that makes a locker trustworthy, and the reason no layer-shell fallback ships. |
| D3 | **Cross-user auth abuse / brute-force oracle.** A locker running as user A (or any client on the seam) asks doord to verify or guess user B's password — turning `Reauth` into an offline-style oracle against another account. | **uid bound from `SO_PEERCRED` (RI1).** `Reauth` derives its target uid *only* from the connection's peer credentials, never from a client-supplied username. A locker for A can verify only A. There is no request field that names a different user, so no cross-user oracle exists to build. |
| D4 | **Online password guessing.** P1 types candidate passwords at the lock as fast as the input plane allows. | **Min-failure-delay + coarse reporting (RI7).** `Reauth` reuses the login path's minimum-failure-delay, rate-limiting online guessing, and reports only a generic deny (no account-existence or reason leak). Same anti-guessing posture as the greeter. |

## 5. What door-lock explicitly does NOT defend against (honest bounds)

Naming what is out is doctrine here (Principle 6/9): a locker that overclaims is
worse than one with honest limits.

- **N1 — A compromised live session (malware inside the session before lock).**
  door-lock fronts a session that already holds A2 decrypted. Code that was
  running *as the user before the lock* can read those secrets directly, screen-
  scrape, or keylog — it is inside the boundary the locker guards. The locker is an
  access-control surface **at the glass**, not a session sandbox; the session's own
  sandboxing (portals, app confinement) is a separate, out-of-scope concern. This
  is the sharpest and most important bound: **the locker protects a walk-up
  attacker out, it does not protect a session that is already owned.**
- **N2 — Compositors without `ext-session-lock-v1`.** The crash-safety guarantee
  (TB1) is a property of the protocol; a compositor that does not implement it
  cannot provide it. GNOME/Mutter (which ships its own locker) and exotic WMs
  lacking the protocol are a **named, unsupported bound** (F-lock-1) — door-lock
  **declines to lock them** rather than ship a layer-shell fallback that would
  reveal the session on crash. No insecure fallback is shipped: the honest limit
  is "we do not lock this compositor," never "we lock it weakly." The unsupported
  list is installer-printed (M10 recovery docs).
- **N3 — DMA / cold-boot / physical-RAM attacks.** A software locker cannot defend
  the contents of RAM against an attacker who extracts or images it (bench access,
  DMA over a debug port, cold-boot). Out of scope by adversary model (§2): the
  modeled threat is the walk-up keyboard, not a disassembled machine. Mitigation,
  where wanted, is disk/RAM encryption and firmware/port policy — orthogonal to
  door.

## 6. Residual risk — the wedged-unlock trap

The deliberate cost of the design. Because door-lock has **no** root break-glass
unlock lever (F-lock-4), a locker that fails to *unlock* — accepts no valid
credential (a bug, a broken PAM stack, a lost second factor) — traps the user out
of a **live session with unsaved work**. This is a real availability cost, and it
is accepted knowingly:

- **Recovery is the documented TTY path.** VT-switch to a text console → log in →
  kill/restart door-lock; the compositor accepts a fresh lock client, and the
  crash-safety floor (TB1) means the session stayed protected the whole time. Two
  commands, installer-printed, in the door-lock spirit of Principle 4
  (installer-revert discipline). The M10 recovery docs own the exact commands.
- **Why no daemon force-unlock verb.** A doord-side "force-unlock" would be a
  *standing, root-reachable unlock lever* — precisely the attack surface a locker
  exists to deny. Any P1/P2 (or any bug reachable on the seam) that could invoke it
  would bypass the credential entirely. The design trades a small, TTY-recoverable
  friction cost for **removing that lever from existence** (Alternatives, D-0019 —
  "A doord force-unlock / break-glass verb: rejected"). The residual is
  availability-under-bug, not a confidentiality hole; the crash-safety floor keeps
  the failure on the safe side (locked-out, not exposed).

## 7. `Reauth` verb invariants

The load-bearing security properties the doord `Reauth` verb **must** enforce.
These are the contract the daemon-side build satisfies; each maps to an
enforcement citation in §8. They are stated as invariants, not aspirations —
a violation of any one is a Critical defect.

- **RI1 — uid from `SO_PEERCRED` only.** The target uid is derived from the
  connection's peer credentials, never from a client-supplied username or any
  request-body field. (This is the durable seam rule D-0019 adds: *any* credential
  path on the seam binds its target uid from `SO_PEERCRED`.) Defeats D3.
- **RI2 — verify-only: never opens a PAM session.** `Reauth` runs the PAM
  *authentication* conversation and returns a verdict. It does **not** call
  `pam_open_session`, does not register with logind, and does not become a session
  leader. It is orthogonal to the login/spawn path — no session lifecycle is
  touched.
- **RI3 — no privilege handoff: never spawns, execs, takes a VT, or drops-and-execs.**
  `Reauth` performs no fork→exec, seizes no VT/controlling-tty, and never drops
  privilege to launch anything. It is a pure predicate over credentials. The entire
  session-spawn machinery (privdrop, VT handoff, env build) is not on this path.
- **RI4 — no reachable forgery of an "allow."** The allow/deny verdict is decided
  **only** by the PAM result inside root doord. There is no client-supplied field,
  malformed frame, error path, or default that yields "allow" without a genuine PAM
  success. door-lock relays the verdict; it cannot manufacture one. Extends the
  greeter's `AuthSuccess`-forgery guard. Defeats D1.
- **RI5 — credentials zeroized.** Supplied secrets (password, FIDO2 PIN, typed
  factors) live in zeroizing buffers and are wiped after use, on both success and
  every failure/error path. No secret lingers in doord memory past the
  verification. (Hard constraint: secrets zeroized.)
- **RI6 — no network, ever.** `Reauth` performs no network I/O — no remote
  validation, no OTP-against-a-cloud, no lookup off-box. Local PAM only, over the
  local seam. (Hard constraint: `No network, ever`.)
- **RI7 — failure-padding + coarse reporting.** `Reauth` reuses the login path's
  minimum-failure-delay (rate-limiting online guessing, D4) and returns a **generic
  deny** — no account-existence signal, no reason string. The minimum-delay is a
  *floor*, not a constant-time equalizer, so a residual timing difference remains
  between the no-such-user and wrong-password paths (**§9 R2**) — neutralized here
  by RI1, because the only account whose existence the timing can reveal is the
  caller's own peer-cred uid, which it already knows. Same coarse verdict the
  greeter gets.
- **RI8 — the greeter login path is unaffected.** Adding `Reauth` changes no
  behavior of the existing auth/spawn verbs. `Start`, the auth gate, the per-login
  worker, and the greeter's disclosure posture are untouched; `Reauth` is an
  additive, orthogonal verb sharing the audited PAM engine but not altering the
  login flow (D-0019 Consequences — "Unchanged: the greeter…").

## 8. Enforcement evidence

The daemon-side `Reauth` verb landed on master at commit `8421353` (D-0019 M10).
Each invariant's enforcement point is cited below to the merged tree. These
citations, and the residuals in §9, were established by **adversarial
verification** — four independent review passes at merge: uid-binding/forgery,
PAM/session-boundary/secrets, protocol/concurrency/regression, and a focused
sandbox-fix re-verify. All eight named invariants (RI1–RI8) were confirmed sound;
the sandbox-confinement invariant (RI9) was added after the review found the
initial build left the reauth thread outside the sandbox, and was re-verified
after the fix.

- **RI1 — uid from `SO_PEERCRED`** — `doord/src/reauth.rs:350` (`peer_cred(&stream)`)
  → `:355` (`resolve_uid(cred.uid)`, the sole identity source); enforced
  *structurally* by the wire type — `protocol/src/lib.rs:179` (`ReauthRequest` has
  no username field), `:186` (`Begin` is a unit variant) + `deny_unknown_fields`.
- **RI2 — verify-only, no PAM session** — the relay emits only `WorkerCommand::Auth`
  (`doord/src/reauth.rs:186`), never `Start`; the worker stops at
  `context.acct_mgmt` (`doord/src/worker.rs:452`) and `pam_open_session` is reachable
  only via `Start` (`doord/src/worker.rs:499`), which this path never sends.
  Type-level: `ReauthRequest` (`protocol/src/lib.rs:179`) has no session verb.
- **RI3 — no spawn / exec / VT / privdrop** — same structural argument as RI2: with
  no `Start` emitted, `start_session`/privdrop/VT-seizure are dead code on this
  path. Teardown is a socket `shutdown(Both)` (`doord/src/reauth.rs:261`), no process.
- **RI4 — no reachable "allow" forgery** — `ReauthResponse::Allow` is written at one
  site (`doord/src/reauth.rs:401`), gated on `ReauthOutcome::Allow`, whose only
  producer is `Verdict::Success` (`doord/src/reauth.rs:85`) via a genuine
  `WorkerEvent::Auth` (`:245`). Every worker-death / EOF / malformed / stray-event
  branch fails closed to Deny/Transport (no `_ => Allow`).
- **RI5 — credentials zeroized** — `Reply { response: Secret }`
  (`protocol/src/lib.rs:189`) backed by `Secret` = `ZeroizeOnDrop`
  (`protocol/src/secret.rs:24-26`); wire staging buffers are wrapped in `Zeroizing`
  by the `frame` codec; the daemon never `.expose()`s or logs the plaintext.
- **RI6 — no network** — one local `UnixListener::bind` (`doord/src/reauth.rs:337`) +
  the peer-cred read (`:350`); no new dependency (`Cargo.toml` unchanged in the diff).
- **RI7 — failure-padding + coarse reporting** — `pad_failure(started)` on Deny
  (`doord/src/reauth.rs:403`) reusing `MIN_AUTH_FAILURE` (`doord/src/ipc.rs:141`);
  coarse reason. Floor-only caveat + neutralization tracked in **§9 R2**.
- **RI8 — greeter login path unaffected** — the greeter functions in
  `doord/src/ipc.rs` (`serve`/`handle_connection`/`run_auth`/`run_start`/`bind`) are
  behaviorally unchanged; the only edits are `pub(crate)` exposure of `peer_cred`
  (`ipc.rs:1395`), `pad_failure` (`ipc.rs:1308`), and `MIN_AUTH_FAILURE`
  (`ipc.rs:141`). Greeter/login regression tests pass unchanged.
- **RI9 — the reauth surface is sandbox-confined** *(added by the review)* — the
  reauth listener thread runs inside the supervisor's seccomp filter and Landlock
  ruleset, not outside them: seccomp is installed with TSYNC
  (`seccompiler::apply_filter_all_threads`, `doord/src/hardening.rs:149`) so it
  covers the pre-existing sibling thread; Landlock is applied *before* the thread is
  spawned so it inherits the domain (order in `doord/src/main.rs`: `apply_landlock`
  `:143` → parked reauth-thread spawn `:171` → `apply_seccomp` `:211` → release
  `:234`). The thread parks on an `mpsc` release gate (futex-only) and issues no
  socket syscall until *after* the filter is installed — no boot-time
  unconfined-but-connectable window. Positive control: the TSYNC test
  `all_threads_filter_confines_a_sibling_thread` (`doord/src/hardening.rs`).
- **D2 — locker compromise ≠ exposure (crash-safety)** — _(live: kill door-lock
  under lock, confirm compositor keeps screen blanked — pending the door-lock binary)_
- **N2 — unsupported-compositor decline** — _(behavior + installer-printed list —
  pending the door-lock binary + M10 recovery docs)_

## 9. Verified residuals (deferred, per owner decision 2026-07-03)

Two findings from the adversarial review are **known, accepted, and deferred** —
neither is a confidentiality/integrity bypass; both are recorded here so a future
reviewer does not mistake them for oversights. Owner chose (2026-07-03) to ship the
verb with these documented and revisit R1 when the door-lock client exists to
exercise it.

- **R1 — single-thread unlock-DoS (availability-only, fail-safe).** The reauth
  socket is world-connectable (`0666`; the peer-cred uid is the gate, not the mode)
  and served by a *single, sequential* listener thread (`doord/src/reauth.rs`
  `serve_reauth`) — a deliberate consequence of the no-`clone`-after-seccomp
  constraint (a per-connection thread would need `clone`, which the supervisor
  allowlist omits). Any local uid can therefore head-of-line-block the listener by
  connecting and stalling within the bounded read windows
  (`REAUTH_HANDSHAKE_TIMEOUT` ~30s idle, `REAUTH_REPLY_TIMEOUT` ~120s in an active
  prompt), repeated on reconnect — denying *unlock* to every session while it holds
  the thread. Impact is **fail-safe**: the screen stays locked (no break-in); this
  is an availability DoS on unlock, not a credential bypass, and it fails closed.
  **Deferred:** a concurrency cap / per-uid connection limit / shorter reply window
  is a hardening follow-on, to be weighed against the no-`clone` reality once
  door-lock lands. (Confirmed by two independent review passes.)
- **R2 — RI7 failure-padding is a floor, not a constant-time equalizer.** A
  no-such-user probe (a uid with no passwd entry) short-circuits to Deny at the ~1s
  `MIN_AUTH_FAILURE` floor **without running PAM** (`doord/src/reauth.rs:355` →
  `None` → `:397`/`pad_failure`), whereas a wrong-password deny runs the worker's
  PAM `pam_authenticate` to its ~2s fail-delay. That ~1s gap is an observable timing
  difference, so RI7's "no timing channel between no-such-user and wrong-password"
  is **not** met as literally written, and (unlike the login path, which routes both
  through PAM identically) it is reauth-introduced. **It is not an account-existence
  oracle:** by RI1 the probed uid is always the caller's *own* `SO_PEERCRED` uid, so
  the only account whose existence the timing reveals is the caller's own — which it
  already knows via `getpwuid(getuid())`. The safety rests on RI1, not the padding.
  **Deferred:** route the no-such-user path through a PAM-matched delay if a
  constant-time guarantee is ever wanted. (Confirmed by two independent review passes.)
