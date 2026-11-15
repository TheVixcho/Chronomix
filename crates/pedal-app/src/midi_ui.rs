use std::{
    f32::consts::PI,
    time::Instant,
};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use pedal_engine::{MediaBuffer, MixerEngine};

use crate::{
    i18n::Lang,
    mixer_ui::{MixerUiState, WingSelection},
    theme::StudioPalette,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackSound {
    Kick,
    Clap,
    Hat,
    Snare,
    SynthLead,
    SynthBass,
}

impl TrackSound {
    pub fn label(self) -> &'static str {
        match self {
            Self::Kick => "Kick 808",
            Self::Clap => "Clap Pro",
            Self::Hat => "Hi-Hat",
            Self::Snare => "Snare Crisp",
            Self::SynthLead => "Analog Lead",
            Self::SynthBass => "Sub Bass",
        }
    }
}

#[derive(Debug, Clone)]
pub struct SequencerTrack {
    pub name: String,
    pub sound: TrackSound,
    pub steps: [bool; 16],
    pub volume: f32,
    pub pan: f32,
    pub pitch_semitones: i8,
    pub mute: bool,
    pub solo: bool,
}

impl SequencerTrack {
    pub fn new(name: &str, sound: TrackSound) -> Self {
        Self {
            name: name.to_string(),
            sound,
            steps: [false; 16],
            volume: 0.82,
            pan: 0.0,
            pitch_semitones: 0,
            mute: false,
            solo: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SynthParams {
    pub attack: f32,
    pub release: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    pub delay_send: f32,
    pub delay_time_ms: f32,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            attack: 0.005,
            release: 0.28,
            cutoff: 4500.0,
            resonance: 0.35,
            drive: 0.25,
            delay_send: 0.15,
            delay_time_ms: 250.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct MidiSession {
    pub id: usize,
    pub name: String,
    pub channel_index: Option<usize>,
    pub bpm: f32,
    pub swing: f32,
    pub tracks: Vec<SequencerTrack>,
    pub synth: SynthParams,
    pub is_playing: bool,
    pub play_start: Option<Instant>,
    pub current_step: usize,
    pub last_synced_version: u64,
    pub edit_version: u64,
}

impl MidiSession {
    pub fn new(id: usize, name: String) -> Self {
        let mut kick = SequencerTrack::new("Kick", TrackSound::Kick);
        kick.steps[0] = true;
        kick.steps[4] = true;
        kick.steps[8] = true;
        kick.steps[12] = true;

        let mut clap = SequencerTrack::new("Clap", TrackSound::Clap);
        clap.steps[4] = true;
        clap.steps[12] = true;

        let mut hat = SequencerTrack::new("Hat", TrackSound::Hat);
        for i in (0..16).step_by(2) {
            hat.steps[i] = true;
        }

        let mut snare = SequencerTrack::new("Snare", TrackSound::Snare);
        snare.steps[4] = true;
        snare.steps[12] = true;

        let mut bass = SequencerTrack::new("Bass 808", TrackSound::SynthBass);
        bass.steps[0] = true;
        bass.steps[6] = true;
        bass.steps[10] = true;

        Self {
            id,
            name,
            channel_index: None,
            bpm: 120.0,
            swing: 0.0,
            tracks: vec![kick, clap, hat, snare, bass],
            synth: SynthParams::default(),
            is_playing: false,
            play_start: None,
            current_step: 0,
            last_synced_version: 0,
            edit_version: 1,
        }
    }

    pub fn mark_edited(&mut self) {
        self.edit_version = self.edit_version.wrapping_add(1);
    }
}

pub struct MidiStudioState {
    pub sessions: Vec<MidiSession>,
    pub active_session_idx: usize,
    pub next_session_id: usize,
    pub active_piano_key: Option<usize>,
    pub status_message: Option<String>,
    pub last_render_instant: Instant,
}

impl Default for MidiStudioState {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiStudioState {
    pub fn new() -> Self {
        let default_session = MidiSession::new(1, "MIDI #1".to_string());
        Self {
            sessions: vec![default_session],
            active_session_idx: 0,
            next_session_id: 2,
            active_piano_key: None,
            status_message: None,
            last_render_instant: Instant::now(),
        }
    }

    pub fn active_session(&self) -> Option<&MidiSession> {
        self.sessions.get(self.active_session_idx)
    }

    pub fn active_session_mut(&mut self) -> Option<&mut MidiSession> {
        self.sessions.get_mut(self.active_session_idx)
    }

    pub fn add_new_session(&mut self) -> usize {
        let id = self.next_session_id;
        self.next_session_id += 1;
        let session = MidiSession::new(id, format!("MIDI #{id}"));
        self.sessions.push(session);
        self.active_session_idx = self.sessions.len() - 1;
        self.active_session_idx
    }

    pub fn remove_active_session(&mut self) -> bool {
        if self.sessions.len() <= 1 {
            return false;
        }
        self.sessions.remove(self.active_session_idx);
        if self.active_session_idx >= self.sessions.len() {
            self.active_session_idx = self.sessions.len() - 1;
        }
        true
    }
}

pub fn synthesize_session_pattern(session: &MidiSession, sample_rate: u32) -> MediaBuffer {
    let sr = sample_rate.max(22050) as f32;
    let bpm = session.bpm.clamp(40.0, 240.0);
    let step_duration = 60.0 / (bpm * 4.0);
    let total_secs = step_duration * 16.0;
    let total_frames = (total_secs * sr).round() as usize;

    let mut stereo: Vec<f32> = vec![0.0; total_frames * 2];
    let any_solo = session.tracks.iter().any(|t| t.solo);

    for track in &session.tracks {
        if track.mute || (any_solo && !track.solo) {
            continue;
        }

        let left_gain = track.volume * (1.0 - track.pan).clamp(0.0, 1.0);
        let right_gain = track.volume * (1.0 + track.pan).clamp(0.0, 1.0);

        for step in 0..16 {
            if !track.steps[step] {
                continue;
            }

            let mut step_start_sec = step as f32 * step_duration;
            if step % 2 == 1 && session.swing > 0.001 {
                step_start_sec += session.swing.clamp(0.0, 0.6) * step_duration * 0.45;
            }

            let start_frame = (step_start_sec * sr) as usize;
            let note_len_frames = (match track.sound {
                TrackSound::Kick => 0.45,
                TrackSound::Clap => 0.35,
                TrackSound::Hat => 0.12,
                TrackSound::Snare => 0.38,
                TrackSound::SynthLead => session.synth.release.clamp(0.05, 1.2),
                TrackSound::SynthBass => 0.55,
            } * sr) as usize;

            for f in 0..note_len_frames {
                let frame_idx = start_frame + f;
                if frame_idx >= total_frames {
                    break;
                }

                let t = f as f32 / sr;
                let smp = match track.sound {
                    TrackSound::Kick => {
                        let env = (-t * 22.0).exp();
                        let pitch = 45.0 + 120.0 * (-t * 36.0).exp();
                        let phase = 2.0 * PI * pitch * t;
                        phase.sin() * env * 0.95
                    }
                    TrackSound::Clap => {
                        let tap1 = if t < 0.012 { 0.6 } else { 0.0 };
                        let tap2 = if (0.012..0.024).contains(&t) { 0.8 } else { 0.0 };
                        let tap3 = if (0.024..0.038).contains(&t) { 0.9 } else { 0.0 };
                        let noise_env = (-t * 26.0).exp();
                        let hash = ((f as f32 * 19.371 + t * 43.1).sin() * 43758.5453).fract();
                        let noise = (hash * 2.0 - 1.0) * noise_env;
                        (tap1 + tap2 + tap3 + noise) * 0.45
                    }
                    TrackSound::Hat => {
                        let env = (-t * 70.0).exp();
                        let hash = ((f as f32 * 31.713 + t * 91.2).sin() * 23421.631).fract();
                        let tone = (2.0 * PI * 8200.0 * t).sin() * 0.3 + (2.0 * PI * 11400.0 * t).sin() * 0.2;
                        (hash * 2.0 - 1.0 + tone) * env * 0.4
                    }
                    TrackSound::Snare => {
                        let body_env = (-t * 28.0).exp();
                        let body = (2.0 * PI * 190.0 * t).sin() * body_env * 0.6;
                        let snare_env = (-t * 24.0).exp();
                        let hash = ((f as f32 * 17.519 + t * 67.3).sin() * 37421.123).fract();
                        let noise = (hash * 2.0 - 1.0) * snare_env * 0.55;
                        body + noise
                    }
                    TrackSound::SynthLead => {
                        let freq = 261.63 * 2.0_f32.powf(track.pitch_semitones as f32 / 12.0);
                        let env = if t < session.synth.attack {
                            t / session.synth.attack.max(0.001)
                        } else {
                            (-((t - session.synth.attack) / session.synth.release.max(0.01))).exp()
                        };
                        let saw = (2.0 * (t * freq).fract() - 1.0) * 0.6;
                        let sub = (2.0 * PI * (freq * 0.5) * t).sin() * 0.3;
                        (saw + sub) * env * 0.55
                    }
                    TrackSound::SynthBass => {
                        let freq = 65.41 * 2.0_f32.powf(track.pitch_semitones as f32 / 12.0);
                        let env = (-t * 6.5).exp();
                        let wave = (2.0 * PI * freq * t).sin() * 0.75 + (2.0 * PI * (freq * 2.0) * t).sin() * 0.25;
                        (wave * (1.0 + session.synth.drive)).tanh() * env * 0.75
                    }
                };

                let drive_smp = (smp * (1.0 + session.synth.drive * 0.8)).tanh();
                stereo[frame_idx * 2] += drive_smp * left_gain;
                stereo[frame_idx * 2 + 1] += drive_smp * right_gain;
            }
        }
    }

    if session.synth.delay_send > 0.01 {
        let delay_frames = ((session.synth.delay_time_ms * 0.001 * sr).round() as usize).max(1);
        let fb = 0.38;
        let send = session.synth.delay_send.clamp(0.0, 1.0);

        for i in delay_frames..total_frames {
            let src_l = stereo[(i - delay_frames) * 2];
            let src_r = stereo[(i - delay_frames) * 2 + 1];
            stereo[i * 2] += src_r * send * fb;
            stereo[i * 2 + 1] += src_l * send * fb;
        }
    }

    for smp in &mut stereo {
        *smp = smp.clamp(-1.0, 1.0);
    }

    MediaBuffer {
        interleaved_stereo: stereo,
        total_frames,
        sample_rate,
        title: format!("{} - Sequencer Pattern", session.name),
    }
}

pub struct MidiStudioContext<'a> {
    pub lang: Lang,
    pub state: &'a mut MidiStudioState,
    pub mixer: &'a mut MixerEngine,
    pub mixer_ui: &'a mut MixerUiState,
    pub fallback_output: Option<&'a str>,
    pub switch_to_mixer: &'a mut bool,
}

pub fn render_midi_studio(ui: &mut egui::Ui, mut ctx: MidiStudioContext<'_>) {
    ensure_session_mixer_bindings(&mut ctx);

    let active_idx = ctx.state.active_session_idx;
    if let Some(session) = ctx.state.sessions.get_mut(active_idx) {
        if session.is_playing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
            if let Some(start) = session.play_start {
                let elapsed = start.elapsed().as_secs_f32();
                let step_sec = 60.0 / (session.bpm.clamp(40.0, 240.0) * 4.0);
                session.current_step = ((elapsed / step_sec) as usize) % 16;
            }
        }
    }

    egui::Frame::new()
        .fill(StudioPalette::BG_DEEP)
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            render_top_session_bar(ui, &mut ctx);
            ui.add_space(8.0);
            render_sequencer_rack(ui, &mut ctx);
            ui.add_space(8.0);
            render_synth_and_keyboard(ui, &mut ctx);
        });
}

fn ensure_session_mixer_bindings(ctx: &mut MidiStudioContext<'_>) {
    let mut needs_sync_indices = Vec::new();

    for (s_idx, session) in ctx.state.sessions.iter_mut().enumerate() {
        let ch_idx = session.channel_index;
        let is_valid = match ch_idx {
            Some(i) => i < ctx.mixer.channels().len() && ctx.mixer.channel(i).is_some(),
            None => false,
        };

        if !is_valid {
            let existing_idx = ctx.mixer.channels().iter().position(|c| c.name == session.name);
            let target_idx = match existing_idx {
                Some(idx) => {
                    ctx.mixer.set_channel_multimedia(idx);
                    idx
                }
                None => {
                    if let Some(new_ch) = ctx.mixer.add_midi_channel(&session.name) {
                        new_ch
                    } else {
                        0
                    }
                }
            };
            session.channel_index = Some(target_idx);
            needs_sync_indices.push(s_idx);
        } else if session.edit_version != session.last_synced_version {
            needs_sync_indices.push(s_idx);
        }
    }

    for s_idx in needs_sync_indices {
        let sr = ctx.mixer.sample_rate();
        let fallback = ctx.fallback_output.map(|s| s.to_string());
        if let Some(session) = ctx.state.sessions.get_mut(s_idx) {
            let buf = synthesize_session_pattern(session, sr);
            if let Some(ch) = session.channel_index {
                let _ = ctx.mixer.install_custom_media_buffer(
                    ch,
                    session.name.clone(),
                    buf,
                    session.is_playing,
                    fallback.as_deref(),
                );
                session.last_synced_version = session.edit_version;
            }
        }
    }
}

fn render_top_session_bar(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    egui::Frame::new()
        .fill(Color32::from_rgb(18, 19, 23))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 40, 48)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("MIDI SEQUENCER")
                        .strong()
                        .color(Color32::from_rgb(240, 160, 120))
                        .size(13.0),
                );
                ui.add_space(10.0);

                let session_count = ctx.state.sessions.len();
                let mut switch_to = None;
                for (idx, sess) in ctx.state.sessions.iter().enumerate() {
                    let active = idx == ctx.state.active_session_idx;
                    let (bg, border, text_color) = if active {
                        (Color32::from_rgb(38, 42, 52), Color32::from_rgb(240, 160, 120), Color32::WHITE)
                    } else {
                        (Color32::from_rgb(24, 25, 30), Color32::from_rgb(45, 48, 56), Color32::from_rgb(170, 175, 185))
                    };

                    let btn = egui::Button::new(RichText::new(&sess.name).strong().color(text_color))
                        .fill(bg)
                        .stroke(Stroke::new(1.0_f32, border))
                        .corner_radius(CornerRadius::same(4));
                    if ui.add(btn).clicked() {
                        switch_to = Some(idx);
                    }
                }
                if let Some(to) = switch_to {
                    ctx.state.active_session_idx = to;
                }

                if ui.button(RichText::new("+ Nueva Sesión").color(StudioPalette::TEXT_PRIMARY)).clicked() {
                    let new_idx = ctx.state.add_new_session();
                    let name = ctx.state.sessions[new_idx].name.clone();
                    if let Some(ch) = ctx.mixer.add_midi_channel(&name) {
                        ctx.state.sessions[new_idx].channel_index = Some(ch);
                    }
                }

                if session_count > 1 {
                    if ui.button(RichText::new("Eliminar").color(Color32::from_rgb(215, 60, 60))).clicked() {
                        ctx.state.remove_active_session();
                    }
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(sess) = ctx.state.sessions.get(ctx.state.active_session_idx) {
                        if let Some(ch) = sess.channel_index {
                            let label = format!("Canal Mixer: CH {:02} [{}]", ch + 1, sess.name);
                            if ui.button(RichText::new(format!("{label} -> Ir al Mixer")).strong().color(Color32::from_rgb(90, 180, 255))).clicked() {
                                ctx.mixer_ui.selection = WingSelection::Channel(ch);
                                *ctx.switch_to_mixer = true;
                            }
                        }
                    }
                });
            });
        });
}

fn render_sequencer_rack(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;
    let sr = ctx.mixer.sample_rate();
    let fallback = ctx.fallback_output.map(|s| s.to_string());

    egui::Frame::new()
        .fill(Color32::from_rgb(22, 23, 27))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 46, 54)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(12))
        .show(ui, |ui| {
            let mut manual_sync = false;
            let mut toggle_play = false;

            if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                ui.horizontal(|ui| {
                    let play_text = if sess.is_playing { "⏹ Detener" } else { "▶ Reproducir Loop" };
                    let play_color = if sess.is_playing { Color32::from_rgb(230, 80, 80) } else { Color32::from_rgb(70, 200, 120) };
                    if ui.add(egui::Button::new(RichText::new(play_text).strong().color(Color32::WHITE)).fill(play_color).corner_radius(CornerRadius::same(4))).clicked() {
                        toggle_play = true;
                    }

                    ui.add_space(12.0);
                    ui.label(RichText::new("Tempo:").color(StudioPalette::TEXT_SECONDARY));
                    let old_bpm = sess.bpm;
                    ui.add(egui::DragValue::new(&mut sess.bpm).range(40.0..=240.0).speed(0.5).suffix(" BPM"));
                    if (old_bpm - sess.bpm).abs() > 0.01 {
                        sess.mark_edited();
                    }

                    ui.add_space(8.0);
                    ui.label(RichText::new("Swing:").color(StudioPalette::TEXT_SECONDARY));
                    let old_sw = sess.swing;
                    ui.add(egui::Slider::new(&mut sess.swing, 0.0..=0.6).show_value(false));
                    if (old_sw - sess.swing).abs() > 0.001 {
                        sess.mark_edited();
                    }

                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(32, 34, 40))
                        .corner_radius(CornerRadius::same(3))
                        .inner_margin(Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(RichText::new("4/4").strong().monospace().color(Color32::WHITE));
                        });

                    ui.add_space(8.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgb(32, 34, 40))
                        .corner_radius(CornerRadius::same(3))
                        .inner_margin(Margin::symmetric(6, 2))
                        .show(ui, |ui| {
                            ui.label(RichText::new("16 STEPS").strong().monospace().color(Color32::from_rgb(180, 185, 195)));
                        });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("Sincronizar con Mixer").strong().color(Color32::from_rgb(240, 160, 120))).clicked() {
                            manual_sync = true;
                        }
                    });
                });

                ui.add_space(10.0);
                ui.separator();
                ui.add_space(6.0);

                let current_step = sess.current_step;
                let is_playing = sess.is_playing;

                egui::ScrollArea::horizontal()
                    .id_salt("fl_step_sequencer_scroll")
                    .show(ui, |ui| {
                        for track_idx in 0..sess.tracks.len() {
                            let track = &mut sess.tracks[track_idx];
                            ui.horizontal(|ui| {
                                let (mute_bg, mute_fg) = if track.mute {
                                    (Color32::from_rgb(160, 40, 40), Color32::WHITE)
                                } else {
                                    (Color32::from_rgb(40, 140, 70), Color32::WHITE)
                                };
                                if ui.add(egui::Button::new(RichText::new("M").small().strong().color(mute_fg)).fill(mute_bg).corner_radius(CornerRadius::same(3))).clicked() {
                                    track.mute = !track.mute;
                                }

                                let (solo_bg, solo_fg) = if track.solo {
                                    (Color32::from_rgb(200, 150, 40), Color32::WHITE)
                                } else {
                                    (Color32::from_rgb(34, 36, 42), StudioPalette::TEXT_MUTED)
                                };
                                if ui.add(egui::Button::new(RichText::new("S").small().strong().color(solo_fg)).fill(solo_bg).corner_radius(CornerRadius::same(3))).clicked() {
                                    track.solo = !track.solo;
                                }

                                ui.add_space(4.0);
                                egui::Frame::new()
                                    .fill(Color32::from_rgb(34, 36, 42))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 54, 64)))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(10, 5))
                                    .show(ui, |ui| {
                                        ui.set_min_width(85.0);
                                        ui.label(RichText::new(&track.name).strong().color(Color32::WHITE));
                                    });

                                ui.add_space(4.0);
                                ui.add(egui::DragValue::new(&mut track.volume).range(0.0..=1.0).speed(0.01).prefix("Vol: "));
                                ui.add(egui::DragValue::new(&mut track.pan).range(-1.0..=1.0).speed(0.02).prefix("Pan: "));

                                ui.add_space(8.0);

                                for step_idx in 0..16 {
                                    let is_active = track.steps[step_idx];
                                    let is_playhead = is_playing && current_step == step_idx;

                                    let is_light_group = (step_idx / 4) % 2 == 0;
                                    let base_btn_color = if is_light_group {
                                        Color32::from_rgb(86, 90, 102)
                                    } else {
                                        Color32::from_rgb(48, 50, 58)
                                    };
                                    let border_color = if is_playhead {
                                        Color32::from_rgb(255, 220, 80)
                                    } else if is_light_group {
                                        Color32::from_rgb(112, 116, 128)
                                    } else {
                                        Color32::from_rgb(68, 70, 80)
                                    };

                                    let btn_rect = ui.allocate_space(Vec2::new(26.0, 30.0)).1;
                                    let resp = ui.interact(btn_rect, ui.id().with(("step_btn", track_idx, step_idx)), Sense::click());
                                    if resp.clicked() {
                                        track.steps[step_idx] = !track.steps[step_idx];
                                    }

                                    ui.painter().rect_filled(btn_rect, CornerRadius::same(4), base_btn_color);
                                    let stroke_w = if is_playhead { 2.0_f32 } else { 1.0_f32 };
                                    ui.painter().rect_stroke(btn_rect, CornerRadius::same(4), Stroke::new(stroke_w, border_color), StrokeKind::Inside);

                                    if is_active {
                                        let pill_rect = Rect::from_center_size(btn_rect.center(), Vec2::new(14.0, 18.0));
                                        let active_led = Color32::from_rgb(245, 175, 150);
                                        ui.painter().rect_filled(pill_rect, CornerRadius::same(3), active_led);
                                        ui.painter().rect_stroke(
                                            pill_rect,
                                            CornerRadius::same(3),
                                            Stroke::new(1.0_f32, Color32::WHITE),
                                            StrokeKind::Inside,
                                        );
                                    }

                                    if is_playhead {
                                        let top_light = Rect::from_min_size(
                                            Pos2::new(btn_rect.min.x + 2.0, btn_rect.min.y + 1.0),
                                            Vec2::new(btn_rect.width() - 4.0, 3.0),
                                        );
                                        ui.painter().rect_filled(top_light, CornerRadius::same(1), Color32::from_rgb(255, 230, 90));
                                    }
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
            }

            if toggle_play {
                if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                    sess.is_playing = !sess.is_playing;
                    if sess.is_playing {
                        sess.play_start = Some(Instant::now());
                    } else {
                        sess.play_start = None;
                        sess.current_step = 0;
                    }
                    sess.mark_edited();
                    manual_sync = true;
                }
            }

            if manual_sync {
                if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                    let buf = synthesize_session_pattern(sess, sr);
                    if let Some(ch) = sess.channel_index {
                        let _ = ctx.mixer.install_custom_media_buffer(
                            ch,
                            sess.name.clone(),
                            buf,
                            sess.is_playing,
                            fallback.as_deref(),
                        );
                        sess.last_synced_version = sess.edit_version;
                    }
                }
            }
        });
}

fn render_synth_and_keyboard(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;

    ui.columns(2, |cols| {
        cols[0].group(|ui| {
            ui.label(RichText::new("SINTETIZADOR & EFECTOS").strong().color(Color32::from_rgb(240, 160, 120)));
            ui.add_space(6.0);

            if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Ataque").small().color(StudioPalette::TEXT_SECONDARY));
                        if ui.add(egui::Slider::new(&mut sess.synth.attack, 0.001..=0.3).show_value(true)).changed() {
                            sess.mark_edited();
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Caída / Rel").small().color(StudioPalette::TEXT_SECONDARY));
                        if ui.add(egui::Slider::new(&mut sess.synth.release, 0.02..=1.5).show_value(true)).changed() {
                            sess.mark_edited();
                        }
                    });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Filtro Cutoff").small().color(StudioPalette::TEXT_SECONDARY));
                        if ui.add(egui::Slider::new(&mut sess.synth.cutoff, 100.0..=10000.0).logarithmic(true).suffix(" Hz")).changed() {
                            sess.mark_edited();
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Resonancia").small().color(StudioPalette::TEXT_SECONDARY));
                        if ui.add(egui::Slider::new(&mut sess.synth.resonance, 0.0..=0.9)).changed() {
                            sess.mark_edited();
                        }
                    });
                });

                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Drive / Saturación").small().color(StudioPalette::TEXT_SECONDARY));
                        if ui.add(egui::Slider::new(&mut sess.synth.drive, 0.0..=1.5)).changed() {
                            sess.mark_edited();
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(RichText::new("Delay Envío").small().color(StudioPalette::TEXT_SECONDARY));
                        if ui.add(egui::Slider::new(&mut sess.synth.delay_send, 0.0..=0.8)).changed() {
                            sess.mark_edited();
                        }
                    });
                });
            }
        });

        cols[1].group(|ui| {
            ui.label(RichText::new("TECLADO VIRTUAL INTERACTIVO").strong().color(Color32::from_rgb(240, 160, 120)));
            ui.add_space(4.0);
            ui.label(
                RichText::new("Toca con clic o con teclado físico [A, W, S, E, D, F, T, G, Y, H, U, J, K]")
                    .small()
                    .color(StudioPalette::TEXT_MUTED),
            );
            ui.add_space(6.0);

            render_piano_keyboard(ui, ctx);
        });
    });
}

fn render_piano_keyboard(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let white_notes = [
        ("C3", "A", 48),
        ("D3", "S", 50),
        ("E3", "D", 52),
        ("F3", "F", 53),
        ("G3", "G", 55),
        ("A3", "H", 57),
        ("B3", "J", 59),
        ("C4", "K", 60),
        ("D4", "L", 62),
        ("E4", "Ñ", 64),
    ];

    let black_notes = [
        ("C#3", "W", 49, 0),
        ("D#3", "E", 51, 1),
        ("F#3", "T", 54, 3),
        ("G#3", "Y", 56, 4),
        ("A#3", "U", 58, 5),
        ("C#4", "O", 61, 7),
        ("D#4", "P", 63, 8),
    ];

    let key_width = 30.0;
    let key_height = 80.0;
    let total_width = white_notes.len() as f32 * (key_width + 2.0);

    let (kb_rect, _) = ui.allocate_exact_size(Vec2::new(total_width, key_height), Sense::hover());
    let mut pressed_note: Option<(usize, &'static str)> = None;

    for (i, (name, key_hint, midi_num)) in white_notes.iter().enumerate() {
        let x = kb_rect.min.x + i as f32 * (key_width + 2.0);
        let key_r = Rect::from_min_size(Pos2::new(x, kb_rect.min.y), Vec2::new(key_width, key_height));

        let key_id = ui.id().with(("white_key", *midi_num));
        let resp = ui.interact(key_r, key_id, Sense::click_and_drag());
        let is_down = resp.is_pointer_button_down_on();

        let bg = if is_down {
            Color32::from_rgb(240, 160, 120)
        } else {
            Color32::from_rgb(220, 224, 230)
        };

        ui.painter().rect_filled(key_r, CornerRadius::same(3), bg);
        ui.painter().rect_stroke(key_r, CornerRadius::same(3), Stroke::new(1.0_f32, Color32::from_rgb(120, 124, 134)), StrokeKind::Inside);

        let text_col = if is_down { Color32::WHITE } else { Color32::from_rgb(40, 42, 48) };
        ui.painter().text(
            Pos2::new(key_r.center().x, key_r.max.y - 18.0),
            Align2::CENTER_CENTER,
            *name,
            FontId::proportional(10.0),
            text_col,
        );
        ui.painter().text(
            Pos2::new(key_r.center().x, key_r.max.y - 8.0),
            Align2::CENTER_CENTER,
            *key_hint,
            FontId::monospace(9.0),
            Color32::from_rgb(110, 114, 124),
        );

        if is_down {
            pressed_note = Some((*midi_num, *name));
        }
    }

    let black_w = 18.0;
    let black_h = 48.0;
    for (name, key_hint, midi_num, white_slot) in &black_notes {
        let center_x = kb_rect.min.x + *white_slot as f32 * (key_width + 2.0) + key_width + 1.0;
        let b_rect = Rect::from_min_size(Pos2::new(center_x - black_w * 0.5, kb_rect.min.y), Vec2::new(black_w, black_h));

        let key_id = ui.id().with(("black_key", *midi_num));
        let resp = ui.interact(b_rect, key_id, Sense::click_and_drag());
        let is_down = resp.is_pointer_button_down_on();

        let bg = if is_down {
            Color32::from_rgb(240, 160, 120)
        } else {
            Color32::from_rgb(24, 25, 30)
        };

        ui.painter().rect_filled(b_rect, CornerRadius::same(2), bg);
        ui.painter().rect_stroke(b_rect, CornerRadius::same(2), Stroke::new(1.0_f32, Color32::from_rgb(70, 72, 80)), StrokeKind::Inside);

        ui.painter().text(
            Pos2::new(b_rect.center().x, b_rect.max.y - 10.0),
            Align2::CENTER_CENTER,
            *key_hint,
            FontId::monospace(8.0),
            Color32::from_rgb(180, 184, 194),
        );

        if is_down {
            pressed_note = Some((*midi_num, *name));
        }
    }

    if let Some((midi_num, _name)) = pressed_note {
        ctx.state.active_piano_key = Some(midi_num);
        let active_s_idx = ctx.state.active_session_idx;
        if let Some(sess) = ctx.state.sessions.get_mut(active_s_idx) {
            let semi = midi_num as i8 - 48;
            if let Some(lead) = sess.tracks.iter_mut().find(|t| t.sound == TrackSound::SynthLead) {
                lead.pitch_semitones = semi;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_midi_studio_state_sessions_and_synthesis() {
        let mut state = MidiStudioState::new();
        assert_eq!(state.sessions.len(), 1);
        assert_eq!(state.sessions[0].name, "MIDI #1");
        assert_eq!(state.sessions[0].tracks.len(), 5);

        let buf = synthesize_session_pattern(&state.sessions[0], 48000);
        assert_eq!(buf.sample_rate, 48000);
        assert_eq!(buf.total_frames, 96000);
        assert_eq!(buf.interleaved_stereo.len(), 192000);
        assert!(buf.interleaved_stereo.iter().any(|&s| s.abs() > 0.01));

        let new_idx = state.add_new_session();
        assert_eq!(new_idx, 1);
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.sessions[1].name, "MIDI #2");
    }
}
