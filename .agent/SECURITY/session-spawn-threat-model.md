# Threat model — the session-spawn path

**Scope of this model:** everything from an authenticated greeter asking to start
a session (`Start { session_id }`), through the daemon resolving the user and the
session, to the fork → privilege drop → environment build → exec that hands the
seat to the user's session. It picks up where the auth-path model
(`auth-path-threat-model.md`) left off (its N8) and covers the privilege handoff
itself.

It does **not** cover seat/VT ownership or the logind session registration
(`XDG_SESSION_*`, controlling tty, `setsid`, VT switch): that is the next task on
this milestone and a different axis. Until it lands the session inherits the
daemon's stdio — which is what makes the drop demonstrable — and the threats it
introduces (handoff races, VT confusion, respawn loops) are modeled when it
lands. They are listed in §5 as explicitly deferred, not silently omitted.

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
| S1 | Greeter sends `Start` with no prior auth, to spawn a session (a shell) it was never entitled to | **Auth gate.** `Start` is honored only when the connection holds a completed PAM success; otherwise it is refused with a generic error and **no launch occurs**. State is per-connection (`authenticated: Option<String>` in `ipc::handle_connection`), set only by `run_auth` on `AuthOutcome::Success`, and cannot outlive the greeter that earned it. (`ipc::tests::start_before_auth_is_refused_and_never_launches`) |
| S2 | Greeter authenticates as itself but asks to start a session **as another user** (confused deputy → privilege/identity escalation) | **Identity binding.** `Request::Start` carries only a `session_id`, never a username. The spawn target is the username PAM authenticated on this connection, resolved fresh via `getpwnam` (`user::resolve`). The greeter has no channel to name a different identity. (`ipc::tests::start_after_auth_launches_the_chosen_session_as_the_authed_user`) |
| S3 | A `setres*id` silently fails and the session is exec'd still holding root | **Drop-or-abort.** The drop runs in `Command::pre_exec`; `drop_to` verifies the post-drop uid/gid and **refuses uid 0**, returning an error. `Command::spawn` propagates a `pre_exec` error, so a session whose drop did not fully take is **never exec'd** — the launch fails closed. (`privdrop::{drop_to,verify_dropped}`, `spawn::ProcessLauncher`) |
| S4 | Group privileges stranded (uid dropped while still in root's groups) | **Ordering.** `initgroups → setresgid → setresuid`: supplementary + primary groups are set while still privileged, uid last. (`privdrop::drop_to`. See the H5 text-vs-code note in `ROADMAP.md`; both orderings satisfy groups+gid-before-uid.) |
| S5 | Daemon's root environment leaks into the user session (`LD_PRELOAD`, root `PATH`) | **Allowlist env.** The child environment is `env_clear`ed and rebuilt from `privdrop::sanitized_env` (HOME/USER/LOGNAME/SHELL + a fixed PATH) bound to the target — never inherited. (`spawn::build_command`, `spawn::tests::command_environment_is_the_sanitized_allowlist_only`) |
| S6 | Greeter starts a `session_id` that does not exist, or a stale one, to probe or crash the daemon | **Re-resolve + refuse.** `Start` re-runs discovery and matches by `id`; an unknown id is a generic "no such session" refusal and the connection keeps serving. The `Exec` is never taken from the greeter — only from the daemon-side discovered entry. (`ipc::run_start`) |
| S7 | A spawn error reveals privileged detail to the login screen (e.g. whether an account exists, via a distinct error) | **Generic refusal.** Every `LaunchError` (user-not-found, exec failure) maps to one opaque "could not start the session" to the greeter; the detail is journaled only. (`ipc::run_start`, `spawn::LaunchError`) |
| S8 | Non-async-signal-safe work in the forked child deadlocks (the classic fork-in-a-threaded-process allocator deadlock) | The daemon serves one greeter sequentially with **no worker threads**, so it is single-threaded at fork; the child is the only thread and `initgroups`/exec setup are safe — the same shape `login(1)`/`sshd` use. (`ipc::serve` is sequential; noted at the `pre_exec` call.) |

## 5. Explicitly NOT defended in this task (honest bounds)

- **N1 — Seat/VT/logind session.** No `setsid`, no controlling-tty handoff, no VT
  switch, no `XDG_SESSION_*` / logind registration yet. The session inherits the
  daemon's stdio. This is the next task; its threats (handoff races between
  greeter teardown and session start, VT ownership confusion, two sessions on one
  seat) are modeled then. Until then door is not a complete seat manager.
- **N2 — Respawn loops / session lifecycle.** The daemon currently waits on the
  session and then the connection ends; there is no crash-loop backoff, no
  re-greet policy, no concurrent-session arbitration. Deferred with the logind
  task.
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
