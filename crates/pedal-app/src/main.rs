mod i18n;
mod mixer_ui;
mod theme;

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::Ordering,
    },
    thread,
    time::Duration,
};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use i18n::Lang;
use mixer_ui::{MixerPanelContext, MixerUiState, WingScreenTab, WingSelection, render_mixer_panel};
use pedal_audio::AudioDeviceInfo;
use pedal_engine::{
    LoadedPluginMeta, MeterSnapshot, MixerEngine, MixerInsertTarget, PedalEngine, WineAction,
    WineBridgeMode, WineConfig, WineEnvironmentStatus, initialize_wine_prefix,
    inspect_wine_environment, launch_wine_executable, launch_wine_tool, rebuild_wine_bridge,
};
use theme::{
    StudioIcon, StudioPalette, animated_action_button, animated_led_toggle, apply_studio_theme,
    draw_animated_signal_pipe, draw_rotary_knob, draw_studio_vu_meter, draw_vector_icon,
    icon_heading, lerp_color, studio_card,
};
use vst3_host::PluginInfo;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Chronomix - Studio Audio DAW & VST2/VST3 Host")
            .with_inner_size([1020.0, 740.0])
            .with_min_inner_size([360.0, 260.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Chronomix",
        options,
        Box::new(|cc| {
            apply_studio_theme(&cc.egui_ctx);
            Ok(Box::new(ChronomixApp::new()))
        }),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePanel {
    News,
    Recording,
    Mixer,
    Settings,
}

fn default_true_setting() -> bool {
    true
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AppSettings {
    custom_vst_dir: Option<PathBuf>,
    include_system_vst: bool,
    lang: Option<String>,
    #[serde(default)]
    wine_prefix: Option<PathBuf>,
    #[serde(default)]
    wine_binary: Option<PathBuf>,
    #[serde(default = "default_true_setting")]
    wine_auto_display: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            custom_vst_dir: None,
            include_system_vst: false,
            lang: None,
            wine_prefix: None,
            wine_binary: None,
            wine_auto_display: true,
        }
    }
}

impl AppSettings {
    fn settings_file_path() -> PathBuf {
        std::env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("."))
            .join("chronomix-settings.json")
    }

    fn load() -> Self {
        let path = Self::settings_file_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    fn save(&self) {
        let path = Self::settings_file_path();
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) enum StatusState {
    Ready,
    MonitoringStopped,
    Live {
        in_sr: u32,
        in_ch: u16,
        out_sr: u32,
        out_ch: u16,
        block_size: usize,
    },
    Scanning,
    ScanComplete(usize),
    LoadedPlugin {
        name: String,
        params: usize,
    },
    PluginUnloaded,
    SavedRecording(PathBuf),
    RecordingTo(PathBuf),
    OpenedFolder(PathBuf),
    UpdatedVstFolder(PathBuf),
    Plain(String),
}

impl StatusState {
    fn format(&self, lang: Lang) -> String {
        let t = lang.text();
        match self {
            Self::Ready => t.status_ready.into(),
            Self::MonitoringStopped => t.status_stopped.into(),
            Self::Live {
                in_sr,
                in_ch,
                out_sr,
                out_ch,
                block_size,
            } => lang.live_status(*in_sr, *in_ch, *out_sr, *out_ch, *block_size),
            Self::Scanning => t.status_scanning.into(),
            Self::ScanComplete(count) => lang.scan_complete_status(*count),
            Self::LoadedPlugin { name, params } => lang.loaded_vst3_status(name, *params),
            Self::PluginUnloaded => t.status_unloaded.into(),
            Self::SavedRecording(path) => lang.saved_recording_status(path),
            Self::RecordingTo(path) => lang.recording_to_status(path),
            Self::OpenedFolder(path) => lang.opened_folder_status(path),
            Self::UpdatedVstFolder(path) => lang.vst_folder_updated_status(path),
            Self::Plain(msg) => msg.clone(),
        }
    }
}

#[derive(Debug, Clone)]
enum ErrorState {
    SelectIo,
    StartAudioBeforeRec,
    StartAudioFailed(String),
    LoadPluginFailed(String),
    OpenGuiFailed(String),
    Plain(String),
}

impl ErrorState {
    fn format(&self, lang: Lang) -> String {
        let t = lang.text();
        match self {
            Self::SelectIo => t.error_select_io.into(),
            Self::StartAudioBeforeRec => t.error_start_before_rec.into(),
            Self::StartAudioFailed(e) => lang.failed_start_audio(e),
            Self::LoadPluginFailed(e) => lang.failed_load_vst3(e),
            Self::OpenGuiFailed(e) => lang.failed_open_gui(e),
            Self::Plain(e) => e.clone(),
        }
    }
}

struct WineTaskResult {
    rescan_plugins: bool,
    outcome: Result<String, String>,
}

struct ChronomixApp {
    theme_initialized: bool,
    lang: Lang,
    active_panel: ActivePanel,
    splash_start: Option<std::time::Instant>,

    program_vst_dir: PathBuf,
    custom_vst_dir: Option<PathBuf>,
    include_system_vst: bool,

    wine_config: WineConfig,
    wine_status: Option<WineEnvironmentStatus>,
    wine_status_slot: Arc<Mutex<Option<WineEnvironmentStatus>>>,
    wine_status_refreshing: bool,
    wine_task_slot: Arc<Mutex<Option<WineTaskResult>>>,
    wine_busy_msg: Option<String>,
    show_wine_paths: bool,

    engine: PedalEngine,
    mixer_engine: MixerEngine,
    mixer_ui: MixerUiState,
    input_devices: Vec<AudioDeviceInfo>,
    output_devices: Vec<AudioDeviceInfo>,
    selected_input: String,
    selected_output: String,
    sample_rate: u32,
    block_size: usize,

    input_gain_db: f32,
    output_gain_db: f32,
    mix_percent: f32,
    bypass: bool,
    monitor_muted: bool,

    meters: MeterSnapshot,
    peak_hold: MeterSnapshot,

    manual_plugin_path: String,
    discovered_plugins: Vec<PluginInfo>,
    selected_discovered_idx: Option<usize>,
    scan_in_progress: Arc<Mutex<Option<Vec<PluginInfo>>>>,
    is_scanning: bool,
    library_filter: String,
    show_path_loader: bool,

    loaded_plugin: Option<LoadedPluginMeta>,
    param_filter: String,

    status: StatusState,
    error: Option<ErrorState>,
}

fn pick_best_default_device(
    devices: &[AudioDeviceInfo],
    reported_default: Option<String>,
    is_input: bool,
) -> String {
    if let Some(ref def_id) = reported_default {
        if devices.iter().any(|d| &d.id == def_id) {
            return def_id.clone();
        }
    }
    if let Some(sys_def) = devices
        .iter()
        .find(|d| d.name.contains("System Default") || d.id.ends_with(":default"))
    {
        return sys_def.id.clone();
    }
    if !is_input {
        if let Some(analog) = devices.iter().find(|d| {
            let low = d.name.to_lowercase();
            (low.contains("analog") || low.contains("headphone") || low.contains("alc"))
                && !low.contains("hdmi")
        }) {
            return analog.id.clone();
        }
    } else if let Some(mic) = devices.iter().find(|d| {
        let low = d.name.to_lowercase();
        low.contains("mic") || low.contains("usb") || low.contains("analog")
    }) {
        return mic.id.clone();
    }
    devices.first().map(|d| d.id.clone()).unwrap_or_default()
}

impl ChronomixApp {
    fn new() -> Self {
        let settings = AppSettings::load();
        let lang = match settings.lang.as_deref() {
            Some("ES") => Lang::Es,
            Some("EN") => Lang::En,
            _ => Lang::detect_default(),
        };

        let program_vst_dir = PedalEngine::default_program_vst_dir();
        let custom_vst_dir = settings
            .custom_vst_dir
            .filter(|p| std::fs::create_dir_all(p).is_ok() || p.is_dir());
        let include_system_vst = settings.include_system_vst;

        let wine_config = WineConfig {
            wine_binary: settings.wine_binary,
            wine_prefix: settings.wine_prefix,
            auto_display: settings.wine_auto_display,
        };

        let mut engine = PedalEngine::new();
        engine.set_wine_config(wine_config.clone());
        let active_dir = custom_vst_dir
            .clone()
            .unwrap_or_else(|| program_vst_dir.clone());
        let discovered_plugins = engine.discover_vst_in_directory(&active_dir);
        let selected_discovered_idx = if !discovered_plugins.is_empty() {
            Some(0)
        } else {
            None
        };

        let input_devices = engine.list_input_devices().unwrap_or_default();
        let output_devices = engine.list_output_devices().unwrap_or_default();

        let selected_input =
            pick_best_default_device(&input_devices, engine.default_input_device(), true);
        let selected_output =
            pick_best_default_device(&output_devices, engine.default_output_device(), false);

        let mut mixer_engine = MixerEngine::new();
        mixer_engine.set_wine_config(wine_config.clone());
        let mixer_ui = MixerUiState::new(selected_output.clone(), mixer_engine.channels().len());

        let mut app = Self {
            theme_initialized: false,
            lang,
            active_panel: ActivePanel::Mixer,
            splash_start: Some(std::time::Instant::now()),
            program_vst_dir,
            custom_vst_dir,
            include_system_vst,
            wine_config,
            wine_status: None,
            wine_status_slot: Arc::new(Mutex::new(None)),
            wine_status_refreshing: false,
            wine_task_slot: Arc::new(Mutex::new(None)),
            wine_busy_msg: None,
            show_wine_paths: false,
            engine,
            mixer_engine,
            mixer_ui,
            input_devices,
            output_devices,
            selected_input,
            selected_output,
            sample_rate: 48_000,
            block_size: 256,
            input_gain_db: 0.0,
            output_gain_db: 0.0,
            mix_percent: 100.0,
            bypass: false,
            monitor_muted: false,
            meters: MeterSnapshot::default(),
            peak_hold: MeterSnapshot::default(),
            manual_plugin_path: String::new(),
            discovered_plugins,
            selected_discovered_idx,
            scan_in_progress: Arc::new(Mutex::new(None)),
            is_scanning: false,
            library_filter: String::new(),
            show_path_loader: false,
            loaded_plugin: None,
            param_filter: String::new(),
            status: StatusState::Ready,
            error: None,
        };
        if cfg!(target_os = "linux") {
            app.refresh_wine_status();
        }
        app
    }

    fn active_vst_dir(&self) -> PathBuf {
        self.custom_vst_dir
            .clone()
            .unwrap_or_else(|| self.program_vst_dir.clone())
    }

    fn save_settings(&self) {
        let settings = AppSettings {
            custom_vst_dir: self.custom_vst_dir.clone(),
            include_system_vst: self.include_system_vst,
            lang: Some(self.lang.code().to_string()),
            wine_prefix: self.wine_config.wine_prefix.clone(),
            wine_binary: self.wine_config.wine_binary.clone(),
            wine_auto_display: self.wine_config.auto_display,
        };
        settings.save();
    }

    fn sync_and_save_wine_config(&mut self) {
        self.engine.set_wine_config(self.wine_config.clone());
        self.mixer_engine.set_wine_config(self.wine_config.clone());
        self.save_settings();
        self.refresh_wine_status();
        if self.include_system_vst {
            self.start_scan();
        }
    }

    fn refresh_wine_status(&mut self) {
        if self.wine_status_refreshing {
            return;
        }
        self.wine_status_refreshing = true;
        let slot = Arc::clone(&self.wine_status_slot);
        let cfg = self.wine_config.clone();
        thread::spawn(move || {
            let inspected = inspect_wine_environment(&cfg);
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(inspected);
            }
        });
    }

    fn spawn_wine_task(
        &mut self,
        busy_label: String,
        rescan_plugins: bool,
        job: impl FnOnce(WineConfig) -> Result<String, String> + Send + 'static,
    ) {
        if self.wine_busy_msg.is_some() {
            return;
        }
        self.wine_busy_msg = Some(busy_label.clone());
        self.status = StatusState::Plain(busy_label);
        self.error = None;
        let slot = Arc::clone(&self.wine_task_slot);
        let cfg = self.wine_config.clone();
        thread::spawn(move || {
            let outcome = job(cfg);
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(WineTaskResult {
                    rescan_plugins,
                    outcome,
                });
            }
        });
    }

    fn poll_wine_tasks(&mut self) {
        if self.wine_status_refreshing {
            let status_update = self
                .wine_status_slot
                .try_lock()
                .ok()
                .and_then(|mut guard| guard.take());
            if let Some(inspected) = status_update {
                self.wine_status = Some(inspected);
                self.wine_status_refreshing = false;
            }
        }
        if self.wine_busy_msg.is_some() {
            let task_done = self
                .wine_task_slot
                .try_lock()
                .ok()
                .and_then(|mut guard| guard.take());
            if let Some(done) = task_done {
                self.wine_busy_msg = None;
                match done.outcome {
                    Ok(msg) => {
                        self.status = StatusState::Plain(msg);
                        self.error = None;
                        if done.rescan_plugins {
                            self.start_scan();
                        }
                    }
                    Err(err) => {
                        self.error = Some(ErrorState::Plain(err));
                    }
                }
                self.refresh_wine_status();
            }
        }
    }

    fn open_active_vst_folder(&mut self) {
        let dir = self.active_vst_dir();
        let _ = std::fs::create_dir_all(&dir);
        if let Err(e) = open_folder_in_file_manager(&dir) {
            self.error = Some(ErrorState::Plain(e));
        } else {
            self.status = StatusState::OpenedFolder(dir);
            self.error = None;
        }
    }

    fn refresh_devices(&mut self) {
        match self.engine.list_input_devices() {
            Ok(list) => {
                self.input_devices = list;
                if !self
                    .input_devices
                    .iter()
                    .any(|d| d.id == self.selected_input)
                {
                    self.selected_input = pick_best_default_device(
                        &self.input_devices,
                        self.engine.default_input_device(),
                        true,
                    );
                }
            }
            Err(e) => self.error = Some(ErrorState::Plain(e.to_string())),
        }

        match self.engine.list_output_devices() {
            Ok(list) => {
                self.output_devices = list;
                if !self
                    .output_devices
                    .iter()
                    .any(|d| d.id == self.selected_output)
                {
                    self.selected_output = pick_best_default_device(
                        &self.output_devices,
                        self.engine.default_output_device(),
                        false,
                    );
                }
            }
            Err(e) => self.error = Some(ErrorState::Plain(e.to_string())),
        }
    }

    fn start_or_restart_audio(&mut self) {
        if self.selected_input.is_empty() || self.selected_output.is_empty() {
            self.error = Some(ErrorState::SelectIo);
            return;
        }

        self.mixer_engine.stop_audio();

        match self.engine.start_audio(
            &self.selected_input,
            &self.selected_output,
            self.sample_rate,
            self.block_size,
        ) {
            Ok(()) => {
                self.error = None;
                if let Some(info) = self.engine.active_audio_info() {
                    self.status = StatusState::Live {
                        in_sr: info.input_sample_rate,
                        in_ch: info.input_channels,
                        out_sr: info.output_sample_rate,
                        out_ch: info.output_channels,
                        block_size: info.block_size,
                    };
                }
            }
            Err(e) => {
                self.error = Some(ErrorState::StartAudioFailed(e.to_string()));
            }
        }
    }

    fn restart_audio_if_running(&mut self) {
        if self.engine.is_audio_running() {
            self.start_or_restart_audio();
        }
    }

    fn toggle_audio(&mut self) {
        if self.engine.is_audio_running() {
            self.engine.stop_audio();
            self.status = StatusState::MonitoringStopped;
            self.error = None;
            return;
        }
        self.start_or_restart_audio();
    }

    fn start_scan(&mut self) {
        if self.is_scanning {
            return;
        }
        self.is_scanning = true;
        self.status = StatusState::Scanning;
        let slot = Arc::clone(&self.scan_in_progress);
        let vst_dir = self.active_vst_dir();
        let include_system = self.include_system_vst;
        let wine_config = self.wine_config.clone();
        thread::spawn(move || {
            let mut temp_engine = PedalEngine::new();
            temp_engine.set_wine_config(wine_config);
            let found = temp_engine.discover_plugins(&vst_dir, include_system);
            if let Ok(mut guard) = slot.lock() {
                *guard = Some(found);
            }
        });
    }

    fn poll_scan(&mut self) {
        if !self.is_scanning {
            return;
        }
        if let Ok(mut guard) = self.scan_in_progress.try_lock() {
            if let Some(found) = guard.take() {
                self.is_scanning = false;
                let count = found.len();
                self.discovered_plugins = found;
                self.selected_discovered_idx = if count > 0 { Some(0) } else { None };
                self.status = StatusState::ScanComplete(count);
            }
        }
    }

    fn load_plugin_from_path(&mut self, path: PathBuf) {
        self.manual_plugin_path = path.display().to_string();
        match self.engine.load_vst3_plugin(&path) {
            Ok(meta) => {
                self.status = StatusState::LoadedPlugin {
                    name: meta.info.name.clone(),
                    params: meta.parameters.len(),
                };
                self.loaded_plugin = Some(meta);
                self.error = None;
            }
            Err(e) => {
                self.error = Some(ErrorState::LoadPluginFailed(e.to_string()));
            }
        }
    }

    fn toggle_recording(&mut self) {
        if self.engine.shared.is_recording() {
            match self.engine.stop_recording() {
                Ok(Some(path)) => {
                    self.status = StatusState::SavedRecording(path);
                    self.error = None;
                }
                Ok(None) => {}
                Err(e) => self.error = Some(ErrorState::Plain(e.to_string())),
            }
            return;
        }

        if !self.engine.is_audio_running() {
            self.error = Some(ErrorState::StartAudioBeforeRec);
            return;
        }

        let t = self.lang.text();
        if let Some(path) = rfd::FileDialog::new()
            .set_file_name("chronomix-take.wav")
            .add_filter(t.dialog_wav_filter, &["wav"])
            .save_file()
        {
            match self.engine.start_recording(path.clone()) {
                Ok(()) => {
                    self.status = StatusState::RecordingTo(path);
                    self.error = None;
                }
                Err(e) => self.error = Some(ErrorState::Plain(e.to_string())),
            }
        }
    }

    fn update_meters(&mut self) {
        let snap = self.engine.shared.take_meters();
        let decay = 0.84_f32;
        self.meters.in_left = snap.in_left.max(self.meters.in_left * decay);
        self.meters.in_right = snap.in_right.max(self.meters.in_right * decay);
        self.meters.out_left = snap.out_left.max(self.meters.out_left * decay);
        self.meters.out_right = snap.out_right.max(self.meters.out_right * decay);

        let peak_decay = 0.985_f32;
        self.peak_hold.in_left = self.meters.in_left.max(self.peak_hold.in_left * peak_decay);
        self.peak_hold.in_right = self
            .meters
            .in_right
            .max(self.peak_hold.in_right * peak_decay);
        self.peak_hold.out_left = self
            .meters
            .out_left
            .max(self.peak_hold.out_left * peak_decay);
        self.peak_hold.out_right = self
            .meters
            .out_right
            .max(self.peak_hold.out_right * peak_decay);
    }

    fn render_splash_screen(&self, ui: &mut egui::Ui, elapsed: f32, duration: f32) {
        let t = self.lang.text();
        let progress = (elapsed / duration).clamp(0.0, 1.0);
        let screen_rect = ui.max_rect();

        let painter = ui.painter();
        painter.rect_filled(screen_rect, CornerRadius::ZERO, Color32::from_rgb(12, 13, 16));

        let card_w = 520.0_f32.min(screen_rect.width() - 32.0).max(280.0);
        let card_h = 320.0_f32.min(screen_rect.height() - 32.0).max(220.0);
        let card_rect = Rect::from_center_size(screen_rect.center(), Vec2::new(card_w, card_h));

        painter.rect_filled(
            card_rect,
            CornerRadius::same(6),
            Color32::from_rgb(18, 20, 26),
        );
        painter.rect_stroke(
            card_rect,
            CornerRadius::same(6),
            Stroke::new(1.2_f32, Color32::from_rgb(46, 52, 68)),
            StrokeKind::Inside,
        );

        let mut card_ui = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(card_rect.shrink(24.0))
                .layout(egui::Layout::top_down(egui::Align::Center)),
        );

        card_ui.add_space(8.0);

        let (logo_rect, _) = card_ui.allocate_exact_size(Vec2::new(48.0, 36.0), Sense::hover());
        let p = card_ui.painter();
        let anim_phase = (elapsed * 5.0) as usize;
        let bar_heights: [f32; 5] = [0.45, 0.85, 0.65, 0.38, 0.72];
        for (i, _) in bar_heights.iter().enumerate() {
            let step = (i + anim_phase) % bar_heights.len();
            let h_ratio = bar_heights[step].min(1.0);
            let bar_h = 8.0 + h_ratio * 22.0;
            let x = logo_rect.left() + 4.0 + i as f32 * 8.5;
            let r = Rect::from_center_size(
                Pos2::new(x, logo_rect.center().y),
                Vec2::new(4.5, bar_h),
            );
            let col = if progress > 0.85 {
                StudioPalette::ACCENT_EMERALD
            } else {
                StudioPalette::ACCENT_CYAN
            };
            p.rect_filled(r, CornerRadius::same(1), col);
        }

        card_ui.add_space(10.0);
        card_ui.label(
            RichText::new("CHRONOMIX")
                .size(28.0)
                .strong()
                .color(Color32::WHITE),
        );
        card_ui.label(
            RichText::new("STUDIO DAW & MULTITRACK CONSOLE")
                .size(11.0)
                .strong()
                .monospace()
                .color(Color32::from_rgb(160, 172, 190)),
        );

        card_ui.add_space(8.0);
        egui::Frame::new()
            .fill(Color32::from_rgb(26, 30, 40))
            .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 56, 72)))
            .corner_radius(CornerRadius::same(3))
            .inner_margin(Margin::symmetric(10, 3))
            .show(&mut card_ui, |ui| {
                ui.label(
                    RichText::new("64-BIT DSP · VST2 / VST3 · WINE BRIDGE · LOW-LATENCY")
                        .size(9.5)
                        .monospace()
                        .color(StudioPalette::ACCENT_CYAN),
                );
            });

        card_ui.add_space(18.0);

        let step_text = if progress < 0.28 {
            t.splash_init_engine
        } else if progress < 0.58 {
            t.splash_load_dsp
        } else if progress < 0.85 {
            t.splash_check_vst
        } else {
            t.splash_ready
        };

        card_ui.label(
            RichText::new(step_text)
                .size(12.0)
                .color(Color32::from_rgb(215, 222, 235)),
        );

        card_ui.add_space(8.0);

        let bar_total_w = (card_w - 64.0).max(100.0);
        let (bar_rect, _) = card_ui.allocate_exact_size(Vec2::new(bar_total_w, 6.0), Sense::hover());
        let bp = card_ui.painter();
        bp.rect_filled(bar_rect, CornerRadius::same(2), Color32::from_rgb(26, 28, 36));
        let fill_w = (bar_total_w * progress).clamp(4.0, bar_total_w);
        let fill_rect = Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, 6.0));
        bp.rect_filled(fill_rect, CornerRadius::same(2), Color32::from_rgb(230, 235, 245));

        card_ui.add_space(16.0);
        card_ui.label(
            RichText::new(t.splash_skip_hint)
                .size(10.0)
                .color(Color32::from_rgb(110, 118, 134)),
        );
    }

    /// Static, squared top navigation bar with vector logo, tabs, live status badge, and language selector.
    #[allow(dead_code)]
    fn render_main_toolbar(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();

        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = Vec2::new(8.0, 6.0);

            // Static Squared Studio Logo Mark (no animation)
            let (logo_rect, _) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::hover());
            if ui.is_rect_visible(logo_rect) {
                let painter = ui.painter();
                painter.rect_filled(
                    logo_rect,
                    CornerRadius::same(2),
                    Color32::from_rgb(20, 20, 24),
                );
                painter.rect_stroke(
                    logo_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, Color32::from_rgb(180, 185, 195)),
                    StrokeKind::Inside,
                );
                let static_bars: [f32; 4] = [0.45, 0.85, 0.65, 0.38];
                for (bar, frac) in static_bars.iter().enumerate() {
                    let h = 5.0 + frac * 13.0;
                    let x = logo_rect.left() + 5.5 + bar as f32 * 5.0;
                    let bar_rect = Rect::from_center_size(
                        Pos2::new(x, logo_rect.center().y),
                        Vec2::new(3.0, h),
                    );
                    painter.rect_filled(
                        bar_rect,
                        CornerRadius::same(1),
                        Color32::from_rgb(220, 225, 235),
                    );
                }
            }

            if ui.available_width() >= 560.0 {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.label(
                        RichText::new("CHRONOMIX")
                            .size(15.5)
                            .strong()
                            .color(Color32::WHITE),
                    );
                    ui.label(
                        RichText::new("STUDIO DAW  |  VST2 / VST3")
                            .size(9.5)
                            .color(Color32::from_rgb(170, 175, 185)),
                    );
                });
            } else {
                ui.label(
                    RichText::new("CHRONOMIX")
                        .size(15.0)
                        .strong()
                        .color(Color32::WHITE),
                );
            }

            ui.separator();

            // Squared Tab Buttons with Vector Icons
            for (panel, icon, label, id_salt) in [
                (
                    ActivePanel::News,
                    StudioIcon::Sparkle,
                    t.nav_news,
                    "nav_tab_news",
                ),
                (
                    ActivePanel::Recording,
                    StudioIcon::Mic,
                    t.nav_recording,
                    "nav_tab_recording",
                ),
                (
                    ActivePanel::Mixer,
                    StudioIcon::Sliders,
                    t.nav_mixer,
                    "nav_tab_mixer",
                ),
                (
                    ActivePanel::Settings,
                    StudioIcon::Gear,
                    t.nav_settings,
                    "nav_tab_settings",
                ),
            ] {
                let selected = self.active_panel == panel;
                let id = ui.make_persistent_id(id_salt);
                let galley = ui.painter().layout_no_wrap(
                    label.to_string(),
                    FontId::proportional(13.0),
                    Color32::WHITE,
                );
                let text_w = galley.size().x;
                let desired = Vec2::new(text_w + 38.0, 30.0);
                let (rect, resp) = ui.allocate_exact_size(desired, Sense::click());
                if resp.clicked() {
                    self.active_panel = panel;
                }
                if resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }

                let sel_t = ui.ctx().animate_bool_responsive(id.with("sel"), selected);
                let hov_t = ui
                    .ctx()
                    .animate_bool_responsive(id.with("hov"), resp.hovered());

                if ui.is_rect_visible(rect) {
                    let painter = ui.painter();
                    let bg = lerp_color(
                        lerp_color(StudioPalette::BG_PANEL, StudioPalette::BG_ELEVATED, hov_t),
                        Color32::from_rgb(42, 44, 52),
                        sel_t,
                    );
                    painter.rect_filled(rect, CornerRadius::same(2), bg);

                    let border_col = lerp_color(
                        StudioPalette::BORDER_SUBTLE,
                        Color32::from_rgb(180, 185, 195),
                        sel_t.max(hov_t * 0.5),
                    );
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(2),
                        Stroke::new(1.0_f32, border_col),
                        StrokeKind::Inside,
                    );

                    if sel_t > 0.02 {
                        let bar_w = (rect.width() - 8.0) * sel_t;
                        let bar_rect = Rect::from_center_size(
                            Pos2::new(rect.center().x, rect.bottom() - 1.5),
                            Vec2::new(bar_w.max(4.0), 2.0),
                        );
                        painter.rect_filled(
                            bar_rect,
                            CornerRadius::same(1),
                            Color32::WHITE,
                        );
                    }

                    let fg_col = lerp_color(
                        StudioPalette::TEXT_SECONDARY,
                        Color32::WHITE,
                        sel_t.max(hov_t * 0.7),
                    );
                    let icon_col = lerp_color(
                        StudioPalette::TEXT_SECONDARY,
                        Color32::WHITE,
                        sel_t.max(hov_t * 0.7),
                    );
                    let total_w = 13.0 + 6.0 + text_w;
                    let start_x = rect.center().x - total_w * 0.5;
                    draw_vector_icon(
                        painter,
                        Pos2::new(start_x + 6.5, rect.center().y),
                        12.5,
                        icon,
                        icon_col,
                    );
                    painter.text(
                        Pos2::new(start_x + 19.0, rect.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(13.0),
                        fg_col,
                    );
                }
            }

            ui.separator();

            // Status badges & language segmented control (wrapped inline so it never overlaps tabs on narrow Hyprland splits)
            if self.engine.shared.is_recording() || self.mixer_engine.is_recording() {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(239, 68, 68, 35))
                    .stroke(Stroke::new(1.0_f32, StudioPalette::ACCENT_CRIMSON))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                            ui.painter().rect_filled(
                                dot_rect,
                                CornerRadius::same(1),
                                StudioPalette::ACCENT_CRIMSON,
                            );
                            ui.label(
                                RichText::new(t.nav_rec_badge)
                                    .small()
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                        });
                    });
            } else if self.engine.is_audio_running() || self.mixer_engine.is_audio_running() {
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(16, 185, 129, 30))
                    .stroke(Stroke::new(1.0_f32, StudioPalette::ACCENT_EMERALD))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(8, 3))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (dot_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(7.0), Sense::hover());
                            ui.painter().rect_filled(
                                dot_rect,
                                CornerRadius::same(1),
                                StudioPalette::ACCENT_EMERALD,
                            );
                            ui.label(
                                RichText::new(t.nav_live_badge)
                                    .small()
                                    .strong()
                                    .color(StudioPalette::ACCENT_EMERALD),
                            );
                        });
                    });
            }

            egui::Frame::new()
                .fill(StudioPalette::BG_INPUT)
                .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                .corner_radius(CornerRadius::same(2))
                .inner_margin(Margin::symmetric(3, 2))
                .show(ui, |ui| {
                    ui.spacing_mut().item_spacing.x = 3.0;
                    for lang in [Lang::En, Lang::Es] {
                        let selected = self.lang == lang;
                        let btn = egui::Button::new(
                            RichText::new(lang.code())
                                .small()
                                .strong()
                                .color(if selected {
                                    Color32::WHITE
                                } else {
                                    StudioPalette::TEXT_SECONDARY
                                }),
                        )
                        .fill(if selected {
                            StudioPalette::ACCENT_BLUE
                        } else {
                            Color32::TRANSPARENT
                        })
                        .corner_radius(CornerRadius::same(2));
                        if ui
                            .add(btn)
                            .on_hover_text(format!("{}: {}", t.lang_label, lang.display_name()))
                            .clicked()
                        {
                            self.lang = lang;
                            self.save_settings();
                        }
                    }
                });
        });
    }

    /// Startup / What's New (`Novedades`) panel with static squared hero card and responsive 3-step quick workflow.
    fn render_news_panel(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);

                // Clean, static squared Hero Studio Card (adapts to any window width)
                studio_card(
                    ui,
                    Some(StudioPalette::ACCENT_CYAN),
                    Margin::same(16),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgba_unmultiplied(230, 234, 242, 25))
                                .stroke(Stroke::new(1.0_f32, StudioPalette::ACCENT_CYAN))
                                .corner_radius(CornerRadius::same(2))
                                .inner_margin(Margin::symmetric(8, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new("VST2  |  VST3  |  WINE X64  |  LOW-LATENCY")
                                            .size(10.5)
                                            .strong()
                                            .color(StudioPalette::ACCENT_CYAN),
                                    );
                                });

                            egui::Frame::new()
                                .fill(StudioPalette::BG_INPUT)
                                .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(2))
                                .inner_margin(Margin::symmetric(8, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(format!(
                                            "{} VSTs  |  {} Hz",
                                            self.discovered_plugins.len(),
                                            self.sample_rate
                                        ))
                                        .small()
                                        .monospace()
                                        .color(StudioPalette::TEXT_SECONDARY),
                                    );
                                });
                        });

                        ui.add_space(6.0);
                        ui.label(
                            RichText::new("Chronomix Studio")
                                .size(22.0)
                                .strong()
                                .color(Color32::WHITE),
                        );
                        ui.label(
                            RichText::new(t.news_hero_subtitle)
                                .size(14.0)
                                .color(StudioPalette::ACCENT_CYAN),
                        );
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(t.news_hero_description)
                                    .color(StudioPalette::TEXT_SECONDARY),
                            )
                            .wrap(),
                        );
                        ui.add_space(12.0);

                        ui.horizontal_wrapped(|ui| {
                            if animated_action_button(
                                ui,
                                "hero_go_rec",
                                Some(StudioIcon::Mic),
                                t.news_go_recording,
                                StudioPalette::ACCENT_EMERALD,
                                true,
                                Vec2::new(155.0, 32.0),
                            )
                            .clicked()
                            {
                                self.mixer_ui.active_tab = WingScreenTab::Home;
                                self.active_panel = ActivePanel::Mixer;
                            }

                            if animated_action_button(
                                ui,
                                "hero_open_vst_dir",
                                Some(StudioIcon::Folder),
                                t.settings_open_vst_folder,
                                StudioPalette::ACCENT_BLUE,
                                false,
                                Vec2::new(165.0, 32.0),
                            )
                            .clicked()
                            {
                                self.open_active_vst_folder();
                            }

                            if animated_action_button(
                                ui,
                                "hero_go_settings",
                                Some(StudioIcon::Gear),
                                t.nav_settings,
                                StudioPalette::BG_ELEVATED,
                                false,
                                Vec2::new(120.0, 32.0),
                            )
                            .clicked()
                            {
                                self.mixer_ui.active_tab = WingScreenTab::Settings;
                                self.active_panel = ActivePanel::Mixer;
                            }
                        });
                    },
                );

                ui.add_space(12.0);

                // 3-Step Quick Workflow Cards (3 columns on wide windows, 1 column on narrow tiled windows)
                icon_heading(
                    ui,
                    StudioIcon::Bolt,
                    StudioPalette::ACCENT_AMBER,
                    t.news_quickstart_title.trim(),
                    15.5,
                );
                ui.add_space(6.0);

                let steps = [
                    (
                        t.news_step1_title,
                        t.news_step1_desc,
                        StudioPalette::ACCENT_PURPLE,
                        ActivePanel::Settings,
                    ),
                    (
                        t.news_step2_title,
                        t.news_step2_desc,
                        StudioPalette::ACCENT_EMERALD,
                        ActivePanel::Recording,
                    ),
                    (
                        t.news_step3_title,
                        t.news_step3_desc,
                        StudioPalette::ACCENT_CYAN,
                        ActivePanel::Recording,
                    ),
                ];

                let mut target_panel: Option<ActivePanel> = None;
                let render_step = |ui: &mut egui::Ui,
                                   title: &str,
                                   desc: &str,
                                   accent: Color32,
                                   panel: ActivePanel,
                                   target: &mut Option<ActivePanel>| {
                    studio_card(ui, Some(accent), Margin::same(12), |ui| {
                        ui.label(RichText::new(title).size(14.0).strong().color(accent));
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(desc)
                                    .small()
                                    .color(StudioPalette::TEXT_SECONDARY),
                            )
                            .wrap(),
                        );
                        ui.add_space(6.0);
                        let go_label = if panel == ActivePanel::Settings {
                            format!("{} ->", t.nav_settings)
                        } else {
                            format!("{} ->", t.nav_recording)
                        };
                        if ui.small_button(RichText::new(go_label).color(accent)).clicked() {
                            *target = Some(panel);
                        }
                    });
                };

                if ui.available_width() >= 680.0 {
                    ui.columns(3, |cols| {
                        for (col_idx, (title, desc, accent, panel)) in steps.into_iter().enumerate() {
                            render_step(
                                &mut cols[col_idx],
                                title,
                                desc,
                                accent,
                                panel,
                                &mut target_panel,
                            );
                        }
                    });
                } else {
                    for (title, desc, accent, panel) in steps {
                        render_step(ui, title, desc, accent, panel, &mut target_panel);
                        ui.add_space(6.0);
                    }
                }

                if let Some(p) = target_panel {
                    match p {
                        ActivePanel::Settings => {
                            self.mixer_ui.active_tab = WingScreenTab::Settings;
                            self.active_panel = ActivePanel::Mixer;
                        }
                        _ => {
                            self.mixer_ui.active_tab = WingScreenTab::Home;
                            self.active_panel = ActivePanel::Mixer;
                        }
                    }
                }

                ui.add_space(12.0);
                icon_heading(
                    ui,
                    StudioIcon::Sparkle,
                    StudioPalette::ACCENT_CYAN,
                    t.news_section_title,
                    15.5,
                );
                ui.add_space(6.0);

                for item in t.news_items {
                    studio_card(ui, None, Margin::same(12), |ui| {
                        ui.horizontal_wrapped(|ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgba_unmultiplied(200, 204, 215, 30))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(100, 105, 115)))
                                .corner_radius(CornerRadius::same(2))
                                .inner_margin(Margin::symmetric(7, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(item.tag)
                                            .small()
                                            .strong()
                                            .color(StudioPalette::ACCENT_CYAN),
                                    );
                                });
                            ui.label(
                                RichText::new(item.title)
                                    .size(14.0)
                                    .strong()
                                    .color(Color32::WHITE),
                            );
                        });
                        ui.add_space(4.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(item.summary).color(StudioPalette::TEXT_SECONDARY),
                            )
                            .wrap(),
                        );
                    });
                    ui.add_space(6.0);
                }
            });
    }

    /// Settings (`Ajustes`) panel for managing the program VST folder, custom VST folder, and VST2/VST3 library.
    fn render_settings_panel(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        let active_dir = self.active_vst_dir();
        let is_custom = self.custom_vst_dir.is_some();

        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.add_space(4.0);

                studio_card(
                    ui,
                    Some(StudioPalette::ACCENT_BLUE),
                    Margin::same(14),
                    |ui| {
                        ui.horizontal_wrapped(|ui| {
                            icon_heading(
                                ui,
                                StudioIcon::Gear,
                                StudioPalette::ACCENT_CYAN,
                                t.settings_title,
                                18.0,
                            );
                            let (badge_text, badge_color) = if is_custom {
                                (t.settings_mode_custom, StudioPalette::ACCENT_AMBER)
                            } else {
                                (t.settings_mode_program, StudioPalette::ACCENT_EMERALD)
                            };
                            draw_status_pill(ui, badge_text, badge_color);
                        });

                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(t.settings_subtitle).color(StudioPalette::TEXT_SECONDARY),
                        );
                        ui.add_space(8.0);
                        ui.label(
                            RichText::new(t.settings_vst_folder_heading)
                                .size(14.5)
                                .strong()
                                .color(StudioPalette::ACCENT_CYAN),
                        );
                        ui.add_space(6.0);

                        // Active VST Folder Path Display Box
                        egui::Frame::new()
                            .fill(StudioPalette::BG_INPUT)
                            .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_BRIGHT))
                            .corner_radius(CornerRadius::same(2))
                            .inner_margin(Margin::symmetric(12, 8))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    let (ic_rect, _) =
                                        ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                                    draw_vector_icon(
                                        ui.painter(),
                                        ic_rect.center(),
                                        14.0,
                                        StudioIcon::Folder,
                                        StudioPalette::ACCENT_CYAN,
                                    );
                                    ui.label(
                                        RichText::new(t.settings_active_folder_label)
                                            .strong()
                                            .color(StudioPalette::TEXT_SECONDARY),
                                    );
                                    ui.label(
                                        RichText::new(active_dir.display().to_string())
                                            .monospace()
                                            .strong()
                                            .color(StudioPalette::ACCENT_CYAN),
                                    );
                                });
                            });

                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(
                                RichText::new(t.settings_vst_folder_desc)
                                    .small()
                                    .color(StudioPalette::TEXT_SECONDARY),
                            )
                            .wrap(),
                        );
                        ui.add_space(10.0);

                        // Action Buttons Row
                        ui.horizontal_wrapped(|ui| {
                            if animated_action_button(
                                ui,
                                "settings_open_vst_folder",
                                Some(StudioIcon::Folder),
                                t.settings_open_vst_folder,
                                StudioPalette::ACCENT_EMERALD,
                                true,
                                Vec2::new(165.0, 32.0),
                            )
                            .clicked()
                            {
                                self.open_active_vst_folder();
                            }

                            if animated_action_button(
                                ui,
                                "settings_choose_custom",
                                Some(StudioIcon::Folder),
                                t.settings_choose_custom_folder,
                                StudioPalette::ACCENT_BLUE,
                                false,
                                Vec2::new(180.0, 32.0),
                            )
                            .clicked()
                            {
                                if let Some(folder) = rfd::FileDialog::new()
                                    .set_title(t.dialog_select_vst_folder)
                                    .set_directory(&active_dir)
                                    .pick_folder()
                                {
                                    let _ = std::fs::create_dir_all(&folder);
                                    self.custom_vst_dir = Some(folder.clone());
                                    self.save_settings();
                                    self.status = StatusState::UpdatedVstFolder(folder);
                                    self.start_scan();
                                }
                            }

                            if is_custom
                                && animated_action_button(
                                    ui,
                                    "settings_reset_program_folder",
                                    Some(StudioIcon::Refresh),
                                    t.settings_use_program_folder,
                                    StudioPalette::ACCENT_AMBER,
                                    false,
                                    Vec2::new(165.0, 32.0),
                                )
                                .clicked()
                            {
                                self.custom_vst_dir = None;
                                self.save_settings();
                                let prog_dir = self.program_vst_dir.clone();
                                self.status = StatusState::UpdatedVstFolder(prog_dir);
                                self.start_scan();
                            }

                            let scan_label = if self.is_scanning {
                                t.scanning
                            } else {
                                t.settings_scan_now
                            };
                            if animated_action_button(
                                ui,
                                "settings_scan_now",
                                Some(StudioIcon::Refresh),
                                scan_label,
                                StudioPalette::ACCENT_PURPLE,
                                self.is_scanning,
                                Vec2::new(150.0, 32.0),
                            )
                            .clicked()
                            {
                                self.start_scan();
                            }
                        });

                        ui.add_space(8.0);
                        if animated_led_toggle(
                            ui,
                            "settings_include_sys_toggle",
                            &mut self.include_system_vst,
                            t.settings_include_system_vst,
                            StudioPalette::ACCENT_CYAN,
                        ) {
                            self.save_settings();
                            self.start_scan();
                        }
                    },
                );

                ui.add_space(12.0);

                if cfg!(target_os = "linux") {
                    self.render_wine_settings_card(ui);
                    ui.add_space(12.0);
                }

                // Detected Plugins Header + Search Filter (wraps cleanly on narrow windows)
                ui.horizontal_wrapped(|ui| {
                    icon_heading(
                        ui,
                        StudioIcon::Sliders,
                        StudioPalette::ACCENT_PURPLE,
                        t.settings_detected_heading,
                        16.0,
                    );
                    egui::Frame::new()
                        .fill(StudioPalette::BG_ELEVATED)
                        .corner_radius(CornerRadius::same(2))
                        .inner_margin(Margin::symmetric(7, 2))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("{}", self.discovered_plugins.len()))
                                    .small()
                                    .strong()
                                    .color(StudioPalette::ACCENT_CYAN),
                            );
                        });

                    let (sr, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                    draw_vector_icon(
                        ui.painter(),
                        sr.center(),
                        13.0,
                        StudioIcon::Search,
                        StudioPalette::TEXT_SECONDARY,
                    );
                    let search_w = (ui.available_width() - 12.0).clamp(120.0, 240.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.library_filter)
                            .desired_width(search_w)
                            .hint_text(t.settings_search_hint),
                    );
                });
                ui.add_space(6.0);

                if self.discovered_plugins.is_empty() {
                    studio_card(ui, None, Margin::same(14), |ui| {
                        ui.vertical_centered(|ui| {
                            let (r, _) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::hover());
                            draw_vector_icon(
                                ui.painter(),
                                r.center(),
                                24.0,
                                StudioIcon::Folder,
                                StudioPalette::TEXT_MUTED,
                            );
                            ui.add_space(4.0);
                            ui.add(
                                egui::Label::new(
                                    RichText::new(t.settings_no_plugins_in_folder)
                                        .color(StudioPalette::TEXT_SECONDARY),
                                )
                                .wrap(),
                            );
                        });
                    });
                } else {
                    let filter_q = self.library_filter.trim().to_lowercase();
                    let mut plugin_to_load: Option<PathBuf> = None;

                    for (idx, plugin) in self.discovered_plugins.iter().enumerate() {
                        if !filter_q.is_empty()
                            && !plugin.name.to_lowercase().contains(&filter_q)
                            && !plugin.vendor.to_lowercase().contains(&filter_q)
                            && !plugin.category.to_lowercase().contains(&filter_q)
                        {
                            continue;
                        }

                        let is_vst2 = plugin.category.contains("VST2")
                            || plugin
                                .path
                                .extension()
                                .and_then(|e| e.to_str())
                                .is_some_and(|e| {
                                    e.eq_ignore_ascii_case("dll") || e.eq_ignore_ascii_case("so")
                                });
                        let badge_label = if !plugin.category.is_empty() {
                            plugin.category.as_str()
                        } else if is_vst2 {
                            "VST2"
                        } else {
                            "VST3"
                        };
                        let badge_color = if is_vst2 {
                            StudioPalette::ACCENT_PURPLE
                        } else {
                            StudioPalette::ACCENT_CYAN
                        };

                        studio_card(ui, Some(badge_color), Margin::symmetric(12, 8), |ui| {
                            ui.horizontal_wrapped(|ui| {
                                egui::Frame::new()
                                    .fill(Color32::from_rgba_unmultiplied(
                                        badge_color.r(),
                                        badge_color.g(),
                                        badge_color.b(),
                                        40,
                                    ))
                                    .stroke(Stroke::new(1.0_f32, badge_color))
                                    .corner_radius(CornerRadius::same(2))
                                    .inner_margin(Margin::symmetric(7, 2))
                                    .show(ui, |ui| {
                                        ui.label(
                                            RichText::new(badge_label)
                                                .small()
                                                .strong()
                                                .color(badge_color),
                                        );
                                    });

                                ui.label(
                                    RichText::new(&plugin.name)
                                        .size(14.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                ui.label(
                                    RichText::new(format!("| {}", plugin.vendor))
                                        .small()
                                        .color(StudioPalette::TEXT_SECONDARY),
                                );

                                if animated_action_button(
                                    ui,
                                    &format!("load_lib_plugin_{idx}"),
                                    Some(StudioIcon::Play),
                                    t.settings_load_in_recording,
                                    StudioPalette::ACCENT_EMERALD,
                                    false,
                                    Vec2::new(140.0, 26.0),
                                )
                                .clicked()
                                {
                                    plugin_to_load = Some(plugin.path.clone());
                                }
                            });

                            ui.add(
                                egui::Label::new(
                                    RichText::new(plugin.path.display().to_string())
                                        .small()
                                        .monospace()
                                        .color(StudioPalette::TEXT_MUTED),
                                )
                                .truncate(),
                            );
                        });
                        ui.add_space(5.0);
                    }

                    if let Some(path) = plugin_to_load {
                        let target = match self.mixer_ui.selection {
                            WingSelection::Channel(ch) => MixerInsertTarget::Channel {
                                ch,
                                slot: self.mixer_ui.focused_vst_slot.min(3),
                            },
                            WingSelection::Master => MixerInsertTarget::Master {
                                slot: self.mixer_ui.focused_vst_slot.min(3),
                            },
                        };
                        let _ = self.mixer_engine.load_insert_plugin(target, &path);
                        self.mixer_ui.active_tab = WingScreenTab::Vst;
                        self.active_panel = ActivePanel::Mixer;
                    }
                }
            });
    }

    fn render_wine_settings_card(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        let status_opt = self.wine_status.clone();
        let wine_busy = self.wine_busy_msg.is_some();

        studio_card(
            ui,
            Some(StudioPalette::ACCENT_CYAN),
            Margin::same(14),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    icon_heading(
                        ui,
                        StudioIcon::Bolt,
                        StudioPalette::ACCENT_CYAN,
                        t.settings_wine_heading,
                        16.5,
                    );

                    if let Some(ref st) = status_opt {
                        if st.active_wine_binary.is_none() {
                            draw_status_pill(
                                ui,
                                t.settings_wine_status_missing,
                                StudioPalette::ACCENT_CRIMSON,
                            );
                        } else if st.active_prefix_initialized {
                            draw_status_pill(
                                ui,
                                t.settings_wine_status_ready,
                                StudioPalette::ACCENT_EMERALD,
                            );
                        } else {
                            draw_status_pill(
                                ui,
                                t.settings_wine_status_uninit,
                                StudioPalette::ACCENT_AMBER,
                            );
                        }

                        if st.wayland_x11_configured {
                            draw_status_pill(
                                ui,
                                t.settings_wine_driver_ok,
                                StudioPalette::ACCENT_CYAN,
                            );
                        } else {
                            draw_status_pill(
                                ui,
                                t.settings_wine_driver_pending,
                                StudioPalette::ACCENT_AMBER,
                            );
                        }

                        let (bridge_lbl, bridge_col) = match st.bridge_mode {
                            WineBridgeMode::EmbeddedPrebuilt => {
                                (t.settings_wine_bridge_prebuilt, StudioPalette::ACCENT_EMERALD)
                            }
                            WineBridgeMode::RuntimeCached => {
                                (t.settings_wine_bridge_cached, StudioPalette::ACCENT_CYAN)
                            }
                            WineBridgeMode::CanCompileAtRuntime => {
                                (t.settings_wine_bridge_compile, StudioPalette::ACCENT_BLUE)
                            }
                            WineBridgeMode::Unavailable => {
                                (t.settings_wine_bridge_missing, StudioPalette::ACCENT_CRIMSON)
                            }
                        };
                        draw_status_pill(ui, bridge_lbl, bridge_col);
                    }
                });

                ui.add_space(4.0);
                ui.add(
                    egui::Label::new(
                        RichText::new(t.settings_wine_desc)
                            .small()
                            .color(StudioPalette::TEXT_SECONDARY),
                    )
                    .wrap(),
                );

                if let Some(ref st) = status_opt {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t.settings_wine_session_label)
                                .small()
                                .color(StudioPalette::TEXT_MUTED),
                        );
                        ui.label(
                            RichText::new(&st.display_session)
                                .small()
                                .monospace()
                                .color(StudioPalette::ACCENT_CYAN),
                        );
                        if let Some(ref ver) = st.active_wine_version {
                            ui.label(
                                RichText::new(format!("• {ver}"))
                                    .small()
                                    .monospace()
                                    .color(StudioPalette::TEXT_SECONDARY),
                            );
                        }
                    });
                }

                ui.add_space(8.0);
                let combo_w = (ui.available_width() - 18.0).clamp(150.0, 440.0);

                // Wine runner selector
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(t.settings_wine_runner_label)
                            .strong()
                            .color(StudioPalette::TEXT_SECONDARY),
                    );
                    let runner_text = match &self.wine_config.wine_binary {
                        None => {
                            let auto_suffix = status_opt
                                .as_ref()
                                .and_then(|s| s.active_wine_version.as_deref())
                                .unwrap_or("");
                            if auto_suffix.is_empty() {
                                t.settings_wine_auto_label.to_string()
                            } else {
                                format!("{} ({auto_suffix})", t.settings_wine_auto_label)
                            }
                        }
                        Some(p) => status_opt
                            .as_ref()
                            .and_then(|s| s.available_runners.iter().find(|r| &r.path == p))
                            .map(|r| r.label.clone())
                            .unwrap_or_else(|| p.display().to_string()),
                    };

                    let mut next_runner: Option<Option<PathBuf>> = None;
                    egui::ComboBox::from_id_salt("wine_runner_combo")
                        .width(combo_w)
                        .selected_text(runner_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.wine_config.wine_binary.is_none(),
                                    t.settings_wine_auto_label,
                                )
                                .clicked()
                            {
                                next_runner = Some(None);
                            }
                            if let Some(ref st) = status_opt {
                                for runner in &st.available_runners {
                                    let selected = self
                                        .wine_config
                                        .wine_binary
                                        .as_ref()
                                        .is_some_and(|p| p == &runner.path);
                                    if ui.selectable_label(selected, &runner.label).clicked() {
                                        next_runner = Some(Some(runner.path.clone()));
                                    }
                                }
                            }
                        });
                    if let Some(chosen) = next_runner {
                        self.wine_config.wine_binary = chosen;
                        self.sync_and_save_wine_config();
                    }

                    if animated_action_button(
                        ui,
                        "wine_choose_runner",
                        Some(StudioIcon::File),
                        t.settings_wine_choose_runner,
                        StudioPalette::ACCENT_BLUE,
                        false,
                        Vec2::new(126.0, 26.0),
                    )
                    .clicked()
                    {
                        if let Some(bin) = rfd::FileDialog::new()
                            .set_title(t.dialog_select_wine_runner)
                            .set_directory("/usr/bin")
                            .pick_file()
                        {
                            self.wine_config.wine_binary = Some(bin);
                            self.sync_and_save_wine_config();
                        }
                    }
                });

                ui.add_space(6.0);

                // WINEPREFIX selector
                let resolved_prefix = status_opt
                    .as_ref()
                    .and_then(|s| s.active_prefix.clone())
                    .or_else(|| self.wine_config.wine_prefix.clone());
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(t.settings_wine_prefix_label)
                            .strong()
                            .color(StudioPalette::TEXT_SECONDARY),
                    );
                    let prefix_text = match &self.wine_config.wine_prefix {
                        None => match &resolved_prefix {
                            Some(p) => format!("{} — {}", t.settings_wine_auto_label, p.display()),
                            None => t.settings_wine_auto_label.to_string(),
                        },
                        Some(p) => p.display().to_string(),
                    };

                    let mut next_prefix: Option<Option<PathBuf>> = None;
                    egui::ComboBox::from_id_salt("wine_prefix_combo")
                        .width(combo_w)
                        .selected_text(prefix_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_label(
                                    self.wine_config.wine_prefix.is_none(),
                                    t.settings_wine_auto_label,
                                )
                                .clicked()
                            {
                                next_prefix = Some(None);
                            }
                            if let Some(ref st) = status_opt {
                                for pref in &st.available_prefixes {
                                    let selected = self
                                        .wine_config
                                        .wine_prefix
                                        .as_ref()
                                        .is_some_and(|p| p == &pref.path);
                                    let mark = if pref.initialized { "[OK]" } else { "[--]" };
                                    let label =
                                        format!("{mark} {} — {}", pref.name, pref.path.display());
                                    if ui.selectable_label(selected, label).clicked() {
                                        next_prefix = Some(Some(pref.path.clone()));
                                    }
                                }
                            }
                        });
                    if let Some(chosen) = next_prefix {
                        self.wine_config.wine_prefix = chosen;
                        self.sync_and_save_wine_config();
                    }

                    if animated_action_button(
                        ui,
                        "wine_choose_prefix",
                        Some(StudioIcon::Folder),
                        t.settings_wine_choose_prefix,
                        StudioPalette::ACCENT_BLUE,
                        false,
                        Vec2::new(124.0, 26.0),
                    )
                    .clicked()
                    {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title(t.dialog_select_wine_prefix)
                            .pick_folder()
                        {
                            self.wine_config.wine_prefix = Some(folder);
                            self.sync_and_save_wine_config();
                        }
                    }

                    if (self.wine_config.wine_prefix.is_some()
                        || self.wine_config.wine_binary.is_some())
                        && animated_action_button(
                            ui,
                            "wine_reset_auto",
                            Some(StudioIcon::Refresh),
                            t.settings_wine_reset_auto,
                            StudioPalette::ACCENT_AMBER,
                            false,
                            Vec2::new(92.0, 26.0),
                        )
                        .clicked()
                    {
                        self.wine_config.wine_prefix = None;
                        self.wine_config.wine_binary = None;
                        self.sync_and_save_wine_config();
                    }

                    if let Some(pref_dir) = resolved_prefix.clone() {
                        if animated_action_button(
                            ui,
                            "wine_open_drive_c",
                            Some(StudioIcon::Folder),
                            t.settings_wine_open_drive_c,
                            StudioPalette::ACCENT_CYAN,
                            false,
                            Vec2::new(118.0, 26.0),
                        )
                        .clicked()
                        {
                            let drive_c = pref_dir.join("drive_c");
                            let target = if drive_c.is_dir() { drive_c } else { pref_dir };
                            let _ = std::fs::create_dir_all(&target);
                            if let Err(e) = open_folder_in_file_manager(&target) {
                                self.error = Some(ErrorState::Plain(e));
                            } else {
                                self.status = StatusState::OpenedFolder(target);
                                self.error = None;
                            }
                        }
                    }
                });

                ui.add_space(6.0);
                if animated_led_toggle(
                    ui,
                    "wine_auto_display_toggle",
                    &mut self.wine_config.auto_display,
                    t.settings_wine_auto_display,
                    StudioPalette::ACCENT_CYAN,
                ) {
                    self.sync_and_save_wine_config();
                }

                ui.add_space(8.0);
                if let Some(ref busy) = self.wine_busy_msg {
                    ui.horizontal_wrapped(|ui| {
                        ui.spinner();
                        ui.label(
                            RichText::new(busy)
                                .small()
                                .strong()
                                .color(StudioPalette::ACCENT_AMBER),
                        );
                    });
                    ui.add_space(6.0);
                }

                // Primary Wine Actions Row
                ui.add_enabled_ui(!wine_busy, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if animated_action_button(
                            ui,
                            "wine_init_repair",
                            Some(StudioIcon::Bolt),
                            t.settings_wine_init_repair,
                            StudioPalette::ACCENT_EMERALD,
                            true,
                            Vec2::new(188.0, 30.0),
                        )
                        .clicked()
                        {
                            let ok_msg = t.settings_wine_init_ok.to_string();
                            self.spawn_wine_task(
                                t.settings_wine_busy_init.to_string(),
                                true,
                                move |cfg| {
                                    initialize_wine_prefix(&cfg)
                                        .map(|p| format!("{ok_msg} ({})", p.display()))
                                },
                            );
                        }

                        if animated_action_button(
                            ui,
                            "wine_run_exe",
                            Some(StudioIcon::Play),
                            t.settings_wine_run_exe,
                            StudioPalette::ACCENT_CYAN,
                            false,
                            Vec2::new(165.0, 30.0),
                        )
                        .clicked()
                        {
                            let mut dlg = rfd::FileDialog::new()
                                .set_title(t.dialog_select_exe)
                                .add_filter(
                                    "Windows App / Installer (.exe, .msi)",
                                    &["exe", "EXE", "msi", "MSI", "bat", "cmd"],
                                );
                            if let Some(ref pref) = resolved_prefix {
                                let dc = pref.join("drive_c");
                                if dc.is_dir() {
                                    dlg = dlg.set_directory(dc);
                                }
                            }
                            if let Some(file) = dlg.pick_file() {
                                self.spawn_wine_task(
                                    t.settings_wine_busy_run.to_string(),
                                    true,
                                    move |cfg| launch_wine_executable(&file, &cfg),
                                );
                            }
                        }

                        if animated_action_button(
                            ui,
                            "wine_open_winecfg",
                            Some(StudioIcon::Gear),
                            "winecfg",
                            StudioPalette::ACCENT_BLUE,
                            false,
                            Vec2::new(98.0, 30.0),
                        )
                        .clicked()
                        {
                            self.spawn_wine_task(
                                format!("{} (winecfg)", t.settings_wine_busy_tool),
                                false,
                                |cfg| launch_wine_tool(WineAction::Winecfg, &cfg),
                            );
                        }

                        if animated_action_button(
                            ui,
                            "wine_open_explorer",
                            Some(StudioIcon::Window),
                            "Explorer",
                            StudioPalette::ACCENT_BLUE,
                            false,
                            Vec2::new(104.0, 30.0),
                        )
                        .clicked()
                        {
                            self.spawn_wine_task(
                                format!("{} (explorer)", t.settings_wine_busy_tool),
                                false,
                                |cfg| launch_wine_tool(WineAction::Explorer, &cfg),
                            );
                        }

                        if animated_action_button(
                            ui,
                            "wine_rebuild_bridge",
                            Some(StudioIcon::Refresh),
                            t.settings_wine_rebuild_bridge,
                            StudioPalette::ACCENT_PURPLE,
                            false,
                            Vec2::new(172.0, 30.0),
                        )
                        .clicked()
                        {
                            let ok_msg = t.settings_wine_bridge_rebuilt_ok.to_string();
                            self.spawn_wine_task(
                                t.settings_wine_busy_bridge.to_string(),
                                false,
                                move |cfg| {
                                    rebuild_wine_bridge(&cfg)
                                        .map(|(p, _)| format!("{ok_msg} ({})", p.display()))
                                        .map_err(|e| e.to_string())
                                },
                            );
                        }
                    });

                    ui.add_space(6.0);

                    // Secondary Wine Utilities Row
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t.settings_wine_tools_label)
                                .small()
                                .color(StudioPalette::TEXT_MUTED),
                        );

                        if status_opt.as_ref().is_some_and(|s| s.winetricks_available)
                            && ui.small_button("Winetricks").clicked()
                        {
                            self.spawn_wine_task(
                                format!("{} (winetricks)", t.settings_wine_busy_tool),
                                true,
                                |cfg| launch_wine_tool(WineAction::Winetricks, &cfg),
                            );
                        }
                        if ui.small_button("CMD").clicked() {
                            self.spawn_wine_task(
                                format!("{} (cmd)", t.settings_wine_busy_tool),
                                false,
                                |cfg| launch_wine_tool(WineAction::Cmd, &cfg),
                            );
                        }
                        if ui.small_button("Regedit").clicked() {
                            self.spawn_wine_task(
                                format!("{} (regedit)", t.settings_wine_busy_tool),
                                false,
                                |cfg| launch_wine_tool(WineAction::Regedit, &cfg),
                            );
                        }
                        if ui.small_button("Uninstaller").clicked() {
                            self.spawn_wine_task(
                                format!("{} (uninstaller)", t.settings_wine_busy_tool),
                                true,
                                |cfg| launch_wine_tool(WineAction::Uninstaller, &cfg),
                            );
                        }
                        if ui.small_button(t.settings_wine_kill_server).clicked() {
                            self.spawn_wine_task(
                                format!("{} (wineserver -k)", t.settings_wine_busy_tool),
                                false,
                                |cfg| launch_wine_tool(WineAction::KillServer, &cfg),
                            );
                        }
                        if ui.small_button(t.refresh).clicked() {
                            self.refresh_wine_status();
                        }
                    });
                });

                if let Some(ref st) = status_opt {
                    if !st.vst_directories.is_empty() {
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            if ui
                                .small_button(format!(
                                    "{} ({})",
                                    t.settings_wine_vst_paths_title,
                                    st.vst_directories.len()
                                ))
                                .clicked()
                            {
                                self.show_wine_paths = !self.show_wine_paths;
                            }
                        });
                        if self.show_wine_paths {
                            ui.add_space(4.0);
                            egui::Frame::new()
                                .fill(StudioPalette::BG_INPUT)
                                .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                                .corner_radius(CornerRadius::same(2))
                                .inner_margin(Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    for dir in &st.vst_directories {
                                        let exists = dir.is_dir();
                                        let (tag, col) = if exists {
                                            ("[OK]", StudioPalette::ACCENT_EMERALD)
                                        } else {
                                            ("[--]", StudioPalette::TEXT_MUTED)
                                        };
                                        ui.horizontal_wrapped(|ui| {
                                            ui.label(
                                                RichText::new(tag)
                                                    .small()
                                                    .monospace()
                                                    .color(col),
                                            );
                                            ui.label(
                                                RichText::new(dir.display().to_string())
                                                    .small()
                                                    .monospace()
                                                    .color(StudioPalette::TEXT_SECONDARY),
                                            );
                                        });
                                    }
                                });
                        }
                    }
                }
            },
        );
    }

    /// Visual signal-flow bar (`Input -> VST Effect -> Output -> Record`) at the top of Recording.
    fn render_signal_chain_banner(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        let time = ui.input(|i| i.time);
        let running = self.engine.is_audio_running();
        let recording = self.engine.shared.is_recording();

        let in_name = self
            .input_devices
            .iter()
            .find(|d| d.id == self.selected_input)
            .map(|d| d.name.as_str())
            .unwrap_or(t.none);
        let out_name = self
            .output_devices
            .iter()
            .find(|d| d.id == self.selected_output)
            .map(|d| d.name.as_str())
            .unwrap_or(t.none);
        let fx_name = self
            .loaded_plugin
            .as_ref()
            .map(|m| m.info.name.as_str())
            .unwrap_or(t.chain_clean);

        let show_pipes = ui.available_width() >= 680.0;

        studio_card(ui, None, Margin::symmetric(12, 8), |ui| {
            ui.horizontal_wrapped(|ui| {
                // Node 1: Input
                draw_signal_node(
                    ui,
                    StudioIcon::Mic,
                    t.chain_input,
                    in_name,
                    if running {
                        StudioPalette::ACCENT_EMERALD
                    } else {
                        StudioPalette::ACCENT_BLUE
                    },
                );

                if show_pipes {
                    draw_animated_signal_pipe(ui, running, time, StudioPalette::ACCENT_EMERALD);
                }

                // Node 2: VST Effect
                let fx_col = if self.loaded_plugin.is_some() && !self.bypass {
                    StudioPalette::ACCENT_PURPLE
                } else if self.bypass {
                    StudioPalette::ACCENT_AMBER
                } else {
                    StudioPalette::ACCENT_CYAN
                };
                draw_signal_node(ui, StudioIcon::Sliders, t.chain_effect, fx_name, fx_col);

                if show_pipes {
                    draw_animated_signal_pipe(
                        ui,
                        running && !self.monitor_muted,
                        time,
                        StudioPalette::ACCENT_CYAN,
                    );
                }

                // Node 3: Output
                draw_signal_node(
                    ui,
                    StudioIcon::Headphones,
                    t.chain_output,
                    out_name,
                    if running && !self.monitor_muted {
                        StudioPalette::ACCENT_CYAN
                    } else {
                        StudioPalette::TEXT_MUTED
                    },
                );

                if show_pipes {
                    draw_animated_signal_pipe(ui, recording, time, StudioPalette::ACCENT_CRIMSON);
                }

                // Node 4: WAV Recorder
                let rec_sub = if recording {
                    t.nav_rec_badge
                } else {
                    t.chain_ready
                };
                draw_signal_node(
                    ui,
                    StudioIcon::Record,
                    t.chain_record,
                    rec_sub,
                    if recording {
                        StudioPalette::ACCENT_CRIMSON
                    } else {
                        StudioPalette::TEXT_MUTED
                    },
                );
            });
        });
    }

    fn render_recording_transport(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        let running = self.engine.is_audio_running();
        let recording = self.engine.shared.is_recording();

        studio_card(
            ui,
            Some(if recording {
                StudioPalette::ACCENT_CRIMSON
            } else if running {
                StudioPalette::ACCENT_EMERALD
            } else {
                StudioPalette::BORDER_SUBTLE
            }),
            Margin::symmetric(12, 8),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    let (monitor_icon, monitor_label, monitor_col) = if running {
                        (
                            StudioIcon::Stop,
                            t.stop_monitoring,
                            StudioPalette::ACCENT_CRIMSON,
                        )
                    } else {
                        (
                            StudioIcon::Play,
                            t.start_monitoring,
                            StudioPalette::ACCENT_EMERALD,
                        )
                    };
                    if animated_action_button(
                        ui,
                        "transport_monitor_btn",
                        Some(monitor_icon),
                        monitor_label,
                        monitor_col,
                        running,
                        Vec2::new(155.0, 32.0),
                    )
                    .clicked()
                    {
                        self.toggle_audio();
                    }

                    let (rec_icon, rec_label) = if recording {
                        let secs = self.engine.shared.recorded_frames() as f64
                            / self.engine.sample_rate().max(1) as f64;
                        (StudioIcon::Stop, self.lang.stop_rec_label(secs))
                    } else {
                        (StudioIcon::Record, t.record_wav.to_string())
                    };
                    if animated_action_button(
                        ui,
                        "transport_rec_btn",
                        Some(rec_icon),
                        &rec_label,
                        StudioPalette::ACCENT_CRIMSON,
                        recording,
                        Vec2::new(135.0, 32.0),
                    )
                    .clicked()
                    {
                        self.toggle_recording();
                    }

                    if animated_led_toggle(
                        ui,
                        "transport_mute_toggle",
                        &mut self.monitor_muted,
                        t.mute_output,
                        StudioPalette::ACCENT_AMBER,
                    ) {
                        self.engine.shared.set_monitor_muted(self.monitor_muted);
                    }

                    ui.separator();

                    let prev_sr = self.sample_rate;
                    ui.label(
                        RichText::new(t.sample_rate_label)
                            .small()
                            .color(StudioPalette::TEXT_SECONDARY),
                    );
                    egui::ComboBox::from_id_salt("sr_combo")
                        .width(88.0)
                        .selected_text(format!("{} Hz", self.sample_rate))
                        .show_ui(ui, |ui| {
                            for sr in [44_100, 48_000, 96_000] {
                                ui.selectable_value(&mut self.sample_rate, sr, format!("{sr} Hz"));
                            }
                        });
                    if self.sample_rate != prev_sr {
                        self.restart_audio_if_running();
                    }

                    let prev_bs = self.block_size;
                    ui.label(
                        RichText::new(t.buffer_label)
                            .small()
                            .color(StudioPalette::TEXT_SECONDARY),
                    );
                    egui::ComboBox::from_id_salt("buf_combo")
                        .width(74.0)
                        .selected_text(format!("{}", self.block_size))
                        .show_ui(ui, |ui| {
                            for bs in [64, 128, 256, 512, 1024] {
                                ui.selectable_value(&mut self.block_size, bs, format!("{bs}"));
                            }
                        });
                    if self.block_size != prev_bs {
                        self.restart_audio_if_running();
                    }

                    let est_ms = (self.block_size as f32 / self.sample_rate as f32) * 1000.0;
                    egui::Frame::new()
                        .fill(StudioPalette::BG_INPUT)
                        .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                        .corner_radius(CornerRadius::same(2))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("~{est_ms:.1} ms"))
                                    .small()
                                    .monospace()
                                    .color(if running {
                                        StudioPalette::ACCENT_EMERALD
                                    } else {
                                        StudioPalette::TEXT_SECONDARY
                                    }),
                            );
                        });
                });
            },
        );
    }

    fn render_input_channel_card(&mut self, ui: &mut egui::Ui, restart_needed: &mut bool) {
        let t = self.lang.text();
        let lang = self.lang;

        studio_card(
            ui,
            Some(StudioPalette::ACCENT_EMERALD),
            Margin::same(12),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    icon_heading(
                        ui,
                        StudioIcon::Mic,
                        StudioPalette::ACCENT_EMERALD,
                        t.input_device_heading,
                        14.0,
                    );
                    if ui.small_button(t.refresh).clicked() {
                        self.refresh_devices();
                    }
                });

                ui.add_space(4.0);
                let prev_in = self.selected_input.clone();
                let selected_input_label = self
                    .input_devices
                    .iter()
                    .find(|d| d.id == self.selected_input)
                    .map(|d| d.name.as_str())
                    .unwrap_or(t.none);
                let combo_w = (ui.available_width() - 8.0).max(80.0);
                egui::ComboBox::from_id_salt("input_device_combo")
                    .width(combo_w)
                    .selected_text(selected_input_label)
                    .show_ui(ui, |ui| {
                        for dev in &self.input_devices {
                            let label = lang.device_label(
                                &dev.name,
                                dev.default_channels,
                                dev.default_sample_rate,
                            );
                            ui.selectable_value(&mut self.selected_input, dev.id.clone(), label);
                        }
                    });
                if self.selected_input != prev_in {
                    *restart_needed = true;
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if draw_rotary_knob(
                        ui,
                        "knob_input_gain",
                        t.input_gain_db,
                        &mut self.input_gain_db,
                        -60.0..=24.0,
                        0.0,
                        "dB",
                        StudioPalette::ACCENT_EMERALD,
                    ) {
                        self.engine.shared.set_input_gain_db(self.input_gain_db);
                    }

                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        draw_studio_vu_meter(
                            ui,
                            t.meter_in,
                            self.meters.in_left,
                            self.meters.in_right,
                            self.peak_hold.in_left,
                            self.peak_hold.in_right,
                        );
                    });
                });
            },
        );
    }

    fn render_output_channel_card(&mut self, ui: &mut egui::Ui, restart_needed: &mut bool) {
        let t = self.lang.text();
        let lang = self.lang;

        studio_card(
            ui,
            Some(StudioPalette::ACCENT_CYAN),
            Margin::same(12),
            |ui| {
                ui.horizontal_wrapped(|ui| {
                    icon_heading(
                        ui,
                        StudioIcon::Headphones,
                        StudioPalette::ACCENT_CYAN,
                        t.output_device_heading,
                        14.0,
                    );
                    if ui.small_button(t.refresh).clicked() {
                        self.refresh_devices();
                    }
                });

                ui.add_space(4.0);
                let prev_out = self.selected_output.clone();
                let selected_output_label = self
                    .output_devices
                    .iter()
                    .find(|d| d.id == self.selected_output)
                    .map(|d| d.name.as_str())
                    .unwrap_or(t.none);
                let combo_w = (ui.available_width() - 8.0).max(80.0);
                egui::ComboBox::from_id_salt("output_device_combo")
                    .width(combo_w)
                    .selected_text(selected_output_label)
                    .show_ui(ui, |ui| {
                        for dev in &self.output_devices {
                            let label = lang.device_label(
                                &dev.name,
                                dev.default_channels,
                                dev.default_sample_rate,
                            );
                            ui.selectable_value(&mut self.selected_output, dev.id.clone(), label);
                        }
                    });
                if self.selected_output != prev_out {
                    *restart_needed = true;
                }

                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    if draw_rotary_knob(
                        ui,
                        "knob_output_gain",
                        t.output_gain_db,
                        &mut self.output_gain_db,
                        -60.0..=12.0,
                        0.0,
                        "dB",
                        StudioPalette::ACCENT_CYAN,
                    ) {
                        self.engine.shared.set_output_gain_db(self.output_gain_db);
                    }

                    ui.add_space(4.0);
                    ui.vertical(|ui| {
                        ui.add_space(10.0);
                        draw_studio_vu_meter(
                            ui,
                            t.meter_out,
                            self.meters.out_left,
                            self.meters.out_right,
                            self.peak_hold.out_left,
                            self.peak_hold.out_right,
                        );
                    });
                });
            },
        );
    }

    fn render_io_section(&mut self, ui: &mut egui::Ui) {
        let mut restart_needed = false;

        if ui.available_width() >= 680.0 {
            ui.columns(2, |cols| {
                self.render_input_channel_card(&mut cols[0], &mut restart_needed);
                self.render_output_channel_card(&mut cols[1], &mut restart_needed);
            });
        } else {
            self.render_input_channel_card(ui, &mut restart_needed);
            ui.add_space(8.0);
            self.render_output_channel_card(ui, &mut restart_needed);
        }

        if restart_needed {
            self.restart_audio_if_running();
        }
    }

    fn render_vst_section(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();

        studio_card(
            ui,
            Some(StudioPalette::ACCENT_PURPLE),
            Margin::same(14),
            |ui| {
                // Top Rack Header Bar
                ui.horizontal_wrapped(|ui| {
                    icon_heading(
                        ui,
                        StudioIcon::Sliders,
                        StudioPalette::ACCENT_PURPLE,
                        t.vst3_slot_heading,
                        15.0,
                    );

                    let scan_label = if self.is_scanning {
                        t.scanning
                    } else {
                        t.scan_installed_vst3
                    };
                    if animated_action_button(
                        ui,
                        "rack_scan_btn",
                        Some(StudioIcon::Refresh),
                        scan_label,
                        StudioPalette::ACCENT_PURPLE,
                        self.is_scanning,
                        Vec2::new(115.0, 28.0),
                    )
                    .clicked()
                    {
                        self.start_scan();
                    }

                    if animated_action_button(
                        ui,
                        "rack_open_vst_folder_btn",
                        Some(StudioIcon::Folder),
                        t.open_vst_folder_btn,
                        StudioPalette::ACCENT_EMERALD,
                        false,
                        Vec2::new(135.0, 28.0),
                    )
                    .clicked()
                    {
                        self.open_active_vst_folder();
                    }

                    if animated_action_button(
                        ui,
                        "rack_open_file_btn",
                        Some(StudioIcon::File),
                        t.open_file,
                        StudioPalette::ACCENT_BLUE,
                        false,
                        Vec2::new(165.0, 28.0),
                    )
                    .clicked()
                    {
                        if let Some(file) = rfd::FileDialog::new()
                            .set_title(t.dialog_select_plugin)
                            .set_directory(self.active_vst_dir())
                            .add_filter(t.dialog_vst3_filter, &["vst3", "so", "dll"])
                            .pick_file()
                        {
                            self.load_plugin_from_path(file);
                        }
                    }

                    if ui.button(t.open_bundle).clicked() {
                        if let Some(folder) = rfd::FileDialog::new()
                            .set_title(t.dialog_select_bundle)
                            .set_directory(self.active_vst_dir())
                            .pick_folder()
                        {
                            self.load_plugin_from_path(folder);
                        }
                    }

                    if ui
                        .selectable_label(self.show_path_loader, t.path_label)
                        .clicked()
                    {
                        self.show_path_loader = !self.show_path_loader;
                    }
                });

                // Quick Discovered Plugin Bar (responsive ComboBox width)
                if !self.discovered_plugins.is_empty() {
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        ui.label(
                            RichText::new(t.discovered)
                                .small()
                                .strong()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        let selected_label = self
                            .selected_discovered_idx
                            .and_then(|i| self.discovered_plugins.get(i))
                            .map(|p| format!("{} - {} [{}]", p.name, p.vendor, p.category))
                            .unwrap_or_else(|| t.select_plugin.into());
                        let combo_w = (ui.available_width() - 165.0).clamp(110.0, 340.0);
                        egui::ComboBox::from_id_salt("discovered_vst3_combo")
                            .width(combo_w)
                            .selected_text(selected_label)
                            .show_ui(ui, |ui| {
                                for (idx, p) in self.discovered_plugins.iter().enumerate() {
                                    let label =
                                        format!("{} - {} [{}]", p.name, p.vendor, p.category);
                                    ui.selectable_value(
                                        &mut self.selected_discovered_idx,
                                        Some(idx),
                                        label,
                                    );
                                }
                            });
                        if animated_action_button(
                            ui,
                            "rack_load_selected_btn",
                            Some(StudioIcon::Play),
                            t.load_selected,
                            StudioPalette::ACCENT_EMERALD,
                            false,
                            Vec2::new(135.0, 28.0),
                        )
                        .clicked()
                        {
                            if let Some(path) = self
                                .selected_discovered_idx
                                .and_then(|i| self.discovered_plugins.get(i))
                                .map(|p| p.path.clone())
                            {
                                self.load_plugin_from_path(path);
                            }
                        }
                    });
                }

                // Optional Manual Path Loader
                if self.show_path_loader {
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let resp = ui.add(
                            egui::TextEdit::singleline(&mut self.manual_plugin_path)
                                .desired_width((ui.available_width() - 220.0).max(120.0))
                                .hint_text(t.path_hint),
                        );
                        let enter_pressed =
                            resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if ui.button(t.browse).clicked() {
                            if let Some(chosen) = pick_vst3_dialog(self.lang, false) {
                                self.manual_plugin_path = chosen.display().to_string();
                                self.load_plugin_from_path(chosen);
                            }
                        }
                        if ui.button(t.folder).clicked() {
                            if let Some(chosen) = pick_vst3_dialog(self.lang, true) {
                                self.manual_plugin_path = chosen.display().to_string();
                                self.load_plugin_from_path(chosen);
                            }
                        }
                        if ui.button(t.load_path).clicked() || enter_pressed {
                            let trimmed = self.manual_plugin_path.trim();
                            if !trimmed.is_empty() {
                                self.load_plugin_from_path(PathBuf::from(trimmed));
                            }
                        }
                    });
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);

                if let Some(meta) = self.loaded_plugin.clone() {
                    // Loaded Plugin Hardware Rack Unit
                    egui::Frame::new()
                        .fill(StudioPalette::BG_INPUT)
                        .stroke(Stroke::new(
                            1.2_f32,
                            if self.bypass {
                                StudioPalette::ACCENT_AMBER
                            } else {
                                StudioPalette::ACCENT_PURPLE
                            },
                        ))
                        .corner_radius(CornerRadius::same(2))
                        .inner_margin(Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(
                                    RichText::new(&meta.info.name)
                                        .size(17.0)
                                        .strong()
                                        .color(Color32::WHITE),
                                );
                                if meta.is_wine_bridged {
                                    egui::Frame::new()
                                        .fill(Color32::from_rgba_unmultiplied(245, 158, 11, 40))
                                        .stroke(Stroke::new(1.0_f32, StudioPalette::ACCENT_AMBER))
                                        .corner_radius(CornerRadius::same(2))
                                        .inner_margin(Margin::symmetric(6, 2))
                                        .show(ui, |ui| {
                                            ui.label(
                                                RichText::new("WINE x64")
                                                    .small()
                                                    .strong()
                                                    .color(StudioPalette::ACCENT_AMBER),
                                            );
                                        });
                                }
                                ui.label(
                                    RichText::new(self.lang.plugin_meta_details(
                                        &meta.info.vendor,
                                        &meta.info.version,
                                        &meta.info.category,
                                        meta.input_channels,
                                        meta.output_channels,
                                    ))
                                    .small()
                                    .color(StudioPalette::TEXT_SECONDARY),
                                );
                            });

                            ui.add_space(6.0);
                            ui.horizontal_wrapped(|ui| {
                                if draw_rotary_knob(
                                    ui,
                                    "knob_dry_wet_mix",
                                    t.dry_wet_percent,
                                    &mut self.mix_percent,
                                    0.0..=100.0,
                                    100.0,
                                    "%",
                                    StudioPalette::ACCENT_PURPLE,
                                ) {
                                    self.engine.shared.set_mix(self.mix_percent / 100.0);
                                }

                                ui.vertical(|ui| {
                                    ui.add_space(6.0);
                                    ui.horizontal_wrapped(|ui| {
                                        if animated_led_toggle(
                                            ui,
                                            "plugin_bypass_toggle",
                                            &mut self.bypass,
                                            t.bypass_effect,
                                            StudioPalette::ACCENT_AMBER,
                                        ) {
                                            self.engine.shared.set_bypass(self.bypass);
                                        }

                                        if meta.info.has_gui {
                                            let win_open = self.engine.is_plugin_window_open();
                                            let gui_label = if win_open {
                                                t.close_native_gui
                                            } else {
                                                t.open_native_gui
                                            };
                                            if animated_action_button(
                                                ui,
                                                "plugin_gui_btn",
                                                Some(StudioIcon::Window),
                                                gui_label,
                                                StudioPalette::ACCENT_CYAN,
                                                win_open,
                                                Vec2::new(150.0, 28.0),
                                            )
                                            .clicked()
                                            {
                                                if win_open {
                                                    self.engine.close_plugin_window();
                                                } else if let Err(e) =
                                                    self.engine.open_plugin_window()
                                                {
                                                    self.error = Some(ErrorState::OpenGuiFailed(
                                                        e.to_string(),
                                                    ));
                                                }
                                            }
                                        }

                                        if ui.button(t.refresh_params).clicked() {
                                            self.engine.refresh_plugin_parameters();
                                            self.loaded_plugin =
                                                self.engine.loaded_plugin().cloned();
                                        }

                                        if animated_action_button(
                                            ui,
                                            "plugin_unload_btn",
                                            Some(StudioIcon::Close),
                                            t.unload,
                                            StudioPalette::ACCENT_CRIMSON,
                                            false,
                                            Vec2::new(100.0, 28.0),
                                        )
                                        .clicked()
                                        {
                                            self.engine.unload_vst3_plugin();
                                            self.loaded_plugin = None;
                                            self.status = StatusState::PluginUnloaded;
                                        }
                                    });
                                });
                            });
                        });

                    if self.loaded_plugin.is_none() {
                        return;
                    }

                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        icon_heading(
                            ui,
                            StudioIcon::Sliders,
                            StudioPalette::ACCENT_CYAN,
                            &format!("{} ({})", t.parameters, meta.parameters.len()),
                            14.0,
                        );
                        let filter_w = (ui.available_width() - 16.0).clamp(110.0, 220.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.param_filter)
                                .desired_width(filter_w)
                                .hint_text(t.filter_params_hint),
                        );
                    });

                    ui.add_space(6.0);
                    let filter_lower = self.param_filter.to_lowercase();
                    let mut changed_param: Option<(u32, f64)> = None;

                    let filtered_params: Vec<_> = meta
                        .parameters
                        .iter()
                        .filter(|e| {
                            filter_lower.is_empty()
                                || e.param.name.to_lowercase().contains(&filter_lower)
                        })
                        .collect();

                    let render_param_box = |ui: &mut egui::Ui,
                                            name: &str,
                                            formatted: &str,
                                            param_id: u32,
                                            param_val: f64,
                                            is_read_only: bool,
                                            changed: &mut Option<(u32, f64)>| {
                        egui::Frame::new()
                            .fill(StudioPalette::BG_INPUT)
                            .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                            .corner_radius(CornerRadius::same(2))
                            .inner_margin(Margin::symmetric(8, 5))
                            .show(ui, |ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.add(
                                        egui::Label::new(
                                            RichText::new(name)
                                                .small()
                                                .strong()
                                                .color(StudioPalette::TEXT_PRIMARY),
                                        )
                                        .truncate(),
                                    );
                                    ui.label(
                                        RichText::new(formatted)
                                            .small()
                                            .monospace()
                                            .color(StudioPalette::ACCENT_CYAN),
                                    );
                                });
                                let mut val = param_val;
                                let enabled = !is_read_only;
                                let slider =
                                    egui::Slider::new(&mut val, 0.0..=1.0).show_value(false);
                                if ui.add_enabled(enabled, slider).changed() {
                                    *changed = Some((param_id, val));
                                }
                            });
                        ui.add_space(4.0);
                    };

                    egui::ScrollArea::vertical()
                        .max_height((ui.available_height() - 10.0).max(140.0))
                        .show(ui, |ui| {
                            if ui.available_width() >= 620.0 {
                                ui.columns(2, |cols| {
                                    for (idx, entry) in filtered_params.iter().enumerate() {
                                        render_param_box(
                                            &mut cols[idx % 2],
                                            &entry.param.name,
                                            &entry.formatted,
                                            entry.param.id,
                                            entry.param.value,
                                            entry.param.is_read_only,
                                            &mut changed_param,
                                        );
                                    }
                                });
                            } else {
                                for entry in &filtered_params {
                                    render_param_box(
                                        ui,
                                        &entry.param.name,
                                        &entry.formatted,
                                        entry.param.id,
                                        entry.param.value,
                                        entry.param.is_read_only,
                                        &mut changed_param,
                                    );
                                }
                            }
                        });

                    if let Some((id, val)) = changed_param {
                        self.engine.set_plugin_parameter(id, val);
                        self.loaded_plugin = self.engine.loaded_plugin().cloned();
                    }
                } else {
                    // Empty Rack Dropzone + Quick Plugin Chips
                    egui::Frame::new()
                        .fill(StudioPalette::BG_INPUT)
                        .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_BRIGHT))
                        .corner_radius(CornerRadius::same(2))
                        .inner_margin(Margin::same(14))
                        .show(ui, |ui| {
                            ui.vertical_centered(|ui| {
                                let (r, _) =
                                    ui.allocate_exact_size(Vec2::splat(24.0), Sense::hover());
                                draw_vector_icon(
                                    ui.painter(),
                                    r.center(),
                                    20.0,
                                    StudioIcon::Sliders,
                                    StudioPalette::ACCENT_PURPLE,
                                );
                                ui.add_space(4.0);
                                ui.add(
                                    egui::Label::new(
                                        RichText::new(t.no_plugin_loaded)
                                            .color(StudioPalette::TEXT_SECONDARY),
                                    )
                                    .wrap(),
                                );
                            });

                            if !self.discovered_plugins.is_empty() {
                                ui.add_space(8.0);
                                ui.separator();
                                ui.add_space(4.0);
                                ui.label(
                                    RichText::new(format!(
                                        "{} ({})",
                                        t.settings_detected_heading,
                                        self.discovered_plugins.len()
                                    ))
                                    .small()
                                    .strong()
                                    .color(StudioPalette::ACCENT_CYAN),
                                );
                                ui.add_space(4.0);

                                let mut quick_load: Option<PathBuf> = None;
                                ui.horizontal_wrapped(|ui| {
                                    for (idx, p) in
                                        self.discovered_plugins.iter().take(8).enumerate()
                                    {
                                        if animated_action_button(
                                            ui,
                                            &format!("quick_chip_{idx}"),
                                            Some(StudioIcon::Play),
                                            &format!("{} [{}]", p.name, p.category),
                                            StudioPalette::BG_ELEVATED,
                                            false,
                                            Vec2::new(125.0, 26.0),
                                        )
                                        .clicked()
                                        {
                                            quick_load = Some(p.path.clone());
                                        }
                                    }
                                });
                                if let Some(path) = quick_load {
                                    self.load_plugin_from_path(path);
                                }
                            }
                        });
                }
            },
        );
    }
}

fn draw_signal_node(
    ui: &mut egui::Ui,
    icon: StudioIcon,
    title: &str,
    subtitle: &str,
    accent: Color32,
) {
    egui::Frame::new()
        .fill(StudioPalette::BG_INPUT)
        .stroke(Stroke::new(
            1.0_f32,
            lerp_color(StudioPalette::BORDER_SUBTLE, accent, 0.55),
        ))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(Margin::symmetric(8, 5))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                let (ic_rect, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                draw_vector_icon(ui.painter(), ic_rect.center(), 14.0, icon, accent);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 1.0;
                    ui.label(RichText::new(title).size(10.0).strong().color(accent));
                    let short = if subtitle.chars().count() > 22 {
                        let s: String = subtitle.chars().take(20).collect();
                        format!("{s}...")
                    } else {
                        subtitle.to_string()
                    };
                    ui.label(
                        RichText::new(short)
                            .small()
                            .strong()
                            .color(StudioPalette::TEXT_PRIMARY),
                    );
                });
            });
        });
}

impl eframe::App for ChronomixApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if !self.theme_initialized {
            apply_studio_theme(ctx);
            self.theme_initialized = true;
        }
        self.poll_scan();
        self.poll_wine_tasks();
        self.engine.service_plugin_window();
        self.mixer_engine.service_all_plugin_windows();
        let mixer_out = (!self.mixer_ui.selected_output.is_empty())
            .then_some(self.mixer_ui.selected_output.clone());
        if !self.mixer_engine.is_audio_running()
            && self.engine.is_audio_running()
            && self.mixer_engine.channels().iter().any(|c| c.media.loading)
        {
            self.engine.stop_audio();
        }
        for (ch_idx, res) in self.mixer_engine.poll_media_loads(mixer_out.as_deref()) {
            match res {
                Ok(title) => {
                    let ch_name = self
                        .mixer_engine
                        .channel(ch_idx)
                        .map(|c| c.name.clone())
                        .unwrap_or_else(|| format!("CH {:02}", ch_idx + 1));
                    self.status = StatusState::Plain(format!("{ch_name}: {title}"));
                    self.error = None;
                }
                Err(msg) => {
                    self.error = Some(ErrorState::Plain(msg));
                }
            }
        }
        self.update_meters();
        self.mixer_ui
            .update_telemetry(self.mixer_engine.take_telemetry());

        // Keep meters and native plugin GUI smooth at 60 FPS only when active.
        if self.engine.is_audio_running()
            || self.engine.is_plugin_window_open()
            || self.mixer_engine.is_audio_running()
            || self.mixer_engine.any_insert_window_open()
            || self.mixer_engine.is_recording()
            || self.mixer_engine.channels().iter().any(|c| c.media.loading)
            || self.is_scanning
            || self.wine_busy_msg.is_some()
            || self.wine_status_refreshing
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        if let Some(start) = self.splash_start {
            let elapsed = start.elapsed().as_secs_f32();
            let total_duration = 1.6_f32;
            let skip = ui.input(|i| {
                i.pointer.any_pressed()
                    || i.key_pressed(egui::Key::Escape)
                    || i.key_pressed(egui::Key::Space)
                    || i.key_pressed(egui::Key::Enter)
            });
            if elapsed < total_duration && !skip {
                ui.ctx().request_repaint();
                self.render_splash_screen(ui, elapsed, total_duration);
                return;
            } else {
                self.splash_start = None;
            }
        }

        let t = self.lang.text();
        let lang = self.lang;
        egui::Panel::bottom("status_bar")
            .frame(
                egui::Frame::new()
                    .fill(StudioPalette::BG_PANEL)
                    .inner_margin(Margin::symmetric(16, 6))
                    .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE)),
            )
            .show_inside(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    if let Some(ref err) = self.error {
                        let msg = err.format(lang);
                        let (dot_r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                        ui.painter().circle_filled(
                            dot_r.center(),
                            4.0,
                            StudioPalette::ACCENT_CRIMSON,
                        );
                        ui.add(
                            egui::Label::new(
                                RichText::new(format!("{} {msg}", t.error_prefix))
                                    .strong()
                                    .color(StudioPalette::ACCENT_CRIMSON),
                            )
                            .wrap(),
                        );
                        if ui.small_button(t.dismiss).clicked() {
                            self.error = None;
                        }
                    } else {
                        let msg = self.status.format(lang);
                        let (dot_r, _) = ui.allocate_exact_size(Vec2::splat(8.0), Sense::hover());
                        let dot_col = if self.engine.is_audio_running()
                            || self.mixer_engine.is_audio_running()
                        {
                            StudioPalette::ACCENT_EMERALD
                        } else {
                            StudioPalette::ACCENT_CYAN
                        };
                        ui.painter().circle_filled(dot_r.center(), 3.5, dot_col);
                        ui.add(
                            egui::Label::new(
                                RichText::new(msg)
                                    .small()
                                    .color(StudioPalette::TEXT_SECONDARY),
                            )
                            .wrap(),
                        );
                    }

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let active_stats = if self.active_panel == ActivePanel::Mixer
                            && self.mixer_engine.is_audio_running()
                        {
                            self.mixer_engine.audio_stats()
                        } else {
                            self.engine
                                .audio_stats()
                                .cloned()
                                .or_else(|| self.mixer_engine.audio_stats())
                        };
                        if let Some(stats) = active_stats {
                            let underruns = stats.output_underruns.load(Ordering::Relaxed);
                            let overruns = stats.input_overruns.load(Ordering::Relaxed);
                            ui.label(
                                RichText::new(lang.underruns_overruns(underruns, overruns))
                                    .small()
                                    .monospace()
                                    .color(StudioPalette::TEXT_MUTED),
                            );
                        }
                    });
                });
            });

        let central_margin = if self.active_panel == ActivePanel::Mixer {
            Margin::same(10)
        } else {
            Margin::same(16)
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(StudioPalette::BG_DEEP)
                    .inner_margin(central_margin),
            )
            .show_inside(ui, |ui| match self.active_panel {
                ActivePanel::News => self.render_news_panel(ui),
                ActivePanel::Recording => {
                    self.active_panel = ActivePanel::Mixer;
                }
                ActivePanel::Mixer => {
                    let mut mixer_err: Option<String> =
                        self.error.as_ref().map(|e| e.format(self.lang));
                    let had_err_msg = mixer_err.clone();
                    let mut wants_refresh_devices = false;
                    {
                        let app_ptr: *mut ChronomixApp = self as *mut _;
                        let mut render_settings_fn = |ui: &mut egui::Ui| {
                            unsafe {
                                (*app_ptr).render_settings_panel(ui);
                            }
                        };
                        let mut render_news_fn = |ui: &mut egui::Ui| {
                            unsafe {
                                (*app_ptr).render_news_panel(ui);
                            }
                        };
                        let mut change_lang_fn = |lang: Lang| {
                            unsafe {
                                (*app_ptr).lang = lang;
                                (*app_ptr).save_settings();
                            }
                        };
                        let engine = &mut self.engine;
                        let mut stop_classic = || engine.stop_audio();
                        let mut flag_refresh = || {
                            wants_refresh_devices = true;
                        };
                        render_mixer_panel(
                            ui,
                            MixerPanelContext {
                                lang: self.lang,
                                mixer: &mut self.mixer_engine,
                                state: &mut self.mixer_ui,
                                input_devices: &self.input_devices,
                                output_devices: &self.output_devices,
                                discovered_plugins: &self.discovered_plugins,
                                status: &mut self.status,
                                error: &mut mixer_err,
                                stop_classic_audio: &mut stop_classic,
                                refresh_devices: &mut flag_refresh,
                                render_settings: Some(&mut render_settings_fn),
                                render_news: Some(&mut render_news_fn),
                                change_lang: Some(&mut change_lang_fn),
                            },
                        );
                    }
                    if mixer_err != had_err_msg {
                        self.error = mixer_err.map(ErrorState::Plain);
                    }
                    if wants_refresh_devices {
                        self.refresh_devices();
                    }
                }
                ActivePanel::Settings => self.render_settings_panel(ui),
            });
    }
}

fn draw_status_pill(ui: &mut egui::Ui, text: &str, color: Color32) {
    egui::Frame::new()
        .fill(Color32::from_rgba_unmultiplied(
            color.r(),
            color.g(),
            color.b(),
            40,
        ))
        .stroke(Stroke::new(1.0_f32, color))
        .corner_radius(CornerRadius::same(2))
        .inner_margin(Margin::symmetric(8, 2))
        .show(ui, |ui| {
            ui.label(RichText::new(text).small().strong().color(color));
        });
}

fn open_folder_in_file_manager(path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "linux")]
    let mut cmd = {
        let mut c = std::process::Command::new("xdg-open");
        c.arg(path);
        c
    };
    #[cfg(target_os = "windows")]
    let mut cmd = {
        let mut c = std::process::Command::new("explorer");
        c.arg(path);
        c
    };
    #[cfg(target_os = "macos")]
    let mut cmd = {
        let mut c = std::process::Command::new("open");
        c.arg(path);
        c
    };
    cmd.spawn()
        .map(|_| ())
        .map_err(|e| format!("Failed to open folder '{}': {e}", path.display()))
}

fn pick_vst3_dialog(lang: Lang, directory: bool) -> Option<PathBuf> {
    let t = lang.text();
    let dialog = rfd::FileDialog::new();
    if directory {
        dialog.set_title(t.dialog_select_bundle).pick_folder()
    } else {
        dialog
            .set_title(t.dialog_select_plugin)
            .add_filter(t.dialog_vst3_filter, &["vst3", "dll", "so"])
            .pick_file()
    }
}
