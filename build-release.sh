#!/usr/bin/env bash
set -euo pipefail

VER=$(grep -m1 '^version =' Cargo.toml | cut -d'"' -f2)
mkdir -p dist

echo "========================================================"
echo "  Empaquetando Chronomix v${VER} (Local Release Builder)"
echo "========================================================"

echo "[1/2] Compilando binario de producción para Linux (x86_64)..."
cargo build --release --bin chronomix

tar -czf "dist/Chronomix-Linux-x86_64.tar.gz" -C target/release chronomix -C ../.. LICENSE README.md
cp "dist/Chronomix-Linux-x86_64.tar.gz" "dist/Chronomix-v${VER}-Linux-x86_64.tar.gz"
cp install.sh "dist/install.sh"
echo "  -> Listo: dist/Chronomix-v${VER}-Linux-x86_64.tar.gz"

if command -v x86_64-w64-mingw32-gcc >/dev/null 2>&1; then
    echo "[2/2] Compilando versión Windows x64 e Instalador .exe..."
    rustup target add x86_64-pc-windows-gnu >/dev/null 2>&1 || true
    cargo build --release --target x86_64-pc-windows-gnu --bin chronomix

    WIN_BIN="target/x86_64-pc-windows-gnu/release/chronomix.exe"
    ISCC_PATH="${HOME}/.wine/drive_c/Program Files (x86)/Inno Setup 6/ISCC.exe"

    if [ -f "${ISCC_PATH}" ] && command -v wine >/dev/null 2>&1; then
        mkdir -p target/release
        cp "${WIN_BIN}" "target/release/chronomix.exe"
        wine "${ISCC_PATH}" "/DMyAppVersion=${VER}" "installer\\windows\\chronomix.iss"
        cp "dist/Chronomix-Setup-v${VER}-Windows-x64.exe" "dist/Chronomix-Setup-Windows-x64.exe"
    else
        TMP_BUILD="$(mktemp -d)"
        cp "${WIN_BIN}" "${TMP_BUILD}/payload_chronomix.exe"
        cp LICENSE "${TMP_BUILD}/payload_LICENSE.txt"
        cp README.md "${TMP_BUILD}/payload_README.md"

        cat > "${TMP_BUILD}/resources.rc" <<EOF
101 RCDATA "payload_chronomix.exe"
102 RCDATA "payload_LICENSE.txt"
103 RCDATA "payload_README.md"
EOF

        cat > "${TMP_BUILD}/setup.c" <<EOF
#include <windows.h>
#include <shlobj.h>
#include <stdio.h>

static int extract_resource(int res_id, const char *out_path) {
    HRSRC hRes = FindResourceA(NULL, MAKEINTRESOURCEA(res_id), RT_RCDATA);
    if (!hRes) return 0;
    HGLOBAL hData = LoadResource(NULL, hRes);
    if (!hData) return 0;
    DWORD size = SizeofResource(NULL, hRes);
    const void *ptr = LockResource(hData);
    if (!ptr) return 0;
    FILE *f = fopen(out_path, "wb");
    if (!f) return 0;
    fwrite(ptr, 1, size, f);
    fclose(f);
    return 1;
}

int WINAPI WinMain(HINSTANCE hInst, HINSTANCE hPrev, LPSTR lpCmd, int nShow) {
    const char *lic_msg =
        "Chronomix Studio Audio DAW v${VER}\n"
        "Creado por: TheVixcho\n"
        "Licencia: MIT License\n\n"
        "Permission is hereby granted, free of charge, to any person obtaining a copy "
        "of this software and associated documentation files (the \"Software\"), to deal "
        "in the Software without restriction (MIT License - Copyright (c) 2026 TheVixcho).\n\n"
        "Presiona Aceptar para instalar Chronomix en tu sistema.";

    if (MessageBoxA(NULL, lic_msg, "Instalador de Chronomix v${VER} - Licencia MIT", MB_OKCANCEL | MB_ICONINFORMATION) != IDOK) {
        return 0;
    }

    char local_appdata[MAX_PATH];
    if (!GetEnvironmentVariableA("LOCALAPPDATA", local_appdata, MAX_PATH)) {
        return 1;
    }

    char install_dir[MAX_PATH];
    snprintf(install_dir, MAX_PATH, "%s\\Programs", local_appdata);
    CreateDirectoryA(install_dir, NULL);
    snprintf(install_dir, MAX_PATH, "%s\\Programs\\Chronomix", local_appdata);
    CreateDirectoryA(install_dir, NULL);

    char vst_dir[MAX_PATH];
    snprintf(vst_dir, MAX_PATH, "%s\\vst_plugins", install_dir);
    CreateDirectoryA(vst_dir, NULL);

    char exe_path[MAX_PATH], lic_path[MAX_PATH], readme_path[MAX_PATH];
    snprintf(exe_path, MAX_PATH, "%s\\chronomix.exe", install_dir);
    snprintf(lic_path, MAX_PATH, "%s\\LICENSE.txt", install_dir);
    snprintf(readme_path, MAX_PATH, "%s\\README.md", install_dir);

    if (!extract_resource(101, exe_path)) {
        MessageBoxA(NULL, "No se pudo extraer chronomix.exe.", "Error", MB_OK | MB_ICONERROR);
        return 1;
    }
    extract_resource(102, lic_path);
    extract_resource(103, readme_path);

    if (MessageBoxA(NULL, "¿Deseas crear un acceso directo de Chronomix en el Escritorio?", "Acceso Directo", MB_YESNO | MB_ICONQUESTION) == IDYES) {
        char ps_cmd[1024];
        snprintf(
            ps_cmd,
            sizeof(ps_cmd),
            "powershell -NoProfile -WindowStyle Hidden -Command \"$s=(New-Object -COM WScript.Shell).CreateShortcut([Environment]::GetFolderPath('Desktop')+'\\\\Chronomix.lnk');$s.TargetPath='%s';$s.WorkingDirectory='%s';$s.Save()\"",
            exe_path,
            install_dir
        );
        WinExec(ps_cmd, SW_HIDE);
    }

    if (MessageBoxA(NULL, "Chronomix v${VER} se ha instalado correctamente.\n\n¿Deseas iniciar Chronomix ahora?", "Instalación Completada", MB_YESNO | MB_ICONINFORMATION) == IDYES) {
        ShellExecuteA(NULL, "open", exe_path, NULL, install_dir, SW_SHOWNORMAL);
    }
    return 0;
}
EOF

        x86_64-w64-mingw32-windres "${TMP_BUILD}/resources.rc" -O coff -o "${TMP_BUILD}/resources.o"
        x86_64-w64-mingw32-gcc -O2 -mwindows "${TMP_BUILD}/setup.c" "${TMP_BUILD}/resources.o" -lshell32 -lole32 -o "dist/Chronomix-Setup-v${VER}-Windows-x64.exe"
        cp "dist/Chronomix-Setup-v${VER}-Windows-x64.exe" "dist/Chronomix-Setup-Windows-x64.exe"
        rm -rf "${TMP_BUILD}"
    fi
    echo "  -> Instalador Windows creado: dist/Chronomix-Setup-v${VER}-Windows-x64.exe"
else
    echo "[2/2] Para generar también el instalador .exe de Windows localmente desde tu PC (sin GitHub Actions):"
    echo "      sudo pacman -S mingw-w64-gcc && rustup target add x86_64-pc-windows-gnu"
    echo "      ./build-release.sh"
fi

echo "========================================================"
echo "  Archivos generados en ./dist/ :"
ls -lh dist/
echo "========================================================"
