# Chronomix — Studio Audio DAW, Digital Mixer & Dual-Deck DJ (`v0.0.1`)

**Creado por [TheVixcho](https://github.com/TheVixcho)** · **Licencia [MIT](LICENSE)**

**Chronomix** es una estación de trabajo de audio digital (**DAW**), consola de mezcla profesional, estudio **MIDI**, mezclador **DJ de doble bandeja** y host nativo **VST2 / VST3** multiplataforma construido con una arquitectura híbrida **JUCE (C++20) + Rust** (`chronomix-ffi` + `pedal-engine` + `pedal-audio` + `pedal-wine`), con soporte para plugins nativos y puente Wine para plugins Windows de 64 bits (`.dll` / `.vst3`) en Linux.

---

## Instalación Rápida

### Linux x86_64 / ARM64 y macOS (Instalación en 1 Comando)
El instalador resuelve automáticamente la publicación más reciente a través de la API de GitHub Releases (incluyendo pre-releases) y verifica dependencias con mensajes claros si necesita compilar desde el código fuente:

```bash
curl -fsSL https://raw.githubusercontent.com/TheVixcho/chronomix/main/install.sh | bash
```

### Windows
Compila nativamente con CMake + Rust (`MSVC`) usando:

```powershell
.\installer\build-windows.ps1
```

O descarga los paquetes publicados en [Releases](https://github.com/TheVixcho/chronomix/releases).

---

## Almacenamiento Estándar del Sistema (XDG / AppData)

Chronomix no ensucia tu directorio de trabajo ni `$HOME`:

- **Linux (XDG)**:
  - Ajustes y estado de ventana validado: `~/.config/chronomix/` (`settings.json`, `window.json`)
  - Plugins VST: `~/.local/share/chronomix/vst_plugins/`
  - Grabaciones multipista: `~/.local/share/chronomix/recordings/`
  - Perfiles DSP JSON: `~/.local/share/chronomix/profiles/`
- **macOS**: `~/Library/Application Support/Chronomix/`
- **Windows**: `%APPDATA%\Chronomix\`

---

## Características Principales (`v0.0.1`)

- **Arquitectura Nativa JUCE + Rust**:
  - Interfaz de hardware cuadrada (`0px`) con acentos naranja de estudio (`#F58220`), perillas rotativas, faders metálicos, medidores LED segmentados de 3 colores y decodificación nativa de audio (**WAV, MP3, FLAC, OGG, AIFF**) sin dependencias externas.
  - Posicionamiento de ventana validado contra todos los monitores activos (`Desktop::getDisplays`), evitando aperturas fuera de pantalla en **Hyprland / Wayland**.
- **Consola Mixer Profesional (hasta 48 canales)**:
  - Menú principal a la izquierda (`INICIO`, `FUENTE`, `EQ`, `DINÁMICA`, `PERFILES`, `VST FX`, `E/S & GRAB`, `AJUSTES`, `NOVEDADES`) adaptable a mosaicos estrechos.
  - Bancos de canales (`CH 1-16`, `CH 17-32`, `CH 33-48`, `MEDIA`, `TODOS`) y **Master Bus** fijado a la derecha con margen de seguridad.
  - Cadena DSP de latencia cero por canal: Trim, Balance, Fase, HPF/LPF, EQ paramétrico de 4 bandas con **barras LED de saturación en 3 colores (Verde / Amarillo / Rojo)**, Noise Gate, Compresor, De-Esser, Analog Drive, Stereo Delay, Studio Reverb, Peak Limiter y 4 slots VST2/VST3 por canal y en Master.
  - **23 Perfiles DSP predeterminados** categorizados + importación y exportación de perfiles en formato `.json`.
  - **Micrófono Virtual (Discord / OBS)** sobre PipeWire y **Grabación Multipista selectiva (`REC`)**.
  - **Aviso en tiempo de ejecución para `wineg++`**: al intentar cargar un plugin Windows `.dll` o `.vst3` en Linux sin `wineg++` instalado, la aplicación muestra una alerta clara con instrucciones de instalación.
- **Panel DJ de Doble Bandeja (Deck A + Deck B)**:
  - Mezcla en vivo dos pistas directamente integradas en el motor maestro (compatible con grabación y Mic Virtual).
  - Arrastra y suelta o carga pistas **WAV, MP3, FLAC, OGG, AIFF** en Deck A y Deck B.
  - **Waveforms RGB de 3 bandas** (vista general + zoom desplazable en tiempo real) con búsqueda por clic.
  - **Jog Wheels interactivos** giratorios para scratch y ajuste fino (pitch bend).
  - Detección automática de **BPM**, botón **SYNC** entre bandejas, fader de **Pitch (-16% a +16%)**, **CUE**, **4 Hot Cues** por bandeja y **Auto-Loop** (`1/2`, `1`, `2`, `4`, `8`, `16` beats).
  - Mezclador central DJ con **Trim**, **Isolator EQ de 3 bandas + botones KILL**, **Filtro bipolar color (LPF / HPF)**, medidores LED estéreo, faders de canal, **Crossfader** con 3 curvas (*Smooth Power*, *Lineal*, *Scratch Cut*) y transición automática **Auto-Mix (`<< MIX A` / `MIX B >>`)**.
- **MIDI Studio**:
  - Múltiples sesiones MIDI enlazadas como canales del Mixer.
  - Secuenciador de batería de 16 pasos (`Kick`, `Clap`, `Hat`, `Snare`, `Bass 808`), **Piano Roll** interactivo de 24 semitonos × 16 pasos con playhead en vivo y **teclado virtual** tocable por ratón y teclado físico.
  - 5 motores sintetizadores (*Studio Keys / Analog Lead*, *Electric Piano FM*, *Warm Saw Pad*, *Pluck Arp*, *808 Sub Bass*).
- **Biblioteca de Audio**:
  - Explora grabaciones y colecciones de audio, con asignación en un clic a un **canal del Mixer**, **DJ Deck A** o **DJ Deck B**.

---

## Catálogo de Perfiles DSP Integrados (23 Presets)

| Perfil / Preset | Categoría | Uso Recomendado |
| :--- | :--- | :--- |
| **Podcast Pro (Voz Broadcast)** | Voz & Podcasts | Podcasts, locución profesional, entrevistas de micrófono cercano |
| **Voz Radial FM / DJ** | Voz & Podcasts | Locutores de radio, voces enérgicas y contundentes |
| **Streamer / Directos** | Voz & Podcasts | Directos, partidas de videojuegos y chats de voz |
| **Voz Cálida Studio** | Voz & Podcasts | Baladas, cantantes solistas, acústicos íntimos |
| **Voz Brillante Pop** | Voz & Podcasts | Voz líder moderna y pop contemporáneo |
| **Voz Íntima / ASMR** | Voz & Podcasts | Susurros, foley íntimo y grabaciones binaurales |
| **Doblaje y Cine** | Voz & Podcasts | Actuación de voz, documentales y doblaje audiovisual |
| **Guitarra Acústica Natural** | Guitarras & Cuerdas | Guitarras folk y clásicas grabadas con micrófono |
| **Guitarra Fingerstyle** | Guitarras & Cuerdas | Arpegios y solos acústicos |
| **Guitarra Eléctrica Rock** | Guitarras & Cuerdas | Solos de rock, riffs potentes y guitarras con sustain |
| **Guitarra Clean Ambience** | Guitarras & Cuerdas | Acordes limpios, ambient e indie |
| **Bajo Punch & Slap** | Bajo | Bajos slap funk, pop moderno y rock |
| **Bajo Cálido Tubo** | Bajo | Soul, R&B, jazz y rock clásico |
| **Bombo Punch 808** | Batería & Percusión | Bombos acústicos, samples trap/hip hop y bajos 808 |
| **Caja Snare Crisp** | Batería & Percusión | Cajas de batería con pegada y chasquido nítido |
| **Platillos / Overheads** | Batería & Percusión | Micrófonos aéreos de batería y platillos |
| **Piano de Cola Concierto** | Teclados & Orquesta | Pianos acústicos de cola y teclados de escenario |
| **Cuerdas Sinfónicas** | Teclados & Orquesta | Violines, violas, cellos y arreglos orquestales |
| **Sintetizador Lead** | Teclados & Orquesta | Sintetizadores lead, plucks y arpegios |
| **Teléfono Vintage Lofi** | Efectos & Retro | Efecto retro de llamada telefónica y lofi |
| **Megáfono y Altavoz** | Efectos & Retro | Avisos de megafonía y radio distorsionada |
| **Mastering Polish Glue** | Mastering & Utilidad | Toque final y pegamento para el bus Master |
| **Plano / Reset Total** | Mastering & Utilidad | Curva completamente limpia sin procesadores |

---

## Compilar desde el Código Fuente (JUCE + Rust)

```bash
cmake -S . -B build -G Ninja -DCMAKE_BUILD_TYPE=Release
cmake --build build --parallel
cargo test --workspace
```

Para generar el paquete de distribución en `./dist/`:

```bash
./build-release.sh
```

---

## Licencia

Distribuido bajo la licencia **MIT**. Creado por **TheVixcho**. Consulta [LICENSE](LICENSE).
