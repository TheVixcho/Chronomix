# Chronomix

**Chronomix** is a cross-platform real-time **Audio DAW** (Digital Audio Workstation) and **VST2 / VST3 host** written in native Rust (`eframe`/`egui` + `cpal` + `vst3-host` + `libloading`), with integrated Wine VST2 (`.dll`) and VST3 (`.vst3`) bridging on Linux so you can monitor, process live audio with effects, tweak plugin interfaces, and record WAV takes in real time.

## Workspace Structure

- `crates/pedal-audio` — Cross-platform full-duplex `cpal` I/O, device deduplication, mono-to-stereo upmixing, sample-rate bridging, and lock-free `rtrb` ring buffer.
- `crates/pedal-engine` — Real-time DSP pipeline, native VST2 (`.so`/`.dll`) & VST3 (`.vst3`) hosting, Wine VST2/VST3 orchestration, VST folder discovery (`vst_plugins/`), peak meters, and lock-free WAV recording (`hound`).
- `crates/pedal-wine` — Automatic PE/ELF binary detection and Winelib VST2/VST3 host (`chronomix-wine-host`) for running 64-bit Windows `.vst3` and `.dll` plugins transparently on Linux via shared memory and `AF_UNIX` IPC.
- `crates/pedal-app` — Native `eframe`/`egui` desktop DAW interface (`chronomix`) with **What's New**, **Recording**, and **Settings** panels (`ES` / `EN`).

## Build & Run

```bash
cargo run --release
```

Or explicitly:

```bash
cargo run --bin chronomix
```

Run tests:

```bash
cargo test --workspace
```

### Linux Notes
- Requires ALSA development headers (`libasound2-dev` / `alsa-lib-devel`), PipeWire (`pipewire-alsa` / `pipewire-pulse`) or PulseAudio for low-latency audio monitoring, and X11/Wayland/GTK3 dev libs for `eframe` and `rfd`.
- For automatic 64-bit Windows `.vst3` / `.dll` (VST2) support on Linux, ensure `wine` (and optionally `wineg++`) is installed. Chronomix's **Settings -> Wine** manager automatically detects `~/.wine`, Bottles, Lutris, Heroic, and custom prefixes, auto-configures `x11,wayland` graphics drivers for Wayland/Hyprland/X11 sessions, compiles the Winelib bridge at runtime if needed, and lets you initialize prefixes or launch `.exe`/`.msi` VST installers, `winecfg`, Explorer, and `winetricks` directly inside the app.

### Windows / macOS Notes
- On Windows, WASAPI works out of the box; for ASIO support in `cpal`, enable `cpal`'s `asio` feature with the Steinberg ASIO SDK configured.
- On macOS, CoreAudio is used natively.
