---
name: skillhealth
description: Audit installed agent skills — what you have, what you actually use (hot/warm/cold/dead), what's broken, and how skills relate. Use when the user asks "what skills do I have", "which skills are dead or unused", "audit my skills", "skill health", "skills dashboard", or wants a skill usage report.
---

# skillhealth

Run the skillhealth CLI and interpret the results. The binary is local-only (no network).

1. Run `npx -y skillhealth --json` (use `skillhealth --json` if installed globally).
2. Summarize for the user: totals by temperature, the top dead skills by token weight,
   and any doctor errors.
3. Diagnostics: `npx -y skillhealth doctor` — relay each finding's fix verbatim.
   NEVER apply a fix without the user explicitly asking.
4. Visual exploration: `npx -y skillhealth graph --open` opens the HTML dashboard.
5. Exit codes: 0 healthy, 1 warnings, 2 errors — report the state plainly.
