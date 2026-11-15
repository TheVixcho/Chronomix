#!/usr/bin/env bash
set -euo pipefail

VER=$(grep -m1 '^version =' Cargo.toml | cut -d'"' -f2)
rm -rf dist
mkdir -p dist

echo "========================================================"
echo "  Empaquetando Chronomix v${VER} (Estándares Oficiales)"
echo "========================================================"

echo "[1/3] Compilando Linux x86_64..."
cargo build --release --bin chronomix
tar -czf "dist/Chronomix-Linux-x86_64.tar.gz" -C target/release chronomix -C ../.. LICENSE README.md
echo "  -> Listo: dist/Chronomix-Linux-x86_64.tar.gz"

TMP_BUILD="/tmp/chronomix-mingw-build"
mkdir -p "${TMP_BUILD}"

cat > "${TMP_BUILD}/setup.c" <<'EOF'
#include <windows.h>
#include <stdio.h>

static int extract_resource(int res_id, const char *out_path) {
    HRSRC hRes = FindResourceA(NULL, MAKEINTRESOURCEA(res_id), (LPCSTR)10);
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
        "Chronomix Studio Audio DAW v" APP_VER "\n"
        "Creado por: TheVixcho\n"
        "Licencia: MIT License\n\n"
        "Permission is hereby granted, free of charge, to any person obtaining a copy "
        "of this software and associated documentation files (the \"Software\"), to deal "
        "in the Software without restriction (MIT License - Copyright (c) 2026 TheVixcho).\n\n"
        "Presiona Aceptar para instalar Chronomix en tu sistema.";

    if (MessageBoxA(NULL, lic_msg, "Instalador de Chronomix v" APP_VER " - Licencia MIT", MB_OKCANCEL | MB_ICONINFORMATION) != IDOK) {
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

    if (MessageBoxA(NULL, "Deseas crear un acceso directo de Chronomix en el Escritorio y Menu Inicio?", "Acceso Directo", MB_YESNO | MB_ICONQUESTION) == IDYES) {
        char ps_cmd[2048];
        snprintf(
            ps_cmd,
            sizeof(ps_cmd),
            "powershell -NoProfile -WindowStyle Hidden -Command \"$w=New-Object -COM WScript.Shell;$s=$w.CreateShortcut([Environment]::GetFolderPath('Desktop')+'\\Chronomix.lnk');$s.TargetPath='%s';$s.WorkingDirectory='%s';$s.Description='Chronomix Studio Audio DAW by TheVixcho';$s.Save();$m=$w.CreateShortcut([Environment]::GetFolderPath('StartMenu')+'\\Programs\\Chronomix.lnk');$m.TargetPath='%s';$m.WorkingDirectory='%s';$m.Description='Chronomix Studio Audio DAW by TheVixcho';$m.Save()\"",
            exe_path,
            install_dir,
            exe_path,
            install_dir
        );
        WinExec(ps_cmd, SW_HIDE);
    }

    if (MessageBoxA(NULL, "Chronomix v" APP_VER " se ha instalado correctamente.\n\nDeseas iniciar Chronomix ahora?", "Instalacion Completada", MB_YESNO | MB_ICONINFORMATION) == IDYES) {
        ShellExecuteA(NULL, "open", exe_path, NULL, install_dir, SW_SHOWNORMAL);
    }
    return 0;
}
EOF

cat > "${TMP_BUILD}/crt2.c" <<'EOF'
extern int main(int argc, char **argv, char **envp);
extern void exit(int status);
void __main(void) {}
int _setargv(void) { return 0; }
int mainCRTStartup(void) {
    char *empty_argv[2] = { "chronomix.exe", 0 };
    int ret = main(1, empty_argv, 0);
    exit(ret);
    return ret;
}
int WinMainCRTStartup(void) { return mainCRTStartup(); }
EOF

cat > "${TMP_BUILD}/unwind_shim.c" <<'EOF'
#include <stdint.h>
extern void abort(void);
extern double sqrt(double);
uint32_t _tls_index = 0;
typedef int _Unwind_Reason_Code;
typedef int _Unwind_Action;
struct _Unwind_Context;
struct _Unwind_Exception;
_Unwind_Reason_Code _Unwind_Resume(struct _Unwind_Exception *e) { abort(); return 0; }
_Unwind_Reason_Code _Unwind_RaiseException(struct _Unwind_Exception *e) { abort(); return 0; }
uintptr_t _Unwind_GetIP(struct _Unwind_Context *c) { return 0; }
uintptr_t _Unwind_GetIPInfo(struct _Unwind_Context *c, int *ip_before_insn) { if (ip_before_insn) *ip_before_insn = 0; return 0; }
void _Unwind_SetIP(struct _Unwind_Context *c, uintptr_t v) {}
void _Unwind_SetGR(struct _Unwind_Context *c, int i, uintptr_t v) {}
uintptr_t _Unwind_GetLanguageSpecificData(struct _Unwind_Context *c) { return 0; }
uintptr_t _Unwind_GetRegionStart(struct _Unwind_Context *c) { return 0; }
uintptr_t _Unwind_GetDataRelBase(struct _Unwind_Context *c) { return 0; }
uintptr_t _Unwind_GetTextRelBase(struct _Unwind_Context *c) { return 0; }
void _Unwind_DeleteException(struct _Unwind_Exception *e) {}
typedef _Unwind_Reason_Code (*_Unwind_Trace_Fn)(struct _Unwind_Context *, void *);
_Unwind_Reason_Code _Unwind_Backtrace(_Unwind_Trace_Fn f, void *a) { return 0; }
_Unwind_Reason_Code __gcc_personality_seh0(int v, _Unwind_Action a, uint64_t c, struct _Unwind_Exception *e, struct _Unwind_Context *ctx) { return 0; }
_Unwind_Reason_Code _GCC_specific_handler(void *a, void *b, void *c, void *d, void *e) { return 0; }
float hypotf(float x, float y) { return (float)sqrt((double)x * (double)x + (double)y * (double)y); }
EOF

echo "[2/3] Compilando Instalador Windows x64 (.exe)..."
MINGW_BIN="${TMP_BUILD}/mingw-bin"
MINGW_LIBS="${TMP_BUILD}/mingw-libs"
mkdir -p "${MINGW_BIN}" "${MINGW_LIBS}"

for tool in dlltool ar ranlib windres strip nm objcopy; do
    ln -sf "/usr/bin/llvm-${tool}" "${MINGW_BIN}/x86_64-w64-mingw32-${tool}"
done

cat > "${MINGW_BIN}/x86_64-w64-mingw32-gcc" <<EOF
#!/usr/bin/env bash
filtered=()
for arg in "\$@"; do
    case "\$arg" in
        *rsbegin.o|*rsend.o|crtbegin.o|crtend.o) ;;
        *) filtered+=("\$arg") ;;
    esac
done
exec clang --target=x86_64-pc-windows-gnu -fuse-ld=lld -Wno-unused-command-line-argument -L"${MINGW_LIBS}" -L/usr/lib/wine/x86_64-windows "\${filtered[@]}"
EOF
chmod +x "${MINGW_BIN}/x86_64-w64-mingw32-gcc"
ln -sf "${MINGW_BIN}/x86_64-w64-mingw32-gcc" "${MINGW_BIN}/x86_64-w64-mingw32-g++"

clang --target=x86_64-pc-windows-gnu -O2 -c "${TMP_BUILD}/crt2.c" -o "${MINGW_LIBS}/crt2.o"
cp "${MINGW_LIBS}/crt2.o" "${MINGW_LIBS}/dllcrt2.o"
clang --target=x86_64-pc-windows-gnu -x c /dev/null -c -o "${MINGW_LIBS}/crtbegin.o"
cp "${MINGW_LIBS}/crtbegin.o" "${MINGW_LIBS}/crtend.o"
clang --target=x86_64-pc-windows-gnu -O2 -fno-builtin -c "${TMP_BUILD}/unwind_shim.c" -o "${TMP_BUILD}/unwind_shim.o"

for a in libgcc_eh.a libgcc.a libgcc_s.a libmingwex.a libmingw32.a libmoldname.a libpthread.a; do
    llvm-ar rcs "${MINGW_LIBS}/${a}" "${TMP_BUILD}/unwind_shim.o"
done
cp /usr/lib/wine/x86_64-windows/libucrtbase.a "${MINGW_LIBS}/libmsvcrt.a"

PATH="${MINGW_BIN}:${PATH}" \
CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER="${MINGW_BIN}/x86_64-w64-mingw32-gcc" \
CC_x86_64_pc_windows_gnu="${MINGW_BIN}/x86_64-w64-mingw32-gcc" \
CXX_x86_64_pc_windows_gnu="${MINGW_BIN}/x86_64-w64-mingw32-g++" \
AR_x86_64_pc_windows_gnu="${MINGW_BIN}/x86_64-w64-mingw32-ar" \
cargo build --release --target x86_64-pc-windows-gnu --bin chronomix

cp "target/x86_64-pc-windows-gnu/release/chronomix.exe" "${TMP_BUILD}/payload_chronomix.exe"
llvm-strip "${TMP_BUILD}/payload_chronomix.exe"
cp LICENSE "${TMP_BUILD}/payload_LICENSE.txt"
cp README.md "${TMP_BUILD}/payload_README.md"

cat > "${TMP_BUILD}/resources.rc" <<EOF
101 RCDATA "payload_chronomix.exe"
102 RCDATA "payload_LICENSE.txt"
103 RCDATA "payload_README.md"
EOF

(cd "${TMP_BUILD}" && llvm-windres --target=pe-x86-64 resources.rc -O coff -o resources_x64.o)

clang --target=x86_64-pc-windows-gnu -nostdlib -O2 \
    -DAPP_VER="\"${VER}\"" \
    -I/usr/include/wine/windows -I/usr/include/wine/msvcrt \
    "${TMP_BUILD}/setup.c" "${TMP_BUILD}/resources_x64.o" \
    -L/usr/lib/wine/x86_64-windows \
    -Wl,-subsystem,windows -luser32 -lkernel32 -lshell32 -lucrtbase -lmsvcrt \
    -Wl,-e,WinMain -fuse-ld=lld \
    -o "dist/Chronomix-Setup-Windows-x64.exe"
echo "  -> Listo: dist/Chronomix-Setup-Windows-x64.exe"

echo "[3/3] Compilando Instalador Windows ARM64 (.exe)..."
ARM64_BIN="${TMP_BUILD}/arm64-bin"
ARM64_LIBS="${TMP_BUILD}/arm64-libs"
mkdir -p "${ARM64_BIN}" "${ARM64_LIBS}"

for tool in dlltool ar ranlib windres strip nm objcopy; do
    ln -sf "/usr/bin/llvm-${tool}" "${ARM64_BIN}/aarch64-w64-mingw32-${tool}"
done

python3 - "${ARM64_LIBS}" <<'PYEOF'
import subprocess, os, sys
out_dir = sys.argv[1]
for lib in ["ucrtbase", "kernel32", "user32", "gdi32", "shell32", "advapi32", "ntdll", "userenv", "ws2_32", "dbghelp", "msimg32", "winspool", "opengl32", "ole32", "oleaut32", "imm32", "winmm", "uxtheme", "dwmapi", "shlwapi", "comdlg32", "comctl32", "propsys", "rpcrt4", "secur32", "bcrypt", "synchronization"]:
    src = f"/usr/lib/wine/x86_64-windows/lib{lib}.a"
    if not os.path.exists(src): continue
    out = subprocess.check_output(["llvm-nm", "--defined-only", src], text=True, errors="ignore")
    syms = []
    for line in out.splitlines():
        parts = line.split()
        if len(parts) == 3 and parts[1] in ("T", "I") and not parts[2].startswith("__imp_") and not parts[2].startswith("."):
            syms.append(parts[2])
    syms = sorted(set(syms))
    def_path = os.path.join(out_dir, f"{lib}.def")
    with open(def_path, "w") as f:
        f.write(f"LIBRARY {lib}.dll\nEXPORTS\n")
        for s in syms:
            f.write(f"  {s}\n")
    subprocess.check_call(["llvm-dlltool", "-m", "arm64", "-d", def_path, "-l", os.path.join(out_dir, f"lib{lib}.a")])
PYEOF

cat > "${ARM64_BIN}/aarch64-w64-mingw32-clang" <<EOF
#!/usr/bin/env bash
filtered=()
for arg in "\$@"; do
    case "\$arg" in
        *rsbegin.o|*rsend.o|crtbegin.o|crtend.o) ;;
        *) filtered+=("\$arg") ;;
    esac
done
exec clang --target=aarch64-pc-windows-gnu -fuse-ld=lld -Wno-unused-command-line-argument -L"${ARM64_LIBS}" "\${filtered[@]}"
EOF
chmod +x "${ARM64_BIN}/aarch64-w64-mingw32-clang"
ln -sf "${ARM64_BIN}/aarch64-w64-mingw32-clang" "${ARM64_BIN}/aarch64-w64-mingw32-clang++"

clang --target=aarch64-pc-windows-gnu -O2 -c "${TMP_BUILD}/crt2.c" -o "${ARM64_LIBS}/crt2.o"
cp "${ARM64_LIBS}/crt2.o" "${ARM64_LIBS}/dllcrt2.o"
clang --target=aarch64-pc-windows-gnu -x c /dev/null -c -o "${ARM64_LIBS}/crtbegin.o"
cp "${ARM64_LIBS}/crtbegin.o" "${ARM64_LIBS}/crtend.o"
clang --target=aarch64-pc-windows-gnu -O2 -fno-builtin -c "${TMP_BUILD}/unwind_shim.c" -o "${TMP_BUILD}/unwind_shim_arm64.o"

for a in libgcc_eh.a libgcc.a libgcc_s.a libmingwex.a libmingw32.a libmoldname.a libpthread.a libunwind.a; do
    llvm-ar rcs "${ARM64_LIBS}/${a}" "${TMP_BUILD}/unwind_shim_arm64.o"
done
cp "${ARM64_LIBS}/libucrtbase.a" "${ARM64_LIBS}/libmsvcrt.a"

PATH="${ARM64_BIN}:${PATH}" \
CARGO_TARGET_AARCH64_PC_WINDOWS_GNULLVM_LINKER="${ARM64_BIN}/aarch64-w64-mingw32-clang" \
CC_aarch64_pc_windows_gnullvm="${ARM64_BIN}/aarch64-w64-mingw32-clang" \
AR_aarch64_pc_windows_gnullvm="${ARM64_BIN}/aarch64-w64-mingw32-ar" \
cargo build --release --target aarch64-pc-windows-gnullvm --bin chronomix

cp "target/aarch64-pc-windows-gnullvm/release/chronomix.exe" "${TMP_BUILD}/payload_chronomix.exe"
llvm-strip "${TMP_BUILD}/payload_chronomix.exe"

(cd "${TMP_BUILD}" && llvm-windres --target=aarch64-w64-mingw32 resources.rc -O coff -o resources_arm64.o)

clang --target=aarch64-pc-windows-gnu -nostdlib -O2 \
    -DAPP_VER="\"${VER}\"" \
    -I/usr/include/wine/windows -I/usr/include/wine/msvcrt \
    "${TMP_BUILD}/setup.c" "${TMP_BUILD}/resources_arm64.o" \
    -L"${ARM64_LIBS}" \
    -Wl,-subsystem,windows -luser32 -lkernel32 -lshell32 -lucrtbase -lmsvcrt \
    -Wl,-e,WinMain -fuse-ld=lld \
    -o "dist/Chronomix-Setup-Windows-ARM64.exe"
echo "  -> Listo: dist/Chronomix-Setup-Windows-ARM64.exe"

echo "========================================================"
echo "  Archivos finales en ./dist/ :"
ls -lh dist/
echo "========================================================"
