# CHECK-IN 0003 — command-center batch in flight (pre-reboot snapshot)

**Opened:** 2026-07-03 · **Status:** OPEN · **Criticality:** Material
**Context:** M0–M9 all shipped/complete; black-strip fixed (v0.1.7). User asked to
finish M4, bring remaining decisions up, and batch out the rest (M-F stays parked).
This check-in captures the in-flight state at a reboot so a fresh session can resume.

## What landed durably before the reboot
- **`delegate/cc-settings-docs` @ `8f96be6`** (pushed to origin) — Agent B's WIP,
  **UNVERIFIED** (no build/test run confirmed). ~314 lines across:
  `door-settings/src/main.rs` (+253: M8 settings-UX grouping, M7-E import/export,
  M7-D tz control), `door-theme/src/lib.rs` (+36: tz Theme key), `dist/door/greeter.toml`
  (docs). **Next:** recreate a worktree from this branch (its `/tmp` worktree was
  wiped by reboot — run `git worktree prune`), then review → `cargo build`/`test -p
  door-theme -p door-settings`/`clippy` → verify byte-identical defaults held →
  integrate via the command-center gate (`delegate finish`). Master must stay at
  `fa5571e` base or B needs a rebase.
- **`scratch/m4-save-verify.sh`** (gitignored, persistent) — owner-run harness to
  close M4's last leg (door-settings pkexec Save → `/etc/door/greeter.toml`). Run
  `bash ./scratch/m4-save-verify.sh` on genny; a PASS closes M4's Done-when.

## Agent A (greeter-render) — returned NO code, by design; findings:
- **Premise correction:** greeter is a single-window `iced::application` under
  **cage**, NOT `iced_layershell` (ratified D-0006/D-0007). Windowing-agnostic tasks
  (B's) are unaffected.
- **Per-monitor wallpaper is NOT ready work — it's a governance decision.** Sky shader
  is already aspect-aware; the gap is pure windowing (cage fullscreens one output).
  Per-output surfaces would reverse D-0006/D-0007 + touch the TCB cage invocation
  (`doord/src/config.rs`). Decision needed: (a) supersede D-0006/D-0007 for a
  layer-shell compositor, or (b) accept "primary output only, others dark" as a
  documented v1 bound (seat's lean: **b**).
- **Sunrise transition** is buildable but must be sequenced: a default-off
  `day_transition` Theme key (lib.rs, B-domain) → consumed in `app.rs` +
  `skyshader.rs` (greeter-domain follow-up). A hardcoded window would break the
  byte-identical-default invariant. Low priority.

## Decisions awaiting the owner (5)
1. User list + avatars — seat lean: **park** (pre-auth enumeration + new daemon protocol).
2. Keyboard-layout indicator — seat lean: **authorize** (local xkb, ~TCB-neutral).
3. Battery indicator — seat lean: **authorize** (local sysfs read).
4. Network indicator — seat lean: **park**, or authorize via F3 helper-file pattern
   (post-login helper writes `/run/door/net.json`, greeter reads the file; no pre-auth dbus).
5. Per-monitor wallpaper (from Agent A) — supersede D-0006/D-0007 vs. documented v1 bound.
Each authorized item graduates with a light off-by-default threat-model DECISION first.

## Held follow-ups (not lost, queued)
- Animated logo (cross-cuts greeter+theme+settings; dispatch after B's schema lands).
- Sunrise 2-step (transition key → consume).
- M-F stays parked entirely (owner directive).

## Stale ROADMAP edits deferred to B's integration gate (keep master linear until then)
- `## Active` banner still says "M5 Tier 2" — M5 closed 2026-07-02.
- M7-D leaf "font weight, SVG + animated logo, custom timezones": font_weight
  (`lib.rs:966`) + SVG logos already shipped v0.1.1 — mark done; only animated-logo +
  timezones were real.
- Per-monitor wallpaper: reclassify from plain M7-C leaf → DECISION-gated (D-0006/7).
