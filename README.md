# BlinkView

A deliberately small native folder-aware image/video viewer.

The goal is simple: open one file fast, move through neighbouring media fast, preload only a bounded working set, clean decoded data when it falls out of range, and optionally keep a tiny background instance ready for the next file-open request.

## Highlights

- Native Rust executable; no Electron/WebView/browser runtime.
- Open one image or video and automatically discover supported media in the same folder.
- Natural filename order (`2.png` before `10.png`).
- Fast keyboard navigation through the whole folder.
- Asynchronous image decoding.
- Bounded neighbour preloading.
- Immediate cache cleanup outside `current +/- cache-radius`.
- Additional decoded-image RAM budget with `--max-cache-mb`.
- Video frames are streamed through FFmpeg with a tiny bounded frame queue.
- Lightweight best-effort video audio through `ffplay`.
- Pause/resume restarts audio at the current video position instead of keeping an audio engine resident.
- Optional resident/background mode with blocking local IPC while idle.
- Single-instance forwarding: opening another file wakes the existing BlinkView process.
- Cross-platform background startup command for Linux, Windows, and macOS.
- Built-in PNG thumbnail generator for images and videos.
- Linux installer registers BlinkView as a Freedesktop thumbnailer.
- Windows installer preserves Explorer's existing native thumbnail providers rather than replacing them with a heavier shell extension.

## Architecture

BlinkView keeps the hot path intentionally small:

1. **Folder index** — stores paths and media kinds only; it does not decode the whole folder.
2. **Image cache worker** — one decode worker with generation invalidation. Rapid navigation makes stale preload work self-discard.
3. **Bounded image cache** — keeps only nearby decoded images and also enforces a decoded-byte budget.
4. **Video stream** — `ffprobe` reads dimensions/FPS; `ffmpeg` streams raw BGRA frames into a queue of only a few reusable frames.
5. **Simple audio companion** — `ffplay` plays audio only. It is started with the first decoded video frame, stopped on pause/navigation, and restarted at the current approximate media position on resume.
6. **Resident IPC** — localhost TCP blocks while idle; there is no busy polling when BlinkView is waiting in the background.
7. **Thumbnail command** — creates PNG thumbnails on demand and does not keep a separate media database.

The window layer is `minifb`; image decoding is `image-rs`.

## Supported files

### Images

PNG, JPEG, GIF (first decoded image), BMP, WebP, TIFF, ICO, PNM/PPM/PGM/PBM, QOI.

### Videos

MP4, MKV, WebM, MOV, AVI, M4V, MPG/MPEG, WMV, FLV, TS/MTS/M2TS, subject to the installed FFmpeg build.

## Requirements

- Rust stable toolchain
- Linux / Windows / macOS
- `ffmpeg` + `ffprobe` in `PATH` for video frames and video thumbnails
- `ffplay` in `PATH` for video audio

If `ffplay` is missing, BlinkView simply plays videos silently. Image viewing never requires FFmpeg.

On Linux, `minifb` needs the native X11/Wayland development packages required by its selected backend.

## Build

```bash
cargo build --release
```

Binary:

- Linux/macOS: `target/release/blinkview`
- Windows: `target\release\blinkview.exe`

Release profile:

- ThinLTO
- one codegen unit
- `panic = "abort"`
- stripped symbols
- `opt-level = 3`

## Use

```bash
blinkview ~/Pictures/shot001.png
```

| Key | Action |
|---|---|
| Left / Right | Previous / next media |
| PageUp / PageDown | Jump 10 |
| Home / End | First / last |
| Space | Pause/resume video + audio |
| R | Rescan/reload current folder |
| Esc | Close viewer window |
| Q | Quit the whole process |

## Background / resident mode

Start BlinkView without a window:

```bash
blinkview --background
```

Then another invocation:

```bash
blinkview ~/Pictures/a.png
```

forwards the path to the resident instance and exits. The resident instance opens the viewer immediately. Closing the window returns it to the waiting state when background mode is active.

The waiting IPC loop is blocking, so it does not need a busy polling loop while idle.

## Automatic startup

Enable background startup for the current user:

```bash
blinkview --startup enable
```

Disable it:

```bash
blinkview --startup disable
```

Implementation:

- Linux: `~/.config/autostart/blinkview-background.desktop`
- Windows: current-user `Run` registry entry
- macOS: `~/Library/LaunchAgents/me.tamkungz.blinkview.background.plist`

The install scripts enable startup automatically after copying the release binary to its final location.

## Thumbnail generation

BlinkView can create a PNG thumbnail without opening a window:

```bash
blinkview --thumbnail INPUT OUTPUT [SIZE]
```

Example:

```bash
blinkview --thumbnail ~/Pictures/photo.webp /tmp/photo-thumb.png 256
```

For images, BlinkView decodes with `image-rs` and scales directly. For video, it asks FFmpeg for a representative frame and writes PNG.

### Linux file-manager integration

`scripts/install-linux.sh` installs:

```text
~/.local/share/thumbnailers/blinkview.thumbnailer
```

so Freedesktop-compatible thumbnail managers can invoke BlinkView on demand. This is especially useful when BlinkView is selected as the viewer for its supported formats. BlinkView itself does not keep a thumbnail index/cache database; the desktop thumbnail manager owns that cache.

### Windows thumbnail behavior

The Windows installer does **not** replace Explorer's existing image/video thumbnail providers. Common media formats already have shell thumbnail infrastructure, and making BlinkView the default opener should not require replacing those handlers. BlinkView's `--thumbnail` command is still available for external integrations or a future dedicated shell extension.

A custom Windows `IThumbnailProvider` DLL is intentionally not included in this small 0.1.x core because that would add COM registration and an additional in-process shell binary.

### macOS thumbnail behavior

The core `--thumbnail` command works on macOS as well. A dedicated Quick Look extension is not bundled in 0.1.1.

## Cache tuning

Defaults:

```text
preload       = 2
cache-radius  = 4
max-cache-mb  = 384
```

Lower-memory example:

```bash
blinkview --background --preload 1 --cache-radius 2 --max-cache-mb 128
```

- `--preload N` — proactively decode nearby images.
- `--cache-radius N` — decoded images farther than this from the current folder index are removed.
- `--max-cache-mb N` — if decoded pixels still exceed the budget, farthest cached neighbours are removed first.

The current image is retained even if that single image is larger than the configured cache budget.

## Audio design

0.1.1 deliberately avoids adding a full audio framework to the Rust process.

For videos:

- video pixels: `ffmpeg`
- metadata/FPS: `ffprobe`
- audio only: `ffplay -nodisp`

Audio starts when the first video frame arrives. On pause it is stopped; on resume it is restarted near the current video time. This is intentionally simple rather than sample-perfect A/V synchronization, because BlinkView is a lightweight viewer rather than a media-player replacement.

## File association

Point an OS file association at:

```text
blinkview "%1"
```

For the fastest repeated-open path, also enable:

```bash
blinkview --startup enable
```

### Linux

```bash
./scripts/install-linux.sh
```

Installs the binary, desktop entry, thumbnailer, and background startup entry. It does **not** forcibly change your default app.

### Windows

```powershell
.\scripts\install-windows.ps1
```

Installs into `%LOCALAPPDATA%\BlinkView`, registers BlinkView under **Open with**, preserves existing Explorer thumbnail providers, and enables background startup.

Modern Windows may still ask the user to select BlinkView as the default app.

### macOS

```bash
./scripts/install-macos.sh
```

Installs the CLI binary to `~/.local/bin` and enables the per-user LaunchAgent.

## Cross-platform CI

`.github/workflows/build.yml` builds and tests on:

- Ubuntu
- Windows
- macOS

## Known limitations of 0.1.1

- Audio synchronization is intentionally approximate, especially immediately after pause/resume or heavy frame drops.
- If `ffplay` is unavailable, video playback is silent.
- No seek bar / mouse UI / metadata panel.
- Animated image formats are treated as still images rather than animation timelines.
- Image/video fitting uses a simple CPU nearest-neighbour scaler in the viewer hot path.
- Linux gets direct `.thumbnailer` integration; Windows does not yet ship a COM `IThumbnailProvider` DLL and macOS does not yet ship a Quick Look extension.
- Local IPC uses a configurable localhost TCP port. If port `43119` is occupied by another program, use `--port` consistently or change the default.

## License

MIT License. Copyright (c) 2026 TamKungZ_ <dev@tamkungz.me>.
