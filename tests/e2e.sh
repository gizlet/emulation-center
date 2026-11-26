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
if [ -e "$GIZLET_SAVE_DIR/hang" ]; then
    echo running >> "$log"
    exec sleep 3141
fi
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

# The user's card: FAT, with one synthetic game file. A mount table and sysfs
# made for the test present its loop device as partition 1 of mmcblk1, a
# card other than the system's mmcblk0, as on an R36S with two cards.
mkdir -p "$work/media/tf2"
truncate -s 32M "$work/tf2.img"
mkfs.vfat -n GAMES "$work/tf2.img" >/dev/null
mount -o loop,uid=0,gid=0 "$work/tf2.img" "$work/media/tf2"
mkdir "$work/media/tf2/nes"
printf 'NES\032synthetic test file\n' > "$work/media/tf2/nes/homebrew-test.nes"
tf2_device=$(mountpoint -d "$work/media/tf2")
sys=$work/sys
mkdir -p "$sys/block/mmcblk0/mmcblk0p2" "$sys/block/mmcblk1/mmcblk1p1" "$sys/dev/block"
echo 2 > "$sys/block/mmcblk0/mmcblk0p2/partition"
echo 1 > "$sys/block/mmcblk1/mmcblk1p1/partition"
ln -s ../../block/mmcblk0/mmcblk0p2 "$sys/dev/block/179:2"
ln -s ../../block/mmcblk1/mmcblk1p1 "$sys/dev/block/$tf2_device"
{
    echo "21 1 179:2 / / ro,relatime shared:1 - squashfs /dev/root ro"
    grep " $work/media/tf2 " /proc/self/mountinfo
} > "$work/mountinfo"
export EMULATION_CENTER_MOUNTINFO="$work/mountinfo" EMULATION_CENTER_SYSFS="$sys"

"$GIZLET_CLI" install "$center_package"
"$GIZLET_CLI" list | grep -q '^io.gizlet.emulation-center	'

EMULATION_CENTER_UI=$flow FLOW_PUBLISH_UPDATE=$work/publish-update \
    FLOW_TF2=$work/media/tf2 FLOW_GAMES=$work/media/tf2/nes \
    "$GIZLET_CLI" run io.gizlet.emulation-center

"$GIZLET_CLI" list
"$GIZLET_CLI" list | grep -q '^io.gizlet.retroarch	1.0.1	'
! "$GIZLET_CLI" list | grep -q '^io.gizlet.ppsspp	'
log=$GIZLET_PREFIX/saves/io.gizlet.retroarch/opened.log
cat "$log"
grep -q '"version": "1.0.1"' "$log"
grep -qx "args: --games $work/media/tf2" "$log"
grep -qx 'nes' "$log"
grep -qx "args: --games $work/media/tf2/nes" "$log"
grep -qx 'homebrew-test.nes' "$log"
[ ! -e "$GIZLET_PREFIX/saves/io.gizlet.emulation-center/open-request" ]
ls "$GIZLET_PREFIX/saves/io.gizlet.emulation-center/downloads"
[ -z "$(ls "$GIZLET_PREFIX/saves/io.gizlet.emulation-center/downloads" | grep -v '\.part$')" ]

# Holding FN ends the app on the screen: `gizlet run` sends SIGTERM to the
# center's process group. An emulator the center opened must end with it.
saves=$GIZLET_PREFIX/saves
touch "$saves/io.gizlet.retroarch/hang"
cat > "$work/open-once" <<EOF
#!/bin/sh
[ ! -e "\$GIZLET_SAVE_DIR/opened-once" ] || exit 0
touch "\$GIZLET_SAVE_DIR/opened-once"
# The UI runs in the center's process group, the one \`gizlet run\` signals.
ps -o pgid= -p \$\$ | tr -d ' ' > "\$GIZLET_SAVE_DIR/center-group"
printf 'io.gizlet.retroarch\n%s\n' "$work/media/tf2" > "\$GIZLET_SAVE_DIR/open-request"
EOF
chmod 0755 "$work/open-once"
EMULATION_CENTER_UI=$work/open-once "$GIZLET_CLI" run io.gizlet.emulation-center &
center_run=$!
tries=0
until pgrep -x -f 'sleep 3141' >/dev/null; do
    tries=$((tries + 1))
    [ "$tries" -lt 100 ] || { echo 'the emulator did not start' >&2; exit 1; }
    sleep 0.1
done
python3 -c 'import os, signal, sys; os.killpg(int(sys.argv[1]), signal.SIGTERM)' \
    "$(cat "$saves/io.gizlet.emulation-center/center-group")"
wait "$center_run" || :
sleep 0.5
if pgrep -x -f 'sleep 3141' >/dev/null; then
    echo 'the emulator outlived the center' >&2
    exit 1
fi
rm "$saves/io.gizlet.retroarch/hang"
echo 'e2e: installed, updated, refused a corrupted download, found the TF2 card and opened the emulator there and in a chosen folder'
echo 'e2e: ending the center while an emulator runs ends the emulator too'
