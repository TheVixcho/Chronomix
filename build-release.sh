#!/usr/bin/env bash
set -euo pipefail

VER=$(grep -m1 '^version =' Cargo.toml | cut -d'"' -f2)
ARCH="$(uname -m)"
rm -rf dist
mkdir -p dist

echo "========================================================"
echo "  Empaquetando Chronomix v${VER} (JUCE + Rust)"
echo "========================================================"

echo "[1/2] Compilando Chronomix (JUCE + Rust) Release para Linux (${ARCH})..."
GEN_ARGS=()
if command -v ninja >/dev/null 2>&1; then
    GEN_ARGS=(-G Ninja)
fi

cmake -S . -B build "${GEN_ARGS[@]}" -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel

BIN_PATH=""
for candidate in \
    "build/chronomix_artefacts/Release/Chronomix" \
    "build/chronomix_artefacts/Release/chronomix" \
    "build/chronomix_artefacts/Chronomix" \
    "build/chronomix_artefacts/chronomix"; do
    if [ -f "${candidate}" ]; then
        BIN_PATH="${candidate}"
        break
    fi
done

if [ -z "${BIN_PATH}" ]; then
    echo "Error: no se encontró el binario compilado en build/chronomix_artefacts/."
    exit 1
fi

STAGE="/tmp/chronomix-dist-stage"
rm -rf "${STAGE}"
mkdir -p "${STAGE}"
cp "${BIN_PATH}" "${STAGE}/chronomix"
chmod +x "${STAGE}/chronomix"
cp LICENSE "${STAGE}/LICENSE"
cp README.md "${STAGE}/README.md"

ASSET_NAME="Chronomix-Linux-${ARCH}.tar.gz"
tar -czf "dist/${ASSET_NAME}" -C "${STAGE}" chronomix LICENSE README.md
rm -rf "${STAGE}"
echo "  -> Listo: dist/${ASSET_NAME}"

echo "[2/2] Comprobando compilación Windows..."
if command -v x86_64-w64-mingw32-g++ >/dev/null 2>&1; then
    echo "  -> MinGW-w64 C++ detectado; usa installer/build-windows.ps1 en Windows o configura toolchain MinGW para generar el .exe JUCE + Rust."
else
    echo "  -> Compilación cruzada JUCE C++ para Windows omitida en este equipo (requiere MSVC en Windows mediante installer/build-windows.ps1 o mingw-w64-gcc)."
fi

echo "========================================================"
echo "  Archivos finales en ./dist/ :"
ls -lh dist/
echo "========================================================"
