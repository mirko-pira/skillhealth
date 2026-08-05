# Changelog

## Unreleased

- **Plugin-aware doctor**: dead plugin skills no longer get per-skill `rm -r` fixes pointing
  into the synced plugin cache (wrong and non-actionable). They roll up into one **W011**
  finding per plugin — `mostly-dead plugin — <name> (dead/total skills unused, ~N always-on
  tokens/session)` — with a disable-the-plugin fix. Fires only when at least half of a
  plugin's skills are dead; plugins that earn their keep stay quiet. On a plugin-heavy
  install this collapses hundreds of warnings into a handful of actionable lines.
- **W006 heavy-skill fix is source-aware**: plugin-vendored skills get "report upstream or
  disable" instead of a restructuring suggestion you can't apply to the cache.
- **Short flags**: `-s` for `--scope`, `-l` for `--lens`.
- CI: weekly scheduled run (advisory drift surfaces without a push), `checkout@v7`,
  `allow-dirty` for the dist-generated release.yml so dependabot action bumps pass `dist plan`.
- Deps refreshed: clears RUSTSEC-2026-0190 (anyhow) and RUSTSEC-2026-0204 (crossbeam-epoch).

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
