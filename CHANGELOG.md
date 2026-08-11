# Changelog

## 0.1.1

- Added lightweight video audio through `ffplay` with pause/resume restart at the current approximate playback position.
- Added `--thumbnail INPUT OUTPUT [SIZE]` for PNG thumbnails.
- Added Linux Freedesktop thumbnailer registration in the installer.
- Preserved Windows Explorer's existing thumbnail providers instead of overwriting them.
- Added `--startup enable|disable` with Linux, Windows, and macOS implementations.
- Linux and Windows installers now enable startup through BlinkView itself after installation.
- Added macOS install helper with per-user LaunchAgent startup.
- Bumped version to 0.1.1.
