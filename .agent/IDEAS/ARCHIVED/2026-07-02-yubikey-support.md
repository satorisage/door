# Idea: YubiKey (hardware-key) support for door

**Raised:** 2026-07-02 (owner request: "can we make door support a yubikey")
**Status:** ✅ **GRADUATED 2026-07-02.** All three forks ratified by owner:
**FIDO2/U2F · password+key 2FA · door stays policy-neutral** — the recommended
path across the board (authorizes no divergence; 2FA was already in-scope per
SCOPE:89 / D-0001, so no new DECISION was cut — the durable record is the threat
model). Shipped artifacts: `docs/yubikey.md` (setup + lockout-avoidance),
README §Two-factor, `pam-u2f` PKGBUILD optdepend,
`.agent/SECURITY/yubikey-2fa-threat-model.md`, ROADMAP `## Loose` entry. No TCB
change. Open follow-ups (uncommitted): greeter "waiting for touch" affordance
(Minor), passwordless+PIN (contained greeter multi-prompt add). Owner's remaining
step = the hardware enrollment + real-VT touch login (commands in the guide).
This file archived to `IDEAS/ARCHIVED/`.
**Criticality:** Critical (auth path) — but the load-bearing core was already built.

## Finding: the hard part already exists

door was scoped for 2FA from day one (SCOPE:89, D-0001), and the code matches.
The security-critical pieces need **zero change**:

- **Wire protocol** already models a full multi-round conversation:
  `AuthPrompt::{Question{text,secret}, Info, Error}` + `BeginAuth`/`AuthReply`
  loop — `protocol/src/lib.rs:79-153`.
- **Privileged worker** relays all four PAM message styles and does a real
  greeter round-trip *per prompt*, so password → "touch your key" → second
  factor already flows across the seam — `ControlConversation`,
  `doord/src/worker.rs:535-598`. (USB touch happens at the worker via
  libfido2/`/dev/hidraw` while it's still root at auth phase — greeter never
  touches the key. Privilege boundary intact.)
- **Daemon relay loop** is N-prompt (`doord/src/pam.rs` authenticate loop).
- **PAM config** is a static `include system-local-login`
  (`dist/pam.d/doord`), so a `pam_u2f.so` line drops straight in — no code
  invokes it; the worker proxies whatever rounds the stack issues.

## Finding: the one gap is greeter UI (unprivileged, not TCB)

`door-greeter/src/app.rs:367-376` only *auto-answers a single pre-typed
secret* (the password). Consequences:

- **Password + touch (FIDO2, `cue` mode) likely works end-to-end today** — the
  password is answered, the touch needs no typed reply, the "touch your key"
  Info renders as the status line. UX is a bare status line, not a real
  affordance.
- **Any *typed* second factor stalls** — FIDO2 PIN, an OTP field, or
  passwordless-initiation. The greeter has no interactive reply path for a
  *second* prompt, and `std::mem::take` already emptied the password field. Fix
  touches `Message::Prompt` (app.rs:367-376), `run_conversation`
  (app.rs:1211-1247), and the fixed two-field form in `view`. Contained,
  Minor-by-rubric (pre-auth UI, no trust-boundary change).

## Finding: two constraints bound the design

- **Yubico OTP → YubiCloud is OUT.** Needs network validation; violates the
  "No network, ever" hard constraint. Offline mechanisms only.
- **door mirrors distro auth policy by design** ("door owns the conversation,
  not the policy" — `dist/pam.d/doord`). So *where* the key requirement lives
  is a real architecture fork, not a detail.

## Open forks (owner decision)

1. **Mechanism.** FIDO2/U2F (`pam_u2f`, offline, phishing-resistant, touch) —
   recommended — vs offline HMAC challenge-response (`pam_yubico` chalresp) vs
   reframe (PIV/smartcard). Network-OTP excluded by constraint.
2. **Factor model.** Password + key (2FA, security-first default, closest to
   today's greeter) vs key-alone (passwordless; needs greeter multi-prompt fix
   for FIDO2 PIN).
3. **Policy home.** Distro stack, door stays neutral (GUI login == console
   login; door ships greeter touch-UX + docs + optdepend only) — recommended,
   consistent with the mirror-policy design — vs a door-owned `pam_u2f` line in
   `/etc/pam.d/doord` (GUI stricter than console; **diverges from mirror-policy,
   needs a DECISION to authorize**).

## Likely shape once ratified (recommended path: FIDO2 / 2FA / distro-neutral)

- **No TCB change.** Greeter touch-UX only: a proper "Touch your key" state +
  (if passwordless/PIN ever wanted) the multi-prompt reply path.
- **Threat-model doc** in `.agent/SECURITY/` — what the key defends (phishing,
  password-only shoulder-surf) and what it does not (evil-maid with key present;
  a stolen unlocked key if passwordless).
- **Packaging:** `pam-u2f` as an `optdepend` in PKGBUILD; docs for the mapping
  file (`authfile`/`u2f_keys`). Note: greeter user has no `$HOME`, but the
  mapping is the *authenticating* user's — either their `~/.config/Yubico/` or a
  system `authfile=`; the security-first pick is a root-owned system authfile.
- **Reversibility:** a config-only, keep-password-fallback path so a missing or
  broken key never locks the machine out (SCOPE reversibility constraint). The
  2FA `required` line must not be the only auth on a machine whose sole key can
  walk away — document the `nullok`/fallback posture explicitly.

## On graduation

When ratified: a `DECISION-NNNN` (esp. if fork 3 picks the door-owned line, which
*requires* a decision to authorize the mirror-policy divergence), a ROADMAP task
(new milestone or an M4/M5 sibling), pointer appended here, move to `IDEAS/ARCHIVED/`.
