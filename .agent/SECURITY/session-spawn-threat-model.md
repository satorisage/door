# Threat model — the session-spawn path

**Scope of this model:** everything from an authenticated greeter asking to start
a session (`Start { session_id }`), through the daemon resolving the user and the
session, to the PAM session open (logind registration via `pam_systemd`) and the
fork → VT/controlling-tty handoff → privilege drop → environment build → exec
that hands the seat to the user's session, and finally closing the PAM/logind
session when it exits. It picks up where the auth-path model
(`auth-path-threat-model.md`) left off (its N8) and covers the privilege handoff
itself.

**Updated 2026-06-25 (D-0004):** the seat/VT/logind registration that was
deferred (the old N1) is now built and modeled here — `setsid`, the
controlling-tty handoff, and the `XDG_SESSION_*` / logind session via
`pam_systemd` (S9–S12 below). door registers the session with logind through
PAM's session phase rather than a direct D-Bus call.

**Updated 2026-06-26 (D-0005):** the first live run showed that making the
*long-lived daemon* the logind leader breaks the multi-login lifecycle —
`pam_systemd` migrates the leader into the session scope, so the session never
closes and a second login cannot register. The leader is now a **per-login
session worker**: for each login the daemon re-execs itself as a short-lived
worker that owns the PAM transaction, is the logind leader, spawns + waits the
session, closes the PAM session, and exits. The daemon never enters a session
scope and serves login after login. This rewrites **S12** and removes the old N5.
The worker also tightens the trust boundary (**S13**): it never reads a greeter
byte. What remains out of scope is the session *lifecycle* beyond one clean
open→run→close cycle: respawn backoff, re-greet policy, and concurrent-session
arbitration (the residual N2).

---

## 1. Assets

- **A1 — Root authority** held by the daemon. The spawn is the one place door
  *uses* root to become someone else; a flaw here is a flaw in the whole point of
  the architecture.
- **A2 — Session identity integrity.** "As whom does this session run?" must be
  decided by the authenticated PAM result, never by anything the greeter chooses.
- **A3 — The user session's environment.** It must carry none of the daemon's
  root environment (no inherited `LD_*`, no root `PATH`).

## 2. Trust boundary

- **TB1 — The IPC seam (same as the auth path).** `Start` is untrusted input to
  the TCB. The greeter may send it at any time, with any `session_id`, in any
  order, including without ever authenticating.

## 3. Attacker persona

- **P2 — Compromised greeter** (the load-bearing persona). Fully
  attacker-controlled, sends arbitrary frames. The claim under test: P2 cannot
  start a session **without authenticating**, cannot start one **as another
  user**, and cannot get root authority into the spawned process.

## 4. Threats → controls

| # | Threat | Control |
|---|---|---|
| S1 | Greeter sends `Start` with no prior auth, to spawn a session (a shell) it was never entitled to | **Auth gate.** `Start` is honored only when the connection's `Login` reports an authenticated user (`login.user().is_some()`); otherwise it is refused with a generic error and **no launch occurs**. The authenticated identity lives in the per-connection `Login` (set only when the per-login worker reports a PAM success) and dies with the connection that earned it. (`ipc::tests::start_before_auth_is_refused_and_never_launches`) |
| S2 | Greeter authenticates as itself but asks to start a session **as another user** (confused deputy → privilege/identity escalation) | **Identity binding.** `Request::Start` carries only a `session_id`, never a username. The spawn target is the username PAM authenticated on this connection, resolved fresh via `getpwnam` (`user::resolve`). The greeter has no channel to name a different identity. (`ipc::tests::start_after_auth_launches_the_chosen_session_as_the_authed_user`) |
| S3 | A `setres*id` silently fails and the session is exec'd still holding root | **Drop-or-abort.** The drop runs in `Command::pre_exec` (`spawn::session_setup`, after `setsid` + the VT handoff); `drop_to` verifies the post-drop uid/gid and **refuses uid 0**, returning an error. `Command::spawn` propagates a `pre_exec` error, so a session whose drop did not fully take is **never exec'd** — the launch fails closed. (`privdrop::{drop_to,verify_dropped}`, `spawn::launch`) |
| S4 | Group privileges stranded (uid dropped while still in root's groups) | **Ordering.** `initgroups → setresgid → setresuid`: supplementary + primary groups are set while still privileged, uid last. (`privdrop::drop_to`. See the H5 text-vs-code note in `ROADMAP.md`; both orderings satisfy groups+gid-before-uid.) |
| S5 | Daemon's root environment leaks into the user session (`LD_PRELOAD`, root `PATH`) | **Allowlist ∪ PAM env, never inherited.** The child environment is `env_clear`ed and rebuilt from `privdrop::sanitized_env` (HOME/USER/LOGNAME/SHELL + a fixed PATH) bound to the target, then the **PAM session env** (logind's `XDG_SESSION_ID` / `XDG_RUNTIME_DIR`, harvested from `pam_getenvlist`) is merged over it. Only the allowlist and what PAM explicitly produced are present — the daemon's own `PATH`/`LD_*` are never copied, because nothing is inherited. (`spawn::build_command`; `spawn::tests::{command_environment_is_the_sanitized_allowlist_only_without_pam_env, pam_session_env_is_merged_and_wins_on_overlap}`) |
| S6 | Greeter starts a `session_id` that does not exist, or a stale one, to probe or crash the daemon | **Re-resolve + refuse.** `Start` re-runs discovery and matches by `id`; an unknown id is a generic "no such session" refusal and the connection keeps serving. The `Exec` is never taken from the greeter — only from the daemon-side discovered entry. (`ipc::run_start`) |
| S7 | A spawn error reveals privileged detail to the login screen (e.g. whether an account exists, via a distinct error) | **Generic refusal.** Every `LaunchError` (user-not-found, exec failure) maps to one opaque "could not start the session" to the greeter; the detail is journaled only. (`ipc::run_start`, `spawn::LaunchError`) |
| S8 | Non-async-signal-safe work in the forked child deadlocks (the classic fork-in-a-threaded-process allocator deadlock) | Both forks are from **single-threaded** processes. The daemon serves one greeter sequentially with no worker threads, and it re-execs the worker immediately (the `pre_exec` does only async-signal-safe `dup2`/`fcntl`). The worker is a fresh `exec` (no threads) when it forks the session, whose `pre_exec` (`initgroups`/VT setup) is therefore safe — the same shape `login(1)`/`sshd` use. (`ipc::serve` sequential; `pam::spawn_worker`; `spawn::launch`.) |
| S9 | Greeter dictates the seat or VT the session registers on (register on another seat's VT, or claim a VT it shouldn't) | **Seat/VT are door's, not the greeter's.** `XDG_SEAT`/`XDG_VTNR` are sourced from the daemon's own logind session environment (the VT door runs on), with config overrides (`DOORD_SEAT`/`DOORD_VTNR`) for dev — never from any greeter frame (`Start` still carries only a `session_id`). They are `pam_putenv`'d before `pam_open_session`, so logind registers the session where **door** is. The daemon passes them to the worker; the greeter never supplies them. (`config::SeatTarget`, `worker::start_session`) |
| S10 | The user session inherits the daemon's stdio (the daemon's pipes/journal fds become the session's std streams — an information channel between TCB and session) | **Controlling-tty handoff.** In `pre_exec` the child `setsid`s into its own session, then (when a VT is known) opens `/dev/tty<vtnr>`, claims it with `TIOCSCTTY`, and `dup2`s it onto std{in,out,err} — replacing the inherited stdio with the seat's VT. A handoff failure returns an error from `pre_exec`, so the session is **never exec'd off its seat** (fail-closed). (`spawn::take_controlling_tty`, `spawn::session_setup`) |
| S11 | VT acquisition ordered after the privilege drop, so the child can no longer open the root-owned VT device (handoff silently skipped, or grabs the wrong tty) | **Ordering.** `setsid` → open VT + `TIOCSCTTY` → dup stdio happen **while still privileged**; the privilege drop (`drop_to`) is the **last** step before exec. The VT device is opened with the authority to open it, then privilege is shed. (`spawn::session_setup`) |
| S12 | A long-lived leader traps the daemon in a session scope, so the session never closes and no second login can register (the defect the first live run exposed under the old daemon-as-leader model) | **Per-login worker is the leader (D-0005).** The process that calls `pam_open_session` is a short-lived worker the daemon re-execs per login; it is the logind leader, spawns + waits the session, then closes the PAM session (`pam_close_session` + `setcred(DELETE_CRED)` via `worker::AuthedSession::drop`, still root) and **exits**. The session scope then empties and logind reaps it. The daemon is never in any session scope, so it serves login after login. (`worker::serve`, `worker::AuthedSession::drop`) |
| S13 | A flaw in the PAM/session-spawn code is reachable from greeter-controlled bytes (parsing the wire protocol in the same process that holds the password and root session authority) | **Boundary split.** All greeter wire-protocol framing stays in the daemon; the worker that runs PAM and the spawn never reads a greeter byte. The daemon re-execs the worker with the greeter socket closed (it is `O_CLOEXEC`; only the control socket survives on a fixed fd), and proxies the conversation over a small private daemon↔worker codec. A greeter cannot deliver a malformed frame to the PAM/spawn process. (`pam::spawn_worker`, `worker` control protocol) |

## 5. Explicitly NOT defended in this task (honest bounds)

- **N1 — Seat/VT/logind session.** ~~Deferred.~~ **Now built and modeled** (S9–S13;
  D-0004 + D-0005): `setsid`, the controlling-tty handoff, the `XDG_SESSION_*` /
  logind registration via `pam_systemd`, and the per-login-worker leader.
- **N2 — Respawn loops / session lifecycle (residual).** One clean
  open→run→close cycle is modeled (S12). Still **not** defended: crash-loop
  backoff, re-greet policy after a session exits, and concurrent-session
  arbitration. door currently waits on the one session and then the connection
  ends; a follow-up task owns the lifecycle policy.
- **N5 — ~~Daemon is in the logind session scope~~ (removed, D-0005).** Under the
  old plan A the daemon was the leader and was migrated into the session scope.
  The first live run showed this is not merely weaker isolation but a functional
  break (one session per daemon lifetime), so D-0005 moved the leader to a
  per-login worker. The daemon is no longer in any session scope; this bound no
  longer exists (see S12/S13).
- **N3 — A malicious session `Exec`.** door runs the `Exec` from a discovered
  `.desktop` file as the authenticated user; integrity of the installed session
  files is the OS/packaging's responsibility (a user who can write
  `/usr/share/wayland-sessions` can already run code). door does not sandbox the
  session beyond the privilege drop and env sanitization.
- **N4 — Everything inherited from the auth-path model** (N1–N7 there): root-
  equivalent local attacker, malicious PAM stack, offline shadow attacks, libpam's
  transient plaintext, physical capture, finer side channels.

## 6. Verification status

- S1, S2 — exercised by in-process IPC tests (auth-gate refusal with no launch;
  post-auth launch bound to the authenticated user and chosen session).
- S5, and `Exec`/argv/cwd honoring — unit-tested (`spawn::tests`).
- S3, S4 — unit-tested in `privdrop` **and live-confirmed on real hardware**
  (2026-06-25 root run): a `Start` after a real PAM success spawned a session that
  reported `uid=1000 gid=1000` with the user's supplementary groups (not root's),
  the sanitized `PATH`, and `LD_PRELOAD` unset — the drop (S3), the ordering (S4),
  and the env allowlist (S5) demonstrated end to end.
- S6, S7 — code-reviewed; covered indirectly by the unknown-session and
  generic-error paths in `ipc::run_start`.
- S9 — covered by the identity-binding property (`Start` carries only a
  `session_id`; seat/VT come from `config::SeatTarget`, never a greeter frame) and
  the env-merge unit tests.
- S10, S11 — **live-confirmed on real hardware (2026-06-26).** A `Start` on VT4
  spawned a session that ran as `uid=1000(stephen)` on `/dev/tty4` with
  `XDG_SESSION_*` / `XDG_RUNTIME_DIR` set and a sanitized env — the `setsid` +
  controlling-tty handoff and the `pam_systemd` registration demonstrated end to
  end (`loginctl` showed the session on seat0/vc4).
- S12 — **the defect this control fixes was observed live (2026-06-26):** under
  the old daemon-as-leader model the session lingered with the daemon in its scope
  and a second login could not register. The per-login-worker fix (D-0005) is
  code-complete and unit-tested; the **multi-login live re-test** (two sequential
  logins, each registering and cleanly closing its own session) is the remaining
  live DoD.
- S13 — by construction: the worker is re-exec'd with the greeter socket
  `O_CLOEXEC`-closed and only the control fd passed; verified by code review of
  `pam::spawn_worker` (no greeter fd reaches the worker).
