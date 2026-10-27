# Security Policy

Kudu listens to your microphone, reads the keyboard and types into other apps, so we take
security seriously. Thank you for helping keep Kudu and its users safe.

## Supported versions

Security fixes go into the latest release. Please update before reporting.

| Version | Supported |
|---|---|
| Latest release (0.1.x) | ✅ |
| Older releases | ❌ |

| Platform | Status |
|---|---|
| macOS | Stable |
| Windows | Stable |
| Linux | Beta (security reports welcome) |

## Reporting a vulnerability

**Please don't open a public issue for security problems.**

Report privately through
[GitHub Security Advisories](https://github.com/manasdutta04/kudu/security/advisories/new)
("Report a vulnerability" on the repository's **Security** tab).

Please include:
- The Kudu version and your OS (version, CPU, and on Linux X11 or Wayland).
- What the problem is and what an attacker could do with it.
- Steps to reproduce, or a proof of concept.
- Any suggested fix, if you have one.

## What to expect

- **Within 3 days:** we confirm we've received your report.
- **Within 7 days:** a first assessment and, if the issue is confirmed, a plan for the fix.
- **Fix and disclosure:** we aim to ship a fix within 30 days for serious issues, then publish an
  advisory crediting you (unless you'd rather stay anonymous).

Please give us a reasonable chance to fix the problem before disclosing it publicly.

## Scope

Especially interesting:
- Audio, transcripts, history or meeting notes leaving the computer, or being readable by
  other users or apps.
- Bypassing the model download's sha256 check, or tampering with the update flow.
- Privilege escalation through the Linux install (the `uinput` group and udev rule), the Windows
  installer or the macOS install script.
- Text being typed into the wrong window, or Kudu typing when you didn't speak.

Out of scope:
- Issues that need an attacker who already has full control of your user account.
- The unsigned-app warnings described in the README (signing is planned).
- Bugs in third-party projects (report those upstream; tell us if Kudu is affected).

## Our privacy promise

Kudu makes no network requests apart from the one-time model download and the daily update
check against GitHub. No analytics, no crash reporting, no accounts. Anything that breaks that
promise is a security bug. See [PRIVACY.md](PRIVACY.md).
