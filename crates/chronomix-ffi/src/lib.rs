use std::{
    ffi::{CStr, CString},
    fs,
    os::raw::c_char,
    path::{Path, PathBuf},
    slice,
    sync::atomic::Ordering,
    time::{Instant, UNIX_EPOCH},
};

use pedal_audio::AudioDeviceInfo;
use pedal_engine::{
    AppPaths, ChannelDspConfig, ChannelInputMode, ChannelInputSource, CrossfaderCurve,
    DJ_HOT_CUES, DJ_WAVEFORM_BINS, DjDeckParams, DjMixerParams, DspPreset, MAX_MIXER_CHANNELS,
    MAX_MIXER_INSERTS, MediaBuffer, MidiStudioState, MixerEngine, MixerInsertTarget,
    PIANO_ROLL_STEPS, PedalEngine, SynthPreset, WineAction, WineConfig, eval_channel_eq_db,
    initialize_wine_prefix, inspect_wine_environment, launch_wine_executable, launch_wine_tool,
    load_settings_json_with_migration, rebuild_wine_bridge, save_settings_json,
    synthesize_session_pattern, synthesize_single_note_preview, wine_bridge_runtime_warning,
};
use serde::{Deserialize, Serialize};
use vst3_host::PluginInfo;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CChannelMeter {
    pub in_left: f32,
    pub in_right: f32,
    pub out_left: f32,
    pub out_right: f32,
    pub comp_gr_db: f32,
    pub media_pos_frames: u64,
    pub media_playing: u8,
    pub _pad: [u8; 7],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct CMixerTelemetry {
    pub channel_count: u32,
    pub master_left: f32,
    pub master_right: f32,
    pub virtual_mic_left: f32,
    pub virtual_mic_right: f32,
    pub is_virtual_mic_active: u8,
    pub is_recording: u8,
    pub is_audio_running: u8,
    pub _pad0: u8,
    pub recorded_frames: u64,
    pub underruns: u64,
    pub overruns: u64,
    pub sample_rate: u32,
    pub block_size: u32,
    pub channels: [CChannelMeter; 48],
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CDjDeckParams {
    pub gain_db: f32,
    pub volume: f32,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub kill_low: u8,
    pub kill_mid: u8,
    pub kill_high: u8,
    pub pfl_cue: u8,
    pub filter: f32,
    pub tempo_percent: f32,
    pub loop_enabled: u8,
    pub _pad: [u8; 3],
    pub loop_beats: f32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CDjMixerParams {
    pub crossfader: f32,
    pub curve: u8,
    pub send_to_master: u8,
    pub _pad: [u8; 2],
    pub master_volume: f32,
    pub cue_mix: f32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CDjDeckTelemetry {
    pub pos_frames: u64,
    pub total_frames: u64,
    pub sample_rate: u32,
    pub base_bpm: f32,
    pub effective_bpm: f32,
    pub peak_l: f32,
    pub peak_r: f32,
    pub cue_frame: f64,
    pub loop_start_frame: f64,
    pub loop_end_frame: f64,
    pub hot_cues: [f64; 4],
    pub hot_cue_set: [u8; 4],
    pub playing: u8,
    pub loop_enabled: u8,
    pub has_track: u8,
    pub _pad: u8,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CDjTelemetry {
    pub decks: [CDjDeckTelemetry; 2],
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub crossfader: f32,
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct CWaveformBin {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    pub peak: f32,
}

fn default_true() -> bool {
    true
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedSettings {
    #[serde(default)]
    custom_vst_dir: Option<String>,
    #[serde(default)]
    include_system_vst: bool,
    #[serde(default)]
    lang: String,
    #[serde(default)]
    wine_prefix: Option<String>,
    #[serde(default)]
    wine_binary: Option<String>,
    #[serde(default = "default_true")]
    wine_auto_display: bool,
    #[serde(default)]
    recording_dir: Option<String>,
}

impl Default for PersistedSettings {
    fn default() -> Self {
        Self {
            custom_vst_dir: None,
            include_system_vst: false,
            lang: "ES".to_string(),
            wine_prefix: None,
            wine_binary: None,
            wine_auto_display: true,
            recording_dir: None,
        }
    }
}

pub struct ChronomixAppHandle {
    paths: AppPaths,
    settings: PersistedSettings,
    wine_config: WineConfig,
    engine: PedalEngine,
    mixer: MixerEngine,
    midi: MidiStudioState,
    input_devices: Vec<AudioDeviceInfo>,
    output_devices: Vec<AudioDeviceInfo>,
    default_input_id: String,
    default_output_id: String,
    preferred_sample_rate: u32,
    preferred_block_size: usize,
    discovered_plugins: Vec<PluginInfo>,
    last_error: Option<String>,
    status_text: String,
}

impl ChronomixAppHandle {
    fn new() -> Self {
        let _ = env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
            .try_init();
        let paths = AppPaths::resolve();
        let mut settings: PersistedSettings = load_settings_json_with_migration(&paths)
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default();
        if settings.lang.is_empty() {
            settings.lang = "ES".to_string();
        }
        let wine_config = WineConfig {
            wine_binary: settings
                .wine_binary
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from),
            wine_prefix: settings
                .wine_prefix
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from),
            auto_display: settings.wine_auto_display,
        };

        let mut engine = PedalEngine::new();
        engine.set_wine_config(wine_config.clone());
        let mut mixer = MixerEngine::new();
        mixer.set_wine_config(wine_config.clone());

        let input_devices = mixer.list_input_devices().unwrap_or_default();
        let output_devices = mixer.list_output_devices().unwrap_or_default();
        let default_input_id =
            pick_best_default_output(&input_devices, pedal_audio::default_input_device_id());
        let default_output_id =
            pick_best_default_output(&output_devices, mixer.default_output_device());

        let active_vst_dir = settings
            .custom_vst_dir
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| paths.vst_plugins_dir());
        let discovered_plugins = engine.discover_plugins(&active_vst_dir, settings.include_system_vst);

        Self {
            paths,
            settings,
            wine_config,
            engine,
            mixer,
            midi: MidiStudioState::new(),
            input_devices,
            output_devices,
            default_input_id,
            default_output_id,
            preferred_sample_rate: 48_000,
            preferred_block_size: 256,
            discovered_plugins,
            last_error: None,
            status_text: "Listo — Motor de audio Chronomix (JUCE + Rust)".to_string(),
        }
    }

    fn ensure_audio_running(&mut self) {
        if self.mixer.is_audio_running() {
            return;
        }
        let out_id = if !self.default_output_id.is_empty() {
            self.default_output_id.clone()
        } else {
            self.output_devices
                .first()
                .map(|d| d.id.clone())
                .unwrap_or_default()
        };
        if out_id.is_empty() {
            return;
        }
        let sr = self.preferred_sample_rate.max(44_100);
        let bs = self.preferred_block_size.max(64);
        if let Err(e) = self.mixer.start_audio(&out_id, sr, bs) {
            self.last_error = Some(e.to_string());
        } else {
            self.status_text = format!(
                "Audio en vivo ({} Hz / {} muestras)",
                self.mixer.sample_rate(),
                self.mixer.block_size()
            );
        }
    }

    fn sync_active_midi_session(&mut self, force: bool) {
        let Some(sess) = self.midi.active_session() else {
            return;
        };
        let needs_channel = sess
            .channel_index
            .map(|idx| idx >= self.mixer.channels().len())
            .unwrap_or(true);
        let name = sess.name.clone();
        if needs_channel {
            if let Some(ch_idx) = self.mixer.add_midi_channel(&name) {
                if let Some(s) = self.midi.active_session_mut() {
                    s.channel_index = Some(ch_idx);
                    s.last_synced_version = 0;
                }
            }
        }
        let sr = self.mixer.sample_rate();
        let Some(sess) = self.midi.active_session_mut() else {
            return;
        };
        let Some(ch_idx) = sess.channel_index else {
            return;
        };
        if force || sess.last_synced_version != sess.edit_version {
            let buf = synthesize_session_pattern(sess, sr);
            let playing = sess.is_playing;
            let title = sess.name.clone();
            sess.last_synced_version = sess.edit_version;
            let out = self.default_output_id.clone();
            let _ = self
                .mixer
                .install_custom_media_buffer(ch_idx, title, buf, playing, Some(&out));
        }
    }

    fn save_settings(&mut self) {
        self.wine_config = WineConfig {
            wine_binary: self
                .settings
                .wine_binary
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from),
            wine_prefix: self
                .settings
                .wine_prefix
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .map(PathBuf::from),
            auto_display: self.settings.wine_auto_display,
        };
        self.engine.set_wine_config(self.wine_config.clone());
        self.mixer.set_wine_config(self.wine_config.clone());
        if let Ok(json) = serde_json::to_string_pretty(&self.settings) {
            let _ = save_settings_json(&self.paths, &json);
        }
    }
}

fn pick_best_default_output(devices: &[AudioDeviceInfo], reported: Option<String>) -> String {
    if let Some(ref def_id) = reported {
        if devices.iter().any(|d| &d.id == def_id) {
            return def_id.clone();
        }
    }
    if let Some(d) = devices
        .iter()
        .find(|d| d.name.contains("System Default") || d.id.ends_with(":default"))
    {
        return d.id.clone();
    }
    devices.first().map(|d| d.id.clone()).unwrap_or_default()
}

fn to_c_string(s: String) -> *mut c_char {
    let clean = s.replace('\0', "");
    CString::new(clean)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

fn c_str_to_string(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

fn parse_target(is_master: u8, ch: u32, slot: u32) -> MixerInsertTarget {
    let s = (slot as usize).min(MAX_MIXER_INSERTS.saturating_sub(1));
    if is_master != 0 {
        MixerInsertTarget::Master { slot: s }
    } else {
        MixerInsertTarget::Channel {
            ch: ch as usize,
            slot: s,
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_create() -> *mut ChronomixAppHandle {
    Box::into_raw(Box::new(ChronomixAppHandle::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_destroy(handle: *mut ChronomixAppHandle) {
    if !handle.is_null() {
        unsafe {
            let mut app = Box::from_raw(handle);
            app.mixer.stop_audio();
            app.engine.stop_audio();
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            let _ = CString::from_raw(ptr);
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_tick(handle: *mut ChronomixAppHandle) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    let out_id = app.default_output_id.clone();
    for (_ch, res) in app.mixer.poll_media_loads(Some(&out_id)) {
        match res {
            Ok(title) => app.status_text = format!("Cargado: {title}"),
            Err(e) => app.last_error = Some(e),
        }
    }
    app.mixer.service_all_plugin_windows();

    for sess in &mut app.midi.sessions {
        if sess.is_playing {
            let step_dur = 60.0 / (sess.bpm.clamp(40.0, 240.0) * 4.0);
            if let Some(start) = sess.play_start {
                let elapsed = start.elapsed().as_secs_f32();
                let pos = (elapsed / step_dur) % PIANO_ROLL_STEPS as f32;
                sess.playhead_pos_steps = pos;
                sess.current_step = (pos.floor() as usize) % PIANO_ROLL_STEPS;
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_paths_json(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("{}".into());
    };
    let rec_dir = app
        .settings
        .recording_dir
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app.paths.recordings_dir());
    let vst_dir = app
        .settings
        .custom_vst_dir
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app.paths.vst_plugins_dir());
    let val = serde_json::json!({
        "config_dir": app.paths.config_dir.display().to_string(),
        "data_dir": app.paths.data_dir.display().to_string(),
        "cache_dir": app.paths.cache_dir.display().to_string(),
        "vst_dir": vst_dir.display().to_string(),
        "default_vst_dir": app.paths.vst_plugins_dir().display().to_string(),
        "recordings_dir": rec_dir.display().to_string(),
        "profiles_dir": app.paths.profiles_dir().display().to_string(),
        "window_state_file": app.paths.window_state_file().display().to_string(),
    });
    to_c_string(val.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_settings_json(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("{}".into());
    };
    to_c_string(serde_json::to_string(&app.settings).unwrap_or_else(|_| "{}".into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_update_settings_json(
    handle: *mut ChronomixAppHandle,
    json_str: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let raw = c_str_to_string(json_str);
    match serde_json::from_str::<PersistedSettings>(&raw) {
        Ok(next) => {
            app.settings = next;
            app.save_settings();
            1
        }
        Err(e) => {
            app.last_error = Some(format!("Ajustes inválidos: {e}"));
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_take_last_error(handle: *mut ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return to_c_string(String::new());
    };
    to_c_string(app.last_error.take().unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_status_text(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string(String::new());
    };
    to_c_string(app.status_text.clone())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_wine_warning(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string(String::new());
    };
    to_c_string(wine_bridge_runtime_warning(&app.wine_config).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_list_devices_json(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("{}".into());
    };
    let map_dev = |d: &AudioDeviceInfo| {
        serde_json::json!({
            "id": d.id,
            "name": d.name,
            "channels": d.default_channels,
            "default_sample_rate": d.default_sample_rate,
        })
    };
    let val = serde_json::json!({
        "inputs": app.input_devices.iter().map(map_dev).collect::<Vec<_>>(),
        "outputs": app.output_devices.iter().map(map_dev).collect::<Vec<_>>(),
        "default_input_id": app.default_input_id,
        "default_output_id": app.default_output_id,
    });
    to_c_string(val.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_refresh_devices(handle: *mut ChronomixAppHandle) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    app.input_devices = app.mixer.list_input_devices().unwrap_or_default();
    app.output_devices = app.mixer.list_output_devices().unwrap_or_default();
    app.default_input_id =
        pick_best_default_output(&app.input_devices, pedal_audio::default_input_device_id());
    if app.default_output_id.is_empty()
        || !app.output_devices.iter().any(|d| d.id == app.default_output_id)
    {
        app.default_output_id =
            pick_best_default_output(&app.output_devices, app.mixer.default_output_device());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_output_device(
    handle: *mut ChronomixAppHandle,
    output_device_id: *const c_char,
    sample_rate: u32,
    block_size: u32,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let out_id = c_str_to_string(output_device_id);
    if !out_id.is_empty() {
        app.default_output_id = out_id.clone();
    }
    if sample_rate >= 8_000 {
        app.preferred_sample_rate = sample_rate;
    }
    if block_size >= 64 {
        app.preferred_block_size = block_size as usize;
    }
    let chosen_id = app.default_output_id.clone();
    let chosen_name = app
        .output_devices
        .iter()
        .find(|d| d.id == chosen_id)
        .map(|d| d.name.clone())
        .unwrap_or_else(|| chosen_id.clone());

    if app.mixer.is_audio_running() && !chosen_id.is_empty() {
        let sr = app.preferred_sample_rate;
        let bs = app.preferred_block_size;
        match app.mixer.start_audio(&chosen_id, sr, bs) {
            Ok(()) => {
                app.status_text = format!(
                    "Salida activa: {} ({} Hz / {} smp)",
                    chosen_name,
                    app.mixer.sample_rate(),
                    app.mixer.block_size()
                );
                1
            }
            Err(e) => {
                app.last_error = Some(format!("Error al cambiar salida: {e}"));
                0
            }
        }
    } else {
        app.status_text = format!("Salida seleccionada: {chosen_name}");
        1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_start_audio(
    handle: *mut ChronomixAppHandle,
    output_device_id: *const c_char,
    sample_rate: u32,
    block_size: u32,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let mut out_id = c_str_to_string(output_device_id);
    if out_id.is_empty() {
        out_id = app.default_output_id.clone();
    } else {
        app.default_output_id = out_id.clone();
    }
    let sr = if sample_rate == 0 {
        app.preferred_sample_rate
    } else {
        app.preferred_sample_rate = sample_rate;
        sample_rate
    };
    let bs = if block_size == 0 {
        app.preferred_block_size
    } else {
        app.preferred_block_size = block_size as usize;
        block_size as usize
    };
    match app.mixer.start_audio(&out_id, sr, bs) {
        Ok(()) => {
            app.status_text = format!(
                "Audio en vivo ({} Hz / {} muestras)",
                app.mixer.sample_rate(),
                app.mixer.block_size()
            );
            1
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_stop_audio(handle: *mut ChronomixAppHandle) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.stop_audio();
        app.status_text = "Motor de audio detenido".to_string();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_toggle_virtual_mic(
    handle: *mut ChronomixAppHandle,
    enable: u8,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    if enable != 0 {
        app.ensure_audio_running();
        let out = app.default_output_id.clone();
        match app.mixer.enable_virtual_mic(Some(&out)) {
            Ok(msg) => {
                app.status_text = msg;
                1
            }
            Err(e) => {
                app.last_error = Some(e.to_string());
                0
            }
        }
    } else {
        app.mixer.disable_virtual_mic();
        app.status_text = "Micrófono Virtual desactivado".to_string();
        1
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_virtual_mic_gain_db(
    handle: *mut ChronomixAppHandle,
    gain_db: f32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.set_virtual_mic_gain_db(gain_db);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_start_recording(
    handle: *mut ChronomixAppHandle,
    base_dir: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    app.ensure_audio_running();
    let dir_str = c_str_to_string(base_dir);
    let dir = if !dir_str.trim().is_empty() {
        PathBuf::from(dir_str)
    } else if let Some(ref cfg_dir) = app.settings.recording_dir {
        if !cfg_dir.trim().is_empty() {
            PathBuf::from(cfg_dir)
        } else {
            app.paths.recordings_dir()
        }
    } else {
        app.paths.recordings_dir()
    };
    let _ = fs::create_dir_all(&dir);
    match app.mixer.start_recording(dir) {
        Ok(session_dir) => {
            app.status_text = format!("Grabando en {}", session_dir.display());
            1
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_stop_recording(handle: *mut ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return to_c_string(String::new());
    };
    match app.mixer.stop_recording() {
        Ok(Some(rep)) => {
            let msg = format!(
                "Grabación guardada en {} ({} archivos WAV)",
                rep.directory.display(),
                rep.files.len()
            );
            app.status_text = msg.clone();
            to_c_string(msg)
        }
        Ok(None) => to_c_string(String::new()),
        Err(e) => {
            app.last_error = Some(e.to_string());
            to_c_string(String::new())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_mixer_telemetry(
    handle: *mut ChronomixAppHandle,
    out_tel: *mut CMixerTelemetry,
) {
    let (Some(app), Some(out)) = (unsafe { handle.as_mut() }, unsafe { out_tel.as_mut() }) else {
        return;
    };
    let snap = app.mixer.take_telemetry();
    let (underruns, overruns) = app
        .mixer
        .audio_stats()
        .map(|s| {
            (
                s.output_underruns.load(Ordering::Relaxed),
                s.input_overruns.load(Ordering::Relaxed),
            )
        })
        .unwrap_or((0, 0));

    out.channel_count = snap.channels.len().min(MAX_MIXER_CHANNELS) as u32;
    out.master_left = snap.master_left;
    out.master_right = snap.master_right;
    out.virtual_mic_left = snap.virtual_mic_left;
    out.virtual_mic_right = snap.virtual_mic_right;
    out.is_virtual_mic_active = u8::from(snap.is_virtual_mic_active);
    out.is_recording = u8::from(snap.is_recording);
    out.is_audio_running = u8::from(app.mixer.is_audio_running());
    out.recorded_frames = snap.recorded_frames;
    out.underruns = underruns;
    out.overruns = overruns;
    out.sample_rate = app.mixer.sample_rate();
    out.block_size = app.mixer.block_size() as u32;

    for (i, ch_m) in snap.channels.iter().take(48).enumerate() {
        out.channels[i] = CChannelMeter {
            in_left: ch_m.in_left,
            in_right: ch_m.in_right,
            out_left: ch_m.out_left,
            out_right: ch_m.out_right,
            comp_gr_db: ch_m.comp_gr_db,
            media_pos_frames: ch_m.media_pos_frames,
            media_playing: u8::from(ch_m.media_playing),
            _pad: [0; 7],
        };
    }
}

fn serialize_insert_slot(slot: &pedal_engine::InsertSlotInfo, gui_open: bool) -> serde_json::Value {
    if let Some(ref meta) = slot.meta {
        let params: Vec<serde_json::Value> = meta
            .parameters
            .iter()
            .map(|p| {
                serde_json::json!({
                    "id": p.param.id,
                    "name": p.param.name,
                    "value": p.param.value,
                    "formatted": p.formatted,
                    "is_read_only": p.param.is_read_only,
                })
            })
            .collect();
        serde_json::json!({
            "loaded": true,
            "name": meta.info.name,
            "vendor": meta.info.vendor,
            "path": meta.info.path.display().to_string(),
            "is_wine_bridged": meta.is_wine_bridged,
            "has_gui": meta.info.has_gui,
            "gui_open": gui_open,
            "bypass": slot.bypass,
            "mix": slot.mix,
            "params": params,
        })
    } else {
        serde_json::json!({
            "loaded": false,
            "bypass": slot.bypass,
            "mix": slot.mix,
            "params": [],
        })
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_mixer_state_json(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("{}".into());
    };
    let channels: Vec<serde_json::Value> = app
        .mixer
        .channels()
        .iter()
        .enumerate()
        .map(|(ch_idx, ch)| {
            let mode_id: u8 = match ch.source.mode {
                ChannelInputMode::Unassigned => 0,
                ChannelInputMode::Stereo(2, 3) => 4,
                ChannelInputMode::Stereo(_, _) => 1,
                ChannelInputMode::Mono(0) => 2,
                ChannelInputMode::Mono(1) => 3,
                ChannelInputMode::Mono(2) => 6,
                ChannelInputMode::Mono(_) => 7,
                ChannelInputMode::Multimedia => 5,
            };
            let inserts: Vec<serde_json::Value> = ch
                .inserts
                .iter()
                .enumerate()
                .map(|(s_idx, sl)| {
                    let gui_open = app
                        .mixer
                        .is_insert_window_open(MixerInsertTarget::Channel { ch: ch_idx, slot: s_idx });
                    serialize_insert_slot(sl, gui_open)
                })
                .collect();
            serde_json::json!({
                "index": ch.index,
                "name": ch.name,
                "color": ch.color,
                "source_mode": mode_id,
                "source_device_id": ch.source.device_id,
                "source_device_label": ch.source.device_label,
                "dsp": ch.dsp,
                "inserts": inserts,
                "media": {
                    "title": ch.media.title,
                    "sample_rate": ch.media.sample_rate,
                    "total_frames": ch.media.total_frames,
                    "current_frame": ch.media.current_frame,
                    "playing": ch.media.playing,
                    "looping": ch.media.looping,
                    "loading": ch.media.loading,
                }
            })
        })
        .collect();

    let m = app.mixer.master();
    let master_inserts: Vec<serde_json::Value> = m
        .inserts
        .iter()
        .enumerate()
        .map(|(s_idx, sl)| {
            let gui_open = app
                .mixer
                .is_insert_window_open(MixerInsertTarget::Master { slot: s_idx });
            serialize_insert_slot(sl, gui_open)
        })
        .collect();

    let selected_out_name = app
        .output_devices
        .iter()
        .find(|d| d.id == app.default_output_id)
        .map(|d| d.name.clone())
        .unwrap_or_default();

    let val = serde_json::json!({
        "channels": channels,
        "master": {
            "fader_db": m.fader_db,
            "mute": m.mute,
            "dim": m.dim,
            "record_mix": m.record_mix,
            "virtual_mic_enabled": m.virtual_mic_enabled,
            "virtual_mic_gain_db": m.virtual_mic_gain_db,
            "inserts": master_inserts,
        },
        "selected_output_id": app.default_output_id,
        "selected_output_name": selected_out_name,
        "preferred_sample_rate": app.preferred_sample_rate,
        "preferred_block_size": app.preferred_block_size,
        "sample_rate": app.mixer.sample_rate(),
        "block_size": app.mixer.block_size(),
        "is_audio_running": app.mixer.is_audio_running(),
        "is_recording": app.mixer.is_recording(),
    });
    to_c_string(val.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_add_channel(handle: *mut ChronomixAppHandle) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return -1;
    };
    app.mixer.add_channel().map(|i| i as i32).unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_add_multimedia_channel(handle: *mut ChronomixAppHandle) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return -1;
    };
    app.mixer
        .add_multimedia_channel()
        .map(|i| i as i32)
        .unwrap_or(-1)
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_remove_channel(handle: *mut ChronomixAppHandle, ch: u32) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    i32::from(app.mixer.remove_channel(ch as usize))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_channel_name(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    name: *const c_char,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.set_channel_name(ch as usize, c_str_to_string(name));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_channel_color(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    r: u8,
    g: u8,
    b: u8,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.set_channel_color(ch as usize, [r, g, b]);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_channel_dsp_json(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    dsp_json: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let raw = c_str_to_string(dsp_json);
    match serde_json::from_str::<ChannelDspConfig>(&raw) {
        Ok(dsp) => {
            app.mixer.set_channel_dsp(ch as usize, dsp);
            1
        }
        Err(e) => {
            app.last_error = Some(format!("Error DSP: {e}"));
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_channel_dsp_json(
    handle: *const ChronomixAppHandle,
    ch: u32,
) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("{}".into());
    };
    let Some(channel) = app.mixer.channel(ch as usize) else {
        return to_c_string("{}".into());
    };
    to_c_string(serde_json::to_string(&channel.dsp).unwrap_or_else(|_| "{}".into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_eval_channel_eq_db(
    handle: *const ChronomixAppHandle,
    ch: u32,
    freq_hz: f32,
) -> f32 {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return 0.0;
    };
    let Some(channel) = app.mixer.channel(ch as usize) else {
        return 0.0;
    };
    eval_channel_eq_db(&channel.dsp, freq_hz, app.mixer.sample_rate() as f32)
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_channel_input(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    device_id: *const c_char,
    device_label: *const c_char,
    mode: u8,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let dev_id_str = c_str_to_string(device_id);
    let dev_label = c_str_to_string(device_label);
    let input_mode = match mode {
        1 => ChannelInputMode::Stereo(0, 1),
        2 => ChannelInputMode::Mono(0),
        3 => ChannelInputMode::Mono(1),
        4 => ChannelInputMode::Stereo(2, 3),
        5 => ChannelInputMode::Multimedia,
        6 => ChannelInputMode::Mono(2),
        7 => ChannelInputMode::Mono(3),
        _ => ChannelInputMode::Unassigned,
    };
    let source = ChannelInputSource {
        device_id: if dev_id_str.is_empty() { None } else { Some(dev_id_str) },
        device_label: dev_label.clone(),
        mode: input_mode,
    };
    let out_id = app.default_output_id.clone();
    match app.mixer.set_channel_input(ch as usize, source, Some(&out_id)) {
        Ok(()) => {
            app.status_text = if input_mode == ChannelInputMode::Unassigned {
                format!("CH {:02}: entrada desconectada", ch + 1)
            } else if input_mode == ChannelInputMode::Multimedia {
                format!("CH {:02}: modo Multimedia", ch + 1)
            } else {
                format!("CH {:02} entrada asignada: {}", ch + 1, dev_label)
            };
            1
        }
        Err(e) => {
            app.last_error = Some(format!("Error asignando entrada en CH {:02}: {e}", ch + 1));
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_channel_multimedia(handle: *mut ChronomixAppHandle, ch: u32) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.set_channel_multimedia(ch as usize);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_load_channel_media_pcm(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    interleaved_stereo: *const f32,
    total_frames: u32,
    sample_rate: u32,
    title: *const c_char,
    _path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    if interleaved_stereo.is_null() || total_frames == 0 {
        return 0;
    }
    let len = (total_frames as usize).saturating_mul(2);
    let samples = unsafe { slice::from_raw_parts(interleaved_stereo, len) }.to_vec();
    let title_str = c_str_to_string(title);
    let buf = MediaBuffer {
        interleaved_stereo: samples,
        total_frames: total_frames as usize,
        sample_rate: sample_rate.max(8000),
        title: if title_str.is_empty() {
            "Audio".to_string()
        } else {
            title_str.clone()
        },
    };
    app.mixer.set_channel_multimedia(ch as usize);
    let out = app.default_output_id.clone();
    let _ = app.mixer.install_custom_media_buffer(
        ch as usize,
        title_str.clone(),
        buf,
        false,
        Some(&out),
    );
    app.status_text = format!("Pista cargada en CH {:02}: {title_str}", ch + 1);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_load_channel_media_file(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let p = PathBuf::from(c_str_to_string(path));
    let out = app.default_output_id.clone();
    match app.mixer.load_media_file_sync(ch as usize, &p, false, Some(&out)) {
        Ok(title) => {
            app.status_text = format!("Pista cargada: {title}");
            1
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_unload_channel_media(handle: *mut ChronomixAppHandle, ch: u32) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.unload_channel_media(ch as usize);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_media_transport(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    playing: u8,
    looping: u8,
    seek_frame: i64,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if playing != 0 {
        app.ensure_audio_running();
    }
    let out = app.default_output_id.clone();
    let seek = if seek_frame >= 0 {
        Some(seek_frame as usize)
    } else {
        None
    };
    if let Err(e) =
        app.mixer
            .set_media_transport(ch as usize, playing != 0, looping != 0, seek, Some(&out))
    {
        app.last_error = Some(e.to_string());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_all_media_transport(
    handle: *mut ChronomixAppHandle,
    playing: u8,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if playing != 0 {
        app.ensure_audio_running();
    }
    let out = app.default_output_id.clone();
    if let Err(e) = app
        .mixer
        .set_all_media_transport(playing != 0, None, None, Some(&out))
    {
        app.last_error = Some(e.to_string());
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_clear_all_solos(handle: *mut ChronomixAppHandle) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.clear_all_solos();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_master(
    handle: *mut ChronomixAppHandle,
    fader_db: f32,
    mute: u8,
    dim: u8,
    record_mix: u8,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer
            .set_master(fader_db, mute != 0, dim != 0, record_mix != 0);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_get_presets_json() -> *mut c_char {
    let presets: Vec<serde_json::Value> = DspPreset::presets()
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let mut cfg = ChannelDspConfig::default();
            p.apply(&mut cfg);
            serde_json::json!({
                "index": i,
                "name": p.name(),
                "category": p.category(),
                "description": p.description(),
                "eq": cfg.eq_enabled,
                "gate": cfg.gate_enabled,
                "comp": cfg.comp_enabled,
                "deesser": cfg.deesser_enabled,
                "drive": cfg.drive_enabled,
                "delay": cfg.delay_enabled,
                "reverb": cfg.reverb_enabled,
                "limiter": cfg.limiter_enabled,
            })
        })
        .collect();
    to_c_string(serde_json::to_string(&presets).unwrap_or_else(|_| "[]".into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_apply_preset(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    preset_index: u32,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let Some(preset) = DspPreset::presets().get(preset_index as usize) else {
        return 0;
    };
    app.mixer.apply_channel_preset(ch as usize, *preset);
    app.status_text = format!("Perfil '{}' aplicado en CH {:02}", preset.name(), ch + 1);
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_save_channel_profile_file(
    handle: *const ChronomixAppHandle,
    ch: u32,
    path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    let Some(channel) = app.mixer.channel(ch as usize) else {
        return 0;
    };
    let p = PathBuf::from(c_str_to_string(path));
    match serde_json::to_string_pretty(&channel.dsp) {
        Ok(json) => i32::from(fs::write(p, json).is_ok()),
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_load_channel_profile_file(
    handle: *mut ChronomixAppHandle,
    ch: u32,
    path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let p = PathBuf::from(c_str_to_string(path));
    match fs::read_to_string(&p) {
        Ok(text) => match serde_json::from_str::<ChannelDspConfig>(&text) {
            Ok(cfg) => {
                app.mixer.set_channel_dsp(ch as usize, cfg);
                app.status_text = format!("Perfil cargado desde {}", p.display());
                1
            }
            Err(e) => {
                app.last_error = Some(format!("JSON de perfil inválido: {e}"));
                0
            }
        },
        Err(e) => {
            app.last_error = Some(format!("No se pudo leer perfil: {e}"));
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_scan_plugins_json(handle: *mut ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return to_c_string("[]".into());
    };
    let active_vst_dir = app
        .settings
        .custom_vst_dir
        .as_deref()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| app.paths.vst_plugins_dir());
    app.discovered_plugins = app
        .engine
        .discover_plugins(&active_vst_dir, app.settings.include_system_vst);
    let list: Vec<serde_json::Value> = app
        .discovered_plugins
        .iter()
        .map(|p| {
            serde_json::json!({
                "name": p.name,
                "vendor": p.vendor,
                "category": p.category,
                "path": p.path.display().to_string(),
            })
        })
        .collect();
    app.status_text = format!("Escaneo completado: {} plugins encontrados", list.len());
    to_c_string(serde_json::to_string(&list).unwrap_or_else(|_| "[]".into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_load_insert_plugin(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
    path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let p = PathBuf::from(c_str_to_string(path));
    let target = parse_target(is_master, ch, slot);
    match app.mixer.load_insert_plugin(target, &p) {
        Ok(meta) => {
            app.status_text = format!("Plugin cargado: {}", meta.info.name);
            1
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_unload_insert_plugin(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.unload_insert_plugin(parse_target(is_master, ch, slot));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_insert_bypass_mix(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
    bypass: u8,
    mix: f32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer
            .set_insert_bypass_and_mix(parse_target(is_master, ch, slot), bypass != 0, mix);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_set_insert_param(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
    param_id: u32,
    value: f64,
) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return to_c_string(String::new());
    };
    let fmt = app
        .mixer
        .set_insert_parameter(parse_target(is_master, ch, slot), param_id, value)
        .unwrap_or_default();
    to_c_string(fmt)
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_refresh_insert_params(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer
            .refresh_insert_parameters(parse_target(is_master, ch, slot));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_open_insert_gui(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    match app.mixer.open_insert_window(parse_target(is_master, ch, slot)) {
        Ok(()) => 1,
        Err(e) => {
            app.last_error = Some(e.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_close_insert_gui(
    handle: *mut ChronomixAppHandle,
    is_master: u8,
    ch: u32,
    slot: u32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.close_insert_window(parse_target(is_master, ch, slot));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_get_state_json(handle: *mut ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return to_c_string("{}".into());
    };
    let sessions: Vec<serde_json::Value> = app
        .midi
        .sessions
        .iter()
        .map(|s| {
            let tracks: Vec<serde_json::Value> = s
                .tracks
                .iter()
                .map(|tr| {
                    serde_json::json!({
                        "name": tr.name,
                        "sound": tr.sound as u8,
                        "steps": tr.steps,
                        "volume": tr.volume,
                        "pan": tr.pan,
                        "mute": tr.mute,
                        "solo": tr.solo,
                    })
                })
                .collect();
            let notes: Vec<serde_json::Value> = s
                .notes
                .iter()
                .map(|n| {
                    serde_json::json!({
                        "midi": n.midi,
                        "start_step": n.start_step,
                        "length_steps": n.length_steps,
                        "velocity": n.velocity,
                    })
                })
                .collect();
            serde_json::json!({
                "id": s.id,
                "name": s.name,
                "channel_index": s.channel_index,
                "bpm": s.bpm,
                "swing": s.swing,
                "is_playing": s.is_playing,
                "drums_enabled": s.drums_enabled,
                "playhead_pos_steps": s.playhead_pos_steps,
                "current_step": s.current_step,
                "synth": {
                    "preset": s.synth.preset as u8,
                    "attack": s.synth.attack,
                    "release": s.synth.release,
                    "cutoff": s.synth.cutoff,
                    "resonance": s.synth.resonance,
                    "drive": s.synth.drive,
                    "delay_send": s.synth.delay_send,
                    "delay_time_ms": s.synth.delay_time_ms,
                },
                "tracks": tracks,
                "notes": notes,
            })
        })
        .collect();
    let val = serde_json::json!({
        "active_session_idx": app.midi.active_session_idx,
        "last_triggered_key": app.midi.last_triggered_key,
        "sessions": sessions,
    });
    to_c_string(val.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_add_session(handle: *mut ChronomixAppHandle) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return -1;
    };
    let idx = app.midi.add_new_session();
    app.sync_active_midi_session(true);
    idx as i32
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_remove_active_session(handle: *mut ChronomixAppHandle) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    i32::from(app.midi.remove_active_session())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_select_session(handle: *mut ChronomixAppHandle, idx: u32) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if (idx as usize) < app.midi.sessions.len() {
        app.midi.active_session_idx = idx as usize;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_set_transport(
    handle: *mut ChronomixAppHandle,
    playing: u8,
    bpm: f32,
    swing: f32,
    drums_enabled: u8,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    let play_bool = playing != 0;
    if play_bool {
        app.ensure_audio_running();
    }
    let mut need_resync = false;
    if let Some(s) = app.midi.active_session_mut() {
        let bpm_clamped = bpm.clamp(40.0, 240.0);
        let swing_clamped = swing.clamp(0.0, 0.6);
        let drums_bool = drums_enabled != 0;
        if (s.bpm - bpm_clamped).abs() > 0.05
            || (s.swing - swing_clamped).abs() > 0.005
            || s.drums_enabled != drums_bool
        {
            s.bpm = bpm_clamped;
            s.swing = swing_clamped;
            s.drums_enabled = drums_bool;
            s.mark_edited();
            need_resync = true;
        }
        if play_bool && !s.is_playing {
            s.play_start = Some(Instant::now());
            s.playhead_pos_steps = 0.0;
            s.current_step = 0;
        } else if !play_bool {
            s.play_start = None;
            s.playhead_pos_steps = 0.0;
            s.current_step = 0;
        }
        s.is_playing = play_bool;
    }
    app.sync_active_midi_session(need_resync);
    if let Some(ch_idx) = app.midi.active_session().and_then(|s| s.channel_index) {
        let out = app.default_output_id.clone();
        let seek = if play_bool { Some(0) } else { Some(0) };
        let _ = app
            .mixer
            .set_media_transport(ch_idx, play_bool, true, seek, Some(&out));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_set_synth(
    handle: *mut ChronomixAppHandle,
    preset: u8,
    attack: f32,
    release: f32,
    cutoff: f32,
    resonance: f32,
    drive: f32,
    delay_send: f32,
    delay_time_ms: f32,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if let Some(s) = app.midi.active_session_mut() {
        s.synth.preset = SynthPreset::from_u8(preset);
        s.synth.attack = attack.clamp(0.002, 0.4);
        s.synth.release = release.clamp(0.03, 1.5);
        s.synth.cutoff = cutoff.clamp(200.0, 16000.0);
        s.synth.resonance = resonance.clamp(0.0, 1.0);
        s.synth.drive = drive.clamp(0.0, 1.0);
        s.synth.delay_send = delay_send.clamp(0.0, 1.0);
        s.synth.delay_time_ms = delay_time_ms.clamp(40.0, 800.0);
        s.mark_edited();
    }
    app.sync_active_midi_session(false);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_toggle_step(
    handle: *mut ChronomixAppHandle,
    track_idx: u32,
    step: u32,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if let Some(s) = app.midi.active_session_mut() {
        if let Some(tr) = s.tracks.get_mut(track_idx as usize) {
            if (step as usize) < PIANO_ROLL_STEPS {
                tr.steps[step as usize] = !tr.steps[step as usize];
                s.mark_edited();
            }
        }
    }
    app.sync_active_midi_session(false);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_set_track_mix(
    handle: *mut ChronomixAppHandle,
    track_idx: u32,
    volume: f32,
    pan: f32,
    mute: u8,
    solo: u8,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if let Some(s) = app.midi.active_session_mut() {
        if let Some(tr) = s.tracks.get_mut(track_idx as usize) {
            tr.volume = volume.clamp(0.0, 1.2);
            tr.pan = pan.clamp(-1.0, 1.0);
            tr.mute = mute != 0;
            tr.solo = solo != 0;
            s.mark_edited();
        }
    }
    app.sync_active_midi_session(false);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_toggle_note(
    handle: *mut ChronomixAppHandle,
    midi_note: u8,
    step: u32,
    length_steps: u32,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let added = app
        .midi
        .active_session_mut()
        .map(|s| s.toggle_or_set_note(midi_note, step as usize, length_steps.max(1) as usize))
        .unwrap_or(false);
    app.sync_active_midi_session(false);
    if added {
        chronomix_midi_trigger_key(handle, midi_note);
    }
    i32::from(added)
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_load_demo_melody(handle: *mut ChronomixAppHandle) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if let Some(s) = app.midi.active_session_mut() {
        s.load_demo_melody();
    }
    app.sync_active_midi_session(false);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_clear_notes(handle: *mut ChronomixAppHandle) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if let Some(s) = app.midi.active_session_mut() {
        s.clear_notes();
    }
    app.sync_active_midi_session(false);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_midi_trigger_key(handle: *mut ChronomixAppHandle, midi_note: u8) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    app.ensure_audio_running();
    app.sync_active_midi_session(false);
    app.midi.last_triggered_key = Some(midi_note);
    let sr = app.mixer.sample_rate();
    let out = app.default_output_id.clone();
    if let Some(sess) = app.midi.active_session() {
        if let Some(ch_idx) = sess.channel_index {
            let preview = synthesize_single_note_preview(midi_note, &sess.synth, sr);
            let _ = app
                .mixer
                .trigger_channel_preview(ch_idx, preview, Some(&out));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_load_pcm(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    interleaved_stereo: *const f32,
    total_frames: u32,
    sample_rate: u32,
    title: *const c_char,
    path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    if deck >= 2 || interleaved_stereo.is_null() || total_frames == 0 {
        return 0;
    }
    let len = (total_frames as usize).saturating_mul(2);
    let samples = unsafe { slice::from_raw_parts(interleaved_stereo, len) }.to_vec();
    let title_str = c_str_to_string(title);
    let path_str = c_str_to_string(path);
    let buf = MediaBuffer {
        interleaved_stereo: samples,
        total_frames: total_frames as usize,
        sample_rate: sample_rate.max(8000),
        title: if title_str.is_empty() {
            format!("Deck {}", if deck == 0 { "A" } else { "B" })
        } else {
            title_str.clone()
        },
    };
    let opt_path = (!path_str.is_empty()).then(|| PathBuf::from(path_str));
    app.mixer.dj_load_buffer(deck as usize, buf, opt_path);
    app.status_text = format!(
        "DJ Deck {} cargado: {}",
        if deck == 0 { "A" } else { "B" },
        title_str
    );
    1
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_load_file(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    path: *const c_char,
) -> i32 {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return 0;
    };
    let p = PathBuf::from(c_str_to_string(path));
    match app.mixer.dj_load_file(deck as usize, p) {
        Ok(()) => {
            if let Some(m) = app.mixer.dj_deck_meta(deck as usize) {
                app.status_text = format!(
                    "DJ Deck {} cargado: {} ({:.1} BPM)",
                    if deck == 0 { "A" } else { "B" },
                    m.title,
                    m.base_bpm
                );
            }
            1
        }
        Err(e) => {
            app.last_error = Some(e.to_string());
            0
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_unload(handle: *mut ChronomixAppHandle, deck: u32) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_unload(deck as usize);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_set_playing(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    playing: u8,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if playing != 0 {
        app.ensure_audio_running();
    }
    app.mixer.dj_set_playing(deck as usize, playing != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_seek_norm(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    norm: f64,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_seek_norm(deck as usize, norm);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_nudge_secs(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    delta_secs: f64,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_nudge_secs(deck as usize, delta_secs);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_set_cue(handle: *mut ChronomixAppHandle, deck: u32) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_set_cue_here(deck as usize);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_jump_cue(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    play: u8,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if play != 0 {
        app.ensure_audio_running();
    }
    app.mixer.dj_jump_cue(deck as usize, play != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_hot_cue(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    slot: u32,
    clear: u8,
) {
    let Some(app) = (unsafe { handle.as_mut() }) else {
        return;
    };
    if clear == 0 {
        app.ensure_audio_running();
    }
    app.mixer
        .dj_hot_cue(deck as usize, slot as usize, clear != 0);
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_sync_bpm(handle: *mut ChronomixAppHandle, deck: u32) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_sync_bpm(deck as usize);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_set_loop(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    enabled: u8,
    beats: f32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_set_loop(deck as usize, enabled != 0, beats);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_set_deck_params(
    handle: *mut ChronomixAppHandle,
    deck: u32,
    params: *const CDjDeckParams,
) {
    let (Some(app), Some(p)) = (unsafe { handle.as_mut() }, unsafe { params.as_ref() }) else {
        return;
    };
    app.mixer.dj_set_deck_params(
        deck as usize,
        DjDeckParams {
            gain_db: p.gain_db,
            volume: p.volume,
            eq_low_db: p.eq_low_db,
            eq_mid_db: p.eq_mid_db,
            eq_high_db: p.eq_high_db,
            kill_low: p.kill_low != 0,
            kill_mid: p.kill_mid != 0,
            kill_high: p.kill_high != 0,
            filter: p.filter,
            tempo_percent: p.tempo_percent,
            pfl_cue: p.pfl_cue != 0,
            loop_enabled: p.loop_enabled != 0,
            loop_beats: p.loop_beats,
        },
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_get_deck_params(
    handle: *const ChronomixAppHandle,
    deck: u32,
    out_params: *mut CDjDeckParams,
) {
    let (Some(app), Some(out)) = (unsafe { handle.as_ref() }, unsafe { out_params.as_mut() }) else {
        return;
    };
    let Some(meta) = app.mixer.dj_deck_meta(deck as usize) else {
        return;
    };
    let p = meta.params;
    *out = CDjDeckParams {
        gain_db: p.gain_db,
        volume: p.volume,
        eq_low_db: p.eq_low_db,
        eq_mid_db: p.eq_mid_db,
        eq_high_db: p.eq_high_db,
        kill_low: u8::from(p.kill_low),
        kill_mid: u8::from(p.kill_mid),
        kill_high: u8::from(p.kill_high),
        pfl_cue: u8::from(p.pfl_cue),
        filter: p.filter,
        tempo_percent: p.tempo_percent,
        loop_enabled: u8::from(p.loop_enabled),
        _pad: [0; 3],
        loop_beats: p.loop_beats,
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_set_mixer_params(
    handle: *mut ChronomixAppHandle,
    params: *const CDjMixerParams,
) {
    let (Some(app), Some(p)) = (unsafe { handle.as_mut() }, unsafe { params.as_ref() }) else {
        return;
    };
    app.mixer.dj_set_mixer_params(DjMixerParams {
        crossfader: p.crossfader,
        curve: CrossfaderCurve::from_u8(p.curve),
        master_volume: p.master_volume,
        cue_mix: p.cue_mix,
        send_to_master: p.send_to_master != 0,
    });
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_get_mixer_params(
    handle: *const ChronomixAppHandle,
    out_params: *mut CDjMixerParams,
) {
    let (Some(app), Some(out)) = (unsafe { handle.as_ref() }, unsafe { out_params.as_mut() }) else {
        return;
    };
    let p = app.mixer.dj_mixer_params();
    *out = CDjMixerParams {
        crossfader: p.crossfader,
        curve: p.curve as u8,
        send_to_master: u8::from(p.send_to_master),
        _pad: [0; 2],
        master_volume: p.master_volume,
        cue_mix: p.cue_mix,
    };
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_start_auto_crossfade(
    handle: *mut ChronomixAppHandle,
    target: f32,
    duration_secs: f32,
) {
    if let Some(app) = unsafe { handle.as_mut() } {
        app.mixer.dj_start_auto_crossfade(target, duration_secs);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_get_telemetry(
    handle: *mut ChronomixAppHandle,
    out_tel: *mut CDjTelemetry,
) {
    let (Some(app), Some(out)) = (unsafe { handle.as_mut() }, unsafe { out_tel.as_mut() }) else {
        return;
    };
    let snap = app.mixer.dj_take_telemetry();
    out.master_peak_l = snap.master_peak_l;
    out.master_peak_r = snap.master_peak_r;
    out.crossfader = snap.crossfader;

    for i in 0..2 {
        if let Some(meta) = app.mixer.dj_deck_meta(i) {
            let mut hot_cues = [0.0_f64; DJ_HOT_CUES];
            let mut hot_cue_set = [0_u8; DJ_HOT_CUES];
            for (slot, hc) in meta.hot_cues.iter().enumerate() {
                if let Some(frame) = hc {
                    hot_cues[slot] = *frame;
                    hot_cue_set[slot] = 1;
                }
            }
            out.decks[i] = CDjDeckTelemetry {
                pos_frames: snap.decks[i].pos_frames,
                total_frames: meta.total_frames as u64,
                sample_rate: meta.sample_rate,
                base_bpm: meta.base_bpm,
                effective_bpm: meta.effective_bpm(),
                peak_l: snap.decks[i].peak_l,
                peak_r: snap.decks[i].peak_r,
                cue_frame: meta.cue_frame,
                loop_start_frame: meta.loop_start_frame,
                loop_end_frame: meta.loop_end_frame,
                hot_cues,
                hot_cue_set,
                playing: u8::from(snap.decks[i].playing),
                loop_enabled: u8::from(meta.params.loop_enabled),
                has_track: u8::from(meta.total_frames > 0),
                _pad: 0,
            };
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_get_waveform(
    handle: *const ChronomixAppHandle,
    deck: u32,
    out_bins: *mut CWaveformBin,
    max_bins: u32,
) -> u32 {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return 0;
    };
    if out_bins.is_null() || max_bins == 0 {
        return 0;
    }
    let Some(meta) = app.mixer.dj_deck_meta(deck as usize) else {
        return 0;
    };
    let n = meta.waveform.len().min(max_bins as usize).min(DJ_WAVEFORM_BINS);
    let dst = unsafe { slice::from_raw_parts_mut(out_bins, n) };
    for i in 0..n {
        let w = meta.waveform[i];
        dst[i] = CWaveformBin {
            low: w.low,
            mid: w.mid,
            high: w.high,
            peak: w.peak,
        };
    }
    n as u32
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_dj_get_deck_title(
    handle: *const ChronomixAppHandle,
    deck: u32,
) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string(String::new());
    };
    to_c_string(
        app.mixer
            .dj_deck_meta(deck as usize)
            .map(|m| m.title.clone())
            .unwrap_or_default(),
    )
}

fn scan_audio_recursive(dir: &Path, depth: usize, out: &mut Vec<serde_json::Value>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_dir() {
            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if !name.starts_with('.') && name != "target" && name != "build" && name != "third_party" {
                scan_audio_recursive(&p, depth + 1, out);
            }
        } else if p.is_file() {
            let ext = p
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("")
                .to_ascii_lowercase();
            if matches!(ext.as_str(), "wav" | "flac" | "mp3" | "ogg" | "aiff" | "m4a") {
                let filename = p
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("audio")
                    .to_string();
                let parent = p
                    .parent()
                    .and_then(|pp| pp.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("")
                    .to_string();
                let meta = fs::metadata(&p).ok();
                let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let modified_secs = meta
                    .and_then(|m| m.modified().ok())
                    .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
                    .map(|d| d.as_secs())
                    .unwrap_or(0);
                out.push(serde_json::json!({
                    "path": p.display().to_string(),
                    "filename": filename,
                    "parent_folder": parent,
                    "size_bytes": size_bytes,
                    "modified_secs": modified_secs,
                    "extension": ext.to_ascii_uppercase(),
                }));
            }
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_library_scan_json(dir_path: *const c_char) -> *mut c_char {
    let dir_str = c_str_to_string(dir_path);
    let dir = if dir_str.trim().is_empty() {
        AppPaths::resolve().recordings_dir()
    } else {
        PathBuf::from(dir_str)
    };
    let mut items = Vec::new();
    scan_audio_recursive(&dir, 0, &mut items);
    items.sort_by(|a, b| {
        let ma = a.get("modified_secs").and_then(|v| v.as_u64()).unwrap_or(0);
        let mb = b.get("modified_secs").and_then(|v| v.as_u64()).unwrap_or(0);
        mb.cmp(&ma)
    });
    to_c_string(serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_wine_status_json(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("{}".into());
    };
    let st = inspect_wine_environment(&app.wine_config);
    let bridge_mode = format!("{:?}", st.bridge_mode);
    let warning = wine_bridge_runtime_warning(&app.wine_config).unwrap_or_default();
    let val = serde_json::json!({
        "wine_binary": st.active_wine_binary.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        "wine_version": st.active_wine_version.unwrap_or_default(),
        "winegpp": st.winegpp_path.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        "bridge_mode": bridge_mode,
        "prefix": st.active_prefix.as_ref().map(|p| p.display().to_string()).unwrap_or_default(),
        "prefix_initialized": st.active_prefix_initialized,
        "warning": warning,
    });
    to_c_string(val.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_wine_init_prefix(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("Error interno".into());
    };
    match initialize_wine_prefix(&app.wine_config) {
        Ok(p) => to_c_string(format!("OK: Prefijo Wine inicializado en {}", p.display())),
        Err(e) => to_c_string(format!("ERROR: {e}")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_wine_rebuild_bridge(handle: *const ChronomixAppHandle) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("Error interno".into());
    };
    match rebuild_wine_bridge(&app.wine_config) {
        Ok((so_path, _)) => to_c_string(format!("OK: Puente Wine compilado en {}", so_path.display())),
        Err(e) => to_c_string(format!("ERROR: {e}")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_wine_launch_tool(
    handle: *const ChronomixAppHandle,
    action_id: u8,
) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("Error interno".into());
    };
    let action = match action_id {
        0 => WineAction::Winecfg,
        1 => WineAction::Explorer,
        2 => WineAction::Cmd,
        3 => WineAction::Taskmgr,
        4 => WineAction::Uninstaller,
        5 => WineAction::Regedit,
        6 => WineAction::Winetricks,
        _ => WineAction::KillServer,
    };
    match launch_wine_tool(action, &app.wine_config) {
        Ok(msg) => to_c_string(format!("OK: {msg}")),
        Err(e) => to_c_string(format!("ERROR: {e}")),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn chronomix_wine_launch_exe(
    handle: *const ChronomixAppHandle,
    path: *const c_char,
) -> *mut c_char {
    let Some(app) = (unsafe { handle.as_ref() }) else {
        return to_c_string("Error interno".into());
    };
    let p = PathBuf::from(c_str_to_string(path));
    match launch_wine_executable(&p, &app.wine_config) {
        Ok(msg) => to_c_string(format!("OK: {msg}")),
        Err(e) => to_c_string(format!("ERROR: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ffi_handle_lifecycle_and_dj_midi_work() {
        let h = chronomix_create();
        assert!(!h.is_null());

        let presets_ptr = chronomix_get_presets_json();
        let presets_str = c_str_to_string(presets_ptr);
        chronomix_free_string(presets_ptr);
        let presets: serde_json::Value = serde_json::from_str(&presets_str).unwrap();
        assert_eq!(presets.as_array().unwrap().len(), 23);

        let paths_ptr = chronomix_get_paths_json(h);
        let paths_str = c_str_to_string(paths_ptr);
        chronomix_free_string(paths_ptr);
        assert!(paths_str.contains("vst_plugins"));
        assert!(paths_str.contains("recordings"));

        let pcm = vec![0.25_f32; 48_000 * 2];
        let title = CString::new("Demo Track").unwrap();
        let empty = CString::new("").unwrap();
        let ok = chronomix_dj_load_pcm(h, 0, pcm.as_ptr(), 48_000, 48_000, title.as_ptr(), empty.as_ptr());
        assert_eq!(ok, 1);

        let mut wf = vec![CWaveformBin::default(); DJ_WAVEFORM_BINS];
        let n = chronomix_dj_get_waveform(h, 0, wf.as_mut_ptr(), DJ_WAVEFORM_BINS as u32);
        assert_eq!(n as usize, DJ_WAVEFORM_BINS);

        let added = chronomix_midi_add_session(h);
        assert_eq!(added, 1);

        chronomix_destroy(h);
    }
}
