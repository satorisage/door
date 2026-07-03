# Engagement — prove `No network, ever` holds (red-team the no-network claim)

**Date opened:** 2026-07-03
**Status:** SCOPED — awaiting owner-run execution
**Commissioned by:** DECISION-0018 (in place of a pre-auth network indicator)
**Operator:** the owner (CRTO / OSCP) — this is an owner-run offensive engagement,
recorded here as a runtime security act (findings sync back per D-0037).
**Complements** `2026-06-30-ipc-pentest.md` (IPC trust boundary) and the auth /
session-spawn threat models — this one targets the **network** axis specifically.

## Thesis under test

`No network, ever` is a **hard constraint** (PROJECT-SCOPE `## Hard constraints`)
and door's cleanest security headline: the greeter and daemon do **no network
I/O**, so the pre-auth surface is *provably-by-construction* unreachable from and
unable to reach the network. This engagement's job is to try to **falsify** that —
to find any egress, ingress, or network-reachable code path on the pre-auth
surface — and, failing to, to convert "provable by construction" into "construction
verified adversarially."

A clean run does **not** license lifting the constraint (absence of a found bug
is not absence of bugs — that asymmetry is exactly why the constraint stays
absolute, D-0018). It raises confidence and produces a durable regression record.

## Scope — what to attack

The two processes that exist **before authentication**, plus the handoff:

1. **`door-greeter`** (unprivileged, pre-auth UI) — the highest-value target: it
   runs before any login and renders attacker-influenceable theme/asset input.
2. **`doord`** (privileged daemon) — PAM, logind, VT/seat, IPC server, session
   spawn. The TCB.
3. **The session-spawn environment** — what the launched session inherits (a leak
   path *out* of the sanitized env).

## Test plan (falsification attempts)

- **Static egress audit.** Grep the whole workspace + the dependency tree for
  socket/DNS/HTTP surface: `std::net`, `TcpStream`/`UdpSocket`, `getaddrinfo`,
  `reqwest`/`hyper`/`curl`/`ureq`, any `AF_INET`/`AF_INET6` construction. Expect:
  only `AF_UNIX` (the local IPC socket). Record the dependency-tree sweep so a
  future crate that pulls in a networked transitive dep is caught.
- **Runtime egress observation.** Boot doord + greeter and watch for any network
  syscall / packet: `ss -tunap` (no listening/connecting inet sockets owned by
  doord/greeter), `strace -f -e trace=network` on both PIDs across a full
  login → logout → recycle cycle, and a packet capture (`tcpdump`) on all
  interfaces during the same cycle — expect zero packets attributable to either.
- **Namespace-level proof.** Run the pair under a network namespace with **no
  interfaces** (`unshare -n`) and confirm the greeter + full login still work —
  i.e. door is *indifferent* to the absence of a network (positive proof it needs
  none). Cross-check against the seccomp filter: whether `socket(AF_INET, …)` is
  even reachable / permitted under `DOORD_SECCOMP=enforce`.
- **Ingress / reachability.** Confirm nothing pre-auth binds an inet port; confirm
  the IPC socket is `AF_UNIX` with peer-cred authorization (already covered by the
  IPC pentest — re-assert here that it is the *only* listener).
- **IPC-seam & env leak.** Verify no greeter-supplied field (theme path, asset
  URL-looking string, session `Exec=`) can induce the daemon to open a network
  connection or resolve a name; verify the sanitized session-spawn env carries no
  network-relevant secret out of the TCB.

## Deliverable

A findings report filed here (`.agent/SECURITY/<date>-no-network-pentest.md`,
mirroring `2026-06-30-ipc-pentest.md`): verdict, any findings with severity +
disposition, and — ideally — a **permanent regression** (e.g. a workspace test /
CI grep asserting no `std::net` / inet-socket construction reaches the pre-auth
crates) so the no-network claim stays true as the tree evolves.

## Owner run-book (starting point)

```sh
# 1. Static egress sweep (workspace + deps)
grep -rInE 'std::net|TcpStream|UdpSocket|AF_INET|getaddrinfo|reqwest|hyper::|ureq|curl' \
  --include='*.rs' . | grep -v '/target/'
cargo tree -e no-dev 2>/dev/null | grep -iE 'reqwest|hyper|tokio-.*net|ureq|curl|trust-dns|rustls|native-tls' || echo "no networked deps"

# 2. Runtime: nothing inet-bound/connected by doord or greeter
ss -tunap | grep -iE 'doord|greeter' || echo "no inet sockets — expected"

# 3. Positive proof: door is indifferent to having no network at all
#    (run the greeter/login under a no-interface netns; expect full function)
#    sudo unshare -n --  <launch doord+greeter test harness here>

# 4. Syscall + packet capture across a full login cycle (root)
#    sudo strace -f -e trace=network -p "$(pgrep -x doord)" &
#    sudo tcpdump -ni any -c 50 &   # expect no doord/greeter-attributable traffic
```
