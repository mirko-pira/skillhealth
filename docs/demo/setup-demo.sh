#!/usr/bin/env bash
# Generates the fixture environment the demo tape records against.
# Fully synthetic and deterministic: skills, transcripts and CLAUDE.md are
# written from scratch on every run, and the tape pins the clock with --now,
# so the recording is reproducible on any machine. Nothing here is real —
# the skills are a generic, made-up dev portfolio.
set -euo pipefail

DEMO="/tmp/skillhealth-demo"
NOW="2026-06-10T17:00:00Z" # must match docs/demo/env.sh

rm -rf "$DEMO"
mkdir -p "$DEMO/config/skills" "$DEMO/config/plugins/cache/acme/superpowers/5.1.0/skills" \
  "$DEMO/projects/demo-app" "$DEMO/repo/.claude/skills" "$DEMO/cache"

# mkskill <root> <name> <description> <body...>
mkskill() {
  local root="$1" name="$2" desc="$3"
  shift 3
  mkdir -p "$root/$name"
  {
    printf -- '---\nname: %s\ndescription: %s\n---\n\n' "$name" "$desc"
    printf '%s\n' "$@"
  } >"$root/$name/SKILL.md"
}

U="$DEMO/config/skills"
P="$DEMO/config/plugins/cache/acme/superpowers/5.1.0/skills"
R="$DEMO/repo/.claude/skills"

FILLER="State the goal, gather the evidence, then act. Prefer the boring,
obvious solution over the clever one. Verify the result before declaring
done: run the checks, read the output, compare against the acceptance
criteria. If a step is ambiguous, surface the assumption instead of
guessing silently."

# body <n> — n filler paragraphs, to give each skill a realistic token count
body() {
  local i
  for ((i = 0; i < $1; i++)); do printf '%s\n\n' "$FILLER"; done
}

# ── User skills ──
mkskill "$U" commit-msg "Write a conventional-commit message from the staged diff." \
  "Read the staged diff, infer type and scope, keep the subject under 72 chars." "$(body 14)"
mkskill "$U" code-review "Staff-level code review — few themes, a clear merge call." \
  "Review like the engineer who built these systems. Pair with /test-gen on risky changes." "$(body 9)"
mkskill "$U" test-gen "Generate unit tests for the current changes." \
  "Cover the happy path, the guards, and the failure modes. Run after /code-review." "$(body 6)"
mkskill "$U" bundle-budget "Check the production bundle against the size budget." \
  "Diff against the last build; if a route regresses, hand off to /perf-audit." "$(body 8)"
mkskill "$U" api-docs "Generate an API reference from source annotations." \
  "Walk the public surface; feed the call graph to /diagram-gen for the architecture page." "$(body 4)"
mkskill "$U" regex-build "Build and explain a regular expression from examples." \
  "Start from the positive and negative examples, then prove each branch." "$(body 5)"
mkskill "$U" scaffold "Scaffold a new module from a template." "Folders, an entry point, and a smoke test."

# ── Plugin skills (acme/superpowers) ──
mkskill "$P" task-planner "Turn a spec into an ordered, verifiable plan." \
  "Break work into thin vertical slices. Ship the plan through /code-review before executing." "$(body 11)"
mkskill "$P" tdd "Red, green, refactor — failing test first, always." "$(body 5)"
mkskill "$P" diagram-gen "Render architecture diagrams from a spec." "Boxes, edges, exports." "$(body 3)"
mkskill "$P" release-notes "Draft release notes from merged PRs." "Group by type, link the diffs."

# ── Project skills ──
mkskill "$R" perf-audit "Profile hot paths and flag regressions." \
  "Profile the slow path, compare against the budget from /bundle-budget." "$(body 4)"
mkskill "$R" db-migrate "Generate and check a database migration." \
  "Diff the schema, write the up/down pair, dry-run before applying." "$(body 6)"

# Dead skills with old mtimes: never fired AND untouched for months → W004.
# regex-build stays fresh (recently installed, correctly not flagged) — it is
# the live flip target in the TUI demo.
touch -t 202602150900 "$U/scaffold/SKILL.md"
touch -t 202601201200 "$P/release-notes/SKILL.md"

# Debris: a directory in the skills root with no SKILL.md → W003.
mkdir -p "$U/scratch-pad"
echo "leftover scratch file" >"$U/scratch-pad/notes.txt"

# Global CLAUDE.md: real triggers, one pointing at a skill that no longer
# exists (/lint-fix → W007 drift) and one broken @import (→ E002).
cat >"$DEMO/config/CLAUDE.md" <<'EOF'
# CLAUDE.md

@imports/team-style.md

## Triggers
- "review this PR" -> /code-review
- "write the commit" -> /commit-msg
- "plan this feature" -> /task-planner
- "clean up imports" -> /lint-fix
EOF

# fire <skill> <timestamp...> — one Skill tool_use line per invocation.
fire() {
  local skill="$1"
  shift
  for ts in "$@"; do
    printf '{"type":"assistant","timestamp":"%s","message":{"role":"assistant","content":[{"type":"tool_use","id":"t1","name":"Skill","input":{"skill":"%s"}}]}}\n' \
      "$ts" "$skill" >>"$DEMO/projects/demo-app/session1.jsonl"
  done
}

# NOW is 2026-06-10T17:00Z. hot <=7d, warm <=30d, cold older, dead = never.
fire commit-msg 2026-06-10T14:00:00Z 2026-06-09T16:00:00Z 2026-06-08T10:00:00Z \
  2026-06-06T15:00:00Z 2026-06-05T09:00:00Z 2026-06-04T11:00:00Z 2026-06-02T10:00:00Z \
  2026-05-30T14:00:00Z 2026-05-27T09:00:00Z 2026-05-23T16:00:00Z 2026-05-18T10:00:00Z \
  2026-05-12T15:00:00Z 2026-05-04T09:00:00Z 2026-04-22T11:00:00Z
fire task-planner 2026-06-10T12:00:00Z 2026-06-09T10:00:00Z 2026-06-06T15:00:00Z \
  2026-06-04T09:00:00Z 2026-06-03T18:00:00Z 2026-05-28T11:00:00Z 2026-05-21T16:00:00Z \
  2026-05-13T10:00:00Z 2026-05-04T13:00:00Z 2026-04-23T09:00:00Z 2026-04-11T10:00:00Z
fire code-review 2026-06-09T17:00:00Z 2026-06-07T10:00:00Z 2026-06-05T14:00:00Z \
  2026-06-04T09:00:00Z 2026-05-30T15:00:00Z 2026-05-22T11:00:00Z 2026-05-13T10:00:00Z \
  2026-04-29T13:00:00Z 2026-04-16T09:00:00Z
fire db-migrate 2026-06-09T15:00:00Z 2026-06-07T11:00:00Z 2026-06-05T10:00:00Z \
  2026-06-03T17:30:00Z 2026-05-29T14:00:00Z 2026-05-20T09:00:00Z 2026-05-09T10:00:00Z 2026-04-26T13:00:00Z
fire bundle-budget 2026-05-25T10:00:00Z 2026-05-19T14:00:00Z 2026-05-14T09:00:00Z \
  2026-05-06T15:00:00Z 2026-04-27T11:00:00Z 2026-04-15T10:00:00Z
fire tdd 2026-05-26T10:00:00Z 2026-05-19T14:00:00Z 2026-05-12T09:00:00Z 2026-05-02T15:00:00Z 2026-04-20T10:00:00Z
fire test-gen 2026-05-28T11:00:00Z 2026-05-20T14:00:00Z 2026-05-12T10:00:00Z 2026-04-25T15:00:00Z
fire perf-audit 2026-05-20T09:00:00Z 2026-05-08T14:00:00Z 2026-04-21T10:00:00Z
fire api-docs 2026-05-01T10:00:00Z 2026-04-12T14:00:00Z
fire diagram-gen 2026-04-20T10:00:00Z

echo "demo environment ready at $DEMO"
