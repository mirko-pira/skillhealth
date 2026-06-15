# skillhealth TUI demo — reproducible recording.
#   cargo build --release && vhs docs/demo/demo-tui.tape
# Fixture is synthetic (docs/demo/setup-demo.sh), clock pinned via --now.
# A backgrounded append to a transcript fires WHILE the TUI is on screen,
# so the recording shows the live refresh: regex-build flips dead → hot.
# NOTE: vhs 0.11 rejects absolute paths in Output — relative to the cwd vhs
# runs from (repo root).
# NOTE: vhs 0.11 does not parse \" escapes inside double-quoted Type strings —
# the backgrounded append below is backtick-quoted (string contains ' and ").
# The job is disowned or bash prints "[1]+ Done (sleep 11; ...)" over the
# final frame after the TUI quits, leaking the trick on camera.

Output docs/demo/demo-tui.gif

Set Shell bash
Set FontSize 17
Set Width 1200
Set Height 700
Set Padding 18
Set TypingSpeed 55ms

Hide
Type "source docs/demo/env.sh && clear"
Enter
Sleep 4s
Type `( sleep 11; printf '{"type":"assistant","timestamp":"2026-06-10T16:59:00.000Z","message":{"role":"assistant","content":[{"type":"tool_use","id":"t9","name":"Skill","input":{"skill":"regex-build"}}]}}\n' >> "$DEMO/projects/demo-app/session1.jsonl" ) & disown; clear`
Enter
Show

Sleep 600ms
Type "skillhealth --scope all"
Sleep 500ms
Enter
Sleep 4s

Type "j"
Sleep 400ms
Type "j"
Sleep 1.2s

Type "/"
Sleep 300ms
Type "plan"
Sleep 1.5s
Escape
Sleep 800ms

# ~11s mark: the background append lands → watcher → debounce → rescan.
# Header flashes "updated", regex-build jumps up the list as hot.
# (14s put the flip exactly on the doctor transition — verified frame-by-frame.)
Sleep 4s

Type "d"
Sleep 2.5s
Type "j"
Sleep 800ms
Type "y"
Sleep 1.5s

Type "?"
Sleep 2s
Type "q"
Sleep 500ms

Type "q"
Sleep 1s
