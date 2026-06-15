# Visual verification harness — renders the TUI against the demo fixture and
# dumps PNG frames for design review. Not the marketing GIF (that's demo-tui.tape).
# NOTE: vhs 0.11 rejects absolute paths in Output/Screenshot — paths are
# relative to the repo root (run `vhs docs/demo/tui-visual-check.tape` from there).
# Frames land in target/tui-check/ (gitignored). mkdir -p target/tui-check first.
# NOTE: Screenshot capture lags the action stream — keep >=2s sleep before each
# Screenshot and 1s after, or frames come out one action late (verified empirically).
Output target/tui-check/run.gif

Set Shell bash
Set FontSize 17
Set Width 1200
Set Height 700
Set Padding 18

Hide
Type "source docs/demo/env.sh && clear"
Enter
Sleep 4s
Show

Type "skillhealth"
Enter
Sleep 4s
Screenshot target/tui-check/overview.png
Sleep 1s

Type "j"
Type "j"
Sleep 2s
Screenshot target/tui-check/selection.png
Sleep 1s

Type "S"
Sleep 2s
Screenshot target/tui-check/grouped.png
Sleep 1s
Type "S"
Sleep 1s

Type "/"
Type "plan"
Sleep 2s
Screenshot target/tui-check/filter.png
Sleep 1s
Escape
Sleep 500ms

Type "d"
Sleep 2s
Screenshot target/tui-check/doctor.png
Sleep 1s

Type "?"
Sleep 2s
Screenshot target/tui-check/help.png
Sleep 1s

Type "q"
Sleep 500ms
Type "q"
Sleep 1s
