#!/bin/sh
# Builds Emulation Center for aarch64 Gizlet OS on ARM64 Linux, runs the
# tests, and writes the .gzl and its notices under dist/ (or $GIZLET_DIST).
#
# Needs Rust, a C compiler with static glibc, Python 3.11+, curl, libclang
# (for raylib-sys's bindings) and squashfs-tools. The UI links Gizlet
# runtime 1's libraylib.so.600, taken from the OS release pinned below.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
dist=${GIZLET_DIST:-$root/dist}
if [ "$(uname -s)" != Linux ] || [ "$(uname -m)" != aarch64 ]; then
    echo 'Build on ARM64 Linux.' >&2
    exit 1
fi
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

os_release=v0.3.1
runtime_sha256=aa71157d5ecec8316692175670b956e8f8363658e82cff2f64eb7173665d014b
curl -fsSL --retry 3 -o "$work/runtime.sqfs" "https://dl.gizlet.io/releases/$os_release/runtime/gizlet-runtime-1.sqfs"
echo "$runtime_sha256  $work/runtime.sqfs" | sha256sum -c --quiet
unsquashfs -q -n -d "$work/runtime" "$work/runtime.sqfs" lib/libraylib.so.600 lib/libc.so.6 >/dev/null
mkdir "$work/raylib"
cp "$work/runtime/lib/libraylib.so.600" "$work/raylib/"
ln -s libraylib.so.600 "$work/raylib/libraylib.so"

cd "$root"
version=$(python3 -c 'import tomllib; print(tomllib.load(open("gizlet.toml", "rb"))["version"])')
cargo test --locked --no-default-features
GIZLET_RAYLIB_LIB="$work/raylib" GIZLET_APP_VERSION="$version" \
    cargo build --release --locked --features gizlet-os --bin emulation-center
cc -Wall -Wextra -Werror -std=c17 -O2 -static -s app/launch.c -o "$work/launch"

ui=target/release/emulation-center
python3 scripts/check_elf.py "$ui" "$work/runtime/lib/libc.so.6"

app=$work/app
mkdir -p "$app/bin" "$app/assets/fonts" "$app/licenses" "$dist/sources"
install -m 0755 "$work/launch" "$app/bin/emulation-center"
install -m 0755 app/emulation-center.sh "$app/bin/emulation-center.sh"
install -m 0755 "$ui" "$app/bin/emulation-center-ui"
install -m 0644 assets/fonts/*.ttf "$app/assets/fonts/"
install -m 0644 LICENSE "$app/licenses/emulation-center-MIT.txt"
install -m 0644 assets/fonts/Inter-OFL.txt assets/fonts/SpaceGrotesk-OFL.txt \
    assets/fonts/DroidSansFallback-LICENSE.txt "$app/licenses/"
install -m 0644 NOTICE.md "$app/NOTICE.md"
python3 scripts/pack.py "$app" "$dist"

cp NOTICE.md "$dist/sources/io.gizlet.emulation-center-$version-NOTICE.md"
git -C "$root" archive --format=tar --prefix="emulation-center-$version/" \
    -o "$dist/sources/io.gizlet.emulation-center-$version-source.tar" HEAD
ls -l "$dist" "$dist/sources"
