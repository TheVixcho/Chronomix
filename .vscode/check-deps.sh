#!/usr/bin/env bash
set -u

# Load rustup environment if present so ~/.cargo/bin takes precedence over distro packages.
if [ -f "${HOME:-}/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "${HOME}/.cargo/env"
fi

MIN_RUST_MAJOR=1
MIN_RUST_MINOR=85

MISSING_ITEMS=()
APT_PACKAGES=()
APT_SEEN=""
NEED_RUSTUP=0
REMOVE_APT_RUST=0
WARNINGS=()

add_apt_pkg() {
    local pkg="$1"
    case " $APT_SEEN " in
        *" $pkg "*) ;;
        *)
            APT_SEEN="$APT_SEEN $pkg"
            APT_PACKAGES+=("$pkg")
            ;;
    esac
}

add_missing() {
    local msg="$1"
    MISSING_ITEMS+=("$msg")
}

check_rustc_version() {
    local ver_line ver major minor
    ver_line="$(rustc --version 2>/dev/null || true)"
    ver="${ver_line#rustc }"
    ver="${ver%% *}"
    major="${ver%%.*}"
    minor="${ver#*.}"
    minor="${minor%%.*}"

    if ! [[ "$major" =~ ^[0-9]+$ && "$minor" =~ ^[0-9]+$ ]]; then
        add_missing "No se pudo leer la versión de rustc ('$ver_line'). Se requiere Rust >= ${MIN_RUST_MAJOR}.${MIN_RUST_MINOR}."
        NEED_RUSTUP=1
        return
    fi

    if (( major < MIN_RUST_MAJOR || (major == MIN_RUST_MAJOR && minor < MIN_RUST_MINOR) )); then
        local rustc_path
        rustc_path="$(command -v rustc)"
        add_missing "Rust ${ver} en ${rustc_path} es demasiado antiguo (Cargo.toml usa edition = 2024; requiere >= ${MIN_RUST_MAJOR}.${MIN_RUST_MINOR})."
        NEED_RUSTUP=1
        if [[ "$rustc_path" == "/usr/bin/rustc" ]]; then
            REMOVE_APT_RUST=1
        fi
    fi
}

check_libclang() {
    if [ -n "${LIBCLANG_PATH:-}" ]; then
        if [ -f "$LIBCLANG_PATH" ] || compgen -G "${LIBCLANG_PATH%/}/libclang*.so*" >/dev/null 2>&1; then
            return 0
        fi
    fi

    if command -v ldconfig >/dev/null 2>&1 && ldconfig -p 2>/dev/null | grep -q 'libclang'; then
        return 0
    fi

    local patterns=(
        "/usr/lib/llvm-*/lib/libclang*.so*"
        "/usr/lib/*/libclang*.so*"
        "/usr/lib/libclang*.so*"
        "/usr/local/lib/libclang*.so*"
    )
    local pattern
    for pattern in "${patterns[@]}"; do
        if compgen -G "$pattern" >/dev/null 2>&1; then
            return 0
        fi
    done

    return 1
}

# 1. Rust toolchain
if ! command -v cargo >/dev/null 2>&1 || ! command -v rustc >/dev/null 2>&1; then
    add_missing "Falta Rust/Cargo (>= ${MIN_RUST_MAJOR}.${MIN_RUST_MINOR}, necesario para edition = 2024)."
    NEED_RUSTUP=1
else
    check_rustc_version
fi

# 2. C compiler and pkg-config
if ! command -v cc >/dev/null 2>&1 && ! command -v gcc >/dev/null 2>&1; then
    add_missing "Falta compilador C (cc/gcc)."
    add_apt_pkg "build-essential"
fi

if ! command -v pkg-config >/dev/null 2>&1; then
    add_missing "Falta pkg-config."
    add_apt_pkg "pkg-config"
fi

# 3. libclang (needed by bindgen for libspa-sys / pipewire-sys)
if ! check_libclang; then
    add_missing "Falta libclang (requerido por bindgen para PipeWire/libspa-sys)."
    add_apt_pkg "libclang-dev"
fi

# 4. System libraries via pkg-config
if command -v pkg-config >/dev/null 2>&1; then
    PKG_SPECS=(
        "alsa|libasound2-dev|ALSA audio"
        "libpipewire-0.3|libpipewire-0.3-dev|PipeWire"
        "libspa-0.2|libpipewire-0.3-dev|SPA (PipeWire)"
        "libpulse|libpulse-dev|PulseAudio"
        "gtk+-3.0|libgtk-3-dev|GTK3 (diálogos de archivo rfd)"
        "libudev|libudev-dev|libudev"
    )

    for spec in "${PKG_SPECS[@]}"; do
        IFS='|' read -r mod pkg label <<< "$spec"
        if ! pkg-config --exists "$mod" 2>/dev/null; then
            add_missing "Falta librería de desarrollo ${label} (pkg-config: ${mod})."
            add_apt_pkg "$pkg"
        fi
    done

    if ! pkg-config --exists x11 2>/dev/null && ! pkg-config --exists wayland-client 2>/dev/null; then
        add_missing "Falta librería de servidor gráfico (X11 o Wayland dev)."
        add_apt_pkg "libx11-dev"
    fi

    if ! pkg-config --exists gl 2>/dev/null && ! pkg-config --exists egl 2>/dev/null; then
        add_missing "Falta librería de desarrollo OpenGL/EGL (gl o egl)."
        add_apt_pkg "libgl-dev"
    fi
fi

# 5. Non-blocking advisories
if [ -z "${DISPLAY:-}" ] && [ -z "${WAYLAND_DISPLAY:-}" ]; then
    WARNINGS+=("No se detectó DISPLAY ni WAYLAND_DISPLAY en el entorno; la ventana gráfica podría no abrirse.")
fi

HAS_WINE=0
for w in wine wine64 /opt/wine-staging/bin/wine /opt/wine-devel/bin/wine /opt/wine-stable/bin/wine; do
    if command -v "$w" >/dev/null 2>&1 || [ -x "$w" ]; then
        HAS_WINE=1
        break
    fi
done

HAS_WINEGPP=0
for g in wineg++ /opt/wine-staging/bin/wineg++ /opt/wine-devel/bin/wineg++ /opt/wine-stable/bin/wineg++; do
    if command -v "$g" >/dev/null 2>&1 || [ -x "$g" ]; then
        HAS_WINEGPP=1
        break
    fi
done

if [ "$HAS_WINE" -eq 0 ] || [ "$HAS_WINEGPP" -eq 0 ]; then
    WARNINGS+=("Falta wine/wineg++ (opcional): Chronomix funcionará con plugins nativos, y si instalas wine+wineg++ después podrás compilar el bridge e inicializar prefijos desde Ajustes.")
fi

if [ "${#WARNINGS[@]}" -gt 0 ]; then
    for w in "${WARNINGS[@]}"; do
        echo "[AVISO] $w"
    done
fi

if [ "${#MISSING_ITEMS[@]}" -gt 0 ]; then
    echo ""
    echo "========================================================================"
    echo " ADVERTENCIA: Faltan requisitos necesarios para compilar/ejecutar Chronomix"
    echo "========================================================================"
    for item in "${MISSING_ITEMS[@]}"; do
        echo "Cargo.toml:1: error: $item"
    done
    echo ""
    echo "Cómo solucionarlo:"
    if [ "$REMOVE_APT_RUST" -eq 1 ]; then
        echo "  1) Quitar Rust/Cargo antiguo de apt:"
        echo "     sudo apt remove -y cargo rustc"
    fi
    if [ "$NEED_RUSTUP" -eq 1 ]; then
        echo "  -> Instalar/actualizar Rust con rustup:"
        echo "     curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
        echo "     . \"\$HOME/.cargo/env\""
    fi
    if [ "${#APT_PACKAGES[@]}" -gt 0 ]; then
        echo "  -> Instalar paquetes de sistema faltantes (Debian/Ubuntu/Mint):"
        echo "     sudo apt update && sudo apt install -y ${APT_PACKAGES[*]}"
    fi
    echo "========================================================================"
    exit 1
fi

echo "[OK] Todos los requisitos obligatorios están instalados ($(rustc --version))."
