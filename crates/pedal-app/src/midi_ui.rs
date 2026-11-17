use std::{
    f32::consts::PI,
    time::Instant,
};

use eframe::egui::{
    self, Align2, Color32, CornerRadius, FontId, Key, Margin, Pos2, Rect, RichText, Sense, Stroke,
    StrokeKind, Vec2,
};
use pedal_engine::{MediaBuffer, MixerEngine};

use crate::{
    i18n::Lang,
    mixer_ui::{MixerUiState, WingSelection},
    theme::StudioPalette,
};

pub const PIANO_ROLL_STEPS: usize = 16;
pub const PIANO_MIN_MIDI: u8 = 48;
pub const PIANO_MAX_MIDI: u8 = 71;
pub const PIANO_NOTE_COUNT: usize = (PIANO_MAX_MIDI - PIANO_MIN_MIDI + 1) as usize;

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SynthPreset {
    AnalogLead,
    FmKeys,
    WarmPad,
    PluckSynth,
    SubBass,
}

impl SynthPreset {
    pub fn all() -> &'static [SynthPreset] {
        &[
            Self::AnalogLead,
            Self::FmKeys,
            Self::WarmPad,
            Self::PluckSynth,
            Self::SubBass,
        ]
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::AnalogLead => "FL Keys / Analog Lead",
            Self::FmKeys => "Electric Piano FM",
            Self::WarmPad => "Warm Saw Pad",
            Self::PluckSynth => "Pluck Arp",
            Self::SubBass => "808 Sub Bass",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PianoNote {
    pub midi: u8,
    pub start_step: usize,
    pub length_steps: usize,
    pub velocity: f32,
}

#[derive(Debug, Clone)]
pub struct SequencerTrack {
    pub name: String,
    pub sound: TrackSound,
    pub steps: [bool; PIANO_ROLL_STEPS],
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
            steps: [false; PIANO_ROLL_STEPS],
            volume: 0.80,
            pan: 0.0,
            pitch_semitones: 0,
            mute: false,
            solo: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct SynthParams {
    pub preset: SynthPreset,
    pub attack: f32,
    pub release: f32,
    pub cutoff: f32,
    pub resonance: f32,
    pub drive: f32,
    pub delay_send: f32,
    pub delay_time_ms: f32,
    pub default_note_len: usize,
}

impl Default for SynthParams {
    fn default() -> Self {
        Self {
            preset: SynthPreset::AnalogLead,
            attack: 0.008,
            release: 0.32,
            cutoff: 5200.0,
            resonance: 0.30,
            drive: 0.20,
            delay_send: 0.18,
            delay_time_ms: 240.0,
            default_note_len: 1,
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
    pub notes: Vec<PianoNote>,
    pub synth: SynthParams,
    pub is_playing: bool,
    pub is_recording: bool,
    pub drums_enabled: bool,
    pub play_start: Option<Instant>,
    pub playhead_pos_steps: f32,
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
        for i in (0..PIANO_ROLL_STEPS).step_by(2) {
            hat.steps[i] = true;
        }

        let mut snare = SequencerTrack::new("Snare", TrackSound::Snare);
        snare.steps[4] = true;
        snare.steps[12] = true;

        let mut bass = SequencerTrack::new("Bass 808", TrackSound::SynthBass);
        bass.steps[0] = true;
        bass.steps[6] = true;
        bass.steps[10] = true;

        let default_notes = vec![
            PianoNote { midi: 60, start_step: 0, length_steps: 2, velocity: 0.90 },
            PianoNote { midi: 64, start_step: 2, length_steps: 2, velocity: 0.85 },
            PianoNote { midi: 67, start_step: 4, length_steps: 2, velocity: 0.90 },
            PianoNote { midi: 71, start_step: 6, length_steps: 2, velocity: 0.85 },
            PianoNote { midi: 69, start_step: 8, length_steps: 2, velocity: 0.90 },
            PianoNote { midi: 67, start_step: 10, length_steps: 2, velocity: 0.85 },
            PianoNote { midi: 64, start_step: 12, length_steps: 2, velocity: 0.88 },
            PianoNote { midi: 62, start_step: 14, length_steps: 2, velocity: 0.84 },
            PianoNote { midi: 48, start_step: 0, length_steps: 4, velocity: 0.78 },
            PianoNote { midi: 55, start_step: 4, length_steps: 4, velocity: 0.76 },
            PianoNote { midi: 57, start_step: 8, length_steps: 4, velocity: 0.78 },
            PianoNote { midi: 53, start_step: 12, length_steps: 4, velocity: 0.76 },
        ];

        Self {
            id,
            name,
            channel_index: None,
            bpm: 124.0,
            swing: 0.0,
            tracks: vec![kick, clap, hat, snare, bass],
            notes: default_notes,
            synth: SynthParams::default(),
            is_playing: false,
            is_recording: false,
            drums_enabled: true,
            play_start: None,
            playhead_pos_steps: 0.0,
            current_step: 0,
            last_synced_version: 0,
            edit_version: 1,
        }
    }

    pub fn mark_edited(&mut self) {
        self.edit_version = self.edit_version.wrapping_add(1);
    }

    pub fn toggle_or_set_note(&mut self, midi: u8, step: usize, len: usize) -> bool {
        if let Some(pos) = self.notes.iter().position(|n| {
            n.midi == midi && step >= n.start_step && step < n.start_step + n.length_steps.max(1)
        }) {
            self.notes.remove(pos);
            self.mark_edited();
            false
        } else {
            let clamped_len = len.clamp(1, (PIANO_ROLL_STEPS - step).max(1));
            self.notes.retain(|n| {
                !(n.midi == midi && n.start_step >= step && n.start_step < step + clamped_len)
            });
            self.notes.push(PianoNote {
                midi,
                start_step: step,
                length_steps: clamped_len,
                velocity: 0.88,
            });
            self.mark_edited();
            true
        }
    }

    pub fn load_demo_melody(&mut self) {
        self.notes = vec![
            PianoNote { midi: 60, start_step: 0, length_steps: 2, velocity: 0.90 },
            PianoNote { midi: 64, start_step: 2, length_steps: 2, velocity: 0.85 },
            PianoNote { midi: 67, start_step: 4, length_steps: 2, velocity: 0.90 },
            PianoNote { midi: 71, start_step: 6, length_steps: 2, velocity: 0.85 },
            PianoNote { midi: 69, start_step: 8, length_steps: 2, velocity: 0.90 },
            PianoNote { midi: 67, start_step: 10, length_steps: 2, velocity: 0.85 },
            PianoNote { midi: 64, start_step: 12, length_steps: 2, velocity: 0.88 },
            PianoNote { midi: 62, start_step: 14, length_steps: 2, velocity: 0.84 },
            PianoNote { midi: 48, start_step: 0, length_steps: 4, velocity: 0.78 },
            PianoNote { midi: 55, start_step: 4, length_steps: 4, velocity: 0.76 },
            PianoNote { midi: 57, start_step: 8, length_steps: 4, velocity: 0.78 },
            PianoNote { midi: 53, start_step: 12, length_steps: 4, velocity: 0.76 },
        ];
        self.mark_edited();
    }
}

pub struct MidiStudioState {
    pub sessions: Vec<MidiSession>,
    pub active_session_idx: usize,
    pub next_session_id: usize,
    pub active_piano_key: Option<usize>,
    pub last_triggered_key: Option<u8>,
    pub status_message: Option<String>,
    pub last_render_instant: Instant,
    pub show_drum_rack: bool,
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
            last_triggered_key: None,
            status_message: None,
            last_render_instant: Instant::now(),
            show_drum_rack: true,
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

pub fn midi_note_to_freq(midi: u8) -> f32 {
    440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0)
}

pub fn midi_note_name(midi: u8) -> String {
    let names = ["C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B"];
    let octave = (midi / 12) as i32 - 1;
    let name = names[(midi % 12) as usize];
    format!("{name}{octave}")
}

pub fn is_black_midi_key(midi: u8) -> bool {
    matches!(midi % 12, 1 | 3 | 6 | 8 | 10)
}

fn synth_voice_sample(preset: SynthPreset, freq: f32, t: f32, hold_dur: f32, synth: &SynthParams) -> f32 {
    let att = synth.attack.clamp(0.002, 0.4);
    let rel = synth.release.clamp(0.03, 1.5);
    let env = if t < att {
        t / att
    } else if t < hold_dur {
        let decay = (-((t - att) * 2.2)).exp();
        0.68 + 0.32 * decay
    } else {
        let after = t - hold_dur;
        0.68 * (-(after / rel)).exp()
    };

    let cut_norm = (synth.cutoff / 8000.0).clamp(0.15, 1.4);
    let raw = match preset {
        SynthPreset::AnalogLead => {
            let saw1 = 2.0 * (t * freq).fract() - 1.0;
            let saw2 = 2.0 * (t * freq * 1.006).fract() - 1.0;
            let sub = (2.0 * PI * (freq * 0.5) * t).sin();
            (saw1 * 0.38 + saw2 * 0.38) * cut_norm + sub * 0.28
        }
        SynthPreset::FmKeys => {
            let mod_idx = 2.2 * (-t * 6.0).exp() * cut_norm;
            let modulator = (2.0 * PI * (freq * 2.0) * t).sin() * mod_idx;
            let carrier = (2.0 * PI * freq * t + modulator).sin();
            let tine = (2.0 * PI * (freq * 6.0) * t).sin() * (-t * 24.0).exp() * 0.22;
            carrier * 0.78 + tine
        }
        SynthPreset::WarmPad => {
            let det1 = 2.0 * (t * freq * 0.996).fract() - 1.0;
            let det2 = 2.0 * (t * freq * 1.004).fract() - 1.0;
            let fund = (2.0 * PI * freq * t).sin();
            (det1 * 0.30 + det2 * 0.30) * cut_norm * 0.7 + fund * 0.45
        }
        SynthPreset::PluckSynth => {
            let pluck_env = (-t * 14.0).exp();
            let sq = if (t * freq).fract() < 0.5 { 0.7 } else { -0.7 };
            let harm = (2.0 * PI * (freq * 3.0) * t).sin() * pluck_env * 0.4;
            let body = (2.0 * PI * freq * t).sin() * 0.5;
            (sq * pluck_env * cut_norm + harm + body) * 0.85
        }
        SynthPreset::SubBass => {
            let pitch_drop = 1.0 + 0.35 * (-t * 28.0).exp();
            let f = freq * 0.5 * pitch_drop;
            let s1 = (2.0 * PI * f * t).sin() * 0.82;
            let s2 = (2.0 * PI * (f * 2.0) * t).sin() * 0.22 * cut_norm;
            (s1 + s2 * (1.0 + synth.drive)).tanh()
        }
    };

    (raw * (1.0 + synth.drive * 0.9)).tanh() * env * 0.48
}

pub fn synthesize_single_note_preview(midi: u8, synth: &SynthParams, sample_rate: u32) -> MediaBuffer {
    let sr = sample_rate.max(22050) as f32;
    let hold_dur = 0.22_f32;
    let total_dur = (hold_dur + synth.release.clamp(0.05, 0.8)).min(0.85);
    let total_frames = (total_dur * sr).round() as usize;
    let freq = midi_note_to_freq(midi);

    let mut stereo = vec![0.0_f32; total_frames * 2];
    for f in 0..total_frames {
        let t = f as f32 / sr;
        let s = synth_voice_sample(synth.preset, freq, t, hold_dur, synth).clamp(-0.95, 0.95);
        stereo[f * 2] = s;
        stereo[f * 2 + 1] = s;
    }

    MediaBuffer {
        interleaved_stereo: stereo,
        total_frames,
        sample_rate,
        title: format!("Note {}", midi_note_name(midi)),
    }
}

pub fn synthesize_session_pattern(session: &MidiSession, sample_rate: u32) -> MediaBuffer {
    let sr = sample_rate.max(22050) as f32;
    let bpm = session.bpm.clamp(40.0, 240.0);
    let step_duration = 60.0 / (bpm * 4.0);
    let total_secs = step_duration * PIANO_ROLL_STEPS as f32;
    let total_frames = (total_secs * sr).round() as usize;

    let mut stereo: Vec<f32> = vec![0.0; total_frames * 2];

    for note in &session.notes {
        if note.start_step >= PIANO_ROLL_STEPS {
            continue;
        }
        let mut step_start_sec = note.start_step as f32 * step_duration;
        if note.start_step % 2 == 1 && session.swing > 0.001 {
            step_start_sec += session.swing.clamp(0.0, 0.6) * step_duration * 0.45;
        }
        let hold_dur = (note.length_steps.max(1) as f32 * step_duration).max(0.04);
        let tail_dur = session.synth.release.clamp(0.04, 0.8);
        let note_total_frames = ((hold_dur + tail_dur) * sr) as usize;
        let start_frame = (step_start_sec * sr) as usize;
        let freq = midi_note_to_freq(note.midi);
        let vel = note.velocity.clamp(0.1, 1.0);

        for f in 0..note_total_frames {
            let frame_idx = (start_frame + f) % total_frames.max(1);
            let t = f as f32 / sr;
            let smp = synth_voice_sample(session.synth.preset, freq, t, hold_dur, &session.synth) * vel;
            stereo[frame_idx * 2] += smp;
            stereo[frame_idx * 2 + 1] += smp;
        }
    }

    if session.drums_enabled {
        let any_solo = session.tracks.iter().any(|t| t.solo);

        for track in &session.tracks {
            if track.mute || (any_solo && !track.solo) {
                continue;
            }

            let left_gain = track.volume * (1.0 - track.pan).clamp(0.0, 1.0);
            let right_gain = track.volume * (1.0 + track.pan).clamp(0.0, 1.0);

            for step in 0..PIANO_ROLL_STEPS {
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
                            phase.sin() * env * 0.85
                        }
                        TrackSound::Clap => {
                            let tap1 = if t < 0.012 { 0.6 } else { 0.0 };
                            let tap2 = if (0.012..0.024).contains(&t) { 0.8 } else { 0.0 };
                            let tap3 = if (0.024..0.038).contains(&t) { 0.9 } else { 0.0 };
                            let noise_env = (-t * 26.0).exp();
                            let hash = ((f as f32 * 19.371 + t * 43.1).sin() * 43758.5453).fract();
                            let noise = (hash * 2.0 - 1.0) * noise_env;
                            (tap1 + tap2 + tap3 + noise) * 0.38
                        }
                        TrackSound::Hat => {
                            let env = (-t * 70.0).exp();
                            let hash = ((f as f32 * 31.713 + t * 91.2).sin() * 23421.631).fract();
                            let tone = (2.0 * PI * 8200.0 * t).sin() * 0.3
                                + (2.0 * PI * 11400.0 * t).sin() * 0.2;
                            (hash * 2.0 - 1.0 + tone) * env * 0.32
                        }
                        TrackSound::Snare => {
                            let body_env = (-t * 28.0).exp();
                            let body = (2.0 * PI * 190.0 * t).sin() * body_env * 0.55;
                            let snare_env = (-t * 24.0).exp();
                            let hash = ((f as f32 * 17.519 + t * 67.3).sin() * 37421.123).fract();
                            let noise = (hash * 2.0 - 1.0) * snare_env * 0.45;
                            body + noise
                        }
                        TrackSound::SynthLead => {
                            let freq = 261.63 * 2.0_f32.powf(track.pitch_semitones as f32 / 12.0);
                            synth_voice_sample(session.synth.preset, freq, t, 0.15, &session.synth)
                        }
                        TrackSound::SynthBass => {
                            let freq = 65.41 * 2.0_f32.powf(track.pitch_semitones as f32 / 12.0);
                            let env = (-t * 6.5).exp();
                            let wave = (2.0 * PI * freq * t).sin() * 0.75
                                + (2.0 * PI * (freq * 2.0) * t).sin() * 0.25;
                            (wave * (1.0 + session.synth.drive)).tanh() * env * 0.65
                        }
                    };

                    let drive_smp = (smp * (1.0 + session.synth.drive * 0.6)).tanh();
                    stereo[frame_idx * 2] += drive_smp * left_gain;
                    stereo[frame_idx * 2 + 1] += drive_smp * right_gain;
                }
            }
        }
    }

    if session.synth.delay_send > 0.01 {
        let delay_frames = ((session.synth.delay_time_ms * 0.001 * sr).round() as usize).max(1);
        let fb = 0.36;
        let send = session.synth.delay_send.clamp(0.0, 1.0);

        for i in delay_frames..total_frames {
            let src_l = stereo[(i - delay_frames) * 2];
            let src_r = stereo[(i - delay_frames) * 2 + 1];
            stereo[i * 2] += src_r * send * fb;
            stereo[i * 2 + 1] += src_l * send * fb;
        }
    }

    for smp in &mut stereo {
        *smp = smp.clamp(-0.98, 0.98);
    }

    MediaBuffer {
        interleaved_stereo: stereo,
        total_frames,
        sample_rate,
        title: format!("{} - Piano Roll & Sequencer", session.name),
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
    update_playhead_state(ui, &mut ctx);

    egui::Frame::new()
        .fill(StudioPalette::BG_DEEP)
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            egui::ScrollArea::vertical()
                .id_salt("midi_studio_main_scroll")
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    render_top_session_and_transport_bar(ui, &mut ctx);
                    ui.add_space(6.0);
                    render_piano_roll_matrix(ui, &mut ctx);
                    ui.add_space(6.0);
                    if ctx.state.show_drum_rack {
                        render_sequencer_rack(ui, &mut ctx);
                        ui.add_space(6.0);
                    }
                    render_synth_and_keyboard(ui, &mut ctx);
                });
        });

    ensure_session_mixer_bindings(&mut ctx);
}

fn update_playhead_state(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;
    if let Some(session) = ctx.state.sessions.get_mut(active_idx) {
        if session.is_playing {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(16));
            let mut updated_from_mixer = false;
            if let Some(ch_idx) = session.channel_index {
                if let Some(ch) = ctx.mixer.channel(ch_idx) {
                    if ch.media.playing && ch.media.total_frames > 0 {
                        let norm = (ch.media.current_frame as f32) / (ch.media.total_frames as f32);
                        session.playhead_pos_steps = (norm * PIANO_ROLL_STEPS as f32)
                            .rem_euclid(PIANO_ROLL_STEPS as f32);
                        session.current_step =
                            (session.playhead_pos_steps.floor() as usize) % PIANO_ROLL_STEPS;
                        updated_from_mixer = true;
                    }
                }
            }
            if !updated_from_mixer {
                let start = *session.play_start.get_or_insert_with(Instant::now);
                let elapsed = start.elapsed().as_secs_f32();
                let step_sec = 60.0 / (session.bpm.clamp(40.0, 240.0) * 4.0);
                session.playhead_pos_steps =
                    (elapsed / step_sec).rem_euclid(PIANO_ROLL_STEPS as f32);
                session.current_step =
                    (session.playhead_pos_steps.floor() as usize) % PIANO_ROLL_STEPS;
            }
        }
    }
}

fn trigger_live_midi_note(ctx: &mut MidiStudioContext<'_>, midi: u8, record_if_armed: bool) {
    let active_idx = ctx.state.active_session_idx;
    let sr = ctx.mixer.sample_rate();
    let fallback = ctx.fallback_output.map(|s| s.to_string());

    let Some(sess) = ctx.state.sessions.get_mut(active_idx) else {
        return;
    };
    let preview_buf = synthesize_single_note_preview(midi, &sess.synth, sr);
    let ch_opt = sess.channel_index;

    if record_if_armed && sess.is_recording && sess.is_playing {
        let step = sess.current_step % PIANO_ROLL_STEPS;
        let len = sess.synth.default_note_len.clamp(1, 4);
        sess.notes.retain(|n| !(n.midi == midi && n.start_step == step));
        sess.notes.push(PianoNote {
            midi,
            start_step: step,
            length_steps: len.min(PIANO_ROLL_STEPS - step),
            velocity: 0.90,
        });
        sess.mark_edited();
    }

    if let Some(ch) = ch_opt {
        let _ = ctx
            .mixer
            .trigger_channel_preview(ch, preview_buf, fallback.as_deref());
    }
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

fn render_top_session_and_transport_bar(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;
    let sr = ctx.mixer.sample_rate();
    let fallback = ctx.fallback_output.map(|s| s.to_string());

    let mut toggle_play = false;
    let mut stop_play = false;

    if ui.input(|i| i.key_pressed(Key::Space)) && !ui.ctx().egui_wants_keyboard_input() {
        toggle_play = true;
    }

    egui::Frame::new()
        .fill(Color32::from_rgb(18, 19, 23))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(38, 40, 48)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::symmetric(12, 8))
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(
                    RichText::new("PIANO ROLL & MIDI DAW")
                        .strong()
                        .color(Color32::from_rgb(240, 160, 120))
                        .size(13.0),
                );
                ui.add_space(6.0);

                let session_count = ctx.state.sessions.len();
                let mut switch_to = None;
                for (idx, sess) in ctx.state.sessions.iter().enumerate() {
                    let active = idx == ctx.state.active_session_idx;
                    let (bg, border, text_color) = if active {
                        (
                            Color32::from_rgb(38, 42, 52),
                            Color32::from_rgb(240, 160, 120),
                            Color32::WHITE,
                        )
                    } else {
                        (
                            Color32::from_rgb(24, 25, 30),
                            Color32::from_rgb(45, 48, 56),
                            Color32::from_rgb(170, 175, 185),
                        )
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

                if ui
                    .button(RichText::new("+ Sesión").color(StudioPalette::TEXT_PRIMARY))
                    .clicked()
                {
                    let new_idx = ctx.state.add_new_session();
                    let name = ctx.state.sessions[new_idx].name.clone();
                    if let Some(ch) = ctx.mixer.add_midi_channel(&name) {
                        ctx.state.sessions[new_idx].channel_index = Some(ch);
                    }
                }

                if session_count > 1
                    && ui
                        .button(RichText::new("Eliminar").color(Color32::from_rgb(215, 60, 60)))
                        .clicked()
                {
                    ctx.state.remove_active_session();
                }

                ui.separator();

                if let Some(sess) = ctx.state.sessions.get_mut(ctx.state.active_session_idx) {
                    let play_label = if sess.is_playing { "⏸ PAUSE" } else { "▶ PLAY" };
                    let play_bg = if sess.is_playing {
                        Color32::from_rgb(235, 155, 50)
                    } else {
                        Color32::from_rgb(55, 185, 105)
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new(play_label).strong().color(Color32::WHITE))
                                .fill(play_bg)
                                .corner_radius(CornerRadius::same(4)),
                        )
                        .clicked()
                    {
                        toggle_play = true;
                    }

                    if ui
                        .add(
                            egui::Button::new(RichText::new("⏹ STOP").strong().color(Color32::WHITE))
                                .fill(Color32::from_rgb(52, 55, 64))
                                .corner_radius(CornerRadius::same(4)),
                        )
                        .clicked()
                    {
                        stop_play = true;
                    }

                    let rec_bg = if sess.is_recording {
                        Color32::from_rgb(220, 45, 45)
                    } else {
                        Color32::from_rgb(42, 44, 52)
                    };
                    if ui
                        .add(
                            egui::Button::new(RichText::new("● REC").strong().color(Color32::WHITE))
                                .fill(rec_bg)
                                .corner_radius(CornerRadius::same(4)),
                        )
                        .clicked()
                    {
                        sess.is_recording = !sess.is_recording;
                    }

                    ui.add_space(6.0);
                    let old_bpm = sess.bpm;
                    ui.add(
                        egui::DragValue::new(&mut sess.bpm)
                            .range(40.0..=240.0)
                            .speed(0.5)
                            .prefix("BPM: "),
                    );
                    if (old_bpm - sess.bpm).abs() > 0.01 {
                        sess.mark_edited();
                    }

                    egui::ComboBox::from_id_salt("piano_roll_preset_combo")
                        .selected_text(sess.synth.preset.label())
                        .width(155.0)
                        .show_ui(ui, |ui| {
                            for &preset in SynthPreset::all() {
                                if ui
                                    .selectable_value(&mut sess.synth.preset, preset, preset.label())
                                    .changed()
                                {
                                    sess.mark_edited();
                                }
                            }
                        });

                    let bar_num = (sess.current_step / 4) + 1;
                    let step_in_bar = (sess.current_step % 4) + 1;
                    egui::Frame::new()
                        .fill(Color32::from_rgb(12, 14, 18))
                        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(48, 52, 62)))
                        .corner_radius(CornerRadius::same(4))
                        .inner_margin(Margin::symmetric(8, 3))
                        .show(ui, |ui| {
                            ui.label(
                                RichText::new(format!("BAR {bar_num} : BEAT {step_in_bar} | STEP {:02}/16", sess.current_step + 1))
                                    .monospace()
                                    .strong()
                                    .color(Color32::from_rgb(255, 200, 90)),
                            );
                        });
                }

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some(sess) = ctx.state.sessions.get(ctx.state.active_session_idx) {
                        if let Some(ch) = sess.channel_index {
                            let label = format!("CH {:02} [{}] -> Mixer", ch + 1, sess.name);
                            if ui
                                .button(
                                    RichText::new(label)
                                        .strong()
                                        .color(Color32::from_rgb(90, 180, 255)),
                                )
                                .clicked()
                            {
                                ctx.mixer_ui.selection = WingSelection::Channel(ch);
                                *ctx.switch_to_mixer = true;
                            }
                        }
                    }
                });
            });
        });

    if toggle_play {
        if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
            sess.is_playing = !sess.is_playing;
            if sess.is_playing {
                sess.play_start = Some(Instant::now());
            } else {
                sess.play_start = None;
            }
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

    if stop_play {
        if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
            sess.is_playing = false;
            sess.play_start = None;
            sess.playhead_pos_steps = 0.0;
            sess.current_step = 0;
            if let Some(ch) = sess.channel_index {
                let _ = ctx
                    .mixer
                    .set_media_transport(ch, false, true, Some(0), fallback.as_deref());
            }
        }
    }
}

fn render_piano_roll_matrix(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;
    let fallback = ctx.fallback_output.map(|s| s.to_string());

    let mut preview_midi_to_trigger: Option<u8> = None;
    let mut seek_to_step: Option<f32> = None;

    egui::Frame::new()
        .fill(Color32::from_rgb(20, 22, 27))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 48, 58)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("MATRIZ PIANO ROLL (Haz clic en la cuadrícula para colocar/quitar notas | Clic en el teclado izquierdo para escuchar)")
                            .small()
                            .strong()
                            .color(Color32::from_rgb(190, 198, 210)),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let drum_btn_text = if ctx.state.show_drum_rack {
                            "Ocultar Batería (Step Rack)"
                        } else {
                            "Mostrar Batería (Step Rack)"
                        };
                        if ui.small_button(drum_btn_text).clicked() {
                            ctx.state.show_drum_rack = !ctx.state.show_drum_rack;
                        }

                        if ui.small_button("Cargar Melodía Demo").clicked() {
                            sess.load_demo_melody();
                        }
                        if ui.small_button("Limpiar Matriz").clicked() {
                            sess.notes.clear();
                            sess.mark_edited();
                        }

                        ui.checkbox(&mut sess.drums_enabled, "Acompañamiento Batería")
                            .changed()
                            .then(|| sess.mark_edited());

                        ui.label(RichText::new("Duración Nota:").small().color(StudioPalette::TEXT_SECONDARY));
                        for len in [1_usize, 2, 4] {
                            let selected = sess.synth.default_note_len == len;
                            if ui.selectable_label(selected, format!("{len}x")).clicked() {
                                sess.synth.default_note_len = len;
                            }
                        }
                    });
                });

                ui.add_space(6.0);

                let piano_key_w = 68.0_f32;
                let row_h = 17.0_f32;
                let ruler_h = 22.0_f32;
                let avail_w = (ui.available_width() - piano_key_w - 6.0).max(480.0);
                let step_w = (avail_w / PIANO_ROLL_STEPS as f32).max(28.0);
                let grid_w = step_w * PIANO_ROLL_STEPS as f32;
                let grid_h = row_h * PIANO_NOTE_COUNT as f32;
                let total_size = Vec2::new(piano_key_w + grid_w, ruler_h + grid_h);

                let (canvas_rect, _) = ui.allocate_exact_size(total_size, Sense::hover());
                let painter = ui.painter_at(canvas_rect);

                let ruler_rect = Rect::from_min_size(
                    Pos2::new(canvas_rect.min.x + piano_key_w, canvas_rect.min.y),
                    Vec2::new(grid_w, ruler_h),
                );
                let corner_rect = Rect::from_min_size(canvas_rect.min, Vec2::new(piano_key_w, ruler_h));
                painter.rect_filled(corner_rect, CornerRadius::ZERO, Color32::from_rgb(24, 26, 32));
                painter.text(
                    corner_rect.center(),
                    Align2::CENTER_CENTER,
                    "TECLADO",
                    FontId::monospace(9.5),
                    Color32::from_rgb(150, 156, 168),
                );

                painter.rect_filled(ruler_rect, CornerRadius::ZERO, Color32::from_rgb(26, 29, 36));
                let ruler_resp = ui.interact(
                    ruler_rect,
                    ui.id().with("piano_roll_ruler"),
                    Sense::click_and_drag(),
                );
                if ruler_resp.clicked() || ruler_resp.dragged() {
                    if let Some(pos) = ruler_resp.interact_pointer_pos() {
                        let rel_x = (pos.x - ruler_rect.min.x).clamp(0.0, grid_w - 0.01);
                        seek_to_step = Some(rel_x / step_w);
                    }
                }

                for step in 0..PIANO_ROLL_STEPS {
                    let x = ruler_rect.min.x + step as f32 * step_w;
                    let is_bar_start = step % 4 == 0;
                    let line_col = if is_bar_start {
                        Color32::from_rgb(95, 102, 118)
                    } else {
                        Color32::from_rgb(48, 52, 62)
                    };
                    painter.line_segment(
                        [Pos2::new(x, ruler_rect.min.y), Pos2::new(x, ruler_rect.max.y)],
                        Stroke::new(1.0_f32, line_col),
                    );
                    let label = if is_bar_start {
                        format!("Compás {} ({})", (step / 4) + 1, step + 1)
                    } else {
                        format!("{}", step + 1)
                    };
                    painter.text(
                        Pos2::new(x + 4.0, ruler_rect.center().y),
                        Align2::LEFT_CENTER,
                        label,
                        FontId::monospace(if is_bar_start { 10.0 } else { 8.5 }),
                        if is_bar_start {
                            Color32::from_rgb(235, 240, 248)
                        } else {
                            Color32::from_rgb(130, 136, 148)
                        },
                    );
                }

                let grid_origin = Pos2::new(canvas_rect.min.x + piano_key_w, canvas_rect.min.y + ruler_h);
                let playhead_step_f = sess.playhead_pos_steps;

                for row in 0..PIANO_NOTE_COUNT {
                    let midi = PIANO_MAX_MIDI - row as u8;
                    let y = grid_origin.y + row as f32 * row_h;
                    let is_black = is_black_midi_key(midi);
                    let is_c = midi % 12 == 0;

                    let key_rect = Rect::from_min_size(
                        Pos2::new(canvas_rect.min.x, y),
                        Vec2::new(piano_key_w, row_h),
                    );
                    let key_resp = ui.interact(
                        key_rect,
                        ui.id().with(("piano_roll_left_key", midi)),
                        Sense::click(),
                    );
                    let key_down = key_resp.is_pointer_button_down_on();
                    if key_resp.clicked() {
                        preview_midi_to_trigger = Some(midi);
                    }

                    let note_playing_now = sess.is_playing
                        && sess.notes.iter().any(|n| {
                            n.midi == midi
                                && playhead_step_f >= n.start_step as f32
                                && playhead_step_f < (n.start_step + n.length_steps.max(1)) as f32
                        });

                    let key_bg = if key_down || note_playing_now || ctx.state.active_piano_key == Some(midi as usize) {
                        Color32::from_rgb(245, 165, 85)
                    } else if is_black {
                        Color32::from_rgb(30, 32, 38)
                    } else {
                        Color32::from_rgb(218, 222, 230)
                    };
                    let key_fg = if key_down || note_playing_now || ctx.state.active_piano_key == Some(midi as usize) {
                        Color32::from_rgb(20, 22, 26)
                    } else if is_black {
                        Color32::from_rgb(200, 205, 215)
                    } else {
                        Color32::from_rgb(32, 34, 40)
                    };

                    painter.rect_filled(key_rect, CornerRadius::ZERO, key_bg);
                    painter.rect_stroke(
                        key_rect,
                        CornerRadius::ZERO,
                        Stroke::new(0.8_f32, Color32::from_rgb(65, 70, 80)),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        Pos2::new(key_rect.max.x - 6.0, key_rect.center().y),
                        Align2::RIGHT_CENTER,
                        midi_note_name(midi),
                        FontId::monospace(if is_c { 10.0 } else { 9.0 }),
                        key_fg,
                    );

                    let row_bg = if is_black {
                        Color32::from_rgb(30, 34, 41)
                    } else if is_c {
                        Color32::from_rgb(44, 50, 60)
                    } else {
                        Color32::from_rgb(37, 42, 50)
                    };
                    let row_rect = Rect::from_min_size(Pos2::new(grid_origin.x, y), Vec2::new(grid_w, row_h));
                    painter.rect_filled(row_rect, CornerRadius::ZERO, row_bg);
                    painter.line_segment(
                        [Pos2::new(grid_origin.x, y + row_h), Pos2::new(grid_origin.x + grid_w, y + row_h)],
                        Stroke::new(0.7_f32, Color32::from_rgb(25, 28, 34)),
                    );

                    for step in 0..PIANO_ROLL_STEPS {
                        let cell_x = grid_origin.x + step as f32 * step_w;
                        let cell_rect = Rect::from_min_size(Pos2::new(cell_x, y), Vec2::new(step_w, row_h));
                        let cell_resp = ui.interact(
                            cell_rect,
                            ui.id().with(("pr_cell", midi, step)),
                            Sense::click(),
                        );
                        if cell_resp.clicked() {
                            let default_len = sess.synth.default_note_len;
                            let added = sess.toggle_or_set_note(midi, step, default_len);
                            if added {
                                preview_midi_to_trigger = Some(midi);
                            }
                        } else if cell_resp.secondary_clicked() {
                            if let Some(pos) = sess.notes.iter().position(|n| {
                                n.midi == midi
                                    && step >= n.start_step
                                    && step < n.start_step + n.length_steps.max(1)
                            }) {
                                sess.notes.remove(pos);
                                sess.mark_edited();
                            }
                        }
                    }
                }

                for step in 0..=PIANO_ROLL_STEPS {
                    let x = grid_origin.x + step as f32 * step_w;
                    let is_bar = step % 4 == 0;
                    let stroke = if is_bar {
                        Stroke::new(1.3_f32, Color32::from_rgb(76, 84, 98))
                    } else {
                        Stroke::new(0.8_f32, Color32::from_rgb(46, 52, 62))
                    };
                    painter.line_segment(
                        [Pos2::new(x, grid_origin.y), Pos2::new(x, grid_origin.y + grid_h)],
                        stroke,
                    );
                }

                for note in &sess.notes {
                    if note.midi < PIANO_MIN_MIDI || note.midi > PIANO_MAX_MIDI {
                        continue;
                    }
                    let row = (PIANO_MAX_MIDI - note.midi) as usize;
                    let x = grid_origin.x + note.start_step as f32 * step_w + 1.0;
                    let y = grid_origin.y + row as f32 * row_h + 1.0;
                    let w = (note.length_steps.max(1) as f32 * step_w - 2.0).max(6.0);
                    let h = row_h - 2.0;
                    let note_rect = Rect::from_min_size(Pos2::new(x, y), Vec2::new(w, h));

                    let is_active_now = sess.is_playing
                        && playhead_step_f >= note.start_step as f32
                        && playhead_step_f < (note.start_step + note.length_steps.max(1)) as f32;

                    let fill_col = if is_active_now {
                        Color32::from_rgb(200, 255, 195)
                    } else {
                        Color32::from_rgb(138, 218, 148)
                    };
                    let border_col = if is_active_now {
                        Color32::WHITE
                    } else {
                        Color32::from_rgb(70, 140, 80)
                    };

                    painter.rect_filled(note_rect, CornerRadius::same(3), fill_col);
                    painter.rect_stroke(
                        note_rect,
                        CornerRadius::same(3),
                        Stroke::new(1.2_f32, border_col),
                        StrokeKind::Inside,
                    );
                    painter.text(
                        Pos2::new(note_rect.min.x + 4.0, note_rect.center().y),
                        Align2::LEFT_CENTER,
                        midi_note_name(note.midi),
                        FontId::monospace(9.0),
                        Color32::from_rgb(18, 32, 20),
                    );
                }

                let playhead_x = grid_origin.x
                    + (sess.playhead_pos_steps / PIANO_ROLL_STEPS as f32).clamp(0.0, 1.0) * grid_w;
                let line_col = Color32::from_rgb(255, 190, 55);
                painter.line_segment(
                    [
                        Pos2::new(playhead_x, ruler_rect.min.y),
                        Pos2::new(playhead_x, grid_origin.y + grid_h),
                    ],
                    Stroke::new(2.2_f32, line_col),
                );
                let head_pts = vec![
                    Pos2::new(playhead_x - 6.0, ruler_rect.min.y),
                    Pos2::new(playhead_x + 6.0, ruler_rect.min.y),
                    Pos2::new(playhead_x, ruler_rect.min.y + 9.0),
                ];
                painter.add(egui::Shape::convex_polygon(
                    head_pts,
                    line_col,
                    Stroke::NONE,
                ));
            }
        });

    if let Some(target_step) = seek_to_step {
        if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
            sess.playhead_pos_steps = target_step.clamp(0.0, (PIANO_ROLL_STEPS - 1) as f32);
            sess.current_step = (sess.playhead_pos_steps.floor() as usize) % PIANO_ROLL_STEPS;
            if let Some(ch_idx) = sess.channel_index {
                if let Some(ch) = ctx.mixer.channel(ch_idx) {
                    let target_frame = ((sess.playhead_pos_steps / PIANO_ROLL_STEPS as f32)
                        * ch.media.total_frames as f32) as usize;
                    let _ = ctx.mixer.set_media_transport(
                        ch_idx,
                        sess.is_playing,
                        true,
                        Some(target_frame),
                        fallback.as_deref(),
                    );
                }
            }
        }
    }

    if let Some(midi) = preview_midi_to_trigger {
        ctx.state.active_piano_key = Some(midi as usize);
        trigger_live_midi_note(ctx, midi, false);
    }
}

fn render_sequencer_rack(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;

    egui::Frame::new()
        .fill(Color32::from_rgb(22, 23, 27))
        .stroke(Stroke::new(1.0_f32, Color32::from_rgb(44, 46, 54)))
        .corner_radius(CornerRadius::same(6))
        .inner_margin(Margin::same(10))
        .show(ui, |ui| {
            if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new("CHANNEL RACK / STEP SEQUENCER (Batería & Ritmo)")
                            .strong()
                            .color(Color32::from_rgb(240, 160, 120)),
                    );
                    ui.add_space(12.0);
                    ui.label(RichText::new("Swing:").color(StudioPalette::TEXT_SECONDARY));
                    let old_sw = sess.swing;
                    ui.add(egui::Slider::new(&mut sess.swing, 0.0..=0.6).show_value(false));
                    if (old_sw - sess.swing).abs() > 0.001 {
                        sess.mark_edited();
                    }
                });

                ui.add_space(6.0);

                let current_step = sess.current_step;
                let is_playing = sess.is_playing;
                let mut edited = false;

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
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("M").small().strong().color(mute_fg),
                                        )
                                        .fill(mute_bg)
                                        .corner_radius(CornerRadius::same(3)),
                                    )
                                    .clicked()
                                {
                                    track.mute = !track.mute;
                                    edited = true;
                                }

                                let (solo_bg, solo_fg) = if track.solo {
                                    (Color32::from_rgb(200, 150, 40), Color32::WHITE)
                                } else {
                                    (Color32::from_rgb(34, 36, 42), StudioPalette::TEXT_MUTED)
                                };
                                if ui
                                    .add(
                                        egui::Button::new(
                                            RichText::new("S").small().strong().color(solo_fg),
                                        )
                                        .fill(solo_bg)
                                        .corner_radius(CornerRadius::same(3)),
                                    )
                                    .clicked()
                                {
                                    track.solo = !track.solo;
                                    edited = true;
                                }

                                ui.add_space(4.0);
                                egui::Frame::new()
                                    .fill(Color32::from_rgb(34, 36, 42))
                                    .stroke(Stroke::new(1.0_f32, Color32::from_rgb(52, 54, 64)))
                                    .corner_radius(CornerRadius::same(4))
                                    .inner_margin(Margin::symmetric(10, 4))
                                    .show(ui, |ui| {
                                        ui.set_min_width(80.0);
                                        ui.label(
                                            RichText::new(&track.name).strong().color(Color32::WHITE),
                                        );
                                    });

                                ui.add_space(4.0);
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut track.volume)
                                            .range(0.0..=1.0)
                                            .speed(0.01)
                                            .prefix("Vol: "),
                                    )
                                    .changed()
                                {
                                    edited = true;
                                }
                                if ui
                                    .add(
                                        egui::DragValue::new(&mut track.pan)
                                            .range(-1.0..=1.0)
                                            .speed(0.02)
                                            .prefix("Pan: "),
                                    )
                                    .changed()
                                {
                                    edited = true;
                                }

                                ui.add_space(6.0);

                                for step_idx in 0..PIANO_ROLL_STEPS {
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

                                    let btn_rect = ui.allocate_space(Vec2::new(24.0, 26.0)).1;
                                    let resp = ui.interact(
                                        btn_rect,
                                        ui.id().with(("step_btn", track_idx, step_idx)),
                                        Sense::click(),
                                    );
                                    if resp.clicked() {
                                        track.steps[step_idx] = !track.steps[step_idx];
                                        edited = true;
                                    }

                                    ui.painter().rect_filled(
                                        btn_rect,
                                        CornerRadius::same(4),
                                        base_btn_color,
                                    );
                                    let stroke_w = if is_playhead { 2.0_f32 } else { 1.0_f32 };
                                    ui.painter().rect_stroke(
                                        btn_rect,
                                        CornerRadius::same(4),
                                        Stroke::new(stroke_w, border_color),
                                        StrokeKind::Inside,
                                    );

                                    if is_active {
                                        let pill_rect = Rect::from_center_size(
                                            btn_rect.center(),
                                            Vec2::new(13.0, 15.0),
                                        );
                                        let active_led = Color32::from_rgb(245, 175, 150);
                                        ui.painter().rect_filled(
                                            pill_rect,
                                            CornerRadius::same(3),
                                            active_led,
                                        );
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
                                        ui.painter().rect_filled(
                                            top_light,
                                            CornerRadius::same(1),
                                            Color32::from_rgb(255, 230, 90),
                                        );
                                    }
                                }
                            });
                            ui.add_space(3.0);
                        }
                    });

                if edited {
                    sess.mark_edited();
                }
            }
        });
}

fn render_synth_and_keyboard(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let active_idx = ctx.state.active_session_idx;

    ui.columns(2, |cols| {
        cols[0].group(|ui| {
            ui.label(
                RichText::new("SINTETIZADOR & EFECTOS")
                    .strong()
                    .color(Color32::from_rgb(240, 160, 120)),
            );
            ui.add_space(4.0);

            if let Some(sess) = ctx.state.sessions.get_mut(active_idx) {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Ataque")
                                .small()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        if ui
                            .add(egui::Slider::new(&mut sess.synth.attack, 0.001..=0.3).show_value(true))
                            .changed()
                        {
                            sess.mark_edited();
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Caída / Release")
                                .small()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        if ui
                            .add(egui::Slider::new(&mut sess.synth.release, 0.03..=1.2).show_value(true))
                            .changed()
                        {
                            sess.mark_edited();
                        }
                    });
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Filtro Cutoff")
                                .small()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        if ui
                            .add(
                                egui::Slider::new(&mut sess.synth.cutoff, 200.0..=10000.0)
                                    .logarithmic(true)
                                    .suffix(" Hz"),
                            )
                            .changed()
                        {
                            sess.mark_edited();
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Resonancia")
                                .small()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        if ui
                            .add(egui::Slider::new(&mut sess.synth.resonance, 0.0..=0.9))
                            .changed()
                        {
                            sess.mark_edited();
                        }
                    });
                });

                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Drive / Saturación")
                                .small()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        if ui
                            .add(egui::Slider::new(&mut sess.synth.drive, 0.0..=1.5))
                            .changed()
                        {
                            sess.mark_edited();
                        }
                    });
                    ui.vertical(|ui| {
                        ui.label(
                            RichText::new("Delay Envío")
                                .small()
                                .color(StudioPalette::TEXT_SECONDARY),
                        );
                        if ui
                            .add(egui::Slider::new(&mut sess.synth.delay_send, 0.0..=0.8))
                            .changed()
                        {
                            sess.mark_edited();
                        }
                    });
                });
            }
        });

        cols[1].group(|ui| {
            ui.label(
                RichText::new("TECLADO VIRTUAL FUNCIONAL (EN VIVO)")
                    .strong()
                    .color(Color32::from_rgb(240, 160, 120)),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new("Toca con clic o teclado físico [A W S E D F T G Y H U J K O L P]")
                    .small()
                    .color(StudioPalette::TEXT_MUTED),
            );
            ui.add_space(4.0);

            render_piano_keyboard(ui, ctx);
        });
    });
}

fn render_piano_keyboard(ui: &mut egui::Ui, ctx: &mut MidiStudioContext<'_>) {
    let white_notes: [(&str, &str, u8, Option<Key>); 14] = [
        ("C3", "A", 48, Some(Key::A)),
        ("D3", "S", 50, Some(Key::S)),
        ("E3", "D", 52, Some(Key::D)),
        ("F3", "F", 53, Some(Key::F)),
        ("G3", "G", 55, Some(Key::G)),
        ("A3", "H", 57, Some(Key::H)),
        ("B3", "J", 59, Some(Key::J)),
        ("C4", "K", 60, Some(Key::K)),
        ("D4", "L", 62, Some(Key::L)),
        ("E4", "Z", 64, Some(Key::Z)),
        ("F4", "X", 65, Some(Key::X)),
        ("G4", "C", 67, Some(Key::C)),
        ("A4", "V", 69, Some(Key::V)),
        ("B4", "B", 71, Some(Key::B)),
    ];

    let black_notes: [(&str, &str, u8, usize, Option<Key>); 10] = [
        ("C#3", "W", 49, 0, Some(Key::W)),
        ("D#3", "E", 51, 1, Some(Key::E)),
        ("F#3", "T", 54, 3, Some(Key::T)),
        ("G#3", "Y", 56, 4, Some(Key::Y)),
        ("A#3", "U", 58, 5, Some(Key::U)),
        ("C#4", "O", 61, 7, Some(Key::O)),
        ("D#4", "P", 63, 8, Some(Key::P)),
        ("F#4", "1", 66, 10, Some(Key::Num1)),
        ("G#4", "2", 68, 11, Some(Key::Num2)),
        ("A#4", "3", 70, 12, Some(Key::Num3)),
    ];

    let key_width = 27.0_f32;
    let key_height = 82.0_f32;
    let total_width = white_notes.len() as f32 * (key_width + 2.0);

    let (kb_rect, _) = ui.allocate_exact_size(Vec2::new(total_width, key_height), Sense::hover());
    let mut currently_down_midi: Option<u8> = None;
    let mut newly_triggered_midi: Option<u8> = None;

    let kb_free = !ui.ctx().egui_wants_keyboard_input();

    for (i, (name, key_hint, midi_num, key_code)) in white_notes.iter().enumerate() {
        let x = kb_rect.min.x + i as f32 * (key_width + 2.0);
        let key_r = Rect::from_min_size(Pos2::new(x, kb_rect.min.y), Vec2::new(key_width, key_height));

        let key_id = ui.id().with(("white_key", *midi_num));
        let resp = ui.interact(key_r, key_id, Sense::click_and_drag());
        let phys_down = kb_free && key_code.map(|k| ui.input(|inp| inp.key_down(k))).unwrap_or(false);
        let phys_pressed = kb_free && key_code.map(|k| ui.input(|inp| inp.key_pressed(k))).unwrap_or(false);
        let is_down = resp.is_pointer_button_down_on() || phys_down;

        let bg = if is_down {
            Color32::from_rgb(245, 165, 90)
        } else {
            Color32::from_rgb(224, 228, 235)
        };

        ui.painter().rect_filled(key_r, CornerRadius::same(3), bg);
        ui.painter().rect_stroke(
            key_r,
            CornerRadius::same(3),
            Stroke::new(1.0_f32, Color32::from_rgb(115, 120, 130)),
            StrokeKind::Inside,
        );

        let text_col = if is_down {
            Color32::WHITE
        } else {
            Color32::from_rgb(36, 38, 44)
        };
        ui.painter().text(
            Pos2::new(key_r.center().x, key_r.max.y - 18.0),
            Align2::CENTER_CENTER,
            *name,
            FontId::proportional(9.5),
            text_col,
        );
        ui.painter().text(
            Pos2::new(key_r.center().x, key_r.max.y - 8.0),
            Align2::CENTER_CENTER,
            *key_hint,
            FontId::monospace(8.5),
            Color32::from_rgb(105, 110, 120),
        );

        if is_down {
            currently_down_midi = Some(*midi_num);
        }
        if resp.clicked() || phys_pressed {
            newly_triggered_midi = Some(*midi_num);
        }
    }

    let black_w = 17.0_f32;
    let black_h = 50.0_f32;
    for (_name, key_hint, midi_num, white_slot, key_code) in &black_notes {
        let center_x = kb_rect.min.x + *white_slot as f32 * (key_width + 2.0) + key_width + 1.0;
        let b_rect = Rect::from_min_size(
            Pos2::new(center_x - black_w * 0.5, kb_rect.min.y),
            Vec2::new(black_w, black_h),
        );

        let key_id = ui.id().with(("black_key", *midi_num));
        let resp = ui.interact(b_rect, key_id, Sense::click_and_drag());
        let phys_down = kb_free && key_code.map(|k| ui.input(|inp| inp.key_down(k))).unwrap_or(false);
        let phys_pressed = kb_free && key_code.map(|k| ui.input(|inp| inp.key_pressed(k))).unwrap_or(false);
        let is_down = resp.is_pointer_button_down_on() || phys_down;

        let bg = if is_down {
            Color32::from_rgb(245, 165, 90)
        } else {
            Color32::from_rgb(24, 25, 30)
        };

        ui.painter().rect_filled(b_rect, CornerRadius::same(2), bg);
        ui.painter().rect_stroke(
            b_rect,
            CornerRadius::same(2),
            Stroke::new(1.0_f32, Color32::from_rgb(72, 75, 84)),
            StrokeKind::Inside,
        );

        ui.painter().text(
            Pos2::new(b_rect.center().x, b_rect.max.y - 10.0),
            Align2::CENTER_CENTER,
            *key_hint,
            FontId::monospace(8.0),
            Color32::from_rgb(185, 190, 200),
        );

        if is_down {
            currently_down_midi = Some(*midi_num);
        }
        if resp.clicked() || phys_pressed {
            newly_triggered_midi = Some(*midi_num);
        }
    }

    if let Some(midi_num) = currently_down_midi {
        ctx.state.active_piano_key = Some(midi_num as usize);
        if ctx.state.last_triggered_key != Some(midi_num) {
            ctx.state.last_triggered_key = Some(midi_num);
            newly_triggered_midi = Some(midi_num);
        }
    } else {
        ctx.state.last_triggered_key = None;
    }

    if let Some(midi_num) = newly_triggered_midi {
        ctx.state.active_piano_key = Some(midi_num as usize);
        trigger_live_midi_note(ctx, midi_num, true);
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
        assert!(!state.sessions[0].notes.is_empty());

        let buf = synthesize_session_pattern(&state.sessions[0], 48000);
        assert_eq!(buf.sample_rate, 48000);
        assert!(buf.total_frames > 40000);
        assert!(buf.interleaved_stereo.iter().any(|&s| s.abs() > 0.01));

        let preview = synthesize_single_note_preview(60, &state.sessions[0].synth, 48000);
        assert!(preview.interleaved_stereo.iter().any(|&s| s.abs() > 0.01));

        let new_idx = state.add_new_session();
        assert_eq!(new_idx, 1);
        assert_eq!(state.sessions.len(), 2);
        assert_eq!(state.sessions[1].name, "MIDI #2");
    }
}
