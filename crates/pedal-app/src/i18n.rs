use std::path::PathBuf;

/// Supported interface languages for Chronomix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Lang {
    Es,
    En,
}

impl Lang {
    /// Detects the initial UI language from standard POSIX locale environment variables.
    pub fn detect_default() -> Self {
        for var in ["LC_ALL", "LC_MESSAGES", "LANG", "LANGUAGE"] {
            if let Ok(val) = std::env::var(var) {
                let lower = val.trim().to_ascii_lowercase();
                if lower.starts_with("es") {
                    return Self::Es;
                }
                if lower.starts_with("en") {
                    return Self::En;
                }
            }
        }
        Self::Es
    }

    pub fn code(self) -> &'static str {
        match self {
            Self::Es => "ES",
            Self::En => "EN",
        }
    }

    pub fn display_name(self) -> &'static str {
        match self {
            Self::Es => "Español",
            Self::En => "English",
        }
    }

    /// Returns static UI copy for the selected language.
    pub fn text(self) -> &'static Strings {
        match self {
            Self::Es => &ES_STRINGS,
            Self::En => &EN_STRINGS,
        }
    }

    pub fn live_status(
        self,
        in_sr: u32,
        in_ch: u16,
        out_sr: u32,
        out_ch: u16,
        block_size: usize,
    ) -> String {
        match self {
            Self::Es => format!(
                "En vivo: {in_sr} Hz ({in_ch} ch ent.) -> {out_sr} Hz ({out_ch} ch sal.), {block_size} muestras"
            ),
            Self::En => format!(
                "Live: {in_sr} Hz (in {in_ch} ch) -> {out_sr} Hz (out {out_ch} ch), {block_size} samples"
            ),
        }
    }

    pub fn scan_complete_status(self, count: usize) -> String {
        match self {
            Self::Es => format!("Escaneo VST completado: {count} plugin(s) encontrado(s)."),
            Self::En => format!("VST scan complete: found {count} plugin(s)."),
        }
    }

    pub fn loaded_vst3_status(self, name: &str, param_count: usize) -> String {
        match self {
            Self::Es => format!("VST cargado: {name} ({param_count} parámetros)"),
            Self::En => format!("Loaded VST: {name} ({param_count} parameters)"),
        }
    }

    pub fn opened_folder_status(self, path: &std::path::Path) -> String {
        match self {
            Self::Es => format!("Carpeta de VST abierta: {}", path.display()),
            Self::En => format!("Opened VST folder: {}", path.display()),
        }
    }

    pub fn vst_folder_updated_status(self, path: &std::path::Path) -> String {
        match self {
            Self::Es => format!("Carpeta de VST actualizada: {}", path.display()),
            Self::En => format!("Updated VST folder: {}", path.display()),
        }
    }

    pub fn saved_recording_status(self, path: &PathBuf) -> String {
        match self {
            Self::Es => format!("Grabación guardada en {}", path.display()),
            Self::En => format!("Saved recording to {}", path.display()),
        }
    }

    pub fn recording_to_status(self, path: &PathBuf) -> String {
        match self {
            Self::Es => format!("Grabando en {}...", path.display()),
            Self::En => format!("Recording to {}...", path.display()),
        }
    }

    pub fn stop_rec_label(self, secs: f64) -> String {
        match self {
            Self::Es => format!("Detener Grab. ({secs:.1}s)"),
            Self::En => format!("Stop Rec ({secs:.1}s)"),
        }
    }

    pub fn plugin_meta_details(
        self,
        vendor: &str,
        version: &str,
        category: &str,
        in_ch: usize,
        out_ch: usize,
    ) -> String {
        match self {
            Self::Es => format!(
                "por {vendor} | v{version} | {category} | {in_ch} ent. / {out_ch} sal."
            ),
            Self::En => format!(
                "by {vendor} | v{version} | {category} | {in_ch} in / {out_ch} out ch"
            ),
        }
    }

    pub fn device_label(self, name: &str, ch: u16, sr: u32) -> String {
        match self {
            Self::Es => format!("{name} ({ch} ch, {sr} Hz)"),
            Self::En => format!("{name} ({ch} ch, {sr} Hz)"),
        }
    }

    pub fn underruns_overruns(self, underruns: u64, overruns: u64) -> String {
        match self {
            Self::Es => format!("Subdesbordes: {underruns} | Desbordes: {overruns}"),
            Self::En => format!("Underruns: {underruns} | Overruns: {overruns}"),
        }
    }

    pub fn failed_start_audio(self, err: &str) -> String {
        match self {
            Self::Es => format!("No se pudo iniciar el audio: {err}"),
            Self::En => format!("Failed to start audio: {err}"),
        }
    }

    pub fn failed_load_vst3(self, err: &str) -> String {
        match self {
            Self::Es => format!("Error al cargar VST3: {err}"),
            Self::En => format!("Failed to load VST3: {err}"),
        }
    }

    pub fn failed_open_gui(self, err: &str) -> String {
        match self {
            Self::Es => format!("No se pudo abrir la GUI del plugin: {err}"),
            Self::En => format!("Failed to open plugin GUI: {err}"),
        }
    }
}

/// Static localized UI copy for all panels and controls.
pub struct Strings {
    pub nav_news: &'static str,
    pub nav_recording: &'static str,
    pub nav_settings: &'static str,
    pub nav_live_badge: &'static str,
    pub nav_rec_badge: &'static str,
    pub lang_label: &'static str,

    pub news_hero_subtitle: &'static str,
    pub news_hero_description: &'static str,
    pub news_go_recording: &'static str,
    pub news_quickstart_title: &'static str,
    pub news_step1_title: &'static str,
    pub news_step1_desc: &'static str,
    pub news_step2_title: &'static str,
    pub news_step2_desc: &'static str,
    pub news_step3_title: &'static str,
    pub news_step3_desc: &'static str,
    pub news_section_title: &'static str,
    pub news_items: &'static [NewsItem],

    pub chain_input: &'static str,
    pub chain_effect: &'static str,
    pub chain_output: &'static str,
    pub chain_record: &'static str,
    pub chain_clean: &'static str,
    pub chain_ready: &'static str,

    pub settings_title: &'static str,
    pub settings_subtitle: &'static str,
    pub settings_vst_folder_heading: &'static str,
    pub settings_vst_folder_desc: &'static str,
    pub settings_active_folder_label: &'static str,
    pub settings_mode_program: &'static str,
    pub settings_mode_custom: &'static str,
    pub settings_open_vst_folder: &'static str,
    pub settings_choose_custom_folder: &'static str,
    pub settings_use_program_folder: &'static str,
    pub settings_scan_now: &'static str,
    pub settings_include_system_vst: &'static str,
    pub settings_detected_heading: &'static str,
    pub settings_search_hint: &'static str,
    pub settings_no_plugins_in_folder: &'static str,
    pub settings_load_in_recording: &'static str,

    pub start_monitoring: &'static str,
    pub stop_monitoring: &'static str,
    pub sample_rate_label: &'static str,
    pub buffer_label: &'static str,
    pub mute_output: &'static str,
    pub record_wav: &'static str,

    pub input_device_heading: &'static str,
    pub output_device_heading: &'static str,
    pub refresh: &'static str,
    pub none: &'static str,
    pub input_gain_db: &'static str,
    pub output_gain_db: &'static str,
    pub meter_in: &'static str,
    pub meter_out: &'static str,

    pub vst3_slot_heading: &'static str,
    pub scanning: &'static str,
    pub scan_installed_vst3: &'static str,
    pub open_vst_folder_btn: &'static str,
    pub open_bundle: &'static str,
    pub open_file: &'static str,
    pub discovered: &'static str,
    pub select_plugin: &'static str,
    pub load_selected: &'static str,
    pub path_label: &'static str,
    pub path_hint: &'static str,
    pub browse: &'static str,
    pub folder: &'static str,
    pub load_path: &'static str,
    pub bypass_effect: &'static str,
    pub dry_wet_percent: &'static str,
    pub open_native_gui: &'static str,
    pub close_native_gui: &'static str,
    pub refresh_params: &'static str,
    pub unload: &'static str,
    pub parameters: &'static str,
    pub filter_params_hint: &'static str,
    pub no_plugin_loaded: &'static str,

    pub status_ready: &'static str,
    pub status_stopped: &'static str,
    pub status_scanning: &'static str,
    pub status_unloaded: &'static str,
    pub error_prefix: &'static str,
    pub error_select_io: &'static str,
    pub error_start_before_rec: &'static str,
    pub dismiss: &'static str,

    pub dialog_wav_filter: &'static str,
    pub dialog_vst3_filter: &'static str,
    pub dialog_select_bundle: &'static str,
    pub dialog_select_plugin: &'static str,
    pub dialog_select_vst_folder: &'static str,
}

pub struct NewsItem {
    pub tag: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

static ES_NEWS: &[NewsItem] = &[
    NewsItem {
        tag: "Nuevo",
        title: "Soporte VST2 + VST3 y Panel de Ajustes con Carpeta de VST",
        summary: "Ahora puedes cargar plugins VST2 (.dll / .so) y VST3 (.vst3). En el nuevo panel Ajustes puedes abrir la carpeta de VST del programa para pegar tus plugins o elegir una carpeta personalizada.",
    },
    NewsItem {
        tag: "Audio + Wine",
        title: "Puente integrado para VST2 y VST3 de Windows (x64) en Linux",
        summary: "Carga plugins VST2 y VST3 nativos de Linux o de Windows de 64 bits mediante Wine con procesamiento en tiempo real y ventana nativa HWND.",
    },
    NewsItem {
        tag: "Grabación",
        title: "Monitoreo en tiempo real y exportación WAV",
        summary: "Enruta tu micrófono o instrumento con baja latencia, ajusta ganancia de entrada/salida, mezcla Dry/Wet, bypass y graba tus tomas directamente a WAV.",
    },
];

static EN_NEWS: &[NewsItem] = &[
    NewsItem {
        tag: "New",
        title: "VST2 + VST3 support & Settings Panel with VST Folder",
        summary: "Chronomix now supports both VST2 (.dll / .so) and VST3 (.vst3) plugins. Use the new Settings panel to open the program's VST folder and paste your plugins or configure a custom VST folder.",
    },
    NewsItem {
        tag: "Audio + Wine",
        title: "Integrated Windows (x64) VST2 & VST3 bridge on Linux",
        summary: "Load native Linux VST2/VST3 plugins and 64-bit Windows VST2 (.dll) & VST3 (.vst3) plugins via Wine with real-time DSP and native HWND editor windows.",
    },
    NewsItem {
        tag: "Recording",
        title: "Real-time monitoring & WAV recording",
        summary: "Route your microphone or instrument at low latency, dial input/output gain, Dry/Wet mix, bypass, and record processed takes straight to WAV.",
    },
];

static ES_STRINGS: Strings = Strings {
    nav_news: "Novedades",
    nav_recording: "Grabación",
    nav_settings: "Ajustes",
    nav_live_badge: "En vivo",
    nav_rec_badge: "Grabando",
    lang_label: "Idioma:",

    news_hero_subtitle: "DAW de audio en tiempo real y host VST2 / VST3",
    news_hero_description: "Conecta tu interfaz o micrófono USB, procesa efectos VST2 y VST3 con baja latencia y graba directamente a WAV. Gestiona tu carpeta de plugins en Ajustes o abre Grabación en la barra superior.",
    news_go_recording: "Ir a Grabación",
    news_quickstart_title: " Flujo de trabajo en 3 pasos",
    news_step1_title: "1. Añade tus plugins VST",
    news_step1_desc: "Ve a Ajustes y pulsa «Abrir Carpeta de VST» para pegar tus archivos .vst3 o .dll.",
    news_step2_title: "2. Activa el monitoreo",
    news_step2_desc: "En Grabación elige tu micrófono/instrumento y tus audífonos, y pulsa «Iniciar Monitoreo».",
    news_step3_title: "3. Carga un efecto y graba",
    news_step3_desc: "Selecciona un plugin de tu biblioteca, abre su GUI nativa, ajusta la mezcla y graba a WAV.",
    news_section_title: "Novedades",
    news_items: ES_NEWS,

    chain_input: "1. ENTRADA",
    chain_effect: "2. EFECTO VST",
    chain_output: "3. SALIDA",
    chain_record: "4. GRABACIÓN",
    chain_clean: "Señal limpia (Sin VST)",
    chain_ready: "Listo para grabar WAV",

    settings_title: "Ajustes de Chronomix",
    settings_subtitle: "Gestiona dónde se guardan y escanean tus plugins VST2 (.dll / .so) y VST3 (.vst3).",
    settings_vst_folder_heading: "Carpeta de Plugins VST (VST2 y VST3)",
    settings_vst_folder_desc: "Pulsa «Abrir Carpeta de VST» para abrir el explorador de archivos y mover o pegar ahí tus plugins (.vst3, .dll, .so), o selecciona cualquier carpeta personalizada de tu equipo.",
    settings_active_folder_label: "Carpeta activa:",
    settings_mode_program: "Carpeta del Programa",
    settings_mode_custom: "Carpeta Personalizada",
    settings_open_vst_folder: "Abrir Carpeta de VST",
    settings_choose_custom_folder: "Elegir Carpeta Personalizada...",
    settings_use_program_folder: "Usar Carpeta del Programa",
    settings_scan_now: "Escanear Carpeta VST",
    settings_include_system_vst: "Incluir también carpetas VST del sistema y Wine",
    settings_detected_heading: "Plugins detectados en tu biblioteca",
    settings_search_hint: "Buscar plugin por nombre o fabricante...",
    settings_no_plugins_in_folder: "Aún no se detectaron plugins en esta carpeta. Pulsa «Abrir Carpeta de VST», pega tus archivos .vst3 o .dll y pulsa «Escanear Carpeta VST».",
    settings_load_in_recording: "Cargar en Grabación",

    start_monitoring: "Iniciar Monitoreo",
    stop_monitoring: "Detener Monitoreo",
    sample_rate_label: "Frec.:",
    buffer_label: "Búfer:",
    mute_output: "Silenciar Salida",
    record_wav: "Grabar WAV",

    input_device_heading: "Dispositivo de Entrada (Mic / Instrumento)",
    output_device_heading: "Dispositivo de Salida (Audífonos / Monitores)",
    refresh: "Actualizar",
    none: "Ninguno",
    input_gain_db: "Ganancia Entrada (dB)",
    output_gain_db: "Ganancia Salida (dB)",
    meter_in: "Ent.",
    meter_out: "Sal.",

    vst3_slot_heading: "Ranura de Efecto VST (VST2 / VST3)",
    scanning: "Escaneando...",
    scan_installed_vst3: "Escanear VSTs",
    open_vst_folder_btn: "Abrir Carpeta VST",
    open_bundle: "Abrir Carpeta VST...",
    open_file: "Abrir Archivo (.vst3 / .dll / .so)...",
    discovered: "Encontrados:",
    select_plugin: "Seleccionar plugin",
    load_selected: "Cargar Seleccionado",
    path_label: "Ruta:",
    path_hint: "vst_plugins/MiEfecto.vst3 o MiPlugin.dll",
    browse: "Examinar...",
    folder: "Carpeta...",
    load_path: "Cargar Ruta",
    bypass_effect: "Bypass Efecto",
    dry_wet_percent: "Mezcla Dry/Wet %",
    open_native_gui: "Abrir GUI Nativa",
    close_native_gui: "Cerrar GUI Nativa",
    refresh_params: "Actualizar Parámetros",
    unload: "Descargar",
    parameters: "Parámetros:",
    filter_params_hint: "Filtrar parámetros...",
    no_plugin_loaded: "No hay ningún plugin VST cargado. Puedes iniciar el monitoreo para escuchar la señal limpia o cargar un efecto VST2 (.dll/.so) o VST3 (.vst3) arriba.",

    status_ready: "Listo. Selecciona dispositivos de entrada/salida y pulsa Iniciar Monitoreo.",
    status_stopped: "Monitoreo detenido.",
    status_scanning: "Escaneando carpeta de plugins VST2/VST3...",
    status_unloaded: "Plugin descargado (monitoreo limpio activo).",
    error_prefix: "Error:",
    error_select_io: "Selecciona un dispositivo de entrada y otro de salida.",
    error_start_before_rec: "Inicia el monitoreo de audio antes de grabar.",
    dismiss: "Cerrar",

    dialog_wav_filter: "Audio WAV",
    dialog_vst3_filter: "Plugin VST2 / VST3 (.vst3, .dll, .so)",
    dialog_select_bundle: "Seleccionar paquete o carpeta VST",
    dialog_select_plugin: "Seleccionar plugin VST2 / VST3",
    dialog_select_vst_folder: "Seleccionar carpeta de plugins VST",
};

static EN_STRINGS: Strings = Strings {
    nav_news: "What's New",
    nav_recording: "Recording",
    nav_settings: "Settings",
    nav_live_badge: "Live",
    nav_rec_badge: "Rec",
    lang_label: "Language:",

    news_hero_subtitle: "Real-Time Audio DAW & VST2 / VST3 Host",
    news_hero_description: "Connect your audio interface or USB mic, process VST2 & VST3 effects at low latency, and capture straight to WAV. Manage your VST folder in Settings or click Recording in the top toolbar.",
    news_go_recording: "Open Recording",
    news_quickstart_title: " 3-Step Quick Workflow",
    news_step1_title: "1. Add your VST plugins",
    news_step1_desc: "Open Settings and click \"Open VST Folder\" to drop in your .vst3 or .dll plugins.",
    news_step2_title: "2. Start live monitoring",
    news_step2_desc: "In Recording, pick your input mic/instrument and output headphones, then click \"Start Monitoring\".",
    news_step3_title: "3. Load an effect & record",
    news_step3_desc: "Pick any plugin from your library, open its native GUI, dial Dry/Wet mix, and record to WAV.",
    news_section_title: "What's New",
    news_items: EN_NEWS,

    chain_input: "1. INPUT",
    chain_effect: "2. VST EFFECT",
    chain_output: "3. OUTPUT",
    chain_record: "4. RECORDING",
    chain_clean: "Clean signal (No VST)",
    chain_ready: "Ready to record WAV",

    settings_title: "Chronomix Settings",
    settings_subtitle: "Manage where your VST2 (.dll / .so) and VST3 (.vst3) plugins are stored and scanned.",
    settings_vst_folder_heading: "VST Plugins Folder (VST2 & VST3)",
    settings_vst_folder_desc: "Click \"Open VST Folder\" to open your system file manager and move or paste your plugins (.vst3, .dll, .so) right in, or choose any custom folder on your computer.",
    settings_active_folder_label: "Active folder:",
    settings_mode_program: "Program Folder",
    settings_mode_custom: "Custom Folder",
    settings_open_vst_folder: "Open VST Folder",
    settings_choose_custom_folder: "Choose Custom Folder...",
    settings_use_program_folder: "Use Program Folder",
    settings_scan_now: "Scan VST Folder",
    settings_include_system_vst: "Also include system & Wine VST folders",
    settings_detected_heading: "Detected plugins in your library",
    settings_search_hint: "Search plugin by name or vendor...",
    settings_no_plugins_in_folder: "No plugins found in this folder yet. Click \"Open VST Folder\", paste your .vst3 or .dll files, and click \"Scan VST Folder\".",
    settings_load_in_recording: "Load in Recording",

    start_monitoring: "Start Monitoring",
    stop_monitoring: "Stop Monitoring",
    sample_rate_label: "SR:",
    buffer_label: "Buffer:",
    mute_output: "Mute Output",
    record_wav: "Record WAV",

    input_device_heading: "Input Device (Mic / Instrument)",
    output_device_heading: "Output Device (Headphones / Speakers)",
    refresh: "Refresh",
    none: "None",
    input_gain_db: "Input Gain (dB)",
    output_gain_db: "Output Gain (dB)",
    meter_in: "In",
    meter_out: "Out",

    vst3_slot_heading: "VST Effect Slot (VST2 / VST3)",
    scanning: "Scanning...",
    scan_installed_vst3: "Scan VSTs",
    open_vst_folder_btn: "Open VST Folder",
    open_bundle: "Open VST Folder...",
    open_file: "Open File (.vst3 / .dll / .so)...",
    discovered: "Discovered:",
    select_plugin: "Select plugin",
    load_selected: "Load Selected",
    path_label: "Path:",
    path_hint: "vst_plugins/MyEffect.vst3 or MyPlugin.dll",
    browse: "Browse...",
    folder: "Folder...",
    load_path: "Load Path",
    bypass_effect: "Bypass Effect",
    dry_wet_percent: "Dry/Wet %",
    open_native_gui: "Open Native GUI",
    close_native_gui: "Close Native GUI",
    refresh_params: "Refresh Params",
    unload: "Unload",
    parameters: "Parameters:",
    filter_params_hint: "Filter parameters...",
    no_plugin_loaded: "No VST plugin loaded. You can still start monitoring to hear clean input, or load a VST2 (.dll/.so) or VST3 (.vst3) effect above.",

    status_ready: "Ready. Select input/output devices and press Start Monitoring.",
    status_stopped: "Monitoring stopped.",
    status_scanning: "Scanning VST2/VST3 plugin directories...",
    status_unloaded: "Plugin unloaded (clean monitoring active).",
    error_prefix: "Error:",
    error_select_io: "Please select both an input and an output device.",
    error_start_before_rec: "Start audio monitoring before recording.",
    dismiss: "Dismiss",

    dialog_wav_filter: "WAV Audio",
    dialog_vst3_filter: "VST2 / VST3 Plugin (.vst3, .dll, .so)",
    dialog_select_bundle: "Select VST bundle or folder",
    dialog_select_plugin: "Select VST2 / VST3 plugin",
    dialog_select_vst_folder: "Select VST plugins folder",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn switches_between_spanish_and_english_strings() {
        assert_eq!(Lang::Es.text().nav_recording, "Grabación");
        assert_eq!(Lang::Es.text().nav_news, "Novedades");
        assert_eq!(Lang::En.text().nav_recording, "Recording");
        assert_eq!(Lang::En.text().nav_news, "What's New");
    }

    #[test]
    fn formats_dynamic_messages_per_language() {
        assert!(Lang::Es.scan_complete_status(3).contains("3 plugin(s)"));
        assert!(Lang::En.scan_complete_status(3).contains("found 3 plugin(s)"));
        assert!(!Lang::Es.text().news_items.is_empty());
        assert_eq!(Lang::Es.text().news_items.len(), Lang::En.text().news_items.len());
    }
}
