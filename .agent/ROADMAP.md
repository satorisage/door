# door — ROADMAP

The work-structure tree (milestone → task, with `depends:` edges). The active
milestone's task tree is the plan; `.agent/TODO.md` is its derived ready-frontier.

## Active

### M0 — Vision, scope, architecture decisions
- [x] vision brief locked (`.agent/REPORTS/project-brief.md`)
- [x] PROJECT-SCOPE.md committed
- [ ] **ratify broker strategy** (CHECKIN-0001 → DECISION-0001) — *Critical, blocks M1*
- [ ] **ratify implementation language** (CHECKIN-0002 → DECISION-0002) — *Critical, blocks M1*
- [ ] one-page architecture sketch: `doord` / `door-greeter` split + IPC seam
      `depends:` DECISION-0001, DECISION-0002

**Done-when:** scope committed; both Critical decisions filed (ratified or
Proposed-with-ratify-task); architecture sketch exists.

## Backlog (future milestones, not yet sequenced)

### M1 — Privileged core skeleton (`doord`)
- PAM auth conversation over a peer-cred-checked Unix socket
- privilege drop + sanitized session environment
- threat model written for the auth path
  `depends:` M0

### M2 — Session discovery + launch
- discover `/usr/share/wayland-sessions` + `xsessions`
- spawn honoring `.desktop` `Exec=` (incl. wrapper launchers like `start-hyprland`)
- `logind` seat/VT/session wiring
  `depends:` M1

### M3 — Minimal greeter (functional, ugly)
- Wayland client: session picker, password field, power controls
- speaks the IPC protocol; holds no credential beyond submit
  `depends:` M1

### M4 — The beautiful greeter
- first beautiful default: animation, theming, system-wide assets
  `depends:` M3

### M5 — Hardening pass
- seccomp/landlock bounding, privilege-drop audit, IPC/PAM fuzzing
- secrets-zeroization audit; external review of the TCB
  `depends:` M1, M2

### M6 — Packaging + reversible install
- Arch PKGBUILD; installed-but-disabled by default
- installer prints the two-command TTY revert; previous DM kept as fallback
  `depends:` M2, M4

## Loose

- Decide greeter toolkit (GTK4 / Qt-QML / Iced / bespoke wgpu) — Material.

## Shipped

- (none yet)
