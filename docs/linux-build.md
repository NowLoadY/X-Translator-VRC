# Linux Build

The desktop client is built with `eframe`/`egui`. `eframe` selects the native
`winit` backend for Linux (X11 or Wayland), so Linux does not need a second
window framework or a platform-specific application entry point.

## Prerequisites

Install Rust 1.95 or newer and the native libraries used by the selected
backend. `eframe`/`egui` 0.36 require Rust 1.95; older toolchains fail before
platform compilation begins.
On Debian/Ubuntu this is typically:

```sh
sudo apt install build-essential pkg-config libx11-dev libxcursor-dev \
  libxrandr-dev libxi-dev libwayland-dev libxkbcommon-dev libasound2-dev \
  libssl-dev
```

The `xr-corpus-core` path dependency must be initialized at `XR-Corpus/`.
The core Linux client does not link to MPV. This keeps model download,
inference, session, and non-video plugins independent from native player
libraries. Build the optional MPV capability only when video playback or MPV
audio extraction is needed:

```sh
cargo build -p rust-client --features mpv
```

That build requires a system `libmpv` development package, for example
`libmpv-dev` on Debian-based distributions. Without `--features mpv`, the
player reports a clear capability error and Symphonia-only audio imports still
work; unsupported formats do not silently fall through to a missing native
library.

## Build and run

```sh
git submodule update --init XR-Corpus
cargo build -p rust-client --release
cargo run -p rust-client --release
```

Use `WINIT_UNIX_BACKEND=x11` or `WINIT_UNIX_BACKEND=wayland` to select a window
backend when both are available. Linux builds keep the same host/plugin
composition and shared runtime contracts as Windows builds.

The llama.cpp installer selects archives by the declared target metadata in
`config.json`; the verified b10333 Ubuntu x86_64 CPU archive is configured for
`linux-x86_64`. Archives may be declared as `zip` or `tar-gz`; the extractor
validates paths, rejects links/special entries, and restores executable
permissions on Unix. Linux GPU archives can be added later as separate
declared capabilities without changing model download or inference code.

## Platform limits

Microphone capture and TTS use CPAL on Linux. Windows WASAPI loopback is not
silently emulated: system-audio capture reports an actionable unavailable
error until a PipeWire/PulseAudio implementation is added. The Windows-only
embedded mpv child window is similarly isolated behind the player window host;
Linux still builds and can use mpv for non-embedded operations.
