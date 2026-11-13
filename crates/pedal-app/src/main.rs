mod i18n;

use std::{
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::Ordering,
    },
    thread,
    time::Duration,
};

use eframe::egui::{self, Color32, RichText, Stroke, Vec2};
use i18n::Lang;
use pedal_audio::AudioDeviceInfo;
use pedal_engine::{LoadedPluginMeta, MeterSnapshot, PedalEngine};
use vst3_host::PluginInfo;

fn main() -> eframe::Result<()> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Chronomix — Audio DAW & VST3 Host")
            .with_inner_size([980.0, 720.0])
            .with_min_inner_size([760.0, 540.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Chronomix",
        options,
        Box::new(|_cc| Ok(Box::new(ChronomixApp::new()))),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ActivePanel {
    News,
    Recording,
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
    lang: Lang,
    active_panel: ActivePanel,

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

    manual_plugin_path: String,
    discovered_plugins: Vec<PluginInfo>,
    selected_discovered_idx: Option<usize>,
    scan_in_progress: Arc<Mutex<Option<Vec<PluginInfo>>>>,
    is_scanning: bool,

    loaded_plugin: Option<LoadedPluginMeta>,
    param_filter: String,

    status: StatusState,
    error: Option<ErrorState>,
}

impl ChronomixApp {
    fn new() -> Self {
        let engine = PedalEngine::new();
        let input_devices = engine.list_input_devices().unwrap_or_default();
        let output_devices = engine.list_output_devices().unwrap_or_default();

        let selected_input = engine
            .default_input_device()
            .filter(|id| input_devices.iter().any(|d| &d.id == id))
            .or_else(|| input_devices.first().map(|d| d.id.clone()))
            .unwrap_or_default();
        let selected_output = engine
            .default_output_device()
            .filter(|id| output_devices.iter().any(|d| &d.id == id))
            .or_else(|| output_devices.first().map(|d| d.id.clone()))
            .unwrap_or_default();

        Self {
            lang: Lang::detect_default(),
            active_panel: ActivePanel::News,
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
            manual_plugin_path: String::new(),
            discovered_plugins: Vec::new(),
            selected_discovered_idx: None,
            scan_in_progress: Arc::new(Mutex::new(None)),
            is_scanning: false,
            loaded_plugin: None,
            param_filter: String::new(),
            status: StatusState::Ready,
            error: None,
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
                    self.selected_input = self
                        .engine
                        .default_input_device()
                        .filter(|id| self.input_devices.iter().any(|d| &d.id == id))
                        .or_else(|| self.input_devices.first().map(|d| d.id.clone()))
                        .unwrap_or_default();
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
                    self.selected_output = self
                        .engine
                        .default_output_device()
                        .filter(|id| self.output_devices.iter().any(|d| &d.id == id))
                        .or_else(|| self.output_devices.first().map(|d| d.id.clone()))
                        .unwrap_or_default();
                }
            }
            Err(e) => self.error = Some(ErrorState::Plain(e.to_string())),
        }
    }

    fn toggle_audio(&mut self) {
        if self.engine.is_audio_running() {
            self.engine.stop_audio();
            self.status = StatusState::MonitoringStopped;
            self.error = None;
            return;
        }

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

    fn start_scan(&mut self) {
        if self.is_scanning {
            return;
        }
        self.is_scanning = true;
        self.status = StatusState::Scanning;
        let slot = Arc::clone(&self.scan_in_progress);
        thread::spawn(move || {
            let temp_engine = PedalEngine::new();
            let found = temp_engine.discover_vst3_plugins();
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
        let decay = 0.82_f32;
        self.meters.in_left = snap.in_left.max(self.meters.in_left * decay);
        self.meters.in_right = snap.in_right.max(self.meters.in_right * decay);
        self.meters.out_left = snap.out_left.max(self.meters.out_left * decay);
        self.meters.out_right = snap.out_right.max(self.meters.out_right * decay);
    }

    /// Global top toolbar with brand, section navigation (`Novedades` / `Grabación`) and i18n switcher.
    fn render_main_toolbar(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        ui.horizontal_wrapped(|ui| {
            ui.heading(RichText::new("Chronomix").strong());
            ui.separator();

            for (panel, label) in [
                (ActivePanel::News, t.nav_news),
                (ActivePanel::Recording, t.nav_recording),
            ] {
                let selected = self.active_panel == panel;
                let btn = egui::Button::new(
                    RichText::new(label)
                        .strong()
                        .color(if selected { Color32::WHITE } else { Color32::LIGHT_GRAY }),
                )
                .fill(if selected {
                    Color32::from_rgb(56, 108, 184)
                } else {
                    Color32::from_rgb(44, 47, 54)
                });
                if ui.add(btn).clicked() {
                    self.active_panel = panel;
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                for lang in [Lang::En, Lang::Es] {
                    let selected = self.lang == lang;
                    let btn = egui::Button::new(
                        RichText::new(lang.code())
                            .small()
                            .strong()
                            .color(if selected { Color32::WHITE } else { Color32::LIGHT_GRAY }),
                    )
                    .fill(if selected {
                        Color32::from_rgb(56, 108, 184)
                    } else {
                        Color32::from_rgb(42, 44, 50)
                    });
                    if ui
                        .add(btn)
                        .on_hover_text(lang.display_name())
                        .clicked()
                    {
                        self.lang = lang;
                    }
                }
                ui.label(RichText::new(t.lang_label).small().weak());

                if self.engine.shared.is_recording() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("● {}", t.nav_rec_badge))
                            .small()
                            .strong()
                            .color(Color32::from_rgb(235, 80, 80)),
                    );
                } else if self.engine.is_audio_running() {
                    ui.separator();
                    ui.label(
                        RichText::new(format!("● {}", t.nav_live_badge))
                            .small()
                            .strong()
                            .color(Color32::LIGHT_GREEN),
                    );
                }
            });
        });
    }

    /// Startup / What's New (`Novedades`) panel shown when Chronomix opens.
    fn render_news_panel(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();

        egui::ScrollArea::vertical().show(ui, |ui| {
            ui.add_space(6.0);
            egui::Frame::group(ui.style())
                .inner_margin(egui::Margin::same(16))
                .show(ui, |ui| {
                    ui.heading(RichText::new("Chronomix").size(24.0).strong());
                    ui.add_space(2.0);
                    ui.label(RichText::new(t.news_hero_subtitle).size(15.0).color(Color32::LIGHT_BLUE));
                    ui.add_space(8.0);
                    ui.add(egui::Label::new(RichText::new(t.news_hero_description)).wrap());
                    ui.add_space(12.0);

                    let go_btn = egui::Button::new(
                        RichText::new(t.news_go_recording).strong().color(Color32::WHITE),
                    )
                    .fill(Color32::from_rgb(38, 138, 72))
                    .min_size(Vec2::new(160.0, 32.0));
                    if ui.add(go_btn).clicked() {
                        self.active_panel = ActivePanel::Recording;
                    }
                });

            ui.add_space(14.0);
            ui.heading(RichText::new(t.news_section_title).strong());
            ui.add_space(6.0);

            for item in t.news_items {
                egui::Frame::group(ui.style())
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            egui::Frame::new()
                                .fill(Color32::from_rgb(56, 108, 184))
                                .corner_radius(4.0)
                                .inner_margin(egui::Margin::symmetric(8, 2))
                                .show(ui, |ui| {
                                    ui.label(RichText::new(item.tag).small().strong().color(Color32::WHITE));
                                });
                            ui.label(RichText::new(item.title).strong());
                        });
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(RichText::new(item.summary).weak()).wrap());
                    });
                ui.add_space(8.0);
            }
        });
    }

    fn render_recording_transport(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                let running = self.engine.is_audio_running();
                let btn_text = if running {
                    t.stop_monitoring
                } else {
                    t.start_monitoring
                };
                let btn = egui::Button::new(RichText::new(btn_text).strong().color(Color32::WHITE))
                    .fill(if running {
                        Color32::from_rgb(175, 48, 48)
                    } else {
                        Color32::from_rgb(38, 138, 72)
                    });
                if ui.add(btn).clicked() {
                    self.toggle_audio();
                }

                ui.separator();
                ui.label(t.sample_rate_label);
                egui::ComboBox::from_id_salt("sr_combo")
                    .width(86.0)
                    .selected_text(format!("{} Hz", self.sample_rate))
                    .show_ui(ui, |ui| {
                        for sr in [44_100, 48_000, 96_000] {
                            ui.selectable_value(&mut self.sample_rate, sr, format!("{sr} Hz"));
                        }
                    });

                ui.label(t.buffer_label);
                egui::ComboBox::from_id_salt("buf_combo")
                    .width(72.0)
                    .selected_text(format!("{}", self.block_size))
                    .show_ui(ui, |ui| {
                        for bs in [64, 128, 256, 512, 1024] {
                            ui.selectable_value(&mut self.block_size, bs, format!("{bs}"));
                        }
                    });

                if running {
                    let est_ms = (self.block_size as f32 / self.sample_rate as f32) * 1000.0;
                    ui.label(
                        RichText::new(format!("~{est_ms:.1} ms"))
                            .small()
                            .color(Color32::LIGHT_GREEN),
                    );
                }

                ui.separator();
                if ui
                    .selectable_label(self.monitor_muted, t.mute_output)
                    .clicked()
                {
                    self.monitor_muted = !self.monitor_muted;
                    self.engine.shared.set_monitor_muted(self.monitor_muted);
                }

                let recording = self.engine.shared.is_recording();
                let rec_label = if recording {
                    let secs = self.engine.shared.recorded_frames() as f64
                        / self.engine.sample_rate().max(1) as f64;
                    self.lang.stop_rec_label(secs)
                } else {
                    t.record_wav.into()
                };
                let rec_btn = egui::Button::new(
                    RichText::new(rec_label).color(if recording {
                        Color32::WHITE
                    } else {
                        Color32::LIGHT_GRAY
                    }),
                )
                .fill(if recording {
                    Color32::from_rgb(200, 40, 40)
                } else {
                    Color32::from_rgb(55, 58, 66)
                });
                if ui.add(rec_btn).clicked() {
                    self.toggle_recording();
                }
            });
        });
    }

    fn render_io_section(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        let lang = self.lang;
        ui.columns(2, |cols| {
            cols[0].group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(t.input_device_heading).strong());
                    if ui.small_button(t.refresh).clicked() {
                        self.refresh_devices();
                    }
                });

                let selected_input_label = self
                    .input_devices
                    .iter()
                    .find(|d| d.id == self.selected_input)
                    .map(|d| d.name.as_str())
                    .unwrap_or(t.none);
                egui::ComboBox::from_id_salt("input_device_combo")
                    .width(ui.available_width())
                    .selected_text(selected_input_label)
                    .show_ui(ui, |ui| {
                        for dev in &self.input_devices {
                            let label =
                                lang.device_label(&dev.name, dev.default_channels, dev.default_sample_rate);
                            ui.selectable_value(&mut self.selected_input, dev.id.clone(), label);
                        }
                    });

                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Slider::new(&mut self.input_gain_db, -60.0..=24.0)
                            .text(t.input_gain_db),
                    )
                    .changed()
                {
                    self.engine.shared.set_input_gain_db(self.input_gain_db);
                }
                draw_stereo_meter(ui, t.meter_in, self.meters.in_left, self.meters.in_right);
            });

            cols[1].group(|ui| {
                ui.label(RichText::new(t.output_device_heading).strong());
                let selected_output_label = self
                    .output_devices
                    .iter()
                    .find(|d| d.id == self.selected_output)
                    .map(|d| d.name.as_str())
                    .unwrap_or(t.none);
                egui::ComboBox::from_id_salt("output_device_combo")
                    .width(ui.available_width())
                    .selected_text(selected_output_label)
                    .show_ui(ui, |ui| {
                        for dev in &self.output_devices {
                            let label =
                                lang.device_label(&dev.name, dev.default_channels, dev.default_sample_rate);
                            ui.selectable_value(&mut self.selected_output, dev.id.clone(), label);
                        }
                    });

                ui.add_space(4.0);
                if ui
                    .add(
                        egui::Slider::new(&mut self.output_gain_db, -60.0..=12.0)
                            .text(t.output_gain_db),
                    )
                    .changed()
                {
                    self.engine.shared.set_output_gain_db(self.output_gain_db);
                }
                draw_stereo_meter(ui, t.meter_out, self.meters.out_left, self.meters.out_right);
            });
        });
    }

    fn render_vst_section(&mut self, ui: &mut egui::Ui) {
        let t = self.lang.text();
        ui.group(|ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(RichText::new(t.vst3_slot_heading).strong());

                let scan_btn = ui.add_enabled(
                    !self.is_scanning,
                    egui::Button::new(if self.is_scanning {
                        t.scanning
                    } else {
                        t.scan_installed_vst3
                    }),
                );
                if scan_btn.clicked() {
                    self.start_scan();
                }

                if ui.button(t.open_bundle).clicked() {
                    if let Some(folder) = rfd::FileDialog::new()
                        .set_title(t.dialog_select_bundle)
                        .pick_folder()
                    {
                        self.load_plugin_from_path(folder);
                    }
                }

                if ui.button(t.open_file).clicked() {
                    if let Some(file) = rfd::FileDialog::new()
                        .set_title(t.dialog_select_plugin)
                        .add_filter(t.dialog_vst3_filter, &["vst3", "so", "dll"])
                        .pick_file()
                    {
                        self.load_plugin_from_path(file);
                    }
                }
            });

            if !self.discovered_plugins.is_empty() {
                ui.horizontal(|ui| {
                    ui.label(t.discovered);
                    let selected_label = self
                        .selected_discovered_idx
                        .and_then(|i| self.discovered_plugins.get(i))
                        .map(|p| format!("{} ({})", p.name, p.vendor))
                        .unwrap_or_else(|| t.select_plugin.into());
                    egui::ComboBox::from_id_salt("discovered_vst3_combo")
                        .width(340.0)
                        .selected_text(selected_label)
                        .show_ui(ui, |ui| {
                            for (idx, p) in self.discovered_plugins.iter().enumerate() {
                                let label = format!("{} — {} [{}]", p.name, p.vendor, p.category);
                                ui.selectable_value(&mut self.selected_discovered_idx, Some(idx), label);
                            }
                        });
                    if ui.button(t.load_selected).clicked() {
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

            ui.horizontal(|ui| {
                ui.label(t.path_label);
                let resp = ui.add(
                    egui::TextEdit::singleline(&mut self.manual_plugin_path)
                        .desired_width((ui.available_width() - 230.0).max(120.0))
                        .hint_text(t.path_hint),
                );
                let enter_pressed = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
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

            ui.separator();

            if let Some(meta) = self.loaded_plugin.clone() {
                ui.horizontal_wrapped(|ui| {
                    ui.label(RichText::new(&meta.info.name).heading().strong());
                    if meta.is_wine_bridged {
                        ui.label(
                            RichText::new("[Wine]")
                                .small()
                                .strong()
                                .color(Color32::from_rgb(245, 180, 70)),
                        );
                    }
                    ui.label(
                        RichText::new(self.lang.plugin_meta_details(
                            &meta.info.vendor,
                            &meta.info.version,
                            &meta.info.category,
                            meta.input_channels,
                            meta.output_channels,
                        ))
                        .weak(),
                    );
                });

                ui.horizontal_wrapped(|ui| {
                    if ui.selectable_label(self.bypass, t.bypass_effect).clicked() {
                        self.bypass = !self.bypass;
                        self.engine.shared.set_bypass(self.bypass);
                    }

                    if ui
                        .add(
                            egui::Slider::new(&mut self.mix_percent, 0.0..=100.0)
                                .text(t.dry_wet_percent),
                        )
                        .changed()
                    {
                        self.engine.shared.set_mix(self.mix_percent / 100.0);
                    }

                    if meta.info.has_gui {
                        let win_open = self.engine.is_plugin_window_open();
                        let gui_label = if win_open {
                            t.close_native_gui
                        } else {
                            t.open_native_gui
                        };
                        if ui.button(gui_label).clicked() {
                            if win_open {
                                self.engine.close_plugin_window();
                            } else if let Err(e) = self.engine.open_plugin_window() {
                                self.error = Some(ErrorState::OpenGuiFailed(e.to_string()));
                            }
                        }
                    }

                    if ui.button(t.refresh_params).clicked() {
                        self.engine.refresh_plugin_parameters();
                        self.loaded_plugin = self.engine.loaded_plugin().cloned();
                    }

                    if ui.button(t.unload).clicked() {
                        self.engine.unload_vst3_plugin();
                        self.loaded_plugin = None;
                        self.status = StatusState::PluginUnloaded;
                        return;
                    }
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(t.parameters);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.param_filter)
                            .hint_text(t.filter_params_hint),
                    );
                });

                let filter_lower = self.param_filter.to_lowercase();
                let mut changed_param: Option<(u32, f64)> = None;

                egui::ScrollArea::vertical()
                    .max_height(ui.available_height() - 10.0)
                    .show(ui, |ui| {
                        for entry in &meta.parameters {
                            if !filter_lower.is_empty()
                                && !entry.param.name.to_lowercase().contains(&filter_lower)
                            {
                                continue;
                            }
                            ui.horizontal(|ui| {
                                ui.add_sized(
                                    [200.0, 18.0],
                                    egui::Label::new(&entry.param.name).truncate(),
                                );
                                let mut val = entry.param.value;
                                let enabled = !entry.param.is_read_only;
                                let slider = egui::Slider::new(&mut val, 0.0..=1.0)
                                    .show_value(false)
                                    .fixed_decimals(3);
                                if ui.add_enabled(enabled, slider).changed() {
                                    changed_param = Some((entry.param.id, val));
                                }
                                ui.add_sized(
                                    [120.0, 18.0],
                                    egui::Label::new(
                                        RichText::new(&entry.formatted).monospace().small(),
                                    )
                                    .truncate(),
                                );
                            });
                        }
                    });

                if let Some((id, val)) = changed_param {
                    self.engine.set_plugin_parameter(id, val);
                    self.loaded_plugin = self.engine.loaded_plugin().cloned();
                }
            } else {
                ui.add(egui::Label::new(RichText::new(t.no_plugin_loaded).weak()).wrap());
            }
        });
    }
}

impl eframe::App for ChronomixApp {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan();
        self.engine.service_plugin_window();
        self.update_meters();

        // Keep meters and native plugin GUI responsive while monitoring or editing.
        if self.engine.is_audio_running() || self.engine.is_plugin_window_open() || self.is_scanning
        {
            ctx.request_repaint_after(Duration::from_millis(16));
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::top("main_toolbar").show_inside(ui, |ui| {
            ui.add_space(4.0);
            self.render_main_toolbar(ui);
            ui.add_space(4.0);
        });

        let t = self.lang.text();
        let lang = self.lang;
        egui::Panel::bottom("status_bar").show_inside(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if let Some(ref err) = self.error {
                    let msg = err.format(lang);
                    ui.add(
                        egui::Label::new(
                            RichText::new(format!("{} {msg}", t.error_prefix))
                                .color(Color32::from_rgb(245, 95, 95)),
                        )
                        .wrap(),
                    );
                    if ui.small_button(t.dismiss).clicked() {
                        self.error = None;
                    }
                } else {
                    let msg = self.status.format(lang);
                    ui.add(egui::Label::new(RichText::new(msg).small()).wrap());
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(stats) = self.engine.audio_stats() {
                        let underruns = stats.output_underruns.load(Ordering::Relaxed);
                        let overruns = stats.input_overruns.load(Ordering::Relaxed);
                        ui.label(
                            RichText::new(lang.underruns_overruns(underruns, overruns))
                                .small()
                                .weak(),
                        );
                    }
                });
            });
        });

        egui::CentralPanel::default().show_inside(ui, |ui| match self.active_panel {
            ActivePanel::News => self.render_news_panel(ui),
            ActivePanel::Recording => {
                self.render_recording_transport(ui);
                ui.add_space(6.0);
                self.render_io_section(ui);
                ui.add_space(8.0);
                self.render_vst_section(ui);
            }
        });
    }
}

fn draw_stereo_meter(ui: &mut egui::Ui, label: &str, left: f32, right: f32) {
    ui.add_space(4.0);
    for (ch_name, level) in [("L", left), ("R", right)] {
        ui.horizontal(|ui| {
            ui.add_sized(
                [42.0, 12.0],
                egui::Label::new(RichText::new(format!("{label} {ch_name}")).small().monospace()),
            );
            let desired = Vec2::new(ui.available_width().max(60.0), 10.0);
            let (rect, _) = ui.allocate_exact_size(desired, egui::Sense::hover());
            let painter = ui.painter();
            painter.rect_filled(rect, 2.0, Color32::from_rgb(28, 30, 35));

            let norm = amp_to_meter(level);
            if norm > 0.0 {
                let mut fill = rect;
                fill.set_width(rect.width() * norm);
                let color = if level >= 0.98 {
                    Color32::from_rgb(235, 65, 65)
                } else if level >= 0.7 {
                    Color32::from_rgb(235, 195, 55)
                } else {
                    Color32::from_rgb(65, 200, 105)
                };
                painter.rect_filled(fill, 2.0, color);
            }
            painter.rect_stroke(
                rect,
                2.0,
                Stroke::new(1.0_f32, Color32::from_rgb(55, 58, 66)),
                egui::StrokeKind::Inside,
            );
        });
    }
}

fn amp_to_meter(amp: f32) -> f32 {
    if amp <= 1e-4 {
        return 0.0;
    }
    let db = 20.0 * amp.log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn pick_vst3_dialog(lang: Lang, directory: bool) -> Option<PathBuf> {
    let t = lang.text();
    let dialog = rfd::FileDialog::new();
    if directory {
        dialog.set_title(t.dialog_select_bundle).pick_folder()
    } else {
        dialog
            .set_title(t.dialog_select_plugin)
            .add_filter(t.dialog_vst3_filter, &["vst3"])
            .pick_file()
    }
}
