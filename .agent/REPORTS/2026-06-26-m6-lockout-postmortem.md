# M6 live-enable lockout — postmortem

**Date:** 2026-06-26
**Trigger:** First live `systemctl enable --now doord.service`. Symptoms:
(1) greeter could not connect to the socket; (2) **total VT lockout** —
`Ctrl+Alt+F3` dead, no escape, recovered only by chroot from a live env.
**Criticality:** Critical (the lockout domain the M6 rubric names; the
*documented* revert path — "Ctrl+Alt+F3" — was itself dead).

---

## Root cause 1 — greeter can't reach the socket (the "won't connect")

`dist/systemd/doord.service` sets `RuntimeDirectory=doord` +
`RuntimeDirectoryMode=0700`, and the service runs as **root** (no `User=`).
So systemd creates `/run/doord` as **`0700 root:root`**.

The greeter runs as the unprivileged `door-greeter` user. To reach
`/run/doord/door.sock` it must have **search (`x`) on `/run/doord`** — which
`0700 root:root` denies. `connect()` fails with `EACCES` *regardless of the
socket's own `0660 root:door-greeter` perms*: the parent directory gates the
path before the socket mode is ever consulted.

`ipc.rs::bind()` only locks down the directory in the `if !dir.exists()`
branch — but systemd pre-creates it, so doord never touches its mode/owner.
The socket gets chgrp'd to the greeter group; the *directory* never does.

**Fix:** in `bind()`, when `greeter_gid` is known, ensure the runtime dir is
**`0750` and group-owned by the greeter group** (chgrp + chmod), independent
of who created it. `RuntimeDirectoryMode=0750` alone is insufficient — systemd
would still leave the group as `root`.

## Root cause 2 — total VT lockout (the dangerous one)

The greeter runs `cage` on **tty1** (`DOORD_VTNR=1`,
`Conflicts=getty@tty1.service`). cage holds the DRM master and puts the VT in
graphics mode (`KD_GRAPHICS`). When the greeter then **fails and respawns**,
nothing restores a usable console or bounds the respawn:

- **Respawn storm.** The unit has `Restart=on-failure RestartSec=2` but **no
  `StartLimitIntervalSec`/`StartLimitBurst`**, and the *internal* re-greet loop
  (`ipc.rs::serve`) re-launches `cage` every `GREETER_RESPAWN_BACKOFF` (2 s)
  **forever**, with no give-up. A compositor that grabs+drops DRM/KMS on a tight
  loop wedges the GPU/VT subsystem → VT switches stop producing a usable mode →
  black, switch-dead.
- **No DM conflict.** The unit only `Conflicts=getty@tty1.service`, **not
  `display-manager.service`**. If the prior DM is still enabled, two
  compositors fight for DRM master.
- **No VT reset on greeter death.** doord grabs tty1 via raw `TIOCSCTTY`
  (`spawn.rs::take_controlling_tty`) but never `KD_TEXT`-restores it when the
  greeter exits, and never activates/validates the VT. On a crash it is left
  in graphics mode — black and unswitchable.
- **Consequence:** the install scriptlet's documented escape ("revert from
  Ctrl+Alt+F3") was killed by the very failure it was meant to recover.
  Because `WantedBy=graphical.target`, this recurs on **every boot** → chroot
  was the only way back.

**Fixes (defense in depth):**
1. `Conflicts=display-manager.service` + `After=display-manager.service`.
2. `StartLimitIntervalSec=` / `StartLimitBurst=` so a storm self-disables the
   unit into a recoverable text state instead of thrashing the GPU.
3. Cap the internal re-greet loop: after N rapid greeter failures, **stop
   launching the greeter** and idle (serve no greeter) rather than loop — the
   machine stays in a recoverable state, journal says why.
4. Reset the VT (`KD_TEXT`) when the managed greeter exits without a handoff,
   so a failed greeter leaves a usable console.

---

## Root cause 3 — handoff orphans the compositor; the session can't take the VT

Found on the post-RC1/RC2 live test: the greeter now connects and auth succeeds,
but the session never starts. Journal:

```
doord-worker: could not start session 'plasma': starting the session failed: \
  Operation not permitted (os error 1)
```

That `EPERM` is `TIOCSCTTY` on `/dev/tty1` in `spawn.rs::take_controlling_tty`.
The kernel refuses `TIOCSCTTY` with arg `0` when the tty is already the
controlling terminal of another *live* session — even for root (stealing needs
`CAP_SYS_ADMIN` **and** arg `1`). That other session is the compositor's, and it
is still alive because the handoff kills the wrong process:

- The greeter tree is doord → **greeter-worker** (holds the greeter PAM/logind
  session) → **cage** (forked via `spawn::launch`, which `setsid`s it into its
  *own* session and `TIOCSCTTY`s tty1).
- `GreeterHandle::terminate` SIGTERMs the **greeter-worker**, not cage. The
  greeter-worker had no signal handler, so it just died; cage — a separate
  session leader — was reparented to init and kept running, holding DRM master,
  tty1's controlling terminal, and (PAM token leaked) the greeter logind session.
- So the handoff's "freeing the VT" was a no-op. doord then re-greeted, which is
  why the screen returned to a greeter instead of the session.

Distinct from RC1/RC2 (which were about *reaching* the greeter and *recovering*
from a failed one); this is the *success* path's teardown killing the wrong pid.

**Fix (the greeter-worker now owns cage's teardown):**
1. The greeter-worker installs a SIGTERM/SIGINT handler that **forwards the
   terminate to cage** (`worker.rs::forward_terminate`). cage answers SIGTERM by
   tearing the Wayland display down — releasing DRM and the VT — then the
   greeter-worker's `wait` returns and it closes the greeter logind session,
   handing seat0 to the user session.
2. Backstop: cage is spawned with **`PR_SET_PDEATHSIG=SIGKILL`**
   (`spawn.rs::session_setup`, armed *after* the privilege drop because a uid/gid
   change clears it), so even if doord SIGKILLs the greeter-worker before it can
   forward, cage cannot be orphaned onto the VT.
3. `GREETER_TERM_GRACE` raised to 2 s so the clean teardown (DRM release + session
   close) finishes before the SIGKILL escalation.

## Root cause 4 — session starts but the desktop never renders (the black screen)

Found on the post-RC3 live test: the handoff now succeeds — journal shows
`started session 'plasma'` with **no `EPERM`** (RC3 is fixed) — but the screen
goes black with a message about the locale `C` not being UTF-8. That is Qt:

```
Detected locale "C" with character encoding "ANSI_X3.4-1968", which is not UTF-8.
Qt depends on a UTF-8 locale to function correctly...
```

Plasma (Qt) detects a non-UTF-8 `ctype` locale and refuses to render → the login
completes but no desktop appears.

Cause: the session env is built from an explicit allowlist
(`privdrop::sanitized_env`) that intentionally drops the daemon's inherited
environment — but it listed only `HOME/USER/LOGNAME/SHELL/PATH`, **no
`LANG`/`LC_*`**. The PAM env merged on top carries none either: `pam_env` reads
`/etc/environment`, which on this host has no `LANG` (the system locale lives in
`/etc/locale.conf`). So the session launched with locale `C`. (Distinct from
RC1/RC2/RC3, which were all about the VT/socket; this is the session's *runtime
environment*.)

**Fix (`privdrop::sanitized_env` + `locale_env`):** pass the standard locale
variables through from the daemon's own environment — systemd imports
`/etc/locale.conf` into the service-manager env doord inherits, so this is the
system-configured locale, admitted by an explicit name list (not a blanket env
copy) into the user's own session. Plus a fail-safe: if no ctype locale
(`LC_ALL`/`LC_CTYPE`/`LANG`) is set anywhere, default `LANG=C.UTF-8` so a session
can never again land in a non-UTF-8 locale. Locale vars are system config, not a
code path, and go in at the target user's own privilege — none of the escalation
risk the allowlist exists to block.

## Root cause 5 — clean `stop`/`disable` of doord doesn't reset the VT (the revert "lockout")

Found 2026-06-26 during the live **revert** test. The documented revert
(`systemctl disable --now doord` + `systemctl enable --now sddm`) left tty1
frozen on the last greeter/session framebuffer — *looked* like a hard lockout,
though `getty@tty2` + SSH were both live (recoverable, not a true lockout).

Two compounding causes:

1. **VT not reset on clean shutdown.** The RC2 VT-reset (`KD_TEXT`/`VT_AUTO`)
   only fires on the *greeter give-up* path (`ipc::serve` after
   `GREETER_MAX_RAPID_FAILURES`). An admin-initiated `systemctl stop` (SIGTERM)
   exits **without** restoring the VT, so when the VT is in `KD_GRAPHICS`
   (cage/session left it there) it stays frozen. Fix: a shutdown/`Drop` handler
   that restores `VT_AUTO` + `KD_TEXT` whenever doord exits while owning the VT
   with no live handoff in flight — so the *admin teardown* path leaves a usable
   console, same guarantee RC2 gave the *crash* path.
2. **`enable` ≠ start.** Enabling sddm mid-session arms it for next boot but
   does not launch it; the revert needs `enable --now`. The doc says `--now`,
   but it is easy to drop, and the frozen VT (cause 1) made the gap look fatal.

Distinct from RC1–RC4 (socket / lockout-storm / handoff-orphan / locale): this
is the **teardown-by-admin** path — the exact path the revert uses.

## Root cause 6 (cosmetic) — greeter shader-cache permission error

`Failed to create //.cache for shader cache (Permission denied)` in the greeter
log: the `door-greeter` sysusers account has `HOME=/`, unwritable by uid 954, so
Mesa cannot create its shader cache and disables it. **Harmless** — a one-time
perf optimization for the *greeter only*; no functional, auth, or session
impact. Fix: give the greeter a writable `XDG_CACHE_HOME` (e.g. under `/run`) or
a real home.

## Root cause 8 (cosmetic) — session stdio paints the VT console

Observed 2026-06-27 on a clean boot into Plasma: tty1 briefly showed scary-looking
error text before the desktop appeared. It was **not an error and not door** —
stock `kwin_wayland`/xkbcomp keymap warnings (`Virtual modifier Hyper multiply
defined`, `Multiple symbols for level 1/group 1 on key <FK23>`, closing with
`Errors from xkbcomp are not fatal`), plus a benign `xdg-desktop-portal-gtk: Lost
connection to Wayland compositor` at the greeter→session handoff instant.

The reason it is *visible on the console* is door's own design:
`spawn.rs::take_controlling_tty` dup2's the session child's `stdin`/`stdout`/`stderr`
onto the seat VT (so the session owns the seat rather than the daemon's pipes).
A side effect is that the compositor's startup `stderr` lands on the VT
framebuffer, then the compositor sets the KMS mode and paints the desktop over it.
A conventional DM (sddm) hides the equivalent by routing the session log to a file
(`~/.local/share/sddm/wayland-session.log`) or the journal; door points it at the VT.

**Harmless** — purely cosmetic; the session is fine. Fix: keep the VT as the
session's controlling terminal (needed for input/DRM/VT-switch) but redirect
`stdout`/`stderr` to the journal or a per-session logfile instead of dup2'ing them
onto the VT, so the console stays clean and the logs are still captured. Deferred
to M4 (beauty) / M5 (hardening) polish.

## Root cause 7 — doord hangs when the greeter dies before connecting (live-switch wedge)

Found 2026-06-26 attempting a **live** DM switch: `disable --now sddm && enable
--now doord` *while two Plasma sessions were still on seat0* (left in
`State=closing` from earlier experimentation). Symptom: black screen + blinking
cursor; doord `active` but no login, no journal beyond "launched greeter".

Chain: doord launched the greeter-worker → cage tried to acquire the seat0 DRM
master → the two `closing` sessions had **not** released it → cage failed and
died → the greeter-worker went `<defunct>`. doord's log stops at "launched
greeter" — **no connect, no give-up**: it neither reaped the dead worker nor
tripped the RC2 give-up/backoff. It blocked (waiting to `accept()` the greeter
connection) on a greeter that was already dead.

Two distinct facts:

1. **Operational (not a bug):** you cannot hot-swap a DM under live graphical
   sessions — a DM owns the seat from boot, and cage cannot take a DRM master
   another session holds. The supported switch is **clean-boot**: sddm disabled,
   doord enabled, reboot — the path proven to work (the 22:22 boot).
2. **Robustness gap (the actual bug):** doord must detect greeter-worker death
   *before* the handshake and route it into the give-up/backoff path (reap the
   child, restore the VT, count the failure) instead of blocking forever on
   `accept()`. The RC2 give-up only catches rapid *connected-then-failed* loops;
   a die-before-connect wedge slips past it. Fix: have the serve loop wait on
   the greeter child concurrently with the accept, so a pre-handshake death is a
   counted failure (→ VT reset + backoff), never a silent hang.

## Root cause 9 — revert to another DM under a live session squats tty1 (the "bare blinking cursor" on `stop doord` + `start sddm`)

Found 2026-06-27 reproducing the user's repeated revert failure: booted into door,
logged into Plasma, then `systemctl stop doord && systemctl start sddm`. Physical
screen lands on a **bare blinking cursor**; sddm never appears; restarting sddm
does not help. Diagnosed live over SSH (`scratch/revert-seat-diag.sh`,
`scratch/revert-lockout-diag.sh`).

This is **not** a wedged VT (RC5/RC7). Captured facts:

- The Plasma session **survives** `stop doord` — it lives in
  `user.slice/user-1000.slice` (session-3.scope + `user@1000.service`), wholly
  independent of `doord.service`'s cgroup, so stopping doord cannot and does not
  kill it. RC5's "the session survives a doord restart" assumption is *correct*.
- After the stop, `fuser /dev/dri/card1` shows **`kwin_wayland` still holds
  `[MASTER]`** and `loginctl seat-status seat0` shows session-3 still owns the
  seat on **tty1**. tty1 is `KD_TEXT` — an *empty* text console, which is the
  "bare blinking cursor."
- sddm is hard-wired to **VT 1**. Its greeter helper tries to take tty1, fails
  with `SDDM::Auth::HELPER_TTY_ERROR` (`sddm-helper exited with 5`), and the
  display add/remove loop spins forever — because the live Plasma session still
  owns that VT and the DRM master.

Root cause: **door runs the user session on the seat VT (tty1, `DOORD_VTNR=1`) —
the same VT every login manager wants.** A conventional DM (sddm/gdm) runs its
*greeter* on tty1 but each *user session* on a fresh logind-allocated VT (tty2+),
so a DM restart or DM switch only ever contends for the greeter VT and never
touches live sessions. door's single-VT handoff (D-0008: greeter→session on the
same VT) means a surviving session squats tty1, blocking any new login manager —
sddm here, and also doord's *own* re-greet: the journal shows a paired event at
00:30 where **restarting** doord under live Plasma made the new cage greeter fail
to take tty1 (Plasma holds DRM) → die pre-connect → RC7 give-up → doord reset
tty1 to text and exited. Same root, two surfaces.

Severity: **not a lockout.** SSH stayed live throughout and a reboot recovers
cleanly (the proven clean-boot revert). It *looks* like a hard lockout but isn't
— it is seat/VT contention from switching login managers under a live session,
which is unsupported for DMs generally; door is merely more visibly fragile
because its session shares the greeter VT.

Fix is an architecture decision (pending, see Disposition): (1) run sessions on
their own VT like a conventional DM — removes the whole contention class incl.
doord-restart-under-session — but is a substantial change to the D-0008 handoff;
(2) make `stop`/`disable` doord tear down the session it spawned to free the seat
— cheap, but kills the desktop on any stop and diverges from DM convention; or
(3) keep clean-boot/log-out-first as the supported revert and document it (+ an
optional revert helper that `loginctl terminate-session`s the seat first) —
near-zero cost, matches the already-proven path, leaves the naive command looking
broken.

**Web research (2026-06-27) — the "clean" separate-VT fix is the unsolved DM
frontier, not a quick win.** Verified at source before relying on it:

- SDDM does *not* reliably put a wayland session on its own VT, and **does not
  free the VT** when a wayland session ends → VT exhaustion, and eventually loss
  of Fn-key VT switching (sddm#1200, sddm#1409). The session-on-separate-VT design
  is "supposed to" happen but is inconsistent/buggy in practice.
- The **blank-screen-with-cursor symptom is itself a known SDDM failure** — SDDM
  "frequently fails to reactivate the greeter after the compositor exits," and
  "Removing a Display causes a VT switch" (sddm#1803, arch BBS #284449). RC9's
  symptom is the generic wayland DM/VT-contention failure, not a door-only defect.
- greetd (door's nearest analogue: greetd+cage) uses the **same single-VT model**
  (`vt = 1`); cage's `-s` VT-switch flag exists precisely because this area
  locks people out (ArchWiki: greetd).

Implication: option (1) buys "desktop survives a doord *restart*" at the cost of
entering SDDM's unsolved VT-leak/reactivation bug class. For the *revert* use case
(leaving door), preserving the session has no value. Recommendation shifted to a
variant of (2): **doord ends its own logind session on teardown via
`terminate-session`** (the lever that actually reaches the decoupled
`user@1000.service` compositor — a parent-death signal on the session child does
not, since kwin is not doord's child). Sub-decision: fire on every doord exit
(incl. crash; most consistent, no residual wedge) vs only clean stop/disable.
Sources: sddm#1200, sddm#1409, sddm#1803, arch BBS #284449, ArchWiki Greetd/SDDM.

**Hardware finding (2026-06-27) — `terminate-session` is insufficient; the
compositor escapes the session scope.** Tested `loginctl terminate-session` on the
live seat0 session (`scratch/post-terminate-state.sh`): logind dropped the session
(`seat0 Sessions=` empty, `startplasma` pid gone) **but kwin/plasmashell/Xwayland
stayed alive under `user@1000.service` and kwin kept `[MASTER]` on card1.** The
graphical units live in the per-user systemd manager, not the logind session
scope; because other sessions (SSH/pts) keep `user@1000` running and door's
session is not bound tightly enough for the graphical target to stop with the seat
session, the compositor **outlives the logind session, doord, *and*
`terminate-session`** — permanently squatting seat0's DRM master. This also defeats
doord's own re-greet (cage can't take a held master → dies pre-connect → RC7
give-up → doord `inactive`). Consequence: the committed "doord ends its logind
session on teardown" plan does **not** free the seat. The real lever must target
the **DRM-master holder / graphical-session.target under `user@1000`** (kill the
compositor, or stop the user graphical target), not the logind session. Clean
in-session logout (Plasma stops its own graphical target) is expected to free the
seat normally — the un-tested supported revert; external teardown is the broken
path.

**Lever confirmed on hardware (2026-06-27, `scratch/recover-kill-compositor.sh`):**
`SIGTERM`→`SIGKILL` of the compositor processes freed card1's DRM master (fuser:
no holder), and `systemctl start sddm` then took VT1 cleanly — "Greeter session
started successfully", GPU re-acquired by sddm's own kwin. So (a) the only
effective lever is **killing the DRM-master holder**, not terminating the logind
session, and (b) the seat **re-acquires cleanly** once the GPU is free (rules out
the sddm#1200 dirty-seat dead end). Fix re-scoped accordingly — see Disposition.

## Disposition

- RC1 (socket dir perms) — **fixed** (`ipc.rs::bind`).
- RC2 (lockout recoverability) — **fixed** (unit + `ipc.rs` re-greet cap + VT
  reset). Highest safety value: a failed greeter must never be able to kill the
  revert path.
- RC3 (handoff orphans the compositor) — **fixed** (`worker.rs` terminate
  forwarder + `spawn.rs` `PR_SET_PDEATHSIG` backstop + 2 s grace). Load-bearing
  kernel/userspace semantics (TIOCSCTTY EPERM-unless-arg-1, cage's SIGTERM
  handling, PDEATHSIG cleared on fork / on uid-gid change / preserved across
  non-setuid execve) verified against current man-pages and cage source before
  landing.
- RC4 (non-UTF-8 session locale → black screen) — **fixed and proven on
  hardware** (`privdrop::sanitized_env` passes locale through + `C.UTF-8`
  fail-safe). 38 tests green, clippy clean. **2026-06-26 live boot: full path
  green** — journal shows greeter connected (uid 954) → `authentication
  succeeded for 'stephen'` → `handoff … tearing down greeter, freeing the VT`
  (no `EPERM`) → `started session 'plasma'`, and Plasma rendered. SSH safety
  net held throughout. All four root causes now demonstrated on real hardware.
- RC5 (clean stop/disable doesn't reset the VT → revert looks like a lockout)
  — **fixed (2026-06-27), not yet re-validated on hardware.** A SIGTERM/SIGINT
  teardown handler (`ipc.rs::install_teardown_handler`/`handle_teardown`) resets
  `VT_AUTO`/`KD_TEXT` **only while doord holds the VT at the greeter** (the
  `GREETING` flag), then re-raises with the default disposition. A live session
  owns its own VT and has no parent-death signal, so it is left untouched and
  survives a doord restart — only the *greeter-held* VT is reset on admin
  teardown, extending the RC2 crash-path guarantee to the admin-stop path. The
  handler does only async-signal-safe work (atomics + open/ioctl/close via
  `restore_text_vt_raw`, then `signal`/`raise`). `enable --now` doc emphasis
  already landed in `door.install`.
- RC6 (greeter shader-cache permission error from `HOME=/`) — **fixed 2026-06-27**
  (`worker.rs::launch_greeter`): the greeter env now sets `XDG_CACHE_HOME` to the
  greeter's logind runtime dir (`XDG_RUNTIME_DIR`, 0700/writable/ephemeral), so
  Mesa stops trying `//.cache` and the shader cache works. Cosmetic; pending a
  hardware glance at the greeter log.
- RC7 (greeter dies pre-handshake → doord hangs; surfaced by a live DM switch
  under occupied seat0) — **fixed (2026-06-27), not yet re-validated on
  hardware.** The serve loop now waits via a non-blocking accept + bounded poll
  (`ipc.rs::accept_with_greeter_watch` + `GreeterHandle::reap_if_exited`): a
  greeter that dies before connecting is reaped, the VT is reset, and the failure
  counts toward give-up/backoff instead of blocking forever on `accept()`. The
  teardown handler (RC5) is also a backstop — a `systemctl stop` now escapes even
  a wedged accept. Operationally, the supported DM switch remains clean-boot, not
  live `enable --now`.
- RC8 (session stdio paints the VT console → scary-but-harmless compositor
  warnings flash on tty1) — **fixed 2026-06-27** (`spawn.rs::take_controlling_tty`):
  the VT is still the session's controlling terminal and stdin, but stdout/stderr
  are left on the daemon's inherited streams (the service journal) instead of being
  dup2'd onto the VT — so the compositor's startup chatter goes to the journal, not
  the framebuffer. Cosmetic; pending a hardware glance at the console on login.
- RC9 (revert/DM-switch under a live session squats the seat's DRM master → next
  login manager can't acquire the GPU → bare blinking cursor) — **fixed and proven
  on hardware 2026-06-27.** Lever proven on hardware: only killing the compositor
  frees the seat (`terminate-session` leaves it orphaned under `user@1000`), and —
  the decisive subtlety — the compositor runs under a **supervisor**
  (`kwin_wayland_wrapper`) that *respawns* it, so killing the bare DRM-holder pid
  just bounces DRM (desktop sees a monitor unplug/replug) and never frees the seat.
  The whole compositor subtree shares one process group, so the fix kills the
  **process group** of each DRM-card holder — taking the supervisor down with it,
  no respawn. Fix (`ipc.rs`): `free_seat()` scans `/proc` for holders of the seat's
  `/dev/dri/card*`, maps them to process groups, and `SIGTERM`→`SIGKILL`s the
  groups (skipping doord's own); wired to three points — (1) **before each greet**
  (seat-claim: cage always gets a clean GPU — also kills the RC7
  restart/crash-respawn wedge at its source), (2) **on teardown** (a
  `SIGTERM`/`SIGINT` during a live session sets a flag the new interruptible
  session-wait loop acts on — free the seat, restore the VT, exit — so `stop doord;
  start sddm` reverts cleanly), and (3) a **panic hook**. Owner decision: sessions
  die with doord (every exit incl. crash); the desktop is not preserved across a
  doord stop/restart/crash — accepted, since the compositor is the squatter and
  can't be cleanly preserved. Residual gap: a `SIGKILL`/power-loss of doord runs no
  cleanup, but the next start's seat-claim (point 1) clears the orphan. (After
  killing the compositor, `free_seat` also runs a best-effort `loginctl
  terminate-seat` to sweep session-bound user units — e.g. `plasmashell`,
  `PartOf=graphical-session.target Restart=on-failure` — so they stop cleanly
  instead of restart-looping.) **Validated
  live: step 1 — doord seat-claimed a stuck compositor and greeted; step 2 —
  `stop doord; start sddm` reached the sddm login with no flicker/respawn.** 26 unit
  + 3 integration tests green, clippy clean. Distinct from RC5/RC7
  (VT-left-in-graphics); this is a healthy text VT whose GPU a survived,
  self-respawning compositor held.
- Re-validation stays **revert-first on a spare VT/machine**, escape path
  confirmed working *before* `enable`.
