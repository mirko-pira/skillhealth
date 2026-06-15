# Changelog

## 0.2.0 — 2026-06-15

- **Scope picker** (`--scope project|all|user`): auto-detected as `project` when the repo has a `.claude/skills` dir; otherwise `all`; `p` key in the TUI cycles scopes live.
- **Project lens** (`--lens project|global`): filters usage heat to the current project root; `L` key in the TUI toggles the lens.
- **Disabled-plugin awareness**: skills turned off via `enabledPlugins` are flagged `off` in both the overview source column and the detail view, and excluded from the always-on total.
- **Honest cost split**: every skill reports `always_on` (loaded every session) and `on_fire` (loaded only when invoked) token costs separately; the overview footer shows the always-on total.
- **History as second usage source** (`history.jsonl`): Claude Code's typed-command log is parsed as a parallel signal. Doctor finding W010 fires when a skill appears in history but has zero transcript usage, flagging likely transcript rotation or a wrong `--projects-dir`.
- **`--projects-dir` / `--config-dir` decoupled**: the two roots are now independent flags; `--projects-dir` defaults to `~/.claude/projects` while `--config-dir` stays at `~/.claude`.
- **Walk-up boundary fix**: project-scope discovery walks up from `cwd` collecting `.claude/skills` dirs, stopping at the first `.git` ancestor (inclusive); prevents leaking skills from outside the repo into project scope.

## 0.1.0 — unreleased

Initial release: discovery (user/project/plugin roots), usage heat from transcripts
(hot/warm/cold/dead) with incremental cache, lexical relationship graph, doctor with
copy-pasteable fixes, HTML dashboard, --json/--md outputs, semantic exit codes.
