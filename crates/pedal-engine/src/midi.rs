use std::{
    f32::consts::PI,
    time::Instant,
};

use crate::MediaBuffer;

pub const PIANO_ROLL_STEPS: usize = 16;
pub const PIANO_MIN_MIDI: u8 = 48;
pub const PIANO_MAX_MIDI: u8 = 71;
pub const PIANO_NOTE_COUNT: usize = (PIANO_MAX_MIDI - PIANO_MIN_MIDI + 1) as usize;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TrackSound {
    Kick = 0,
    Clap = 1,
    Hat = 2,
    Snare = 3,
    SynthLead = 4,
    SynthBass = 5,
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
#[repr(u8)]
pub enum SynthPreset {
    AnalogLead = 0,
    FmKeys = 1,
    WarmPad = 2,
    PluckSynth = 3,
    SubBass = 4,
}

impl SynthPreset {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::FmKeys,
            2 => Self::WarmPad,
            3 => Self::PluckSynth,
            4 => Self::SubBass,
            _ => Self::AnalogLead,
        }
    }

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
            Self::AnalogLead => "Studio Keys / Analog Lead",
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

#[derive(Debug, Clone, Copy)]
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

    pub fn clear_notes(&mut self) {
        self.notes.clear();
        self.mark_edited();
    }
}

pub struct MidiStudioState {
    pub sessions: Vec<MidiSession>,
    pub active_session_idx: usize,
    pub next_session_id: usize,
    pub last_triggered_key: Option<u8>,
}

impl Default for MidiStudioState {
    fn default() -> Self {
        Self::new()
    }
}

impl MidiStudioState {
    pub fn new() -> Self {
        Self {
            sessions: vec![MidiSession::new(1, "MIDI #1".to_string())],
            active_session_idx: 0,
            next_session_id: 2,
            last_triggered_key: None,
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
        self.sessions.push(MidiSession::new(id, format!("MIDI #{id}")));
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
                            (2.0 * PI * pitch * t).sin() * env * 0.85
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn midi_studio_sessions_and_synthesis_work() {
        let mut st = MidiStudioState::new();
        assert_eq!(st.sessions.len(), 1);
        let buf = synthesize_session_pattern(st.active_session().unwrap(), 48_000);
        assert!(buf.total_frames > 0);
        assert!(buf.interleaved_stereo.iter().any(|&v| v.abs() > 0.05));
        st.add_new_session();
        assert_eq!(st.sessions.len(), 2);
        let preview = synthesize_single_note_preview(60, &SynthParams::default(), 48_000);
        assert!(preview.total_frames > 1000);
    }
}
