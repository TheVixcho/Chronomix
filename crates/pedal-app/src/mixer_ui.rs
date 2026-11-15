use std::{collections::HashMap, path::{Path, PathBuf}, process::Command};

use eframe::egui::{
    self, Align2, Color32, ColorImage, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense,
    Shape, Stroke, StrokeKind, TextureHandle, TextureOptions, Vec2,
};
use pedal_audio::AudioDeviceInfo;
use pedal_engine::{
    ChannelDspConfig, ChannelIcon, ChannelInputMode, ChannelInputSource, MAX_MIXER_CHANNELS,
    MAX_MIXER_INSERTS, MixerChannelState, MixerEngine, MixerInsertTarget, MixerMeterSnapshot,
    VIRTUAL_MIC_DISPLAY_NAME, eval_channel_eq_db,
};
use vst3_host::PluginInfo;

use crate::{
    StatusState,
    i18n::{Lang, Strings},
    theme::{StudioIcon, StudioPalette, animated_action_button, draw_rotary_knob, lerp_color},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingSelection {
    Channel(usize),
    Master,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WingScreenTab {
    Home,
    Source,
    Eq,
    Dyn,
    Vst,
    IoRec,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ChannelBankSection {
    #[default]
    Ch1_16,
    Ch17_32,
    Ch33_48,
    Multimedia,
    All,
}

impl ChannelBankSection {
    pub fn matches(&self, is_multimedia: bool, ch_index: usize) -> bool {
        match self {
            Self::All => true,
            Self::Multimedia => is_multimedia,
            Self::Ch1_16 => !is_multimedia && ch_index < 16,
            Self::Ch17_32 => !is_multimedia && (16..32).contains(&ch_index),
            Self::Ch33_48 => !is_multimedia && (32..48).contains(&ch_index),
        }
    }

    pub fn label(&self, s: &Strings) -> &'static str {
        match self {
            Self::Ch1_16 => s.mixer_bank_ch1_16,
            Self::Ch17_32 => s.mixer_bank_ch17_32,
            Self::Ch33_48 => s.mixer_bank_ch33_48,
            Self::Multimedia => s.mixer_bank_multimedia,
            Self::All => s.mixer_bank_all,
        }
    }

    pub fn for_channel(is_multimedia: bool, ch_index: usize) -> Self {
        if is_multimedia {
            Self::Multimedia
        } else if ch_index < 16 {
            Self::Ch1_16
        } else if ch_index < 32 {
            Self::Ch17_32
        } else {
            Self::Ch33_48
        }
    }
}

pub struct MixerUiState {
    pub selection: WingSelection,
    pub active_tab: WingScreenTab,
    pub active_bank: ChannelBankSection,
    pub focused_vst_slot: usize,
    pub vst_picker_filter: String,
    pub param_filter: String,
    pub meters: MixerMeterSnapshot,
    pub peak_hold_master: [f32; 2],
    pub peak_hold_vmic: [f32; 2],
    pub peak_hold_channels: Vec<[f32; 2]>,
    pub selected_output: String,
    pub sample_rate: u32,
    pub block_size: usize,
    pub recording_base_dir: PathBuf,
    pub scroll_to_channel: Option<usize>,
    pub pending_fader_scroll_x: Option<f32>,
    pub last_fader_scroll_x: f32,
    pub last_fader_max_scroll_x: f32,
    pub custom_icon_textures: HashMap<PathBuf, Option<TextureHandle>>,
}

impl MixerUiState {
    pub fn new(default_output: String, default_channels: usize) -> Self {
        let base_dir = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        Self {
            selection: WingSelection::Channel(0),
            active_tab: WingScreenTab::Home,
            active_bank: ChannelBankSection::Ch1_16,
            focused_vst_slot: 0,
            vst_picker_filter: String::new(),
            param_filter: String::new(),
            meters: MixerMeterSnapshot {
                channels: Vec::new(),
                master_left: 0.0,
                master_right: 0.0,
                virtual_mic_left: 0.0,
                virtual_mic_right: 0.0,
                is_virtual_mic_active: false,
                recorded_frames: 0,
                is_recording: false,
            },
            peak_hold_master: [0.0, 0.0],
            peak_hold_vmic: [0.0, 0.0],
            peak_hold_channels: vec![[0.0, 0.0]; default_channels],
            selected_output: default_output,
            sample_rate: 48_000,
            block_size: 256,
            recording_base_dir: base_dir,
            scroll_to_channel: None,
            pending_fader_scroll_x: None,
            last_fader_scroll_x: 0.0,
            last_fader_max_scroll_x: 0.0,
            custom_icon_textures: HashMap::new(),
        }
    }

    pub fn update_telemetry(&mut self, snap: MixerMeterSnapshot) {
        if self.peak_hold_channels.len() != snap.channels.len() {
            self.peak_hold_channels.resize(snap.channels.len(), [0.0, 0.0]);
        }
        for (idx, m) in snap.channels.iter().enumerate() {
            self.peak_hold_channels[idx][0] =
                (self.peak_hold_channels[idx][0] * 0.94).max(m.out_left);
            self.peak_hold_channels[idx][1] =
                (self.peak_hold_channels[idx][1] * 0.94).max(m.out_right);
        }
        self.peak_hold_master[0] = (self.peak_hold_master[0] * 0.94).max(snap.master_left);
        self.peak_hold_master[1] = (self.peak_hold_master[1] * 0.94).max(snap.master_right);
        self.peak_hold_vmic[0] = (self.peak_hold_vmic[0] * 0.94).max(snap.virtual_mic_left);
        self.peak_hold_vmic[1] = (self.peak_hold_vmic[1] * 0.94).max(snap.virtual_mic_right);
        self.meters = snap;
    }
}

struct WingColors;

impl WingColors {
    const CHASSIS: Color32 = Color32::from_rgb(12, 12, 15);
    const DECK: Color32 = Color32::from_rgb(18, 18, 22);
    const BEZEL: Color32 = Color32::from_rgb(12, 12, 14);
    const SCREEN: Color32 = Color32::from_rgb(13, 13, 16);
    const FADER_WELL: Color32 = Color32::from_rgb(10, 10, 12);
    const STRIP_BG: Color32 = Color32::from_rgb(19, 19, 23);
    const STRIP_SELECTED: Color32 = Color32::from_rgb(30, 30, 36);
    const CAP_TOP: Color32 = Color32::from_rgb(230, 232, 238);
    const CAP_MID: Color32 = Color32::from_rgb(160, 164, 174);
    const CAP_DARK: Color32 = Color32::from_rgb(60, 62, 70);
    const WING_ORANGE: Color32 = Color32::from_rgb(240, 242, 248);
    const WALNUT_BASE: Color32 = Color32::from_rgb(22, 22, 26);
    const WALNUT_LIGHT: Color32 = Color32::from_rgb(38, 38, 44);
    const WALNUT_DARK: Color32 = Color32::from_rgb(14, 14, 17);
}

const WING_PALETTE_SWATCHES: [[u8; 3]; 16] = [
    [20, 22, 28],
    [54, 58, 70],
    [150, 156, 172],
    [230, 235, 245],
    [235, 48, 56],
    [245, 115, 30],
    [245, 175, 35],
    [245, 215, 45],
    [140, 215, 45],
    [36, 195, 105],
    [32, 205, 175],
    [28, 195, 240],
    [48, 145, 245],
    [60, 95, 235],
    [155, 75, 240],
    [235, 55, 160],
];

pub struct MixerPanelContext<'a> {
    pub lang: Lang,
    pub mixer: &'a mut MixerEngine,
    pub state: &'a mut MixerUiState,
    pub input_devices: &'a [AudioDeviceInfo],
    pub output_devices: &'a [AudioDeviceInfo],
    pub discovered_plugins: &'a [PluginInfo],
    pub status: &'a mut StatusState,
    pub error: &'a mut Option<String>,
    pub stop_classic_audio: &'a mut dyn FnMut(),
    pub refresh_devices: &'a mut dyn FnMut(),
}

pub fn render_mixer_panel(ui: &mut egui::Ui, ctx: MixerPanelContext<'_>) {
    let channel_count = ctx.mixer.channels().len();
    if let WingSelection::Channel(ch) = ctx.state.selection {
        if ch >= channel_count && channel_count > 0 {
            ctx.state.selection = WingSelection::Channel(channel_count - 1);
        }
    }

    let mut ctx = ctx;
    let viewport_w = ui.available_width().max(260.0);
    ui.set_max_width(viewport_w);
    egui::ScrollArea::vertical()
        .id_salt("mixer_panel_vertical_scroll")
        .auto_shrink([false, false])
        .max_width(viewport_w)
        .show(ui, |ui| {
            let scroll_inner_w = ui.available_width().max(240.0);
            ui.set_max_width(scroll_inner_w);
            egui::Frame::new()
                .fill(WingColors::CHASSIS)
                .stroke(Stroke::new(1.2_f32, Color32::from_rgb(44, 49, 62)))
                .corner_radius(CornerRadius::same(3))
                .inner_margin(Margin::same(8))
                .show(ui, |ui| {
                    let chassis_w = ui.available_width().max(220.0);
                    ui.set_max_width(chassis_w);
                    render_upper_console_deck(ui, &mut ctx);
                    ui.add_space(8.0);
                    render_fader_bank(ui, &mut ctx);
                    ui.add_space(6.0);
                    draw_wing_wood_bolster(ui);
                });
        });
}

fn render_upper_console_deck(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let avail_w = ui.available_width();
    if avail_w >= 860.0 {
        let gap = ui.spacing().item_spacing.x;
        let monitor_w = (avail_w * 0.24).clamp(206.0, 246.0).floor();
        let screen_w = (avail_w - monitor_w - gap - 2.0).max(320.0).floor();
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(screen_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_width(screen_w);
                    render_wing_touchscreen(ui, ctx);
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new(monitor_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_width(monitor_w);
                    render_wing_monitor_deck(ui, ctx);
                },
            );
        });
    } else {
        render_wing_touchscreen(ui, ctx);
        ui.add_space(8.0);
        render_wing_monitor_deck(ui, ctx);
    }
}

fn render_wing_touchscreen(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let outer_w = ui.available_width().max(220.0);
    ui.set_max_width(outer_w);
    let narrow = outer_w < 660.0;

    egui::Frame::new()
        .fill(WingColors::BEZEL)
        .stroke(Stroke::new(1.5_f32, Color32::from_rgb(48, 54, 68)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            let bezel_w = ui.available_width().max(200.0);
            ui.set_max_width(bezel_w);
            let tabs = [
                (WingScreenTab::Home, t.mixer_tab_home),
                (WingScreenTab::Source, t.mixer_tab_source),
                (WingScreenTab::Eq, t.mixer_tab_eq),
                (WingScreenTab::Dyn, t.mixer_tab_dyn),
                (WingScreenTab::Vst, t.mixer_tab_vst),
                (WingScreenTab::IoRec, t.mixer_tab_io),
            ];

            if narrow {
                ui.horizontal_wrapped(|ui| {
                    for (tab, label) in tabs {
                        draw_wing_bezel_tab(ui, &mut ctx.state.active_tab, tab, label, 80.0);
                    }
                });
                ui.add_space(6.0);
                render_touchscreen_display(ui, ctx);
            } else {
                let tab_w = 84.0;
                let gap = ui.spacing().item_spacing.x + 4.0;
                let disp_w = (bezel_w - tab_w - gap).max(200.0).floor();
                ui.horizontal_top(|ui| {
                    ui.allocate_ui_with_layout(
                        Vec2::new(tab_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.spacing_mut().item_spacing.y = 5.0;
                            for (tab, label) in tabs {
                                draw_wing_bezel_tab(ui, &mut ctx.state.active_tab, tab, label, tab_w);
                            }
                        },
                    );
                    ui.add_space(4.0);
                    ui.allocate_ui_with_layout(
                        Vec2::new(disp_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(disp_w);
                            render_touchscreen_display(ui, ctx);
                        },
                    );
                });
            }
        });
}

fn draw_wing_bezel_tab(
    ui: &mut egui::Ui,
    active_tab: &mut WingScreenTab,
    tab: WingScreenTab,
    label: &str,
    width: f32,
) {
    let active = *active_tab == tab;
    let id = ui.make_persistent_id(("wing_bezel_tab", label));
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 28.0), Sense::click());
    if resp.clicked() {
        *active_tab = tab;
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let sel_t = ui.ctx().animate_bool_responsive(id.with("sel"), active);
    let hov_t = ui
        .ctx()
        .animate_bool_responsive(id.with("hov"), resp.hovered());

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let pressed_offset = if resp.is_pointer_button_down_on() { 0.7 } else { 0.0 };
        let socket_rect = rect.expand(0.8);
        p.rect_filled(socket_rect, CornerRadius::same(3), Color32::from_rgb(7, 7, 9));
        p.rect_stroke(
            socket_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, Color32::from_rgb(18, 19, 24)),
            StrokeKind::Inside,
        );

        let btn_rect = rect.translate(Vec2::new(0.0, pressed_offset));
        let off_body = Color32::from_rgb(22, 23, 28);
        let hov_body = Color32::from_rgb(32, 34, 42);
        let on_body = Color32::from_rgb(46, 48, 58);
        let base_body = lerp_color(lerp_color(off_body, hov_body, hov_t), on_body, sel_t);
        p.rect_filled(btn_rect, CornerRadius::same(3), base_body);

        if sel_t > 0.02 {
            let core_rect = Rect::from_center_size(
                btn_rect.center(),
                Vec2::new(btn_rect.width() * 0.7, btn_rect.height() * 0.55),
            );
            let alpha = (90.0 * sel_t).clamp(0.0, 110.0) as u8;
            p.rect_filled(
                core_rect,
                CornerRadius::same(2),
                Color32::from_rgba_unmultiplied(255, 255, 255, alpha),
            );

            let led = Rect::from_min_size(
                Pos2::new(btn_rect.left() + 3.0, btn_rect.top() + 4.0),
                Vec2::new(3.0, btn_rect.height() - 8.0),
            );
            p.rect_filled(led, CornerRadius::same(1), Color32::WHITE);
        }

        let rim_alpha = if active { 110 } else { 32 };
        p.line_segment(
            [
                Pos2::new(btn_rect.left() + 2.0, btn_rect.top() + 1.0),
                Pos2::new(btn_rect.right() - 2.0, btn_rect.top() + 1.0),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, rim_alpha)),
        );
        p.line_segment(
            [
                Pos2::new(btn_rect.left() + 2.0, btn_rect.bottom() - 1.0),
                Pos2::new(btn_rect.right() - 2.0, btn_rect.bottom() - 1.0),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 85)),
        );

        let stroke_col = lerp_color(
            Color32::from_rgb(42, 45, 54),
            Color32::from_rgb(180, 185, 195),
            sel_t.max(hov_t * 0.5),
        );
        p.rect_stroke(
            btn_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, stroke_col),
            StrokeKind::Inside,
        );

        let txt_col = lerp_color(StudioPalette::TEXT_SECONDARY, Color32::WHITE, sel_t.max(hov_t * 0.6));
        p.text(
            btn_rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(10.0),
            txt_col,
        );
    }
}

fn render_touchscreen_display(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let disp_w = ui.available_width().max(180.0);
    ui.set_max_width(disp_w);

    egui::Frame::new()
        .fill(WingColors::SCREEN)
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(50, 62, 84)))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            let inner_w = ui.available_width().max(160.0);
            ui.set_max_width(inner_w);
            render_screen_header(ui, ctx);
            ui.add_space(5.0);
            render_screen_signal_blocks(ui, ctx, t);
            ui.add_space(6.0);

            match ctx.state.active_tab {
                WingScreenTab::Home => render_screen_home_tab(ui, ctx),
                WingScreenTab::Source => render_screen_source_tab(ui, ctx),
                WingScreenTab::Eq => render_screen_eq_tab(ui, ctx),
                WingScreenTab::Dyn => render_screen_dyn_tab(ui, ctx),
                WingScreenTab::Vst => render_screen_vst_tab(ui, ctx),
                WingScreenTab::IoRec => render_screen_io_rec_tab(ui, ctx),
            }
        });
}

fn render_screen_header(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let ch_count = ctx.mixer.channels().len();
    let t = ctx.lang.text();

    ui.horizontal_wrapped(|ui| {
        let (tag_bg, tag_title, src_summary, maybe_icon, mut maybe_ch_rec) =
            match ctx.state.selection {
                WingSelection::Channel(idx) => {
                    let ch = &ctx.mixer.channels()[idx];
                    let rgb = ch.color;
                    let bg = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
                    let src = match ch.source.mode {
                        ChannelInputMode::Unassigned => t.mixer_unassigned.to_string(),
                        ChannelInputMode::Multimedia => {
                            if ch.media.loading {
                                t.mixer_media_loading.to_string()
                            } else if !ch.media.title.is_empty() {
                                format!("MEDIA · {}", truncate_chars(&ch.media.title, 20))
                            } else {
                                t.mixer_media_no_track.to_string()
                            }
                        }
                        mode => format!(
                            "{} · {}",
                            truncate_chars(&ch.source.device_label, 14),
                            format_input_mode(mode)
                        ),
                    };
                    (
                        bg,
                        format!("CH {:02}  {}", idx + 1, truncate_chars(&ch.name, 12)),
                        src,
                        Some(ch.icon.clone()),
                        Some((idx, ch.dsp)),
                    )
                }
                WingSelection::Master => {
                    let out_name = ctx
                        .output_devices
                        .iter()
                        .find(|d| d.id == ctx.state.selected_output)
                        .map(|d| short_device_name(&d.name))
                        .unwrap_or_else(|| ctx.state.selected_output.clone());
                    (
                        WingColors::WING_ORANGE,
                        "MAIN L/R  MASTER".to_string(),
                        out_name,
                        None,
                        None,
                    )
                }
            };

        let tag_lum = tag_bg.r() as f32 * 0.299 + tag_bg.g() as f32 * 0.587 + tag_bg.b() as f32 * 0.114;
        let tag_fg = if tag_lum > 130.0 {
            Color32::from_rgb(12, 14, 20)
        } else {
            Color32::from_rgb(235, 240, 250)
        };
        let tag_border = if tag_lum < 50.0 {
            Stroke::new(1.0_f32, Color32::from_rgb(52, 58, 72))
        } else {
            Stroke::NONE
        };

        egui::Frame::new()
            .fill(tag_bg)
            .stroke(tag_border)
            .corner_radius(CornerRadius::same(2))
            .inner_margin(Margin::symmetric(8, 3))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    if let Some(ref ic) = maybe_icon {
                        if *ic != ChannelIcon::None {
                            let (ic_rect, _) =
                                ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                            draw_channel_icon(
                                ui,
                                ctx.state,
                                ic_rect,
                                ic,
                                tag_fg,
                            );
                        }
                    }
                    ui.label(
                        RichText::new(tag_title)
                            .strong()
                            .size(12.0)
                            .color(tag_fg),
                    );
                });
            });

        if draw_console_switch(ui, "<", false, StudioPalette::ACCENT_CYAN, Vec2::new(18.0, 18.0)) {
            ctx.state.selection = match ctx.state.selection {
                WingSelection::Channel(0) => WingSelection::Master,
                WingSelection::Channel(i) => WingSelection::Channel(i - 1),
                WingSelection::Master => WingSelection::Channel(ch_count.saturating_sub(1)),
            };
        }
        if draw_console_switch(ui, ">", false, StudioPalette::ACCENT_CYAN, Vec2::new(18.0, 18.0)) {
            ctx.state.selection = match ctx.state.selection {
                WingSelection::Channel(i) if i + 1 < ch_count => WingSelection::Channel(i + 1),
                WingSelection::Channel(_) => WingSelection::Master,
                WingSelection::Master => WingSelection::Channel(0),
            };
        }

        egui::Frame::new()
            .fill(Color32::from_rgb(20, 26, 38))
            .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
            .corner_radius(CornerRadius::same(2))
            .inner_margin(Margin::symmetric(7, 3))
            .show(ui, |ui| {
                ui.label(
                    RichText::new(src_summary)
                        .small()
                        .monospace()
                        .color(StudioPalette::ACCENT_CYAN),
                );
            });

        if let Some((ch_idx, mut dsp)) = maybe_ch_rec.take() {
            let lbl = if dsp.record_arm { "REC ON" } else { "REC OFF" };
            let r = draw_console_switch(
                ui,
                lbl,
                dsp.record_arm,
                StudioPalette::ACCENT_CRIMSON,
                Vec2::new(64.0, 21.0),
            );
            if r {
                dsp.record_arm = !dsp.record_arm;
                ctx.mixer.set_channel_dsp(ch_idx, dsp);
            }
        }

        let media_count = ctx.mixer.count_media_channels();
        if media_count > 0 {
            let any_playing = ctx.mixer.is_any_media_playing();
            let out = (!ctx.state.selected_output.is_empty()).then_some(ctx.state.selected_output.as_str());
            ui.separator();
            ui.label(
                RichText::new(format!("PLAYBACK ({media_count})"))
                    .small()
                    .monospace()
                    .strong()
                    .color(WingColors::WING_ORANGE),
            );
            if draw_console_switch(
                ui,
                "-10",
                false,
                Color32::from_rgb(70, 82, 104),
                Vec2::new(26.0, 19.0),
            ) {
                let _ = ctx.mixer.seek_all_media_relative_seconds(-10.0, out);
            }
            let play_lbl = if any_playing { "||" } else { ">" };
            if draw_console_switch(
                ui,
                play_lbl,
                any_playing,
                StudioPalette::ACCENT_EMERALD,
                Vec2::new(26.0, 19.0),
            ) {
                if !ctx.mixer.is_audio_running() && !any_playing {
                    (ctx.stop_classic_audio)();
                }
                let _ = ctx.mixer.set_all_media_transport(!any_playing, None, None, out);
            }
            if draw_console_switch(
                ui,
                "+10",
                false,
                Color32::from_rgb(70, 82, 104),
                Vec2::new(26.0, 19.0),
            ) {
                let _ = ctx.mixer.seek_all_media_relative_seconds(10.0, out);
            }
            if draw_console_switch(
                ui,
                "[]",
                false,
                Color32::from_rgb(80, 90, 110),
                Vec2::new(22.0, 19.0),
            ) {
                let _ = ctx.mixer.set_all_media_transport(false, None, Some(0), out);
            }
        }

        let sr_khz = ctx.mixer.sample_rate() as f32 / 1000.0;
        let live = ctx.mixer.is_audio_running();
        let (stat_lbl, stat_col) = if live {
            (
                format!("LIVE {:.1}kHz/{}smp", sr_khz, ctx.mixer.block_size()),
                StudioPalette::ACCENT_EMERALD,
            )
        } else {
            (
                format!("OFFLINE {:.1}kHz", sr_khz),
                StudioPalette::TEXT_MUTED,
            )
        };
        ui.label(RichText::new(stat_lbl).small().monospace().color(stat_col));
    });
}

fn render_screen_signal_blocks(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>, t: &Strings) {
    let active_vsts = match ctx.state.selection {
        WingSelection::Channel(i) => ctx.mixer.channels()[i]
            .inserts
            .iter()
            .filter(|s| s.meta.is_some())
            .count(),
        WingSelection::Master => ctx
            .mixer
            .master()
            .inserts
            .iter()
            .filter(|s| s.meta.is_some())
            .count(),
    };

    let vst_lbl = format!("VST FX ({active_vsts}/{MAX_MIXER_INSERTS})");
    let blocks: Vec<(WingScreenTab, &str, Color32)> = vec![
        (WingScreenTab::Source, t.mixer_tab_source, Color32::from_rgb(220, 224, 232)),
        (WingScreenTab::Eq, t.mixer_tab_eq, Color32::from_rgb(205, 210, 220)),
        (WingScreenTab::Dyn, t.mixer_tab_dyn, Color32::from_rgb(190, 195, 205)),
        (WingScreenTab::Vst, &vst_lbl, Color32::from_rgb(230, 234, 242)),
        (WingScreenTab::IoRec, t.mixer_tab_io, Color32::from_rgb(215, 220, 228)),
    ];

    ui.horizontal_wrapped(|ui| {
        for (idx, (tab, text, col)) in blocks.into_iter().enumerate() {
            let active = ctx.state.active_tab == tab;
            let fill = if active {
                Color32::from_rgb(44, 46, 54)
            } else {
                Color32::from_rgb(18, 18, 22)
            };
            let stroke = if active {
                Stroke::new(1.3_f32, Color32::WHITE)
            } else {
                Stroke::new(1.0_f32, Color32::from_rgb(46, 46, 54))
            };
            let resp = ui
                .scope(|ui| {
                    egui::Frame::new()
                        .fill(fill)
                        .stroke(stroke)
                        .corner_radius(CornerRadius::same(2))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(text)
                                    .small()
                                    .strong()
                                    .monospace()
                                    .color(if active { Color32::WHITE } else { col }),
                            );
                        });
                })
                .response
                .interact(Sense::click());
            if resp.clicked() {
                ctx.state.active_tab = tab;
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if idx < 4 {
                ui.label(RichText::new("->").small().monospace().color(Color32::from_rgb(110, 115, 125)));
            }
        }
    });
}

fn render_screen_home_tab(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let total = ui.available_width().max(180.0);
    ui.set_max_width(total);
    let narrow = total < 620.0;
    if narrow {
        render_home_eq_preview(ui, ctx);
        ui.add_space(6.0);
        render_upper_vst_rack_section(ui, ctx, false);
    } else {
        let gap = ui.spacing().item_spacing.x;
        let left_w = ((total - gap) * 0.53).floor().max(180.0);
        let right_w = (total - gap - left_w).floor().max(160.0);
        ui.horizontal_top(|ui| {
            ui.allocate_ui_with_layout(
                Vec2::new(left_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_width(left_w);
                    render_home_eq_preview(ui, ctx);
                },
            );
            ui.allocate_ui_with_layout(
                Vec2::new(right_w, 0.0),
                egui::Layout::top_down(egui::Align::Min),
                |ui| {
                    ui.set_max_width(right_w);
                    render_upper_vst_rack_section(ui, ctx, false);
                },
            );
        });
    }
}

fn render_home_eq_preview(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    match ctx.state.selection {
        WingSelection::Channel(idx) => {
            let ch_snapshot = ctx.mixer.channels()[idx].clone();
            if ch_snapshot.source.mode == ChannelInputMode::Multimedia {
                render_multimedia_source_deck(ui, ctx, idx, &ch_snapshot);
                ui.add_space(4.0);
            }

            let mut dsp = ctx.mixer.channels()[idx].dsp;
            let sr = ctx.mixer.sample_rate() as f32;
            let mut changed = false;

            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(t.mixer_trim_label)
                        .small()
                        .monospace()
                        .color(StudioPalette::ACCENT_CYAN),
                );
                if ui
                    .add(
                        egui::DragValue::new(&mut dsp.trim_db)
                            .range(-24.0..=24.0)
                            .speed(0.15)
                            .suffix(" dB"),
                    )
                    .changed()
                {
                    changed = true;
                }

                ui.label(
                    RichText::new(t.mixer_balance_label)
                        .small()
                        .monospace()
                        .color(WingColors::WING_ORANGE),
                );
                if ui
                    .add(
                        egui::DragValue::new(&mut dsp.balance)
                            .range(-1.0..=1.0)
                            .speed(0.02),
                    )
                    .changed()
                {
                    changed = true;
                }

                if ui.checkbox(&mut dsp.phase_invert, "Ø").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut dsp.hpf_enabled, "HPF").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut dsp.lpf_enabled, "LPF").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut dsp.gate_enabled, "GATE").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut dsp.comp_enabled, "COMP").changed() {
                    changed = true;
                }
            });

            ui.add_space(4.0);
            draw_wing_eq_curve(ui, &dsp, sr, 118.0);
            ui.add_space(4.0);

            ui.horizontal_wrapped(|ui| {
                let labels = ["LOW", "LO-M", "HI-M", "HIGH"];
                for (i, band_lbl) in labels.iter().enumerate() {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new(*band_lbl)
                                .small()
                                .monospace()
                                .color(WingColors::WING_ORANGE),
                        );
                        if ui
                            .add(
                                egui::DragValue::new(&mut dsp.eq_bands[i].gain_db)
                                    .range(-18.0..=18.0)
                                    .speed(0.15)
                                    .suffix(" dB"),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                }
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(t.mixer_pan_label)
                            .small()
                            .monospace()
                            .color(StudioPalette::ACCENT_CYAN),
                    );
                    if ui
                        .add(
                            egui::DragValue::new(&mut dsp.pan)
                                .range(-1.0..=1.0)
                                .speed(0.02),
                        )
                        .changed()
                    {
                        changed = true;
                    }
                });
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new(t.mixer_width_label)
                            .small()
                            .monospace()
                            .color(StudioPalette::ACCENT_EMERALD),
                    );
                    let mut width_pct = dsp.stereo_width * 100.0;
                    if ui
                        .add(
                            egui::DragValue::new(&mut width_pct)
                                .range(0.0..=200.0)
                                .speed(1.0)
                                .suffix("%"),
                        )
                        .changed()
                    {
                        dsp.stereo_width = (width_pct / 100.0).clamp(0.0, 2.0);
                        changed = true;
                    }
                });
            });

            if changed {
                ctx.mixer.set_channel_dsp(idx, dsp);
            }
        }
        WingSelection::Master => {
            let m = ctx.mixer.master().clone();
            let mut fader_db = m.fader_db;
            let mut mute = m.mute;
            let mut dim = m.dim;
            let mut rec_mix = m.record_mix;
            let mut changed = false;

            ui.label(
                RichText::new("MAIN L/R MASTER BUS")
                    .strong()
                    .color(WingColors::WING_ORANGE),
            );
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                if ui.checkbox(&mut mute, "MUTE MAIN").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut dim, "DIM (-12dB)").changed() {
                    changed = true;
                }
                if ui.checkbox(&mut rec_mix, t.mixer_rec_mix_toggle).changed() {
                    changed = true;
                }
            });
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                ui.label(RichText::new("Master Level:").small().color(StudioPalette::TEXT_SECONDARY));
                if ui
                    .add(
                        egui::Slider::new(&mut fader_db, -60.0..=10.0)
                            .suffix(" dB")
                            .fixed_decimals(1),
                    )
                    .changed()
                {
                    changed = true;
                }
            });
            if changed {
                ctx.mixer.set_master(fader_db, mute, dim, rec_mix);
            }
        }
    }
}

fn render_screen_source_tab(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let WingSelection::Channel(idx) = ctx.state.selection else {
        ui.label(
            RichText::new("MAIN L/R Bus recibe la suma de todos los canales activos.")
                .color(StudioPalette::TEXT_SECONDARY),
        );
        return;
    };

    let ch = ctx.mixer.channels()[idx].clone();
    let mut name_buf = ch.name.clone();
    let mut dsp = ch.dsp;
    let mut dsp_changed = false;

    ui.horizontal(|ui| {
        ui.label(RichText::new(t.mixer_channel_name).strong().color(StudioPalette::TEXT_SECONDARY));
        let edit_w = (ui.available_width() * 0.40).clamp(110.0, 190.0);
        if ui
            .add_sized(Vec2::new(edit_w, 22.0), egui::TextEdit::singleline(&mut name_buf))
            .changed()
        {
            ctx.mixer.set_channel_name(idx, name_buf);
        }
    });

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(t.mixer_channel_color).small().strong().color(WingColors::WING_ORANGE));
        ui.spacing_mut().item_spacing = Vec2::new(3.0, 3.0);
        for rgb in WING_PALETTE_SWATCHES {
            let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let selected = ch.color == rgb;
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                ctx.mixer.set_channel_color(idx, rgb);
            }
            let is_dark = (rgb[0] as f32 * 0.299 + rgb[1] as f32 * 0.587 + rgb[2] as f32 * 0.114) < 45.0;
            let base_stroke = if is_dark {
                Stroke::new(1.0_f32, Color32::from_rgb(60, 66, 80))
            } else {
                Stroke::NONE
            };
            ui.painter().rect(
                r,
                CornerRadius::same(3),
                swatch,
                base_stroke,
                StrokeKind::Inside,
            );
            if selected {
                ui.painter().rect_stroke(
                    r.expand(1.0),
                    CornerRadius::same(3),
                    Stroke::new(2.0_f32, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
        }
    });

    ui.add_space(4.0);
    render_channel_icon_picker_row(ui, ctx, idx, &ch.icon);

    ui.add_space(5.0);
    if ch.source.mode == ChannelInputMode::Multimedia {
        render_multimedia_source_deck(ui, ctx, idx, &ch);
    } else {
        ui.horizontal_wrapped(|ui| {
            let cur_label = if ch.source.mode == ChannelInputMode::Unassigned {
                t.mixer_unassigned.to_string()
            } else {
                format!("{} -> {}", ch.source.device_label, format_input_mode(ch.source.mode))
            };

            let mut next_source: Option<ChannelInputSource> = None;
            egui::ComboBox::from_id_salt(("wing_source_combo", idx))
                .width((ui.available_width() - 195.0).clamp(120.0, 290.0))
                .selected_text(cur_label)
                .show_ui(ui, |ui| {
                    if ui
                        .selectable_label(
                            ch.source.mode == ChannelInputMode::Unassigned,
                            t.mixer_disconnect_input,
                        )
                        .clicked()
                    {
                        next_source = Some(ChannelInputSource::default());
                    }
                    for dev in ctx.input_devices {
                        ui.separator();
                        ui.label(
                            RichText::new(&dev.name)
                                .small()
                                .strong()
                                .color(StudioPalette::ACCENT_CYAN),
                        );
                        let max_ch = (dev.default_channels as usize).clamp(2, 16);
                        for c in 0..max_ch {
                            let mode = ChannelInputMode::Mono(c);
                            let sel = ch.source.device_id.as_deref() == Some(dev.id.as_str())
                                && ch.source.mode == mode;
                            if ui.selectable_label(sel, format!("  Mono In {}", c + 1)).clicked() {
                                next_source = Some(ChannelInputSource {
                                    device_id: Some(dev.id.clone()),
                                    device_label: short_device_name(&dev.name),
                                    mode,
                                });
                            }
                        }
                        let mut pair = 0;
                        while pair + 1 < max_ch {
                            let mode = ChannelInputMode::Stereo(pair, pair + 1);
                            let sel = ch.source.device_id.as_deref() == Some(dev.id.as_str())
                                && ch.source.mode == mode;
                            if ui
                                .selectable_label(sel, format!("  Stereo {}+{}", pair + 1, pair + 2))
                                .clicked()
                            {
                                next_source = Some(ChannelInputSource {
                                    device_id: Some(dev.id.clone()),
                                    device_label: short_device_name(&dev.name),
                                    mode,
                                });
                            }
                            pair += 2;
                        }
                    }
                });
            if ui.small_button(t.refresh).clicked() {
                (ctx.refresh_devices)();
            }
            if ui
                .add(
                    egui::Button::new(
                        RichText::new(t.mixer_media_tag)
                            .small()
                            .strong()
                            .color(Color32::from_rgb(10, 14, 20)),
                    )
                    .fill(StudioPalette::ACCENT_CYAN)
                    .corner_radius(CornerRadius::same(2)),
                )
                .on_hover_text(t.mixer_media_convert)
                .clicked()
            {
                ctx.mixer.set_channel_multimedia(idx);
            }
            if let Some(src) = next_source {
                assign_channel_input_with_audio(ctx, idx, src);
            }
        });
    }

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        if draw_rotary_knob(
            ui,
            &format!("wing_src_trim_{idx}"),
            t.mixer_trim_label,
            &mut dsp.trim_db,
            -24.0..=24.0,
            0.0,
            "dB",
            StudioPalette::ACCENT_CYAN,
        ) {
            dsp_changed = true;
        }
        if draw_rotary_knob(
            ui,
            &format!("wing_src_bal_{idx}"),
            t.mixer_balance_label,
            &mut dsp.balance,
            -1.0..=1.0,
            0.0,
            "",
            WingColors::WING_ORANGE,
        ) {
            dsp_changed = true;
        }
        let mut width_pct = dsp.stereo_width * 100.0;
        if draw_rotary_knob(
            ui,
            &format!("wing_src_width_{idx}"),
            t.mixer_width_label,
            &mut width_pct,
            0.0..=200.0,
            100.0,
            "%",
            StudioPalette::ACCENT_PURPLE,
        ) {
            dsp.stereo_width = (width_pct / 100.0).clamp(0.0, 2.0);
            dsp_changed = true;
        }
        if draw_rotary_knob(
            ui,
            &format!("wing_src_hpf_{idx}"),
            "LCUT Hz",
            &mut dsp.hpf_freq_hz,
            20.0..=400.0,
            80.0,
            "Hz",
            WingColors::WING_ORANGE,
        ) {
            dsp_changed = true;
        }
        if draw_rotary_knob(
            ui,
            &format!("wing_src_lpf_{idx}"),
            t.mixer_lpf_label,
            &mut dsp.lpf_freq_hz,
            1500.0..=20000.0,
            16000.0,
            "Hz",
            StudioPalette::ACCENT_AMBER,
        ) {
            dsp_changed = true;
        }
        if draw_rotary_knob(
            ui,
            &format!("wing_src_pan_{idx}"),
            t.mixer_pan_label,
            &mut dsp.pan,
            -1.0..=1.0,
            0.0,
            "",
            StudioPalette::ACCENT_EMERALD,
        ) {
            dsp_changed = true;
        }
    });

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut dsp.phase_invert, "Ø 180°").changed() {
            dsp_changed = true;
        }
        if ui.checkbox(&mut dsp.swap_lr, t.mixer_swap_lr).changed() {
            dsp_changed = true;
        }
        if ui.checkbox(&mut dsp.hpf_enabled, "LCUT (HPF)").changed() {
            dsp_changed = true;
        }
        if ui.checkbox(&mut dsp.lpf_enabled, t.mixer_lpf_label).changed() {
            dsp_changed = true;
        }
        if ui.checkbox(&mut dsp.solo_safe, t.mixer_solo_safe).changed() {
            dsp_changed = true;
        }
        if ui.checkbox(&mut dsp.fx_bypass, t.mixer_fx_bypass).changed() {
            dsp_changed = true;
        }
        if ui.checkbox(&mut dsp.pre_fader_rec, t.mixer_pre_fader_rec).changed() {
            dsp_changed = true;
        }
        if ui
            .checkbox(&mut dsp.send_to_virtual_mic, t.mixer_vmic_send_ch)
            .changed()
        {
            dsp_changed = true;
        }
    });

    if dsp_changed {
        ctx.mixer.set_channel_dsp(idx, dsp);
    }
}

fn render_channel_icon_picker_row(
    ui: &mut egui::Ui,
    ctx: &mut MixerPanelContext<'_>,
    idx: usize,
    current_icon: &ChannelIcon,
) {
    let t = ctx.lang.text();
    let mut chosen_icon: Option<ChannelIcon> = None;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(t.mixer_icon_label)
                .small()
                .strong()
                .color(StudioPalette::TEXT_SECONDARY),
        );
        for preset in ChannelIcon::presets() {
            let active = current_icon == preset;
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(26.0, 22.0), Sense::click());
            if resp.clicked() {
                chosen_icon = Some(preset.clone());
            }
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let bg = if active {
                Color32::from_rgb(36, 52, 74)
            } else {
                Color32::from_rgb(20, 24, 34)
            };
            let stroke_col = if active {
                StudioPalette::ACCENT_CYAN
            } else {
                Color32::from_rgb(48, 56, 72)
            };
            ui.painter().rect_filled(rect, CornerRadius::same(2), bg);
            ui.painter().rect_stroke(
                rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, stroke_col),
                StrokeKind::Inside,
            );
            if *preset == ChannelIcon::None {
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    "--",
                    FontId::monospace(9.0),
                    StudioPalette::TEXT_MUTED,
                );
            } else {
                let ic_rect = rect.shrink2(Vec2::new(4.0, 2.0));
                draw_channel_icon(
                    ui,
                    ctx.state,
                    ic_rect,
                    preset,
                    if active {
                        StudioPalette::ACCENT_CYAN
                    } else {
                        StudioPalette::TEXT_PRIMARY
                    },
                );
            }
            resp.on_hover_text(channel_icon_name(preset, ctx.lang));
        }

        let is_custom = matches!(current_icon, ChannelIcon::CustomImage(_));
        if ui
            .add(
                egui::Button::new(
                    RichText::new(t.mixer_icon_custom)
                        .small()
                        .strong()
                        .color(if is_custom {
                            Color32::from_rgb(10, 14, 20)
                        } else {
                            Color32::WHITE
                        }),
                )
                .fill(if is_custom {
                    WingColors::WING_ORANGE
                } else {
                    Color32::from_rgb(36, 44, 58)
                })
                .corner_radius(CornerRadius::same(2)),
            )
            .clicked()
        {
            if let Some(img_path) = pick_custom_icon_dialog() {
                ctx.state.custom_icon_textures.remove(&img_path);
                chosen_icon = Some(ChannelIcon::CustomImage(img_path));
            }
        }
    });

    if let Some(ic) = chosen_icon {
        ctx.mixer.set_channel_icon(idx, ic);
    }
}

fn render_multimedia_source_deck(
    ui: &mut egui::Ui,
    ctx: &mut MixerPanelContext<'_>,
    idx: usize,
    ch: &MixerChannelState,
) {
    let t = ctx.lang.text();
    let media = &ch.media;
    let has_track = media.total_frames > 0;
    let sr = media.sample_rate.max(1) as f64;
    let cur_secs = media.current_frame as f64 / sr;
    let total_secs = media.total_frames as f64 / sr;

    egui::Frame::new()
        .fill(Color32::from_rgb(16, 22, 32))
        .stroke(Stroke::new(1.0_f32, StudioPalette::ACCENT_CYAN))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(7))
        .show(ui, |ui| {
            let inner_w = ui.available_width().max(150.0);
            ui.set_max_width(inner_w);
            ui.horizontal_wrapped(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new(t.mixer_media_load)
                                .small()
                                .strong()
                                .color(Color32::from_rgb(10, 14, 20)),
                        )
                        .fill(StudioPalette::ACCENT_CYAN)
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    trigger_load_media_dialog(ctx, idx);
                }

                if ui
                    .add_enabled(
                        has_track && !media.loading,
                        egui::Button::new(
                            RichText::new(format!("<< {}", t.mixer_media_back_10s))
                                .small()
                                .strong(),
                        )
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    let out = (!ctx.state.selected_output.is_empty())
                        .then_some(ctx.state.selected_output.as_str());
                    let _ = ctx.mixer.seek_media_relative_seconds(idx, -10.0, out);
                }

                let play_lbl = if media.playing {
                    t.mixer_media_pause
                } else {
                    t.mixer_media_play
                };
                if ui
                    .add_enabled(
                        has_track && !media.loading,
                        egui::Button::new(
                            RichText::new(play_lbl)
                                .small()
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(if media.playing {
                            StudioPalette::ACCENT_EMERALD
                        } else {
                            Color32::from_rgb(38, 48, 66)
                        })
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    if !ctx.mixer.is_audio_running() && !media.playing {
                        (ctx.stop_classic_audio)();
                    }
                    let out = (!ctx.state.selected_output.is_empty())
                        .then_some(ctx.state.selected_output.as_str());
                    if let Err(e) = ctx.mixer.set_media_transport(
                        idx,
                        !media.playing,
                        media.looping,
                        if !media.playing && media.current_frame >= media.total_frames {
                            Some(0)
                        } else {
                            None
                        },
                        out,
                    ) {
                        *ctx.error = Some(e.to_string());
                    }
                }

                if ui
                    .add_enabled(
                        has_track && !media.loading,
                        egui::Button::new(
                            RichText::new(format!("{} >>", t.mixer_media_fwd_10s))
                                .small()
                                .strong(),
                        )
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    let out = (!ctx.state.selected_output.is_empty())
                        .then_some(ctx.state.selected_output.as_str());
                    let _ = ctx.mixer.seek_media_relative_seconds(idx, 10.0, out);
                }

                if ui
                    .add_enabled(
                        has_track && !media.loading,
                        egui::Button::new(RichText::new(t.mixer_media_stop).small())
                            .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    let out = (!ctx.state.selected_output.is_empty())
                        .then_some(ctx.state.selected_output.as_str());
                    let _ = ctx
                        .mixer
                        .set_media_transport(idx, false, media.looping, Some(0), out);
                }

                if draw_console_switch(
                    ui,
                    t.mixer_media_loop,
                    media.looping,
                    WingColors::WING_ORANGE,
                    Vec2::new(56.0, 21.0),
                ) {
                    let out = (!ctx.state.selected_output.is_empty())
                        .then_some(ctx.state.selected_output.as_str());
                    let _ = ctx.mixer.set_media_transport(
                        idx,
                        media.playing,
                        !media.looping,
                        None,
                        out,
                    );
                }

                if ctx.mixer.count_media_channels() > 1 {
                    let any_playing = ctx.mixer.is_any_media_playing();
                    let all_lbl = if any_playing { "PAUSE ALL" } else { "PLAY ALL (SYNC)" };
                    if draw_console_switch(
                        ui,
                        all_lbl,
                        any_playing,
                        StudioPalette::ACCENT_CYAN,
                        Vec2::new(96.0, 21.0),
                    ) {
                        if !ctx.mixer.is_audio_running() && !any_playing {
                            (ctx.stop_classic_audio)();
                        }
                        let out = (!ctx.state.selected_output.is_empty())
                            .then_some(ctx.state.selected_output.as_str());
                        let _ = ctx.mixer.set_all_media_transport(!any_playing, None, None, out);
                    }
                }

                if ui.small_button(t.mixer_media_to_audio).clicked() {
                    assign_channel_input_with_audio(ctx, idx, ChannelInputSource::default());
                }
            });

            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let status_txt = if media.loading {
                    t.mixer_media_loading.to_string()
                } else if has_track {
                    format!(
                        "{} ({:.1} kHz)",
                        truncate_chars(&media.title, 34),
                        sr / 1000.0
                    )
                } else {
                    t.mixer_media_no_track.to_string()
                };
                ui.label(
                    RichText::new(status_txt)
                        .small()
                        .strong()
                        .color(if has_track {
                            Color32::WHITE
                        } else {
                            StudioPalette::TEXT_MUTED
                        }),
                );
                ui.label(
                    RichText::new(format!(
                        "[{} / {}]",
                        format_media_time(cur_secs),
                        format_media_time(total_secs)
                    ))
                    .small()
                    .monospace()
                    .color(StudioPalette::ACCENT_CYAN),
                );
            });

            ui.add_space(3.0);
            let bar_w = ui.available_width().max(100.0);
            let (bar_rect, bar_resp) =
                ui.allocate_exact_size(Vec2::new(bar_w, 14.0), Sense::click_and_drag());
            let frac = if media.total_frames > 0 {
                (media.current_frame as f32 / media.total_frames as f32).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let p = ui.painter();
            p.rect_filled(bar_rect, CornerRadius::same(2), Color32::from_rgb(10, 13, 18));
            let fill_w = (bar_rect.width() * frac).max(0.0);
            if fill_w > 0.5 {
                let fill_rect =
                    Rect::from_min_size(bar_rect.min, Vec2::new(fill_w, bar_rect.height()));
                p.rect_filled(
                    fill_rect,
                    CornerRadius::same(2),
                    if media.playing {
                        StudioPalette::ACCENT_CYAN
                    } else {
                        WingColors::WING_ORANGE
                    },
                );
            }
            p.rect_stroke(
                bar_rect,
                CornerRadius::same(2),
                Stroke::new(1.0_f32, Color32::from_rgb(56, 70, 92)),
                StrokeKind::Inside,
            );
            if has_track && (bar_resp.clicked() || bar_resp.dragged()) {
                if let Some(pos) = bar_resp.interact_pointer_pos() {
                    let norm =
                        ((pos.x - bar_rect.left()) / bar_rect.width().max(1.0)).clamp(0.0, 1.0);
                    let target_frame = (norm * media.total_frames as f32).round() as usize;
                    let out = (!ctx.state.selected_output.is_empty())
                        .then_some(ctx.state.selected_output.as_str());
                    let _ = ctx.mixer.set_media_transport(
                        idx,
                        media.playing,
                        media.looping,
                        Some(target_frame),
                        out,
                    );
                }
            }
        });
}

fn render_screen_eq_tab(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let WingSelection::Channel(idx) = ctx.state.selection else {
        ui.label(
            RichText::new("Selecciona un canal de audio en la mesa para editar su EQ paramétrica.")
                .color(StudioPalette::TEXT_SECONDARY),
        );
        return;
    };

    let mut dsp = ctx.mixer.channels()[idx].dsp;
    let sr = ctx.mixer.sample_rate() as f32;
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        if ui.checkbox(&mut dsp.eq_enabled, "EQ ON").changed() {
            changed = true;
        }
        if ui.checkbox(&mut dsp.hpf_enabled, "LCUT (HPF)").changed() {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut dsp.hpf_freq_hz)
                    .range(20.0..=400.0)
                    .speed(1.5)
                    .suffix(" Hz"),
            )
            .changed()
        {
            changed = true;
        }
        if ui.checkbox(&mut dsp.lpf_enabled, "HCUT (LPF)").changed() {
            changed = true;
        }
        if ui
            .add(
                egui::DragValue::new(&mut dsp.lpf_freq_hz)
                    .range(1500.0..=20000.0)
                    .speed(25.0)
                    .suffix(" Hz"),
            )
            .changed()
        {
            changed = true;
        }
    });

    ui.add_space(4.0);
    draw_wing_eq_curve(ui, &dsp, sr, 150.0);
    ui.add_space(6.0);

    let band_names = ["1: LOW SHELF", "2: LO-MID BELL", "3: HI-MID BELL", "4: HIGH SHELF"];
    ui.horizontal_wrapped(|ui| {
        for (i, title) in band_names.iter().enumerate() {
            egui::Frame::new()
                .fill(Color32::from_rgb(18, 22, 31))
                .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                .corner_radius(CornerRadius::same(2))
                .inner_margin(Margin::same(6))
                .show(ui, |ui| {
                    ui.set_min_width(142.0);
                    ui.vertical(|ui| {
                        if ui.checkbox(&mut dsp.eq_bands[i].enabled, *title).changed() {
                            changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut dsp.eq_bands[i].freq_hz, 25.0..=18000.0)
                                    .logarithmic(true)
                                    .suffix(" Hz")
                                    .fixed_decimals(0),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut dsp.eq_bands[i].gain_db, -18.0..=18.0)
                                    .suffix(" dB")
                                    .fixed_decimals(1),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                        if ui
                            .add(
                                egui::Slider::new(&mut dsp.eq_bands[i].q, 0.4..=8.0)
                                    .prefix("Q ")
                                    .fixed_decimals(2),
                            )
                            .changed()
                        {
                            changed = true;
                        }
                    });
                });
        }
    });

    if changed {
        ctx.mixer.set_channel_dsp(idx, dsp);
    }
}

fn render_screen_dyn_tab(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let WingSelection::Channel(idx) = ctx.state.selection else {
        ui.label(
            RichText::new("Selecciona un canal de audio para ajustar Gate y Compresor.")
                .color(StudioPalette::TEXT_SECONDARY),
        );
        return;
    };

    let mut dsp = ctx.mixer.channels()[idx].dsp;
    let gr_db = ctx
        .state
        .meters
        .channels
        .get(idx)
        .map(|m| m.comp_gr_db)
        .unwrap_or(0.0);
    let mut changed = false;

    ui.horizontal_wrapped(|ui| {
        draw_wing_dynamics_curve(ui, &dsp, gr_db);
        ui.vertical(|ui| {
            ui.label(
                RichText::new("NOISE GATE")
                    .strong()
                    .color(StudioPalette::ACCENT_CYAN),
            );
            if ui.checkbox(&mut dsp.gate_enabled, "Gate Active").changed() {
                changed = true;
            }
            if ui
                .add(
                    egui::Slider::new(&mut dsp.gate_threshold_db, -80.0..=-6.0)
                        .suffix(" dB")
                        .fixed_decimals(1),
                )
                .changed()
            {
                changed = true;
            }

            ui.add_space(6.0);
            ui.label(
                RichText::new("COMPRESSOR")
                    .strong()
                    .color(WingColors::WING_ORANGE),
            );
            if ui.checkbox(&mut dsp.comp_enabled, "Compressor Active").changed() {
                changed = true;
            }
            if ui
                .add(
                    egui::Slider::new(&mut dsp.comp_threshold_db, -50.0..=0.0)
                        .prefix("Thr ")
                        .suffix(" dB")
                        .fixed_decimals(1),
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .add(
                    egui::Slider::new(&mut dsp.comp_ratio, 1.0..=12.0)
                        .prefix("Ratio 1:")
                        .fixed_decimals(1),
                )
                .changed()
            {
                changed = true;
            }
            if ui
                .add(
                    egui::Slider::new(&mut dsp.comp_makeup_db, 0.0..=18.0)
                        .prefix("Makeup ")
                        .suffix(" dB")
                        .fixed_decimals(1),
                )
                .changed()
            {
                changed = true;
            }
        });
    });

    if changed {
        ctx.mixer.set_channel_dsp(idx, dsp);
    }
}

fn render_screen_vst_tab(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    render_upper_vst_rack_section(ui, ctx, true);
}

fn render_upper_vst_rack_section(
    ui: &mut egui::Ui,
    ctx: &mut MixerPanelContext<'_>,
    show_params: bool,
) {
    let t = ctx.lang.text();
    let sel = ctx.state.selection;
    let slot_count = MAX_MIXER_INSERTS;

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(t.mixer_upper_vst_title)
                .small()
                .strong()
                .color(StudioPalette::ACCENT_PURPLE),
        );
        if show_params {
            ui.add_sized(
                Vec2::new(150.0, 22.0),
                egui::TextEdit::singleline(&mut ctx.state.vst_picker_filter)
                    .hint_text("Filter VSTs..."),
            );
        }
    });
    ui.add_space(4.0);

    for slot_idx in 0..slot_count {
        let target = match sel {
            WingSelection::Channel(ch) => MixerInsertTarget::Channel { ch, slot: slot_idx },
            WingSelection::Master => MixerInsertTarget::Master { slot: slot_idx },
        };
        let slot_info = match sel {
            WingSelection::Channel(ch) => ctx.mixer.channels()[ch].inserts[slot_idx].clone(),
            WingSelection::Master => ctx.mixer.master().inserts[slot_idx].clone(),
        };
        let focused = ctx.state.focused_vst_slot == slot_idx;
        let gui_open = ctx.mixer.is_insert_window_open(target);

        let border_col = if focused && show_params {
            StudioPalette::ACCENT_PURPLE
        } else if slot_info.meta.is_some() {
            StudioPalette::BORDER_BRIGHT
        } else {
            StudioPalette::BORDER_SUBTLE
        };

        egui::Frame::new()
            .fill(Color32::from_rgb(17, 21, 30))
            .stroke(Stroke::new(1.0_f32, border_col))
            .corner_radius(CornerRadius::same(2))
            .inner_margin(Margin::symmetric(8, 5))
            .show(ui, |ui| {
                ui.horizontal_wrapped(|ui| {
                    ui.label(
                        RichText::new(format!("FX{}", slot_idx + 1))
                            .small()
                            .strong()
                            .monospace()
                            .color(WingColors::WING_ORANGE),
                    );

                    if let Some(ref meta) = slot_info.meta {
                        let name_btn = ui.selectable_label(
                            focused && show_params,
                            RichText::new(&meta.info.name)
                                .small()
                                .strong()
                                .color(Color32::WHITE),
                        );
                        if name_btn.clicked() {
                            ctx.state.focused_vst_slot = slot_idx;
                            ctx.state.active_tab = WingScreenTab::Vst;
                        }

                        let badge = if meta.is_wine_bridged { "Wine" } else { "Native" };
                        ui.label(
                            RichText::new(format!("[{badge}]"))
                                .small()
                                .monospace()
                                .color(StudioPalette::ACCENT_CYAN),
                        );

                        let mut bypass = slot_info.bypass;
                        let mut mix = slot_info.mix * 100.0;
                        let byp_lbl = if bypass { "BYP" } else { "ON" };
                        let byp_col = if bypass {
                            StudioPalette::ACCENT_AMBER
                        } else {
                            StudioPalette::ACCENT_EMERALD
                        };
                        if ui
                            .button(RichText::new(byp_lbl).small().strong().color(byp_col))
                            .clicked()
                        {
                            bypass = !bypass;
                            ctx.mixer
                                .set_insert_bypass_and_mix(target, bypass, mix / 100.0);
                        }

                        if ui
                            .add(
                                egui::DragValue::new(&mut mix)
                                    .range(0.0..=100.0)
                                    .speed(1.0)
                                    .suffix("%"),
                            )
                            .changed()
                        {
                            ctx.mixer
                                .set_insert_bypass_and_mix(target, bypass, mix / 100.0);
                        }

                        if meta.info.has_gui {
                            let gui_txt = if gui_open { "GUI*" } else { "GUI" };
                            if ui.small_button(gui_txt).clicked() {
                                if gui_open {
                                    ctx.mixer.close_insert_window(target);
                                } else if let Err(e) = ctx.mixer.open_insert_window(target) {
                                    *ctx.error = Some(e.to_string());
                                }
                            }
                        }

                        if ui.small_button("×").clicked() {
                            ctx.mixer.unload_insert_plugin(target);
                        }
                    } else {
                        let filter_lc = ctx.state.vst_picker_filter.trim().to_lowercase();
                        let mut chosen_path: Option<PathBuf> = None;

                        egui::ComboBox::from_id_salt(("wing_vst_add", target))
                            .width((ui.available_width() - 95.0).clamp(115.0, 210.0))
                            .selected_text(t.mixer_add_vst)
                            .show_ui(ui, |ui| {
                                let matching: Vec<&PluginInfo> = ctx
                                    .discovered_plugins
                                    .iter()
                                    .filter(|p| {
                                        filter_lc.is_empty()
                                            || p.name.to_lowercase().contains(&filter_lc)
                                            || p.category.to_lowercase().contains(&filter_lc)
                                    })
                                    .collect();
                                if matching.is_empty() {
                                    ui.label(
                                        RichText::new(t.mixer_no_vsts_scanned)
                                            .small()
                                            .color(StudioPalette::TEXT_MUTED),
                                    );
                                } else {
                                    for p in matching {
                                        let label = format!("{} [{}]", p.name, p.category);
                                        if ui.selectable_label(false, label).clicked() {
                                            chosen_path = Some(p.path.clone());
                                        }
                                    }
                                }
                            });

                        if ui.small_button(t.mixer_browse_vst).clicked() {
                            if let Some(file) = rfd::FileDialog::new()
                                .set_title(t.dialog_select_plugin)
                                .add_filter("VST2 / VST3", &["vst3", "dll", "so", "DLL", "SO"])
                                .pick_file()
                            {
                                chosen_path = Some(file);
                            }
                        }

                        if let Some(path) = chosen_path {
                            match ctx.mixer.load_insert_plugin(target, &path) {
                                Ok(meta) => {
                                    ctx.state.focused_vst_slot = slot_idx;
                                    *ctx.status = StatusState::LoadedPlugin {
                                        name: meta.info.name,
                                        params: meta.parameters.len(),
                                    };
                                    *ctx.error = None;
                                }
                                Err(e) => {
                                    *ctx.error = Some(e.to_string());
                                }
                            }
                        }
                    }
                });
            });
        ui.add_space(3.0);
    }

    if show_params {
        let focused_target = match sel {
            WingSelection::Channel(ch) => MixerInsertTarget::Channel {
                ch,
                slot: ctx.state.focused_vst_slot,
            },
            WingSelection::Master => MixerInsertTarget::Master {
                slot: ctx.state.focused_vst_slot,
            },
        };
        let meta_opt = match sel {
            WingSelection::Channel(ch) => ctx.mixer.channels()[ch].inserts
                [ctx.state.focused_vst_slot]
                .meta
                .clone(),
            WingSelection::Master => ctx.mixer.master().inserts[ctx.state.focused_vst_slot]
                .meta
                .clone(),
        };

        if let Some(meta) = meta_opt {
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(format!("{} {}", t.mixer_select_slot_params, meta.info.name))
                        .small()
                        .strong()
                        .color(StudioPalette::ACCENT_CYAN),
                );
                ui.add_sized(
                    Vec2::new(145.0, 22.0),
                    egui::TextEdit::singleline(&mut ctx.state.param_filter)
                        .hint_text(t.filter_params_hint),
                );
                if ui.small_button(t.refresh_params).clicked() {
                    ctx.mixer.refresh_insert_parameters(focused_target);
                }
            });
            ui.add_space(4.0);
            let filter_lc = ctx.state.param_filter.trim().to_lowercase();
            let mut param_updates: Vec<(u32, f64)> = Vec::new();
            egui::ScrollArea::vertical()
                .id_salt("wing_vst_params_scroll")
                .max_height(150.0)
                .show(ui, |ui| {
                    for p in meta.parameters.iter().filter(|p| {
                        filter_lc.is_empty() || p.param.name.to_lowercase().contains(&filter_lc)
                    }) {
                        let mut val = p.param.value;
                        ui.horizontal_wrapped(|ui| {
                            ui.label(
                                RichText::new(&p.param.name)
                                    .small()
                                    .color(StudioPalette::TEXT_SECONDARY),
                            );
                            if ui
                                .add_enabled(
                                    !p.param.is_read_only,
                                    egui::Slider::new(&mut val, p.param.min..=p.param.max)
                                        .show_value(false),
                                )
                                .changed()
                            {
                                param_updates.push((p.param.id, val));
                            }
                            ui.label(
                                RichText::new(&p.formatted)
                                    .small()
                                    .monospace()
                                    .color(StudioPalette::ACCENT_CYAN),
                            );
                        });
                    }
                });
            for (id, val) in param_updates {
                ctx.mixer.set_insert_parameter(focused_target, id, val);
            }
        }
    }
}

fn render_screen_io_rec_tab(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();

    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(t.mixer_output_label)
                .strong()
                .color(StudioPalette::TEXT_SECONDARY),
        );
        let selected_name = ctx
            .output_devices
            .iter()
            .find(|d| d.id == ctx.state.selected_output)
            .map(|d| d.name.clone())
            .unwrap_or_else(|| t.none.to_string());

        let mut next_out: Option<String> = None;
        egui::ComboBox::from_id_salt("wing_master_output_combo")
            .width((ui.available_width() - 95.0).clamp(150.0, 340.0))
            .selected_text(selected_name)
            .show_ui(ui, |ui| {
                for dev in ctx.output_devices {
                    let sel = dev.id == ctx.state.selected_output;
                    let label = format!("{} ({} ch)", dev.name, dev.default_channels);
                    if ui.selectable_label(sel, label).clicked() {
                        next_out = Some(dev.id.clone());
                    }
                }
            });
        if ui.small_button(t.refresh).clicked() {
            (ctx.refresh_devices)();
        }
        if let Some(new_out) = next_out {
            ctx.state.selected_output = new_out;
            if ctx.mixer.is_audio_running() {
                start_or_restart_mixer_audio(ctx);
            }
        }
    });

    ui.add_space(6.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(t.sample_rate_label).small().color(StudioPalette::TEXT_SECONDARY));
        let mut restart = false;
        egui::ComboBox::from_id_salt("wing_sr_combo")
            .width(92.0)
            .selected_text(format!("{} Hz", ctx.state.sample_rate))
            .show_ui(ui, |ui| {
                for sr in [44_100_u32, 48_000, 96_000] {
                    if ui
                        .selectable_value(&mut ctx.state.sample_rate, sr, format!("{sr} Hz"))
                        .clicked()
                    {
                        restart = true;
                    }
                }
            });
        ui.label(RichText::new(t.buffer_label).small().color(StudioPalette::TEXT_SECONDARY));
        egui::ComboBox::from_id_salt("wing_buf_combo")
            .width(82.0)
            .selected_text(format!("{}", ctx.state.block_size))
            .show_ui(ui, |ui| {
                for b in [128_usize, 256, 512, 1024] {
                    if ui
                        .selectable_value(&mut ctx.state.block_size, b, format!("{b}"))
                        .clicked()
                    {
                        restart = true;
                    }
                }
            });
        if restart && ctx.mixer.is_audio_running() {
            start_or_restart_mixer_audio(ctx);
        }
    });

    ui.add_space(8.0);
    let mut m = ctx.mixer.master().clone();
    if ui.checkbox(&mut m.record_mix, t.mixer_rec_mix_toggle).changed() {
        ctx.mixer
            .set_master(m.fader_db, m.mute, m.dim, m.record_mix);
    }

    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.label(
            RichText::new(ctx.state.recording_base_dir.display().to_string())
                .small()
                .monospace()
                .color(StudioPalette::ACCENT_CYAN),
        );
        if ui.small_button(t.folder).clicked() {
            if let Some(dir) = rfd::FileDialog::new()
                .set_directory(&ctx.state.recording_base_dir)
                .pick_folder()
            {
                ctx.state.recording_base_dir = dir;
            }
        }
    });

    ui.add_space(4.0);
    ui.label(
        RichText::new(t.mixer_rec_only_armed_note)
            .small()
            .color(StudioPalette::TEXT_SECONDARY),
    );
    ui.add_space(4.0);
    let armed: Vec<&MixerChannelState> = ctx
        .mixer
        .channels()
        .iter()
        .filter(|c| c.dsp.record_arm)
        .collect();
    ui.label(
        RichText::new(t.mixer_armed_tracks)
            .small()
            .strong()
            .color(StudioPalette::TEXT_SECONDARY),
    );
    if armed.is_empty() && !m.record_mix {
        ui.label(
            RichText::new(t.mixer_no_armed_tracks)
                .small()
                .color(StudioPalette::ACCENT_AMBER),
        );
    } else {
        ui.horizontal_wrapped(|ui| {
            for ch in armed {
                let col = Color32::from_rgb(ch.color[0], ch.color[1], ch.color[2]);
                egui::Frame::new()
                    .fill(Color32::from_rgb(28, 22, 26))
                    .stroke(Stroke::new(1.0_f32, col))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("REC ch{:02}_{}.wav", ch.index + 1, ch.name))
                                .small()
                                .monospace()
                                .color(Color32::WHITE),
                        );
                    });
            }
            if m.record_mix {
                egui::Frame::new()
                    .fill(Color32::from_rgb(30, 26, 20))
                    .stroke(Stroke::new(1.0_f32, WingColors::WING_ORANGE))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new("REC master_mix.wav")
                                .small()
                                .monospace()
                                .color(WingColors::WING_ORANGE),
                        );
                    });
            }
        });
    }

    ui.add_space(8.0);
    let vmic_active = ctx.mixer.is_virtual_mic_active();
    let vmic_border = if vmic_active {
        StudioPalette::ACCENT_EMERALD
    } else {
        Color32::from_rgb(52, 62, 82)
    };
    egui::Frame::new()
        .fill(Color32::from_rgb(15, 21, 30))
        .stroke(Stroke::new(1.2_f32, vmic_border))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            ui.label(
                RichText::new(t.mixer_vmic_title)
                    .small()
                    .strong()
                    .monospace()
                    .color(if vmic_active {
                        StudioPalette::ACCENT_EMERALD
                    } else {
                        StudioPalette::ACCENT_CYAN
                    }),
            );
            ui.label(
                RichText::new(t.mixer_vmic_desc)
                    .small()
                    .color(StudioPalette::TEXT_SECONDARY),
            );
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                let vmic_lbl = if vmic_active {
                    t.mixer_vmic_toggle_on
                } else {
                    t.mixer_vmic_toggle_off
                };
                if draw_console_switch(
                    ui,
                    vmic_lbl,
                    vmic_active,
                    StudioPalette::ACCENT_EMERALD,
                    Vec2::new(128.0, 24.0),
                ) {
                    toggle_virtual_mic(ctx);
                }

                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 26, 36))
                    .stroke(Stroke::new(1.0_f32, StudioPalette::BORDER_SUBTLE))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(6, 3))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(VIRTUAL_MIC_DISPLAY_NAME)
                                .small()
                                .monospace()
                                .color(if vmic_active {
                                    StudioPalette::ACCENT_EMERALD
                                } else {
                                    StudioPalette::TEXT_MUTED
                                }),
                        );
                    });

                let mut vmic_gain = ctx.mixer.master().virtual_mic_gain_db;
                ui.label(
                    RichText::new(t.mixer_vmic_gain)
                        .small()
                        .color(StudioPalette::TEXT_SECONDARY),
                );
                if ui
                    .add(
                        egui::DragValue::new(&mut vmic_gain)
                            .range(-24.0..=18.0)
                            .speed(0.2)
                            .suffix(" dB"),
                    )
                    .changed()
                {
                    ctx.mixer.set_virtual_mic_gain_db(vmic_gain);
                }
            });

            ui.add_space(4.0);
            let ch_len = ctx.mixer.channels().len();
            let mut ch_toggles: Vec<(usize, ChannelDspConfig)> = Vec::new();
            ui.horizontal_wrapped(|ui| {
                for ch_idx in 0..ch_len {
                    let ch = &ctx.mixer.channels()[ch_idx];
                    let mut send = ch.dsp.send_to_virtual_mic;
                    if ui
                        .checkbox(
                            &mut send,
                            RichText::new(format!("CH {:02} {}", ch_idx + 1, truncate_chars(&ch.name, 8)))
                                .small()
                                .monospace(),
                        )
                        .changed()
                    {
                        let mut next_dsp = ch.dsp;
                        next_dsp.send_to_virtual_mic = send;
                        ch_toggles.push((ch_idx, next_dsp));
                    }
                }
            });
            for (ch_idx, next_dsp) in ch_toggles {
                ctx.mixer.set_channel_dsp(ch_idx, next_dsp);
            }
        });
}

fn render_wing_monitor_deck(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let live = ctx.mixer.is_audio_running();
    let recording = ctx.mixer.is_recording();
    let vmic_active = ctx.mixer.is_virtual_mic_active();
    let armed_count = ctx
        .mixer
        .channels()
        .iter()
        .filter(|c| c.dsp.record_arm)
        .count();
    let outer_w = ui.available_width().max(180.0);
    ui.set_max_width(outer_w);

    egui::Frame::new()
        .fill(WingColors::DECK)
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 54, 68)))
        .corner_radius(CornerRadius::same(4))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            let deck_w = ui.available_width().max(160.0);
            ui.set_max_width(deck_w);
            ui.horizontal_top(|ui| {
                ui.vertical(|ui| {
                    let (pw_lbl, pw_col) = if live {
                        (t.mixer_power_on, StudioPalette::ACCENT_EMERALD)
                    } else {
                        (t.mixer_power_off, StudioPalette::ACCENT_BLUE)
                    };
                    if animated_action_button(
                        ui,
                        "wing_power_btn",
                        Some(if live { StudioIcon::Stop } else { StudioIcon::Play }),
                        pw_lbl,
                        pw_col,
                        live,
                        Vec2::new(112.0, 28.0),
                    )
                    .clicked()
                    {
                        if live {
                            ctx.mixer.stop_audio();
                            *ctx.status = StatusState::MonitoringStopped;
                            *ctx.error = None;
                        } else {
                            start_or_restart_mixer_audio(ctx);
                        }
                    }

                    ui.add_space(3.0);

                    let (rec_lbl, rec_col) = if recording {
                        (t.mixer_rec_stop, StudioPalette::ACCENT_CRIMSON)
                    } else {
                        (t.mixer_rec_start, Color32::from_rgb(185, 38, 48))
                    };
                    if animated_action_button(
                        ui,
                        "wing_multi_rec_btn",
                        Some(StudioIcon::Record),
                        rec_lbl,
                        rec_col,
                        recording,
                        Vec2::new(112.0, 28.0),
                    )
                    .on_hover_text(t.mixer_rec_only_armed_note)
                    .clicked()
                    {
                        toggle_mixer_recording(ctx);
                    }

                    ui.add_space(3.0);
                    let vmic_lbl = if vmic_active {
                        t.mixer_vmic_toggle_on
                    } else {
                        t.mixer_vmic_toggle_off
                    };
                    if draw_console_switch(
                        ui,
                        vmic_lbl,
                        vmic_active,
                        StudioPalette::ACCENT_EMERALD,
                        Vec2::new(112.0, 22.0),
                    ) {
                        toggle_virtual_mic(ctx);
                    }

                    ui.add_space(3.0);
                    let secs = ctx.state.meters.recorded_frames as f64
                        / ctx.mixer.sample_rate().max(1) as f64;
                    let mins = (secs / 60.0).floor() as u64;
                    let rem = secs % 60.0;
                    ui.label(
                        RichText::new(format!("REC {mins:02}:{rem:04.1}"))
                            .small()
                            .monospace()
                            .strong()
                            .color(if recording {
                                StudioPalette::ACCENT_CRIMSON
                            } else {
                                StudioPalette::TEXT_MUTED
                            }),
                    );
                    ui.label(
                        RichText::new(format!("{} {armed_count}", t.mixer_rec_armed_count))
                            .small()
                            .monospace()
                            .strong()
                            .color(if armed_count > 0 {
                                StudioPalette::ACCENT_CRIMSON
                            } else {
                                StudioPalette::ACCENT_AMBER
                            }),
                    );
                });

                ui.add_space(4.0);
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("MAIN")
                            .small()
                            .strong()
                            .monospace()
                            .color(WingColors::WING_ORANGE),
                    );
                    draw_wing_vertical_meter(
                        ui,
                        Vec2::new(26.0, 114.0),
                        ctx.state.meters.master_left,
                        ctx.state.meters.master_right,
                        ctx.state.peak_hold_master[0],
                        ctx.state.peak_hold_master[1],
                    );
                });
                ui.vertical(|ui| {
                    ui.label(
                        RichText::new("VMIC")
                            .small()
                            .strong()
                            .monospace()
                            .color(if vmic_active {
                                StudioPalette::ACCENT_EMERALD
                            } else {
                                StudioPalette::TEXT_MUTED
                            }),
                    );
                    draw_wing_vertical_meter(
                        ui,
                        Vec2::new(22.0, 114.0),
                        ctx.state.meters.virtual_mic_left,
                        ctx.state.meters.virtual_mic_right,
                        ctx.state.peak_hold_vmic[0],
                        ctx.state.peak_hold_vmic[1],
                    );
                });
            });

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(4.0);

            let m = ctx.mixer.master().clone();
            let mut fader_db = m.fader_db;
            let mut mute = m.mute;
            let mut dim = m.dim;
            let mut changed = false;

            ui.horizontal_wrapped(|ui| {
                if draw_rotary_knob(
                    ui,
                    "wing_master_monitor_knob",
                    "MASTER",
                    &mut fader_db,
                    -60.0..=10.0,
                    0.0,
                    "dB",
                    WingColors::WING_ORANGE,
                ) {
                    changed = true;
                }

                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    if draw_console_switch(ui, "DIM", dim, StudioPalette::ACCENT_AMBER, Vec2::new(64.0, 21.0)) {
                        dim = !dim;
                        changed = true;
                    }
                    if draw_console_switch(ui, "MUTE", mute, StudioPalette::ACCENT_CRIMSON, Vec2::new(64.0, 21.0)) {
                        mute = !mute;
                        changed = true;
                    }
                    let any_solo = ctx.mixer.channels().iter().any(|c| c.dsp.solo);
                    if draw_console_switch(
                        ui,
                        t.mixer_clear_solo,
                        any_solo,
                        StudioPalette::ACCENT_AMBER,
                        Vec2::new(64.0, 21.0),
                    ) && any_solo
                    {
                        ctx.mixer.clear_all_solos();
                    }
                });
            });
            if changed {
                ctx.mixer.set_master(fader_db, mute, dim, m.record_mix);
            }

            let media_ch_count = ctx.mixer.count_media_channels();
            if media_ch_count > 0 {
                ui.add_space(5.0);
                ui.separator();
                ui.add_space(4.0);
                render_global_playback_section(ui, ctx, deck_w);
            }
        });
}

fn render_global_playback_section(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>, width: f32) {
    let t = ctx.lang.text();
    let media_count = ctx.mixer.count_media_channels();
    let any_playing = ctx.mixer.is_any_media_playing();
    let out = (!ctx.state.selected_output.is_empty()).then_some(ctx.state.selected_output.as_str());

    let max_cur_secs = ctx
        .mixer
        .channels()
        .iter()
        .filter(|c| c.source.mode == ChannelInputMode::Multimedia)
        .map(|c| c.media.current_frame as f64 / c.media.sample_rate.max(1) as f64)
        .fold(0.0_f64, f64::max);

    let max_tot_secs = ctx
        .mixer
        .channels()
        .iter()
        .filter(|c| c.source.mode == ChannelInputMode::Multimedia)
        .map(|c| c.media.total_frames as f64 / c.media.sample_rate.max(1) as f64)
        .fold(0.0_f64, f64::max);

    ui.vertical(|ui| {
        ui.set_max_width(width);
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(t.mixer_playback_section_title)
                    .small()
                    .strong()
                    .monospace()
                    .color(WingColors::WING_ORANGE),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(
                    RichText::new(format!("{media_count} CH"))
                        .small()
                        .monospace()
                        .color(StudioPalette::ACCENT_CYAN),
                );
            });
        });

        let status_lbl = if any_playing {
            t.mixer_playback_sync_all
        } else {
            t.mixer_playback_stop_all
        };
        ui.label(
            RichText::new(status_lbl)
                .small()
                .color(StudioPalette::TEXT_MUTED),
        );

        ui.add_space(2.0);
        let time_txt = format!(
            "{} / {}",
            format_media_time(max_cur_secs),
            format_media_time(max_tot_secs)
        );
        ui.label(
            RichText::new(time_txt)
                .small()
                .monospace()
                .strong()
                .color(if any_playing {
                    StudioPalette::ACCENT_EMERALD
                } else {
                    StudioPalette::TEXT_SECONDARY
                }),
        );

        ui.add_space(3.0);
        let play_all_lbl = if any_playing {
            t.mixer_playback_pause_all
        } else {
            t.mixer_playback_play_all
        };
        if draw_console_switch(
            ui,
            play_all_lbl,
            any_playing,
            StudioPalette::ACCENT_EMERALD,
            Vec2::new(width, 24.0),
        ) {
            if !ctx.mixer.is_audio_running() && !any_playing {
                (ctx.stop_classic_audio)();
            }
            let _ = ctx.mixer.set_all_media_transport(!any_playing, None, None, out);
        }

        ui.add_space(3.0);
        let sub_w = ((width - 6.0) / 4.0).floor().max(20.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if draw_console_switch(
                ui,
                "-10",
                false,
                Color32::from_rgb(70, 82, 104),
                Vec2::new(sub_w, 20.0),
            ) {
                let _ = ctx.mixer.seek_all_media_relative_seconds(-10.0, out);
            }
            if draw_console_switch(
                ui,
                "+10",
                false,
                Color32::from_rgb(70, 82, 104),
                Vec2::new(sub_w, 20.0),
            ) {
                let _ = ctx.mixer.seek_all_media_relative_seconds(10.0, out);
            }
            if draw_console_switch(
                ui,
                "[]",
                false,
                Color32::from_rgb(80, 90, 110),
                Vec2::new(sub_w, 20.0),
            ) {
                let _ = ctx.mixer.set_all_media_transport(false, None, Some(0), out);
            }
            let any_looping = ctx
                .mixer
                .channels()
                .iter()
                .any(|c| c.source.mode == ChannelInputMode::Multimedia && c.media.looping);
            if draw_console_switch(
                ui,
                "LP",
                any_looping,
                WingColors::WING_ORANGE,
                Vec2::new(sub_w, 20.0),
            ) {
                let next_loop = !any_looping;
                let _ = ctx.mixer.set_all_media_transport(any_playing, Some(next_loop), None, out);
            }
        });
    });
}

fn render_fader_bank(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let count = ctx.mixer.channels().len();
    let armed_count = ctx
        .mixer
        .channels()
        .iter()
        .filter(|c| c.dsp.record_arm)
        .count();
    let outer_w = ui.available_width().max(220.0);
    ui.set_max_width(outer_w);

    egui::Frame::new()
        .fill(Color32::from_rgb(13, 15, 20))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(42, 47, 58)))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(8))
        .show(ui, |ui| {
            let bank_inner_w = ui.available_width().max(200.0);
            ui.set_max_width(bank_inner_w);
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new(t.mixer_bank_title)
                        .small()
                        .strong()
                        .monospace()
                        .color(WingColors::WING_ORANGE),
                );
                egui::Frame::new()
                    .fill(Color32::from_rgb(20, 25, 36))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 62, 82)))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{count}/{MAX_MIXER_CHANNELS}"))
                                .small()
                                .monospace()
                                .color(StudioPalette::ACCENT_CYAN),
                        );
                    });
                egui::Frame::new()
                    .fill(if armed_count > 0 {
                        Color32::from_rgb(54, 18, 24)
                    } else {
                        Color32::from_rgb(26, 24, 22)
                    })
                    .stroke(Stroke::new(
                        1.0_f32,
                        if armed_count > 0 {
                            StudioPalette::ACCENT_CRIMSON
                        } else {
                            StudioPalette::ACCENT_AMBER
                        },
                    ))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{} {armed_count}", t.mixer_rec_armed_count))
                                .small()
                                .strong()
                                .monospace()
                                .color(Color32::WHITE),
                        );
                    });

                egui::Frame::new()
                    .fill(Color32::from_rgb(28, 34, 46))
                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(60, 72, 94)))
                    .corner_radius(CornerRadius::same(2))
                    .inner_margin(Margin::symmetric(6, 2))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("CAPA: {}", ctx.state.active_bank.label(&ctx.lang.text())))
                                .small()
                                .strong()
                                .monospace()
                                .color(WingColors::WING_ORANGE),
                        );
                    });

                if ui
                    .add_enabled(
                        count < MAX_MIXER_CHANNELS,
                        egui::Button::new(
                            RichText::new(t.mixer_add_channel)
                                .small()
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(42, 44, 52))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(70, 74, 84)))
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    add_channel_in_current_bank(ctx, false);
                }
                if ui
                    .add_enabled(
                        count < MAX_MIXER_CHANNELS,
                        egui::Button::new(
                            RichText::new(t.mixer_add_media)
                                .small()
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(52, 54, 64))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(90, 94, 105)))
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    add_channel_in_current_bank(ctx, true);
                }
                if count > 1 && ui.small_button(t.mixer_remove_channel).clicked() {
                    let target_idx = match ctx.state.selection {
                        WingSelection::Channel(i) if i < count => i,
                        _ => count - 1,
                    };
                    if ctx.mixer.remove_channel(target_idx) {
                        let next_count = ctx.mixer.channels().len();
                        if next_count > 0 {
                            ctx.state.selection =
                                WingSelection::Channel(target_idx.min(next_count - 1));
                        }
                    }
                }

                if ui.small_button("◀").clicked() {
                    let prev = (ctx.state.last_fader_scroll_x - 260.0).max(0.0);
                    ctx.state.pending_fader_scroll_x = Some(prev);
                }
                if ui.small_button("▶").clicked() {
                    let next = (ctx.state.last_fader_scroll_x + 260.0)
                        .min(ctx.state.last_fader_max_scroll_x);
                    ctx.state.pending_fader_scroll_x = Some(next);
                }
            });

            ui.add_space(6.0);
            let avail_w = ui.available_width();
            let bank_selector_w = 84.0_f32;
            let pin_master = avail_w >= 540.0;

            if pin_master {
                let master_w = 94.0_f32;
                let sep_w = 14.0_f32;
                let bank_w = (avail_w - bank_selector_w - master_w - sep_w).max(160.0).floor();
                ui.horizontal_top(|ui| {
                    render_channel_bank_selector(ui, ctx, bank_selector_w);
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        Vec2::new(bank_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(bank_w);
                            render_scrollable_channels(ui, ctx, false, bank_w);
                        },
                    );
                    ui.separator();
                    render_wing_master_strip(ui, ctx);
                });
            } else {
                let sep_w = 8.0_f32;
                let bank_w = (avail_w - bank_selector_w - sep_w).max(160.0).floor();
                ui.horizontal_top(|ui| {
                    render_channel_bank_selector(ui, ctx, bank_selector_w);
                    ui.separator();
                    ui.allocate_ui_with_layout(
                        Vec2::new(bank_w, 0.0),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_max_width(bank_w);
                            render_scrollable_channels(ui, ctx, true, bank_w);
                        },
                    );
                });
            }
        });
}

fn render_channel_bank_selector(
    ui: &mut egui::Ui,
    ctx: &mut MixerPanelContext<'_>,
    width: f32,
) {
    let t = ctx.lang.text();
    let channels = ctx.mixer.channels();
    let count_ch1_16 = channels
        .iter()
        .filter(|c| c.source.mode != ChannelInputMode::Multimedia && c.index < 16)
        .count();
    let count_ch17_32 = channels
        .iter()
        .filter(|c| c.source.mode != ChannelInputMode::Multimedia && (16..32).contains(&c.index))
        .count();
    let count_ch33_48 = channels
        .iter()
        .filter(|c| c.source.mode != ChannelInputMode::Multimedia && (32..48).contains(&c.index))
        .count();
    let count_media = channels
        .iter()
        .filter(|c| c.source.mode == ChannelInputMode::Multimedia)
        .count();
    let count_all = channels.len();

    let sections = [
        (
            ChannelBankSection::Ch1_16,
            t.mixer_bank_ch1_16,
            count_ch1_16,
            Color32::from_rgb(220, 224, 232),
        ),
        (
            ChannelBankSection::Ch17_32,
            t.mixer_bank_ch17_32,
            count_ch17_32,
            Color32::from_rgb(190, 195, 205),
        ),
        (
            ChannelBankSection::Ch33_48,
            t.mixer_bank_ch33_48,
            count_ch33_48,
            Color32::from_rgb(160, 165, 175),
        ),
        (
            ChannelBankSection::Multimedia,
            t.mixer_bank_multimedia,
            count_media,
            Color32::from_rgb(240, 242, 248),
        ),
        (
            ChannelBankSection::All,
            t.mixer_bank_all,
            count_all,
            Color32::WHITE,
        ),
    ];

    egui::Frame::new()
        .fill(Color32::from_rgb(16, 18, 24))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 50, 64)))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(4))
        .show(ui, |ui| {
            ui.set_min_width(width);
            ui.set_max_width(width);
            ui.vertical(|ui| {
                ui.add_space(2.0);
                ui.vertical_centered(|ui| {
                    ui.label(
                        RichText::new(t.mixer_bank_deck_title)
                            .small()
                            .strong()
                            .monospace()
                            .color(WingColors::WING_ORANGE),
                    );
                });
                ui.add_space(4.0);

                for (section, label, count, accent) in sections {
                    let is_active = ctx.state.active_bank == section;
                    let (btn_rect_alloc, btn_resp) = ui.allocate_exact_size(
                        Vec2::new(width - 8.0, 36.0),
                        Sense::click(),
                    );

                    if btn_resp.clicked() {
                        ctx.state.active_bank = section;
                        ctx.state.pending_fader_scroll_x = Some(0.0);
                        let is_media = |c: &MixerChannelState| c.source.mode == ChannelInputMode::Multimedia;
                        let first_matching = ctx.mixer.channels().iter().enumerate().find(|(idx, c)| {
                            section.matches(is_media(c), *idx)
                        }).map(|(idx, _)| idx);
                        if let Some(target) = first_matching {
                            ctx.state.selection = WingSelection::Channel(target);
                        }
                    }

                    if btn_resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }

                    let painter = ui.painter();
                    let socket_rect = btn_rect_alloc.expand(0.8);
                    painter.rect_filled(socket_rect, CornerRadius::same(3), Color32::from_rgb(7, 7, 9));
                    painter.rect_stroke(
                        socket_rect,
                        CornerRadius::same(3),
                        Stroke::new(1.0_f32, Color32::from_rgb(18, 19, 24)),
                        StrokeKind::Inside,
                    );

                    let pressed_offset = if btn_resp.is_pointer_button_down_on() { 0.7 } else { 0.0 };
                    let btn_rect = btn_rect_alloc.translate(Vec2::new(0.0, pressed_offset));

                    let bg_col = if is_active {
                        Color32::from_rgb(32, 34, 42)
                    } else if btn_resp.hovered() {
                        Color32::from_rgb(26, 28, 36)
                    } else {
                        Color32::from_rgb(20, 21, 26)
                    };

                    let border_col = if is_active {
                        accent
                    } else if btn_resp.hovered() {
                        Color32::from_rgb(70, 75, 90)
                    } else {
                        Color32::from_rgb(38, 40, 50)
                    };

                    painter.rect_filled(btn_rect, CornerRadius::same(3), bg_col);

                    if is_active {
                        let core_rect = Rect::from_center_size(
                            btn_rect.center(),
                            Vec2::new(btn_rect.width() * 0.72, btn_rect.height() * 0.58),
                        );
                        painter.rect_filled(
                            core_rect,
                            CornerRadius::same(2),
                            Color32::from_rgba_unmultiplied(255, 255, 255, 30),
                        );
                    }

                    let rim_alpha = if is_active { 120 } else { 32 };
                    painter.line_segment(
                        [
                            Pos2::new(btn_rect.left() + 2.0, btn_rect.top() + 1.0),
                            Pos2::new(btn_rect.right() - 2.0, btn_rect.top() + 1.0),
                        ],
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, rim_alpha)),
                    );
                    painter.line_segment(
                        [
                            Pos2::new(btn_rect.left() + 2.0, btn_rect.bottom() - 1.0),
                            Pos2::new(btn_rect.right() - 2.0, btn_rect.bottom() - 1.0),
                        ],
                        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 85)),
                    );

                    painter.rect_stroke(
                        btn_rect,
                        CornerRadius::same(3),
                        Stroke::new(if is_active { 1.5_f32 } else { 1.0_f32 }, border_col),
                        StrokeKind::Inside,
                    );

                    let led_rect = Rect::from_min_size(
                        Pos2::new(btn_rect.left() + 3.0, btn_rect.top() + 2.0),
                        Vec2::new(btn_rect.width() - 6.0, 3.0),
                    );
                    let led_col = if is_active {
                        accent
                    } else {
                        Color32::from_rgb(28, 30, 38)
                    };
                    painter.rect_filled(led_rect, CornerRadius::same(1), led_col);

                    let text_col = if is_active {
                        Color32::WHITE
                    } else if btn_resp.hovered() {
                        Color32::from_rgb(220, 226, 238)
                    } else {
                        Color32::from_rgb(150, 155, 168)
                    };

                    painter.text(
                        Pos2::new(btn_rect.center().x, btn_rect.top() + 14.0),
                        Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(11.0),
                        text_col,
                    );

                    let badge_text = format!("{count}");
                    let badge_col = if count > 0 {
                        if is_active { accent } else { Color32::from_rgb(180, 186, 198) }
                    } else {
                        Color32::from_rgb(85, 90, 102)
                    };
                    painter.text(
                        Pos2::new(btn_rect.center().x, btn_rect.top() + 26.0),
                        Align2::CENTER_CENTER,
                        badge_text,
                        FontId::monospace(10.0),
                        badge_col,
                    );

                    ui.add_space(3.0);
                }

                if ctx.state.active_bank == ChannelBankSection::Multimedia {
                    ui.add_space(5.0);
                    ui.separator();
                    ui.add_space(4.0);
                    render_bank_multimedia_toolbar(ui, ctx, width - 8.0);
                }
            });
        });
}

fn render_bank_multimedia_toolbar(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>, width: f32) {
    let any_playing = ctx.mixer.is_any_media_playing();
    let media_count = ctx.mixer.count_media_channels();
    let out = (!ctx.state.selected_output.is_empty()).then_some(ctx.state.selected_output.as_str());

    let (target_ch, cur_time_str) = match ctx.state.selection {
        WingSelection::Channel(idx)
            if idx < ctx.mixer.channels().len()
                && ctx.mixer.channels()[idx].source.mode == ChannelInputMode::Multimedia =>
        {
            let ch = &ctx.mixer.channels()[idx];
            let cur_s = ch.media.current_frame as f64 / ch.media.sample_rate.max(1) as f64;
            (Some(idx), format_media_time(cur_s))
        }
        _ => {
            let first_media = ctx
                .mixer
                .channels()
                .iter()
                .enumerate()
                .find(|(_, c)| c.source.mode == ChannelInputMode::Multimedia);
            if let Some((idx, ch)) = first_media {
                let cur_s = ch.media.current_frame as f64 / ch.media.sample_rate.max(1) as f64;
                (Some(idx), format_media_time(cur_s))
            } else {
                (None, "00:00".to_string())
            }
        }
    };

    ui.vertical(|ui| {
        ui.set_max_width(width);
        ui.vertical_centered(|ui| {
            ui.label(
                RichText::new("TOOLBAR")
                    .small()
                    .monospace()
                    .strong()
                    .color(StudioPalette::ACCENT_CYAN),
            );
            ui.label(
                RichText::new(&cur_time_str)
                    .small()
                    .monospace()
                    .color(if any_playing {
                        StudioPalette::ACCENT_EMERALD
                    } else {
                        StudioPalette::TEXT_SECONDARY
                    }),
            );
        });

        ui.add_space(3.0);
        let btn_w = ((width - 4.0) / 3.0).floor().max(22.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if draw_console_switch(
                ui,
                "-10",
                false,
                Color32::from_rgb(70, 82, 104),
                Vec2::new(btn_w, 20.0),
            ) {
                if let Some(ch) = target_ch {
                    let _ = ctx.mixer.seek_media_relative_seconds(ch, -10.0, out);
                } else {
                    let _ = ctx.mixer.seek_all_media_relative_seconds(-10.0, out);
                }
            }

            let play_sym = if any_playing { "||" } else { ">" };
            if draw_console_switch(
                ui,
                play_sym,
                any_playing,
                StudioPalette::ACCENT_EMERALD,
                Vec2::new(btn_w, 20.0),
            ) {
                if !ctx.mixer.is_audio_running() && !any_playing {
                    (ctx.stop_classic_audio)();
                }
                let next_play = !any_playing;
                if let Some(ch) = target_ch {
                    let looping = ctx.mixer.channels()[ch].media.looping;
                    let _ = ctx.mixer.set_media_transport(ch, next_play, looping, None, out);
                } else {
                    let _ = ctx.mixer.set_all_media_transport(next_play, None, None, out);
                }
            }

            if draw_console_switch(
                ui,
                "+10",
                false,
                Color32::from_rgb(70, 82, 104),
                Vec2::new(btn_w, 20.0),
            ) {
                if let Some(ch) = target_ch {
                    let _ = ctx.mixer.seek_media_relative_seconds(ch, 10.0, out);
                } else {
                    let _ = ctx.mixer.seek_all_media_relative_seconds(10.0, out);
                }
            }
        });

        ui.add_space(2.0);
        let row2_btn_w = ((width - 2.0) * 0.5).floor().max(32.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            if draw_console_switch(
                ui,
                "[]",
                false,
                Color32::from_rgb(80, 90, 110),
                Vec2::new(row2_btn_w, 19.0),
            ) {
                if let Some(ch) = target_ch {
                    let looping = ctx.mixer.channels()[ch].media.looping;
                    let _ = ctx.mixer.set_media_transport(ch, false, looping, Some(0), out);
                } else {
                    let _ = ctx.mixer.set_all_media_transport(false, None, Some(0), out);
                }
            }

            let loop_active = target_ch
                .map(|ch| ctx.mixer.channels()[ch].media.looping)
                .unwrap_or(false);
            if draw_console_switch(
                ui,
                "LP",
                loop_active,
                WingColors::WING_ORANGE,
                Vec2::new(row2_btn_w, 19.0),
            ) {
                if let Some(ch) = target_ch {
                    let playing = ctx.mixer.channels()[ch].media.playing;
                    let _ = ctx.mixer.set_media_transport(ch, playing, !loop_active, None, out);
                }
            }
        });

        if media_count > 1 {
            ui.add_space(2.0);
            let all_lbl = if any_playing { "PAUSE ALL" } else { "PLAY ALL" };
            if draw_console_switch(
                ui,
                all_lbl,
                any_playing,
                StudioPalette::ACCENT_CYAN,
                Vec2::new(width, 21.0),
            ) {
                if !ctx.mixer.is_audio_running() && !any_playing {
                    (ctx.stop_classic_audio)();
                }
                let _ = ctx.mixer.set_all_media_transport(!any_playing, None, None, out);
            }
        }
    });
}

fn render_empty_bank_card(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let bank = ctx.state.active_bank;
    let count = ctx.mixer.channels().len();

    egui::Frame::new()
        .fill(Color32::from_rgb(18, 22, 30))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(46, 54, 70)))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            ui.set_width(190.0);
            ui.vertical_centered(|ui| {
                ui.add_space(14.0);
                ui.label(
                    RichText::new(bank.label(&ctx.lang.text()))
                        .strong()
                        .monospace()
                        .color(WingColors::WING_ORANGE),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t.mixer_bank_empty_title)
                        .small()
                        .strong()
                        .color(Color32::WHITE),
                );
                ui.add_space(4.0);
                ui.label(
                    RichText::new(t.mixer_bank_empty_desc)
                        .small()
                        .color(Color32::from_rgb(140, 150, 168)),
                );
                ui.add_space(14.0);

                match bank {
                    ChannelBankSection::Multimedia => {
                        if ui
                            .add_sized(
                                Vec2::new(150.0, 32.0),
                                egui::Button::new(
                                    RichText::new(t.mixer_add_media)
                                        .small()
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(52, 54, 64))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(90, 94, 105)))
                                .corner_radius(CornerRadius::same(2)),
                            )
                            .clicked()
                        {
                            add_channel_and_focus(ctx, true);
                        }
                    }
                    ChannelBankSection::Ch17_32 => {
                        let btn_label = if count < 16 {
                            t.mixer_bank_enable_section
                        } else {
                            t.mixer_bank_add_channel_here
                        };
                        if ui
                            .add_sized(
                                Vec2::new(150.0, 32.0),
                                egui::Button::new(
                                    RichText::new(btn_label)
                                        .small()
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(46, 48, 56))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(76, 80, 92)))
                                .corner_radius(CornerRadius::same(2)),
                            )
                            .clicked()
                        {
                            add_channel_in_current_bank(ctx, false);
                        }
                    }
                    ChannelBankSection::Ch33_48 => {
                        let btn_label = if count < 32 {
                            t.mixer_bank_enable_section
                        } else {
                            t.mixer_bank_add_channel_here
                        };
                        if ui
                            .add_sized(
                                Vec2::new(150.0, 32.0),
                                egui::Button::new(
                                    RichText::new(btn_label)
                                        .small()
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(46, 48, 56))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(76, 80, 92)))
                                .corner_radius(CornerRadius::same(2)),
                            )
                            .clicked()
                        {
                            add_channel_in_current_bank(ctx, false);
                        }
                    }
                    _ => {
                        if ui
                            .add_sized(
                                Vec2::new(150.0, 32.0),
                                egui::Button::new(
                                    RichText::new(t.mixer_add_channel)
                                        .small()
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(42, 44, 52))
                                .stroke(Stroke::new(1.0_f32, Color32::from_rgb(70, 74, 84)))
                                .corner_radius(CornerRadius::same(2)),
                            )
                            .clicked()
                        {
                            add_channel_in_current_bank(ctx, false);
                        }
                    }
                }
                ui.add_space(10.0);
            });
        });
}

fn render_scrollable_channels(
    ui: &mut egui::Ui,
    ctx: &mut MixerPanelContext<'_>,
    include_master: bool,
    max_w: f32,
) {
    let channels = ctx.mixer.channels();
    let is_media = |c: &MixerChannelState| c.source.mode == ChannelInputMode::Multimedia;
    let matching_indices: Vec<usize> = channels
        .iter()
        .enumerate()
        .filter(|(idx, c)| ctx.state.active_bank.matches(is_media(c), *idx))
        .map(|(idx, _)| idx)
        .collect();

    let mut h_area = egui::ScrollArea::horizontal()
        .id_salt("wing_fader_bank_hscroll")
        .max_width(max_w)
        .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
        .auto_shrink([false, true]);
    if let Some(target_x) = ctx.state.pending_fader_scroll_x.take() {
        h_area = h_area.horizontal_scroll_offset(target_x.max(0.0));
    }
    let out = h_area.show(ui, |ui| {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if matching_indices.is_empty() {
                render_empty_bank_card(ui, ctx);
            } else {
                for &idx in &matching_indices {
                    render_wing_channel_strip(ui, ctx, idx);
                }
            }
            render_inline_add_strip_card(ui, ctx);
            if include_master {
                ui.add_space(4.0);
                ui.separator();
                ui.add_space(4.0);
                render_wing_master_strip(ui, ctx);
            }
        });
    });
    ctx.state.last_fader_scroll_x = out.state.offset.x;
    ctx.state.last_fader_max_scroll_x = (out.content_size.x - out.inner_rect.width()).max(0.0);
}

fn render_inline_add_strip_card(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let t = ctx.lang.text();
    let count = ctx.mixer.channels().len();
    if count >= MAX_MIXER_CHANNELS {
        return;
    }
    egui::Frame::new()
        .fill(Color32::from_rgb(16, 19, 25))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 52, 66)))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(6))
        .show(ui, |ui| {
            ui.set_width(78.0);
            ui.vertical_centered(|ui| {
                ui.add_space(10.0);
                ui.label(
                    RichText::new(format!("{count}/{MAX_MIXER_CHANNELS}"))
                        .small()
                        .monospace()
                        .color(StudioPalette::TEXT_MUTED),
                );
                ui.add_space(12.0);
                if ui
                    .add_sized(
                        Vec2::new(72.0, 38.0),
                        egui::Button::new(
                            RichText::new(t.mixer_add_channel)
                                .small()
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(42, 44, 52))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(70, 74, 84)))
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    add_channel_in_current_bank(ctx, false);
                }
                ui.add_space(10.0);
                if ui
                    .add_sized(
                        Vec2::new(72.0, 44.0),
                        egui::Button::new(
                            RichText::new(t.mixer_add_media)
                                .small()
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(52, 54, 64))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(90, 94, 105)))
                        .corner_radius(CornerRadius::same(2)),
                    )
                    .clicked()
                {
                    add_channel_in_current_bank(ctx, true);
                }
                ui.add_space(10.0);
            });
        });
}

fn render_wing_channel_strip(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>, idx: usize) {
    let t = ctx.lang.text();
    let ch = ctx.mixer.channels()[idx].clone();
    let ch_count = ctx.mixer.channels().len();
    let is_media = ch.source.mode == ChannelInputMode::Multimedia;
    let selected = ctx.state.selection == WingSelection::Channel(idx);
    let accent = if is_media {
        StudioPalette::ACCENT_CYAN
    } else {
        Color32::from_rgb(ch.color[0], ch.color[1], ch.color[2])
    };
    let strip_accent = Color32::from_rgb(ch.color[0], ch.color[1], ch.color[2]);
    let meter = ctx.state.meters.channels.get(idx).copied().unwrap_or_default();
    let hold = ctx
        .state
        .peak_hold_channels
        .get(idx)
        .copied()
        .unwrap_or([0.0, 0.0]);
    let vst_count = ch.inserts.iter().filter(|s| s.meta.is_some()).count();

    let mut dsp = ch.dsp;
    let mut dsp_changed = false;
    let mut pending_input: Option<ChannelInputSource> = None;
    let mut pending_color: Option<[u8; 3]> = None;
    let mut pending_icon: Option<ChannelIcon> = None;
    let mut pending_pick_custom_icon = false;
    let mut pending_load_media = false;
    let mut pending_remove = false;

    let ch_lum = ch.color[0] as f32 * 0.299 + ch.color[1] as f32 * 0.587 + ch.color[2] as f32 * 0.114;
    let frame_bg = if selected {
        WingColors::STRIP_SELECTED
    } else {
        WingColors::STRIP_BG
    };
    let border_col = if selected {
        if ch_lum < 50.0 {
            WingColors::WING_ORANGE
        } else {
            strip_accent
        }
    } else {
        Color32::from_rgb(40, 46, 58)
    };

    let frame_resp = egui::Frame::new()
        .fill(frame_bg)
        .stroke(Stroke::new(
            if selected { 1.5_f32 } else { 1.0_f32 },
            border_col,
        ))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(5))
        .show(ui, |ui| {
            ui.set_min_width(82.0);
            ui.set_max_width(82.0);
            ui.vertical(|ui| {
                let (scr_rect, scr_resp) =
                    ui.allocate_exact_size(Vec2::new(82.0, 48.0), Sense::click());
                if scr_resp.clicked() {
                    ctx.state.selection = WingSelection::Channel(idx);
                }
                if scr_resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                }
                scr_resp.context_menu(|ui| {
                    show_channel_context_menu(
                        ui,
                        ctx.lang,
                        &ch,
                        ch_count,
                        ctx.input_devices,
                        &mut pending_input,
                        &mut pending_color,
                        &mut pending_icon,
                        &mut pending_pick_custom_icon,
                        &mut pending_load_media,
                        &mut pending_remove,
                        &mut dsp,
                        &mut dsp_changed,
                        &mut ctx.state.selection,
                        &mut ctx.state.active_tab,
                    );
                });

                {
                    let p = ui.painter();
                    let socket_rect = scr_rect.expand(1.2);
                    p.rect_filled(socket_rect, CornerRadius::same(3), Color32::from_rgb(7, 7, 9));
                    p.rect_stroke(
                        socket_rect,
                        CornerRadius::same(3),
                        Stroke::new(1.0_f32, Color32::from_rgb(26, 28, 34)),
                        StrokeKind::Inside,
                    );

                    p.rect_filled(scr_rect, CornerRadius::same(2), Color32::from_rgb(10, 11, 14));
                    p.rect_filled(
                        Rect::from_min_size(scr_rect.min, Vec2::new(scr_rect.width(), 14.0)),
                        CornerRadius::same(1),
                        strip_accent,
                    );
                    let stroke_col = if ch_lum < 50.0 {
                        Color32::from_rgb(46, 50, 62)
                    } else {
                        strip_accent
                    };
                    p.rect_stroke(
                        scr_rect,
                        CornerRadius::same(2),
                        Stroke::new(1.0_f32, stroke_col),
                        StrokeKind::Inside,
                    );
                    let top_txt = format!("{:02} {}", idx + 1, truncate_chars(&ch.name, 7));
                    let top_txt_col = if ch_lum > 130.0 {
                        Color32::from_rgb(12, 14, 20)
                    } else {
                        Color32::from_rgb(230, 235, 245)
                    };
                    p.text(
                        Pos2::new(scr_rect.center().x, scr_rect.top() + 7.0),
                        Align2::CENTER_CENTER,
                        top_txt,
                        FontId::monospace(9.5),
                        top_txt_col,
                    );
                }

                let (src_lbl, src_col) = match ch.source.mode {
                    ChannelInputMode::Unassigned => {
                        (t.mixer_unassigned.to_string(), StudioPalette::TEXT_MUTED)
                    }
                    ChannelInputMode::Multimedia => {
                        if ch.media.loading {
                            ("LOAD..".to_string(), StudioPalette::ACCENT_AMBER)
                        } else if ch.media.total_frames > 0 {
                            let cur_s = ch.media.current_frame as f64
                                / ch.media.sample_rate.max(1) as f64;
                            let icon = if ch.media.playing { ">" } else { "||" };
                            (
                                format!("{}{}", icon, format_media_time(cur_s)),
                                StudioPalette::ACCENT_CYAN,
                            )
                        } else {
                            ("MEDIA".to_string(), StudioPalette::ACCENT_CYAN)
                        }
                    }
                    mode => (
                        format!(
                            "{} {}",
                            truncate_chars(&ch.source.device_label, 5),
                            format_input_mode_short(mode)
                        ),
                        accent,
                    ),
                };

                if ch.icon != ChannelIcon::None {
                    let ic_rect = Rect::from_min_size(
                        Pos2::new(scr_rect.left() + 4.0, scr_rect.top() + 19.0),
                        Vec2::new(22.0, 22.0),
                    );
                    let icon_tint = if ch_lum < 50.0 {
                        Color32::from_rgb(190, 196, 210)
                    } else {
                        strip_accent
                    };
                    draw_channel_icon(ui, ctx.state, ic_rect, &ch.icon, icon_tint);
                    ui.painter().text(
                        Pos2::new(scr_rect.left() + 54.0, scr_rect.top() + 30.0),
                        Align2::CENTER_CENTER,
                        truncate_chars(&src_lbl, 7),
                        FontId::monospace(8.5),
                        src_col,
                    );
                } else {
                    ui.painter().text(
                        Pos2::new(scr_rect.center().x, scr_rect.top() + 30.0),
                        Align2::CENTER_CENTER,
                        src_lbl,
                        FontId::monospace(9.0),
                        src_col,
                    );
                }

                paint_glass_scribble_cover(&ui.painter(), scr_rect);

                ui.add_space(3.0);
                if is_media {
                    let has_track = ch.media.total_frames > 0 && !ch.media.loading;
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if draw_console_switch(
                            ui,
                            "-10",
                            false,
                            Color32::from_rgb(70, 82, 104),
                            Vec2::new(26.0, 18.0),
                        ) && has_track
                        {
                            let out = (!ctx.state.selected_output.is_empty())
                                .then_some(ctx.state.selected_output.as_str());
                            let _ = ctx.mixer.seek_media_relative_seconds(idx, -10.0, out);
                        }
                        let play_lbl = if ch.media.playing { "||" } else { ">" };
                        if draw_console_switch(
                            ui,
                            play_lbl,
                            ch.media.playing,
                            StudioPalette::ACCENT_EMERALD,
                            Vec2::new(26.0, 18.0),
                        ) && has_track
                        {
                            if !ctx.mixer.is_audio_running() && !ch.media.playing {
                                (ctx.stop_classic_audio)();
                            }
                            let out = (!ctx.state.selected_output.is_empty())
                                .then_some(ctx.state.selected_output.as_str());
                            let _ = ctx.mixer.set_media_transport(
                                idx,
                                !ch.media.playing,
                                ch.media.looping,
                                if !ch.media.playing
                                    && ch.media.current_frame >= ch.media.total_frames
                                {
                                    Some(0)
                                } else {
                                    None
                                },
                                out,
                            );
                        }
                        if draw_console_switch(
                            ui,
                            "+10",
                            false,
                            Color32::from_rgb(70, 82, 104),
                            Vec2::new(26.0, 18.0),
                        ) && has_track
                        {
                            let out = (!ctx.state.selected_output.is_empty())
                                .then_some(ctx.state.selected_output.as_str());
                            let _ = ctx.mixer.seek_media_relative_seconds(idx, 10.0, out);
                        }
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 2.0;
                        if draw_console_switch(
                            ui,
                            "[]",
                            false,
                            Color32::from_rgb(80, 90, 110),
                            Vec2::new(26.0, 18.0),
                        ) && has_track
                        {
                            let out = (!ctx.state.selected_output.is_empty())
                                .then_some(ctx.state.selected_output.as_str());
                            let _ = ctx.mixer.set_media_transport(
                                idx,
                                false,
                                ch.media.looping,
                                Some(0),
                                out,
                            );
                        }
                        if draw_console_switch(
                            ui,
                            "LP",
                            ch.media.looping,
                            WingColors::WING_ORANGE,
                            Vec2::new(26.0, 18.0),
                        ) {
                            let out = (!ctx.state.selected_output.is_empty())
                                .then_some(ctx.state.selected_output.as_str());
                            let _ = ctx.mixer.set_media_transport(
                                idx,
                                ch.media.playing,
                                !ch.media.looping,
                                None,
                                out,
                            );
                        }
                        if draw_console_switch(
                            ui,
                            "...",
                            ch.media.total_frames == 0,
                            StudioPalette::ACCENT_CYAN,
                            Vec2::new(26.0, 18.0),
                        ) {
                            pending_load_media = true;
                        }
                    });
                } else if draw_console_switch(
                    ui,
                    if ch.source.mode == ChannelInputMode::Unassigned {
                        "IN (R-CLICK)"
                    } else {
                        "AUDIO IN"
                    },
                    ch.source.mode != ChannelInputMode::Unassigned,
                    StudioPalette::ACCENT_CYAN,
                    Vec2::new(82.0, 19.0),
                ) {
                    ctx.state.selection = WingSelection::Channel(idx);
                    ctx.state.active_tab = WingScreenTab::Source;
                }

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    let fx_col = if vst_count > 0 && !dsp.fx_bypass {
                        StudioPalette::ACCENT_PURPLE
                    } else {
                        Color32::from_rgb(56, 62, 76)
                    };
                    if draw_console_switch(
                        ui,
                        &format!("FX:{vst_count}"),
                        vst_count > 0 && !dsp.fx_bypass,
                        fx_col,
                        Vec2::new(40.0, 18.0),
                    ) {
                        ctx.state.selection = WingSelection::Channel(idx);
                        ctx.state.active_tab = WingScreenTab::Vst;
                    }
                    let eq_on = dsp.eq_enabled
                        && (dsp.hpf_enabled
                            || dsp.lpf_enabled
                            || dsp.eq_bands.iter().any(|b| b.enabled && b.gain_db.abs() >= 0.05));
                    let dyn_col = if dsp.comp_enabled || dsp.gate_enabled {
                        WingColors::WING_ORANGE
                    } else {
                        Color32::from_rgb(52, 58, 70)
                    };
                    if draw_console_switch(
                        ui,
                        "DSP",
                        eq_on || dsp.comp_enabled || dsp.gate_enabled,
                        dyn_col,
                        Vec2::new(40.0, 18.0),
                    ) {
                        ctx.state.selection = WingSelection::Channel(idx);
                        ctx.state.active_tab = WingScreenTab::Eq;
                    }
                });

                ui.add_space(3.0);
                let rec_lbl = if dsp.record_arm { "REC ● ON" } else { "REC" };
                let rec_row = ui.horizontal(|ui| {
                    draw_console_switch(
                        ui,
                        rec_lbl,
                        dsp.record_arm,
                        StudioPalette::ACCENT_CRIMSON,
                        Vec2::new(82.0, 21.0),
                    )
                });
                let rec_clicked = rec_row.inner;
                rec_row.response.on_hover_text(t.mixer_rec_tooltip);
                if rec_clicked {
                    dsp.record_arm = !dsp.record_arm;
                    dsp_changed = true;
                }

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if draw_console_switch(
                        ui,
                        "SOLO",
                        dsp.solo,
                        StudioPalette::ACCENT_AMBER,
                        Vec2::new(40.0, 20.0),
                    ) {
                        dsp.solo = !dsp.solo;
                        dsp_changed = true;
                    }
                    if draw_console_switch(
                        ui,
                        "MUTE",
                        dsp.mute,
                        StudioPalette::ACCENT_CRIMSON,
                        Vec2::new(40.0, 20.0),
                    ) {
                        dsp.mute = !dsp.mute;
                        dsp_changed = true;
                    }
                });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 6.0;
                    let fader_accent = if ch_lum < 50.0 {
                        Color32::from_rgb(140, 145, 160)
                    } else {
                        strip_accent
                    };
                    if draw_wing_fader(ui, ("wing_fader", idx), &mut dsp.fader_db, fader_accent, 160.0) {
                        dsp_changed = true;
                    }
                    draw_wing_vertical_meter(
                        ui,
                        Vec2::new(24.0, 160.0),
                        meter.out_left,
                        meter.out_right,
                        hold[0],
                        hold[1],
                    );
                });

                ui.add_space(3.0);
                let db_txt = if dsp.fader_db <= -59.5 {
                    "-inf dB".to_string()
                } else {
                    format!("{:+.1} dB", dsp.fader_db)
                };
                ui.add_sized(
                    Vec2::new(82.0, 16.0),
                    egui::Label::new(
                        RichText::new(db_txt)
                            .small()
                            .monospace()
                            .color(StudioPalette::TEXT_SECONDARY),
                    ),
                );

                if draw_console_switch(
                    ui,
                    "SEL",
                    selected,
                    StudioPalette::ACCENT_CYAN,
                    Vec2::new(82.0, 22.0),
                ) {
                    ctx.state.selection = WingSelection::Channel(idx);
                }
            });
        })
        .response;

    let strip_rect = frame_resp.rect;
    let painter = ui.painter();
    painter.line_segment(
        [
            Pos2::new(strip_rect.left() + 0.8, strip_rect.top() + 4.0),
            Pos2::new(strip_rect.left() + 0.8, strip_rect.bottom() - 4.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 10)),
    );
    painter.line_segment(
        [
            Pos2::new(strip_rect.right() - 0.8, strip_rect.top() + 4.0),
            Pos2::new(strip_rect.right() - 0.8, strip_rect.bottom() - 4.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 80)),
    );

    if ctx.state.scroll_to_channel == Some(idx) {
        ctx.state.scroll_to_channel = None;
        frame_resp.scroll_to_me(Some(egui::Align::Center));
    }

    frame_resp.context_menu(|ui| {
        show_channel_context_menu(
            ui,
            ctx.lang,
            &ch,
            ch_count,
            ctx.input_devices,
            &mut pending_input,
            &mut pending_color,
            &mut pending_icon,
            &mut pending_pick_custom_icon,
            &mut pending_load_media,
            &mut pending_remove,
            &mut dsp,
            &mut dsp_changed,
            &mut ctx.state.selection,
            &mut ctx.state.active_tab,
        );
    });

    if let Some(color) = pending_color {
        ctx.mixer.set_channel_color(idx, color);
    }
    if let Some(icon) = pending_icon {
        ctx.mixer.set_channel_icon(idx, icon);
    }
    if pending_pick_custom_icon {
        if let Some(img_path) = pick_custom_icon_dialog() {
            ctx.state.custom_icon_textures.remove(&img_path);
            ctx.mixer
                .set_channel_icon(idx, ChannelIcon::CustomImage(img_path));
        }
    }
    if dsp_changed {
        ctx.mixer.set_channel_dsp(idx, dsp);
    }
    if let Some(src) = pending_input {
        assign_channel_input_with_audio(ctx, idx, src);
    }
    if pending_load_media {
        ctx.state.selection = WingSelection::Channel(idx);
        trigger_load_media_dialog(ctx, idx);
    }
    if pending_remove && ctx.mixer.remove_channel(idx) {
        let next_count = ctx.mixer.channels().len();
        if next_count > 0 {
            ctx.state.selection = WingSelection::Channel(idx.min(next_count - 1));
        }
    }
}

fn render_wing_master_strip(ui: &mut egui::Ui, ctx: &mut MixerPanelContext<'_>) {
    let selected = ctx.state.selection == WingSelection::Master;
    let m = ctx.mixer.master().clone();
    let mut fader_db = m.fader_db;
    let mut mute = m.mute;
    let mut dim = m.dim;
    let mut changed = false;
    let vst_count = m.inserts.iter().filter(|s| s.meta.is_some()).count();
    let any_solo = ctx.mixer.channels().iter().any(|c| c.dsp.solo);
    let armed_count = ctx.mixer.channels().iter().filter(|c| c.dsp.record_arm).count();
    let vmic_active = ctx.mixer.is_virtual_mic_active();
    let mut pending_toggle_vmic = false;

    let master_frame_resp = egui::Frame::new()
        .fill(if selected {
            Color32::from_rgb(33, 29, 24)
        } else {
            Color32::from_rgb(23, 21, 19)
        })
        .stroke(Stroke::new(
            if selected { 1.5_f32 } else { 1.0_f32 },
            if selected {
                WingColors::WING_ORANGE
            } else {
                Color32::from_rgb(46, 52, 66)
            },
        ))
        .corner_radius(CornerRadius::same(3))
        .inner_margin(Margin::same(5))
        .show(ui, |ui| {
            ui.set_min_width(88.0);
            ui.set_max_width(88.0);
            ui.vertical(|ui| {
                let (scr_rect, scr_resp) =
                    ui.allocate_exact_size(Vec2::new(88.0, 48.0), Sense::click());
                if scr_resp.clicked() {
                    ctx.state.selection = WingSelection::Master;
                }
                let p = ui.painter();
                let socket_rect = scr_rect.expand(1.2);
                p.rect_filled(socket_rect, CornerRadius::same(3), Color32::from_rgb(7, 7, 9));
                p.rect_stroke(
                    socket_rect,
                    CornerRadius::same(3),
                    Stroke::new(1.0_f32, Color32::from_rgb(26, 28, 34)),
                    StrokeKind::Inside,
                );

                p.rect_filled(scr_rect, CornerRadius::same(2), Color32::from_rgb(10, 11, 14));
                p.rect_filled(
                    Rect::from_min_size(scr_rect.min, Vec2::new(scr_rect.width(), 14.0)),
                    CornerRadius::same(1),
                    WingColors::WING_ORANGE,
                );
                p.rect_stroke(
                    scr_rect,
                    CornerRadius::same(2),
                    Stroke::new(1.0_f32, WingColors::WING_ORANGE),
                    StrokeKind::Inside,
                );
                p.text(
                    Pos2::new(scr_rect.center().x, scr_rect.top() + 7.0),
                    Align2::CENTER_CENTER,
                    "MAIN L/R",
                    FontId::monospace(9.5),
                    Color32::from_rgb(12, 14, 20),
                );
                p.text(
                    Pos2::new(scr_rect.center().x, scr_rect.top() + 30.0),
                    Align2::CENTER_CENTER,
                    if vmic_active { "VMIC LIVE" } else { "MASTER BUS" },
                    FontId::monospace(9.0),
                    if vmic_active {
                        StudioPalette::ACCENT_EMERALD
                    } else {
                        WingColors::WING_ORANGE
                    },
                );

                paint_glass_scribble_cover(&p, scr_rect);

                ui.add_space(3.0);
                if draw_console_switch(
                    ui,
                    &format!("MASTER FX:{vst_count}"),
                    vst_count > 0,
                    StudioPalette::ACCENT_PURPLE,
                    Vec2::new(88.0, 18.0),
                ) {
                    ctx.state.selection = WingSelection::Master;
                    ctx.state.active_tab = WingScreenTab::Vst;
                }

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if draw_console_switch(
                        ui,
                        &format!("REC:{armed_count}"),
                        armed_count > 0,
                        StudioPalette::ACCENT_CRIMSON,
                        Vec2::new(43.0, 18.0),
                    ) {
                        ctx.state.selection = WingSelection::Master;
                        ctx.state.active_tab = WingScreenTab::IoRec;
                    }
                    if draw_console_switch(
                        ui,
                        "VMIC",
                        vmic_active,
                        StudioPalette::ACCENT_EMERALD,
                        Vec2::new(43.0, 18.0),
                    ) {
                        pending_toggle_vmic = true;
                    }
                });

                ui.add_space(3.0);
                if draw_console_switch(
                    ui,
                    "CLR SOLO",
                    any_solo,
                    StudioPalette::ACCENT_AMBER,
                    Vec2::new(88.0, 21.0),
                ) && any_solo
                {
                    ctx.mixer.clear_all_solos();
                }

                ui.add_space(3.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 2.0;
                    if draw_console_switch(
                        ui,
                        "DIM",
                        dim,
                        StudioPalette::ACCENT_AMBER,
                        Vec2::new(43.0, 20.0),
                    ) {
                        dim = !dim;
                        changed = true;
                    }
                    if draw_console_switch(
                        ui,
                        "MUTE",
                        mute,
                        StudioPalette::ACCENT_CRIMSON,
                        Vec2::new(43.0, 20.0),
                    ) {
                        mute = !mute;
                        changed = true;
                    }
                });

                ui.add_space(5.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 8.0;
                    if draw_wing_fader(
                        ui,
                        ("wing_fader_master", 0_usize),
                        &mut fader_db,
                        WingColors::WING_ORANGE,
                        160.0,
                    ) {
                        changed = true;
                    }
                    draw_wing_vertical_meter(
                        ui,
                        Vec2::new(26.0, 160.0),
                        ctx.state.meters.master_left,
                        ctx.state.meters.master_right,
                        ctx.state.peak_hold_master[0],
                        ctx.state.peak_hold_master[1],
                    );
                });

                ui.add_space(3.0);
                let db_txt = if fader_db <= -59.5 {
                    "-inf dB".to_string()
                } else {
                    format!("{:+.1} dB", fader_db)
                };
                ui.add_sized(
                    Vec2::new(88.0, 16.0),
                    egui::Label::new(
                        RichText::new(db_txt)
                            .small()
                            .monospace()
                            .color(WingColors::WING_ORANGE),
                    ),
                );

                if draw_console_switch(
                    ui,
                    "SEL MAIN",
                    selected,
                    WingColors::WING_ORANGE,
                    Vec2::new(88.0, 22.0),
                ) {
                    ctx.state.selection = WingSelection::Master;
                }
            });
        })
        .response;

    let master_rect = master_frame_resp.rect;
    let painter = ui.painter();
    painter.line_segment(
        [
            Pos2::new(master_rect.left() + 0.8, master_rect.top() + 4.0),
            Pos2::new(master_rect.left() + 0.8, master_rect.bottom() - 4.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 12)),
    );
    painter.line_segment(
        [
            Pos2::new(master_rect.right() - 0.8, master_rect.top() + 4.0),
            Pos2::new(master_rect.right() - 0.8, master_rect.bottom() - 4.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 85)),
    );

    if changed {
        ctx.mixer.set_master(fader_db, mute, dim, m.record_mix);
    }
    if pending_toggle_vmic {
        toggle_virtual_mic(ctx);
    }
}

#[allow(clippy::too_many_arguments)]
fn show_channel_context_menu(
    ui: &mut egui::Ui,
    lang: Lang,
    ch: &MixerChannelState,
    ch_count: usize,
    input_devices: &[AudioDeviceInfo],
    pending_input: &mut Option<ChannelInputSource>,
    pending_color: &mut Option<[u8; 3]>,
    pending_icon: &mut Option<ChannelIcon>,
    pending_pick_custom_icon: &mut bool,
    pending_load_media: &mut bool,
    pending_remove: &mut bool,
    dsp: &mut ChannelDspConfig,
    dsp_changed: &mut bool,
    selection: &mut WingSelection,
    active_tab: &mut WingScreenTab,
) {
    let t = lang.text();
    ui.label(
        RichText::new(format!("CH {:02} · {}", ch.index + 1, ch.name))
            .strong()
            .color(StudioPalette::ACCENT_CYAN),
    );
    ui.label(RichText::new(t.mixer_right_click_title).small().color(StudioPalette::TEXT_SECONDARY));
    ui.separator();

    if ui
        .selectable_label(ch.dsp.record_arm, "● REC (Grabar este canal)")
        .clicked()
    {
        dsp.record_arm = !dsp.record_arm;
        *dsp_changed = true;
        ui.close();
    }
    if ui
        .selectable_label(ch.dsp.send_to_virtual_mic, t.mixer_vmic_send_ch)
        .clicked()
    {
        dsp.send_to_virtual_mic = !dsp.send_to_virtual_mic;
        *dsp_changed = true;
        ui.close();
    }
    if ui
        .selectable_label(ch.dsp.solo_safe, t.mixer_solo_safe)
        .clicked()
    {
        dsp.solo_safe = !dsp.solo_safe;
        *dsp_changed = true;
        ui.close();
    }

    if ui
        .selectable_label(
            ch.source.mode == ChannelInputMode::Multimedia,
            t.mixer_media_convert,
        )
        .clicked()
    {
        *pending_input = Some(ChannelInputSource {
            device_id: None,
            device_label: "MEDIA".into(),
            mode: ChannelInputMode::Multimedia,
        });
        *selection = WingSelection::Channel(ch.index);
        *active_tab = WingScreenTab::Source;
        ui.close();
    }
    if ui.button(t.mixer_media_load).clicked() {
        *pending_load_media = true;
        ui.close();
    }

    ui.separator();
    ui.menu_button(t.mixer_icon_label, |ui| {
        for preset in ChannelIcon::presets() {
            let sel = &ch.icon == preset;
            if ui
                .selectable_label(sel, channel_icon_name(preset, lang))
                .clicked()
            {
                *pending_icon = Some(preset.clone());
                ui.close();
            }
        }
        ui.separator();
        if ui.button(t.mixer_icon_custom).clicked() {
            *pending_pick_custom_icon = true;
            ui.close();
        }
    });

    ui.separator();
    if ui
        .selectable_label(
            ch.source.mode == ChannelInputMode::Unassigned,
            t.mixer_disconnect_input,
        )
        .clicked()
    {
        *pending_input = Some(ChannelInputSource::default());
        ui.close();
    }

    for dev in input_devices {
        ui.menu_button(&dev.name, |ui| {
            let max_ch = (dev.default_channels as usize).clamp(2, 16);
            for c in 0..max_ch {
                let mode = ChannelInputMode::Mono(c);
                let selected = ch.source.device_id.as_deref() == Some(dev.id.as_str())
                    && ch.source.mode == mode;
                if ui
                    .selectable_label(selected, format!("Mono In {}", c + 1))
                    .clicked()
                {
                    *pending_input = Some(ChannelInputSource {
                        device_id: Some(dev.id.clone()),
                        device_label: short_device_name(&dev.name),
                        mode,
                    });
                    ui.close();
                }
            }
            ui.separator();
            let mut pair = 0;
            while pair + 1 < max_ch {
                let mode = ChannelInputMode::Stereo(pair, pair + 1);
                let selected = ch.source.device_id.as_deref() == Some(dev.id.as_str())
                    && ch.source.mode == mode;
                if ui
                    .selectable_label(selected, format!("Stereo In {}+{}", pair + 1, pair + 2))
                    .clicked()
                {
                    *pending_input = Some(ChannelInputSource {
                        device_id: Some(dev.id.clone()),
                        device_label: short_device_name(&dev.name),
                        mode,
                    });
                    ui.close();
                }
                pair += 2;
            }
        });
    }

    ui.separator();
    ui.label(RichText::new(t.mixer_channel_color).small().color(StudioPalette::TEXT_SECONDARY));
    ui.horizontal_wrapped(|ui| {
        for rgb in WING_PALETTE_SWATCHES {
            let swatch = Color32::from_rgb(rgb[0], rgb[1], rgb[2]);
            let (r, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            if resp.clicked() {
                *pending_color = Some(rgb);
                ui.close();
            }
            let is_dark = (rgb[0] as f32 * 0.299 + rgb[1] as f32 * 0.587 + rgb[2] as f32 * 0.114) < 45.0;
            let base_stroke = if is_dark {
                Stroke::new(1.0_f32, Color32::from_rgb(60, 66, 80))
            } else {
                Stroke::NONE
            };
            ui.painter().rect(
                r,
                CornerRadius::same(3),
                swatch,
                base_stroke,
                StrokeKind::Inside,
            );
            if ch.color == rgb {
                ui.painter().rect_stroke(
                    r.expand(1.0),
                    CornerRadius::same(3),
                    Stroke::new(2.0_f32, Color32::WHITE),
                    StrokeKind::Outside,
                );
            }
        }
    });

    ui.separator();
    if ui.button(t.mixer_upper_vst_title).clicked() {
        *selection = WingSelection::Channel(ch.index);
        *active_tab = WingScreenTab::Vst;
        ui.close();
    }
    if ui.button("Unity 0.0 dB").clicked() {
        dsp.fader_db = 0.0;
        *dsp_changed = true;
        ui.close();
    }
    if ch_count > 1 && ui.button(t.mixer_remove_this_channel).clicked() {
        *pending_remove = true;
        ui.close();
    }
}

fn paint_glass_scribble_cover(p: &egui::Painter, rect: Rect) {
    let top_rim = [
        Pos2::new(rect.left() + 1.5, rect.top() + 0.8),
        Pos2::new(rect.right() - 1.5, rect.top() + 0.8),
    ];
    p.line_segment(
        top_rim,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 45)),
    );

    let left_rim = [
        Pos2::new(rect.left() + 0.8, rect.top() + 1.5),
        Pos2::new(rect.left() + 0.8, rect.bottom() - 1.5),
    ];
    p.line_segment(
        left_rim,
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 18)),
    );

    let cut_y_l = (rect.top() + rect.height() * 0.52).min(rect.bottom() - 2.0);
    let cut_y_r = (rect.top() + rect.height() * 0.24).min(rect.bottom() - 2.0);

    let pts = vec![
        Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, cut_y_r),
        Pos2::new(rect.left() + 1.0, cut_y_l),
    ];
    p.add(Shape::convex_polygon(
        pts,
        Color32::from_rgba_unmultiplied(255, 255, 255, 16),
        Stroke::NONE,
    ));

    p.line_segment(
        [
            Pos2::new(rect.left() + 1.0, cut_y_l),
            Pos2::new(rect.right() - 1.0, cut_y_r),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 26)),
    );

    p.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 20)),
        StrokeKind::Inside,
    );
}

fn draw_console_switch(
    ui: &mut egui::Ui,
    label: &str,
    active: bool,
    active_color: Color32,
    size: Vec2,
) -> bool {
    let id = ui.make_persistent_id(("wing_sw", label, ui.next_auto_id()));
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    let act_t = ui.ctx().animate_bool_responsive(id.with("act"), active);
    let hov_t = ui
        .ctx()
        .animate_bool_responsive(id.with("hov"), resp.hovered());

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let pressed_offset = if resp.is_pointer_button_down_on() { 0.7 } else { 0.0 };
        let socket_rect = rect.expand(0.8);
        p.rect_filled(socket_rect, CornerRadius::same(3), Color32::from_rgb(7, 7, 9));
        p.rect_stroke(
            socket_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, Color32::from_rgb(18, 19, 24)),
            StrokeKind::Inside,
        );

        let btn_rect = rect.translate(Vec2::new(0.0, pressed_offset));
        let off_body = Color32::from_rgb(26, 27, 33);
        let hov_body = Color32::from_rgb(38, 40, 48);
        let on_body = lerp_color(active_color, Color32::WHITE, 0.14);
        let base_body = lerp_color(lerp_color(off_body, hov_body, hov_t), on_body, act_t);
        p.rect_filled(btn_rect, CornerRadius::same(3), base_body);

        if act_t > 0.02 {
            let core_w = (btn_rect.width() * 0.66).max(8.0);
            let core_h = (btn_rect.height() * 0.58).max(6.0);
            let core_rect = Rect::from_center_size(btn_rect.center(), Vec2::new(core_w, core_h));
            let alpha = (120.0 * act_t).clamp(0.0, 150.0) as u8;
            let glow_col = if active_color == StudioPalette::ACCENT_CRIMSON {
                Color32::from_rgba_unmultiplied(255, 120, 120, alpha)
            } else if active_color == StudioPalette::ACCENT_AMBER {
                Color32::from_rgba_unmultiplied(255, 235, 160, alpha)
            } else if active_color == StudioPalette::ACCENT_CYAN || active_color == StudioPalette::ACCENT_EMERALD {
                Color32::from_rgba_unmultiplied(215, 245, 255, alpha)
            } else {
                Color32::from_rgba_unmultiplied(255, 255, 255, alpha)
            };
            p.rect_filled(core_rect, CornerRadius::same(2), glow_col);
        }

        let rim_alpha = if active { 135 } else { 38 };
        p.line_segment(
            [
                Pos2::new(btn_rect.left() + 2.0, btn_rect.top() + 1.0),
                Pos2::new(btn_rect.right() - 2.0, btn_rect.top() + 1.0),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, rim_alpha)),
        );
        p.line_segment(
            [
                Pos2::new(btn_rect.left() + 2.0, btn_rect.bottom() - 1.0),
                Pos2::new(btn_rect.right() - 2.0, btn_rect.bottom() - 1.0),
            ],
            Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(0, 0, 0, 85)),
        );

        let border_col = lerp_color(
            Color32::from_rgb(42, 45, 54),
            lerp_color(active_color, Color32::WHITE, 0.45),
            act_t.max(hov_t * 0.4),
        );
        p.rect_stroke(
            btn_rect,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, border_col),
            StrokeKind::Inside,
        );

        let txt_col = if active {
            if active_color == StudioPalette::ACCENT_CRIMSON {
                Color32::WHITE
            } else {
                Color32::from_rgb(12, 14, 20)
            }
        } else {
            lerp_color(StudioPalette::TEXT_PRIMARY, Color32::WHITE, hov_t)
        };
        p.text(
            btn_rect.center(),
            Align2::CENTER_CENTER,
            label,
            FontId::monospace(9.0),
            txt_col,
        );
    }
    resp.clicked()
}

fn draw_wing_fader(
    ui: &mut egui::Ui,
    id_salt: impl std::hash::Hash,
    db_value: &mut f32,
    accent: Color32,
    height: f32,
) -> bool {
    let width = 46.0;
    let id = ui.make_persistent_id(id_salt);
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height), Sense::click_and_drag());
    let mut changed = false;

    if resp.double_clicked() {
        *db_value = 0.0;
        changed = true;
    } else if resp.dragged() {
        if let Some(pos) = resp.interact_pointer_pos() {
            let track_top = rect.top() + 14.0;
            let track_bot = rect.bottom() - 14.0;
            let norm = (1.0 - ((pos.y - track_top) / (track_bot - track_top).max(1.0))).clamp(0.0, 1.0);
            *db_value = fader_norm_to_db(norm);
            changed = true;
        }
    }
    if resp.hovered() || resp.dragged() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
    }

    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        let well = Rect::from_min_max(
            Pos2::new(rect.left() + 8.0, rect.top() + 2.0),
            Pos2::new(rect.right() - 2.0, rect.bottom() - 2.0),
        );
        p.rect_filled(well, CornerRadius::same(2), WingColors::FADER_WELL);
        p.rect_stroke(
            well,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, Color32::from_rgb(36, 42, 52)),
            StrokeKind::Inside,
        );

        let track_top = rect.top() + 14.0;
        let track_bot = rect.bottom() - 14.0;
        let track_x = well.center().x;

        for mark_db in [10.0_f32, 5.0, 0.0, -5.0, -10.0, -20.0, -40.0] {
            let n = fader_db_to_norm(mark_db);
            let y = track_bot - n * (track_bot - track_top);
            let tick_col = if (mark_db - 0.0).abs() < 0.01 {
                Color32::WHITE
            } else {
                Color32::from_rgb(82, 92, 112)
            };
            p.line_segment(
                [Pos2::new(rect.left() + 1.0, y), Pos2::new(well.left() + 5.0, y)],
                Stroke::new(if mark_db == 0.0 { 1.6_f32 } else { 1.0_f32 }, tick_col),
            );
            p.line_segment(
                [Pos2::new(well.right() - 5.0, y), Pos2::new(well.right() - 1.0, y)],
                Stroke::new(1.0_f32, tick_col),
            );
        }

        let norm = fader_db_to_norm(*db_value);
        let anim_norm = ui.ctx().animate_value_with_time(id.with("n"), norm, 0.05);
        let cap_y = track_bot - anim_norm * (track_bot - track_top);

        p.line_segment(
            [Pos2::new(track_x, track_top), Pos2::new(track_x, track_bot)],
            Stroke::new(3.0_f32, Color32::from_rgb(14, 17, 24)),
        );
        if cap_y < track_bot - 1.0 {
            p.line_segment(
                [Pos2::new(track_x, cap_y), Pos2::new(track_x, track_bot)],
                Stroke::new(2.2_f32, lerp_color(accent, Color32::WHITE, 0.2)),
            );
        }

        let cap_rect = Rect::from_center_size(Pos2::new(track_x, cap_y), Vec2::new(28.0, 22.0));
        let shadow_rect = cap_rect.translate(Vec2::new(0.0, 2.5));
        p.rect_filled(shadow_rect, CornerRadius::same(2), Color32::from_black_alpha(135));
        p.rect_filled(cap_rect, CornerRadius::same(2), WingColors::CAP_MID);

        let top_bevel = Rect::from_min_max(
            cap_rect.min,
            Pos2::new(cap_rect.right(), cap_rect.top() + 5.5),
        );
        let bot_bevel = Rect::from_min_max(
            Pos2::new(cap_rect.left(), cap_rect.bottom() - 5.5),
            cap_rect.max,
        );
        p.rect_filled(top_bevel, CornerRadius::same(1), WingColors::CAP_TOP);
        p.rect_filled(bot_bevel, CornerRadius::same(1), WingColors::CAP_DARK);
        p.rect_stroke(
            cap_rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, Color32::from_rgb(235, 240, 248)),
            StrokeKind::Inside,
        );
        p.line_segment(
            [
                Pos2::new(cap_rect.left() + 3.0, cap_y),
                Pos2::new(cap_rect.right() - 3.0, cap_y),
            ],
            Stroke::new(1.8_f32, Color32::from_rgb(18, 22, 28)),
        );
        p.line_segment(
            [
                Pos2::new(cap_rect.left() + 6.0, cap_y - 1.0),
                Pos2::new(cap_rect.right() - 6.0, cap_y - 1.0),
            ],
            Stroke::new(1.0_f32, accent),
        );
    }

    changed
}

fn draw_wing_vertical_meter(
    ui: &mut egui::Ui,
    size: Vec2,
    left_amp: f32,
    right_amp: f32,
    hold_l: f32,
    hold_r: f32,
) {
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), WingColors::FADER_WELL);
    p.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, Color32::from_rgb(38, 43, 56)),
        StrokeKind::Inside,
    );

    let gap = 3.0;
    let bar_w = (rect.width() - gap * 3.0) * 0.5;
    let segments = 18_usize;
    let pad_y = 4.0;
    let usable_h = rect.height() - pad_y * 2.0;
    let seg_step = usable_h / segments as f32;
    let seg_h = (seg_step - 1.5).max(1.5);

    for (ch_i, (amp, hold)) in [(left_amp, hold_l), (right_amp, hold_r)].into_iter().enumerate() {
        let bx = rect.left() + gap + ch_i as f32 * (bar_w + gap);
        let norm = amp_to_meter_norm(amp);
        let hold_norm = amp_to_meter_norm(hold);

        for s in 0..segments {
            let frac_top = (s + 1) as f32 / segments as f32;
            let sy = rect.bottom() - pad_y - (s as f32 + 1.0) * seg_step + 0.75;
            let seg_rect = Rect::from_min_size(Pos2::new(bx, sy), Vec2::new(bar_w, seg_h));

            let base_col = if frac_top > 0.88 {
                StudioPalette::ACCENT_CRIMSON
            } else if frac_top > 0.70 {
                Color32::from_rgb(215, 218, 226)
            } else {
                Color32::from_rgb(175, 180, 192)
            };

            let lit = norm >= frac_top - (0.5 / segments as f32);
            let is_hold = hold_norm > 0.02
                && (hold_norm - frac_top).abs() <= (0.8 / segments as f32);
            let col = if lit || is_hold {
                base_col
            } else {
                Color32::from_rgb(20, 20, 25)
            };
            p.rect_filled(seg_rect, CornerRadius::same(1), col);
        }
    }

    p.line_segment(
        [
            Pos2::new(rect.left() + 1.2, rect.top() + 1.0),
            Pos2::new(rect.right() - 1.2, rect.top() + 1.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgba_unmultiplied(255, 255, 255, 35)),
    );
    let cut_y = rect.top() + (rect.height() * 0.35).min(50.0);
    let pts = vec![
        Pos2::new(rect.left() + 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, rect.top() + 1.0),
        Pos2::new(rect.right() - 1.0, cut_y - 12.0),
        Pos2::new(rect.left() + 1.0, cut_y),
    ];
    p.add(Shape::convex_polygon(
        pts,
        Color32::from_rgba_unmultiplied(255, 255, 255, 12),
        Stroke::NONE,
    ));
}

fn draw_wing_eq_curve(ui: &mut egui::Ui, dsp: &ChannelDspConfig, sr: f32, height: f32) {
    let w = ui.available_width().max(120.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, height), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(10, 13, 19));
    p.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, Color32::from_rgb(46, 54, 72)),
        StrokeKind::Inside,
    );

    let min_f = 20.0_f32.ln();
    let max_f = 20000.0_f32.ln();
    for hz in [50.0_f32, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0] {
        let t = (hz.ln() - min_f) / (max_f - min_f);
        let x = rect.left() + t * rect.width();
        p.line_segment(
            [Pos2::new(x, rect.top() + 2.0), Pos2::new(x, rect.bottom() - 2.0)],
            Stroke::new(1.0_f32, Color32::from_rgb(24, 29, 40)),
        );
    }

    let mid_y = rect.center().y;
    for db in [-12.0_f32, -6.0, 0.0, 6.0, 12.0] {
        let y = mid_y - (db / 20.0) * (rect.height() * 0.46);
        let col = if db == 0.0 {
            Color32::from_rgb(56, 65, 84)
        } else {
            Color32::from_rgb(24, 29, 40)
        };
        p.line_segment(
            [Pos2::new(rect.left() + 2.0, y), Pos2::new(rect.right() - 2.0, y)],
            Stroke::new(1.0_f32, col),
        );
    }

    let steps = 96_usize;
    let mut curve_pts = Vec::with_capacity(steps + 1);
    for i in 0..=steps {
        let t = i as f32 / steps as f32;
        let freq = (min_f + t * (max_f - min_f)).exp();
        let db = eval_channel_eq_db(dsp, freq, sr).clamp(-20.0, 20.0);
        let x = rect.left() + t * rect.width();
        let y = (mid_y - (db / 20.0) * (rect.height() * 0.46)).clamp(rect.top() + 2.0, rect.bottom() - 2.0);
        curve_pts.push(Pos2::new(x, y));
    }

    for pt in &curve_pts {
        let top_y = pt.y.min(mid_y);
        let bot_y = pt.y.max(mid_y);
        if bot_y - top_y > 0.5 {
            p.line_segment(
                [Pos2::new(pt.x, top_y), Pos2::new(pt.x, bot_y)],
                Stroke::new(2.0_f32, Color32::from_rgba_unmultiplied(245, 145, 32, 36)),
            );
        }
    }
    p.add(Shape::line(curve_pts, Stroke::new(2.0_f32, WingColors::WING_ORANGE)));

    if dsp.eq_enabled {
        for (i, band) in dsp.eq_bands.iter().enumerate() {
            if !band.enabled {
                continue;
            }
            let t = ((band.freq_hz.clamp(20.0, 20000.0)).ln() - min_f) / (max_f - min_f);
            let x = rect.left() + t * rect.width();
            let y = (mid_y - (band.gain_db.clamp(-18.0, 18.0) / 20.0) * (rect.height() * 0.46))
                .clamp(rect.top() + 4.0, rect.bottom() - 4.0);
            p.circle_filled(Pos2::new(x, y), 4.0, WingColors::WING_ORANGE);
            p.text(
                Pos2::new(x, y - 7.0),
                Align2::CENTER_BOTTOM,
                format!("{}", i + 1),
                FontId::monospace(8.5),
                Color32::WHITE,
            );
        }
    }
}

fn draw_wing_dynamics_curve(ui: &mut egui::Ui, dsp: &ChannelDspConfig, gr_db: f32) {
    let size = Vec2::new(150.0, 150.0);
    let (rect, _) = ui.allocate_exact_size(size, Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }

    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), Color32::from_rgb(10, 13, 19));
    p.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, Color32::from_rgb(46, 54, 72)),
        StrokeKind::Inside,
    );

    p.line_segment(
        [
            Pos2::new(rect.left() + 4.0, rect.bottom() - 4.0),
            Pos2::new(rect.right() - 4.0, rect.top() + 4.0),
        ],
        Stroke::new(1.0_f32, Color32::from_rgb(44, 52, 68)),
    );

    let thr = dsp.comp_threshold_db;
    let ratio = dsp.comp_ratio.max(1.0);
    let mut pts = Vec::new();
    for db in (-60..=0).step_by(2) {
        let in_db = db as f32;
        let out_db = if dsp.gate_enabled && in_db < dsp.gate_threshold_db {
            -60.0
        } else if dsp.comp_enabled && in_db > thr {
            thr + (in_db - thr) / ratio
        } else {
            in_db
        } + if dsp.comp_enabled { dsp.comp_makeup_db } else { 0.0 };

        let nx = (in_db + 60.0) / 60.0;
        let ny = ((out_db + 60.0) / 60.0).clamp(0.0, 1.0);
        pts.push(Pos2::new(
            rect.left() + 4.0 + nx * (rect.width() - 22.0),
            rect.bottom() - 4.0 - ny * (rect.height() - 8.0),
        ));
    }
    p.add(Shape::line(pts, Stroke::new(2.0_f32, StudioPalette::ACCENT_CYAN)));

    let gr_bar_rect = Rect::from_min_max(
        Pos2::new(rect.right() - 12.0, rect.top() + 4.0),
        Pos2::new(rect.right() - 4.0, rect.bottom() - 4.0),
    );
    p.rect_filled(gr_bar_rect, CornerRadius::same(1), Color32::from_rgb(24, 28, 36));
    let gr_frac = (gr_db / 24.0).clamp(0.0, 1.0);
    if gr_frac > 0.01 {
        let h = gr_bar_rect.height() * gr_frac;
        let fill = Rect::from_min_size(gr_bar_rect.min, Vec2::new(gr_bar_rect.width(), h));
        p.rect_filled(fill, CornerRadius::same(1), WingColors::WING_ORANGE);
    }
}

fn draw_wing_wood_bolster(ui: &mut egui::Ui) {
    let w = ui.available_width().max(100.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 16.0), Sense::hover());
    if !ui.is_rect_visible(rect) {
        return;
    }
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(2), WingColors::WALNUT_BASE);
    let top_strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 4.5));
    let bot_strip = Rect::from_min_size(
        Pos2::new(rect.left(), rect.bottom() - 4.5),
        Vec2::new(rect.width(), 4.5),
    );
    p.rect_filled(top_strip, CornerRadius::same(1), WingColors::WALNUT_LIGHT);
    p.rect_filled(bot_strip, CornerRadius::same(1), WingColors::WALNUT_DARK);
    p.rect_stroke(
        rect,
        CornerRadius::same(2),
        Stroke::new(1.0_f32, Color32::from_rgb(36, 36, 42)),
        StrokeKind::Inside,
    );
}

fn start_or_restart_mixer_audio(ctx: &mut MixerPanelContext<'_>) {
    if ctx.state.selected_output.is_empty() {
        *ctx.error = Some(ctx.lang.text().error_select_io.to_string());
        return;
    }
    (ctx.stop_classic_audio)();
    match ctx.mixer.start_audio(
        &ctx.state.selected_output,
        ctx.state.sample_rate,
        ctx.state.block_size,
    ) {
        Ok(()) => {
            *ctx.error = None;
            if let Some(info) = ctx.mixer.active_audio_info() {
                *ctx.status = StatusState::Plain(format!(
                    "Mixer LIVE · {} Hz · {} feed(s) -> {} ch out",
                    info.output_sample_rate,
                    info.feed_channels.len(),
                    info.output_channels
                ));
            }
        }
        Err(e) => {
            *ctx.error = Some(ctx.lang.failed_start_audio(&e.to_string()));
        }
    }
}

fn assign_channel_input_with_audio(
    ctx: &mut MixerPanelContext<'_>,
    ch_idx: usize,
    src: ChannelInputSource,
) {
    let summary = if src.mode == ChannelInputMode::Unassigned {
        ctx.lang.text().mixer_unassigned.to_string()
    } else {
        format!("{} ({})", src.device_label, format_input_mode(src.mode))
    };
    let ch_name = ctx
        .mixer
        .channel(ch_idx)
        .map(|c| c.name.clone())
        .unwrap_or_else(|| format!("CH {:02}", ch_idx + 1));

    if !ctx.mixer.is_audio_running() && src.mode != ChannelInputMode::Unassigned {
        (ctx.stop_classic_audio)();
    }

    let fallback_out = if ctx.state.selected_output.is_empty() {
        None
    } else {
        Some(ctx.state.selected_output.as_str())
    };
    match ctx.mixer.set_channel_input(ch_idx, src, fallback_out) {
        Ok(()) => {
            *ctx.error = None;
            *ctx.status = StatusState::Plain(ctx.lang.mixer_assigned_input(&ch_name, &summary));
        }
        Err(e) => {
            *ctx.error = Some(ctx.lang.failed_start_audio(&e.to_string()));
        }
    }
}

fn toggle_mixer_recording(ctx: &mut MixerPanelContext<'_>) {
    if ctx.mixer.is_recording() {
        match ctx.mixer.stop_recording() {
            Ok(Some(report)) => {
                *ctx.status = StatusState::Plain(
                    ctx.lang
                        .mixer_saved_session(&report.directory, report.files.len()),
                );
                *ctx.error = None;
            }
            Ok(None) => {}
            Err(e) => *ctx.error = Some(e.to_string()),
        }
        return;
    }

    if !ctx.mixer.is_audio_running() {
        start_or_restart_mixer_audio(ctx);
        if !ctx.mixer.is_audio_running() {
            return;
        }
    }

    let base_dir = ctx.state.recording_base_dir.clone();
    match ctx.mixer.start_recording(base_dir) {
        Ok(dir) => {
            *ctx.status = StatusState::RecordingTo(dir);
            *ctx.error = None;
        }
        Err(e) => {
            *ctx.error = Some(e.to_string());
        }
    }
}

fn toggle_virtual_mic(ctx: &mut MixerPanelContext<'_>) {
    if ctx.mixer.is_virtual_mic_active() {
        ctx.mixer.disable_virtual_mic();
        *ctx.status = StatusState::Plain(ctx.lang.text().mixer_vmic_toggle_off.to_string());
        *ctx.error = None;
        return;
    }

    if !ctx.mixer.is_audio_running() {
        start_or_restart_mixer_audio(ctx);
    }

    match ctx.mixer.enable_virtual_mic(None) {
        Ok(dev_name) => {
            *ctx.status = StatusState::Plain(format!(
                "{}: {}",
                ctx.lang.text().mixer_vmic_toggle_on,
                dev_name
            ));
            *ctx.error = None;
        }
        Err(e) => {
            *ctx.error = Some(e.to_string());
        }
    }
}

fn channel_icon_name(icon: &ChannelIcon, lang: Lang) -> &'static str {
    match lang {
        Lang::Es => match icon {
            ChannelIcon::None => "Sin icono",
            ChannelIcon::Vocals => "Voz / Mic",
            ChannelIcon::Guitar => "Guitarra",
            ChannelIcon::Bass => "Bajo",
            ChannelIcon::Drums => "Batería",
            ChannelIcon::Keys => "Teclado",
            ChannelIcon::Media => "Multimedia",
            ChannelIcon::Strings => "Cuerdas",
            ChannelIcon::Brass => "Vientos",
            ChannelIcon::Broadcast => "Stream / Podcast",
            ChannelIcon::Sfx => "FX / Sintetizador",
            ChannelIcon::CustomImage(_) => "Imagen propia",
        },
        Lang::En => match icon {
            ChannelIcon::None => "No icon",
            ChannelIcon::Vocals => "Vocals / Mic",
            ChannelIcon::Guitar => "Guitar",
            ChannelIcon::Bass => "Bass",
            ChannelIcon::Drums => "Drums",
            ChannelIcon::Keys => "Keys",
            ChannelIcon::Media => "Multimedia",
            ChannelIcon::Strings => "Strings",
            ChannelIcon::Brass => "Brass",
            ChannelIcon::Broadcast => "Broadcast / Stream",
            ChannelIcon::Sfx => "FX / Synth",
            ChannelIcon::CustomImage(_) => "Custom image",
        },
    }
}

fn pick_custom_icon_dialog() -> Option<PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Image (PNG, JPG, WEBP, BMP, GIF)",
            &["png", "jpg", "jpeg", "webp", "bmp", "gif"],
        )
        .pick_file()
}

fn load_custom_icon_color_image(path: &Path) -> Option<ColorImage> {
    let output = Command::new("ffmpeg")
        .args([
            "-v",
            "error",
            "-i",
        ])
        .arg(path)
        .args([
            "-vf",
            "scale=64:64:force_original_aspect_ratio=decrease,pad=64:64:(ow-iw)/2:(oh-ih)/2:color=0x00000000",
            "-frames:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgba",
            "-",
        ])
        .output()
        .ok()?;
    if output.status.success() && output.stdout.len() == 64 * 64 * 4 {
        return Some(ColorImage::from_rgba_unmultiplied([64, 64], &output.stdout));
    }
    None
}

fn draw_channel_icon(
    ui: &mut egui::Ui,
    state: &mut MixerUiState,
    rect: Rect,
    icon: &ChannelIcon,
    tint: Color32,
) {
    let painter = ui.painter();
    let c = rect.center();
    let r = (rect.width().min(rect.height()) * 0.45).max(4.0);
    let stroke = Stroke::new(1.3_f32, tint);
    let thin = Stroke::new(1.0_f32, tint);

    match icon {
        ChannelIcon::None => {
            painter.circle_stroke(c, r * 0.45, Stroke::new(1.0_f32, tint.gamma_multiply(0.5)));
            painter.line_segment(
                [Pos2::new(c.x - r * 0.35, c.y + r * 0.35), Pos2::new(c.x + r * 0.35, c.y - r * 0.35)],
                Stroke::new(1.0_f32, tint.gamma_multiply(0.5)),
            );
        }
        ChannelIcon::Vocals => {
            let cap = Rect::from_center_size(Pos2::new(c.x, c.y - r * 0.25), Vec2::new(r * 0.72, r * 0.92));
            painter.rect_stroke(cap, CornerRadius::same((r * 0.35) as u8), stroke, StrokeKind::Inside);
            painter.line_segment(
                [Pos2::new(cap.left(), cap.center().y), Pos2::new(cap.right(), cap.center().y)],
                thin,
            );
            painter.line_segment(
                [Pos2::new(c.x, cap.bottom()), Pos2::new(c.x, c.y + r * 0.78)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(c.x - r * 0.42, c.y + r * 0.78), Pos2::new(c.x + r * 0.42, c.y + r * 0.78)],
                stroke,
            );
        }
        ChannelIcon::Guitar => {
            painter.circle_stroke(Pos2::new(c.x - r * 0.22, c.y + r * 0.25), r * 0.48, stroke);
            painter.circle_filled(Pos2::new(c.x - r * 0.22, c.y + r * 0.25), r * 0.14, tint);
            painter.line_segment(
                [Pos2::new(c.x + r * 0.08, c.y - r * 0.05), Pos2::new(c.x + r * 0.75, c.y - r * 0.72)],
                Stroke::new(1.8_f32, tint),
            );
            painter.rect_filled(
                Rect::from_center_size(Pos2::new(c.x + r * 0.75, c.y - r * 0.72), Vec2::splat(r * 0.26)),
                CornerRadius::same(1),
                tint,
            );
        }
        ChannelIcon::Bass => {
            painter.circle_stroke(Pos2::new(c.x - r * 0.25, c.y + r * 0.25), r * 0.46, stroke);
            painter.line_segment(
                [Pos2::new(c.x + r * 0.04, c.y - r * 0.04), Pos2::new(c.x + r * 0.80, c.y - r * 0.80)],
                Stroke::new(1.8_f32, tint),
            );
            for i in 0..2 {
                let off = i as f32 * r * 0.18;
                painter.circle_filled(Pos2::new(c.x + r * 0.58 + off, c.y - r * 0.76 + off), 1.4, tint);
            }
            painter.line_segment(
                [Pos2::new(c.x - r * 0.42, c.y + r * 0.18), Pos2::new(c.x - r * 0.08, c.y + r * 0.36)],
                thin,
            );
        }
        ChannelIcon::Drums => {
            let shell = Rect::from_center_size(Pos2::new(c.x, c.y + r * 0.22), Vec2::new(r * 1.35, r * 0.82));
            painter.rect_stroke(shell, CornerRadius::same(2), stroke, StrokeKind::Inside);
            painter.line_segment(
                [Pos2::new(shell.left() + 2.0, shell.top()), Pos2::new(shell.center().x, shell.bottom())],
                thin,
            );
            painter.line_segment(
                [Pos2::new(shell.center().x, shell.bottom()), Pos2::new(shell.right() - 2.0, shell.top())],
                thin,
            );
            painter.line_segment(
                [Pos2::new(c.x - r * 0.65, c.y - r * 0.65), Pos2::new(c.x + r * 0.25, c.y - r * 0.15)],
                thin,
            );
            painter.line_segment(
                [Pos2::new(c.x + r * 0.65, c.y - r * 0.65), Pos2::new(c.x - r * 0.25, c.y - r * 0.15)],
                thin,
            );
        }
        ChannelIcon::Keys => {
            let kb = Rect::from_center_size(c, Vec2::new(r * 1.5, r * 1.1));
            painter.rect_stroke(kb, CornerRadius::same(2), stroke, StrokeKind::Inside);
            for i in 1..4 {
                let x = kb.left() + kb.width() * (i as f32 / 4.0);
                painter.line_segment([Pos2::new(x, kb.top()), Pos2::new(x, kb.bottom())], thin);
            }
            for x_frac in [0.22_f32, 0.48, 0.76] {
                let bx = kb.left() + kb.width() * x_frac;
                let bk = Rect::from_min_size(
                    Pos2::new(bx - r * 0.11, kb.top()),
                    Vec2::new(r * 0.22, kb.height() * 0.56),
                );
                painter.rect_filled(bk, CornerRadius::ZERO, tint);
            }
        }
        ChannelIcon::Media => {
            painter.circle_stroke(c, r * 0.78, stroke);
            let pts = vec![
                Pos2::new(c.x - r * 0.22, c.y - r * 0.36),
                Pos2::new(c.x - r * 0.22, c.y + r * 0.36),
                Pos2::new(c.x + r * 0.38, c.y),
            ];
            painter.add(egui::Shape::convex_polygon(pts, tint, Stroke::NONE));
        }
        ChannelIcon::Strings => {
            painter.circle_stroke(Pos2::new(c.x, c.y - r * 0.20), r * 0.36, stroke);
            painter.circle_stroke(Pos2::new(c.x, c.y + r * 0.28), r * 0.48, stroke);
            painter.line_segment(
                [Pos2::new(c.x, c.y - r * 0.82), Pos2::new(c.x, c.y + r * 0.65)],
                thin,
            );
            painter.line_segment(
                [Pos2::new(c.x - r * 0.65, c.y + r * 0.10), Pos2::new(c.x + r * 0.65, c.y - r * 0.25)],
                thin,
            );
        }
        ChannelIcon::Brass => {
            painter.line_segment(
                [Pos2::new(c.x - r * 0.75, c.y + r * 0.10), Pos2::new(c.x + r * 0.25, c.y + r * 0.10)],
                stroke,
            );
            let bell = vec![
                Pos2::new(c.x + r * 0.20, c.y + r * 0.10),
                Pos2::new(c.x + r * 0.75, c.y - r * 0.45),
                Pos2::new(c.x + r * 0.75, c.y + r * 0.55),
            ];
            painter.add(egui::Shape::closed_line(bell, stroke));
            for i in 0..3 {
                let vx = c.x - r * 0.35 + i as f32 * r * 0.22;
                painter.line_segment(
                    [Pos2::new(vx, c.y + r * 0.10), Pos2::new(vx, c.y - r * 0.28)],
                    thin,
                );
            }
        }
        ChannelIcon::Broadcast => {
            painter.circle_filled(Pos2::new(c.x, c.y - r * 0.10), r * 0.20, tint);
            painter.line_segment(
                [Pos2::new(c.x, c.y + r * 0.10), Pos2::new(c.x - r * 0.35, c.y + r * 0.75)],
                stroke,
            );
            painter.line_segment(
                [Pos2::new(c.x, c.y + r * 0.10), Pos2::new(c.x + r * 0.35, c.y + r * 0.75)],
                stroke,
            );
            painter.circle_stroke(Pos2::new(c.x, c.y - r * 0.10), r * 0.52, thin);
            painter.circle_stroke(Pos2::new(c.x, c.y - r * 0.10), r * 0.80, Stroke::new(0.9_f32, tint.gamma_multiply(0.65)));
        }
        ChannelIcon::Sfx => {
            let pts = [
                Pos2::new(c.x - r * 0.80, c.y),
                Pos2::new(c.x - r * 0.45, c.y - r * 0.65),
                Pos2::new(c.x - r * 0.10, c.y + r * 0.55),
                Pos2::new(c.x + r * 0.28, c.y - r * 0.75),
                Pos2::new(c.x + r * 0.58, c.y + r * 0.35),
                Pos2::new(c.x + r * 0.82, c.y),
            ];
            for w in pts.windows(2) {
                painter.line_segment([w[0], w[1]], stroke);
            }
        }
        ChannelIcon::CustomImage(path) => {
            if !state.custom_icon_textures.contains_key(path) {
                let loaded = load_custom_icon_color_image(path).map(|img| {
                    ui.ctx().load_texture(
                        format!("custom_ch_icon_{}", path.display()),
                        img,
                        TextureOptions::LINEAR,
                    )
                });
                state.custom_icon_textures.insert(path.clone(), loaded);
            }
            if let Some(Some(tex)) = state.custom_icon_textures.get(path) {
                let uv = Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
                painter.image(tex.id(), rect.shrink(1.0), uv, Color32::WHITE);
            } else {
                painter.rect_stroke(rect.shrink(2.0), CornerRadius::same(2), stroke, StrokeKind::Inside);
                painter.circle_filled(Pos2::new(c.x - r * 0.25, c.y - r * 0.22), r * 0.18, tint);
                painter.line_segment(
                    [Pos2::new(rect.left() + 3.0, rect.bottom() - 3.0), Pos2::new(c.x, c.y)],
                    thin,
                );
                painter.line_segment(
                    [Pos2::new(c.x, c.y), Pos2::new(rect.right() - 3.0, rect.bottom() - 3.0)],
                    thin,
                );
            }
        }
    }
}

fn fader_db_to_norm(db: f32) -> f32 {
    ((db.clamp(-60.0, 10.0) + 60.0) / 70.0).clamp(0.0, 1.0)
}

fn fader_norm_to_db(norm: f32) -> f32 {
    (-60.0 + norm.clamp(0.0, 1.0) * 70.0).clamp(-60.0, 10.0)
}

fn amp_to_meter_norm(amp: f32) -> f32 {
    if amp <= 1e-6 {
        return 0.0;
    }
    let db = 20.0 * amp.log10();
    ((db + 60.0) / 60.0).clamp(0.0, 1.0)
}

fn add_channel_and_focus(ctx: &mut MixerPanelContext<'_>, multimedia: bool) {
    let added = if multimedia {
        ctx.mixer.add_multimedia_channel()
    } else {
        ctx.mixer.add_channel()
    };
    if let Some(new_idx) = added {
        ctx.state.selection = WingSelection::Channel(new_idx);
        ctx.state.scroll_to_channel = Some(new_idx);
        if multimedia {
            if ctx.state.active_bank != ChannelBankSection::All {
                ctx.state.active_bank = ChannelBankSection::Multimedia;
            }
            ctx.state.active_tab = WingScreenTab::Source;
        } else if ctx.state.active_bank == ChannelBankSection::Multimedia {
            ctx.state.active_bank = ChannelBankSection::for_channel(false, new_idx);
        }
    }
}

fn add_channel_in_current_bank(ctx: &mut MixerPanelContext<'_>, multimedia: bool) {
    if multimedia {
        add_channel_and_focus(ctx, true);
        return;
    }

    match ctx.state.active_bank {
        ChannelBankSection::Ch17_32 => {
            while ctx.mixer.channels().len() < 16 && ctx.mixer.channels().len() < MAX_MIXER_CHANNELS {
                if ctx.mixer.add_channel().is_none() {
                    break;
                }
            }
            if let Some(new_idx) = ctx.mixer.add_channel() {
                ctx.state.selection = WingSelection::Channel(new_idx);
                ctx.state.scroll_to_channel = Some(new_idx);
            }
        }
        ChannelBankSection::Ch33_48 => {
            while ctx.mixer.channels().len() < 32 && ctx.mixer.channels().len() < MAX_MIXER_CHANNELS {
                if ctx.mixer.add_channel().is_none() {
                    break;
                }
            }
            if let Some(new_idx) = ctx.mixer.add_channel() {
                ctx.state.selection = WingSelection::Channel(new_idx);
                ctx.state.scroll_to_channel = Some(new_idx);
            }
        }
        ChannelBankSection::Multimedia => {
            add_channel_and_focus(ctx, true);
        }
        _ => {
            add_channel_and_focus(ctx, false);
        }
    }
}

fn trigger_load_media_dialog(ctx: &mut MixerPanelContext<'_>, idx: usize) {
    if let Some(path) = pick_media_file_dialog() {
        if !ctx.mixer.is_audio_running() {
            (ctx.stop_classic_audio)();
        }
        ctx.mixer.request_load_media(idx, path);
        ctx.state.selection = WingSelection::Channel(idx);
        *ctx.error = None;
        *ctx.status = StatusState::Plain(ctx.lang.text().mixer_media_loading.to_string());
    }
}

fn pick_media_file_dialog() -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .add_filter(
            "Audio / Media",
            &[
                "wav", "mp3", "flac", "ogg", "opus", "m4a", "aac", "wma", "aiff", "mp4", "mkv",
                "webm",
            ],
        )
        .pick_file()
}

fn format_media_time(secs: f64) -> String {
    let total = secs.max(0.0).floor() as u64;
    let mins = total / 60;
    let rem = total % 60;
    format!("{mins:02}:{rem:02}")
}

fn format_input_mode(mode: ChannelInputMode) -> String {
    match mode {
        ChannelInputMode::Unassigned => "--".to_string(),
        ChannelInputMode::Mono(c) => format!("Mono In {}", c + 1),
        ChannelInputMode::Stereo(l, r) => format!("Stereo {}+{}", l + 1, r + 1),
        ChannelInputMode::Multimedia => "Multimedia".to_string(),
    }
}

fn format_input_mode_short(mode: ChannelInputMode) -> String {
    match mode {
        ChannelInputMode::Unassigned => "--".to_string(),
        ChannelInputMode::Mono(c) => format!("M{}", c + 1),
        ChannelInputMode::Stereo(l, r) => format!("{}+{}", l + 1, r + 1),
        ChannelInputMode::Multimedia => "MEDIA".to_string(),
    }
}

fn short_device_name(raw: &str) -> String {
    raw.split('(').next().unwrap_or(raw).trim().to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    let count = s.chars().count();
    if count <= max {
        s.to_string()
    } else {
        let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
        out.push('.');
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_bank_section_matches_and_partitioning() {
        assert!(ChannelBankSection::Ch1_16.matches(false, 0));
        assert!(ChannelBankSection::Ch1_16.matches(false, 15));
        assert!(!ChannelBankSection::Ch1_16.matches(false, 16));
        assert!(!ChannelBankSection::Ch1_16.matches(true, 0));

        assert!(!ChannelBankSection::Ch17_32.matches(false, 15));
        assert!(ChannelBankSection::Ch17_32.matches(false, 16));
        assert!(ChannelBankSection::Ch17_32.matches(false, 31));
        assert!(!ChannelBankSection::Ch17_32.matches(false, 32));
        assert!(!ChannelBankSection::Ch17_32.matches(true, 16));

        assert!(!ChannelBankSection::Ch33_48.matches(false, 31));
        assert!(ChannelBankSection::Ch33_48.matches(false, 32));
        assert!(ChannelBankSection::Ch33_48.matches(false, 47));
        assert!(!ChannelBankSection::Ch33_48.matches(true, 32));

        assert!(ChannelBankSection::Multimedia.matches(true, 0));
        assert!(ChannelBankSection::Multimedia.matches(true, 5));
        assert!(!ChannelBankSection::Multimedia.matches(false, 5));

        assert!(ChannelBankSection::All.matches(false, 0));
        assert!(ChannelBankSection::All.matches(false, 40));
        assert!(ChannelBankSection::All.matches(true, 2));

        assert_eq!(ChannelBankSection::for_channel(true, 0), ChannelBankSection::Multimedia);
        assert_eq!(ChannelBankSection::for_channel(false, 4), ChannelBankSection::Ch1_16);
        assert_eq!(ChannelBankSection::for_channel(false, 18), ChannelBankSection::Ch17_32);
        assert_eq!(ChannelBankSection::for_channel(false, 35), ChannelBankSection::Ch33_48);
    }
}
