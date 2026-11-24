#!/usr/bin/env bash
set -u

if [ -f "${HOME:-}/.cargo/env" ]; then
    . "${HOME}/.cargo/env"
fi

MISSING_ITEMS=()
WARNINGS=()

add_missing() {
    MISSING_ITEMS+=("$1")
}

command -v cargo >/dev/null 2>&1 || add_missing "Falta Rust/Cargo (>= 1.85, https://rustup.rs)."
command -v cmake >/dev/null 2>&1 || add_missing "Falta CMake (>= 3.22) para compilar JUCE."
if ! command -v g++ >/dev/null 2>&1 && ! command -v clang++ >/dev/null 2>&1; then
    add_missing "Falta compilador C++20 (g++ o clang++)."
fi
command -v pkg-config >/dev/null 2>&1 || add_missing "Falta pkg-config."

if command -v pkg-config >/dev/null 2>&1; then
    for spec in \
        "alsa|ALSA" \
        "libpipewire-0.3|PipeWire" \
        "libpulse|PulseAudio" \
        "freetype2|FreeType2 (JUCE)" \
        "fontconfig|Fontconfig (JUCE)" \
        "x11|X11 (JUCE)" \
        "xrandr|XRandR (JUCE)" \
        "xinerama|Xinerama (JUCE)" \
        "xcursor|XCursor (JUCE)"; do
        IFS='|' read -r mod label <<< "$spec"
        if ! pkg-config --exists "$mod" 2>/dev/null; then
            add_missing "Falta librería de desarrollo ${label} (pkg-config: ${mod})."
        fi
    done
fi

if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    WARNINGS+=("No se detectó DISPLAY ni WAYLAND_DISPLAY; la ventana gráfica podría no abrirse.")
fi

if ! command -v wineg++ >/dev/null 2>&1; then
    WARNINGS+=("Falta wineg++ (opcional): Chronomix funcionará con plugins nativos, pero los plugins Windows (.dll/.vst3) mostrarán aviso en tiempo de ejecución hasta instalar wineg++.")
fi

for w in "${WARNINGS[@]}"; do
    echo "[AVISO] $w"
done

if [ "${#MISSING_ITEMS[@]}" -gt 0 ]; then
    for item in "${MISSING_ITEMS[@]}"; do
        echo "CMakeLists.txt:1: error: $item"
    done
    exit 1
fi

echo "[OK] Dependencias JUCE + Rust verificadas."
