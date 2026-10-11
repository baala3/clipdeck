#!/usr/bin/env bash
# Runs the built app in a headless X11 session and uses it the way a person
# would: copy, open the menu with the hotkey, choose a Clip, pin one.
# Screenshots land in $OUT for a human to look at; CI uploads them.
#
#   .github/scripts/linux-e2e.sh target/debug/clipdeck-app
#
# Needs: xvfb openbox xdotool xclip imagemagick sqlite3 python3-tk dbus.
set -u

app=$(realpath "$1")
OUT=${OUT:-$PWD/e2e-out}
mkdir -p "$OUT"
work=$(mktemp -d)
# A home of its own, so the app's settings and stores start empty.
export HOME="$work/home" XDG_CONFIG_HOME="$work/home/.config" XDG_DATA_HOME="$work/home/.local/share"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_DATA_HOME"
data="$XDG_DATA_HOME/dev.clipdeck.app"
unset WAYLAND_DISPLAY XDG_SESSION_TYPE

failures=0
check() { # check "what should be true" command...
  local what=$1; shift
  if "$@" > /dev/null 2>&1; then echo "ok    $what"; else echo "FAIL  $what"; failures=$((failures + 1)); fi
}
shot() { import -window root "$OUT/$1.png" 2> /dev/null || echo "(no screenshot $1)"; }
history() { sqlite3 "$data/clipdeck-history.sqlite3" "select coalesce(text, 'image ' || image_width || 'x' || image_height) from clips order by id" 2> /dev/null; }
pinned() { sqlite3 "$data/clipdeck-pinned.sqlite3" "select coalesce(text, 'image ' || image_width || 'x' || image_height) from pinned_clips order by id" 2> /dev/null; }
newest() { history | tail -n 1; }
# Copies the way an app does: xclip stays behind as the clipboard's owner.
copy_text() { printf '%s' "$1" | xclip -selection clipboard; sleep 1; }

Xvfb :77 -screen 0 1280x800x24 > "$OUT/xvfb.log" 2>&1 &
export DISPLAY=:77
sleep 1
openbox > "$OUT/openbox.log" 2>&1 &
sleep 1

# Somewhere to paste into: a text box that writes down whatever it holds.
cat > "$work/target.py" << 'PY'
import sys, tkinter
root = tkinter.Tk()
root.title("paste target")
root.geometry("500x200+40+40")
box = tkinter.Text(root)
box.pack()
box.focus_force()
def save():
    open(sys.argv[1], "w").write(box.get("1.0", "end-1c"))
    root.after(100, save)
save()
root.mainloop()
PY
python3 "$work/target.py" "$work/pasted.txt" &
sleep 2

dbus-run-session -- "$app" > "$OUT/app.log" 2>&1 &
sleep 8
check "the app is running" pgrep -f "$app"
shot 1-started

echo "--- capture"
copy_text "first clip"
copy_text "second clip"
check "copied text reaches History" test "$(newest)" = "second clip"
convert -size 320x200 gradient:blue-orange "$work/picture.png"
xclip -selection clipboard -t image/png -i "$work/picture.png"; sleep 2
check "a copied image reaches History" test "$(newest)" = "image 320x200"
check "its PNG and thumbnail are stored" test "$(ls "$data/images" 2> /dev/null | wc -l)" = 2
copy_text "first clip"
check "copying something again moves it to the top" test "$(history | tr '\n' '|')" = "second clip|image 320x200|first clip|"

echo "--- menu"
xdotool search --name "paste target" windowactivate --sync
xdotool mousemove 700 150
xdotool key ctrl+shift+v
sleep 2
shot 2-menu
check "the hotkey opens a menu" test "$(xdotool search --onlyvisible --class clipdeck 2> /dev/null | wc -l)" -ge 1

echo "--- choose"
# Down to the first Clip, down again to the image, down to "second clip".
xdotool key Down Down Down
sleep 0.5
shot 3-highlighted
xdotool key Return
sleep 2
shot 4-chosen
check "choosing a Clip copies it" test "$(xclip -selection clipboard -o 2> /dev/null)" = "second clip"
check "and moves it to the top of History" test "$(newest)" = "second clip"
check "and pastes it into the app underneath" test "$(cat "$work/pasted.txt")" = "second clip"

echo "--- pin"
xdotool key ctrl+shift+v
sleep 2
# Shift held while choosing the second row, the "first clip".
xdotool key Down Down
xdotool keydown shift key Return keyup shift
sleep 2
check "Shift+choosing pins the Clip" test "$(pinned)" = "first clip"
xdotool key ctrl+shift+b
sleep 2
shot 5-pinned-menu
xdotool key Escape

echo "--- image"
xdotool key ctrl+shift+v
sleep 2
# "second clip", "first clip", then the image.
xdotool key Down Down Down Return
sleep 2
check "choosing an image puts it on the clipboard" sh -c 'xclip -selection clipboard -t TARGETS -o | grep -q image/png'
check "without adding a second copy to History" test "$(history | grep -c '^image')" = 1
check "and moves it to the top" test "$(newest)" = "image 320x200"
shot 6-end

echo "--- History"; history
echo "--- Pinned"; pinned
echo "--- app log"; cat "$OUT/app.log"
pkill -f "$app"; pkill -f target.py; pkill openbox; pkill Xvfb
echo
echo "$failures failed"
[ "$failures" = 0 ]
