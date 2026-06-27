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
  — **found, not yet fixed.** Material: the revert path leaves tty1 frozen
  (getty@tty2 + SSH kept it recoverable). Fix is the symmetric VT-reset on the
  admin-teardown path + `enable --now` doc emphasis.
- RC6 (greeter shader-cache permission error from `HOME=/`) — **found, not yet
  fixed.** Cosmetic; greeter `XDG_CACHE_HOME`/home. Deferred to M4/M5 polish.
- RC7 (greeter dies pre-handshake → doord hangs; surfaced by a live DM switch
  under occupied seat0) — **found, not yet fixed.** Material robustness gap:
  serve loop must wait on the greeter child concurrently with `accept()` so a
  die-before-connect routes to give-up/backoff + VT reset, not a silent wedge.
  Operationally, the supported DM switch is clean-boot, not live `enable --now`.
- Re-validation stays **revert-first on a spare VT/machine**, escape path
  confirmed working *before* `enable`.
