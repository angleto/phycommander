# Security Policy

## Supported versions

PhyCMD is maintained as a single active line (`v1.x`). Only the tip of
the default branch receives security fixes. Older tagged releases are
archived and not patched.

| Branch / tag | Security fixes |
|--------------|---------------:|
| `v1.1` (current) | ✅ |
| older `v1.0.*` tags | ❌ |
| any `dependabot/*` branch | ❌ — transient PR branches |

## Reporting a vulnerability

**Do not open a public GitHub issue for security-sensitive reports.**
Instead, email **angelo@leto.blue** with subject `[PhyCMD security]`.

Include:

- The affected component (firmware, Rust server, dashboard, dependency).
- Steps to reproduce or a minimal PoC.
- Impact (what can an attacker do, under which assumed position?).
- Whether you'd like public credit when the fix ships.

You should receive a reply within **7 days**. Please allow up to **90 days**
for a coordinated disclosure window before public write-up. If the
issue is already known and public you're welcome to open a normal
issue referencing the upstream advisory.

## Non-goals

PhyCMD assumes a trusted local network and a trusted USB host. The
dashboard does not implement authentication; it is intended to sit on
an isolated lab LAN. Remote-over-internet deployment is out of scope
and not supported — exposing port 8080 to the public internet is
explicitly unsupported.

## Cryptographic material

There is none. The wire protocol uses CRC-16-CCITT purely as a
*transport* checksum, not a security primitive.
