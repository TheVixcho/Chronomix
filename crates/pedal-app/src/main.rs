mod i18n;
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
use pedal_audio::AudioDeviceInfo;
use pedal_engine::{LoadedPluginMeta, MeterSnapshot, PedalEngine};
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
    Settings,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
struct AppSettings {
    custom_vst_dir: Option<PathBuf>,
    include_system_vst: bool,
    lang: Option<String>,
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
enum StatusState {
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

struct ChronomixApp {
    theme_initialized: bool,
    lang: Lang,
    active_panel: ActivePanel,

    program_vst_dir: PathBuf,
    custom_vst_dir: Option<PathBuf>,
    include_system_vst: bool,

    engine: PedalEngine,
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

        let engine = PedalEngine::new();
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

        Self {
            theme_initialized: false,
            lang,
            active_panel: ActivePanel::News,
            program_vst_dir,
            custom_vst_dir,
            include_system_vst,
            engine,
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
        }
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
        };
        settings.save();
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
        thread::spawn(move || {
            let temp_engine = PedalEngine::new();
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

    /// Static, squared top navigation bar with vector logo, tabs, live status badge, and language selector.
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
                    Color32::from_rgb(20, 32, 54),
                );
                painter.rect_stroke(
                    logo_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, StudioPalette::ACCENT_CYAN),
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
                        StudioPalette::ACCENT_CYAN,
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
                            .color(StudioPalette::ACCENT_CYAN),
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
                        Color32::from_rgb(27, 56, 98),
                        sel_t,
                    );
                    painter.rect_filled(rect, CornerRadius::same(2), bg);

                    let border_col = lerp_color(
                        StudioPalette::BORDER_SUBTLE,
                        StudioPalette::ACCENT_CYAN,
                        sel_t.max(hov_t * 0.5),
                    );
                    painter.rect_stroke(
                        rect,
                        CornerRadius::same(2),
                        Stroke::new(1.0_f32, border_col),
                        StrokeKind::Inside,
                    );

                    // Bottom active indicator bar
                    if sel_t > 0.02 {
                        let bar_w = (rect.width() - 8.0) * sel_t;
                        let bar_rect = Rect::from_center_size(
                            Pos2::new(rect.center().x, rect.bottom() - 1.5),
                            Vec2::new(bar_w.max(4.0), 2.0),
                        );
                        painter.rect_filled(
                            bar_rect,
                            CornerRadius::same(1),
                            StudioPalette::ACCENT_CYAN,
                        );
                    }

                    let fg_col = lerp_color(
                        StudioPalette::TEXT_SECONDARY,
                        Color32::WHITE,
                        sel_t.max(hov_t * 0.7),
                    );
                    let icon_col = lerp_color(
                        StudioPalette::TEXT_SECONDARY,
                        StudioPalette::ACCENT_CYAN,
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
            if self.engine.shared.is_recording() {
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
            } else if self.engine.is_audio_running() {
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
                                .fill(Color32::from_rgba_unmultiplied(14, 165, 233, 35))
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
                                self.active_panel = ActivePanel::Recording;
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
                                self.active_panel = ActivePanel::Settings;
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
                    self.active_panel = p;
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
                                .fill(Color32::from_rgba_unmultiplied(59, 130, 246, 45))
                                .stroke(Stroke::new(1.0_f32, StudioPalette::ACCENT_BLUE))
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
                            egui::Frame::new()
                                .fill(Color32::from_rgba_unmultiplied(
                                    badge_color.r(),
                                    badge_color.g(),
                                    badge_color.b(),
                                    40,
                                ))
                                .stroke(Stroke::new(1.0_f32, badge_color))
                                .corner_radius(CornerRadius::same(2))
                                .inner_margin(Margin::symmetric(8, 2))
                                .show(ui, |ui| {
                                    ui.label(
                                        RichText::new(badge_text)
                                            .small()
                                            .strong()
                                            .color(badge_color),
                                    );
                                });
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
                        self.load_plugin_from_path(path);
                        self.active_panel = ActivePanel::Recording;
                    }
                }
            });
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
        self.engine.service_plugin_window();
        self.update_meters();

        // Keep meters and native plugin GUI smooth at 60 FPS only when active.
        if self.engine.is_audio_running()
            || self.engine.is_plugin_window_open()
            || self.is_scanning
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("main_toolbar")
            .frame(
                egui::Frame::new()
                    .fill(StudioPalette::BG_PANEL)
                    .inner_margin(Margin::symmetric(16, 8))
                    .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE)),
            )
            .show_inside(ui, |ui| {
                self.render_main_toolbar(ui);
            });

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
                        let dot_col = if self.engine.is_audio_running() {
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
                        if let Some(stats) = self.engine.audio_stats() {
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

        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(StudioPalette::BG_DEEP)
                    .inner_margin(Margin::same(16)),
            )
            .show_inside(ui, |ui| match self.active_panel {
                ActivePanel::News => self.render_news_panel(ui),
                ActivePanel::Recording => {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.render_signal_chain_banner(ui);
                        ui.add_space(8.0);
                        self.render_recording_transport(ui);
                        ui.add_space(8.0);
                        self.render_io_section(ui);
                        ui.add_space(10.0);
                        self.render_vst_section(ui);
                    });
                }
                ActivePanel::Settings => self.render_settings_panel(ui),
            });
    }
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
