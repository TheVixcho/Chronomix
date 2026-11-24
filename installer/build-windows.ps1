$ErrorActionPreference = "Stop"

Write-Host "========================================================"
Write-Host "  Compilando Chronomix (JUCE + Rust) para Windows"
Write-Host "========================================================"

if (-not (Get-Command cargo -ErrorAction SilentlyContinue)) {
    throw "Falta Rust/Cargo. Instala desde https://rustup.rs"
}
if (-not (Get-Command cmake -ErrorAction SilentlyContinue)) {
    throw "Falta CMake (>= 3.22). Instala desde https://cmake.org"
}

cmake -S . -B build-win -DCMAKE_BUILD_TYPE=Release
cmake --build build-win --config Release --parallel

$exe = Get-ChildItem -Path build-win -Recurse -Filter "Chronomix.exe" | Select-Object -First 1
if (-not $exe) {
    throw "No se encontró Chronomix.exe después de la compilación."
}

New-Item -ItemType Directory -Force -Path dist | Out-Null
$stage = Join-Path $env:TEMP "chronomix-win-stage"
if (Test-Path $stage) { Remove-Item -Recurse -Force $stage }
New-Item -ItemType Directory -Force -Path $stage | Out-Null

Copy-Item $exe.FullName (Join-Path $stage "chronomix.exe")
Copy-Item "LICENSE" (Join-Path $stage "LICENSE.txt")
Copy-Item "README.md" (Join-Path $stage "README.md")

$zipOut = "dist\Chronomix-Windows-x64.zip"
if (Test-Path $zipOut) { Remove-Item -Force $zipOut }
Compress-Archive -Path "$stage\*" -DestinationPath $zipOut

if (Get-Command makensis -ErrorAction SilentlyContinue) {
    makensis installer\chronomix.nsi
}

Write-Host "Compilación Windows completada en dist\"
