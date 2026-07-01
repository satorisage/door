# Project Brief — door

> Authored from Stephen's locked direction ("set up a beautiful, glorious,
> SECURE login manager for Linux") plus the architecture analysis that
> preceded it. Treat every heading as ratifiable — sharpen, don't assume.

## Vision

**door** is a beautiful, security-first **display manager** (login manager)
for Linux — the program that owns the screen before any user session exists
and decides which session begins. It is a *full* login manager, not a greeter
riding on someone else's broker: door owns the whole pre-session stage —
authentication, seat/VT, session handoff — and pairs it with a greeter that is
genuinely gorgeous (animated, themeable, Wayland-native), the way SDDM aspires
to be but with a hardened core underneath.

Success looks like: door replaces greetd+ReGreet (and is a credible
alternative to SDDM/GDM) on Stephen's own machines first — visibly more
elegant than SDDM, and with a *defensible* security story that a from-scratch
login manager normally cannot claim. "The door to your system": the first and
last thing you see, fast, pretty, and trustworthy.

## Platforms & stack

- **Target OS:** Arch / CachyOS first (Stephen's daily driver — Dell Latitude
  7420), with portability to any systemd-`logind` Linux as a secondary goal.
- **Session layer:** Wayland-first greeter. Launches both Wayland and X11
  sessions discovered from `/usr/share/wayland-sessions` and `/usr/share/xsessions`,
  honoring each `.desktop` `Exec=` (incl. wrapper launchers like `start-hyprland`).
- **System integration:** PAM (auth), systemd-`logind` (seats/sessions/VT),
  optionally `seatd` where `logind` is absent. No network — local only.
- **Implementation language:** memory-safe for the privileged path —
  **Rust** is the presumptive choice (greetd and ReGreet are Rust; it removes a
  whole bug class from the most security-critical code). *Ratifiable.*
- **Greeter rendering:** a real Wayland client (animation/shaders possible,
  e.g. the genny "comet" layer-shell wallpaper). Toolkit TBD (GTK4, Qt/QML,
  Iced, or a bespoke wgpu surface) — a scope decision.

## User-facing surface

Graphical (the pre-session greeter: session picker, user list, password
field, power controls) **and** programmatic (a local IPC protocol between the
privileged daemon and the unprivileged greeter — the security seam). No CLI
beyond an admin/config surface and a `--check`/preview mode.

## Personas / roles & domain-term glossary

**Personas**
- **The person logging in** — wants to get into their session fast, see
  something beautiful, and trust that the box in front of them is the real
  login and not a credential trap. The 99% path.
- **The administrator / packager** (often the same person, Stephen) — installs,
  configures, themes, and *switches the machine to* door. Cares about
  reversibility (a bad greeter locks you out) and about not widening the
  attack surface of their own machine.
- **The attacker** — an explicit persona for a security-first project. Has
  local access (the lock-screen / shoulder-surf / evil-maid threat) or has
  compromised the unprivileged greeter and is trying to cross into root.

**Roles & dual/multi-role users**
- Admin and end-user collapse into one person on a single-seat workstation —
  but their *needs* don't collapse: the admin needs a TTY revert path, the
  end-user needs a fast pretty login. Don't let "it's just me" erase the
  recovery-path requirement.

**Domain-term glossary (overloaded terms flagged)**
- **"greeter"** — here means specifically the *unprivileged UI process*, NOT
  the whole login manager. door (the manager) ≠ door-greeter (the UI). Keep
  these distinct everywhere; conflating them is conflating the trust boundary.
- **"session"** — overloaded: a `logind` session (the kernel/seat object) vs. a
  desktop session (`.desktop` `Exec=` → Hyprland/Plasma). Both appear; name which.
- **"login"** — the auth event (PAM success) vs. the visible form. The security
  claims attach to the former; the elegance claims to the latter.

## Methodology

- **Privilege separation is the cornerstone, not a feature** — a minimal
  privileged daemon owns PAM/`logind`/VT/session-spawn; a separate *unprivileged*
  greeter renders UI and never holds more authority than "ask the daemon to try
  these credentials." A compromised greeter must not be root. This is the
  SDDM/greetd model, adopted *deliberately and audited*, not reinvented blindly.
- **Threat-model-first** — every milestone names what it defends against and
  what it explicitly does not. Reversibility (TTY revert in two commands) is a
  design requirement because the only real test is the destructive switch.
- **Committed:** memory-safe core, least privilege, small TCB, declarative
  tracked config (the files are the engine; the installer only places them).
- **Not committed:** no XDMCP / network login (ever), no X11-rendered greeter
  (Wayland-first), no theming engine before the first beautiful default exists.

## Constraints

- **Worst failure mode in the stack:** a broken greeter locks the user out of
  the GUI with no graceful degradation and no windowed preview — recovery is a
  TTY or a live USB. Reversibility and journald-visible greeter errors are
  non-negotiable.
- **Security is the headline:** the privileged surface (PAM conversation, IPC,
  VT/seat ownership, session spawn, env sanitization) is the whole point — it
  must be small, auditable, privilege-dropping, and ideally seccomp/sandbox-bounded.
- **Exactly one DM owns the machine:** installed-but-disabled fallbacks are
  fine; two enabled DMs racing VT1 is the classic lockout bug.
- **Pre-session environment:** the greeter runs as a system user with no human
  `$HOME` — every asset (theme, cursor, font, shader, wallpaper) must be
  installed system-wide and world-readable or it silently falls back to ugly.
- **Single maintainer** (Stephen) to start — scope must stay shippable by one
  person; the security bar cannot be hand-waved to fit that.

## Expertise needed

Linux session/seat internals (PAM, `logind`, `seatd`, VT/DRM, `pam_systemd`);
the greetd IPC protocol and Desktop Entry spec as prior art; Wayland client +
compositor basics for the greeter; Rust systems programming with a security
lens (privilege drop, sandboxing, fuzzing, constant-time/secret-handling
hygiene); and UI/UX + GPU/shader work for the "beautiful" half.

## Gaps / unknowns

- **Broker strategy (Critical, open):** does door's daemon speak the existing
  **greetd IPC protocol** (instant greeter compat, proven design) or define its
  own? Reuse the protocol, reuse the whole daemon, or build clean?
- **Greeter toolkit (open):** GTK4 vs. Qt/QML vs. Iced vs. bespoke wgpu — trades
  beauty/animation against binary size and dependency surface.
- **Hardening depth for v1:** how far into seccomp/landlock/fuzzing/external
  audit does v1 go vs. v2.
- **Lock-screen scope:** is door *also* the session locker, or login only? (The
  brief currently scopes login only; locking is a candidate, not committed.)
- Many open architecture forks → this project leans **research/design-first**
  before heavy implementation.
