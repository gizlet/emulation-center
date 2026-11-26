#!/bin/sh
# Emulation Center's entry script, run by `gizlet run` through the static
# bin/emulation-center. It shows the center; when the player opens an
# emulator there, the center writes open-request in its save folder and
# exits, and this script runs the emulator with `gizlet run` in a session of
# its own, then shows the center again once the emulator exits.
#
# The emulator's own `gizlet run` ends it when the player holds FN, or
# Select and Start, as this app's does: both see the buttons. If this script
# is told to stop, it first ends the emulator's session, so the launcher never
# comes back while the emulator still holds the screen.
set -u

app_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
save=${GIZLET_SAVE_DIR:-$app_dir/save}
request=$save/open-request
gizlet=${GIZLET_CLI:-gizlet}

# Gizlet runtime 1's runner, as the launcher's start script finds it.
runtime_runner() {
    readlink -e /run/gizlet-ota-runtime/bin/gizlet-run ||
        readlink -e /opt/gizlet/runtime/1-current/bin/gizlet-run ||
        printf '%s\n' /opt/gizlet/runtime/1/bin/gizlet-run
}

ui() {
    if [ -n "${EMULATION_CENTER_UI:-}" ]; then
        # Tests drive the center's logic without a display.
        "$EMULATION_CENTER_UI"
    else
        "$(runtime_runner)" "$app_dir/bin/emulation-center-ui"
    fi
}

session=
# Ends what runs in the emulator's session: SIGTERM, then SIGKILL after one
# and a half seconds, well before the outer `gizlet run` kills this script
# after its three.
end_session() {
    [ -n "$session" ] || return 0
    pkill -TERM -s "$session" 2>/dev/null || :
    tries=0
    while pgrep -s "$session" >/dev/null 2>&1 && [ "$tries" -lt 15 ]; do
        sleep 0.1
        tries=$((tries + 1))
    done
    pkill -KILL -s "$session" 2>/dev/null || :
}
trap 'end_session; exit 143' TERM INT HUP

mkdir -p "$save"
while :; do
    rm -f "$request"
    ui
    status=$?
    [ -s "$request" ] || exit "$status"
    { read -r id && read -r games; } < "$request" || exit 1
    rm -f "$request"
    echo "Emulation Center: opening $id with $games"
    setsid "$gizlet" run "$id" --games "$games" &
    session=$!
    wait "$session"
    echo "Emulation Center: $id finished with status $?"
    session=
done
