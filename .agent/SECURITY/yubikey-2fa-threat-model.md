# Threat model — hardware-key second factor (FIDO2/U2F)

**Scope of this model:** adding a FIDO2/U2F hardware key (YubiKey and
equivalents) as a **second authentication factor** on top of the password, via
`pam_u2f` in the system login stack. It extends
[`auth-path-threat-model.md`](./auth-path-threat-model.md); everything there
(the IPC seam, the greeter-is-untrusted claim, the daemon-owns-PAM invariant)
still holds and is not repeated. This model covers only what the added factor
changes.

**Ratified shape (2026-07-02, owner-selected):**

- **Mechanism: FIDO2/U2F (`pam_u2f`).** Offline challenge-response. Yubico OTP
  against YubiCloud is **excluded** — it needs network validation and door's
  "No network, ever" is a hard constraint.
- **Factor model: password + key (2FA).** `auth required pam_u2f.so … cue`
  *after* the password module — both must pass. Not passwordless.
- **Policy home: the system PAM stack (door stays policy-neutral).** The admin
  adds the `pam_u2f` line to `/etc/pam.d/system-login` (logins) or
  `system-auth` (all PAM). door ships **no** change to its own PAM files and
  **no** TCB change — it already relays a multi-round PAM conversation faithfully
  (`doord/src/worker.rs` `ControlConversation`; `protocol` `AuthPrompt`). door's
  contribution is docs (`docs/yubikey.md`), a `pam-u2f` optdepend, greeter cue
  rendering (already present), and this model.

This choice authorizes **no divergence** from an existing decision: 2FA was
already in scope (`PROJECT-SCOPE.md` "multi-prompt / 2FA-capable flows";
DECISION-0001 "multi-prompt/2FA conversations"). This concretizes it.

---

## 1. Assets (delta over the auth-path model)

- **A5 — The FIDO2 credential exchange** (challenge, signature, key handle)
  between the daemon and the physical key. Must never be reachable, replayable,
  or forgeable by the greeter.
- **A6 — The enrollment mapping** (`/etc/u2f_mappings`): which public keys
  authenticate which users. Not secret, but its **integrity** is an asset — an
  attacker who can append their own key handle grants themselves the second
  factor.

## 2. Where the factor runs — the boundary holds

`pam_u2f` executes inside `context.authenticate()` in door's re-exec'd **session
worker**, which is still **root** at the authentication phase (privilege drop
happens later, for the session). So:

- The worker opens `/dev/hidraw*` and drives the key directly. **The greeter
  never touches the key**, never sees the challenge or signature — it only
  renders the one-way `cue` "touch" message relayed as `AuthPrompt::Info` and,
  on success, receives the same `AuthSuccess` verdict as any other login.
- No udev/group grant is needed for device access (root already has it) — a
  quiet benefit of the privilege-separated design: the USB-device access that a
  normal desktop 2FA setup grants to a user process stays inside the TCB here.

So A5 inherits TB1's protection unchanged: a compromised greeter (persona **P2**,
the load-bearing persona) cannot read, replay, or forge the FIDO2 exchange. It
can only *ask* the daemon to run a login, exactly as before — and now that login
also requires a physical touch the greeter cannot supply.

## 3. What the second factor defends against (new coverage)

- **Password compromise alone no longer logs in.** Shoulder-surfed, phished,
  keylogged, or leaked-hash-then-cracked passwords (personas P1 and beyond) are
  insufficient without the physical key present and touched. This is the whole
  point of the factor.
- **Remote/automated password guessing** against the login is defeated: no key,
  no session, regardless of password correctness.
- **Phishing resistance** is inherent to FIDO2's origin-bound challenge-response
  (relevant if door's stack is ever reused beyond local login; for local login
  the practical win is the "password alone is not enough" property).

## 4. What it does NOT defend against (honest bounds)

- **Evil-maid with the key present.** If the authenticator is left plugged in and
  the attacker knows the password, 2FA is satisfied. The key defends against
  *password-only* compromise, not against an attacker who has both factors.
- **A stolen key — only matters if you go passwordless.** In the ratified 2FA
  setup a stolen key is useless without the password. (This bound is why
  passwordless is *not* the default: there, a stolen unlocked key = login unless
  a FIDO2 PIN is set.)
- **Lockout is a self-inflicted availability risk, not an attack.** A single lost
  key with a `required` line and no backup locks the user out of **all** logins
  (console and greeter alike — door mirrors the stack). Mitigation is
  operational and lives in `docs/yubikey.md`: enroll a backup key, keep a root
  shell open while testing, and the offline `rescue.target` recovery. door's
  service-level revert does not address a PAM-level lockout.
- **Mapping-file tampering** (A6). If an attacker can write `/etc/u2f_mappings`
  they can enroll their own key. Defense is filesystem permissions (root-owned,
  0644) — same trust class as `/etc/shadow`/`/etc/pam.d`; if those are writable,
  the system is already lost. Not a new boundary, but named for completeness.

## 5. Controls (mapped)

| Risk | Control | Where |
|---|---|---|
| Greeter reads/replays the key exchange | Factor runs in the root worker; greeter gets only one-way `Info` cue + final verdict | `doord/src/worker.rs` `ControlConversation`; `protocol` `AuthPrompt::Info` |
| Greeter forges an "authenticated" verdict | Verdict decided only by PAM in the worker (unchanged from auth-path model, invariant A3) | `doord/src/worker.rs`, `doord/src/pam.rs` |
| Password-only compromise | `required` (not `sufficient`) `pam_u2f` after the password module | admin's `/etc/pam.d/system-login` (documented, not shipped) |
| No visible touch prompt (usability → forced-error) | `cue` mandated in the documented line; greeter renders it | `docs/yubikey.md`; greeter `Message::Notice` path |
| Lockout | Backup-key + open-root-shell + rescue-target recovery | `docs/yubikey.md` §"Read this first" |
| Mapping tampering | root-owned 0644 mapping file | `docs/yubikey.md` §2 |

## 6. Greeter multi-prompt + passwordless (built 2026-07-02)

The greeter now renders an **arbitrary** PAM conversation, not just a pre-typed
password (`door-greeter/src/app.rs`): any prompt it cannot pre-answer (a FIDO2
PIN, a second typed factor, or a passwordless first prompt) surfaces PAM's own
text in a focused, masked-if-secret field whose reply is moved straight into a
zeroizing `Secret`; one-way cues render as a distinct pulsing "touch your key"
indicator. This unlocks **passwordless + FIDO2-PIN** (`sufficient pam_u2f …
pinverification=1`, documented in `docs/yubikey.md`). Both new states were
visually verified headless (cage+grim).

**Trust-boundary note:** this is entirely unprivileged, pre-auth UI. The greeter
still only *relays* prompts and *collects* typed input to hand across the seam;
it gains no authority, decides no verdict, and never sees the FIDO2 exchange. So
the boundary claims in §2 are unchanged — the greeter can now answer more prompt
*shapes*, but it is exactly as untrusted as before.

**Passwordless changes one bound (§4):** with `sufficient pam_u2f`, a stolen key
*is* a login unless a **FIDO2 PIN** is set. The PIN (verified on-key, entered via
the new interactive field) restores the "two things needed" property. Deployments
that skip the PIN accept the stolen-key bound knowingly; the docs call this out.

### Residual / follow-ups

- A passwordless-without-PIN deployment is a weaker posture than the 2FA default;
  it is offered but not recommended, and flagged in the docs.
- Interactive-prompt behavior has no automated UI test (the greeter is Iced
  state); it was verified by headless capture instead. A future harness driving a
  mock daemon through a multi-prompt conversation would close that gap.
