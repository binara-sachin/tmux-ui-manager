# VHS tape for the top-of-README demo. Regenerate with `make demo` (requires
# https://github.com/charmbracelet/vhs — needs vhs, ttyd, and ffmpeg on PATH,
# plus a Chromium/Chrome binary on PATH for vhs's headless capture).
#
# Note: VHS drives real keystrokes into a real pty, not mouse events, so this
# demonstrates window drag-and-drop via keyboard move-mode (Space to pick up,
# arrows to move the target cursor, Enter to commit) rather than a literal
# mouse drag — visually it's the same insertion-highlight/footer-sentence
# feedback either way (see README's "Manual acceptance script" for the mouse
# path, which isn't scriptable headlessly).
#
# Sequence: open the popup, cascade-navigate sessions -> windows -> panes,
# move a window from "scratch" into "main" via keyboard move-mode, rename the
# moved window inline, close.

Output demo.gif

Require tmux

Set Shell "bash"
Set FontSize 16
Set Width 1200
Set Height 700
Set Padding 0
Set TypingSpeed 60ms

# --- fixture setup: a throwaway, isolated tmux server with a few named
# sessions/windows so the demo isn't empty. Hidden so only the popup itself
# shows up in the recording.
Hide
Type "tmux -L demo-tape kill-server 2>/dev/null; tmux -L demo-tape -f /dev/null new-session -d -s main -n editor -c ~"
Enter
Type "tmux -L demo-tape split-window -d -t main:editor -c ~"
Enter
Type "tmux -L demo-tape new-window -d -t main: -n logs -c ~"
Enter
Type "tmux -L demo-tape new-session -d -s scratch -c ~"
Enter
Type "tmux -L demo-tape run-shell $(pwd)/manager.tmux"
Enter
Type "clear; tmux -L demo-tape attach -t main"
Enter
Sleep 500ms
Show

# --- open the popup
Ctrl+B
Type "e"
Sleep 1s

# --- Miller-column cascade: sessions -> windows -> panes
Down
Sleep 400ms
Right
Sleep 400ms
Right
Sleep 600ms

# --- pick up "scratch"'s only window and move it into "main" via keyboard
# move-mode (Space to pick up, arrows to move the drop-target cursor, Enter
# to commit)
Left
Sleep 300ms
Space
Sleep 500ms
Left
Sleep 400ms
Up
Sleep 600ms
Enter
Sleep 1s

# --- inline rename of the just-moved window
Right
Sleep 300ms
Down
Sleep 300ms
Down
Sleep 400ms
Type "r"
Sleep 500ms
Backspace 4
Sleep 300ms
Type "release-notes"
Sleep 500ms
Enter
Sleep 1s

# --- close the popup
Escape
Sleep 500ms

Hide
Type "tmux -L demo-tape kill-server"
Enter
Show
