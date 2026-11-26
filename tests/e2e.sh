#!/bin/sh
# The normal flow with the real gizlet command, run as root on ARM64 Linux:
# install Emulation Center from its .gzl, then, through `gizlet run`, install
# an emulator from a fixture Store, update it, refuse a corrupted download,
# choose the folder on a user's FAT card and open the emulator there. The
# emulators are small test packages; the card holds one synthetic file.
#
#   tests/e2e.sh dist/io.gizlet.emulation-center-VERSION-...gzl target/release/examples/flow
set -eu

here=$(CDPATH='' cd -- "$(dirname -- "$0")" && pwd)
root=$(dirname "$here")
center_package=$(realpath "$1")
flow=$(realpath "$2")
[ "$(id -u)" -eq 0 ] || { echo 'Run as root: it mounts a test card.' >&2; exit 1; }
work=$(mktemp -d)
server=
cleanup() {
    [ -z "$server" ] || kill "$server" 2>/dev/null || :
    umount "$work/media/tf2" 2>/dev/null || :
    rm -rf --one-file-system "$work"
}
trap cleanup EXIT

# The gizlet command from the Launcher release of Gizlet OS v0.3.1.
bundle=$work/launcher.gizlet
curl -fsSL --retry 3 -o "$bundle" https://dl.gizlet.io/releases/v0.3.1/launcher/gizlet-launcher-0.3.1.gizlet
echo "95e6a731acd98889bd533e26a1accc3aa79dc89f0d80e9b5cf56cd5ae32ede94  $bundle" | sha256sum -c --quiet
signature=$(head -c 8 "$bundle" | sed 's/^0*//')
mkdir "$work/launcher"
tail -c "+$((9 + signature + 1))" "$bundle" | tar -x -f - -C "$work/launcher" bin/cli
export GIZLET_CLI="$work/launcher/bin/cli"
export GIZLET_PREFIX="$work/prefix"
[ "$("$GIZLET_CLI" target)" = "aarch64-unknown-linux-gnu lite" ]

# A fixture Store: a RetroArch stand-in at 1.0.0, and a PPSSPP stand-in
# whose download does not match its checksum.
port=18$(( $$ % 900 + 100 ))
export GIZLET_STORE_API="http://127.0.0.1:$port"
cc -static -O2 -s "$root/app/launch.c" -o "$work/launch"
emulator() {
    dir=$work/build/$1-$2
    mkdir -p "$dir/bin"
    cp "$work/launch" "$dir/bin/launch"
    cat > "$dir/bin/launch.sh" <<'EOF'
#!/bin/sh
# A test emulator: records how it was opened and what it finds in the games folder.
log=$GIZLET_SAVE_DIR/opened.log
echo "$GIZLET_APP_ID $(cat "$GIZLET_APP_DIR/manifest.json" | grep '"version"')" >> "$log"
echo "args: $*" >> "$log"
[ "$1" = --games ] && ls -1 "$2" >> "$log"
EOF
    chmod 0755 "$dir/bin/launch.sh"
    python3 "$here/make_package.py" "io.gizlet.$1" "$2" "$dir" "$work/store/files" "$GIZLET_STORE_API" "$work/store/gizlet__$1.json" ${3:-}
}
emulator retroarch 1.0.0
emulator ppsspp 1.0.0 --corrupt
cat > "$work/publish-update" <<EOF
#!/bin/sh
exec python3 "$here/make_package.py" io.gizlet.retroarch 1.0.1 "$work/build/retroarch-1.0.0" \
    "$work/store/files" "$GIZLET_STORE_API" "$work/store/gizlet__retroarch.json"
EOF
chmod 0755 "$work/publish-update"
python3 "$here/fixture_store.py" "$work/store" "$port" &
server=$!
sleep 1

# The user's card: FAT, with one synthetic game file.
mkdir -p "$work/media/tf2"
truncate -s 32M "$work/tf2.img"
mkfs.vfat -n GAMES "$work/tf2.img" >/dev/null
mount -o loop,uid=0,gid=0 "$work/tf2.img" "$work/media/tf2"
mkdir "$work/media/tf2/nes"
printf 'NES\032synthetic test file\n' > "$work/media/tf2/nes/homebrew-test.nes"

"$GIZLET_CLI" install "$center_package"
"$GIZLET_CLI" list | grep -q '^io.gizlet.emulation-center	'

EMULATION_CENTER_UI=$flow FLOW_PUBLISH_UPDATE=$work/publish-update FLOW_GAMES=$work/media/tf2/nes \
    "$GIZLET_CLI" run io.gizlet.emulation-center

"$GIZLET_CLI" list
"$GIZLET_CLI" list | grep -q '^io.gizlet.retroarch	1.0.1	'
! "$GIZLET_CLI" list | grep -q '^io.gizlet.ppsspp	'
log=$GIZLET_PREFIX/saves/io.gizlet.retroarch/opened.log
cat "$log"
grep -q '"version": "1.0.1"' "$log"
grep -qx "args: --games $work/media/tf2/nes" "$log"
grep -qx 'homebrew-test.nes' "$log"
[ ! -e "$GIZLET_PREFIX/saves/io.gizlet.emulation-center/open-request" ]
ls "$GIZLET_PREFIX/saves/io.gizlet.emulation-center/downloads"
[ -z "$(ls "$GIZLET_PREFIX/saves/io.gizlet.emulation-center/downloads" | grep -v '\.part$')" ]
echo 'e2e: installed, updated, refused a corrupted download and opened the emulator on the card'
