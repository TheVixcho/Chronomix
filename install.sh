#!/usr/bin/env bash
set -euo pipefail

REPO="TheVixcho/chronomix"
APP_NAME="chronomix"
DISPLAY_NAME="Chronomix"
VERSION="${CHRONOMIX_VERSION:-latest}"

OS="$(uname -s)"
ARCH="$(uname -m)"

echo "========================================================"
echo "  Instalador de Chronomix Studio DAW (MIT - TheVixcho)"
echo "  Sistema detectado: ${OS} (${ARCH})"
echo "========================================================"

TMP_DIR="$(mktemp -d)"
cleanup() {
    rm -rf "${TMP_DIR}"
}
trap cleanup EXIT

resolve_asset_url_via_api() {
    local asset_name="$1"
    local api_url="https://api.github.com/repos/${REPO}/releases"
    local json=""
    if command -v curl >/dev/null 2>&1; then
        json="$(curl -fsSL -H "Accept: application/vnd.github+json" -H "User-Agent: chronomix-installer" "${api_url}" 2>/dev/null || true)"
    elif command -v wget >/dev/null 2>&1; then
        json="$(wget -qO- --header="Accept: application/vnd.github+json" "${api_url}" 2>/dev/null || true)"
    fi
    if [ -z "${json}" ]; then
        return 1
    fi
    if command -v python3 >/dev/null 2>&1; then
        printf '%s' "${json}" | python3 -c '
import json, sys
target = sys.argv[1]
try:
    releases = json.load(sys.stdin)
except Exception:
    sys.exit(1)
if not isinstance(releases, list):
    sys.exit(1)
for rel in releases:
    if rel.get("draft"):
        continue
    for asset in rel.get("assets", []):
        if asset.get("name") == target and asset.get("browser_download_url"):
            print(asset["browser_download_url"])
            sys.exit(0)
sys.exit(1)
' "${asset_name}" && return 0
    fi
    local matched
    matched="$(printf '%s\n' "${json}" | grep -oE "\"browser_download_url\"[[:space:]]*:[[:space:]]*\"[^\"]*/${asset_name}\"" | head -n1 | sed -E 's/.*"([^"]+)"$/\1/' || true)"
    if [ -n "${matched}" ]; then
        printf '%s\n' "${matched}"
        return 0
    fi
    return 1
}

resolve_download_url() {
    local asset_name="$1"
    if [ "${VERSION}" != "latest" ]; then
        printf 'https://github.com/%s/releases/download/%s/%s\n' "${REPO}" "${VERSION}" "${asset_name}"
        return 0
    fi
    if resolve_asset_url_via_api "${asset_name}"; then
        return 0
    fi
    return 1
}

check_source_build_prereqs() {
    local missing=()
    command -v git >/dev/null 2>&1 || missing+=("git")
    command -v cargo >/dev/null 2>&1 || missing+=("cargo / rustc (https://rustup.rs)")
    command -v cmake >/dev/null 2>&1 || missing+=("cmake (>= 3.22)")
    if ! command -v g++ >/dev/null 2>&1 && ! command -v clang++ >/dev/null 2>&1; then
        missing+=("compilador C++20 (g++ o clang++)")
    fi
    if [ "${#missing[@]}" -gt 0 ]; then
        echo ""
        echo "========================================================"
        echo "  No se encontró binario precompilado compatible para ${OS} (${ARCH})"
        echo "  y faltan herramientas para compilar desde código fuente:"
        for m in "${missing[@]}"; do
            echo "    - ${m}"
        done
        echo ""
        if [ "${OS}" = "Darwin" ]; then
            echo "  En macOS instala Xcode Command Line Tools, CMake y Rust:"
            echo "    xcode-select --install"
            echo "    brew install cmake"
            echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        else
            echo "  En Debian/Ubuntu:"
            echo "    sudo apt install -y git cmake ninja-build g++ pkg-config libasound2-dev libpipewire-0.3-dev libpulse-dev libfreetype-dev libx11-dev libxrandr-dev libxinerama-dev libxcursor-dev libclang-dev"
            echo "  En Arch Linux:"
            echo "    sudo pacman -S git cmake ninja gcc pkgconf alsa-lib pipewire libpulse freetype2 libx11 libxrandr libxinerama libxcursor clang"
            echo "  Y Rust:"
            echo "    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        fi
        echo "========================================================"
        exit 1
    fi
}

build_from_source() {
    local out_bin="$1"
    check_source_build_prereqs
    echo "-> Compilando Chronomix (JUCE + Rust) desde el código fuente..."
    git clone --depth 1 "https://github.com/${REPO}.git" "${TMP_DIR}/src"
    local gen_args=()
    if command -v ninja >/dev/null 2>&1; then
        gen_args=(-G Ninja)
    fi
    cmake -S "${TMP_DIR}/src" -B "${TMP_DIR}/build" "${gen_args[@]}" -DCMAKE_BUILD_TYPE=Release
    cmake --build "${TMP_DIR}/build" --parallel
    local built=""
    for candidate in \
        "${TMP_DIR}/build/chronomix_artefacts/Release/Chronomix" \
        "${TMP_DIR}/build/chronomix_artefacts/Release/chronomix" \
        "${TMP_DIR}/build/chronomix_artefacts/Chronomix" \
        "${TMP_DIR}/build/chronomix_artefacts/chronomix" \
        "${TMP_DIR}/build/chronomix_artefacts/Release/Chronomix.app/Contents/MacOS/Chronomix"; do
        if [ -f "${candidate}" ]; then
            built="${candidate}"
            break
        fi
    done
    if [ -z "${built}" ]; then
        echo "Error: compilación completada pero no se encontró el binario final."
        exit 1
    fi
    cp "${built}" "${out_bin}"
}

install_linux() {
    local bin_dir="${HOME}/.local/bin"
    local apps_dir="${HOME}/.local/share/applications"
    local data_dir="${XDG_DATA_HOME:-${HOME}/.local/share}/chronomix"
    local cfg_dir="${XDG_CONFIG_HOME:-${HOME}/.config}/chronomix"
    mkdir -p "${bin_dir}" "${apps_dir}" "${data_dir}/vst_plugins" "${data_dir}/recordings" "${data_dir}/profiles" "${cfg_dir}"

    local local_built=""
    for c in \
        "./build/chronomix_artefacts/Release/Chronomix" \
        "./build/chronomix_artefacts/Release/chronomix" \
        "./build/chronomix_artefacts/Chronomix" \
        "./build/chronomix_artefacts/chronomix"; do
        if [ -f "${c}" ]; then
            local_built="${c}"
            break
        fi
    done

    if [ -n "${local_built}" ]; then
        echo "-> Usando binario JUCE + Rust compilado localmente (${local_built})..."
        cp "${local_built}" "${bin_dir}/${APP_NAME}"
    else
        local asset="Chronomix-Linux-x86_64.tar.gz"
        if [ "${ARCH}" = "aarch64" ] || [ "${ARCH}" = "arm64" ]; then
            asset="Chronomix-Linux-ARM64.tar.gz"
        fi
        local url=""
        if url="$(resolve_download_url "${asset}")" && [ -n "${url}" ]; then
            echo "-> Descargando ${url}..."
            if curl -fsSL "${url}" -o "${TMP_DIR}/${asset}"; then
                tar -xzf "${TMP_DIR}/${asset}" -C "${TMP_DIR}"
                if [ -f "${TMP_DIR}/${APP_NAME}" ]; then
                    cp "${TMP_DIR}/${APP_NAME}" "${bin_dir}/${APP_NAME}"
                elif [ -f "${TMP_DIR}/${DISPLAY_NAME}" ]; then
                    cp "${TMP_DIR}/${DISPLAY_NAME}" "${bin_dir}/${APP_NAME}"
                else
                    build_from_source "${bin_dir}/${APP_NAME}"
                fi
            else
                echo "-> Descarga fallida; intentando compilar desde código fuente..."
                build_from_source "${bin_dir}/${APP_NAME}"
            fi
        else
            echo "-> No se encontró el paquete ${asset} en GitHub Releases; intentando compilar desde código fuente..."
            build_from_source "${bin_dir}/${APP_NAME}"
        fi
    fi

    chmod +x "${bin_dir}/${APP_NAME}"

    local desktop_file="${apps_dir}/chronomix.desktop"
    cat > "${desktop_file}" <<EOF
[Desktop Entry]
Type=Application
Name=Chronomix
Comment=Studio Audio DAW, DJ & VST2/VST3 Host by TheVixcho
Exec=${bin_dir}/${APP_NAME}
Path=${data_dir}
Terminal=false
Categories=AudioVideo;Audio;Music;
StartupNotify=true
EOF
    chmod +x "${desktop_file}"

    for desk in "${HOME}/Desktop" "${HOME}/Escritorio"; do
        if [ -d "${desk}" ]; then
            cp "${desktop_file}" "${desk}/Chronomix.desktop"
            chmod +x "${desk}/Chronomix.desktop"
            echo "-> Acceso directo creado en ${desk}/Chronomix.desktop"
        fi
    done

    echo "========================================================"
    echo "  Chronomix instalado en: ${bin_dir}/${APP_NAME}"
    echo "  Ajustes: ${cfg_dir}"
    echo "  Datos/VST/Grabaciones: ${data_dir}"
    echo "  Ejecuta 'chronomix' o ábrelo desde tu Escritorio."
    echo "========================================================"
}

install_macos() {
    local app_bundle="${HOME}/Applications/${DISPLAY_NAME}.app"
    local macos_bin_dir="${app_bundle}/Contents/MacOS"
    mkdir -p "${macos_bin_dir}" "${HOME}/.local/bin"

    local asset="Chronomix-macOS.tar.gz"
    local url=""
    if url="$(resolve_download_url "${asset}")" && [ -n "${url}" ] && curl -fsSL "${url}" -o "${TMP_DIR}/${asset}"; then
        tar -xzf "${TMP_DIR}/${asset}" -C "${TMP_DIR}"
        if [ -d "${TMP_DIR}/${DISPLAY_NAME}.app" ]; then
            rm -rf "${app_bundle}"
            mv "${TMP_DIR}/${DISPLAY_NAME}.app" "${app_bundle}"
        elif [ -f "${TMP_DIR}/${APP_NAME}" ]; then
            cp "${TMP_DIR}/${APP_NAME}" "${macos_bin_dir}/${APP_NAME}"
        else
            build_from_source "${macos_bin_dir}/${APP_NAME}"
        fi
    else
        build_from_source "${macos_bin_dir}/${APP_NAME}"
    fi

    chmod +x "${macos_bin_dir}/${APP_NAME}"
    ln -sf "${macos_bin_dir}/${APP_NAME}" "${HOME}/.local/bin/${APP_NAME}"

    if [ ! -f "${app_bundle}/Contents/Info.plist" ]; then
        cat > "${app_bundle}/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleExecutable</key>
    <string>chronomix</string>
    <key>CFBundleIdentifier</key>
    <string>io.github.thevixcho.chronomix</string>
    <key>CFBundleName</key>
    <string>Chronomix</string>
    <key>CFBundleVersion</key>
    <string>0.0.1</string>
    <key>CFBundleShortVersionString</key>
    <string>0.0.1</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>NSHighResolutionCapable</key>
    <true/>
</dict>
</plist>
EOF
    fi

    if [ -d "${HOME}/Desktop" ]; then
        ln -sf "${app_bundle}" "${HOME}/Desktop/${DISPLAY_NAME}.app"
    fi

    echo "========================================================"
    echo "  Chronomix instalado en: ${app_bundle}"
    echo "========================================================"
}

case "${OS}" in
    Linux*)
        install_linux
        ;;
    Darwin*)
        install_macos
        ;;
    *)
        echo "Sistema operativo no soportado por install.sh (${OS}). En Windows usa el instalador .exe o installer/build-windows.ps1."
        exit 1
        ;;
esac
