#!/bin/sh
# CI entry on GitHub's Ubuntu ARM64 runner: installs the build tools, then
# builds, tests and packages, and runs the end-to-end flow with the real
# gizlet command.
set -eu

root=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd)
sudo apt-get update -q
sudo apt-get install -y -q --no-install-recommends \
    build-essential libclang-dev squashfs-tools python3 dosfstools procps util-linux
"$root/scripts/package.sh"
cargo build --release --locked --no-default-features --example flow
sudo "$root/tests/e2e.sh" "$root/dist"/io.gizlet.emulation-center-*.gzl "$root/target/release/examples/flow"
