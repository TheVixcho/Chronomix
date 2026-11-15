# Chronomix — Studio Audio DAW & Digital Mixer (`v0.0.1`)

**Creado por [TheVixcho](https://github.com/TheVixcho)** · **Licencia [MIT](LICENSE)**

**Chronomix** es una estación de trabajo de audio digital (**DAW**), consola de mezcla digital profesional inspirada en **Behringer X32 / Wing** y host nativo **VST2 / VST3** multiplataforma escrito en Rust (`eframe`/`egui` + `cpal` + `vst3-host` + `libloading`), con puente integrado para plugins de Windows (64 bits `.dll` / `.vst3`) sobre Linux mediante Wine.

---

## Instalación Rápida

### Windows (Instalador Oficial `.exe` x64 y ARM64)
1. Descarga **`Chronomix-Setup-Windows-x64.exe`** (PCs Intel/AMD) o **`Chronomix-Setup-Windows-ARM64.exe`** (Windows en ARM / Snapdragon) desde la sección de [Releases](https://github.com/TheVixcho/chronomix/releases/latest).
2. Ejecuta el instalador, acepta la licencia **MIT (TheVixcho)** y confirma crear el **acceso directo en el Escritorio y Menú Inicio**.
3. Abre **Chronomix** desde tu Escritorio o Menú Inicio.

### Linux x86_64 y ARM64 (Instalación en 1 Comando)
Pega este comando en tu terminal para descargar automáticamente el paquete correspondiente a tu arquitectura (`Chronomix-Linux-x86_64.tar.gz` o `Chronomix-Linux-ARM64.tar.gz`), instalar el binario y crear el acceso directo en tu Escritorio y menú de aplicaciones:

```bash
curl -fsSL https://raw.githubusercontent.com/TheVixcho/chronomix/main/install.sh | bash
```

### macOS Apple Silicon / Intel (Instalación en 1 Comando o `.dmg`)
Con un solo comando en la terminal:

```bash
curl -fsSL https://raw.githubusercontent.com/TheVixcho/chronomix/main/install.sh | bash
```

O descarga **`Chronomix-macOS.dmg`** desde [Releases](https://github.com/TheVixcho/chronomix/releases/latest) y arrastra `Chronomix.app` a tu carpeta `Applications`.

---

## Características Principales (`v0.0.1`)

- **Consola Digital Profesional (Estilo Behringer Wing / X32)**:
  - Interfaz en escala de grises y negro de alto contraste con botones táctiles biselados efecto vidrio/goma y faders motorizados.
  - Bancos de canales dedicados a la izquierda: `CH 1-16`, `CH 17-32`, `CH 33-48` y `MULTIMEDIA`, con scroll horizontal fluido.
  - Pantalla superior de consola con altura fija proporcional (`HOME`, `FUENTE`, `EQ`, `DINÁMICA`, `RACK VST`, `SALIDA / REC`, `AJUSTES`, `NOVEDADES`).
- **Canales Personalizables y Multimedia**:
  - Por defecto inicia con 2 canales listos para usar; permite agregar hasta 48 canales.
  - Iconos predeterminados por instrumento (Voz, Guitarra, Bajo, Batería, Teclado, Podcast, Multimedia) o importación de imagen personalizada.
  - Selector de color por canal desde la pestaña `FUENTE` (negro por defecto).
  - Controles de reproducción Multimedia (`Play/Pause`, `Stop`, `-10s`, `+10s`, `Loop`) por canal, barra inferior dedicada en el banco Multimedia y sección de **Producción / Playback Global** para disparar todas las pistas multimedia al unísono.
- **Grabación Multipista Selectiva (`REC`)**:
  - Cada canal cuenta con un botón `REC` independiente: solo se graban en archivos `.wav` separados los canales que tienen `REC` activado, más la mezcla maestra opcional.
- **Procesamiento DSP & Salida de Micrófono Virtual (Discord / OBS)**:
  - Trim de entrada, Balance estéreo, Inversión de fase (`Ø`), Filtros HPF / LPF, EQ paramétrico de 4 bandas, Noise Gate, Compresor y hasta 4 inserciones VST2/VST3 por canal y en el bus Master.
  - Botón **Mic Virtual (OBS / Discord)** para digitalizar y enrutar la salida maestra como un micrófono virtual hacia Discord, OBS Studio, Zoom o navegadores.
- **Barra Inferior Estilo DaVinci Resolve & Biblioteca de Audio**:
  - Navegación inferior rápida entre **Mixer** y **Biblioteca**.
  - En **Biblioteca**, explora todas las grabaciones guardadas por el usuario y asígnalas con un clic a un canal Multimedia nuevo o existente.

---

## Automatización de Releases (CI/CD con GitHub Actions)

El repositorio incluye el flujo automatizado [`.github/workflows/release.yml`](.github/workflows/release.yml).
Cada vez que cambies la versión en `Cargo.toml` y hagas push a `main`, o publiques un nuevo tag:

```bash
git tag v0.0.1
git push origin v0.0.1
```

GitHub Actions encenderá automáticamente máquinas virtuales de **Windows**, **Linux** y **macOS**, compilará el proyecto en modo `--release`, empaquetará el instalador `.exe` (con Inno Setup, licencia MIT y acceso directo en escritorio), el paquete `.deb`, el `.dmg` de macOS y el instalador `install.sh`, y los publicará en **GitHub Releases**.

---

## Compilar desde el Código Fuente

```bash
cargo build --release --bin chronomix
cargo test --workspace
```

---

## Licencia

Distribuido bajo la licencia **MIT**. Creado por **TheVixcho**. Consulta el archivo [LICENSE](LICENSE) para más detalles.
