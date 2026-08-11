#!/usr/bin/env sh
set -eu
cargo build --release
printf '\nBuilt: %s\n' "$(pwd)/target/release/blinkview"
