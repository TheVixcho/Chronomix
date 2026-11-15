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

resolve_download_url() {
    local asset_name="$1"
    if [ "${VERSION}" = "latest" ]; then
        echo "https://github.com/${REPO}/releases/latest/download/${asset_name}"
    else
        echo "https://github.com/${REPO}/releases/download/${VERSION}/${asset_name}"
    fi
}

install_linux() {
    local bin_dir="${HOME}/.local/bin"
    local apps_dir="${HOME}/.local/share/applications"
    mkdir -p "${bin_dir}" "${apps_dir}"

    if [ -f "./target/release/${APP_NAME}" ]; then
        echo "-> Usando binario compilado localmente..."
        cp "./target/release/${APP_NAME}" "${bin_dir}/${APP_NAME}"
    else
        local asset="Chronomix-Linux-x86_64.tar.gz"
        local url
        url="$(resolve_download_url "${asset}")"
        echo "-> Descargando ${url}..."
        if curl -fsSL "${url}" -o "${TMP_DIR}/${asset}"; then
            tar -xzf "${TMP_DIR}/${asset}" -C "${TMP_DIR}"
            cp "${TMP_DIR}/${APP_NAME}" "${bin_dir}/${APP_NAME}"
        else
            echo "-> Compilando desde el código fuente con Cargo..."
            git clone --depth 1 "https://github.com/${REPO}.git" "${TMP_DIR}/src"
            (cd "${TMP_DIR}/src" && cargo build --release --bin "${APP_NAME}")
            cp "${TMP_DIR}/src/target/release/${APP_NAME}" "${bin_dir}/${APP_NAME}"
        fi
    fi

    chmod +x "${bin_dir}/${APP_NAME}"

    local desktop_file="${apps_dir}/chronomix.desktop"
    cat > "${desktop_file}" <<EOF
[Desktop Entry]
Type=Application
Name=Chronomix
Comment=Studio Audio DAW & VST2/VST3 Host by TheVixcho
Exec=${bin_dir}/${APP_NAME}
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
    echo "  Ejecuta 'chronomix' o ábrelo desde tu Escritorio."
    echo "========================================================"
}

install_macos() {
    local app_bundle="${HOME}/Applications/${DISPLAY_NAME}.app"
    local macos_bin_dir="${app_bundle}/Contents/MacOS"
    mkdir -p "${macos_bin_dir}" "${HOME}/.local/bin"

    if [ -f "./target/release/${APP_NAME}" ]; then
        cp "./target/release/${APP_NAME}" "${macos_bin_dir}/${APP_NAME}"
    else
        local asset="Chronomix-macOS.tar.gz"
        local url
        url="$(resolve_download_url "${asset}")"
        echo "-> Descargando ${url}..."
        if curl -fsSL "${url}" -o "${TMP_DIR}/${asset}"; then
            tar -xzf "${TMP_DIR}/${asset}" -C "${TMP_DIR}"
            if [ -d "${TMP_DIR}/${DISPLAY_NAME}.app" ]; then
                rm -rf "${app_bundle}"
                mv "${TMP_DIR}/${DISPLAY_NAME}.app" "${app_bundle}"
            else
                cp "${TMP_DIR}/${APP_NAME}" "${macos_bin_dir}/${APP_NAME}"
            fi
        else
            echo "-> Compilando desde el código fuente con Cargo..."
            git clone --depth 1 "https://github.com/${REPO}.git" "${TMP_DIR}/src"
            (cd "${TMP_DIR}/src" && cargo build --release --bin "${APP_NAME}")
            cp "${TMP_DIR}/src/target/release/${APP_NAME}" "${macos_bin_dir}/${APP_NAME}"
        fi
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
        echo "Sistema operativo no soportado por install.sh (${OS}). En Windows usa el instalador .exe."
        exit 1
        ;;
esac
