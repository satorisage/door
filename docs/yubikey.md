# Two-factor login with a YubiKey (FIDO2/U2F)

door supports a hardware security key — a YubiKey or any FIDO2/U2F
authenticator — as a **second factor**: you type your password *and* touch the
key to log in. This works because door drives the system's PAM stack faithfully,
relaying every prompt of a multi-round conversation between the daemon and the
greeter. You add one `pam_u2f` line to your login PAM stack; door runs it. There
is **no door-specific configuration** and **no change to door's own files**.

> **How door fits in.** The touch happens inside door's *privileged* daemon
> (which talks to `/dev/hidraw` as root during authentication) — never in the
> unprivileged greeter. The greeter only shows the "touch your key" cue and never
> sees the key or the FIDO2 secret. This is the same trust boundary that protects
> your password.

This guide targets **Arch / CachyOS** (door's primary platform). The mechanism is
standard `pam_u2f`, so other distros differ only in PAM file paths.

---

## ⚠️ Read this first — avoid locking yourself out

Editing your login PAM stack is the one change that can lock you out of **every**
login — console TTYs *and* the graphical greeter (door mirrors the system login
policy, so both use the stack you are about to edit). door's two-command service
revert does **not** help here: the fallback display manager authenticates through
the same PAM stack.

Do all three before you reboot:

1. **Enroll a spare key too.** Register a backup authenticator so one lost or
   broken key is not a lockout.
2. **Keep a root shell open** on a separate TTY (`Ctrl+Alt+F3`, log in as root or
   `sudo -i`) the whole time you are testing. If a new login fails, you revert the
   PAM edit from that already-open shell.
3. **Know the offline recovery path.** If every session is closed and you cannot
   log in: reboot, and at the bootloader append `systemd.unit=rescue.target`
   (or `init=/bin/bash`) to the kernel line, then remove the `pam_u2f` line you
   added and reboot normally.

Test by opening a **new** login (a fresh TTY, or `login` in the open root shell)
— never by logging out of your only session.

---

## 1. Install the module

```bash
sudo pacman -S --needed pam-u2f
```

This provides `pam_u2f.so` and the enrollment tool `pamu2fcfg`. If you installed
door from the AUR, `pam-u2f` is listed as an `optdepend` for exactly this.

## 2. Enroll your key(s)

Plug in the key and generate a mapping line. Because a login manager
authenticates *many* users and the greeter user has no home directory, use a
**system-wide, root-owned mapping file** rather than `~/.config/Yubico/u2f_keys`:

```bash
# First key — creates the file. Touch the key when it blinks.
pamu2fcfg | sudo tee /etc/u2f_mappings

# Each additional/backup key — APPEND (note -n, and >> not >). Touch when it blinks.
pamu2fcfg -n | sudo tee -a /etc/u2f_mappings
```

Lock the file down (it maps usernames to public key handles — not secret, but it
should not be user-writable):

```bash
sudo chmod 644 /etc/u2f_mappings
sudo chown root:root /etc/u2f_mappings
```

Each line is `username:keyhandle,pubkey,...`; multiple keys for one user are
comma-separated on that user's line, which `pamu2fcfg -n` appends for you.

## 3. Add the second-factor line to your login stack

Add `pam_u2f.so` to **`/etc/pam.d/system-login`**, which is included by console
`login`, by display managers, and by door — but **not** by `sudo`. So this makes
*logins* require the key without also gating every `sudo`.

> Prefer `sudo` to require the key too? Add the same line to
> `/etc/pam.d/system-auth` instead (broader — it also covers `sudo`, `passwd`,
> etc.). Pick one; do not add it in both.

Edit `/etc/pam.d/system-login` and add the `pam_u2f` line **after** the existing
`auth ... system-auth` include, so the password is checked first and the key
second:

```
#%PAM-1.0

auth       required   pam_shells.so
auth       requisite  pam_nologin.so
auth       include    system-auth
auth       required   pam_u2f.so authfile=/etc/u2f_mappings cue    # ← add this line
account    include    system-auth
...
```

The three parts that matter:

- **`required`** — the key is a *second factor*: password **and** touch must both
  pass. (Using `sufficient` here would make the key *replace* the password —
  passwordless — which is a different, weaker posture; not what this guide sets
  up.)
- **`authfile=/etc/u2f_mappings`** — read enrollments from the system file you
  created, not from user home directories.
- **`cue`** — **required for door.** It makes `pam_u2f` emit a "Please touch the
  device" message, which door's greeter shows so you know to touch the key.
  Without `cue`, the greeter simply sits on "Authenticating…" while the key waits
  — it still works, but there is no visible prompt. Always include `cue`.

## 4. Test without rebooting

With your root shell still open on the other TTY:

```bash
# From the open root shell, exercise the real stack:
login          # then log in as your user: password, then touch the key when cued
```

Or switch to a fresh TTY (`Ctrl+Alt+F4`) and log in there. You should be asked
for your password, then see the touch cue. Only once a **new** login succeeds
should you trust it and reboot into the greeter.

---

## What you'll see in the greeter

1. Type your username and password, pick a session, press **Log in**.
2. door checks the password, then `pam_u2f` runs and the card shows the touch cue
   (from `cue`).
3. Touch the key. On success the session starts as usual.

A wrong password fails at the password step (the key is never reached). A missing
or wrong key fails after the password with an authentication error, and you can
retry.

## Passwordless (touch replaces password)

The guide above sets up **two-factor** (password + key), door's security-first
default. door **also** supports a **passwordless** login where the key replaces
the password — the greeter renders whatever `pam_u2f` asks, including an
interactive **FIDO2 PIN** field, and shows the touch cue as a distinct "waiting"
indicator.

> **Weigh the trade-off.** A bare touch-only key that is lost or left plugged in
> while unlocked *is* a login. Set a **FIDO2 PIN** on the key
> (`ykman fido access change-pin`) and require it (`pinverification=1` below) so a
> stolen key alone is not enough. Passwordless without a PIN is convenient but
> weaker than the 2FA default.

Use `sufficient` (the key is enough on its own) instead of `required`, placed
**before** the password module so it runs first, and add `pinverification=1` to
demand the PIN:

```
# in /etc/pam.d/system-login, BEFORE the `auth include system-auth` line:
auth       sufficient pam_u2f.so authfile=/etc/u2f_mappings cue pinverification=1
auth       include    system-auth   # password fallback if the key is absent/declined
```

What you'll see in the greeter: type your username, leave the password blank,
press **Sign in**. The greeter shows **"Enter PIN for your security key"** with a
masked field → type the PIN, **Submit** → the **touch cue** → touch → session
starts. If the key is absent, PAM falls through to the password prompt, which the
greeter renders in the same interactive field. The same lockout precautions apply
— keep `sufficient` + a password fallback so a missing key never bricks login.

## Why no network / Yubico OTP mode

door has a hard **no-network-ever** constraint, so Yubico's OTP mode (which
validates one-time codes against YubiCloud or a network validation server) is out
by design. FIDO2/U2F is fully offline — the challenge-response happens locally
between the daemon and the key — which is why it's the supported mechanism.

## Security notes

- The FIDO2 exchange never touches the greeter; door's daemon (root, during
  authentication) owns it. A compromised greeter cannot read, replay, or forge
  the key exchange — it can only *ask* the daemon to try a login.
- `/etc/u2f_mappings` holds public key handles, not secrets, but keep it
  root-owned and non-user-writable so an attacker can't enroll their own key.
- See `.agent/SECURITY/yubikey-2fa-threat-model.md` for the full threat model —
  what this defends against (password-only phishing/shoulder-surf) and what it
  does not (an evil-maid with the key present; a stolen key if you go
  passwordless without a PIN).
