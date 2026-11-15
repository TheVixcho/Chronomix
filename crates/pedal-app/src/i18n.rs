use std::path::{Path, PathBuf};

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

    pub fn mixer_saved_session(self, dir: &Path, tracks: usize) -> String {
        match self {
            Self::Es => format!("Sesión multipista guardada ({tracks} pistas WAV) en {}", dir.display()),
            Self::En => format!("Saved multitrack session ({tracks} WAV tracks) to {}", dir.display()),
        }
    }

    pub fn mixer_assigned_input(self, ch_name: &str, src: &str) -> String {
        match self {
            Self::Es => format!("{ch_name}: entrada asignada -> {src}"),
            Self::En => format!("{ch_name}: input assigned -> {src}"),
        }
    }
}

/// Static localized UI copy for all panels and controls.
pub struct Strings {
    pub nav_news: &'static str,
    pub nav_recording: &'static str,
    pub nav_mixer: &'static str,
    pub nav_settings: &'static str,
    pub nav_live_badge: &'static str,
    pub nav_rec_badge: &'static str,
    pub lang_label: &'static str,
    pub mixer_power_on: &'static str,
    pub mixer_power_off: &'static str,
    pub mixer_rec_start: &'static str,
    pub mixer_rec_stop: &'static str,
    pub mixer_rec_mix_toggle: &'static str,
    pub mixer_unassigned: &'static str,
    pub mixer_right_click_title: &'static str,
    pub mixer_disconnect_input: &'static str,
    pub mixer_channel_color: &'static str,
    pub mixer_tab_home: &'static str,
    pub mixer_tab_source: &'static str,
    pub mixer_tab_eq: &'static str,
    pub mixer_tab_dyn: &'static str,
    pub mixer_tab_vst: &'static str,
    pub mixer_tab_io: &'static str,
    pub mixer_upper_vst_title: &'static str,
    pub mixer_add_vst: &'static str,
    pub mixer_browse_vst: &'static str,
    pub mixer_no_vsts_scanned: &'static str,
    pub mixer_clear_solo: &'static str,
    pub mixer_add_channel: &'static str,
    pub mixer_add_media: &'static str,
    pub mixer_remove_channel: &'static str,
    pub mixer_remove_this_channel: &'static str,
    pub mixer_bank_title: &'static str,
    pub mixer_media_tag: &'static str,
    pub mixer_media_load: &'static str,
    pub mixer_media_no_track: &'static str,
    pub mixer_media_loading: &'static str,
    pub mixer_media_play: &'static str,
    pub mixer_media_pause: &'static str,
    pub mixer_media_stop: &'static str,
    pub mixer_media_loop: &'static str,
    pub mixer_media_back_10s: &'static str,
    pub mixer_media_fwd_10s: &'static str,
    pub mixer_media_convert: &'static str,
    pub mixer_media_to_audio: &'static str,
    pub mixer_playback_section_title: &'static str,
    pub mixer_playback_play_all: &'static str,
    pub mixer_playback_pause_all: &'static str,
    pub mixer_playback_stop_all: &'static str,
    pub mixer_playback_sync_all: &'static str,
    pub mixer_rec_armed_count: &'static str,
    pub mixer_rec_only_armed_note: &'static str,
    pub mixer_rec_tooltip: &'static str,
    pub mixer_output_label: &'static str,
    pub mixer_trim_label: &'static str,
    pub mixer_balance_label: &'static str,
    pub mixer_width_label: &'static str,
    pub mixer_lpf_label: &'static str,
    pub mixer_swap_lr: &'static str,
    pub mixer_solo_safe: &'static str,
    pub mixer_fx_bypass: &'static str,
    pub mixer_pan_label: &'static str,
    pub mixer_pre_fader_rec: &'static str,
    pub mixer_channel_name: &'static str,
    pub mixer_icon_label: &'static str,
    pub mixer_icon_custom: &'static str,
    pub mixer_vmic_title: &'static str,
    pub mixer_vmic_desc: &'static str,
    pub mixer_vmic_toggle_on: &'static str,
    pub mixer_vmic_toggle_off: &'static str,
    pub mixer_vmic_gain: &'static str,
    pub mixer_vmic_send_ch: &'static str,
    pub mixer_armed_tracks: &'static str,
    pub mixer_no_armed_tracks: &'static str,
    pub mixer_select_slot_params: &'static str,
    pub mixer_bank_deck_title: &'static str,
    pub mixer_bank_ch1_16: &'static str,
    pub mixer_bank_ch17_32: &'static str,
    pub mixer_bank_ch33_48: &'static str,
    pub mixer_bank_multimedia: &'static str,
    pub mixer_bank_all: &'static str,
    pub mixer_bank_empty_title: &'static str,
    pub mixer_bank_empty_desc: &'static str,
    pub mixer_bank_enable_section: &'static str,
    pub mixer_bank_add_channel_here: &'static str,

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
    pub settings_wine_heading: &'static str,
    pub settings_wine_desc: &'static str,
    pub settings_wine_runner_label: &'static str,
    pub settings_wine_prefix_label: &'static str,
    pub settings_wine_auto_label: &'static str,
    pub settings_wine_auto_display: &'static str,
    pub settings_wine_session_label: &'static str,
    pub settings_wine_status_ready: &'static str,
    pub settings_wine_status_uninit: &'static str,
    pub settings_wine_status_missing: &'static str,
    pub settings_wine_driver_ok: &'static str,
    pub settings_wine_driver_pending: &'static str,
    pub settings_wine_bridge_prebuilt: &'static str,
    pub settings_wine_bridge_cached: &'static str,
    pub settings_wine_bridge_compile: &'static str,
    pub settings_wine_bridge_missing: &'static str,
    pub settings_wine_init_repair: &'static str,
    pub settings_wine_run_exe: &'static str,
    pub settings_wine_open_drive_c: &'static str,
    pub settings_wine_rebuild_bridge: &'static str,
    pub settings_wine_tools_label: &'static str,
    pub settings_wine_kill_server: &'static str,
    pub settings_wine_choose_prefix: &'static str,
    pub settings_wine_choose_runner: &'static str,
    pub settings_wine_reset_auto: &'static str,
    pub settings_wine_vst_paths_title: &'static str,
    pub settings_wine_busy_init: &'static str,
    pub settings_wine_busy_run: &'static str,
    pub settings_wine_busy_bridge: &'static str,
    pub settings_wine_busy_tool: &'static str,
    pub settings_wine_bridge_rebuilt_ok: &'static str,
    pub settings_wine_init_ok: &'static str,
    pub dialog_select_exe: &'static str,
    pub dialog_select_wine_prefix: &'static str,
    pub dialog_select_wine_runner: &'static str,

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
    nav_mixer: "Mixer",
    nav_settings: "Ajustes",
    nav_live_badge: "En vivo",
    nav_rec_badge: "Grabando",
    lang_label: "Idioma:",
    mixer_power_on: "MOTOR ON",
    mixer_power_off: "MOTOR OFF",
    mixer_rec_start: "GRABAR MULTI",
    mixer_rec_stop: "DETENER REC",
    mixer_rec_mix_toggle: "Opcional: incluir Master_Mix.wav (bus completo)",
    mixer_unassigned: "SIN ENTRADA",
    mixer_right_click_title: "Asignar entrada de audio:",
    mixer_disconnect_input: "Desconectar entrada",
    mixer_channel_color: "Color de tira:",
    mixer_tab_home: "HOME",
    mixer_tab_source: "FUENTE",
    mixer_tab_eq: "EQ",
    mixer_tab_dyn: "DINÁMICA",
    mixer_tab_vst: "RACK VST",
    mixer_tab_io: "SALIDA / REC",
    mixer_upper_vst_title: "RACK SUPERIOR VST (VST2 / VST3)",
    mixer_add_vst: "+ Añadir VST...",
    mixer_browse_vst: "Archivo...",
    mixer_no_vsts_scanned: "No hay VSTs escaneados. Pulsa Archivo... o escanea en Ajustes.",
    mixer_clear_solo: "CLR SOLO",
    mixer_add_channel: "+ Canal",
    mixer_add_media: "+ Multimedia",
    mixer_remove_channel: "- Canal",
    mixer_remove_this_channel: "Eliminar este canal",
    mixer_bank_title: "BANCO DE CANALES · SCROLL HORIZONTAL",
    mixer_media_tag: "MULTIMEDIA",
    mixer_media_load: "Cargar audio...",
    mixer_media_no_track: "Sin pista (clic para cargar MP3/WAV)",
    mixer_media_loading: "Decodificando audio...",
    mixer_media_play: "Reproducir",
    mixer_media_pause: "Pausar",
    mixer_media_stop: "Detener",
    mixer_media_loop: "Bucle",
    mixer_media_back_10s: "-10s",
    mixer_media_fwd_10s: "+10s",
    mixer_media_convert: "Canal Multimedia (Música de fondo)",
    mixer_media_to_audio: "Cambiar a entrada de audio",
    mixer_playback_section_title: "PLAYBACK MULTITRACK",
    mixer_playback_play_all: "REPRODUCIR TODO",
    mixer_playback_pause_all: "PAUSAR TODO",
    mixer_playback_stop_all: "DETENER TODO",
    mixer_playback_sync_all: "Playback global de pistas multimedia",
    mixer_rec_armed_count: "CANALES CON REC:",
    mixer_rec_only_armed_note: "Solo se graban los canales con REC activado. Los canales sin REC solo suenan en vivo.",
    mixer_rec_tooltip: "Activar REC para grabar este canal (sin REC no se graba)",
    mixer_output_label: "Salida Maestra:",
    mixer_trim_label: "TRIM (dB)",
    mixer_balance_label: "BALANCE",
    mixer_width_label: "ESTÉREO %",
    mixer_lpf_label: "HCUT (LPF)",
    mixer_swap_lr: "L↔R Swap",
    mixer_solo_safe: "Solo Safe",
    mixer_fx_bypass: "Bypass FX",
    mixer_pan_label: "PAN",
    mixer_pre_fader_rec: "Grabar Pre-Fader",
    mixer_channel_name: "Nombre:",
    mixer_icon_label: "Icono:",
    mixer_icon_custom: "Imagen...",
    mixer_vmic_title: "SALIDA DIGITAL · MICRÓFONO VIRTUAL (DISCORD / OBS)",
    mixer_vmic_desc: "Envía la mezcla digital como un micrófono virtual («Chronomix Virtual Mic (Discord/OBS)») para seleccionarlo en Discord, OBS, Zoom o navegador sin dejar de escuchar por tus audífonos.",
    mixer_vmic_toggle_on: "MIC VIRTUAL ON",
    mixer_vmic_toggle_off: "MIC VIRTUAL OFF",
    mixer_vmic_gain: "Nivel Mic Virtual:",
    mixer_vmic_send_ch: "Enviar a Mic Virtual (Discord/OBS)",
    mixer_armed_tracks: "Pistas armadas (REC):",
    mixer_no_armed_tracks: "Ningún canal tiene REC encendido. Pulsa REC en los canales que quieras grabar.",
    mixer_select_slot_params: "Parámetros del VST:",
    mixer_bank_deck_title: "CAPAS X32",
    mixer_bank_ch1_16: "CH 1-16",
    mixer_bank_ch17_32: "CH 17-32",
    mixer_bank_ch33_48: "CH 33-48",
    mixer_bank_multimedia: "MULTIMEDIA",
    mixer_bank_all: "TODOS",
    mixer_bank_empty_title: "SECCIÓN SIN CANALES",
    mixer_bank_empty_desc: "Esta sección no tiene canales creados todavía.",
    mixer_bank_enable_section: "+ Habilitar sección",
    mixer_bank_add_channel_here: "+ Agregar canal aquí",

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
    settings_wine_heading: "Compatibilidad Wine y Ejecutor Windows (.exe / .msi / VST)",
    settings_wine_desc: "Chronomix detecta tus prefijos (~/.wine, Chronomix, Bottles, Lutris, Heroic) y configura Wayland/X11 automáticamente para abrir instaladores .exe/.msi y plugins VST de Windows sin usar la terminal.",
    settings_wine_runner_label: "Binario Wine:",
    settings_wine_prefix_label: "Carpeta Wine (WINEPREFIX):",
    settings_wine_auto_label: "Automático (recomendado)",
    settings_wine_auto_display: "Configurar automáticamente pantalla Wayland/X11 (x11,wayland) y entorno gráfico",
    settings_wine_session_label: "Sesión gráfica:",
    settings_wine_status_ready: "Prefijo Wine listo",
    settings_wine_status_uninit: "Prefijo sin inicializar",
    settings_wine_status_missing: "Wine no instalado",
    settings_wine_driver_ok: "Driver Wayland+X11 OK",
    settings_wine_driver_pending: "Wayland+X11 pendiente",
    settings_wine_bridge_prebuilt: "Bridge VST: Precompilado",
    settings_wine_bridge_cached: "Bridge VST: Compilado en caché",
    settings_wine_bridge_compile: "Bridge VST: Compilable al vuelo",
    settings_wine_bridge_missing: "Bridge VST: Falta wineg++",
    settings_wine_init_repair: "Inicializar / Reparar Wine",
    settings_wine_run_exe: "Ejecutar .exe / .msi...",
    settings_wine_open_drive_c: "Abrir drive_c",
    settings_wine_rebuild_bridge: "Recompilar Bridge VST",
    settings_wine_tools_label: "Herramientas Wine:",
    settings_wine_kill_server: "Detener Wine (wineserver -k)",
    settings_wine_choose_prefix: "Elegir Prefijo...",
    settings_wine_choose_runner: "Elegir Binario...",
    settings_wine_reset_auto: "Usar Auto",
    settings_wine_vst_paths_title: "Rutas VST estándar dentro de tus prefijos Wine:",
    settings_wine_busy_init: "Inicializando y configurando prefijo Wine (Wayland/X11)...",
    settings_wine_busy_run: "Ejecutando aplicación Windows con Wine...",
    settings_wine_busy_bridge: "Compilando bridge Winelib (wine_host)...",
    settings_wine_busy_tool: "Ejecutando herramienta de Wine...",
    settings_wine_bridge_rebuilt_ok: "Bridge Wine VST compilado correctamente.",
    settings_wine_init_ok: "Prefijo Wine inicializado y listo.",
    dialog_select_exe: "Ejecutar instalador o aplicación Windows (.exe / .msi)",
    dialog_select_wine_prefix: "Seleccionar carpeta de prefijo Wine (WINEPREFIX)",
    dialog_select_wine_runner: "Seleccionar binario de Wine (wine / wine64)",

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
    nav_mixer: "Mixer",
    nav_settings: "Settings",
    nav_live_badge: "Live",
    nav_rec_badge: "Rec",
    lang_label: "Language:",
    mixer_power_on: "ENGINE ON",
    mixer_power_off: "ENGINE OFF",
    mixer_rec_start: "REC MULTI",
    mixer_rec_stop: "STOP REC",
    mixer_rec_mix_toggle: "Optional: also write Master_Mix.wav (full bus)",
    mixer_unassigned: "NO INPUT",
    mixer_right_click_title: "Assign audio input:",
    mixer_disconnect_input: "Disconnect input",
    mixer_channel_color: "Strip color:",
    mixer_tab_home: "HOME",
    mixer_tab_source: "SOURCE",
    mixer_tab_eq: "EQ",
    mixer_tab_dyn: "DYNAMICS",
    mixer_tab_vst: "VST RACK",
    mixer_tab_io: "OUTPUT / REC",
    mixer_upper_vst_title: "UPPER VST RACK (VST2 / VST3)",
    mixer_add_vst: "+ Add VST...",
    mixer_browse_vst: "File...",
    mixer_no_vsts_scanned: "No scanned VSTs yet. Click File... or scan in Settings.",
    mixer_clear_solo: "CLR SOLO",
    mixer_add_channel: "+ Ch",
    mixer_add_media: "+ Media",
    mixer_remove_channel: "- Ch",
    mixer_remove_this_channel: "Remove this channel",
    mixer_bank_title: "CHANNEL BANK · HORIZONTAL SCROLL",
    mixer_media_tag: "MULTIMEDIA",
    mixer_media_load: "Load audio...",
    mixer_media_no_track: "No track (click to load MP3/WAV)",
    mixer_media_loading: "Decoding audio...",
    mixer_media_play: "Play",
    mixer_media_pause: "Pause",
    mixer_media_stop: "Stop",
    mixer_media_loop: "Loop",
    mixer_media_back_10s: "-10s",
    mixer_media_fwd_10s: "+10s",
    mixer_media_convert: "Multimedia Channel (Background Music)",
    mixer_media_to_audio: "Switch to audio input",
    mixer_playback_section_title: "MULTITRACK PLAYBACK",
    mixer_playback_play_all: "PLAY ALL",
    mixer_playback_pause_all: "PAUSE ALL",
    mixer_playback_stop_all: "STOP ALL",
    mixer_playback_sync_all: "Global multimedia playback",
    mixer_rec_armed_count: "REC ARMED:",
    mixer_rec_only_armed_note: "Only REC-armed channels are recorded. Unarmed channels only monitor live.",
    mixer_rec_tooltip: "Arm REC to record this channel (when off, NOT recorded)",
    mixer_output_label: "Master Output:",
    mixer_trim_label: "TRIM (dB)",
    mixer_balance_label: "BALANCE",
    mixer_width_label: "STEREO %",
    mixer_lpf_label: "HCUT (LPF)",
    mixer_swap_lr: "L↔R Swap",
    mixer_solo_safe: "Solo Safe",
    mixer_fx_bypass: "Bypass FX",
    mixer_pan_label: "PAN",
    mixer_pre_fader_rec: "Record Pre-Fader",
    mixer_channel_name: "Name:",
    mixer_icon_label: "Icon:",
    mixer_icon_custom: "Image...",
    mixer_vmic_title: "DIGITAL OUTPUT · VIRTUAL MICROPHONE (DISCORD / OBS)",
    mixer_vmic_desc: "Exposes the digital mix as a virtual microphone (\"Chronomix Virtual Mic (Discord/OBS)\") selectable in Discord, OBS, Zoom, or browsers while still monitoring on your headphones.",
    mixer_vmic_toggle_on: "VIRTUAL MIC ON",
    mixer_vmic_toggle_off: "VIRTUAL MIC OFF",
    mixer_vmic_gain: "Virtual Mic Gain:",
    mixer_vmic_send_ch: "Send to Virtual Mic (Discord/OBS)",
    mixer_armed_tracks: "Armed tracks (REC):",
    mixer_no_armed_tracks: "No channels have REC armed. Press REC on the channels you want to record.",
    mixer_select_slot_params: "VST Parameters:",
    mixer_bank_deck_title: "X32 LAYERS",
    mixer_bank_ch1_16: "CH 1-16",
    mixer_bank_ch17_32: "CH 17-32",
    mixer_bank_ch33_48: "CH 33-48",
    mixer_bank_multimedia: "MULTIMEDIA",
    mixer_bank_all: "ALL",
    mixer_bank_empty_title: "EMPTY SECTION",
    mixer_bank_empty_desc: "This section has no channels created yet.",
    mixer_bank_enable_section: "+ Enable section",
    mixer_bank_add_channel_here: "+ Add channel here",

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
    settings_wine_heading: "Wine Compatibility & Windows App Launcher (.exe / .msi / VST)",
    settings_wine_desc: "Chronomix discovers your Wine prefixes (~/.wine, Chronomix, Bottles, Lutris, Heroic) and configures Wayland/X11 automatically so you can run .exe/.msi installers and Windows VSTs right from the app.",
    settings_wine_runner_label: "Wine Binary:",
    settings_wine_prefix_label: "Wine Prefix (WINEPREFIX):",
    settings_wine_auto_label: "Automatic (recommended)",
    settings_wine_auto_display: "Auto-configure Wayland/X11 display drivers (x11,wayland) and session env",
    settings_wine_session_label: "Display session:",
    settings_wine_status_ready: "Wine Prefix Ready",
    settings_wine_status_uninit: "Prefix Not Initialized",
    settings_wine_status_missing: "Wine Not Installed",
    settings_wine_driver_ok: "Wayland+X11 Driver OK",
    settings_wine_driver_pending: "Wayland+X11 Pending",
    settings_wine_bridge_prebuilt: "VST Bridge: Prebuilt",
    settings_wine_bridge_cached: "VST Bridge: Runtime Cached",
    settings_wine_bridge_compile: "VST Bridge: Runtime Compilable",
    settings_wine_bridge_missing: "VST Bridge: wineg++ missing",
    settings_wine_init_repair: "Initialize / Repair Wine",
    settings_wine_run_exe: "Run .exe / .msi...",
    settings_wine_open_drive_c: "Open drive_c",
    settings_wine_rebuild_bridge: "Rebuild VST Bridge",
    settings_wine_tools_label: "Wine Tools:",
    settings_wine_kill_server: "Stop Wine (wineserver -k)",
    settings_wine_choose_prefix: "Choose Prefix...",
    settings_wine_choose_runner: "Choose Binary...",
    settings_wine_reset_auto: "Use Auto",
    settings_wine_vst_paths_title: "Standard VST folders inside your Wine prefixes:",
    settings_wine_busy_init: "Initializing and configuring Wine prefix (Wayland/X11)...",
    settings_wine_busy_run: "Running Windows application with Wine...",
    settings_wine_busy_bridge: "Compiling Winelib VST bridge (wine_host)...",
    settings_wine_busy_tool: "Running Wine utility...",
    settings_wine_bridge_rebuilt_ok: "Wine VST bridge compiled successfully.",
    settings_wine_init_ok: "Wine prefix initialized and ready.",
    dialog_select_exe: "Run Windows installer or executable (.exe / .msi)",
    dialog_select_wine_prefix: "Select Wine prefix folder (WINEPREFIX)",
    dialog_select_wine_runner: "Select Wine binary (wine / wine64)",

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
