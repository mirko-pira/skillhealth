# skillhealth demo — reproducible recording.
#   cargo build --release && vhs docs/demo/demo.tape
# The fixture environment is synthetic (docs/demo/setup-demo.sh) and the
# clock is pinned via --now, so the output is identical on any machine.

Output docs/demo/demo.gif

Set Shell bash
Set FontSize 17
Set Width 1200
Set Height 620
Set Padding 18
Set TypingSpeed 55ms

Hide
Type "source docs/demo/env.sh && clear"
Enter
Sleep 4s
Show

Sleep 600ms
Type "skillhealth --scope all --plain"
Sleep 500ms
Enter
Sleep 5s

Hide
Type "clear"
Enter
Show
Type "skillhealth --scope all doctor"
Sleep 500ms
Enter
Sleep 5.5s

Hide
Type "clear"
Enter
Show
Type "skillhealth --scope all code-review"
Sleep 500ms
Enter
Sleep 5s
