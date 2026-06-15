# Security Policy

## Supported versions

skillhealth is pre-1.0; only the latest published release receives security
fixes.

| Version | Supported |
|---------|-----------|
| 0.2.x   | ✅        |
| < 0.2   | ❌        |

## Threat model

skillhealth is a local, read-only CLI: zero network, zero telemetry. It does
not transmit anything and it never writes to your skill files (the doctor
reports, it never edits). The realistic attack surface is therefore **untrusted
input it parses** — a crafted `SKILL.md`, `CLAUDE.md`, transcript, or
`history.jsonl` that could cause skillhealth itself to crash, hang, traverse
outside its intended roots, or render attacker-controlled content into a report
(JSON, Markdown, or the HTML dashboard). Reports of that shape are in scope.

Out of scope: vulnerabilities in the skills or transcripts themselves (that is
what the planned supply-chain scan is for), and anything requiring a malicious
local binary already running as your user.

## Reporting a vulnerability

**Please do not open a public issue for security reports.**

Use GitHub's private vulnerability reporting on this repository:
**Security → Report a vulnerability**
(<https://github.com/mirko-pira/skillhealth/security/advisories/new>).

If you cannot use GitHub advisories, email **mp@mirkodev.com** with `skillhealth
security` in the subject.

What to expect:

- An acknowledgement within **5 business days**.
- An assessment and, if confirmed, a fix target, kept in private contact with you.
- Coordinated disclosure: please allow up to **90 days** before any public
  detail, and you will be credited in the advisory unless you prefer otherwise.
