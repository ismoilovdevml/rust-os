#!/usr/bin/env bash
#
# QEMU runner for `cargo run` and `cargo test`.
#
# `bootimage` decides whether a binary is a test by checking that it sits in a
# directory named `deps`. Cargo 1.9x moved test executables to
# `build/<pkg>/<hash>/out/`, so that check now always answers "not a test": the
# `test-args` in Cargo.toml (headless display, isa-debug-exit device) are
# dropped, and every test hangs until the timeout instead of exiting.
#
# Staging test binaries under a `deps/` directory restores the intended
# behaviour without forking an unmaintained tool.
set -euo pipefail

kernel=$1
shift

parent=$(dirname "$kernel")
if [[ $(basename "$parent") == "out" ]]; then
    staged="$parent/deps"
    mkdir -p "$staged"
    cp -f "$kernel" "$staged/"
    kernel="$staged/$(basename "$kernel")"
fi

exec bootimage runner "$kernel" "$@"
