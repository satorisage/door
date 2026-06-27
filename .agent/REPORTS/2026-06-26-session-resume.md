# Session resume — 2026-06-26 (M6 live bring-up)

Read this first on return. Self-contained; no recalibration needed.

## Headline

**M6's done-when is MET and proven on real hardware.** door installs from the
package (disabled by default), enables to become the login manager with sddm
kept as fallback, a real greeter login starts a Plasma session, and the
two-command revert restores sddm. The scary milestone landed.

Branch: `m6-handoff-orchestration` (NOT merged to master yet — that's a pending
decision, see "Next").

## What got committed this session

- `7aef481` — doord: M6 lockout fixes **RC1–RC4**, proven on live boot
  (socket dir perms / lockout recoverability / handoff orphan / non-UTF-8 locale).
- `fd60be7` — M6: revert validated on hardware + RC5/RC6 findings, doc sync
  (postmortem + ROADMAP + door.install; governance token stripped from
  door.install runtime string).
- After this note: a commit carrying **RC7** (postmortem + ROADMAP) + this
  resume file.

## System state at handoff (the user is about to / has rebooted)

- `doord`: **enabled**, was `active` but **wedged** (see RC7) — greeter died
  pre-handshake, doord hung on `accept()`. Black screen + blinking cursor.
- `sddm`: **disabled** (was the fallback; user switched back toward doord).
- Cause of the wedge: user did a **live** `disable --now sddm && enable --now
  doord` while two Plasma sessions were still on seat0 in `State=closing`,
  holding the DRM master. cage couldn't get the GPU → died → doord hung.
- **Recovery = reboot.** Clean boot has no stuck sessions; doord owns seat0 from
  the start = the proven 22:22 path. SSH (192.168.47.150) is the safety net; if
  the reboot misbehaves: `sudo systemctl disable --now doord && sudo systemctl
  enable --now sddm` then reboot.

**First thing to check on return:** did the reboot land in the doord greeter →
Plasma? If yes, RC7 is confirmed a live-switch-only edge case and the clean path
is rock-solid. If no, SSH in and read `journalctl -u doord -b`.

## Open findings (all in the postmortem + ROADMAP)

- **RC5** (material) — clean `systemctl stop`/`disable` of doord doesn't reset a
  graphics-mode VT, so the revert can leave tty1 frozen until the fallback DM
  starts. getty on another VT + SSH keep it recoverable (not a true lockout).
  Fix: VT_AUTO/KD_TEXT reset on admin-teardown (Drop/SIGTERM), extending the RC2
  crash-path guarantee.
- **RC6** (cosmetic) — `door-greeter` HOME=`/` → Mesa shader-cache permission
  error, auto-disabled, zero functional impact. Fix: greeter `XDG_CACHE_HOME` or
  real home. Deferred to M4/M5.
- **RC7** (material) — greeter dies before connecting → doord hangs on
  `accept()` (no reap, no give-up, no VT reset). Fix: serve loop waits on the
  greeter child concurrently with accept; pre-handshake death → counted failure
  → VT reset + backoff. `depends:` RC5.

## Recommended next work order

1. **RC7 + RC5 together** — both are doord teardown/serve-loop robustness on the
   VT, naturally one focused change in `ipc::serve` + the shutdown path
   (`spawn.rs`/`ipc.rs`). Closes the last material rough edges. Was offered as
   "Commit docs + fix RC5"; user chose docs-only for now.
2. **Merge M6 to master** — done-when is met; do it before more churn, or after
   RC5/RC7 if you want the revert fully clean first.
3. **RC6** — fold into M4 (beauty) / M5 (hardening) polish.

## Gotchas learned (don't relearn these)

- **You cannot hot-swap a DM under live graphical sessions.** A DM owns the seat
  from boot; cage can't take a DRM master another session holds. The supported
  switch is clean-boot (disable old, enable new, reboot), NOT live `enable --now`.
- The revert needs `enable --now <dm>` — plain `enable` only arms it for next
  boot (bit the user; door.install now says so).
- Safety nets that held all session: `getty@tty2`+ (Ctrl+Alt+F2) and SSH. tty1
  is the only VT doord/cage take.
- Build artifacts are gitignored now (`pkg/`, `*.pkg.tar.*`, `door.log`, `*.dump`).
- **Hyprland was fully removed this session** (it froze; sddm had defaulted to
  it). sddm default session pinned to `plasma.desktop` via
  `/var/lib/sddm/state.conf`. greetd stack (greetd/agreety/tuigreet) still
  installed — candidate for removal later since door supersedes it.
