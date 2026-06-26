<!-- GENERATED-BY: scope/scope.sh on 2026-06-26T02:01:10Z -->
<!-- Source: /home/stephen/Projects/dotagent/scope -->

# Scope Elicitation — Interview Pack

You are conducting a structured interview to populate a project's
`.agent/PROJECT-SCOPE.md`. The user opens this pack inside the
target project. Your job is to ask the questions in the **Interview**
section, one section at a time, and at the end emit a populated scope
file that follows the **Scope Template** structure exactly.

This is advisory. You produce the populated scope file as a final
markdown block; the human reviews and commits it. Do not write the
file yourself.

The **Principles** below are the source of truth the project will be
measured against. The **Scope Template** is the structure the
populated file must match. Optional **Additional context** sections
(included via `--include`) give domain background — typically a
pairings bundle the user has already selected.

---

# Principles (source of truth)

The text below is the canonical content of `personal/PERSONAL-PRINCIPLES.md`.
Reference principles by number when asking questions or producing the
populated scope.

# Personal Working Principles

These are the principles I apply across all projects I build. Individual
projects can extend, override, or de-prioritize these in their own
`PROJECT-SCOPE.md`, but in the absence of project-specific guidance, these
apply.

These principles are about *how to design and reason* — they do not
dictate implementation choices (language, runtime, framework, stack,
storage). Language and stack choices belong in `PROJECT-SCOPE.md` per
project, where they can be debated against specific requirements. If an
agent reads these and infers a specific implementation constraint
(e.g., "must be bash," "must be TypeScript"), that's a misread —
escalate it back to the project's scope file.

## Orthogonality

The spine. Two things are orthogonal when they sit on independent axes —
changes to one don't ripple into the other, and the same fact is never
represented in two places. The three subsections below establish axes,
keep engines and instructions on separate ones, and preserve axis
separation across modules.

### Establishing axes

1. **Vision down to detail.** Start from the overall picture, even if it's
   wild or only partially formed. The vision is what tells you which axes
   exist. Share specifics where I have conviction; leave the rest loose
   for the design to discover. Don't build detail-up without a vision.

2. **Upfront anticipation over reactive patching.** Enumerate the
   possibilities each axis must handle *before* building it, not after.
   Reactive patches couple by default — the new case gets stapled to
   whatever's nearest. This applies to scope, naming, module boundaries,
   and especially to engines (see #4). Anticipation means *enumerating
   the case-space and naming the undecided cases as parked forks* — not
   resolving them all upfront (that deferral is #3's job). Enumerate the
   axes; park the details.

3. **Modularity absorbs ambiguity within axes.** Identifying which axes
   exist is upfront work (#1, #2, #4). What lives on each axis can be
   deferred: when a detail is undecided, carve out a module to own it
   later rather than guessing or hardcoding. A module boundary is a
   deferred-decision marker — it isolates the unknown from everything
   around it. A deferred-decision marker should be *named and written* —
   a stub module, an address with no children yet, an `Open` fork — not
   left implicit. An empty-but-named boundary is valid and resolvable
   later. When the axis itself is unclear, you're still in vision work
   (#1), not yet in design.

### Engines and instructions as orthogonal layers

*"Engine" here is a role — the thing that handles a job's full
case-space within its module boundary. It is **not** an implementation
choice. A shell script, TypeScript module, Rust binary, SQL query, or
hosted service can each be the engine for the job they own. Per-project
scope decides implementation; the principles do not.*

4. **Engines are built to handle every possibility they could have to deal
   with, from the start — as much as possible.** Design the engine
   against the full range of what it might be asked to do, not just
   today's use case. A case discovered later shouldn't require changing
   the engine; if it does, use-case has coupled to engine internals and
   the upfront thinking was incomplete. This is a design *target*, not a
   claim of completed coverage — claims about coverage are governed
   by #9. An engine covers the *union* of cases across all anticipated
   use-sites; each site opts into a *subset*. Capabilities a site doesn't
   need stay optional and degrade silently (same engine, different
   subset).

5. **Instructions sequence engine capabilities; they don't extend them.**
   Engine and instructions live on orthogonal axes: the engine exposes
   capability, instructions sequence it for a use case. Specifics,
   sequencing, and use-case logic live in instructions written against
   the engine's exposed surface. If an instruction needs to reach past
   the engine, or asks for something the engine can't do, that's a
   coupling violation — fix the engine, don't paper over it. A derived
   view/projection is an instruction too: it reads and presents engine
   output but never authors back into the canonical store. A view that
   needs to write is the same coupling violation — fix the surface, not
   the view.

### Structural orthogonality

6. **Modules do one job.** Each component owns a single, well-named
   responsibility. If something feels like "two things glued together,"
   split it. One axis per module. The test for "two things glued
   together": do they change together (same axis — keep) or vary
   independently (different axes — split)? Independent variation is the
   split signal.

7. **Clean boundaries, owned state.** Components own their own state and
   controls. No reaching into siblings. The boundary is the contract;
   one fact lives in one place. When a fact must appear in a second place
   (a view, board, index, cache), it's a *derived projection* of the one
   canonical place — never a re-authored copy. Derivation preserves
   single-source; duplication breaks it.

8. **Architectural consistency.** Once a category, pattern, or convention
   exists, new items that fit it go there. Scattered parallels are
   duplicate representations of one concept — the same idea on multiple
   axes when it belongs on one. If the existing pattern is wrong, name
   it and propose replacing it before adding to it. Before extending a
   pattern, verify what it actually is *at source* — not from memory —
   and that you're extending the canonical instance, not a stale copy
   (links to #15).

## Scope and rigor

9. **Honest bounds over universal claims.** A claim like "this covers
   everything in domain X" must come with a definition of X and a
   constructive argument for the coverage. "Probably covers most cases"
   is not a finished thought. And when a claim *can't* yet be supported,
   state the gap and name precisely what would close it — an unprovable
   claim becomes a *named gap with a closing condition*, not a hand-wave
   and not a fake pass. (#9 — in-scope rigor — and #10 — naming what's
   out — are the two halves of honest scope.)

10. **Explicit exclusions over vague coverage.** What's NOT in scope
    should be named and justified. "We don't do X because Y" is a
    finished design decision; "we cover everything" is a hand-wave. An
    exclusion that *might* return should name its re-entry condition —
    "out now because Y; revisit when Z" — turning a static exclusion into
    a *tracked deferral* rather than a permanent no. (The negative half of
    honest scope; #9 is the positive half.)

11. **Scope decisions are durable.** Once captured in a decision file,
    a scope decision stands until explicitly superseded by a new dated
    decision. Implementation work cannot quietly expand scope. A change to
    durable scope comes as a new dated decision that **names its
    relationship to the prior** — *supersedes / amends / extends* —
    explicitly, never a silent edit and never left ambiguous.

12. **Surface conflicts, never resolve silently.** When two prior
    decisions disagree, or a new request contradicts an old decision,
    name the conflict and force an explicit choice. Silent resolution
    is how projects drift. The chosen resolution is then recorded durably
    (which way was chosen, and why), so the conflict stays resolved and
    doesn't resurface. Surfacing forces the choice; capturing keeps it.

## Execution

13. **Done means demonstrable, not reported.** If I can't point to it in
    a file or see the behavior, it doesn't exist yet. Roll-ups,
    milestone reports, and research summaries are inputs to verify —
    not evidence. This covers both *outcome* claims ("this was done")
    and *factual* claims ("X is defined at file:line, the value is N"):
    any concrete cite echoed without checking the source is hearsay.
    Where the demonstrable thing can be pointed at mechanically,
    *mechanize the check*: a done-claim carries a machine-resolvable
    evidence pointer (a passing test, an existing artifact, a Binding
    decision), and the verification itself becomes a *test*, not only a
    human reading a diff — "demonstrable" graduates from "a human *can*
    verify" to "the system *enforces*." Its document-review case —
    verifying cited claims in a doc before acting on it — is the corollary
    at #15. Verification protocol in `CLAUDE-OPERATING-MANUAL.md`
    operationalizes this rule.

14. **State lives in files, not conversations.** The chat is volatile;
    the repo is durable. Design rules, decisions, and milestones get
    written down — that's how they survive across sessions and automated
    runs. Durable state includes the *links between files* (provenance
    pointers), not just their contents — kept bidirectional so a fact's
    lineage is traversable from either end. The chain is state too.

15. **Verify cites — the document-review corollary of #13.** When
    reviewing a document with cited technical claims (file:line, "zero
    readers of X", "function does Y"), verify the load-bearing claims at
    source *before* pressure-testing the recommendation. Treat the doc's
    claims as claims-to-verify, not facts-to-paraphrase. Applies
    recursively to subagent reports. Verify the *verification* itself,
    too: confirm your check actually exercised the case it claims to — a
    check that "passes" without running the path it was meant to test is
    false confidence, worse than no check. When verification surfaces a
    finding the doc missed, name it — don't fold it silently into the
    next edit. Skip when the doc is descriptive (changelog, postmortem
    narrative) rather than recommendation-bearing.

16. **Lead architectural choices with capability data.** Before asking
    me to pick "unify vs keep both" / "refactor vs accept" / "implement
    vs delete metadata" — read both implementations, list each side's
    capabilities explicitly, name divergences as bug vs intentional,
    identify side-effects blocking direct merge, sketch the zero-loss
    migration path with effort estimate, and name residual losses
    honestly. Only then pose the question with the unification path
    concretely described. This applies to *greenfield* design forks too,
    not only existing-implementation comparisons: when neither option is
    built yet, the capability data is each direction's *projected*
    capabilities + trade-offs, laid out before the choice. The analysis
    is your job; my job is the architectural decision.

17. **Repeated failure indicts the model, not the attempt — but
    distinguish looping from iterating.** When the *same approach* fails
    the *same way* with *no new information*, treat the repeat as evidence
    of an unaccounted-for assumption, not a prompt to retry harder: stop,
    enumerate what both attempts silently took for granted, and hunt the
    hidden variable they shared. The trigger is *absence of information
    gain, not a failure count* — a path that fails *differently* each
    time, or narrows the problem with each miss, is iterating honestly;
    let it run. And the response is to *escalate the search to the frame,
    not abandon the path* — finding the missing variable often *rescues*
    it; dropping the path is only one possible outcome, and on a
    direction I chose it's a checkpoint to raise (Interaction-Style #4),
    not a unilateral give-up. (The self-directed complement of
    Interaction-Style #7: there *I* reframe a wrong frame; here *you* must,
    because two like failures are the signal your own frame is wrong.
    Distinct from #15 — that's false confidence in one check; this is
    false confidence across attempts.)

---

# Additional context

The sections below are injected by the caller via `--include` (typically a pairings bundle). Use this as domain context when tailoring interview questions and the populated scope.

<!-- ───── pairing: display-manager ───── -->
<!-- GENERATED-BY: pairings/bundle.sh -->

# Pairing: Display Manager (Wayland login)

- **Pairs with:** The login-manager / display-manager layer on Linux — `greetd` (+ `ReGreet`/`gtkgreet`/`tuigreet`), and the same reasoning applies to SDDM, GDM, `plasma-login-manager`, and `ly`. Applies to any project that *configures, themes, or switches* the program that owns the pre-session screen: the greeter, the `greeter`/`plasmalogin`/`sddm` system user, the VT/seat it claims, and the handoff into the chosen session.
- **Sources:** greetd & ReGreet documentation (man `greetd`, `greetd-regreet`, the greetd IPC protocol on git.sr.ht/~kennylevinsen/greetd); Arch Wiki (Display manager, greetd, SDDM, PAM); systemd `logind`/`pam_systemd`/seat docs; freedesktop Desktop Entry spec (`/usr/share/wayland-sessions`); opinion.
- **Date:** 2026-06-25
- **Touches principles:** #1, #2, #4, #5, #6, #7, #9, #13, #14

A display manager is the one program that runs **before there is a user session** and decides which session begins. That single fact dominates the domain: the greeter executes as an unprivileged *system* user (`greeter`, `sddm`, `plasmalogin`) with no access to any human's `$HOME`, no running session bus of its own to inherit theme from, and sole ownership of a VT and a seat. It is also the component with the **worst failure mode in the stack** — a broken greeter config doesn't degrade gracefully, it locks you out of the GUI entirely, and there is no windowed preview to test against. The principles below specialize for those two facts: the greeter is a sandbox that must carry its own assets, and the only real test is the switch itself, so reversibility is a design requirement, not a nicety.

## Per-principle commentary

### #1 — Vision down to detail

The login manager is **one stage in a chain** — bootloader → initramfs/plymouth → *greeter* → session → (lock) — and it reads as correct only when its handoff to the next stage is designed first, not its looks. Decide the **session-handoff contract** before the wallpaper: how does the greeter tell the DM which session to start (greetd: the greeter speaks the IPC protocol over `$GREETD_SOCK` and the DM starts the session *after the greeter process exits*), and what tears the greeter down so that handoff fires? The visible greeter (form, wallpaper, cursor) is detail under that contract — get the contract wrong and the prettiest greeter still drops you back to a respawn loop.

- Name the **session source**: greeters auto-list `/usr/share/wayland-sessions` (and `xsessions`). The dropdown is only as correct as those `.desktop` files; a session that won't start from its `Exec=` won't start from the greeter either.

### #2 — Upfront anticipation over reactive patching

**The test is the switch, and the switch can lock you out** — so the recovery path is part of the change, written *before* you flip the unit. Enumerate the failure up front: the greeter crashes, the compositor won't start, the session `Exec=` is wrong, the new DM and the old one both want VT1. Every one of those lands you at a black screen, and the only way back is a TTY (`Ctrl+Alt+F3`) or a live USB. So:

- Keep the **previous DM installed as a fallback** through the migration; don't uninstall the thing that still works until the new one is proven.
- The DM switch is `systemctl disable old.service && systemctl enable new.service` — make it **reversible from a TTY in two commands**, and write those revert commands into the setup script's output. A migration whose rollback you have to reconstruct from memory at a black screen is an unanticipated migration.
- Anticipate the **respawn loop**: greetd restarts the greeter whenever it exits. A greeter that exits immediately on an error (can't find its theme, can't reach the socket) becomes a silent flicker loop with no error on screen. Guard the greeter's launch and send its stderr somewhere readable (the journal), so a failed greeter is diagnosable instead of just blinking.

### #4 — Engines handle every possibility they could deal with

The greeter's environment is **not a user session**, and a config that only works *as if it were* is half-covered. The case-space the greeter must survive:

- **No user `$HOME`.** The greeter runs as a system user that cannot read `~/.local/share/icons`, `~/.config`, `~/.themes`, or `~/.fonts`. Every asset the greeter names — cursor theme, GTK theme, icon set, font, wallpaper, a QML/shader file — must be installed **system-wide** (`/usr/share/icons`, `/usr/share/themes`, `/usr/share/fonts`, `/usr/share/<app>`) and be world-readable, or it silently falls back to a default. A greeter config that references a theme living only in the human's home is a guaranteed fallback-to-ugly. *(This is the single most common greeter bug: the form looks themed in your head and renders stock on the screen.)*
- **No running session bus, pre-session.** The greeter can't `plasma-apply-*` or read another session's live state; it can't day/night auto-switch on a schedule it isn't running long enough to see. Pin one variant deliberately.
- **The seat/VT/GPU access** the greeter needs is a real input: the greeter user needs `video`/`input` (and DRM/seat access via `logind`, or the `seat` group only if you actually run `seatd`). On a `logind` machine there is no `seat` group — adding it is a no-op, and hiding that no-op behind `|| true` disguises a missing capability as a completed step.
- **Zero or many monitors.** The greeter draws before your monitor layout is known; it must degrade to "whatever is connected, preferred mode."

### #5 — Instructions sequence engine capabilities; they don't extend them

The **tracked config files are the engine; the setup script only places them and flips the unit.** `config.toml`, `regreet.toml`, the greeter compositor's `.conf`, the `.desktop` session files — those *are* the login behavior, declaratively. `setup-*.sh` should only `install` them to `/etc`, install the referenced assets system-wide, set group membership, and (reversibly) enable the service. A setup script that bakes a greeter setting inline — a value written with `kwriteconfig`/`sed` that exists nowhere as a tracked file — has smuggled engine into instructions; push the fact into the tracked config and let the script only deploy it.

### #6 — Modules do one job

**Exactly one display manager owns the machine.** Two enabled DM services racing for the same VT is the classic "it logs in then drops back to a login screen" bug. One job, one owner: the active DM is the single `*.service` that is `enable`d. If a second DM is kept *installed* as a fallback, it must be *disabled* — installed-but-disabled is fine, enabled-alongside is the bug.

- The greeter compositor (if the greeter runs one, e.g. a minimal Hyprland/Cage) does one job too: show the form, then exit so the handoff fires. No bar, no keybinds, no user session creep into the greeter.

### #7 — Clean boundaries, owned state

**"Which DM owns this machine" is one fact with exactly one owner** — the enabled systemd unit — and every other surface must defer to it, never assert a competing answer. A migration that switches the active DM but leaves another script's comments/config asserting "this machine's display manager is X" creates two writers of one fact who disagree. The runtime owner is the enabled unit; stale config that still themes or describes the *old* DM must be re-labeled as the fallback (and cross-referenced to the new one), not left claiming primacy. A reader who opens only the stale script must not come away with the wrong owner.

- Greeter config lives in `/etc` (`/etc/greetd/`, `/etc/sddm.conf.d/`), owned by root, deployed from the repo. The greeter *user's* home (`/var/lib/<greeter>`) holds only what must be in a home (its wallpaper); don't split one fact across both.

### #9 — Honest bounds over universal claims

Name what the greeter **cannot** do, in the config and the docs:

- **No live preview.** "It should theme correctly" is not demonstrable from the repo; say so. The greeter's look is only confirmable by the switch.
- **Pre-session limits:** can't read user dotfiles, can't day/night-switch, can't run a user systemd unit. State the pinned choices these force.
- A "the greeter matches the desktop" claim is true only for assets installed system-wide *and* readable by the greeter user — bound the claim to those, don't imply the whole desktop theme follows into the greeter for free.

### #13 — Done means demonstrable, not reported

Greeter "done" is **the switch performed and a real login completed**, plus the revert path *exercised* — not "the config looks right." The demonstrable artifacts: `systemctl status <dm>` shows it active on the VT; a login through it actually starts the chosen session; and a TTY revert returns you to the fallback. Because the only test is destructive (you're changing your own login), the honest report states which of those three you actually observed versus derived from the config.

### #14 — State lives in files, not conversations

Every byte the greeter reads is a tracked file deployed to a known path: `/etc/greetd/config.toml`, `/etc/greetd/regreet.toml`, the greeter compositor conf, the session `.desktop`. Nothing the greeter depends on should exist only as a manual `kwriteconfig` you ran once — if it's not in the repo and placed by the apply script, the next machine (or the next reinstall) won't have it.

## Addenda

### The greetd handoff, concretely

greetd runs `default_session.command` as `default_session.user` (the greeter). The greeter program speaks the greetd IPC protocol over `$GREETD_SOCK` to submit credentials and the chosen session command. greetd starts that session **only after the greeter process tree exits** — so whatever launches the greeter must also *quit the compositor/greeter* on the greeter's exit (e.g. `regreet; hyprctl dispatch exit`). If the greeter exits without ever submitting a session (crash, cancel), greetd respawns it: that's the intended loop, and also the failure loop — the difference is entirely whether the greeter could start. Guard accordingly.

### Migration checklist (any DM → any DM)

1. New DM's config + all referenced assets installed **system-wide** and greeter-user-readable.
2. Greeter user has `video`/`input` and seat/DRM access (via `logind`, or the `seat` group iff `seatd`).
3. Old DM **disabled but kept installed** as the fallback; its config re-labeled as fallback, not primary.
4. Switch is two TTY commands, both printed by the setup script, both reversible.
5. Reboot, log in, and exercise the revert at least once before deleting the old DM.


<!-- ───── pairing: arch-linux ───── -->
<!-- GENERATED-BY: pairings/bundle.sh -->

# Pairing: Arch Linux

- **Pairs with:** Arch Linux and Arch-derived rolling distributions (developed on CachyOS; nothing here is CachyOS-specific). Applies to any project that *configures a machine* — dotfiles repos, system-provisioning scripts, declarative `/etc` + `~` management — on a pacman/ALPM base.
- **Sources:** Arch Wiki (General recommendations, System maintenance, Pacman, mkinitcpio, systemd, Secure Boot); Arch packaging guidelines & `PKGBUILD`/ALPM docs; `pacman`/`pacman.conf` man pages; systemd `systemd.unit`/`systemd.path`/`tmpfiles.d` man pages; Filesystem Hierarchy Standard; XDG Base Directory spec; opinion.
- **Date:** 2026-06-24
- **Touches principles:** #2, #5, #6, #7, #8, #9, #13, #14

Arch gives you a bare, rolling, *unopinionated* base: no distro-blessed config layer, no `dconf`-style central store, no "the distro will reconcile it for you." Whatever shape your machine has, **you** imposed it, and a rolling release means the ground moves under that shape continuously. The principles below specialize for the two facts that dominate Arch system-config: there is no authority but yours (so reproducibility is a build artifact, not a given), and the package manager owns a large slice of the filesystem you are tempted to edit (so boundaries are not negotiable — pacman will win).

## Per-principle commentary

### #2 — Upfront anticipation over reactive patching

Rolling release is the anticipation forcing-function. A config that "works today" is a config that works against *this week's* package versions; the relevant question is always *what happens on the next `pacman -Syu`*.

- Enumerate the **breakage surface before** you depend on something: an AUR package that may not rebuild, a config key a Qt/KDE/systemd minor bump may rename, a kernel module that may move drivers (`i915` → `xe`). Name the fallback in the same breath.
- **`.pacnew`/`.pacsave` are anticipated events, not surprises.** Any file you ship into `/etc` that pacman also ships will diverge on upgrade. Decide up front whether you own the file (drop-in under `*.d/`, so pacman never touches it) or merge it (and own the merge cadence).
- Prefer **drop-ins over edits**: `/etc/systemd/*.conf.d/`, `/etc/pacman.d/`, `/etc/sysctl.d/`, `/etc/modprobe.d/`, `tmpfiles.d`. A drop-in is upfront anticipation made structural — it survives the upstream file changing underneath it.

### #5 — Instructions sequence engine capabilities; they don't extend them

The cleanest Arch-config split: **the tracked config files are the engine; the apply script is the instructions.** The `.conf`, the unit file, the colorscheme, the keymap — those *are* the capability, declaratively. `apply-*.sh` only *sequences* them onto the live machine (install to the right path, set the right mode, enable the right unit, reload the right daemon).

- An apply script that contains policy the repo can't reproduce without it (a value computed inline, a setting written with `kwriteconfig` that exists nowhere as a file) has smuggled engine into instructions. Push the fact into a tracked file; let the script only place it.
- `pacman -S --needed` is the canonical idempotent install verb; `systemctl enable --now` is the canonical idempotent activation verb. Lean on the package manager's own idempotency instead of reimplementing it.

### #6 — Modules do one job

Split apply logic by **privilege and ownership axis**, because that axis is real and unforgiving on Arch:

- **System vs user.** Anything touching `/etc`, `/usr`, `/boot`, or a system unit needs root and changes the machine for every user → one module (`apply-system.sh`). Anything under `$HOME`/`$XDG_CONFIG_HOME` is per-user and must *never* run as root → a separate module (`apply-user.sh`). Gluing them means one of the two halves runs with the wrong privilege.
- One concern per drop-in file: a `sysctl.d` snippet does sysctl, a `modprobe.d` snippet does module options. Don't co-locate "all my tweaks" in one mega-file — the FHS/`*.d` convention *is* the one-job boundary.

### #7 — Clean boundaries, owned state

**Pacman owns `/usr`. You own `/etc` (drop-ins) and `$HOME`.** This is the hardest boundary on the system and the one most violated.

- Never write into a path a package owns (`pacman -Qo <path>` tells you who owns it). A file you drop into `/usr/share/...` is destroyed on the next upgrade of its package, silently. If upstream ships there, shadow it from a path *you* own instead (user-level `~/.local/share/...`, or a system drop-in directory).
- **One fact, one place.** A value that appears in both a tracked file and an apply-script string will drift. The colorscheme's palette, the resolution, the recipient key — each lives in exactly one tracked file, and everything else *references* it.
- Secrets are a boundary too: plaintext never crosses into the repo. Encrypt at rest (sops+age) and let `.gitignore` make the plaintext path structurally uncommittable.

### #8 — Architectural consistency

Follow the distro's conventions; they are load-bearing, not decoration.

- Units go where systemd looks (`/etc/systemd/system`, `~/.config/systemd/user`); enablement is `systemctl enable`, not a hand-rolled symlink. A `*.path` unit is the idiomatic file-watch trigger — reach for it before a polling loop.
- New machine bring-up should be reproducible with the **package manager + your apply scripts**, no manual GUI steps. If a step can only be done by clicking in System Settings, it isn't tracked — find the config key it writes and track *that* (Principle 14).
- Match the existing repo's idioms (`install -Dm644`, guarded `--needed`, the apply-script echo style) before introducing a new one; propose the replacement, don't diverge silently.

### #9 — Honest bounds over universal claims

Arch's heterogeneity makes universal claims false by default.

- "Works on Arch" is rarely true; "works on this kernel + this Mesa + this Plasma, falls back cleanly otherwise" is. State the version floor you actually tested against.
- AUR is not a guarantee. If the core look/function depends on an AUR package, say so and provide the no-AUR path — or declare AUR an explicit, dated dependency (Principle 10).
- Distinguish *installed* from *applied* from *active*: a unit can be enabled but masked, a config written but shadowed by a higher-precedence drop-in. Don't report "done" from the install step.

### #13 — Done means demonstrable, not reported

On Arch, "done" is observable on the live machine, not asserted by the script that ran.

- Point to the artifact: `pacman -Qi <pkg>` for installed, `systemctl is-enabled --user <unit>` for enabled, the file on disk at its deployed path, the daemon reload that took. A clean apply-script exit is an input, not evidence.
- Idempotency is part of the demonstration: re-running the apply script should be a no-op (or converge), and you should *show* that it does, because a script that only works on a clean machine is a script that will betray the next upgrade.

### #14 — State lives in files, not conversations

The repo is the machine's source of truth; the running system is a derived, disposable projection.

- Anything that matters survives a reinstall *because it's in the repo*, not because you remember the click-path. The test is "wipe `$HOME`/reinstall, run the scripts, is the machine back?" — if a setting doesn't survive that, it isn't tracked yet.
- Keep a restore point before first apply, and make the apply idempotent so the repo — not a backup tarball — is the thing you trust to rebuild from.

## Addenda

### Secure Boot / boot-chain fragility
Where the machine uses signed boot (`sbctl`, enrolled keys) or a config-enrolling bootloader (Limine with `ENABLE_ENROLL_LIMINE_CONFIG`), edits to the boot path are **not** ordinary config edits: a wrong change can make the machine unbootable or require re-enroll + re-sign. Treat `/boot` and bootloader config as a separate, hand-applied tier — never folded into the routine system-apply that runs unattended — and always document the TTY/recovery revert path alongside the change.


<!-- ───── pairing: offensive-security ───── -->
<!-- GENERATED-BY: pairings/bundle.sh -->

# Pairing: Offensive Security

- **Pairs with:** Authorized penetration testing and offensive security engagements (methodology) — rules of engagement, scoping, threat modeling, evidence handling, proof-of-concept discipline, severity rating, and reporting. Assumes a legitimate, authorized context (signed engagement, CTF, internal assessment of a system you own or are permitted to test).
- **Sources:** Penetration Testing Execution Standard (PTES, pentest-standard.org); NIST SP 800-115, *Technical Guide to Information Security Testing and Assessment*; OWASP Web Security Testing Guide (WSTG v4.2, owasp.org/www-project-web-security-testing-guide); OSSTMM 3 (Open Source Security Testing Methodology Manual, isecom.org); MITRE ATT&CK (attack.mitre.org); CVSS v4.0 (first.org/cvss); opinion.
- **Date:** 2026-06-07
- **Touches principles:** #1, #2, #9, #10, #11, #13, #14

Offensive testing is design discipline pointed backwards. You reason about a system's trust boundaries the way its builder should have — except your output is not the system, it's a verified account of where its claims break. Three things make this a methodology rather than ad-hoc poking: the rules of engagement are the scope contract, the proof-of-concept is the demonstrability gate, and the report is the durable artifact. The principles below are about keeping those honest under the pressure to find more, find it faster, and overstate what was found.

## Per-principle commentary

### #1 — Vision down to detail
Threat-model first. The "vision" is the attacker's objective and the trust boundaries that stand between them and it — *what would someone want here, and what is supposed to stop them?* That picture tells you which axes exist: authentication, authorization, data integrity, secrets, availability. Detail-up testing — opening a proxy and fuzzing the first parameter you see — finds shallow bugs and misses structural ones. Start from the boundary map; let the specific test cases fall out of it.

### #2 — Upfront anticipation over reactive patching
Enumerate the full attack surface before testing it: every endpoint, method, parameter, role, trust boundary, and data flow. Reactive testing couples your coverage to whatever you happened to click — the surface you never mapped is the surface you never tested, and your report will silently claim it was clean. The map is upfront work; the probing is the easy part.

### #9 — Honest bounds over universal claims
"We tested everything" is a hand-wave without a defined scope and a coverage argument. A finished report states *which classes* were tested (authn, authz, injection, config, …), what "tested" meant for each (manual, automated, time-boxed), and what was sampled versus exhausted. "No SQL injection found in the 21 endpoints enumerated under WSTG-INPV" is honest; "the app is secure" is not a finished thought.

### #10 — Explicit exclusions over vague coverage
Everything out of scope gets named and justified, not silently skipped. "We did not test the third-party payment provider — out of ROE" and "DoS/load testing excluded by client request" are finished decisions. An untested area that's merely absent from the report reads to the reader as *clean*, which is the most expensive kind of false assurance.

### #11 — Scope decisions are durable
The rules of engagement are the durable scope contract — authorization, targets, windows, and stop conditions stand for the whole engagement until explicitly amended. Mid-engagement scope changes ("can you also look at staging?") get written, dated, and re-authorized, never assumed from a verbal nod. Scope creep in offensive work isn't just untidy; testing an out-of-scope asset can be unlawful.

### #13 — Done means demonstrable, not reported
This is the spine of the whole discipline. A finding is not real until you have a working, repeatable proof-of-concept that an independent reader can reproduce. Scanner output is a *lead*, not a finding. "Theoretically vulnerable," "this pattern is usually exploitable," and "the header looks misconfigured" are hypotheses awaiting a PoC. If you can't point to the request, the response, and the impact, it doesn't exist yet.

### #14 — State lives in files
Evidence is durable or it never happened. Capture the exact requests and responses, the repro steps, the screenshots, and the environment state alongside each finding — "I saw it once in the proxy" does not survive the engagement. The report and its evidence store are the artifact; the live session is volatile. Chain-of-custody for any sensitive data accessed lives in files too.

## Addenda

### Rules of Engagement — the pre-flight checklist
Before the first packet:
- **Written authorization** from someone empowered to grant it, naming the tester and the date range.
- **Scope** — exact in-scope assets (hosts, domains, URLs, accounts, cloud subscriptions) and explicit out-of-scope assets.
- **Test windows** — when testing is permitted; whether production is in scope or only staging.
- **Technique limits** — is DoS allowed? Social engineering? Physical? Data exfiltration, or proof-of-access only?
- **Sensitive-data handling** — what to do if you access PII, credentials, or regulated data; how it's stored and destroyed after.
- **Emergency contacts and stop conditions** — who to call if something breaks, and what triggers an immediate halt.
- **Deconfliction** — how the blue team distinguishes your traffic from a real incident.

### Severity discipline
Rate by likelihood × impact in the system's actual business context, not by raw CVSS base score alone. A CVSS-9 in an asset with no sensitive data and no reachability may matter less than a CVSS-6 on the billing path. State the vector, the prerequisites (auth required? specific role? user interaction?), and the realistic impact. Inflating severity to make a report look productive corrodes trust in every finding you file.

### Finding quality bar
Every finding carries: a one-line title naming the flaw class and location; reproduction steps an independent reader can follow; evidence (request/response, screenshot); impact stated in business terms; concrete remediation; and a reference (CWE/WSTG/ATT&CK). A finding missing the repro or the impact is half a finding.

### The scanner-output trap
Automated tools (DAST, SAST, dependency scanners, cloud config scanners) produce *candidate* findings at volume, with false-positive rates that make raw output worthless as a deliverable. Their job is to widen your search, not to populate your report. Every candidate gets manually validated to a PoC before it becomes a finding — and every finding the scanner *missed* (business logic, chained flaws, authz gaps) is why a human is doing this at all.


<!-- ───── pairing: ui-ux ───── -->
<!-- GENERATED-BY: pairings/bundle.sh -->

# Pairing: UI/UX

- **Pairs with:** UI design as architectural discipline — vision and journey to screens, single-purpose screens, components as bounded units, design systems, visual hierarchy and typography, accessibility basics, and usability testing as the verification surface. Covers the structural and visual layer of UI work. Out of scope: the cognitive layer (mental-model formation, signifiers, recognition vs. recall, cognitive load) and the interaction-behavior layer (microinteractions, feedback timing, error recovery, gesture and modality, latency budgets).
- **Sources:** Steve Krug, *Don't Make Me Think, Revisited* (3rd ed., New Riders, 2014); Jakob Nielsen's *10 Usability Heuristics* (NN/g, ongoing); Edward Tufte, *The Visual Display of Quantitative Information* (2nd ed., Graphics Press, 2001) and *Envisioning Information* (1990); Adam Wathan & Steve Schoger, *Refactoring UI* (2018); W3C WCAG 2.2 (2023); Apple Human Interface Guidelines (ongoing); Material Design 3 (Google, ongoing); opinion.
- **Date:** 2026-05-28
- **Touches principles:** #1, #2, #6, #7, #8, #9, #10, #13

UI work has three layers that pull on different design muscles: the **architectural and visual surface** (how the product is structured, how screens compose, what the design system says), the **cognitive layer** (how users build a working model of the system in their heads), and the **interaction-behavior layer** (what happens over time when the user acts). This pairing covers the first. It is about UI as architecture — vision, structure, component boundaries, design systems, visual hierarchy, accessibility surface. Get the architecture right and the other two layers have somewhere coherent to live.

## Per-principle commentary

### #1 — Vision down to detail
Start with the user's job-to-be-done, then the journey, then the screens, then the components. Designing screen-by-screen produces a collection of screens, not a product. The vision answers "what does the user accomplish?" — that's the axis every screen sits on. Without it, every new screen is its own ad-hoc decision and the product accretes rather than composes.

### #2 — Upfront anticipation over reactive patching
Enumerate every state of every screen before building: empty (first-run, no data, search returned nothing), loading, partial-load, success, error (with sub-categories: network, permission, validation, server, rate-limit), edit, read-only, disabled. Most ugly UIs are 80% complete on the success path and underspecified on the others. Anticipate the range; design each state intentionally. Discovering a needed state in production usually means a screen has to be redesigned, not just patched.

### #6 — Modules do one job
Each screen has one primary purpose. Multi-purpose dashboards confuse users about what they should do next. If a screen needs three calls to action, it's either three screens or one screen with explicit hierarchy that makes the primary action obvious and demotes the rest. Each component does one job; reuse the same component for the same job across the product. A single component that's secretly five, selected by a mode or `variant` parameter, is the multi-job pathology — split it into named components, or design the variant surface as a narrow, deliberate parameter set.

### #7 — Clean boundaries, owned state
Each component owns its local state (open/closed, hover, expanded, focus). Inputs flow in from the parent; events flow back out to it. No reaching into sibling state. The component boundary is the contract — violating it is how a small change in one component breaks three others. Shared state lives in an explicit owner with a named slice; implicit shared state (two components reading the same source and silently drifting) is the canonical decay path.

### #8 — Architectural consistency
A design system is Principle 8 applied to UI. Once typography, spacing, color, and component conventions exist, new screens use them — not local alternatives. A modal in one place and a sheet in another for the same purpose is the visual equivalent of inconsistent error handling in code. If a pattern is wrong, fix it in the system; don't scatter alternatives. The design system is itself a pairing artefact (per Principle 11 — it is a dated, durable decision about the visual surface) and changes to it should be ratified, not slipped in.

The categories where consistency pays the most:

- **Typography scale.** A finite set of type sizes and weights, used consistently. Inline overrides ("just this one place needs `13.5px`") are the visible residue of an under-designed scale.
- **Spacing scale.** A finite set of spacing values driving every margin, padding, gap. Magic numbers in components are scattered parallels of the same fact.
- **Color tokens.** Role-named semantic tokens (`color.bg`, `color.text`, `color.accent`) at the component layer; value-named primitives (`blue-500`) at the palette layer. Components consume semantic tokens; primitives stay below the role layer. The token system is itself a versioned artefact — changes to it propagate everywhere and deserve dated decisions.
- **Component naming and shape.** `Button`, `Card`, `Modal`, `Sheet` mean specific things in your system. Don't reuse a name for a different shape; don't introduce two names for the same shape.

### #9 — Honest bounds over universal claims
Visual hierarchy is a claim about priority. What looks important must *be* important; what looks secondary must be. A heading style applied to non-heading text mis-claims importance and confuses the eye's first read. A primary-button style on a destructive action mis-claims safety. Treat the visual scale (size, weight, color, contrast, position) as a budget — the most prominent slot is the most expensive, and spending it on a low-priority element costs the high-priority element its place.

The same applies to claims about the product itself: "modern, responsive, accessible" without a stated baseline (which devices, which assistive tech, which contrast level) is marketing, not specification. Pick the floor; document it; design against it.

### #10 — Explicit exclusions over vague coverage
"We don't support keyboard navigation" should be a stated decision, not an accidental omission. Same for: offline mode, multi-language, screen readers, RTL layouts, dark mode, reduced-motion, forced-colors mode, print, e-ink readers. Each not-in-scope item belongs on a list with a reason. "We're inclusive" without an enumeration is a hand-wave; "we support WCAG 2.2 AA on Chrome/Firefox/Safari current-and-prior major versions, with keyboard navigation and screen-reader semantics; we do not support forced-colors mode or RTL in v1 — both planned for Q3" is a finished decision.

### #13 — Done means demonstrable
"It looks good" is not done. "A user new to the product completes the task without help in under N seconds on the first try" is. Usability test with five real users — that catches roughly 85% of issues per Nielsen's classic study (NN/g, 2000). Heuristic walkthroughs against the 10 heuristics catch most of the rest. Designs without a test plan are speculation; designs that have only been tested by their author are speculation by a biased observer.

## Addenda

### Visual hierarchy as a budget

The eye reads a screen in scan order — typically top-left for LTR languages, biased by size, weight, color contrast, and position. That order is a finite budget: only a few elements can be "first." Spending it deliberately is the design move; spending it accidentally (by giving every element similar weight) flattens the screen and forces the user to read everything to find anything.

Levers, ranked roughly by perceptual weight:

- **Size.** A 32px headline in a sea of 14px body is read first.
- **Contrast.** High-contrast against background outranks low-contrast at the same size.
- **Weight.** Bold draws the eye before regular.
- **Color saturation.** A saturated accent on a desaturated surface pulls focus.
- **Position.** Top-left (LTR) is read first; isolation pulls focus regardless of position.
- **Negative space.** An element surrounded by empty space is more prominent than the same element packed against neighbors.

These compose. A 14px bold high-contrast saturated word with whitespace around it can outweigh a 24px low-contrast regular headline. The design system's job is to make these levers a small, named set so combinations are deliberate, not accidental.

### Design system vs. style guide

A **style guide** documents visual conventions (typography, colors, spacings) as values. A **design system** adds the component primitives, the role-based tokens that consume the values, and the rules for composing them — and ships as code, not just as a design-tool document. A style guide constrains; a design system constrains *and provides the building blocks*. The handoff between design and engineering is where the difference matters: a design system has one source of truth (the code); a style guide drifts the moment a designer makes a one-off in the design tool and an engineer hardcodes a near-match in the implementation.

The discipline: every new visual decision lands in the design system before it lands in a feature. Features consume the system; they don't extend it locally.

### Accessibility surface (UI architecture concerns)

The interaction-behavior side of accessibility (focus management, ARIA live regions, gesture alternatives) belongs to interaction-behavior work. The architecture-side concerns:

- **Color contrast as token-level decision.** Audit role pairs (`--color-text` on `--color-bg`, etc.) rather than individual usages — one check per pair covers every place those tokens are used.
- **Type sizes at the lower bound.** Body text below 14–16px is fragile on dense displays and below WCAG large-text definitions. Pick a floor; respect it.
- **Touch target floors.** 44×44 pt on Apple HIG, 48×48 dp on Material — these are minimum interactive sizes for finger-driven UIs. Components in the design system should not undercut them; if a layout needs to, add extra hit area beyond the visible bounds.
- **Reduced-motion contract.** Components in the design system declare which motion they include; the platform's reduced-motion signal switches them to instantaneous variants. Decorative animation is a system-level decision, not a per-component one.
- **Forced-colors mode** (Windows high-contrast). The design system declares which surfaces opt out of system color override (logos, color pickers, deliberate brand surfaces); everything else honors it.

### Information architecture (where this pairing meets it)

Information architecture — taxonomy, navigation structure, content categorization, search vs. browse — is a deeper concern than this pairing covers, but it touches the architecture work: the navigation system, the page hierarchy, the URL structure, the breadcrumb scheme. If the product has more than ~20 screens, treat the navigation architecture as a first-class design artefact (a tree or graph, drawn out, reviewed before any screen is laid out). Without it, screens accumulate, navigation accretes, and the user has to learn the product's geography by trial.

### Usability testing as the verification surface

Five users is enough to surface the bulk of issues for one task on one design (Nielsen, NN/g 2000). The discipline:

- **Tasks, not interviews.** "Show me how you would do X" beats "What do you think of this?" — opinions are cheap; observed behavior is expensive and accurate.
- **Don't help.** Watch them struggle silently. Help only when fully stuck and only at the end.
- **Record what they do, not what they say.** "I would click here" is a guess; the click itself is data.
- **Test against the actual design fidelity.** Wireframe-stage tests catch wireframe-stage issues; high-fidelity tests catch visual-hierarchy issues. Match the test to the question.
- **Re-test after changes.** A redesign that fixed one issue often introduces another. Re-test before shipping.




---

# Scope Template

The populated scope file must follow this structure exactly.
Section headings and ordering are part of the contract. Fill
placeholders with the user's answers, not the template's
bracketed instructions.

# Project Scope: [Project Name]

## Overview
[2-3 sentences. What this project is, who it's for, what success looks like.]

## Pairings (optional)
[Domain specializations from `pairings/` that color the canonical principles 
in for a specific domain (language, framework, methodology, skill). Additive 
only — cannot contradict principles. List each by name as it appears in 
`pairings/`. Leave the section empty (or remove it) if none apply.]

- [pairing-name] — [one-line note on why this pairing is selected]
- [pairing-name] — [reason]

## Principles, in priority order
[These extend or override PERSONAL-PRINCIPLES.md for this specific project. 
If a personal principle doesn't fit this project, say so explicitly. If a 
project-specific principle is needed, list it.]

1. [Principle name]. [Brief statement.]
2. [Principle name]. [Brief statement.]
3. [...]

[When principles conflict, prioritize by number. Flag severe conflicts.]

## Hard constraints
[Things that must always be true in this project. Violations are bugs.]

- [Constraint]
- [Constraint]

## Capabilities currently in scope (optional but recommended)
[Authoritative inventory of what this project IS supposed to do. Anything 
not on this list is a candidate for removal. Skip this section for small 
or early-stage projects; add it once the project has enough surface area 
that scope drift becomes a risk.]

### [Category]
- [Capability]
- [Capability]

### Planned but not yet specified (preserve, do not extend)
- **[Capability].** [Why preserved without active development.]

## Out of scope
[Capabilities explicitly excluded from this version of the project. Adding 
anything from this list requires a check-in.]

- [Excluded capability] — [why excluded, and what would be needed to add it later]
- [Excluded capability] — [reason]

## Removal authority (optional, pairs with "Capabilities currently in scope")
[If the in-scope inventory exists, this section authorizes the agent to 
remove anything that doesn't map to it. Skip if no in-scope inventory.]

Anything in the codebase that does not map to a capability in 
"Capabilities currently in scope" above is a candidate for removal. 
Treat removal as the default action for such items. File a check-in 
only when removal is non-trivial, ambiguous, or load-bearing.

## Criticality rubric

What counts as Critical, Material, or Minor for THIS project. The operating 
manual uses this to decide whether to hard-stop or continue on parallel work 
when a check-in is filed.

**Critical** (hard-stop, do not touch related work):
- [Type of change]
- [Type of change]

**Material** (continue on parallel work, avoid downstream):
- [Type of change]
- [Type of change]

**Minor** (continue freely):
- [Type of change]
- [Type of change]

## Default check-in mode
[Usually: hybrid per the operating manual. Override only if this project needs 
something different, like always-hard-stop or always-continue.]

## Active milestone
[If the project uses `.agent/ROADMAP.md` (D-0050), the active milestone, its
per-task done-when, backlog, and shipped history live there — keep this a
one-line pointer and don't duplicate the DoD here. Otherwise fill in below.]

**Milestone:** [name/number — or "see ROADMAP `## Active`"]
**Definition of done:** [observable, verifiable criteria — or per-task in ROADMAP]
**Active blockers:** [if any]

## Project-specific glossary (optional)
[If the project has domain terms that need precise meaning, define them here. 
This is the canonical reference — when in doubt, terms mean what this glossary 
says they mean.]

- **[Term]**: [definition]
- **[Term]**: [definition]
---

# Interview

Conduct this interview one section at a time. For each section:

1. Ask the questions listed.
2. Wait for the user's answer. Ask one follow-up if the answer is
   ambiguous — do not fan out into branching what-ifs (one good
   question, not five hedging ones).
3. Summarize back what you heard in 1–2 sentences. Confirm before
   moving on.
4. Move to the next section.

Sections marked **(optional)** should be asked only if the user
signals they want them, or if the project's surface area justifies
them. Sections marked **(skip if covered by `--include`)** should be
skipped when included context already answers them.

Push back when answers are vague. Defaults are valid answers if the
user explicitly says "default" — record that and move on.

---

## 0. Prior context — read this first

Before asking anything, check for `.agent/REPORTS/project-brief.md`. If it
exists, it carries the vision, stack, surface, methodology, and constraints
captured during the vision phase (Phase 0) — read it in full. Use it to
*pre-fill and confirm* the sections below, not to re-elicit from a blank
slate: state back what the brief already establishes, ask the user to
confirm or correct, and spend new questions only on what the brief doesn't
cover (milestone, definition of done, out of scope, criticality rubric).
Re-asking from scratch what the brief already answers is the
information-loss this step exists to prevent (Principle 14). If no brief
exists, conduct the full interview below.

## 1. Overview

- In 2–3 sentences, what is this project? Who is it for, and what does
  success look like? *(If a project brief exists, confirm its Vision
  rather than re-asking.)*
- Is this a greenfield start, or an existing codebase being adopted
  into this system?

## 2. Pairings (skip if covered by `--include`)

If no pairings bundle was included, ask:

- Which `pairings/` (if any) apply to this project? Name them.
- For each: one line on why it's selected.

If a bundle was included, the user has already chosen — list the
pairing names you see in the additional context and confirm them as
the selection.

## 3. Principles, in priority order

The canonical principles in `personal/PERSONAL-PRINCIPLES.md` apply by
default. Ask:

- Are there any canonical principles that do **not** fit this project,
  and why? (Overrides become numbered project principles that
  explicitly de-prioritize a canonical one — surface the conflict per
  Principle 12, don't bury it.)
- Are there any project-specific principles to add? (E.g., "ship daily
  over polish" for an MVP, "no third-party deps" for embedded, "every
  change ships with a test" for TDD.)

If the user says "defaults are fine," record that as the priority
list — no overrides needed.

## 4. Hard constraints

Hard constraints are things that must always be true. Violations are
bugs, not preferences. Ask:

- What are the immovable constraints? (Performance ceilings, security
  requirements, regulatory limits, hardware budgets, dependency locks,
  language/runtime constraints, deployment targets.)
- For each: what happens if it's violated? If the answer is "nothing
  serious" — it's a preference, not a hard constraint. Recategorize.

## 5. Active milestone & definition of done

Per Principle 13 — done means demonstrable, not reported. Ask:

- What is the current milestone? Give it a short name or number.
- What is the **observable, verifiable** definition of done? (Which
  files exist with what content? What behavior is demonstrated? What
  can the user do after?)
- Any active blockers right now?

If the definition of done is vague ("ship the feature"), push back:
what specifically demonstrates it shipped?

## 6. Out of scope

Per Principles 9 and 10 — honest bounds, explicit exclusions. Ask:

- What capabilities are explicitly **not** part of this version's
  scope? Name 2–5.
- For each: why excluded, and what would be needed to add it later?

This is one of the most load-bearing sections. If the user struggles,
prompt with examples adjacent to their stated scope: "Would X be in
scope? Y? Z?"

## 7. Criticality rubric

This is how the agent decides when to hard-stop vs. continue. Ask:

- What kinds of changes are **Critical** (hard-stop, do not touch
  related work) for this project?
- What kinds are **Material** (continue parallel work, avoid
  downstream)?
- What kinds are **Minor** (continue freely)?

Defaults to suggest if the user is unsure: scope/architecture/data-shape
changes are usually Critical; refactors in well-bounded modules are
usually Material; cosmetic edits, comment fixes, and additive tests are
usually Minor.

## 8. Default check-in mode

Almost always "hybrid" per the operating manual. Ask once:

- Default to hybrid check-ins (mix per criticality), or override to
  always-hard-stop / always-continue?

If "default," set hybrid and move on.

## 9. In-scope capabilities (optional)

Skip for small or early-stage projects. Only ask if the project has
enough surface area that scope drift is a real risk. Ask:

- What capabilities does the project currently have? Group by category
  if useful.
- Are any "planned but not yet specified" — preserve, do not extend?

## 10. Removal authority (optional, pairs with §9)

Only if §9 was populated. Ask:

- Should anything not in the in-scope list be treated as a candidate
  for removal? (Default: yes, per the template's standing language.)

## 11. Project-specific glossary (optional)

Ask:

- Are there domain terms that need precise meaning in this project?
  List them.

If none, skip the section.

---

# Output format

After the interview, produce a single markdown code block containing
the populated scope file. Use the exact section headings from the
**Scope Template**. Fill placeholders with the user's answers, not the
template's bracketed instructions. Sections marked **(optional)** that
were skipped should be **omitted entirely** — do not leave empty
sections with placeholder text.

Hand the populated scope file to the user with these instructions:

1. Review it. Edit anything that drifted from what you said.
2. Save to `.agent/PROJECT-SCOPE.md` in the target project.
3. Run the publish step to produce the project's `CLAUDE.md`:
   ```bash
   cd <target-project>
   ~/Projects/dotagent/publish/publish.sh claude-md \
     --include <(~/Projects/dotagent/pairings/bundle.sh)
   ```

Do not write the file yourself. The human commits scope decisions
(Principle 11).
