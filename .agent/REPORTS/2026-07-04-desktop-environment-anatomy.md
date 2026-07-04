# Desktop-environment anatomy — what makes Plasma *Plasma*, and door's path into it

**Date:** 2026-07-04 · **Requested by owner** ("what would it take to move forward
with door as the first piece of a desktop environment? what other modules would we
need? what runs the desktop? fill me in. seriously.") · Companion to
`.agent/IDEAS/2026-07-04-door-compositor.md` (Tier B is the load-bearing module
below). Reference/analysis, not a commitment.

## 1. The one-sentence answer

On Wayland, **the compositor runs the desktop** — it is the display server, the
window manager, and the protocol hub in one process; everything else (panels,
launchers, notifications, the login screen, the locker) is a *client* of it or a
D-Bus service beside it. A DE is: **one compositor + a family of clients/services
sharing one design language + the freedesktop specs that let them interoperate.**

## 2. What runs a Plasma session, bottom-up

| Layer | Plasma's module | What it actually does |
|---|---|---|
| Login | **SDDM** | pre-session auth + session launch (**door already replaces this**) |
| Display server / WM | **KWin** | owns GPU outputs, input devices, window management, all Wayland protocols (xdg-shell, layer-shell, screencopy, …), XWayland for X11 apps |
| Shell | **plasmashell** | wallpaper, panels, taskbar, launcher, system tray (StatusNotifier), notifications (`org.freedesktop.Notifications`), widgets |
| Session manager | **plasma-session / startplasma** | starts the session: environment, systemd user units, XDG autostart, session restore, logout |
| Settings | **systemsettings + KCMs** | one UI over config: theming, displays (KScreen), input, power (PowerDevil) |
| Locker | **kscreenlocker** | built into the KWin/Plasma pair (why door-lock can't attach there) |
| Privilege UI | **polkit-kde-agent** | the GUI "enter password to do admin thing" prompt; without one, polkit actions just fail |
| Portals | **xdg-desktop-portal-kde** | file pickers, screenshot/screencast (PipeWire), global shortcuts — how sandboxed AND modern Wayland apps reach the desktop |
| Secrets | **KWallet** (`org.freedesktop.secrets`) | password/token storage for apps |
| Glue daemons | **kded, klipper, plasma-nm, plasma-pa, bluedevil, device notifier** | clipboard history, network UI over NetworkManager, audio UI over PipeWire, Bluetooth over BlueZ, mounts over UDisks2 |
| Foundation | **Qt + KDE Frameworks (~70 libs)** | the shared toolkit + libraries — the single biggest reason Plasma apps all look/behave alike |

**The nervous system is D-Bus** (notifications, portals, secrets, polkit, logind
are all D-Bus interfaces). **The organs are system services no DE writes:**
logind, NetworkManager, PipeWire, BlueZ, UDisks2, UPower, polkit. Every DE —
Plasma, GNOME, a hand-rolled sway rig — reuses those. That is a huge chunk of
"a desktop" that is *already not our problem*.

## 3. The existence proof that modules compose

The wlroots world already assembles DEs from parts, glued only by standards:
sway (compositor) + waybar (panel) + rofi (launcher) + mako (notifications) +
swaylock (locker) + swayidle (idle) + a polkit agent + xdg-desktop-portal-wlr.
Each is a separate program. The seams are: **Wayland protocols** (layer-shell is
what lets a separate process *be* the panel), **D-Bus specs**, and **XDG specs**
(autostart, desktop entries, icon themes). A "door DE" is that shape, but
Rust, one aesthetic, one config discipline — the door values applied upward.

## 4. The door module map (what we'd author vs. reuse)

Already built:
- **door** (DM) — the entry point. ✓
- **door-lock** — locker; works day one on any ext-session-lock compositor,
  including ours. ✓
- **door-theme / door-settings** — the design language + its editor; grows
  sections per module. ✓ (this is our "mini-KF": iced + door-theme everywhere)

To author, in rough dependency order:
1. **door-stage** (IDEA Tier A) — kiosk compositor replacing cage under the
   greeter. Weeks-scale. De-risks Smithay; pure-Rust pre-auth TCB.
2. **door-comp** (IDEA Tier B) — *the* mountain: the session compositor.
   Smithay foundation; xdg-shell, layer-shell, ext-session-lock,
   ext-idle-notify, screencopy, XWayland, output/input management. Everything
   else in a DE is a client of this. Many months, permanent tail.
3. **door-shell** — panel/launcher/tray/notification daemon as layer-shell
   clients (iced, door-theme). Second mountain, but shippable in slices:
   wallpaper+clock panel first; StatusNotifier tray and the notifications
   D-Bus service are the gnarly halves.
4. **door-session** — session startup/env/autostart/logout; doord already owns
   the launch seam, this grows out of it. Hills, not mountains.
5. **door-idle** — tiny ext-idle-notify client that triggers door-lock (the
   F-lock-3 backlog item, promoted to a module).
6. **Portal backend + polkit agent** — reuse `xdg-desktop-portal-wlr`/`-gtk`
   and any existing polkit agent initially; author door-native ones late, if
   ever.

Reused forever (not DE work): logind, NetworkManager, PipeWire, BlueZ,
UDisks2, UPower, polkit, XDG portals frontend.

## 5. Honest costs (the things that kill from-scratch DEs)

- **App-compat grind**: XWayland quirks, screen sharing, drag-and-drop,
  clipboard between X and Wayland, fractional scaling, multi-GPU. This is
  where the years go — not the happy path.
- **Accessibility** (AT-SPI) and **i18n**: quietly enormous; every from-scratch
  DE punts and then regrets it publicly.
- **The tray**: StatusNotifier + the legacy XEmbed fallback is legacy-spec
  archaeology.
- **Maintenance tail**: a compositor is never "done"; protocols keep moving.

## 6. The sane sequence (each step ships standalone value)

door (✓) → **door-stage** (replaces cage; proves Smithay; value even if the DE
never happens) → door-stage grows into **door-comp** on a spare/test machine →
**door-lock goes native** on it (M10's bound dissolves on our own stack) →
**door-shell** slices → **door-session** → portals/agents last. Off-ramps at
every arrow; no step is wasted if the vision stops there.

## 7. Status

This is a map, not a decision. Graduation path per D-0028: an owner-ratified
DECISION (likely: ratify Tier A as a milestone first; Tier B/DE as its own
`.agent`-tracked project with door as sibling), then ROADMAP tasks.

## Addendum (2026-07-04): naming — D-0020

The owner ratified the **door-parts convention** (DECISION-0020): every module
is named as a part of, or a figure at, the door. The §4 module map's names are
reserved accordingly: door-stage → **doorstep**, door-comp → **doorframe**,
door-shell's pieces → **sill** (panel) + **doorbell** (notifications),
door-session → **threshold**, door-idle → **latch**; plus **doorman** (polkit
agent) and **keyhole** (secrets) when their day comes. The environment's
umbrella name is parked (candidate: **foyer**) until a shell exists.

## Addendum (2026-07-04, later): graduated — D-0021

This map is now the **founding brief of `foyer`** (`~/Projects/foyer`), the
DE's own `.agent`-tracked sibling project (DECISION-0021; umbrella name
unparked, superseding the parking clause above in part). foyer owns §4's
modules 2–6 (doorframe, sill, doorbell, threshold, latch + the late
doorman/keyhole); **doorstep (module 1) stays a door concern** (pre-auth TCB).
This copy remains door's historical record; foyer carries its own.
