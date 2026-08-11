#!/usr/bin/env sh
set -eu

cargo build --release
mkdir -p "$HOME/.local/bin"
install -m 0755 target/release/blinkview "$HOME/.local/bin/blinkview"
"$HOME/.local/bin/blinkview" --startup enable
nohup "$HOME/.local/bin/blinkview" --background >/dev/null 2>&1 &

printf 'Installed BlinkView to %s\n' "$HOME/.local/bin/blinkview"
printf 'Background startup: enabled through a per-user LaunchAgent.\n'
printf 'Use: blinkview --thumbnail INPUT OUTPUT 256 for thumbnail generation.\n'
