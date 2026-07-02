# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

> **Current frontier (2026-07-01): M5 Tier 2 — pre-forked spawner, login path HARDWARE-VALIDATED.**
> M4's Done-when is met on hardware (real-VT PASS 2026-07-01; one minor save-leg open). M8 and
> M9 are **complete** (see their sections below — tagged in place per the M7 convention, not
> physically relocated). **M5 §Tier 2 part 2 validation happened out-of-tree** (a runtime act,
> so D-0037's commit-boundary sync never fired — recorded here 2026-07-01 from the live machine):
> a `DOORD_SPAWNER=1` systemd drop-in is enabled on `genny`, doord booted in spawner mode, and a
> **real PAM login (`stephen`) handed off into a `plasma` session over the spawner path** (journal
> `-b`, boot 10:45 CDT; session stable 9h+). The login leg is proven on metal. **Remaining before
> the flag becomes the shipped default:** (a) confirm the clean logout/teardown leg (control-EOF →
> worker reap — not yet observed this boot, no logout), (b) flip the shipped `doord.service`
> default (currently on only via the machine-local drop-in), then Tier 3 (seccomp) / Tier 4
> (Landlock). The section lives under `## Backlog` below by the in-place convention.

### M4 — The beautiful greeter — **Done-when MET on hardware (2026-07-01)**; lone open leg: door-settings *save*
**Criticality: Material** (pre-auth UI; no auth/lockout change, but the greeter is
the pre-auth surface — keep new deps justified). Direction (2026-06-27, owner):
**wallpaper + frosted card**, **config-driven theme engine + a built-in default**
(formal DECISION to be filed at milestone close).

- [x] **theme engine** (`door-greeter/src/theme.rs`): a `Theme` (wallpaper,
      background, card, accent, foreground, muted, logo, radius, width, clock) loaded
      from TOML — `$DOORD_GREETER_CONFIG` → `/etc/door/greeter.toml` →
      `/usr/share/door/greeter.toml` → built-in default, merged. Strict parsing
      (`deny_unknown_fields`), `#rrggbb[aa]` colors, missing-asset degrade. Tested.
- [x] **wallpaper + frosted card UI** (`app.rs`): `stack![` full-bleed wallpaper,
      centered translucent rounded card[ clock · logo · title · picker · fields ·
      accent sign-in · status · power ] `]`, themed via `app_style` + per-widget
      styles. v1 = translucent card (true backdrop blur is an Iced limitation —
      deferred). `depends:` theme engine
- [x] **clock** (`app.rs`): 1 Hz `HH:MM` via a thread-tick subscription +
      `localtime_r` (no date/time crate on the pre-auth surface). `depends:` UI
- [x] **default assets + packaging**: `dist/door/greeter.toml` (documented default) +
      `dist/door/wallpaper.png` (the Tokyo Night **comet** wallpaper, the user's own
      from genny-config — the look that sparked door); PKGBUILD installs both to
      `/usr/share/door/`. The built-in palette is Tokyo Night, so the frosted card +
      `#7aa2f7` accent harmonize with the comet out of the box. `depends:` theme engine
- [x] **custom font** (2026-06-27): `font` is a family name (iced loads installed
      system fonts), applied via `default_font`; the shipped default is
      `MesloLGS Nerd Font` (genny's). Falls back to stock if absent. `depends:` theme engine
- [x] **card fade-in** (2026-06-27): a bounded launch fade (~380 ms) ramps the card +
      text alpha, then the ticker stream ends (no perpetual repaint). `depends:` UI
- [x] **minimal redesign** (2026-06-27): smaller card (300 px), clock+date focal
      point, slim rounded fields with accent-on-focus, full-width accent sign-in,
      glassy translucent card with a thin accent hairline + soft drop shadow, and the
      power controls moved out of the card to a subtle bottom-right row. `depends:` UI
- [x] **ambient animation** (2026-06-27): a continuous `canvas` sky over the
      wallpaper, plus a gently breathing accent hairline on the card. Alive while
      idling at the prompt. Driven by `iced::window::frames()` (the compositor's
      vsync frame clock) with the phase recomputed from real elapsed time each frame
      — buttery smooth, no fixed-rate tick. `depends:` UI
- [x] **match the genny wallpaper + `animate` setting** (2026-06-27): the sky is now
      a native port of the user's `com.genny.tokyonightcomet` Plasma wallpaper —
      three parallax depth-colored star layers (≈226) + a comet that sweeps
      upper-right→lower-left on a 7 s InOutSine then pauses, looping. Extracted to a
      shared `door-theme::sky` module (generic over `Message`) so greeter and
      settings animate identically. New `animate` theme key (default on; off → still
      wallpaper, like the wallpaper's battery half); the ~30 fps tick only runs when
      on. `depends:` ambient animation
- [x] **default to the clean animated look** (2026-06-27): the default ships **no**
      static wallpaper — door draws the animated sky (stars + comet + a soft indigo
      depth-glow) over the solid background, matching the desktop's animated comet
      plugin (cleaner than the stylized PNG, which stays available via `wallpaper`).
      `depends:` match the genny wallpaper + `animate` setting
- [x] **day/night greeter (auto by time)** (2026-06-27): a built-in Tokyo Night Day
      (light) palette + a day-recolored sky (`door-theme::sky` is now day/night-aware
      via a `day` flag). The greeter picks the variant by the **local clock** at
      launch (it runs pre-login, so it can't read the user's color scheme); window
      `day_start`/`day_end` in greeter.toml (default 07:00–19:00). Top-level config is
      the night palette; day shares its font/sizes/behavior. Matches the genny
      day/night lock-screen variants. `depends:` default to the clean animated look
- [x] **settings: edit both palettes** (2026-06-27): a `[day]` override block in
      greeter.toml (day = built-in light + overrides), and door-settings gains a
      Night/Day toggle — edit either palette with the live preview switching variant,
      plus the day window; Save writes both via `Theme::render_pair`. `depends:` day/night greeter (auto by time)
- [x] **settings: sliders + spinner controls** (2026-06-27): a per-variant **card
      opacity** slider, plus `spinner_glow` (per-variant; 0 = crisp — day defaults 0
      since a bright bloom bands on a light card) and `spinner_speed` (shared) as
      theme keys with sliders. Day card default opacity 15%. `depends:` settings: edit both palettes
- [x] **settings: full spinner styling** (2026-06-27): the rotating comet is now a
      single configurable color (`spinner_comet`) over a configurable ring
      (`spinner_track`) with a `spinner_trail` length (shorter = fewer overlapping
      dots = crisp on a light card, which fixed the day "blur"; longer = a soft glowing
      ribbon on dark) — all per-variant, with color cells + a Trail slider in
      door-settings. Day defaults to a deep comet + short trail. `depends:` settings: sliders + spinner controls
- [x] **settings: in-window live preview + chrome** (2026-06-27): door-settings now
      renders the *real* wallpaper + animated sky + a themed mock card live as you
      edit (no more launch-to-preview), over a frosted glass control panel — the
      whole window is the greeter you're editing. Form polished: grouped sections,
      styled inputs, two-column color rows with live swatches, togglers, tight
      spacing. `depends:` settings editor
- [x] **comet-spinner logo** (2026-06-27): the card's default logo is a native,
      animated comet spinner in `door-theme::sky` (a port of the genny boot throbber:
      a rotating ring with a bright comet head) — a user `logo` image overrides it.
      Shown in both the greeter and the settings preview. `depends:` match the genny wallpaper
- [x] **keyboard focus** (2026-06-27): Tab / Shift-Tab cycle the username, password,
      and sign-in (via `event::listen_with` + `operation::focus_{next,previous}`);
      Enter still submits. `depends:` UI
- [x] **settings editor (D-0010)** (2026-06-27): the theme schema extracted into a
      shared `door-theme` crate; a standalone pure-Rust `door-settings` (iced) edits
      it — Preview launches the real greeter on a draft config, Save writes
      `/etc/door/greeter.toml` via `pkexec` — with a `.desktop` under Settings. A
      QML System Settings KCM was rejected to stay Rust / out of the TCB (D-0010).
      `depends:` theme engine
- [~] **on-hardware visual check**: boot into door; confirm the themed greeter
      renders (wallpaper, glass card, clock, font, fade) on the real VT, and that
      door-settings previews + saves. Refine to taste. `depends:` default assets + packaging
      - [x] **nested visual check PASS (2026-06-28)** — real `door-greeter` under nested
            `cage` (its production host) + `grim`, both day & night variants forced via
            temp configs. Confirmed: animated sky (night starfield+comet sweep / day
            sun+parallax clouds+haze), glass card, clock+date, Meslo font, per-variant
            spinner (glow night / crisp day), accent fields/button, power controls,
            day/night auto-switch. door-settings live in-window preview confirmed.
            Frames: `scratch/m4shots/{night-f1,night-f3,day-f1,settings-1}.png`.
      - [x] **font dep fixed (2026-06-28)** — default theme requests `MesloLGS Nerd Font`
            (from `ttf-meslo-nerd`), which was absent from PKGBUILD `depends`; clean
            installs fell back to stock. Added `ttf-meslo-nerd` to `depends`, bumped
            `pkgrel 1→2`. Ready for a `0.1.0-2` AUR re-publish.
      - [x] **real-VT confirmation (user) — PASS (2026-07-01)** — after installing the
            new binary + enabling the systemd unit, user rebooted straight into door on
            the real VT: themed greeter rendered, entered password, PAM authed into the
            session. The nested-render prediction held; hardware matched. This closes the
            M4 Done-when on hardware (only the door-settings *save* leg remains, below).
      - [x] **day sun-bloom dialed back (2026-06-29)** — the day `day_sky` WGSL sun
            washed the upper-left quadrant white; tightened halo falloff 7→16 +
            intensity 0.55→0.38 + smaller/softer core, as the new built-in default
            (shared skyshader → greeter + settings). Commit `945cfc5`. Not a theme
            knob; it's the hardcoded default sun. Re-shot the reddit clip's day pass.
      - [ ] **door-settings *save* not yet exercised** — `pkexec`-write to `/etc/door`;
            design-verified, confirm interactively on the real machine.

**Done-when (M4):** booting into door shows the themed greeter — wallpaper, frosted
card, clock, themed font, subtle fade-in — rendered from a config file with a
beautiful built-in default; re-theming needs only an edit to `greeter.toml`.

## Backlog (future milestones, not yet sequenced)

### M8 — Per-scene authoring controls + mode completeness (ratified D-0013, 2026-06-30) — ✅ COMPLETE (2026-06-30, on master)
> **Shipped across 10 commits** (`f9bda7a` scene-param pool + synthwave pilot → `a8f9e07`
> per-scene roll-out complete → `3d0567b` all new modes). Scene-param uniform pool
> (`skyshader.rs` `scene_a..c3`), per-`sky_mode` packing in `from_theme`, named per-scene
> config keys, door-settings per-scene groups, and the full new mode set (Day/Night/Solid,
> Mountains/Forest/Ocean/Pyramids, Sunset/Galaxy/Matrix). Health: `door-theme` 16/16 tests
> incl. naga WGSL gates green. Tag-in-place per the M7 convention.
`depends:` M7. Expose each scene's defining constants as labeled per-scene controls
(shared exclusive-scene uniform pool; named config keys packed by `sky_mode`; per-scene
settings group; defaults preserve the look) + round out the built-in mode set (users
can't author modes). Full classification + EXPOSE map in
`.agent/IDEAS/2026-06-30-scene-controls-spec.md`. TCB-neutral; each shader change
naga-gated; keeps the D-0011 100%-mapped invariant for the exposed set.
- [x] **foundation — shared scene-param uniform pool** (2026-06-30, `f9bda7a`):
      `scene_a/b/c1/c2/c3` vec4s in the sky Uniforms + WGSL; packed in `from_theme`
      (synthwave for now; others key off `sky_mode` as they roll out).
- [x] **synthwave pilot** (2026-06-30, `f9bda7a`): all 11 dials end-to-end
      (grid speed/density/perspective/glow, sun size/stripes/bloom, horizon, grid +
      2 sky colours) → theme keys + uniforms + WGSL reads + a Sky-tab "Synthwave"
      group shown only for that scene. Defaults byte-identical (verified under cage).
      Pattern locked for the roll-out.
- [x] **per-scene roll-out** (COMPLETE 2026-06-30) — all 11 scenes expose their EXPOSE
      map from the spec. `depends:` synthwave pilot. Foundation packs the pool keyed by
      sky_mode (`c0414fe`).
      - [x] storm (2026-06-30, `c0414fe`): lightning rate, strike chance, cloud density,
            bolt + flash colour.
      - [x] rain, snow, fire (2026-06-30): rain = fall/density/slant/brightness + colour;
            snow = fall/density/sway/flake-size + colour; fire = rise/height + flame & tip
            colour (flame hue stored normalised, 1.6 HDR boost kept internal). Defaults
            verified identical under cage.
      - [x] aurora, plasma, water (2026-06-30): aurora = drift/curtain-length/shimmer/
            brightness + green & magenta; plasma = flow/scale/saturation + tint (rainbow
            kept, tint multiplies); water = flow/ripple-scale/distortion/caustic-glow +
            caustic/deep/shallow colours. Defaults verified identical under cage.
      - [x] meteor, moon, fog (2026-06-30): meteor = speed/count/trail-decay/brightness +
            meteor & star colours (count drives a dynamic loop bound); moon =
            size/phase-speed/texture/halo + moon & halo colours; fog =
            drift/scale/thickness/opacity + colour. Defaults verified identical under cage.
      - **per-scene roll-out COMPLETE — all 11 scenes exposed.**
      - synthwave control feel tuned (useful-band ranges + fine steps, `77f1218`) — the
            convention every scene's sliders follow.
- [x] **new modes (near-free)** (2026-06-30) — Day, Night, Solid. Day/Night reuse the
      auto renderer (shader_id 0), distinguished only by `prefers_day` pinning the palette
      + day-flag (zero shader change); Solid = a new `solid_sky` (palette gradient, no
      stars/comet/motion, shader_id 12). Verified under cage: day = light palette + sun,
      night = starfield, solid = calm gradient. Picker + config + naga-gate green.
- [x] **new modes (shaders)** (2026-06-30) — Sunset (warm gradient + low sun + halo + lit
      cloud bands), Galaxy (tilted Milky-Way fbm nebula + dust lanes + dense starfield),
      Matrix (falling code rain: per-column head + fading glyph trail + flicker). Three
      new procedural WGSL shaders (shader_id 16/17/18), each reading the scene pool;
      verified under cage. naga-gated; per-scene settings groups.
- [x] **forest reworked → split into `pyramids` + a real `forest`** (2026-06-30) — the
      original "forest" read as pyramids, so (owner request) it was renamed to `pyramids`
      (kept as-is: angular silhouettes + mist + glints) and a new `forest` shader added
      (shader_id 19): three receding rows of tall, thin, tiered conifers fading into mist,
      with drifting fireflies + dark-green palette. Both verified under cage.
- [x] **new modes (earthy/landscape family)** (2026-06-30) — Mountains (parallax hazy
      ridgelines, atmospheric perspective), Forest (spiky pine treeline + drifting mist +
      twinkling fireflies), Ocean (sea horizon + narrowing moonlit glitter path; ≠ the
      underwater `water` caustics). Three new procedural WGSL shaders (shader_id 13/14/15),
      each reading the scene pool; verified under cage. naga-gated. ALSO fixed a
      pre-existing bug: rain + snow scrolled **upward** (uv.y grows downward, the time
      term needed to subtract) — now fall correctly.
- [ ] **settings UX** — per-`sky_mode` control group (one shown at a time, labeled to
      the scene); mode picker grouped/expanded for the new set.
- [ ] **docs** — `greeter.toml` keys + Help: which modes are procedural vs palette,
      what each scene's dials do.

### M9 — Global GPU-budget level (ratified D-0014, 2026-06-30) — ✅ COMPLETE (2026-06-30, on master)
`depends:` M7. One global `gpu_level` enum (lite/moderate/high/bonkers, default high)
that bundles render-cost levers — wraps the whole render, never per-scene/per-preset.
TCB-neutral; quality uniform stays internal (not a per-scene dial); `high` keeps the
look byte-faithful; each shader change naga-gated.
- [x] **MVP** (2026-06-30): `GpuLevel` enum (lite/moderate/high/bonkers) + `gpu_level`
      key in door-theme; fps-cap via a fixed-rate frame ticker in the greeter (anim stays
      time-correct, repaints throttled); fbm octave count packed into params9.w and read
      by the shared `fbm` (5 = high/bonkers = unchanged look, 3 = moderate, 2 = lite);
      card-blur pass gated off below high; a global "GPU level" dropdown in door-settings
      Behavior tab; documented in greeter.toml + Help. Verified under cage: high = full
      detail + blur, lite = softer fog + no blur. naga-gated, tests + clippy green.
      (Note: octave dial covers the fbm-heavy scenes; per-scene *loop-count* scaling —
      rain/snow layers, meteor count — folded into the render-scale follow-on rather than
      shipped piecemeal.)
- [x] **fast follow — render-scale** (2026-06-30): `GpuLevel::render_scale()` (0.5 lite /
      0.75 moderate / 1.0 high+bonkers). The `SkyPipeline` gained an offscreen texture +
      a bilinear blit pipeline (`BLIT_WGSL`): below native, the sky renders into a
      scale²-smaller texture (`prepare` resizes it to the physical viewport × scale) and
      the blit upscales it to the surface; at native the original single-pass direct draw
      is kept (no offscreen). The card + text stay sharp (drawn by iced at native); only
      the sky is scaled. Verified under cage across all four tiers (galaxy: lite = intact
      but soft, high = full). naga-gated (blit shader tested), fmt + clippy + tests green.
      **M9 complete.**

### M5 — Hardening pass — ⏳ ACTIVE (current frontier, 2026-07-01)
> Tier 1 (systemd unit hardening) done; Tier 2 part 1 (spawner foundation) done; **Tier 2
> part 2 (live-path flip, gated `DOORD_SPAWNER`) landed at HEAD `9c9e1e1`; its LOGIN path is
> now HARDWARE-VALIDATED** — a `DOORD_SPAWNER=1` drop-in on `genny` ran a real PAM login into a
> `plasma` session over the spawner (2026-07-01, recorded out-of-tree; see the `## Active` note).
> Remaining: confirm the clean logout/teardown (control-EOF → reap) leg, then part 3 flips the
> **shipped** unit default (drop-in → real default) → Tier 3 seccomp / Tier 4 Landlock.
- [x] **session discovery / `.desktop` trust pentest** (2026-06-30) — clean, no findings.
      Session dirs are root-owned system dirs only (no user dir read); `Exec` is tokenized
      → direct `Command::new` exec (no shell injection); `session_id` exact-matched (no path
      traversal); env cleared+rebuilt (no daemon-env leak). Report in `.agent/SECURITY/`.
- [x] **session/seat handoff pentest** (2026-06-30) — clean, no findings; report in
      `.agent/SECURITY/2026-06-30-ipc-pentest.md`. Verified the safety-critical guards:
      signal handler is async-signal-safe (defers `free_seat`, allocation-free VT restore),
      `killpg` foot-guns closed (`pgid > 1` + own-group), `PR_SET_PDEATHSIG` armed after the
      uid drop + fork→arm `getppid()==1` race guard, panic hook frees the seat.
- [x] **seccomp/landlock scoped** (2026-06-30) — `.agent/IDEAS/2026-06-30-seccomp-landlock-scope.md`.
      Central constraint: seccomp+Landlock inherit across fork+execve, and doord's session
      is a fork+execve child in its own mount ns, so any self-sandbox leaks into the user's
      desktop. Real kernel sandboxing needs a pre-forked, un-sandboxed spawner (Tier 2,
      DECISION-gated). PAM phase is un-sandboxable (module diversity) — documented residual.
- [x] **Tier 1 — systemd unit hardening** (2026-06-30) — `dist/systemd/doord.service`:
      shipped the session-safe subset (`NoNewPrivileges`, `ProtectClock/KernelModules/
      KernelLogs/Hostname`, `CapabilityBoundingSet=~<unused caps>`), documented why the
      high-value directives (RestrictAddressFamilies, SystemCallFilter, ProtectHome, …) are
      deferred to Tier 2 (they'd break the inherited session). `systemd-analyze verify` clean.
- [~] **Tier 2 — pre-forked spawner** (DECISION-0015, Binding 2026-06-30). In progress:
      - [x] **part 1 — tested foundation** (2026-06-30, `10205a3`): `doord/src/fdpass.rs`
            (SCM_RIGHTS send/recv, tested with a real pipe fd round-trip) + `spawner.rs`
            (pre-forked helper: fork worker → pass control fd → reap → report exit;
            `request_worker`/`wait_worker_exit` supervisor helpers; PDEATHSIG=SIGKILL).
            End-to-end tested with a stand-in worker. `#[allow(dead_code)]` — not yet on
            the production path.
      - [~] **part 2 — live-path flip, gated `DOORD_SPAWNER`** (2026-06-30, `9c9e1e1`):
            `WorkerLoginFactory::with_spawner` routes worker creation through the spawner
            (`main` forks it after baseline, before serve); `SessionChild::via_control`
            reads session-end as EOF on the worker control fd (not `waitpid`) — the worker
            already closes control on exit (`worker.rs`), and the spawner bare-reaps, so no
            exit-report/desync. `spawn_worker_raw` is the `SpawnFn`. Default = direct path,
            unchanged. Boot-tested in the real binary (`ipc_smoke::boots_and_serves_in_spawner_mode`).
            - [x] **login path HARDWARE-VALIDATED (2026-07-01, recorded out-of-tree)** — a
                  `DOORD_SPAWNER=1` systemd drop-in on `genny` (`/etc/systemd/system/doord.service.d/`)
                  booted doord in spawner mode; journal (`-b`, 10:45 CDT): greeter connected →
                  `authentication succeeded for 'stephen'` → handoff → `started session 'plasma'`,
                  session stable 9h+. The full BeginAuth → real worker (SCM_RIGHTS) → session →
                  start path is proven on real seat/PAM. NOTE: this was a runtime act, not a commit,
                  so D-0037's sync never fired — documented after the fact.
            - [ ] **teardown leg** — confirm the clean logout path (control-EOF → worker reap) on
                  the next real logout (not observed this boot; the validated session is still up).
            - [ ] **flip the shipped default** — part 2 currently on only via the machine-local
                  drop-in; part 3 makes it the default in the shipped `doord.service`.
      - [x] **part 3** — spawner is the shipped `doord.service` default (`DOORD_SPAWNER=1`).
      - [ ] **Tier 3 — supervisor seccomp** (per **DECISION-0016**: `seccompiler`,
            log-before-enforce, applied after `fork_spawner`).
            - [x] **increment 1** — greeter routed through the spawner + concurrent reaper
                  (HEAD `86739f6`); hardware-validated on `genny` 2026-07-01.
            - [ ] **2a** — `hardening::apply_seccomp(mode)` + `seccompiler` dep + `DOORD_SECCOMP`
                  flag, default action `SCMP_ACT_LOG`, supervisor-only after `fork_spawner`.
            - [ ] **2b** — genny boot with `DOORD_SECCOMP=log`; grep journal for denials, tune allowlist.
            - [ ] **3** — flip default action to `SCMP_ACT_ERRNO(EPERM)` once the log run is clean.
      - [ ] **Tier 4 — Landlock paths** on the supervisor (same post-`fork_spawner` step).
- privilege-drop audit ✅ (pentest 2026-06-30), IPC/PAM fuzzing ✅ (ipc_fuzz.rs)
- secrets-zeroization audit ✅ (F1 fix); external review of the TCB
  `depends:` M1, M2
- [x] **IPC/PAM/privdrop pentest** (2026-06-30, `bd85280`) — static audit +
      adversarial fuzz suite (`doord/tests/ipc_fuzz.rs`) against the running daemon;
      full report in `.agent/SECURITY/2026-06-30-ipc-pentest.md`. Found + fixed F1
      (plaintext lingering in the un-zeroized wire buffer → `Zeroizing`); confirmed the
      privilege boundary sound (peercred gate, no `AuthSuccess` forgery, uid-from-PAM,
      Exec never crosses the seam, verified privilege drop refusing uid 0). Privilege-drop
      audit + IPC/PAM fuzzing line items are now done; seccomp/landlock still open.
- [x] **pre-auth greeter TCB pentest** (2026-06-30) — report appended to
      `.agent/SECURITY/2026-06-30-ipc-pentest.md`. Surface is tight: "No network, ever"
      verified (dep + code), config read only from root-owned sources (no user-writable
      path, presets not read pre-auth), no process spawning, malformed config degrades
      gracefully, caps-lock read-only. One Low finding (F3): asset paths (wallpaper/logo)
      decoded pre-auth with no ownership check → exploitable only via admin misconfig of a
      world-writable asset path; mitigations = validate paths root-owned/non-writable +
      cap decode dims.
- [x] **F3 fix — pre-auth asset allowlist + decode cap** (2026-06-30) — the greeter vets
      wallpaper/logo (`app.rs::vet_asset`): canonicalized path must resolve under
      `/usr/share/door` or `/etc/door` (else **refused** with a logged reason + render
      without it), plus an 8192px/40MP raster decode-dimension cap (header-probed, so a
      bomb is never allocated). Dev configs bypass the allowlist, keep the cap. Unit-tested
      (valid image under /tmp refused; missing path refused). fmt+clippy+tests green.

### M7 — Graphics & greeter depth (the showpiece program) — SHIPPED in v0.1.1 (2026-06-30)
Authorized 2026-06-29 (user: "a-F it is"). Groups A–E shipped (TCB-neutral); F items
remain DECISION-gated/parked (D-0012). Full source-grounded inventory +
rationale in `.agent/IDEAS/2026-06-29-graphics-capability-audit.md`. Groups A–E are
TCB-neutral (more uniforms / self-contained naga-validated shaders / unprivileged
greeter UI), so they sit cleanly under Tenet 1. Each shader change is gated by the
existing `naga` parse+validate test.

- [x] **A — expose hardcoded shader constants as knobs — COMPLETE (2026-06-29).**
      Each = `Theme` field + uniform + door-settings control (100%-mapped invariant
      held); defaults reproduce the prior look; naga + config round-trip tests green;
      verified live under cage. 17 new knobs across three commits:
      - [x] slice 1 — **sun** position/size/intensity (`7d179bf`).
      - [x] slices 2–3 — **sun color**, **night glow center** (`glow_x`/`glow_y`),
            **day haze** (`0d83a22`). Added a shared-color control path (`color_cell_with`).
      - [x] slices 4–6 — **stars** (layers, size) + **nebula drift**, **sky comet**
            (tilt/pause/width), **clouds** (lit/shadow colors), **spinner orbit radius**
            (`29ba879`).
      - note: cloud *layer count* left fixed (two hand-tuned `cloud_layer` calls, not a
        clean loop like the stars) — deliberately not exposed.
- [x] **B — new sky modes — COMPLETE (2026-06-29).** `sky_mode` selector; each scene =
      one `SkyMode` variant + one shader branch (picker auto-reads `SkyMode::ALL`). 12
      scenes + 2 selectors (auto, seasonal). naga-gated, clippy clean, all verified live. `depends:` A
      - [x] **slice 1 — infrastructure + aurora** (2026-06-29, `a21392e`): `SkyMode`
            enum (Auto|Aurora), `params5.w` dispatch, door-settings Scene picker,
            `aurora_sky()` curtains. Verified live; Auto unchanged.
      - [x] **slice 2 — storm, rain, snow** (2026-06-29, `50a4fbf`): lightning
            (flash + jagged bolt), parallax rain streaks, drifting snow; comet gated to
            auto/aurora. No settings change (picker reads `SkyMode::ALL`).
      - [x] **slice 3 — meteor, moon, synthwave** (2026-06-29, `f9474ea`): diagonal
            meteor shower, phased moon (drifting terminator), synthwave sun + neon grid.
      - [x] **slice 4 — fog, plasma, fire, water** (2026-06-29, `a9cb4aa`): drifting fog
            banks, demoscene plasma, rising fire, underwater caustics. **12 scenes total.**
      - [x] **seasonal auto-select** (2026-06-29, `5b71614`): `SkyMode::Seasonal` →
            scene by month (spring=rain, summer=meteor, autumn=fog, winter=snow);
            door-theme stays calendar-free (caller passes the month).
      - [x] **palette-pin** (2026-06-29, `701d40b`): explicit scenes pin the night card
            via `prefers_day()`; auto/seasonal still follow the clock.
- [x] **Preset library** (2026-06-29, `0561b8a`+`4476aba`): **21 stunning** scene-tuned
      presets — first 12 (aurora-borealis, thunderstorm, rainy-night, snowfall,
      meteor-shower, moonlit, outrun, misty-morning, plasma-dream, ember, deep-ocean,
      wanderer) + 6 more (sakura, blood-moon, noir, emerald, cyberpunk, vaporwave) +
      the 3 originals. Auto-shipped (PKGBUILD globs the dir) and auto-listed in
      door-settings; the parse test reads the dir so every preset is validated.
      (Part of M7-E's preset work.)
- [x] **Default polish** (2026-06-29, `fa297b2`): built-in day & night now ship a
      subtle card sheen (`card_gradient` 0.45) + gentle `vignette` 0.2 for depth;
      palette unchanged.
- [~] **C — composition**. `depends:` M4
      - [x] **slice 1 — card placement** (2026-06-29, `a876d73`): `CardPos`
            center/left/right/top/bottom → container alignment + settings picker/preview.
      - [x] **slice 2 — cursor-parallax sky** (2026-06-29, `85acfc7`): `cursor_parallax`
            knob; cursor fed into the shader, star layers drift per-depth.
      - [x] **slice 3 — card gradient + grain + vignette** (2026-06-29, `94994de`):
            `card_gradient` (iced linear sheen), `grain` + `vignette` (sky shader
            post-effects). All default off. ⚠ visual screenshot pending — cage harness
            wedged this session; verified by build/naga/tests/clippy/dev-run.
      - [x] **slice 4 — true backdrop blur** (2026-06-29, `9bc113d`): a 2nd shader pass
            (`fs_frost`) re-samples the procedural scene blurred, masked to the card's
            rounded rect, drawn under the card via `Stack::push_under`. `card_blur` knob
            (default on) + settings toggle + live preview. Refactored sky into
            `scene_base`/`night_sky` (night verified pixel-identical). Verified live.
      - [ ] per-monitor wallpaper (multi-output; bigger slice).
      - [ ] animated sunrise day↔night transition (low priority — login is brief).
- [~] **D — clock / logo / type**. `depends:` M4
      - [x] **slice 1 — clock** (2026-06-29, `c99c937`): `clock_seconds` + `clock_size`.
      - [x] **slice 2 — spinner styles** (2026-06-29, `f046d80`): comet/ring/dots/pulse.
      - [x] **slice 3 — font scale** (2026-06-29, `0812556`): `font_scale` large-text.
      - [x] **analog clock face** (2026-06-29): `clock_style = digital | analog` —
            a shared canvas widget (`door_theme::clock::AnalogClock`) with a ticked
            rim + hour/minute/second hands, smooth-sweeping when animating. Rendered
            by both greeter and settings preview.
      - [x] **custom clock format** (2026-06-29): `clock_format` strftime string for
            the digital clock (via libc strftime, no new dep); settings Behavior-tab
            input + a live-time preview sample. Timezones still open.
      - [ ] font weight, SVG + animated logo, custom timezones.
- [~] **E — functional / UX**. `depends:` M4
      - [x] **slice 1 — reduced-motion accessibility** (2026-06-29, `a7088ea`):
            `reduced_motion` flag stills all motion at load.
      - [x] **slice 2 — settings randomize** (2026-06-29, `cee45e8`): 🎲 loads a random
            preset.
      - [x] **large-text accessibility** (2026-06-29, `0812556`): `font_scale` (shared
            with D slice 3).
      - [x] **settings FPS badge** (2026-06-29, `e4e803a`): live preview frame-rate.
      - [x] **greeter config hot-reload** (2026-06-29): the greeter re-resolves the
            theme each Tick and swaps on change — edits to `greeter.toml` (and the
            day/night boundary) apply live, no restart.
      - [x] **HSV color picker** (2026-06-29): clicking any palette swatch opens a
            visual saturation/value square + hue strip (a canvas widget, drag to
            pick) over a dim backdrop; emits `Set(param, hex)`, preserves alpha.
      - [x] **live preset thumbnails** (2026-06-29): a horizontally-scrolling gallery
            of mini greeter-card previews (each in its preset's parsed night palette),
            click-to-load with an accent ring on the active one.
      - [ ] settings polish: import/export.
      - [x] **caps-lock indicator** (2026-06-29, D-0012): reclassified TCB-neutral —
            greeter reads the local `capslock` LED (no daemon protocol); a warning
            slips under the password field when on.
      - [ ] **DECISION-gated (threat-model first, D-0012 parks these):** user list +
            avatars, keyboard-layout / battery / network indicators — these add
            pre-auth greeter↔daemon protocol or shoulder-surfer disclosure; do not
            barrel.

### M-F — Pre-auth TCB-expanding graphics (DECISION-gated; NOT ready work)
Off the ready queue by design — Tenet 1 ("a feature that widens the privileged
attack surface loses") + the hard constraints. Each needs an explicit, dated,
off-by-default DECISION naming its threat model before it can graduate.
- [ ] **F1 — custom user `.wgsl` background**: admin-supplied fragment shader as the
      sky. Admin-controlled config (no new *privilege* boundary), but widens the
      greeter's input surface → requires naga validation + resource/time bounds + a
      DECISION. `depends:` A
- [ ] **F2 — video / GIF wallpaper**: pulls a media-decode stack into the (unprivileged)
      greeter — a known CVE class. Needs a DECISION; likely declined under Tenet 1.
- [ ] **F3 → promoted out of M-F: weather-driven sky via helper-file split**
      (chosen 2026-06-29). The greeter-dials-out version stays rejected ("No network,
      ever"); this version **upholds** the constraint, so it's a normal feature, not a
      supersession. A **post-login** helper (systemd timer / user service — never the
      greeter, never the pre-auth daemon path) fetches weather and writes a local
      cached state file (e.g. `/run/door/sky.json`); the greeter just reads that file
      (it already reads local config). Maps weather → `sky_mode` (clear→sun, rain→rain,
      storm→lightning, snow→snow…). Zero network code on the unauthenticated boot/login
      surface. `depends:` B (the sky modes it selects)

## Loose

- ~~Greeter dropout glitch~~ — **fixed 2026-07-01.** A blanket 30s `READ_TIMEOUT`
  on the greeter socket dropped the connection whenever a human paused at the
  prompt (username not yet typed, or mid-password), churning the login screen
  every `READ_TIMEOUT`. `ipc::read_greeter_request` now splits idle-vs-transfer:
  an unbounded `poll` while waiting for a frame to *start*, then `READ_TIMEOUT`
  only once bytes are arriving (slowloris/partial-frame bound preserved). Both
  greeter read paths route through it — the serve loop (`ipc.rs`) and the
  auth-reply wait where the user is actively typing (`pam.rs`). Builds + all
  `doord` tests green. (No injectable-timeout regression test yet — asserting
  the unbounded idle path would need to sit past the 30s timeout.)

- ~~Decide greeter toolkit (GTK4 / Qt-QML / Iced / bespoke wgpu)~~ — **resolved
  2026-06-26: Iced + iced_layershell (D-0006).**

## Shipped

- **v0.1.3 → AUR** (2026-07-01): tagged `v0.1.3` (commit `441cbb6`), live on the
  AUR (pkgver 0.1.3, pkgrel 1). Two items, both traced to an AUR upgrade bug
  report — `door 0.1.2` failed with `pacman` *conflicting files* on
  `/usr/share/door/presets/*.toml`:
  - **install-local hardening** (`84ae429`) — the collision root cause was
    `scripts/install-local.sh` writing the working tree's full preset set into the
    *same* `/usr/share/door/` paths the package owns, seeding files `pacman`
    doesn't track (not a door bug — door-settings saves user presets to
    `~/.config/door/presets`, never `/usr/share`). install-local.sh now warns when
    it shadows a `pacman`-managed door (offering the `--overwrite '/usr/share/door/*'`
    return path) and reseeds the system preset dir from a clean slate (`rm -rf`
    then reinstall) so a renamed/removed preset can't leave a stale unowned file.
  - **Expert settings launcher** (`84ae429`) — new
    `dist/door/door-settings-expert.desktop` (`Exec=door-settings --expert`): a
    second Settings menu entry that opens with the Tier-3 controls revealed,
    alongside the plain one; installed from both PKGBUILD and install-local.sh.
  - release note: `pkgver`/`Cargo.toml` version must move together — the first
    release attempt 404'd because only `PKGBUILD` was bumped while `release.sh`
    tags off `Cargo.toml`'s `[workspace.package] version` (fixed in `441cbb6`);
    the AUR push then needed a retry after a transient SSH drop.

- **v0.1.2 → AUR** (2026-07-01): tagged `v0.1.2`, live on the AUR (pkgver 0.1.2,
  pkgrel 1, commit `cefb329`), published via `scripts/release.sh`. Single fix:
  the **greeter dropout glitch** (`cfd93c4`) — a blanket 30s `READ_TIMEOUT` on the
  greeter socket dropped the connection whenever a human paused at the prompt,
  churning the login screen every `READ_TIMEOUT`. `ipc::read_greeter_request` now
  splits idle-wait (unbounded `poll`) from in-flight transfer (`READ_TIMEOUT`
  bounds only a started frame, preserving the slowloris guard); both greeter read
  paths route through it. Version bump needed because the AUR PKGBUILD builds from
  the `v$pkgver` tag tarball, so a code fix can't ship on a bare `pkgrel` bump.
  (AUR push initially failed on a transient SSH drop; retry from the package clone
  succeeded.)
- **v0.1.1 → AUR** (2026-06-30): tagged `v0.1.1`, live on the AUR (pkgver 0.1.1,
  pkgrel 1), published via `scripts/release.sh`. Ships the **M7 graphics & greeter
  depth** program plus the **door-settings overhaul**:
  - Greeter/theme (door-theme + greeter): 12 GPU sky scenes (A/B) + 23 presets
    (incl. accessibility high-contrast/colorblind), seasonal mode, analog clock
    (`clock_style`), custom `clock_format` (libc strftime), `font_weight`,
    hidden-spinner (`spinner_style = none`), opt-in night-glow `glow_pulse`,
    live cursor parallax, SVG logos, Caps-Lock indicator (D-0012), and live
    config hot-reload (re-resolve theme each tick).
  - door-settings: two-column tabbed layout with pinned header/tabs/actions,
    a Markdown **Help** tab (provenance), searchable preset `combo_box` + a
    **wrapping preset grid** with live mini-card thumbnails, the interactive
    **HSV colour picker**, alpha-checkerboard swatches, hover tooltips, and
    eased button transitions (`iced_anim`).
  - **Bug fix (notable):** the settings **black-screen** on reduced-motion
    presets — a zero-size `Space` as the top-level `stack!` base collapsed the
    layout and dropped the content layer (looked black; hit at random via the
    dice). Fixed with a Fill-size base; greeter unaffected. Long multi-layer
    diagnosis ruled out GPU/driver/MSAA/format/leak/winit before isolating the
    layout cause (reproducible headless + on software). Regression test added
    (every preset loads + builds).
- **v0.1.0 → AUR** (2026-06-28): the tagged `v0.1.0` release is **live on the AUR**
  (`aur.archlinux.org/packages/door`), built from the GitHub source tarball via
  `scripts/release.sh` (public flip + tag push + AUR publish). Public-history audit:
  governance (`.agent/`, `CLAUDE.md`) was gitignored and **never tracked**, so the
  history is clean — the `git-filter-repo` scrub (`door-scrub/`) and `untrack-agent.sh`
  helper were belt-and-suspenders for a non-existent leak, and were discarded. README
  flipped from "(once published)" to the live package link; `scripts/install-local.sh`
  committed as the working-tree dev-install counterpart to the PKGBUILD.
- **M6 — Packaging + reversible install** (2026-06-27): door ships as an Arch
  package (`PKGBUILD` + `door.install`) **installed disabled by default** — never
  enables a unit, never touches the active DM; installs `doord.service` +
  `dist/pam.d/{doord,door-greeter}` + `dist/sysusers.d/door.conf`. doord owns the
  greeter lifecycle (D-0008): greet → serve → handoff (terminate the greeter, free
  the VT) → wait → re-greet, with crash-loop backoff. Sessions are tied to doord's
  lifetime; the seat is freed by killing the compositor's process group (D-0009).
  **Done-when met, proven on hardware:** package install → `enable --now` → real
  greeter login into Plasma → two-command TTY revert (`disable --now doord` +
  `enable --now sddm`) restores the previous DM cleanly. The live-enable lockout
  postmortem found and fixed **nine** root causes, all proven on hardware — RC1
  socket-dir perms, RC2 recoverability/give-up, RC3 handoff orphan, RC4 non-UTF-8
  locale, RC5 admin-teardown VT reset, RC6 greeter shader-cache, RC7 pre-handshake
  wedge, RC8 console stdio, RC9 seat-squat-on-revert (the one that made the revert
  actually work): see `.agent/REPORTS/2026-06-26-m6-lockout-postmortem.md` and
  D-0007/8/9. 26 unit + 3 integration tests green, clippy clean.
- **M3 — Minimal greeter (functional)** (2026-06-26): `door-greeter`, the
  unprivileged Iced + iced_layershell client (D-0006). `client.rs` (protocol
  client, 4 tests) + `app.rs` (layer-shell overlay: session picker,
  username/password, sign-in, power; background worker owns the blocking client,
  bridged to Iced via a `stream::channel` subscription). Holds no credential
  beyond submit; starts no session itself. **Verified live** against a wlroots
  compositor (connect → list → render → auth → `Start`); `DOORD_GREETER_DEV=1` mode
  for safe nested testing. Open follow-ups carried into M6/later: production host
  compositor (cage lacks layer-shell), daemon-side `Power`.
- **M2 — Session discovery + launch** (2026-06-26): `.desktop` session discovery
  (`sessions`), auth-gated identity-bound `Start`, and the full logind handoff —
  pam_systemd registration (D-0004) with a **per-login session worker as the
  logind leader** (D-0005): the daemon re-execs a short-lived worker that owns the
  PAM transaction, `setsid`s + takes the seat's VT as controlling tty, drops
  privilege, runs the session in the sanitized allowlist ∪ PAM env, then closes the
  session and exits — the daemon never enters a session scope, and greeter framing
  never reaches the worker (`O_CLOEXEC`). Threat model `SECURITY/session-spawn-threat-model.md`
  (S1–S13). **Live-confirmed** (multi-login, clean close, daemon out of scope).
- **M1 — Privileged core skeleton (`doord`)** (2026-06-25): Cargo workspace +
  hardened `protocol` crate (D-0001 message types, version handshake, redacted
  `Secret`, strict serde, shared framing, D-0003); peercred-checked Unix-socket
  IPC server; real PAM auth conversation (`Authenticator`/`AuthChannel` seam,
  min-failure-delay); `privdrop::{drop_to,sanitized_env}` (mechanism unit-tested,
  wired into the spawn path in M2); auth-path threat model. **Live root run
  confirmed:** `✓ AUTH SUCCESS` over the peercred socket as the authorized greeter
  uid, foreign uid (root) refused at the peercred gate — both the `0660`/group FS
  reach and the `SO_PEERCRED` uid match demonstrated on real hardware.
- **M0 — Vision, scope, architecture decisions** (2026-06-25): brief locked,
  PROJECT-SCOPE committed, D-0001 (no greetd / bespoke protocol) and D-0002
  (Rust) ratified Binding.
