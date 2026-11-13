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
            Self::Es => format!("Escaneo VST3 completado: {count} plugin(s) encontrado(s)."),
            Self::En => format!("VST3 scan complete: found {count} plugin(s)."),
        }
    }

    pub fn loaded_vst3_status(self, name: &str, param_count: usize) -> String {
        match self {
            Self::Es => format!("VST3 cargado: {name} ({param_count} parámetros)"),
            Self::En => format!("Loaded VST3: {name} ({param_count} parameters)"),
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
    pub nav_live_badge: &'static str,
    pub nav_rec_badge: &'static str,
    pub lang_label: &'static str,

    pub news_hero_subtitle: &'static str,
    pub news_hero_description: &'static str,
    pub news_go_recording: &'static str,
    pub news_section_title: &'static str,
    pub news_items: &'static [NewsItem],

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
}

pub struct NewsItem {
    pub tag: &'static str,
    pub title: &'static str,
    pub summary: &'static str,
}

static ES_NEWS: &[NewsItem] = &[
    NewsItem {
        tag: "Nuevo",
        title: "Panel de inicio, barra superior e idioma ES / EN",
        summary: "Chronomix inicia ahora con esta vista de Novedades, una barra superior fija para navegar al panel de Grabación y selector de idioma en vivo entre Español e Inglés.",
    },
    NewsItem {
        tag: "Audio + Wine",
        title: "Puente integrado para VST3 de Windows (x64) en Linux",
        summary: "Carga plugins .vst3 nativos de Linux y plugins VST3 de Windows de 64 bits mediante Wine con procesamiento en tiempo real y apertura de ventana nativa HWND.",
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
        title: "Start screen, top toolbar & ES / EN localization",
        summary: "Chronomix now opens with this What's New view, a persistent top toolbar to jump into the Recording workspace, and live Spanish / English switching.",
    },
    NewsItem {
        tag: "Audio + Wine",
        title: "Integrated Windows (x64) VST3 bridge on Linux",
        summary: "Load native Linux .vst3 plugins and 64-bit Windows VST3 plugins via Wine with real-time DSP processing and native HWND plugin editor windows.",
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
    nav_live_badge: "En vivo",
    nav_rec_badge: "Grabando",
    lang_label: "Idioma:",

    news_hero_subtitle: "DAW de audio en tiempo real y host VST3",
    news_hero_description: "Conecta tu interfaz o micrófono USB, procesa efectos VST3 con baja latencia y graba directamente a WAV. Usa el botón Grabación de la barra superior para abrir el estudio.",
    news_go_recording: "Ir a Grabación",
    news_section_title: "Novedades",
    news_items: ES_NEWS,

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

    vst3_slot_heading: "Ranura de Efecto VST3",
    scanning: "Escaneando...",
    scan_installed_vst3: "Escanear VST3 Instalados",
    open_bundle: "Abrir Carpeta .vst3...",
    open_file: "Abrir Archivo .vst3...",
    discovered: "Encontrados:",
    select_plugin: "Seleccionar plugin",
    load_selected: "Cargar Seleccionado",
    path_label: "Ruta:",
    path_hint: "/usr/lib/vst3/MiEfecto.vst3 o /home/.../Plugin.vst3",
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
    no_plugin_loaded: "No hay ningún plugin VST3 cargado. Puedes iniciar el monitoreo para escuchar la señal limpia o cargar un efecto .vst3 arriba.",

    status_ready: "Listo. Selecciona dispositivos de entrada/salida y pulsa Iniciar Monitoreo.",
    status_stopped: "Monitoreo detenido.",
    status_scanning: "Escaneando directorios estándar de VST3...",
    status_unloaded: "Plugin descargado (monitoreo limpio activo).",
    error_prefix: "Error:",
    error_select_io: "Selecciona un dispositivo de entrada y otro de salida.",
    error_start_before_rec: "Inicia el monitoreo de audio antes de grabar.",
    dismiss: "Cerrar",

    dialog_wav_filter: "Audio WAV",
    dialog_vst3_filter: "Plugin VST3",
    dialog_select_bundle: "Seleccionar paquete o carpeta VST3",
    dialog_select_plugin: "Seleccionar plugin VST3",
};

static EN_STRINGS: Strings = Strings {
    nav_news: "What's New",
    nav_recording: "Recording",
    nav_live_badge: "Live",
    nav_rec_badge: "Rec",
    lang_label: "Language:",

    news_hero_subtitle: "Real-Time Audio DAW & VST3 Host",
    news_hero_description: "Connect your audio interface or USB mic, process VST3 effects at low latency, and capture straight to WAV. Click Recording in the top toolbar to open the audio workspace.",
    news_go_recording: "Open Recording",
    news_section_title: "What's New",
    news_items: EN_NEWS,

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

    vst3_slot_heading: "VST3 Effect Slot",
    scanning: "Scanning...",
    scan_installed_vst3: "Scan Installed VST3s",
    open_bundle: "Open .vst3 Bundle...",
    open_file: "Open .vst3 File...",
    discovered: "Discovered:",
    select_plugin: "Select plugin",
    load_selected: "Load Selected",
    path_label: "Path:",
    path_hint: "/usr/lib/vst3/MyEffect.vst3 or /home/.../Plugin.vst3",
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
    no_plugin_loaded: "No VST3 plugin loaded. You can still start monitoring to hear clean input, or load a .vst3 effect above.",

    status_ready: "Ready. Select input/output devices and press Start Monitoring.",
    status_stopped: "Monitoring stopped.",
    status_scanning: "Scanning standard VST3 directories...",
    status_unloaded: "Plugin unloaded (clean monitoring active).",
    error_prefix: "Error:",
    error_select_io: "Please select both an input and an output device.",
    error_start_before_rec: "Start audio monitoring before recording.",
    dismiss: "Dismiss",

    dialog_wav_filter: "WAV Audio",
    dialog_vst3_filter: "VST3 Plugin",
    dialog_select_bundle: "Select VST3 bundle or folder",
    dialog_select_plugin: "Select VST3 plugin",
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
