use std::{
    f32::consts::PI,
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
};

use crate::{MediaBuffer, db_to_linear};

pub const DJ_WAVEFORM_BINS: usize = 512;
pub const DJ_HOT_CUES: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum CrossfaderCurve {
    ConstantPower = 0,
    Linear = 1,
    SharpCut = 2,
}

impl CrossfaderCurve {
    pub fn from_u8(v: u8) -> Self {
        match v {
            1 => Self::Linear,
            2 => Self::SharpCut,
            _ => Self::ConstantPower,
        }
    }

    pub fn gains(self, xfade: f32) -> (f32, f32) {
        let t = ((xfade.clamp(-1.0, 1.0) + 1.0) * 0.5).clamp(0.0, 1.0);
        match self {
            Self::ConstantPower => {
                let angle = t * (PI * 0.5);
                (angle.cos(), angle.sin())
            }
            Self::Linear => (1.0 - t, t),
            Self::SharpCut => {
                let a = if t < 0.85 { 1.0 } else { ((1.0 - t) / 0.15).clamp(0.0, 1.0) };
                let b = if t > 0.15 { 1.0 } else { (t / 0.15).clamp(0.0, 1.0) };
                (a, b)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct WaveformBin {
    pub low: f32,
    pub mid: f32,
    pub high: f32,
    pub peak: f32,
}

#[derive(Debug, Clone, Copy)]
pub struct DjDeckParams {
    pub gain_db: f32,
    pub volume: f32,
    pub eq_low_db: f32,
    pub eq_mid_db: f32,
    pub eq_high_db: f32,
    pub kill_low: bool,
    pub kill_mid: bool,
    pub kill_high: bool,
    pub filter: f32,
    pub tempo_percent: f32,
    pub pfl_cue: bool,
    pub loop_enabled: bool,
    pub loop_beats: f32,
}

impl Default for DjDeckParams {
    fn default() -> Self {
        Self {
            gain_db: 0.0,
            volume: 0.85,
            eq_low_db: 0.0,
            eq_mid_db: 0.0,
            eq_high_db: 0.0,
            kill_low: false,
            kill_mid: false,
            kill_high: false,
            filter: 0.0,
            tempo_percent: 0.0,
            pfl_cue: false,
            loop_enabled: false,
            loop_beats: 4.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct DjDeckMeta {
    pub path: Option<PathBuf>,
    pub title: String,
    pub sample_rate: u32,
    pub total_frames: usize,
    pub base_bpm: f32,
    pub cue_frame: f64,
    pub hot_cues: [Option<f64>; DJ_HOT_CUES],
    pub loop_start_frame: f64,
    pub loop_end_frame: f64,
    pub waveform: Vec<WaveformBin>,
    pub params: DjDeckParams,
    pub playing: bool,
}

impl Default for DjDeckMeta {
    fn default() -> Self {
        Self {
            path: None,
            title: String::new(),
            sample_rate: 48_000,
            total_frames: 0,
            base_bpm: 120.0,
            cue_frame: 0.0,
            hot_cues: [None; DJ_HOT_CUES],
            loop_start_frame: 0.0,
            loop_end_frame: 0.0,
            waveform: vec![WaveformBin::default(); DJ_WAVEFORM_BINS],
            params: DjDeckParams::default(),
            playing: false,
        }
    }
}

impl DjDeckMeta {
    pub fn effective_bpm(&self) -> f32 {
        (self.base_bpm * (1.0 + self.params.tempo_percent / 100.0)).clamp(30.0, 260.0)
    }

    pub fn duration_secs(&self) -> f64 {
        if self.sample_rate == 0 {
            0.0
        } else {
            self.total_frames as f64 / self.sample_rate as f64
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct DjMixerParams {
    pub crossfader: f32,
    pub curve: CrossfaderCurve,
    pub master_volume: f32,
    pub cue_mix: f32,
    pub send_to_master: bool,
}

impl Default for DjMixerParams {
    fn default() -> Self {
        Self {
            crossfader: 0.0,
            curve: CrossfaderCurve::ConstantPower,
            master_volume: 1.0,
            cue_mix: 0.0,
            send_to_master: true,
        }
    }
}

pub enum DjCommand {
    LoadTrack {
        deck: usize,
        buffer: Arc<MediaBuffer>,
        base_bpm: f32,
    },
    UnloadTrack {
        deck: usize,
    },
    SetPlaying {
        deck: usize,
        playing: bool,
    },
    SeekFrame {
        deck: usize,
        frame: f64,
    },
    NudgeFrames {
        deck: usize,
        delta_frames: f64,
    },
    SetDeckParams {
        deck: usize,
        params: DjDeckParams,
    },
    SetLoopRegion {
        deck: usize,
        enabled: bool,
        start_frame: f64,
        end_frame: f64,
    },
    SetMixerParams(DjMixerParams),
    StartAutoCrossfade {
        target: f32,
        duration_secs: f32,
    },
}

pub struct DjDeckTelemetryAtomics {
    pub pos_frames: AtomicU64,
    pub playing: AtomicBool,
    pub peak_l: AtomicU32,
    pub peak_r: AtomicU32,
}

impl Default for DjDeckTelemetryAtomics {
    fn default() -> Self {
        Self {
            pos_frames: AtomicU64::new(0),
            playing: AtomicBool::new(false),
            peak_l: AtomicU32::new(0.0_f32.to_bits()),
            peak_r: AtomicU32::new(0.0_f32.to_bits()),
        }
    }
}

pub struct SharedDjTelemetry {
    pub decks: [DjDeckTelemetryAtomics; 2],
    pub master_peak_l: AtomicU32,
    pub master_peak_r: AtomicU32,
    pub crossfader: AtomicU32,
}

impl Default for SharedDjTelemetry {
    fn default() -> Self {
        Self {
            decks: [DjDeckTelemetryAtomics::default(), DjDeckTelemetryAtomics::default()],
            master_peak_l: AtomicU32::new(0.0_f32.to_bits()),
            master_peak_r: AtomicU32::new(0.0_f32.to_bits()),
            crossfader: AtomicU32::new(0.0_f32.to_bits()),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DjDeckTelemetry {
    pub pos_frames: u64,
    pub playing: bool,
    pub peak_l: f32,
    pub peak_r: f32,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct DjTelemetrySnapshot {
    pub decks: [DjDeckTelemetry; 2],
    pub master_peak_l: f32,
    pub master_peak_r: f32,
    pub crossfader: f32,
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    z1: f32,
    z2: f32,
}

#[derive(Clone, Copy)]
struct Biquad {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    st_l: BiquadState,
    st_r: BiquadState,
}

impl Default for Biquad {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
            st_l: BiquadState::default(),
            st_r: BiquadState::default(),
        }
    }
}

impl Biquad {
    fn set_low_shelf(&mut self, freq: f32, gain_db: f32, sr: f32) {
        if gain_db.abs() < 0.05 {
            self.set_unity();
            return;
        }
        let f = freq.clamp(20.0, sr * 0.45);
        let a = 10.0_f32.powf(gain_db.clamp(-24.0, 12.0) / 40.0);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cw = w0.cos();
        let sw = w0.sin();
        let beta = (a.sqrt() / 0.707) * sw;
        let ap1 = a + 1.0;
        let am1 = a - 1.0;
        let a0 = ap1 + am1 * cw + beta;
        self.b0 = (a * (ap1 - am1 * cw + beta)) / a0;
        self.b1 = (2.0 * a * (am1 - ap1 * cw)) / a0;
        self.b2 = (a * (ap1 - am1 * cw - beta)) / a0;
        self.a1 = (-2.0 * (am1 + ap1 * cw)) / a0;
        self.a2 = (ap1 + am1 * cw - beta) / a0;
    }

    fn set_high_shelf(&mut self, freq: f32, gain_db: f32, sr: f32) {
        if gain_db.abs() < 0.05 {
            self.set_unity();
            return;
        }
        let f = freq.clamp(20.0, sr * 0.45);
        let a = 10.0_f32.powf(gain_db.clamp(-24.0, 12.0) / 40.0);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cw = w0.cos();
        let sw = w0.sin();
        let beta = (a.sqrt() / 0.707) * sw;
        let ap1 = a + 1.0;
        let am1 = a - 1.0;
        let a0 = ap1 - am1 * cw + beta;
        self.b0 = (a * (ap1 + am1 * cw + beta)) / a0;
        self.b1 = (-2.0 * a * (am1 + ap1 * cw)) / a0;
        self.b2 = (a * (ap1 + am1 * cw - beta)) / a0;
        self.a1 = (2.0 * (am1 - ap1 * cw)) / a0;
        self.a2 = (ap1 - am1 * cw - beta) / a0;
    }

    fn set_peaking(&mut self, freq: f32, gain_db: f32, q: f32, sr: f32) {
        if gain_db.abs() < 0.05 {
            self.set_unity();
            return;
        }
        let f = freq.clamp(20.0, sr * 0.45);
        let a = 10.0_f32.powf(gain_db.clamp(-24.0, 12.0) / 40.0);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cw = w0.cos();
        let alpha = w0.sin() / (2.0 * q.clamp(0.3, 8.0));
        let a0 = 1.0 + alpha / a;
        self.b0 = (1.0 + alpha * a) / a0;
        self.b1 = (-2.0 * cw) / a0;
        self.b2 = (1.0 - alpha * a) / a0;
        self.a1 = (-2.0 * cw) / a0;
        self.a2 = (1.0 - alpha / a) / a0;
    }

    fn set_low_pass(&mut self, freq: f32, q: f32, sr: f32) {
        let f = freq.clamp(40.0, sr * 0.47);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cw = w0.cos();
        let alpha = w0.sin() / (2.0 * q.clamp(0.5, 4.0));
        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 - cw) * 0.5) / a0;
        self.b1 = (1.0 - cw) / a0;
        self.b2 = ((1.0 - cw) * 0.5) / a0;
        self.a1 = (-2.0 * cw) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn set_high_pass(&mut self, freq: f32, q: f32, sr: f32) {
        let f = freq.clamp(30.0, sr * 0.47);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cw = w0.cos();
        let alpha = w0.sin() / (2.0 * q.clamp(0.5, 4.0));
        let a0 = 1.0 + alpha;
        self.b0 = ((1.0 + cw) * 0.5) / a0;
        self.b1 = -(1.0 + cw) / a0;
        self.b2 = ((1.0 + cw) * 0.5) / a0;
        self.a1 = (-2.0 * cw) / a0;
        self.a2 = (1.0 - alpha) / a0;
    }

    fn set_unity(&mut self) {
        self.b0 = 1.0;
        self.b1 = 0.0;
        self.b2 = 0.0;
        self.a1 = 0.0;
        self.a2 = 0.0;
    }

    #[inline]
    fn process_stereo(&mut self, l: f32, r: f32) -> (f32, f32) {
        let yl = self.b0 * l + self.st_l.z1;
        self.st_l.z1 = self.b1 * l - self.a1 * yl + self.st_l.z2;
        self.st_l.z2 = self.b2 * l - self.a2 * yl;

        let yr = self.b0 * r + self.st_r.z1;
        self.st_r.z1 = self.b1 * r - self.a1 * yr + self.st_r.z2;
        self.st_r.z2 = self.b2 * r - self.a2 * yr;

        (yl, yr)
    }
}

struct RtDjDeck {
    buffer: Option<Arc<MediaBuffer>>,
    playing: bool,
    pos_frames: f64,
    base_bpm: f32,
    params: DjDeckParams,
    loop_start_frame: f64,
    loop_end_frame: f64,
    eq_low: Biquad,
    eq_mid: Biquad,
    eq_high: Biquad,
    color_filter: Biquad,
    last_sr: f32,
    coeffs_dirty: bool,
}

impl Default for RtDjDeck {
    fn default() -> Self {
        Self {
            buffer: None,
            playing: false,
            pos_frames: 0.0,
            base_bpm: 120.0,
            params: DjDeckParams::default(),
            loop_start_frame: 0.0,
            loop_end_frame: 0.0,
            eq_low: Biquad::default(),
            eq_mid: Biquad::default(),
            eq_high: Biquad::default(),
            color_filter: Biquad::default(),
            last_sr: 0.0,
            coeffs_dirty: true,
        }
    }
}

impl RtDjDeck {
    fn refresh_filters(&mut self, sr: f32) {
        if !self.coeffs_dirty && (self.last_sr - sr).abs() < 1.0 {
            return;
        }
        self.last_sr = sr;
        self.coeffs_dirty = false;

        let low_db = if self.params.kill_low { -24.0 } else { self.params.eq_low_db };
        let mid_db = if self.params.kill_mid { -24.0 } else { self.params.eq_mid_db };
        let high_db = if self.params.kill_high { -24.0 } else { self.params.eq_high_db };

        self.eq_low.set_low_shelf(220.0, low_db, sr);
        self.eq_mid.set_peaking(1100.0, mid_db, 0.9, sr);
        self.eq_high.set_high_shelf(4200.0, high_db, sr);

        let f = self.params.filter.clamp(-1.0, 1.0);
        if f < -0.02 {
            let norm = (1.0 + f).clamp(0.0, 1.0);
            let cutoff = 120.0 * (18000.0_f32 / 120.0).powf(norm);
            self.color_filter.set_low_pass(cutoff, 1.15, sr);
        } else if f > 0.02 {
            let cutoff = 40.0 * (11000.0_f32 / 40.0).powf(f);
            self.color_filter.set_high_pass(cutoff, 1.15, sr);
        } else {
            self.color_filter.set_unity();
        }
    }

    #[inline]
    fn sample_hermite(buf: &[f32], total_frames: usize, pos: f64) -> (f32, f32) {
        if total_frames == 0 {
            return (0.0, 0.0);
        }
        let i0 = pos.floor() as isize;
        let frac = (pos - i0 as f64) as f32;
        let idx = |k: isize| -> usize {
            k.clamp(0, (total_frames - 1) as isize) as usize
        };
        let p0 = idx(i0 - 1) * 2;
        let p1 = idx(i0) * 2;
        let p2 = idx(i0 + 1) * 2;
        let p3 = idx(i0 + 2) * 2;

        let hermite = |y0: f32, y1: f32, y2: f32, y3: f32, t: f32| -> f32 {
            let c0 = y1;
            let c1 = 0.5 * (y2 - y0);
            let c2 = y0 - 2.5 * y1 + 2.0 * y2 - 0.5 * y3;
            let c3 = 0.5 * (y3 - y0) + 1.5 * (y1 - y2);
            ((c3 * t + c2) * t + c1) * t + c0
        };

        let l = hermite(buf[p0], buf[p1], buf[p2], buf[p3], frac);
        let r = hermite(buf[p0 + 1], buf[p1 + 1], buf[p2 + 1], buf[p3 + 1], frac);
        (l, r)
    }
}

pub struct DjAudioProcessor {
    telemetry: Arc<SharedDjTelemetry>,
    decks: [RtDjDeck; 2],
    mixer: DjMixerParams,
    auto_fade_active: bool,
    auto_fade_target: f32,
    auto_fade_step_per_sec: f32,
}

impl DjAudioProcessor {
    pub fn new(telemetry: Arc<SharedDjTelemetry>) -> Self {
        Self {
            telemetry,
            decks: [RtDjDeck::default(), RtDjDeck::default()],
            mixer: DjMixerParams::default(),
            auto_fade_active: false,
            auto_fade_target: 0.0,
            auto_fade_step_per_sec: 0.0,
        }
    }

    pub fn handle_command(&mut self, cmd: DjCommand) {
        match cmd {
            DjCommand::LoadTrack {
                deck,
                buffer,
                base_bpm,
            } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.buffer = Some(buffer);
                    d.pos_frames = 0.0;
                    d.playing = false;
                    d.base_bpm = base_bpm.clamp(40.0, 240.0);
                    d.params.loop_enabled = false;
                    d.loop_start_frame = 0.0;
                    d.loop_end_frame = 0.0;
                }
            }
            DjCommand::UnloadTrack { deck } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.buffer = None;
                    d.pos_frames = 0.0;
                    d.playing = false;
                }
            }
            DjCommand::SetPlaying { deck, playing } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.playing = playing && d.buffer.is_some();
                }
            }
            DjCommand::SeekFrame { deck, frame } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    if let Some(ref b) = d.buffer {
                        d.pos_frames = frame.clamp(0.0, (b.total_frames.saturating_sub(1)) as f64);
                    }
                }
            }
            DjCommand::NudgeFrames { deck, delta_frames } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    if let Some(ref b) = d.buffer {
                        d.pos_frames = (d.pos_frames + delta_frames)
                            .clamp(0.0, (b.total_frames.saturating_sub(1)) as f64);
                    }
                }
            }
            DjCommand::SetDeckParams { deck, params } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.params = params;
                    d.coeffs_dirty = true;
                }
            }
            DjCommand::SetLoopRegion {
                deck,
                enabled,
                start_frame,
                end_frame,
            } => {
                if let Some(d) = self.decks.get_mut(deck) {
                    d.params.loop_enabled = enabled;
                    d.loop_start_frame = start_frame.max(0.0);
                    d.loop_end_frame = end_frame.max(start_frame + 128.0);
                }
            }
            DjCommand::SetMixerParams(params) => {
                self.mixer = params;
                self.auto_fade_active = false;
            }
            DjCommand::StartAutoCrossfade {
                target,
                duration_secs,
            } => {
                let target = target.clamp(-1.0, 1.0);
                let dur = duration_secs.clamp(0.25, 16.0);
                let diff = target - self.mixer.crossfader;
                self.auto_fade_target = target;
                self.auto_fade_step_per_sec = diff / dur;
                self.auto_fade_active = diff.abs() > 0.001;
            }
        }
    }

    pub fn process_and_mix(
        &mut self,
        out_left: &mut [f32],
        out_right: &mut [f32],
        vmic_left: &mut [f32],
        vmic_right: &mut [f32],
        frames: usize,
        sample_rate: f64,
    ) {
        let sr = sample_rate.max(8000.0) as f32;
        for d in &mut self.decks {
            d.refresh_filters(sr);
        }

        if self.auto_fade_active {
            let dt = frames as f32 / sr;
            let next = self.mixer.crossfader + self.auto_fade_step_per_sec * dt;
            if (self.auto_fade_step_per_sec >= 0.0 && next >= self.auto_fade_target)
                || (self.auto_fade_step_per_sec < 0.0 && next <= self.auto_fade_target)
            {
                self.mixer.crossfader = self.auto_fade_target;
                self.auto_fade_active = false;
            } else {
                self.mixer.crossfader = next.clamp(-1.0, 1.0);
            }
        }
        self.telemetry
            .crossfader
            .store(self.mixer.crossfader.to_bits(), Ordering::Relaxed);

        let (xf_a, xf_b) = self.mixer.curve.gains(self.mixer.crossfader);
        let xf_gains = [xf_a, xf_b];
        let master_gain = if self.mixer.send_to_master {
            self.mixer.master_volume.clamp(0.0, 1.5)
        } else {
            0.0
        };

        let mut master_peak_l = 0.0_f32;
        let mut master_peak_r = 0.0_f32;
        let mut deck_peaks = [[0.0_f32; 2]; 2];

        for i in 0..frames {
            let mut sum_l = 0.0_f32;
            let mut sum_r = 0.0_f32;

            for (d_idx, deck) in self.decks.iter_mut().enumerate() {
                if !deck.playing {
                    continue;
                }
                let Some(ref buf) = deck.buffer else {
                    deck.playing = false;
                    continue;
                };
                if buf.total_frames == 0 {
                    deck.playing = false;
                    continue;
                }

                let (mut l, mut r) =
                    RtDjDeck::sample_hermite(&buf.interleaved_stereo, buf.total_frames, deck.pos_frames);

                let trim = db_to_linear(deck.params.gain_db);
                l *= trim;
                r *= trim;

                let (l1, r1) = deck.eq_low.process_stereo(l, r);
                let (l2, r2) = deck.eq_mid.process_stereo(l1, r1);
                let (l3, r3) = deck.eq_high.process_stereo(l2, r2);
                let (mut l4, mut r4) = deck.color_filter.process_stereo(l3, r3);

                let vol = deck.params.volume.clamp(0.0, 1.2);
                l4 *= vol;
                r4 *= vol;

                deck_peaks[d_idx][0] = deck_peaks[d_idx][0].max(l4.abs());
                deck_peaks[d_idx][1] = deck_peaks[d_idx][1].max(r4.abs());

                sum_l += l4 * xf_gains[d_idx];
                sum_r += r4 * xf_gains[d_idx];

                let pitch_ratio = (1.0 + deck.params.tempo_percent as f64 / 100.0).clamp(0.25, 2.0);
                let sr_ratio = buf.sample_rate.max(8000) as f64 / sample_rate.max(8000.0);
                deck.pos_frames += pitch_ratio * sr_ratio;

                if deck.params.loop_enabled
                    && deck.loop_end_frame > deck.loop_start_frame + 64.0
                    && deck.pos_frames >= deck.loop_end_frame
                {
                    deck.pos_frames = deck.loop_start_frame
                        + (deck.pos_frames - deck.loop_end_frame)
                            .rem_euclid(deck.loop_end_frame - deck.loop_start_frame);
                } else if deck.pos_frames >= buf.total_frames as f64 {
                    deck.pos_frames = (buf.total_frames.saturating_sub(1)) as f64;
                    deck.playing = false;
                }
            }

            let mix_l = sum_l * master_gain;
            let mix_r = sum_r * master_gain;
            master_peak_l = master_peak_l.max(mix_l.abs());
            master_peak_r = master_peak_r.max(mix_r.abs());

            out_left[i] += mix_l;
            out_right[i] += mix_r;
            vmic_left[i] += mix_l;
            vmic_right[i] += mix_r;
        }

        for (d_idx, deck) in self.decks.iter().enumerate() {
            let td = &self.telemetry.decks[d_idx];
            td.pos_frames
                .store(deck.pos_frames.max(0.0) as u64, Ordering::Relaxed);
            td.playing.store(deck.playing, Ordering::Relaxed);
            store_max_f32(&td.peak_l, deck_peaks[d_idx][0]);
            store_max_f32(&td.peak_r, deck_peaks[d_idx][1]);
        }
        store_max_f32(&self.telemetry.master_peak_l, master_peak_l);
        store_max_f32(&self.telemetry.master_peak_r, master_peak_r);
    }
}

fn store_max_f32(slot: &AtomicU32, val: f32) {
    let mut cur = slot.load(Ordering::Relaxed);
    loop {
        let cur_f = f32::from_bits(cur);
        if val <= cur_f {
            break;
        }
        match slot.compare_exchange_weak(cur, val.to_bits(), Ordering::Relaxed, Ordering::Relaxed) {
            Ok(_) => break,
            Err(next) => cur = next,
        }
    }
}

pub fn analyze_waveform_and_bpm(buf: &MediaBuffer) -> (Vec<WaveformBin>, f32) {
    let total = buf.total_frames;
    if total == 0 || buf.interleaved_stereo.len() < 2 {
        return (vec![WaveformBin::default(); DJ_WAVEFORM_BINS], 120.0);
    }

    let mut bins = vec![WaveformBin::default(); DJ_WAVEFORM_BINS];
    let frames_per_bin = (total as f64 / DJ_WAVEFORM_BINS as f64).max(1.0);

    let mut lp_state = 0.0_f32;
    let mut hp_state = 0.0_f32;
    let mut prev_x = 0.0_f32;
    let sr = buf.sample_rate.max(8000) as f32;
    let lp_a = (2.0 * PI * 250.0 / sr).clamp(0.001, 0.5);
    let hp_a = (1.0 / (1.0 + 2.0 * PI * 2800.0 / sr)).clamp(0.1, 0.99);

    for (b_idx, bin) in bins.iter_mut().enumerate() {
        let start = (b_idx as f64 * frames_per_bin) as usize;
        let end = (((b_idx + 1) as f64 * frames_per_bin) as usize)
            .min(total)
            .max(start + 1);
        let step = ((end - start) / 256).max(1);
        let mut peak = 0.0_f32;
        let mut low_acc = 0.0_f32;
        let mut mid_acc = 0.0_f32;
        let mut high_acc = 0.0_f32;
        let mut count = 0_u32;

        let mut f = start;
        while f < end {
            let l = buf.interleaved_stereo[f * 2];
            let r = buf.interleaved_stereo[f * 2 + 1];
            let mono = (l + r) * 0.5;
            peak = peak.max(l.abs().max(r.abs()));

            lp_state += lp_a * (mono - lp_state);
            hp_state = hp_a * (hp_state + mono - prev_x);
            prev_x = mono;
            let mid = mono - lp_state - hp_state;

            low_acc += lp_state * lp_state;
            mid_acc += mid * mid;
            high_acc += hp_state * hp_state;
            count += 1;
            f += step;
        }
        let inv = 1.0 / (count.max(1) as f32);
        bin.peak = peak.clamp(0.0, 1.0);
        bin.low = (low_acc * inv).sqrt().min(1.0);
        bin.mid = (mid_acc * inv).sqrt().min(1.0);
        bin.high = (high_acc * inv).sqrt().min(1.0);
    }

    let bpm = estimate_bpm(buf);
    (bins, bpm)
}

fn estimate_bpm(buf: &MediaBuffer) -> f32 {
    let sr = buf.sample_rate.max(8000) as usize;
    let hop = (sr / 100).max(1);
    let max_frames = buf.total_frames.min(sr * 60);
    let num_env = max_frames / hop;
    if num_env < 200 {
        return 120.0;
    }

    let mut env = Vec::with_capacity(num_env);
    let mut prev_energy = 0.0_f32;
    for k in 0..num_env {
        let start = k * hop;
        let end = (start + hop).min(max_frames);
        let mut e = 0.0_f32;
        for f in start..end {
            let m = 0.5 * (buf.interleaved_stereo[f * 2] + buf.interleaved_stereo[f * 2 + 1]);
            e += m * m;
        }
        e /= (end - start).max(1) as f32;
        let onset = (e - prev_energy).max(0.0);
        prev_energy = e;
        env.push(onset);
    }

    let env_rate = sr as f32 / hop as f32;
    let min_lag = ((60.0 / 175.0) * env_rate).round() as usize;
    let max_lag = ((60.0 / 75.0) * env_rate).round() as usize;
    if max_lag >= env.len() || min_lag >= max_lag {
        return 120.0;
    }

    let mut best_lag = min_lag;
    let mut best_score = 0.0_f32;
    for lag in min_lag..=max_lag {
        let mut score = 0.0_f32;
        for i in 0..(env.len() - lag) {
            score += env[i] * env[i + lag];
        }
        if score > best_score {
            best_score = score;
            best_lag = lag;
        }
    }

    if best_score <= 1e-8 {
        return 120.0;
    }

    let raw_bpm = 60.0 * env_rate / best_lag as f32;
    let mut bpm = raw_bpm;
    while bpm < 80.0 {
        bpm *= 2.0;
    }
    while bpm > 170.0 {
        bpm *= 0.5;
    }
    (bpm * 10.0).round() / 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_tone_buffer(freq: f32, sr: u32, secs: f32) -> Arc<MediaBuffer> {
        let total_frames = (sr as f32 * secs) as usize;
        let mut stereo = vec![0.0_f32; total_frames * 2];
        for i in 0..total_frames {
            let s = (2.0 * PI * freq * i as f32 / sr as f32).sin() * 0.5;
            stereo[i * 2] = s;
            stereo[i * 2 + 1] = s;
        }
        Arc::new(MediaBuffer {
            interleaved_stereo: stereo,
            total_frames,
            sample_rate: sr,
            title: "Test Tone".into(),
        })
    }

    #[test]
    fn dj_crossfader_isolates_deck_a_and_deck_b() {
        let tel = Arc::new(SharedDjTelemetry::default());
        let mut proc = DjAudioProcessor::new(Arc::clone(&tel));
        proc.handle_command(DjCommand::LoadTrack {
            deck: 0,
            buffer: make_tone_buffer(220.0, 48_000, 1.0),
            base_bpm: 120.0,
        });
        proc.handle_command(DjCommand::LoadTrack {
            deck: 1,
            buffer: make_tone_buffer(880.0, 48_000, 1.0),
            base_bpm: 128.0,
        });
        proc.handle_command(DjCommand::SetPlaying { deck: 0, playing: true });
        proc.handle_command(DjCommand::SetPlaying { deck: 1, playing: true });
        proc.handle_command(DjCommand::SetMixerParams(DjMixerParams {
            crossfader: -1.0,
            curve: CrossfaderCurve::Linear,
            master_volume: 1.0,
            cue_mix: 0.0,
            send_to_master: true,
        }));

        let mut out_l = vec![0.0_f32; 256];
        let mut out_r = vec![0.0_f32; 256];
        let mut vmic_l = vec![0.0_f32; 256];
        let mut vmic_r = vec![0.0_f32; 256];
        proc.process_and_mix(&mut out_l, &mut out_r, &mut vmic_l, &mut vmic_r, 256, 48_000.0);

        let peak = out_l.iter().fold(0.0_f32, |a, &b| a.max(b.abs()));
        assert!(peak > 0.1);
        assert!(tel.decks[0].pos_frames.load(Ordering::Relaxed) >= 250);
    }

    #[test]
    fn dj_auto_loop_wraps_playhead() {
        let tel = Arc::new(SharedDjTelemetry::default());
        let mut proc = DjAudioProcessor::new(Arc::clone(&tel));
        proc.handle_command(DjCommand::LoadTrack {
            deck: 0,
            buffer: make_tone_buffer(440.0, 48_000, 2.0),
            base_bpm: 120.0,
        });
        proc.handle_command(DjCommand::SetLoopRegion {
            deck: 0,
            enabled: true,
            start_frame: 100.0,
            end_frame: 300.0,
        });
        proc.handle_command(DjCommand::SeekFrame { deck: 0, frame: 250.0 });
        proc.handle_command(DjCommand::SetPlaying { deck: 0, playing: true });

        let mut out_l = vec![0.0_f32; 256];
        let mut out_r = vec![0.0_f32; 256];
        let mut vmic_l = vec![0.0_f32; 256];
        let mut vmic_r = vec![0.0_f32; 256];
        proc.process_and_mix(&mut out_l, &mut out_r, &mut vmic_l, &mut vmic_r, 256, 48_000.0);

        let pos = tel.decks[0].pos_frames.load(Ordering::Relaxed);
        assert!(pos >= 100 && pos < 300);
    }

    #[test]
    fn waveform_analysis_produces_512_bins() {
        let buf = make_tone_buffer(440.0, 48_000, 1.0);
        let (bins, bpm) = analyze_waveform_and_bpm(&buf);
        assert_eq!(bins.len(), DJ_WAVEFORM_BINS);
        assert!(bins.iter().any(|b| b.peak > 0.2));
        assert!(bpm >= 75.0 && bpm <= 175.0);
    }
}
