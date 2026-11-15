use std::{
    f32::consts::{PI, SQRT_2},
    fs::{self, File},
    io::{BufWriter, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use hound::{SampleFormat as WavSampleFormat, WavReader, WavSpec, WavWriter};
use pedal_audio::{
    ActiveMixerInfo, AudioDeviceInfo, AudioStats, MixerAudioConfig, MixerAudioProcessor,
    MixerAudioSession,
};
use pedal_wine::WineConfig;
use rtrb::{Consumer, Producer, RingBuffer};
use vst3_host::{AudioBuffers, PluginWindow};

use crate::{
    ActivePluginHandle, EngineError, LoadedBackend, LoadedPluginMeta, close_backend_gui,
    db_to_linear, is_backend_gui_open, load_plugin_backend, make_backend_active_handle,
    open_backend_gui, process_active_handle, refresh_backend_params_ui, service_backend_gui,
    set_backend_param_live, set_backend_param_ui, slice_peak, store_peak,
    unload_backend_resources,
};

pub const MAX_MIXER_CHANNELS: usize = 48;
pub const MAX_MIXER_INSERTS: usize = 4;
pub const DEFAULT_MIXER_CHANNELS: usize = 2;
pub const VIRTUAL_MIC_SINK_NAME: &str = "chronomix_virtual_sink";
pub const VIRTUAL_MIC_SOURCE_NAME: &str = "chronomix_virtual_mic";
pub const VIRTUAL_MIC_DISPLAY_NAME: &str = "Chronomix Virtual Mic (Discord/OBS)";

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ChannelIcon {
    #[default]
    None,
    Vocals,
    Guitar,
    Bass,
    Drums,
    Keys,
    Media,
    Strings,
    Brass,
    Broadcast,
    Sfx,
    CustomImage(PathBuf),
}

impl ChannelIcon {
    pub fn presets() -> &'static [ChannelIcon] {
        &[
            ChannelIcon::None,
            ChannelIcon::Vocals,
            ChannelIcon::Guitar,
            ChannelIcon::Bass,
            ChannelIcon::Drums,
            ChannelIcon::Keys,
            ChannelIcon::Media,
            ChannelIcon::Strings,
            ChannelIcon::Brass,
            ChannelIcon::Broadcast,
            ChannelIcon::Sfx,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChannelInputMode {
    Unassigned,
    Mono(usize),
    Stereo(usize, usize),
    Multimedia,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChannelInputSource {
    pub device_id: Option<String>,
    pub device_label: String,
    pub mode: ChannelInputMode,
}

impl Default for ChannelInputSource {
    fn default() -> Self {
        Self {
            device_id: None,
            device_label: String::new(),
            mode: ChannelInputMode::Unassigned,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EqBandConfig {
    pub enabled: bool,
    pub freq_hz: f32,
    pub gain_db: f32,
    pub q: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChannelDspConfig {
    pub trim_db: f32,
    pub balance: f32,
    pub stereo_width: f32,
    pub phase_invert: bool,
    pub swap_lr: bool,
    pub hpf_enabled: bool,
    pub hpf_freq_hz: f32,
    pub lpf_enabled: bool,
    pub lpf_freq_hz: f32,
    pub gate_enabled: bool,
    pub gate_threshold_db: f32,
    pub eq_enabled: bool,
    pub eq_bands: [EqBandConfig; 4],
    pub comp_enabled: bool,
    pub comp_threshold_db: f32,
    pub comp_ratio: f32,
    pub comp_makeup_db: f32,
    pub fx_bypass: bool,
    pub fader_db: f32,
    pub pan: f32,
    pub mute: bool,
    pub solo: bool,
    pub solo_safe: bool,
    pub record_arm: bool,
    pub pre_fader_rec: bool,
    pub send_to_virtual_mic: bool,
}

impl Default for ChannelDspConfig {
    fn default() -> Self {
        Self {
            trim_db: 0.0,
            balance: 0.0,
            stereo_width: 1.0,
            phase_invert: false,
            swap_lr: false,
            hpf_enabled: false,
            hpf_freq_hz: 80.0,
            lpf_enabled: false,
            lpf_freq_hz: 16000.0,
            gate_enabled: false,
            gate_threshold_db: -48.0,
            eq_enabled: true,
            eq_bands: [
                EqBandConfig {
                    enabled: true,
                    freq_hz: 90.0,
                    gain_db: 0.0,
                    q: 0.75,
                },
                EqBandConfig {
                    enabled: true,
                    freq_hz: 420.0,
                    gain_db: 0.0,
                    q: 1.1,
                },
                EqBandConfig {
                    enabled: true,
                    freq_hz: 2400.0,
                    gain_db: 0.0,
                    q: 1.1,
                },
                EqBandConfig {
                    enabled: true,
                    freq_hz: 9500.0,
                    gain_db: 0.0,
                    q: 0.75,
                },
            ],
            comp_enabled: false,
            comp_threshold_db: -18.0,
            comp_ratio: 3.0,
            comp_makeup_db: 0.0,
            fx_bypass: false,
            fader_db: 0.0,
            pan: 0.0,
            mute: false,
            solo: false,
            solo_safe: false,
            record_arm: false,
            pre_fader_rec: false,
            send_to_virtual_mic: true,
        }
    }
}

#[derive(Debug, Clone)]
pub struct InsertSlotInfo {
    pub meta: Option<LoadedPluginMeta>,
    pub bypass: bool,
    pub mix: f32,
}

impl Default for InsertSlotInfo {
    fn default() -> Self {
        Self {
            meta: None,
            bypass: false,
            mix: 1.0,
        }
    }
}

#[derive(Clone)]
pub struct MediaBuffer {
    pub interleaved_stereo: Vec<f32>,
    pub total_frames: usize,
    pub sample_rate: u32,
    pub title: String,
}

#[derive(Debug, Clone, Default)]
pub struct MediaTrackState {
    pub path: Option<PathBuf>,
    pub title: String,
    pub sample_rate: u32,
    pub total_frames: usize,
    pub current_frame: usize,
    pub playing: bool,
    pub looping: bool,
    pub loading: bool,
}

#[derive(Debug, Clone)]
pub struct MixerChannelState {
    pub index: usize,
    pub name: String,
    pub color: [u8; 3],
    pub icon: ChannelIcon,
    pub source: ChannelInputSource,
    pub dsp: ChannelDspConfig,
    pub inserts: [InsertSlotInfo; MAX_MIXER_INSERTS],
    pub media: MediaTrackState,
}

#[derive(Debug, Clone)]
pub struct MasterBusState {
    pub fader_db: f32,
    pub mute: bool,
    pub dim: bool,
    pub record_mix: bool,
    pub virtual_mic_enabled: bool,
    pub virtual_mic_gain_db: f32,
    pub inserts: [InsertSlotInfo; MAX_MIXER_INSERTS],
}

impl Default for MasterBusState {
    fn default() -> Self {
        Self {
            fader_db: 0.0,
            mute: false,
            dim: false,
            record_mix: false,
            virtual_mic_enabled: false,
            virtual_mic_gain_db: 0.0,
            inserts: std::array::from_fn(|_| InsertSlotInfo::default()),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MixerInsertTarget {
    Channel { ch: usize, slot: usize },
    Master { slot: usize },
}

#[derive(Debug, Clone, Copy, Default)]
pub struct ChannelMeterValues {
    pub in_left: f32,
    pub in_right: f32,
    pub out_left: f32,
    pub out_right: f32,
    pub comp_gr_db: f32,
    pub media_pos_frames: u64,
    pub media_playing: bool,
}

#[derive(Debug, Clone)]
pub struct MixerMeterSnapshot {
    pub channels: Vec<ChannelMeterValues>,
    pub master_left: f32,
    pub master_right: f32,
    pub virtual_mic_left: f32,
    pub virtual_mic_right: f32,
    pub is_virtual_mic_active: bool,
    pub recorded_frames: u64,
    pub is_recording: bool,
}

#[derive(Debug, Clone)]
pub struct MixerRecordingReport {
    pub directory: PathBuf,
    pub files: Vec<PathBuf>,
    pub frames: u64,
}

#[derive(Clone, Copy)]
struct BiquadCoeffs {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
}

impl Default for BiquadCoeffs {
    fn default() -> Self {
        Self {
            b0: 1.0,
            b1: 0.0,
            b2: 0.0,
            a1: 0.0,
            a2: 0.0,
        }
    }
}

impl BiquadCoeffs {
    fn high_pass(freq_hz: f32, sr: f32) -> Self {
        let f = freq_hz.clamp(15.0, (sr * 0.45).max(30.0));
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * 0.707_106_77);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 + cos_w0) * 0.5) / a0,
            b1: -(1.0 + cos_w0) / a0,
            b2: ((1.0 + cos_w0) * 0.5) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    fn low_pass(freq_hz: f32, sr: f32) -> Self {
        let f = freq_hz.clamp(100.0, (sr * 0.48).max(500.0));
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * 0.707_106_77);
        let a0 = 1.0 + alpha;
        Self {
            b0: ((1.0 - cos_w0) * 0.5) / a0,
            b1: (1.0 - cos_w0) / a0,
            b2: ((1.0 - cos_w0) * 0.5) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    fn peaking(freq_hz: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        if gain_db.abs() < 0.05 {
            return Self::default();
        }
        let f = freq_hz.clamp(20.0, (sr * 0.45).max(40.0));
        let a = 10.0_f32.powf(gain_db.clamp(-18.0, 18.0) / 40.0);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cos_w0 = w0.cos();
        let alpha = w0.sin() / (2.0 * q.clamp(0.3, 12.0));
        let a0 = 1.0 + alpha / a;
        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos_w0) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos_w0) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }

    fn low_shelf(freq_hz: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        if gain_db.abs() < 0.05 {
            return Self::default();
        }
        let f = freq_hz.clamp(20.0, (sr * 0.45).max(40.0));
        let a = 10.0_f32.powf(gain_db.clamp(-18.0, 18.0) / 40.0);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let slope_q = q.clamp(0.4, 2.0);
        let beta = (a.sqrt() / slope_q) * sin_w0;
        let ap1 = a + 1.0;
        let am1 = a - 1.0;
        let a0 = ap1 + am1 * cos_w0 + beta;
        Self {
            b0: (a * (ap1 - am1 * cos_w0 + beta)) / a0,
            b1: (2.0 * a * (am1 - ap1 * cos_w0)) / a0,
            b2: (a * (ap1 - am1 * cos_w0 - beta)) / a0,
            a1: (-2.0 * (am1 + ap1 * cos_w0)) / a0,
            a2: (ap1 + am1 * cos_w0 - beta) / a0,
        }
    }

    fn high_shelf(freq_hz: f32, gain_db: f32, q: f32, sr: f32) -> Self {
        if gain_db.abs() < 0.05 {
            return Self::default();
        }
        let f = freq_hz.clamp(20.0, (sr * 0.45).max(40.0));
        let a = 10.0_f32.powf(gain_db.clamp(-18.0, 18.0) / 40.0);
        let w0 = 2.0 * PI * f / sr.max(1000.0);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let slope_q = q.clamp(0.4, 2.0);
        let beta = (a.sqrt() / slope_q) * sin_w0;
        let ap1 = a + 1.0;
        let am1 = a - 1.0;
        let a0 = ap1 - am1 * cos_w0 + beta;
        Self {
            b0: (a * (ap1 + am1 * cos_w0 + beta)) / a0,
            b1: (-2.0 * a * (am1 + ap1 * cos_w0)) / a0,
            b2: (a * (ap1 + am1 * cos_w0 - beta)) / a0,
            a1: (2.0 * (am1 - ap1 * cos_w0)) / a0,
            a2: (ap1 - am1 * cos_w0 - beta) / a0,
        }
    }

    fn magnitude_db(self, freq_hz: f32, sr: f32) -> f32 {
        let w = 2.0 * PI * freq_hz.clamp(10.0, sr * 0.49) / sr.max(1000.0);
        let cos1 = (-w).cos();
        let sin1 = (-w).sin();
        let cos2 = (-2.0 * w).cos();
        let sin2 = (-2.0 * w).sin();
        let num_re = self.b0 + self.b1 * cos1 + self.b2 * cos2;
        let num_im = self.b1 * sin1 + self.b2 * sin2;
        let den_re = 1.0 + self.a1 * cos1 + self.a2 * cos2;
        let den_im = self.a1 * sin1 + self.a2 * sin2;
        let num_sq = num_re * num_re + num_im * num_im;
        let den_sq = (den_re * den_re + den_im * den_im).max(1e-18);
        10.0 * (num_sq / den_sq).max(1e-12).log10()
    }
}

#[derive(Clone, Copy, Default)]
struct BiquadState {
    z1_l: f32,
    z2_l: f32,
    z1_r: f32,
    z2_r: f32,
}

impl BiquadState {
    fn process_stereo(&mut self, coeffs: BiquadCoeffs, l: f32, r: f32) -> (f32, f32) {
        let out_l = coeffs.b0 * l + self.z1_l;
        self.z1_l = coeffs.b1 * l - coeffs.a1 * out_l + self.z2_l;
        self.z2_l = coeffs.b2 * l - coeffs.a2 * out_l;

        let out_r = coeffs.b0 * r + self.z1_r;
        self.z1_r = coeffs.b1 * r - coeffs.a1 * out_r + self.z2_r;
        self.z2_r = coeffs.b2 * r - coeffs.a2 * out_r;

        (
            if out_l.is_finite() { out_l } else { 0.0 },
            if out_r.is_finite() { out_r } else { 0.0 },
        )
    }
}

fn channel_band_coeffs(
    dsp: &ChannelDspConfig,
    sr: f32,
) -> (
    Option<BiquadCoeffs>,
    Option<BiquadCoeffs>,
    [Option<BiquadCoeffs>; 4],
) {
    let hpf = if dsp.hpf_enabled {
        Some(BiquadCoeffs::high_pass(dsp.hpf_freq_hz, sr))
    } else {
        None
    };
    let lpf = if dsp.lpf_enabled {
        Some(BiquadCoeffs::low_pass(dsp.lpf_freq_hz, sr))
    } else {
        None
    };
    let mut eq = [None; 4];
    if dsp.eq_enabled {
        for (i, band) in dsp.eq_bands.iter().enumerate() {
            if !band.enabled || band.gain_db.abs() < 0.05 {
                continue;
            }
            eq[i] = Some(match i {
                0 => BiquadCoeffs::low_shelf(band.freq_hz, band.gain_db, band.q, sr),
                3 => BiquadCoeffs::high_shelf(band.freq_hz, band.gain_db, band.q, sr),
                _ => BiquadCoeffs::peaking(band.freq_hz, band.gain_db, band.q, sr),
            });
        }
    }
    (hpf, lpf, eq)
}

pub fn eval_channel_eq_db(dsp: &ChannelDspConfig, freq_hz: f32, sample_rate: f32) -> f32 {
    let (hpf, lpf, bands) = channel_band_coeffs(dsp, sample_rate.max(8000.0));
    let mut db = 0.0_f32;
    if let Some(c) = hpf {
        db += c.magnitude_db(freq_hz, sample_rate);
    }
    if let Some(c) = lpf {
        db += c.magnitude_db(freq_hz, sample_rate);
    }
    for band_c in bands.into_iter().flatten() {
        db += band_c.magnitude_db(freq_hz, sample_rate);
    }
    db.clamp(-24.0, 24.0)
}

struct ChannelMeterAtomics {
    in_peak_l: AtomicU32,
    in_peak_r: AtomicU32,
    out_peak_l: AtomicU32,
    out_peak_r: AtomicU32,
    comp_gr_db: AtomicU32,
    media_pos_frames: AtomicU64,
    media_playing: AtomicBool,
    media_seq: AtomicU64,
}

impl Default for ChannelMeterAtomics {
    fn default() -> Self {
        Self {
            in_peak_l: AtomicU32::new(0),
            in_peak_r: AtomicU32::new(0),
            out_peak_l: AtomicU32::new(0),
            out_peak_r: AtomicU32::new(0),
            comp_gr_db: AtomicU32::new(0),
            media_pos_frames: AtomicU64::new(0),
            media_playing: AtomicBool::new(false),
            media_seq: AtomicU64::new(0),
        }
    }
}

struct SharedMixerTelemetry {
    channels: [ChannelMeterAtomics; MAX_MIXER_CHANNELS],
    master_peak_l: AtomicU32,
    master_peak_r: AtomicU32,
    virtual_mic_peak_l: AtomicU32,
    virtual_mic_peak_r: AtomicU32,
    virtual_mic_active: AtomicBool,
    recording_active: AtomicBool,
    recorded_frames: AtomicU64,
}

impl Default for SharedMixerTelemetry {
    fn default() -> Self {
        Self {
            channels: std::array::from_fn(|_| ChannelMeterAtomics::default()),
            master_peak_l: AtomicU32::new(0),
            master_peak_r: AtomicU32::new(0),
            virtual_mic_peak_l: AtomicU32::new(0),
            virtual_mic_peak_r: AtomicU32::new(0),
            virtual_mic_active: AtomicBool::new(false),
            recording_active: AtomicBool::new(false),
            recorded_frames: AtomicU64::new(0),
        }
    }
}

#[derive(Clone)]
struct RtInsertSlot {
    handle: Option<ActivePluginHandle>,
    bypass: bool,
    mix: f32,
}

impl Default for RtInsertSlot {
    fn default() -> Self {
        Self {
            handle: None,
            bypass: false,
            mix: 1.0,
        }
    }
}

struct RtMediaSlot {
    buffer: Option<Arc<MediaBuffer>>,
    playing: bool,
    looping: bool,
    pos_frames: f64,
    seq: u64,
}

impl Default for RtMediaSlot {
    fn default() -> Self {
        Self {
            buffer: None,
            playing: false,
            looping: false,
            pos_frames: 0.0,
            seq: 0,
        }
    }
}

impl RtMediaSlot {
    fn render_into(
        &mut self,
        buf_l: &mut [f32],
        buf_r: &mut [f32],
        frames: usize,
        out_sr: f64,
        trim_lin: f32,
    ) {
        buf_l[..frames].fill(0.0);
        buf_r[..frames].fill(0.0);
        if !self.playing {
            return;
        }
        let Some(ref buf) = self.buffer else {
            self.playing = false;
            return;
        };
        if buf.total_frames == 0 || buf.interleaved_stereo.len() < 2 {
            self.playing = false;
            return;
        }
        let step = (buf.sample_rate.max(1) as f64) / out_sr.max(1.0);
        let total_f = buf.total_frames as f64;
        for i in 0..frames {
            if self.pos_frames >= total_f {
                if self.looping {
                    self.pos_frames %= total_f;
                } else {
                    self.playing = false;
                    self.pos_frames = total_f;
                    break;
                }
            }
            let idx0 = (self.pos_frames.floor() as usize).min(buf.total_frames - 1);
            let next_raw = idx0 + 1;
            let idx1 = if next_raw < buf.total_frames {
                next_raw
            } else if self.looping {
                0
            } else {
                buf.total_frames - 1
            };
            let frac = (self.pos_frames - idx0 as f64).clamp(0.0, 1.0) as f32;
            let b0 = idx0 * 2;
            let b1 = idx1 * 2;
            let l0 = buf.interleaved_stereo[b0];
            let r0 = buf.interleaved_stereo[b0 + 1];
            let l1 = buf.interleaved_stereo[b1];
            let r1 = buf.interleaved_stereo[b1 + 1];
            buf_l[i] = (l0 + (l1 - l0) * frac) * trim_lin;
            buf_r[i] = (r0 + (r1 - r0) * frac) * trim_lin;
            self.pos_frames += step;
            if self.pos_frames >= total_f {
                if self.looping {
                    self.pos_frames %= total_f;
                } else {
                    self.playing = false;
                    self.pos_frames = total_f;
                    break;
                }
            }
        }
    }
}

struct RtChannelState {
    feed_idx: Option<usize>,
    mode: ChannelInputMode,
    dsp: ChannelDspConfig,
    hpf_state: BiquadState,
    lpf_state: BiquadState,
    eq_states: [BiquadState; 4],
    gate_env: f32,
    gate_gain: f32,
    comp_env: f32,
    inserts: [RtInsertSlot; MAX_MIXER_INSERTS],
    media: RtMediaSlot,
    rec_tx: Option<Producer<f32>>,
    buf_l: Vec<f32>,
    buf_r: Vec<f32>,
}

impl RtChannelState {
    fn new(max_frames: usize) -> Self {
        Self {
            feed_idx: None,
            mode: ChannelInputMode::Unassigned,
            dsp: ChannelDspConfig::default(),
            hpf_state: BiquadState::default(),
            lpf_state: BiquadState::default(),
            eq_states: [BiquadState::default(); 4],
            gate_env: 0.0,
            gate_gain: 1.0,
            comp_env: 0.0,
            inserts: std::array::from_fn(|_| RtInsertSlot::default()),
            media: RtMediaSlot::default(),
            rec_tx: None,
            buf_l: vec![0.0; max_frames],
            buf_r: vec![0.0; max_frames],
        }
    }
}

struct RecorderChannelTx {
    ch_tx: [Option<Producer<f32>>; MAX_MIXER_CHANNELS],
    master_tx: Option<Producer<f32>>,
}

enum MixerCommand {
    SetChannelCount(usize),
    SetChannelRouting {
        ch: usize,
        feed_idx: Option<usize>,
        mode: ChannelInputMode,
    },
    SetChannelDsp {
        ch: usize,
        dsp: ChannelDspConfig,
    },
    SetChannelMediaBuffer {
        ch: usize,
        buffer: Option<Arc<MediaBuffer>>,
        playing: bool,
        looping: bool,
        seek_frame: usize,
        seq: u64,
    },
    SetMediaTransport {
        ch: usize,
        playing: bool,
        looping: bool,
        seek_frame: Option<usize>,
        seq: u64,
    },
    SetChannelInsert {
        ch: usize,
        slot: usize,
        handle: Option<ActivePluginHandle>,
        bypass: bool,
        mix: f32,
    },
    SetChannelInsertBypassMix {
        ch: usize,
        slot: usize,
        bypass: bool,
        mix: f32,
    },
    SetInsertParam {
        target: MixerInsertTarget,
        id: u32,
        value: f64,
    },
    SetMaster {
        fader_db: f32,
        mute: bool,
        dim: bool,
    },
    SetMasterInsert {
        slot: usize,
        handle: Option<ActivePluginHandle>,
        bypass: bool,
        mix: f32,
    },
    SetMasterInsertBypassMix {
        slot: usize,
        bypass: bool,
        mix: f32,
    },
    SetRecorders(Option<Box<RecorderChannelTx>>),
    SetVirtualMicTx(Option<Producer<f32>>),
    SetVirtualMicGainDb(f32),
}

struct MixerDspProcessor {
    telemetry: Arc<SharedMixerTelemetry>,
    cmd_rx: Consumer<MixerCommand>,
    active_channels: usize,
    channels: Vec<RtChannelState>,
    master_fader_db: f32,
    master_mute: bool,
    master_dim: bool,
    master_inserts: [RtInsertSlot; MAX_MIXER_INSERTS],
    master_rec_tx: Option<Producer<f32>>,
    virtual_mic_tx: Option<Producer<f32>>,
    virtual_mic_gain_db: f32,
    vmic_l: Vec<f32>,
    vmic_r: Vec<f32>,
    vst_buffers: AudioBuffers,
    wet_l: Vec<f32>,
    wet_r: Vec<f32>,
}

impl MixerDspProcessor {
    fn new(
        telemetry: Arc<SharedMixerTelemetry>,
        cmd_rx: Consumer<MixerCommand>,
        active_channels: usize,
        sample_rate: f64,
    ) -> Self {
        let channels = (0..MAX_MIXER_CHANNELS)
            .map(|_| RtChannelState::new(4096))
            .collect();
        Self {
            telemetry,
            cmd_rx,
            active_channels: active_channels.clamp(1, MAX_MIXER_CHANNELS),
            channels,
            master_fader_db: 0.0,
            master_mute: false,
            master_dim: false,
            master_inserts: std::array::from_fn(|_| RtInsertSlot::default()),
            master_rec_tx: None,
            virtual_mic_tx: None,
            virtual_mic_gain_db: 0.0,
            vmic_l: vec![0.0; 4096],
            vmic_r: vec![0.0; 4096],
            vst_buffers: AudioBuffers::new(2, 2, 4096, sample_rate),
            wet_l: vec![0.0; 4096],
            wet_r: vec![0.0; 4096],
        }
    }

    fn drain_commands(&mut self) {
        let budget = self.cmd_rx.buffer().capacity();
        for _ in 0..budget {
            let Ok(cmd) = self.cmd_rx.pop() else {
                break;
            };
            match cmd {
                MixerCommand::SetChannelCount(n) => {
                    self.active_channels = n.clamp(1, MAX_MIXER_CHANNELS);
                }
                MixerCommand::SetChannelRouting { ch, feed_idx, mode } => {
                    if let Some(c) = self.channels.get_mut(ch) {
                        c.feed_idx = feed_idx;
                        c.mode = mode;
                    }
                }
                MixerCommand::SetChannelDsp { ch, dsp } => {
                    if let Some(c) = self.channels.get_mut(ch) {
                        c.dsp = dsp;
                    }
                }
                MixerCommand::SetChannelMediaBuffer {
                    ch,
                    buffer,
                    playing,
                    looping,
                    seek_frame,
                    seq,
                } => {
                    if let Some(c) = self.channels.get_mut(ch) {
                        c.media.buffer = buffer;
                        c.media.playing = playing;
                        c.media.looping = looping;
                        c.media.pos_frames = seek_frame as f64;
                        c.media.seq = seq;
                    }
                }
                MixerCommand::SetMediaTransport {
                    ch,
                    playing,
                    looping,
                    seek_frame,
                    seq,
                } => {
                    if let Some(c) = self.channels.get_mut(ch) {
                        c.media.playing = playing;
                        c.media.looping = looping;
                        if let Some(frame) = seek_frame {
                            c.media.pos_frames = frame as f64;
                        }
                        c.media.seq = seq;
                    }
                }
                MixerCommand::SetChannelInsert {
                    ch,
                    slot,
                    handle,
                    bypass,
                    mix,
                } => {
                    if let Some(s) = self.channels.get_mut(ch).and_then(|c| c.inserts.get_mut(slot)) {
                        s.handle = handle;
                        s.bypass = bypass;
                        s.mix = mix;
                    }
                }
                MixerCommand::SetChannelInsertBypassMix {
                    ch,
                    slot,
                    bypass,
                    mix,
                } => {
                    if let Some(s) = self.channels.get_mut(ch).and_then(|c| c.inserts.get_mut(slot)) {
                        s.bypass = bypass;
                        s.mix = mix;
                    }
                }
                MixerCommand::SetInsertParam { target, id, value } => {
                    let handle = match target {
                        MixerInsertTarget::Channel { ch, slot } => self
                            .channels
                            .get(ch)
                            .and_then(|c| c.inserts.get(slot))
                            .and_then(|s| s.handle.as_ref()),
                        MixerInsertTarget::Master { slot } => {
                            self.master_inserts.get(slot).and_then(|s| s.handle.as_ref())
                        }
                    };
                    if let Some(h) = handle {
                        set_backend_param_live(h, id, value);
                    }
                }
                MixerCommand::SetMaster {
                    fader_db,
                    mute,
                    dim,
                } => {
                    self.master_fader_db = fader_db;
                    self.master_mute = mute;
                    self.master_dim = dim;
                }
                MixerCommand::SetMasterInsert {
                    slot,
                    handle,
                    bypass,
                    mix,
                } => {
                    if let Some(s) = self.master_inserts.get_mut(slot) {
                        s.handle = handle;
                        s.bypass = bypass;
                        s.mix = mix;
                    }
                }
                MixerCommand::SetMasterInsertBypassMix { slot, bypass, mix } => {
                    if let Some(s) = self.master_inserts.get_mut(slot) {
                        s.bypass = bypass;
                        s.mix = mix;
                    }
                }
                MixerCommand::SetRecorders(maybe_rec) => match maybe_rec {
                    Some(mut rec) => {
                        for (c, tx) in self.channels.iter_mut().zip(rec.ch_tx.iter_mut()) {
                            c.rec_tx = tx.take();
                        }
                        self.master_rec_tx = rec.master_tx.take();
                    }
                    None => {
                        for c in &mut self.channels {
                            c.rec_tx = None;
                        }
                        self.master_rec_tx = None;
                    }
                },
                MixerCommand::SetVirtualMicTx(tx) => {
                    self.virtual_mic_tx = tx;
                }
                MixerCommand::SetVirtualMicGainDb(gain_db) => {
                    self.virtual_mic_gain_db = gain_db;
                }
            }
        }
    }
}

fn push_stereo_to_ring(tx: &mut Producer<f32>, l: &[f32], r: &[f32], frames: usize) -> u64 {
    let mut pushed = 0_u64;
    for i in 0..frames {
        if tx.slots() >= 2 {
            let _ = tx.push(l[i]);
            let _ = tx.push(r[i]);
            pushed += 1;
        } else {
            break;
        }
    }
    pushed
}

impl MixerAudioProcessor for MixerDspProcessor {
    fn process(
        &mut self,
        feeds: &[&[f32]],
        feed_channels: &[usize],
        out_left: &mut [f32],
        out_right: &mut [f32],
        frames: usize,
        sample_rate: f64,
    ) {
        self.drain_commands();
        out_left[..frames].fill(0.0);
        out_right[..frames].fill(0.0);
        if self.vmic_l.len() < frames {
            self.vmic_l.resize(frames, 0.0);
            self.vmic_r.resize(frames, 0.0);
        }
        self.vmic_l[..frames].fill(0.0);
        self.vmic_r[..frames].fill(0.0);

        let sr = sample_rate.max(8000.0) as f32;
        let active_n = self.active_channels.min(self.channels.len());
        let any_solo = self.channels[..active_n].iter().any(|c| c.dsp.solo);
        let recording_on = self.telemetry.recording_active.load(Ordering::Relaxed);
        let mut recorded_this_block: u64 = 0;

        let gate_att = (-1.0 / (0.0015 * sr)).exp();
        let gate_rel = (-1.0 / (0.140 * sr)).exp();
        let gate_gain_smooth = (-1.0 / (0.008 * sr)).exp();
        let comp_att = (-1.0 / (0.005 * sr)).exp();
        let comp_rel = (-1.0 / (0.090 * sr)).exp();

        for ch_idx in 0..active_n {
            let ch = &mut self.channels[ch_idx];
            if ch.buf_l.len() < frames {
                ch.buf_l.resize(frames, 0.0);
                ch.buf_r.resize(frames, 0.0);
            }

            let trim_lin =
                db_to_linear(ch.dsp.trim_db) * if ch.dsp.phase_invert { -1.0 } else { 1.0 };
            let m_atomics = &self.telemetry.channels[ch_idx];

            if ch.mode == ChannelInputMode::Multimedia {
                ch.media
                    .render_into(&mut ch.buf_l, &mut ch.buf_r, frames, sample_rate, trim_lin);
                m_atomics.media_pos_frames.store(
                    ch.media.pos_frames.round().max(0.0) as u64,
                    Ordering::Relaxed,
                );
                m_atomics
                    .media_playing
                    .store(ch.media.playing, Ordering::Relaxed);
                m_atomics.media_seq.store(ch.media.seq, Ordering::Relaxed);
            } else {
                let Some(f_idx) = ch.feed_idx else {
                    if recording_on {
                        if let Some(ref mut tx) = ch.rec_tx {
                            ch.buf_l[..frames].fill(0.0);
                            ch.buf_r[..frames].fill(0.0);
                            let n = push_stereo_to_ring(tx, &ch.buf_l, &ch.buf_r, frames);
                            recorded_this_block = recorded_this_block.max(n);
                        }
                    }
                    continue;
                };
                let Some(&feed) = feeds.get(f_idx) else {
                    continue;
                };
                let feed_ch = *feed_channels.get(f_idx).unwrap_or(&0);
                if feed_ch == 0 {
                    continue;
                }

                for i in 0..frames {
                    let base = i * feed_ch;
                    let (mut l, mut r) = match ch.mode {
                        ChannelInputMode::Unassigned | ChannelInputMode::Multimedia => (0.0, 0.0),
                        ChannelInputMode::Mono(c) => {
                            let s = if c < feed_ch && base + c < feed.len() {
                                feed[base + c]
                            } else {
                                0.0
                            };
                            (s, s)
                        }
                        ChannelInputMode::Stereo(cl, cr) => {
                            let sl = if cl < feed_ch && base + cl < feed.len() {
                                feed[base + cl]
                            } else {
                                0.0
                            };
                            let sr_s = if cr < feed_ch && base + cr < feed.len() {
                                feed[base + cr]
                            } else {
                                sl
                            };
                            (sl, sr_s)
                        }
                    };
                    l *= trim_lin;
                    r *= trim_lin;
                    ch.buf_l[i] = l;
                    ch.buf_r[i] = r;
                }
            }

            let bal = ch.dsp.balance.clamp(-1.0, 1.0);
            let bal_l = if bal > 0.0 { 1.0 - bal } else { 1.0 };
            let bal_r = if bal < 0.0 { 1.0 + bal } else { 1.0 };
            let width = ch.dsp.stereo_width.clamp(0.0, 2.0);
            let swap_lr = ch.dsp.swap_lr;
            if swap_lr || (bal - 0.0).abs() > 1e-4 || (width - 1.0).abs() > 1e-4 {
                for i in 0..frames {
                    let mut l = ch.buf_l[i];
                    let mut r = ch.buf_r[i];
                    if swap_lr {
                        std::mem::swap(&mut l, &mut r);
                    }
                    l *= bal_l;
                    r *= bal_r;
                    let mid = 0.5 * (l + r);
                    let side = 0.5 * (l - r) * width;
                    ch.buf_l[i] = mid + side;
                    ch.buf_r[i] = mid - side;
                }
            }

            store_peak(&m_atomics.in_peak_l, slice_peak(&ch.buf_l[..frames]));
            store_peak(&m_atomics.in_peak_r, slice_peak(&ch.buf_r[..frames]));

            let (hpf_coeffs, lpf_coeffs, eq_coeffs) = channel_band_coeffs(&ch.dsp, sr);
            let gate_thresh_lin = db_to_linear(ch.dsp.gate_threshold_db);
            let comp_thresh = ch.dsp.comp_threshold_db;
            let comp_ratio = ch.dsp.comp_ratio.max(1.0);
            let comp_makeup = db_to_linear(ch.dsp.comp_makeup_db);
            let mut max_gr_db = 0.0_f32;

            for i in 0..frames {
                let mut l = ch.buf_l[i];
                let mut r = ch.buf_r[i];

                if let Some(c) = hpf_coeffs {
                    (l, r) = ch.hpf_state.process_stereo(c, l, r);
                }
                if let Some(c) = lpf_coeffs {
                    (l, r) = ch.lpf_state.process_stereo(c, l, r);
                }

                if ch.dsp.gate_enabled {
                    let level = l.abs().max(r.abs());
                    let c_env = if level > ch.gate_env { gate_att } else { gate_rel };
                    ch.gate_env = level + c_env * (ch.gate_env - level);
                    let target = if ch.gate_env >= gate_thresh_lin { 1.0 } else { 0.0 };
                    ch.gate_gain = target + gate_gain_smooth * (ch.gate_gain - target);
                    l *= ch.gate_gain;
                    r *= ch.gate_gain;
                } else {
                    ch.gate_gain = 1.0;
                }

                for (b_idx, maybe_c) in eq_coeffs.iter().enumerate() {
                    if let Some(c) = *maybe_c {
                        (l, r) = ch.eq_states[b_idx].process_stereo(c, l, r);
                    }
                }

                if ch.dsp.comp_enabled {
                    let peak = l.abs().max(r.abs()).max(1e-7);
                    let env_c = if peak > ch.comp_env { comp_att } else { comp_rel };
                    ch.comp_env = peak + env_c * (ch.comp_env - peak);
                    let env_db = 20.0 * ch.comp_env.max(1e-7).log10();
                    let over = (env_db - comp_thresh).max(0.0);
                    let gr_db = over * (1.0 - 1.0 / comp_ratio);
                    max_gr_db = max_gr_db.max(gr_db);
                    let comp_gain = db_to_linear(-gr_db) * comp_makeup;
                    l *= comp_gain;
                    r *= comp_gain;
                }

                ch.buf_l[i] = l;
                ch.buf_r[i] = r;
            }
            store_peak(&m_atomics.comp_gr_db, max_gr_db);

            if !ch.dsp.fx_bypass {
                for slot in &ch.inserts {
                    if !slot.bypass {
                        if let Some(ref handle) = slot.handle {
                            process_active_handle(
                                handle,
                                &mut self.vst_buffers,
                                &mut self.wet_l,
                                &mut self.wet_r,
                                &mut ch.buf_l[..frames],
                                &mut ch.buf_r[..frames],
                                frames,
                                sample_rate,
                                slot.mix,
                            );
                        }
                    }
                }
            }

            let angle = (ch.dsp.pan.clamp(-1.0, 1.0) + 1.0) * (PI * 0.25);
            let pan_l = angle.cos() * SQRT_2;
            let pan_r = angle.sin() * SQRT_2;
            let fader_lin = if ch.dsp.mute { 0.0 } else { db_to_linear(ch.dsp.fader_db) };
            let solo_mult = if !any_solo || ch.dsp.solo || ch.dsp.solo_safe {
                1.0
            } else {
                0.0
            };

            if recording_on && ch.dsp.pre_fader_rec {
                if let Some(ref mut tx) = ch.rec_tx {
                    let n = push_stereo_to_ring(tx, &ch.buf_l, &ch.buf_r, frames);
                    recorded_this_block = recorded_this_block.max(n);
                }
            }

            let send_vmic = ch.dsp.send_to_virtual_mic;
            let mut post_peak_l = 0.0_f32;
            let mut post_peak_r = 0.0_f32;
            for i in 0..frames {
                let pl = ch.buf_l[i] * fader_lin * pan_l;
                let pr = ch.buf_r[i] * fader_lin * pan_r;
                ch.buf_l[i] = pl;
                ch.buf_r[i] = pr;
                post_peak_l = post_peak_l.max(pl.abs());
                post_peak_r = post_peak_r.max(pr.abs());
                let bus_l = pl * solo_mult;
                let bus_r = pr * solo_mult;
                out_left[i] += bus_l;
                out_right[i] += bus_r;
                if send_vmic {
                    self.vmic_l[i] += bus_l;
                    self.vmic_r[i] += bus_r;
                }
            }

            if recording_on && !ch.dsp.pre_fader_rec {
                if let Some(ref mut tx) = ch.rec_tx {
                    let n = push_stereo_to_ring(tx, &ch.buf_l, &ch.buf_r, frames);
                    recorded_this_block = recorded_this_block.max(n);
                }
            }

            store_peak(&m_atomics.out_peak_l, post_peak_l);
            store_peak(&m_atomics.out_peak_r, post_peak_r);
        }

        for slot in &self.master_inserts {
            if !slot.bypass {
                if let Some(ref handle) = slot.handle {
                    process_active_handle(
                        handle,
                        &mut self.vst_buffers,
                        &mut self.wet_l,
                        &mut self.wet_r,
                        &mut out_left[..frames],
                        &mut out_right[..frames],
                        frames,
                        sample_rate,
                        slot.mix,
                    );
                }
            }
        }

        let master_lin = if self.master_mute {
            0.0
        } else {
            db_to_linear(self.master_fader_db) * if self.master_dim { 0.25 } else { 1.0 }
        };
        for i in 0..frames {
            out_left[i] *= master_lin;
            out_right[i] *= master_lin;
        }

        let vmic_lin = if self.master_mute {
            0.0
        } else {
            db_to_linear(self.master_fader_db) * db_to_linear(self.virtual_mic_gain_db)
        };
        for i in 0..frames {
            self.vmic_l[i] = (self.vmic_l[i] * vmic_lin).clamp(-1.0, 1.0);
            self.vmic_r[i] = (self.vmic_r[i] * vmic_lin).clamp(-1.0, 1.0);
        }
        store_peak(
            &self.telemetry.virtual_mic_peak_l,
            slice_peak(&self.vmic_l[..frames]),
        );
        store_peak(
            &self.telemetry.virtual_mic_peak_r,
            slice_peak(&self.vmic_r[..frames]),
        );
        if let Some(ref mut vtx) = self.virtual_mic_tx {
            let _ = push_stereo_to_ring(vtx, &self.vmic_l, &self.vmic_r, frames);
        }

        if recording_on {
            if let Some(ref mut tx) = self.master_rec_tx {
                let n = push_stereo_to_ring(tx, out_left, out_right, frames);
                recorded_this_block = recorded_this_block.max(n);
            }
            if recorded_this_block > 0 {
                self.telemetry
                    .recorded_frames
                    .fetch_add(recorded_this_block, Ordering::Relaxed);
            }
        }

        store_peak(&self.telemetry.master_peak_l, slice_peak(&out_left[..frames]));
        store_peak(&self.telemetry.master_peak_r, slice_peak(&out_right[..frames]));
    }
}

struct BackendSlot {
    backend: LoadedBackend,
    window: Option<PluginWindow>,
}

struct ActiveMixerRecording {
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<Result<(), String>>>,
    report: MixerRecordingReport,
}

struct ActiveVirtualMicPipe {
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<()>>,
    child: Arc<Mutex<Option<Child>>>,
}

impl ActiveVirtualMicPipe {
    fn stop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Ok(mut guard) = self.child.lock() {
            if let Some(ref mut c) = *guard {
                let _ = c.kill();
                let _ = c.wait();
            }
            *guard = None;
        }
        if let Some(jh) = self.join_handle.take() {
            let _ = jh.join();
        }
    }
}

struct ActiveVirtualMic {
    sink_module_id: Option<u32>,
    source_module_id: Option<u32>,
    pipe: Option<ActiveVirtualMicPipe>,
}

const DEFAULT_WING_COLORS: [[u8; 3]; 1] = [
    [20, 22, 28],
];

pub struct MixerEngine {
    telemetry: Arc<SharedMixerTelemetry>,
    session: Option<MixerAudioSession>,
    cmd_tx: Option<Producer<MixerCommand>>,
    active_output_device: Option<String>,
    active_feed_ids: Vec<String>,
    channels: Vec<MixerChannelState>,
    master: MasterBusState,
    channel_backends: Vec<[Option<BackendSlot>; MAX_MIXER_INSERTS]>,
    channel_media_buffers: Vec<Option<Arc<MediaBuffer>>>,
    channel_media_seqs: Vec<u64>,
    retired_media_buffers: Vec<Arc<MediaBuffer>>,
    pending_media_loads: Arc<Mutex<Vec<(usize, PathBuf, Result<MediaBuffer, String>)>>>,
    master_backends: [Option<BackendSlot>; MAX_MIXER_INSERTS],
    recording: Option<ActiveMixerRecording>,
    virtual_mic: Option<ActiveVirtualMic>,
    sample_rate: u32,
    block_size: usize,
    wine_config: WineConfig,
}

impl Default for MixerEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl MixerEngine {
    pub fn new() -> Self {
        let channels = (0..DEFAULT_MIXER_CHANNELS)
            .map(Self::make_default_channel)
            .collect();
        let channel_backends = (0..MAX_MIXER_CHANNELS)
            .map(|_| std::array::from_fn(|_| None))
            .collect();
        Self {
            telemetry: Arc::new(SharedMixerTelemetry::default()),
            session: None,
            cmd_tx: None,
            active_output_device: None,
            active_feed_ids: Vec::new(),
            channels,
            master: MasterBusState::default(),
            channel_backends,
            channel_media_buffers: vec![None; MAX_MIXER_CHANNELS],
            channel_media_seqs: vec![0; MAX_MIXER_CHANNELS],
            retired_media_buffers: Vec::new(),
            pending_media_loads: Arc::new(Mutex::new(Vec::new())),
            master_backends: std::array::from_fn(|_| None),
            recording: None,
            virtual_mic: None,
            sample_rate: 48_000,
            block_size: 256,
            wine_config: WineConfig::default(),
        }
    }

    fn make_default_channel(index: usize) -> MixerChannelState {
        MixerChannelState {
            index,
            name: format!("CH {:02}", index + 1),
            color: DEFAULT_WING_COLORS[index % DEFAULT_WING_COLORS.len()],
            icon: ChannelIcon::None,
            source: ChannelInputSource::default(),
            dsp: ChannelDspConfig::default(),
            inserts: std::array::from_fn(|_| InsertSlotInfo::default()),
            media: MediaTrackState::default(),
        }
    }

    pub fn set_wine_config(&mut self, config: WineConfig) {
        self.wine_config = config;
    }

    pub fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>, EngineError> {
        Ok(pedal_audio::list_input_devices()?)
    }

    pub fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>, EngineError> {
        Ok(pedal_audio::list_output_devices()?)
    }

    pub fn default_output_device(&self) -> Option<String> {
        pedal_audio::default_output_device_id()
    }

    pub fn channels(&self) -> &[MixerChannelState] {
        &self.channels
    }

    pub fn channel(&self, idx: usize) -> Option<&MixerChannelState> {
        self.channels.get(idx)
    }

    pub fn master(&self) -> &MasterBusState {
        &self.master
    }

    pub fn sample_rate(&self) -> u32 {
        self.session
            .as_ref()
            .map(|s| s.info.output_sample_rate)
            .unwrap_or(self.sample_rate)
    }

    pub fn block_size(&self) -> usize {
        self.block_size
    }

    pub fn is_audio_running(&self) -> bool {
        self.session.is_some()
    }

    pub fn active_audio_info(&self) -> Option<&ActiveMixerInfo> {
        self.session.as_ref().map(|s| &s.info)
    }

    pub fn audio_stats(&self) -> Option<Arc<AudioStats>> {
        self.session.as_ref().map(|s| Arc::clone(&s.stats))
    }

    pub fn is_recording(&self) -> bool {
        self.telemetry.recording_active.load(Ordering::Relaxed)
    }

    pub fn active_recording_dir(&self) -> Option<&Path> {
        self.recording.as_ref().map(|r| r.report.directory.as_path())
    }

    pub fn add_channel(&mut self) -> Option<usize> {
        if self.channels.len() >= MAX_MIXER_CHANNELS {
            return None;
        }
        let idx = self.channels.len();
        self.channels.push(Self::make_default_channel(idx));
        self.channel_media_buffers[idx] = None;
        self.channel_media_seqs[idx] = 0;
        if self.cmd_tx.is_some() {
            if let Some(ref mut tx) = self.cmd_tx {
                let _ = tx.push(MixerCommand::SetChannelCount(self.channels.len()));
            }
            Self::push_channel_state_to_rt(
                &mut self.cmd_tx,
                &self.active_feed_ids,
                &self.channels[idx],
                &self.channel_backends[idx],
                &self.channel_media_buffers[idx],
                self.channel_media_seqs[idx],
            );
        }
        Some(idx)
    }

    pub fn add_multimedia_channel(&mut self) -> Option<usize> {
        let idx = self.add_channel()?;
        self.set_channel_multimedia(idx);
        Some(idx)
    }

    pub fn remove_channel(&mut self, idx: usize) -> bool {
        if self.channels.len() <= 1 || idx >= self.channels.len() {
            return false;
        }
        for slot in 0..MAX_MIXER_INSERTS {
            self.unload_insert_plugin(MixerInsertTarget::Channel { ch: idx, slot });
        }
        if let Some(old_buf) = self.channel_media_buffers[idx].take() {
            self.retired_media_buffers.push(old_buf);
        }
        self.channels.remove(idx);
        self.channel_backends.remove(idx);
        self.channel_backends.push(std::array::from_fn(|_| None));
        self.channel_media_buffers.remove(idx);
        self.channel_media_buffers.push(None);
        self.channel_media_seqs.remove(idx);
        self.channel_media_seqs.push(0);

        let new_len = self.channels.len();
        for (i, ch) in self.channels.iter_mut().enumerate() {
            ch.index = i;
        }

        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetChannelCount(new_len));
            for slot in 0..MAX_MIXER_INSERTS {
                let _ = tx.push(MixerCommand::SetChannelInsert {
                    ch: new_len,
                    slot,
                    handle: None,
                    bypass: false,
                    mix: 1.0,
                });
            }
            let _ = tx.push(MixerCommand::SetChannelMediaBuffer {
                ch: new_len,
                buffer: None,
                playing: false,
                looping: false,
                seek_frame: 0,
                seq: 0,
            });
            let _ = tx.push(MixerCommand::SetChannelRouting {
                ch: new_len,
                feed_idx: None,
                mode: ChannelInputMode::Unassigned,
            });
        }
        if self.cmd_tx.is_some() {
            for ch_idx in idx..new_len {
                Self::push_channel_state_to_rt(
                    &mut self.cmd_tx,
                    &self.active_feed_ids,
                    &self.channels[ch_idx],
                    &self.channel_backends[ch_idx],
                    &self.channel_media_buffers[ch_idx],
                    self.channel_media_seqs[ch_idx],
                );
            }
        }
        true
    }

    pub fn remove_last_channel(&mut self) -> bool {
        if self.channels.len() <= 1 {
            return false;
        }
        self.remove_channel(self.channels.len() - 1)
    }

    pub fn set_channel_name(&mut self, ch: usize, name: String) {
        if let Some(c) = self.channels.get_mut(ch) {
            c.name = name;
        }
    }

    pub fn set_channel_color(&mut self, ch: usize, color: [u8; 3]) {
        if let Some(c) = self.channels.get_mut(ch) {
            c.color = color;
        }
    }

    pub fn set_channel_icon(&mut self, ch: usize, icon: ChannelIcon) {
        if let Some(c) = self.channels.get_mut(ch) {
            c.icon = icon;
        }
    }

    pub fn set_channel_dsp(&mut self, ch: usize, dsp: ChannelDspConfig) {
        if let Some(c) = self.channels.get_mut(ch) {
            c.dsp = dsp;
            if let Some(ref mut tx) = self.cmd_tx {
                let _ = tx.push(MixerCommand::SetChannelDsp { ch, dsp });
            }
        }
    }

    pub fn clear_all_solos(&mut self) {
        for ch_idx in 0..self.channels.len() {
            if self.channels[ch_idx].dsp.solo {
                let mut dsp = self.channels[ch_idx].dsp;
                dsp.solo = false;
                self.set_channel_dsp(ch_idx, dsp);
            }
        }
    }

    pub fn set_master(&mut self, fader_db: f32, mute: bool, dim: bool, record_mix: bool) {
        self.master.fader_db = fader_db;
        self.master.mute = mute;
        self.master.dim = dim;
        self.master.record_mix = record_mix;
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetMaster {
                fader_db,
                mute,
                dim,
            });
        }
    }

    pub fn set_virtual_mic_gain_db(&mut self, gain_db: f32) {
        let clamped = gain_db.clamp(-24.0, 18.0);
        self.master.virtual_mic_gain_db = clamped;
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetVirtualMicGainDb(clamped));
        }
    }

    pub fn is_virtual_mic_active(&self) -> bool {
        self.virtual_mic.is_some() && self.master.virtual_mic_enabled
    }

    pub fn set_channel_multimedia(&mut self, ch: usize) {
        if ch >= self.channels.len() {
            return;
        }
        let default_audio_name = format!("CH {:02}", ch + 1);
        if self.channels[ch].name == default_audio_name {
            self.channels[ch].name = format!("MEDIA {:02}", ch + 1);
        }
        if self.channels[ch].icon == ChannelIcon::None {
            self.channels[ch].icon = ChannelIcon::Media;
        }
        self.channels[ch].source = ChannelInputSource {
            device_id: None,
            device_label: "MEDIA".into(),
            mode: ChannelInputMode::Multimedia,
        };
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetChannelRouting {
                ch,
                feed_idx: None,
                mode: ChannelInputMode::Multimedia,
            });
        }
    }

    pub fn set_channel_input(
        &mut self,
        ch: usize,
        source: ChannelInputSource,
        fallback_output_device: Option<&str>,
    ) -> Result<(), EngineError> {
        if ch >= self.channels.len() {
            return Ok(());
        }
        if source.mode == ChannelInputMode::Multimedia {
            self.set_channel_multimedia(ch);
            return Ok(());
        }
        if self.channels[ch].source.mode == ChannelInputMode::Multimedia {
            self.channels[ch].media.playing = false;
            self.channel_media_seqs[ch] += 1;
            if let Some(ref mut tx) = self.cmd_tx {
                let _ = tx.push(MixerCommand::SetMediaTransport {
                    ch,
                    playing: false,
                    looping: self.channels[ch].media.looping,
                    seek_frame: None,
                    seq: self.channel_media_seqs[ch],
                });
            }
            let default_media_name = format!("MEDIA {:02}", ch + 1);
            if self.channels[ch].name == default_media_name {
                self.channels[ch].name = format!("CH {:02}", ch + 1);
            }
        }
        self.channels[ch].source = source.clone();

        if self.session.is_some() {
            let needs_restart = match (&source.device_id, source.mode) {
                (Some(id), mode) if mode != ChannelInputMode::Unassigned => {
                    !self.active_feed_ids.iter().any(|existing| existing == id)
                }
                _ => false,
            };

            if needs_restart {
                if let Some(out_id) = self.active_output_device.clone() {
                    return self.start_audio(&out_id, self.sample_rate, self.block_size);
                }
            } else if let Some(ref mut tx) = self.cmd_tx {
                let feed_idx = source
                    .device_id
                    .as_ref()
                    .and_then(|id| self.active_feed_ids.iter().position(|f| f == id));
                let _ = tx.push(MixerCommand::SetChannelRouting {
                    ch,
                    feed_idx,
                    mode: source.mode,
                });
            }
        } else if source.device_id.is_some() && source.mode != ChannelInputMode::Unassigned {
            return self.ensure_audio_running(fallback_output_device);
        }

        Ok(())
    }

    fn ensure_audio_running(&mut self, fallback_output_device: Option<&str>) -> Result<(), EngineError> {
        if self.session.is_some() {
            return Ok(());
        }
        if let Some(out_id) = self
            .active_output_device
            .clone()
            .or_else(|| fallback_output_device.filter(|s| !s.is_empty()).map(ToString::to_string))
            .or_else(|| self.default_output_device())
        {
            return self.start_audio(&out_id, self.sample_rate, self.block_size);
        }
        Err(EngineError::Audio(pedal_audio::AudioError::DeviceNotFound(
            "No output device selected".into(),
        )))
    }

    pub fn request_load_media(&mut self, ch: usize, path: PathBuf) {
        if ch >= self.channels.len() {
            return;
        }
        self.set_channel_multimedia(ch);
        let fallback_title = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Media")
            .to_string();
        self.channels[ch].media.loading = true;
        self.channels[ch].media.path = Some(path.clone());
        self.channels[ch].media.title = fallback_title;

        let target_sr = self.sample_rate();
        let slot = Arc::clone(&self.pending_media_loads);
        thread::spawn(move || {
            let outcome = decode_audio_file(&path, target_sr).map_err(|e| e.to_string());
            if let Ok(mut guard) = slot.lock() {
                guard.push((ch, path, outcome));
            }
        });
    }

    pub fn load_media_file_sync(
        &mut self,
        ch: usize,
        path: &Path,
        auto_play: bool,
        fallback_output_device: Option<&str>,
    ) -> Result<String, EngineError> {
        let buf = decode_audio_file(path, self.sample_rate())?;
        self.install_channel_media_buffer(ch, path.to_path_buf(), buf, auto_play, fallback_output_device)
    }

    fn install_channel_media_buffer(
        &mut self,
        ch: usize,
        path: PathBuf,
        buf: MediaBuffer,
        auto_play: bool,
        fallback_output_device: Option<&str>,
    ) -> Result<String, EngineError> {
        if ch >= self.channels.len() {
            return Err(EngineError::InvalidPluginPath("Invalid mixer channel".into()));
        }
        self.set_channel_multimedia(ch);
        let title = buf.title.clone();
        let looping = self.channels[ch].media.looping;
        self.channels[ch].media = MediaTrackState {
            path: Some(path),
            title: title.clone(),
            sample_rate: buf.sample_rate,
            total_frames: buf.total_frames,
            current_frame: 0,
            playing: auto_play,
            looping,
            loading: false,
        };
        let arc = Arc::new(buf);
        if let Some(old) = self.channel_media_buffers[ch].replace(Arc::clone(&arc)) {
            self.retired_media_buffers.push(old);
        }
        self.channel_media_seqs[ch] += 1;
        let seq = self.channel_media_seqs[ch];

        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetChannelMediaBuffer {
                ch,
                buffer: Some(arc),
                playing: auto_play,
                looping,
                seek_frame: 0,
                seq,
            });
        } else if auto_play {
            self.ensure_audio_running(fallback_output_device)?;
        }
        Ok(title)
    }

    pub fn poll_media_loads(
        &mut self,
        fallback_output_device: Option<&str>,
    ) -> Vec<(usize, Result<String, String>)> {
        self.retired_media_buffers
            .retain(|arc| Arc::strong_count(arc) > 1);
        let finished: Vec<(usize, PathBuf, Result<MediaBuffer, String>)> = self
            .pending_media_loads
            .lock()
            .map(|mut g| g.drain(..).collect())
            .unwrap_or_default();
        let mut results = Vec::with_capacity(finished.len());
        for (ch, path, outcome) in finished {
            if let Some(c) = self.channels.get_mut(ch) {
                c.media.loading = false;
            }
            match outcome {
                Ok(buf) => {
                    match self.install_channel_media_buffer(
                        ch,
                        path,
                        buf,
                        true,
                        fallback_output_device,
                    ) {
                        Ok(title) => results.push((ch, Ok(title))),
                        Err(e) => results.push((ch, Err(e.to_string()))),
                    }
                }
                Err(err) => results.push((ch, Err(err))),
            }
        }
        results
    }

    pub fn set_media_transport(
        &mut self,
        ch: usize,
        playing: bool,
        looping: bool,
        seek_frame: Option<usize>,
        fallback_output_device: Option<&str>,
    ) -> Result<(), EngineError> {
        if ch >= self.channels.len() {
            return Ok(());
        }
        let has_buffer = self.channel_media_buffers[ch].is_some();
        let next_playing = playing && has_buffer;
        let clamped_seek = seek_frame.map(|f| f.min(self.channels[ch].media.total_frames));
        self.channels[ch].media.playing = next_playing;
        self.channels[ch].media.looping = looping;
        if let Some(f) = clamped_seek {
            self.channels[ch].media.current_frame = f;
        }
        self.channel_media_seqs[ch] += 1;
        let seq = self.channel_media_seqs[ch];

        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetMediaTransport {
                ch,
                playing: next_playing,
                looping,
                seek_frame: clamped_seek,
                seq,
            });
        } else if next_playing {
            self.ensure_audio_running(fallback_output_device)?;
        }
        Ok(())
    }

    pub fn seek_media_relative_seconds(
        &mut self,
        ch: usize,
        delta_secs: f64,
        fallback_output_device: Option<&str>,
    ) -> Result<(), EngineError> {
        let Some(c) = self.channels.get(ch) else {
            return Ok(());
        };
        if c.media.total_frames == 0 {
            return Ok(());
        }
        let sr = c.media.sample_rate.max(1) as f64;
        let cur = c.media.current_frame as f64;
        let max_f = c.media.total_frames as f64;
        let target = (cur + delta_secs * sr).round().clamp(0.0, max_f) as usize;
        let playing = c.media.playing;
        let looping = c.media.looping;
        self.set_media_transport(ch, playing, looping, Some(target), fallback_output_device)
    }

    pub fn is_any_media_playing(&self) -> bool {
        self.channels
            .iter()
            .any(|c| c.source.mode == ChannelInputMode::Multimedia && c.media.playing)
    }

    pub fn count_media_channels(&self) -> usize {
        self.channels
            .iter()
            .filter(|c| c.source.mode == ChannelInputMode::Multimedia)
            .count()
    }

    pub fn set_all_media_transport(
        &mut self,
        playing: bool,
        looping: Option<bool>,
        seek_frame: Option<usize>,
        fallback_output_device: Option<&str>,
    ) -> Result<(), EngineError> {
        let indices: Vec<usize> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(_, c)| c.source.mode == ChannelInputMode::Multimedia)
            .map(|(i, _)| i)
            .collect();
        for ch in indices {
            let loop_val = looping.unwrap_or(self.channels[ch].media.looping);
            let seek_val = if !playing && seek_frame == Some(0) {
                Some(0)
            } else if playing && self.channels[ch].media.current_frame >= self.channels[ch].media.total_frames {
                Some(0)
            } else {
                seek_frame
            };
            let _ = self.set_media_transport(ch, playing, loop_val, seek_val, fallback_output_device);
        }
        Ok(())
    }

    pub fn seek_all_media_relative_seconds(
        &mut self,
        delta_secs: f64,
        fallback_output_device: Option<&str>,
    ) -> Result<(), EngineError> {
        let indices: Vec<usize> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(_, c)| c.source.mode == ChannelInputMode::Multimedia && c.media.total_frames > 0)
            .map(|(i, _)| i)
            .collect();
        for ch in indices {
            let _ = self.seek_media_relative_seconds(ch, delta_secs, fallback_output_device);
        }
        Ok(())
    }

    pub fn unload_channel_media(&mut self, ch: usize) {
        if ch >= self.channels.len() {
            return;
        }
        if let Some(old) = self.channel_media_buffers[ch].take() {
            self.retired_media_buffers.push(old);
        }
        self.channels[ch].media = MediaTrackState::default();
        self.channel_media_seqs[ch] += 1;
        let seq = self.channel_media_seqs[ch];
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetChannelMediaBuffer {
                ch,
                buffer: None,
                playing: false,
                looping: false,
                seek_frame: 0,
                seq,
            });
        }
    }

    fn push_channel_state_to_rt(
        cmd_tx: &mut Option<Producer<MixerCommand>>,
        active_feed_ids: &[String],
        ch: &MixerChannelState,
        backends: &[Option<BackendSlot>; MAX_MIXER_INSERTS],
        media_buf: &Option<Arc<MediaBuffer>>,
        media_seq: u64,
    ) {
        let Some(tx) = cmd_tx.as_mut() else {
            return;
        };
        let ch_idx = ch.index;
        let feed_idx = ch
            .source
            .device_id
            .as_ref()
            .and_then(|id| active_feed_ids.iter().position(|f| f == id));
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: ch_idx,
            feed_idx,
            mode: ch.source.mode,
        });
        let _ = tx.push(MixerCommand::SetChannelDsp {
            ch: ch_idx,
            dsp: ch.dsp,
        });
        let _ = tx.push(MixerCommand::SetChannelMediaBuffer {
            ch: ch_idx,
            buffer: media_buf.clone(),
            playing: ch.media.playing && media_buf.is_some(),
            looping: ch.media.looping,
            seek_frame: ch.media.current_frame,
            seq: media_seq,
        });
        for (slot_idx, slot) in ch.inserts.iter().enumerate() {
            if let (Some(slot_be), Some(meta)) = (backends[slot_idx].as_ref(), slot.meta.as_ref()) {
                let handle = make_backend_active_handle(&slot_be.backend, meta);
                let _ = tx.push(MixerCommand::SetChannelInsert {
                    ch: ch_idx,
                    slot: slot_idx,
                    handle: Some(handle),
                    bypass: slot.bypass,
                    mix: slot.mix,
                });
            } else {
                let _ = tx.push(MixerCommand::SetChannelInsert {
                    ch: ch_idx,
                    slot: slot_idx,
                    handle: None,
                    bypass: false,
                    mix: 1.0,
                });
            }
        }
    }

    pub fn start_audio(
        &mut self,
        output_device: &str,
        preferred_sample_rate: u32,
        block_size: usize,
    ) -> Result<(), EngineError> {
        self.stop_audio_session_only();

        let mut feed_ids = Vec::new();
        for ch in &self.channels {
            if matches!(
                ch.source.mode,
                ChannelInputMode::Unassigned | ChannelInputMode::Multimedia
            ) {
                continue;
            }
            if let Some(ref id) = ch.source.device_id {
                if !feed_ids.contains(id) {
                    feed_ids.push(id.clone());
                }
            }
        }

        let (mut cmd_tx, cmd_rx) = RingBuffer::<MixerCommand>::new(2048);
        let dsp = MixerDspProcessor::new(
            Arc::clone(&self.telemetry),
            cmd_rx,
            self.channels.len(),
            preferred_sample_rate as f64,
        );

        let session = pedal_audio::start_mixer_audio_session(
            &MixerAudioConfig {
                input_devices: feed_ids.clone(),
                output_device: output_device.to_string(),
                preferred_sample_rate,
                block_size,
            },
            dsp,
        )?;

        self.sample_rate = session.info.output_sample_rate;
        self.block_size = session.info.block_size;
        self.active_output_device = Some(output_device.to_string());
        self.active_feed_ids = feed_ids;

        let _ = cmd_tx.push(MixerCommand::SetChannelCount(self.channels.len()));
        let _ = cmd_tx.push(MixerCommand::SetVirtualMicGainDb(
            self.master.virtual_mic_gain_db,
        ));
        self.cmd_tx = Some(cmd_tx);
        for (ch_idx, ch) in self.channels.iter().enumerate() {
            Self::push_channel_state_to_rt(
                &mut self.cmd_tx,
                &self.active_feed_ids,
                ch,
                &self.channel_backends[ch_idx],
                &self.channel_media_buffers[ch_idx],
                self.channel_media_seqs[ch_idx],
            );
        }
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetMaster {
                fader_db: self.master.fader_db,
                mute: self.master.mute,
                dim: self.master.dim,
            });
            for (slot_idx, slot) in self.master.inserts.iter().enumerate() {
                if let (Some(slot_be), Some(meta)) = (
                    self.master_backends[slot_idx].as_ref(),
                    slot.meta.as_ref(),
                ) {
                    let handle = make_backend_active_handle(&slot_be.backend, meta);
                    let _ = tx.push(MixerCommand::SetMasterInsert {
                        slot: slot_idx,
                        handle: Some(handle),
                        bypass: slot.bypass,
                        mix: slot.mix,
                    });
                }
            }
        }

        self.session = Some(session);
        if self.master.virtual_mic_enabled && self.virtual_mic.is_some() {
            if let Err(e) = self.attach_virtual_mic_pipe() {
                log::warn!("Virtual mic pipe reconnect warning: {e}");
            }
        }
        Ok(())
    }

    fn stop_audio_session_only(&mut self) {
        let _ = self.stop_recording();
        if let Some(ref mut vmic) = self.virtual_mic {
            if let Some(mut pipe) = vmic.pipe.take() {
                pipe.stop();
            }
        }
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetVirtualMicTx(None));
            for ch in 0..self.channels.len() {
                for slot in 0..MAX_MIXER_INSERTS {
                    let _ = tx.push(MixerCommand::SetChannelInsert {
                        ch,
                        slot,
                        handle: None,
                        bypass: false,
                        mix: 1.0,
                    });
                }
            }
            for slot in 0..MAX_MIXER_INSERTS {
                let _ = tx.push(MixerCommand::SetMasterInsert {
                    slot,
                    handle: None,
                    bypass: false,
                    mix: 1.0,
                });
            }
        }
        self.session = None;
        self.cmd_tx = None;
        self.active_feed_ids.clear();
    }

    pub fn stop_audio(&mut self) {
        self.disable_virtual_mic();
        self.stop_audio_session_only();
    }

    pub fn enable_virtual_mic(
        &mut self,
        fallback_output_device: Option<&str>,
    ) -> Result<String, EngineError> {
        if !self.is_audio_running() {
            self.ensure_audio_running(fallback_output_device)?;
        }
        if self.virtual_mic.is_none() {
            cleanup_stale_virtual_mic_modules();
            let sr = self.sample_rate();
            let sink_mod = load_pactl_module(&[
                "load-module",
                "module-null-sink",
                &format!("sink_name={VIRTUAL_MIC_SINK_NAME}"),
                &format!("rate={sr}"),
                "channels=2",
                "sink_properties=device.description=\"Chronomix_Virtual_Bus\"",
            ])?;
            let source_mod = match load_pactl_module(&[
                "load-module",
                "module-remap-source",
                &format!("master={VIRTUAL_MIC_SINK_NAME}.monitor"),
                &format!("source_name={VIRTUAL_MIC_SOURCE_NAME}"),
                "channels=2",
                "source_properties=device.description=\"Chronomix_Virtual_Mic_(Discord/OBS)\"",
            ]) {
                Ok(id) => Some(id),
                Err(_) => None,
            };
            self.virtual_mic = Some(ActiveVirtualMic {
                sink_module_id: Some(sink_mod),
                source_module_id: source_mod,
                pipe: None,
            });
        }
        self.master.virtual_mic_enabled = true;
        self.attach_virtual_mic_pipe()?;
        self.telemetry
            .virtual_mic_active
            .store(true, Ordering::Relaxed);
        Ok(VIRTUAL_MIC_DISPLAY_NAME.to_string())
    }

    fn attach_virtual_mic_pipe(&mut self) -> Result<(), EngineError> {
        let sr = self.sample_rate();
        let Some(ref mut vmic) = self.virtual_mic else {
            return Ok(());
        };
        if let Some(mut old_pipe) = vmic.pipe.take() {
            old_pipe.stop();
        }
        let child = spawn_virtual_mic_writer_process(sr)?;
        let (tx, mut rx) = RingBuffer::<f32>::new((sr as usize).max(48_000) * 4);
        if let Some(ref mut cmd_tx) = self.cmd_tx {
            let _ = cmd_tx.push(MixerCommand::SetVirtualMicGainDb(
                self.master.virtual_mic_gain_db,
            ));
            let _ = cmd_tx.push(MixerCommand::SetVirtualMicTx(Some(tx)));
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop_flag);
        let child_arc = Arc::new(Mutex::new(Some(child)));
        let child_thread = Arc::clone(&child_arc);

        let stdin_opt = child_arc
            .lock()
            .ok()
            .and_then(|mut g| g.as_mut().and_then(|c| c.stdin.take()));

        let join_handle = thread::spawn(move || {
            let Some(mut stdin) = stdin_opt else {
                return;
            };
            let mut byte_buf = Vec::with_capacity(4096);
            loop {
                if stop_thread.load(Ordering::Relaxed) {
                    break;
                }
                byte_buf.clear();
                while byte_buf.len() < 2048 {
                    match rx.pop() {
                        Ok(sample) => {
                            byte_buf.extend_from_slice(&sample.to_le_bytes());
                        }
                        Err(_) => break,
                    }
                }
                if !byte_buf.is_empty() {
                    if stdin.write_all(&byte_buf).is_err() {
                        break;
                    }
                } else {
                    thread::sleep(Duration::from_millis(2));
                }
            }
            drop(stdin);
            if let Ok(mut guard) = child_thread.lock() {
                if let Some(ref mut c) = *guard {
                    let _ = c.kill();
                    let _ = c.wait();
                }
                *guard = None;
            }
        });

        vmic.pipe = Some(ActiveVirtualMicPipe {
            stop_flag,
            join_handle: Some(join_handle),
            child: child_arc,
        });
        Ok(())
    }

    pub fn disable_virtual_mic(&mut self) {
        self.master.virtual_mic_enabled = false;
        self.telemetry
            .virtual_mic_active
            .store(false, Ordering::Relaxed);
        self.telemetry.virtual_mic_peak_l.store(0, Ordering::Relaxed);
        self.telemetry.virtual_mic_peak_r.store(0, Ordering::Relaxed);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetVirtualMicTx(None));
        }
        if let Some(mut vmic) = self.virtual_mic.take() {
            if let Some(mut pipe) = vmic.pipe.take() {
                pipe.stop();
            }
            if let Some(id) = vmic.source_module_id {
                unload_pactl_module(id);
            }
            if let Some(id) = vmic.sink_module_id {
                unload_pactl_module(id);
            }
        }
    }

    fn slot_mut(
        &mut self,
        target: MixerInsertTarget,
    ) -> Option<(&mut InsertSlotInfo, &mut Option<BackendSlot>)> {
        match target {
            MixerInsertTarget::Channel { ch, slot } => {
                let info = self.channels.get_mut(ch)?.inserts.get_mut(slot)?;
                let be = self.channel_backends.get_mut(ch)?.get_mut(slot)?;
                Some((info, be))
            }
            MixerInsertTarget::Master { slot } => {
                let info = self.master.inserts.get_mut(slot)?;
                let be = self.master_backends.get_mut(slot)?;
                Some((info, be))
            }
        }
    }

    fn slot_ref(&self, target: MixerInsertTarget) -> Option<(&InsertSlotInfo, &Option<BackendSlot>)> {
        match target {
            MixerInsertTarget::Channel { ch, slot } => {
                let info = self.channels.get(ch)?.inserts.get(slot)?;
                let be = self.channel_backends.get(ch)?.get(slot)?;
                Some((info, be))
            }
            MixerInsertTarget::Master { slot } => {
                let info = self.master.inserts.get(slot)?;
                let be = self.master_backends.get(slot)?;
                Some((info, be))
            }
        }
    }

    pub fn load_insert_plugin(
        &mut self,
        target: MixerInsertTarget,
        path: &Path,
    ) -> Result<LoadedPluginMeta, EngineError> {
        self.unload_insert_plugin(target);

        let sr = self.sample_rate() as f64;
        let block = self.block_size;
        let (backend, meta, handle) = load_plugin_backend(path, sr, block, &self.wine_config)?;

        let (bypass, mix) = if let Some((slot_info, slot_be)) = self.slot_mut(target) {
            slot_info.meta = Some(meta.clone());
            *slot_be = Some(BackendSlot {
                backend,
                window: None,
            });
            (slot_info.bypass, slot_info.mix)
        } else {
            unload_backend_resources(backend);
            return Err(EngineError::InvalidPluginPath("Invalid mixer insert slot".into()));
        };

        if let Some(ref mut tx) = self.cmd_tx {
            let cmd = match target {
                MixerInsertTarget::Channel { ch, slot } => MixerCommand::SetChannelInsert {
                    ch,
                    slot,
                    handle: Some(handle),
                    bypass,
                    mix,
                },
                MixerInsertTarget::Master { slot } => MixerCommand::SetMasterInsert {
                    slot,
                    handle: Some(handle),
                    bypass,
                    mix,
                },
            };
            let _ = tx.push(cmd);
        }

        Ok(meta)
    }

    pub fn unload_insert_plugin(&mut self, target: MixerInsertTarget) {
        self.close_insert_window(target);
        if let Some(ref mut tx) = self.cmd_tx {
            let cmd = match target {
                MixerInsertTarget::Channel { ch, slot } => MixerCommand::SetChannelInsert {
                    ch,
                    slot,
                    handle: None,
                    bypass: false,
                    mix: 1.0,
                },
                MixerInsertTarget::Master { slot } => MixerCommand::SetMasterInsert {
                    slot,
                    handle: None,
                    bypass: false,
                    mix: 1.0,
                },
            };
            let _ = tx.push(cmd);
        }
        if let Some((slot_info, slot_be)) = self.slot_mut(target) {
            slot_info.meta = None;
            slot_info.bypass = false;
            slot_info.mix = 1.0;
            if let Some(mut old) = slot_be.take() {
                close_backend_gui(Some(&old.backend), &mut old.window);
                unload_backend_resources(old.backend);
            }
        }
    }

    pub fn set_insert_bypass_and_mix(
        &mut self,
        target: MixerInsertTarget,
        bypass: bool,
        mix: f32,
    ) {
        let clamped_mix = mix.clamp(0.0, 1.0);
        if let Some((slot_info, _)) = self.slot_mut(target) {
            slot_info.bypass = bypass;
            slot_info.mix = clamped_mix;
        }
        if let Some(ref mut tx) = self.cmd_tx {
            let cmd = match target {
                MixerInsertTarget::Channel { ch, slot } => MixerCommand::SetChannelInsertBypassMix {
                    ch,
                    slot,
                    bypass,
                    mix: clamped_mix,
                },
                MixerInsertTarget::Master { slot } => MixerCommand::SetMasterInsertBypassMix {
                    slot,
                    bypass,
                    mix: clamped_mix,
                },
            };
            let _ = tx.push(cmd);
        }
    }

    pub fn set_insert_parameter(
        &mut self,
        target: MixerInsertTarget,
        id: u32,
        normalized_value: f64,
    ) -> Option<String> {
        let val = normalized_value.clamp(0.0, 1.0);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetInsertParam {
                target,
                id,
                value: val,
            });
        }
        let (slot_info, slot_be) = self.slot_mut(target)?;
        let be = slot_be.as_ref()?;
        let meta = slot_info.meta.as_mut()?;
        Some(set_backend_param_ui(&be.backend, meta, id, val))
    }

    pub fn refresh_insert_parameters(&mut self, target: MixerInsertTarget) {
        if let Some((slot_info, slot_be)) = self.slot_mut(target) {
            if let (Some(be), Some(meta)) = (slot_be.as_ref(), slot_info.meta.as_mut()) {
                refresh_backend_params_ui(&be.backend, meta);
            }
        }
    }

    pub fn open_insert_window(&mut self, target: MixerInsertTarget) -> Result<(), EngineError> {
        if let Some((_, Some(slot_be))) = self.slot_mut(target) {
            return open_backend_gui(&slot_be.backend, &mut slot_be.window);
        }
        Ok(())
    }

    pub fn close_insert_window(&mut self, target: MixerInsertTarget) {
        if let Some((_, Some(slot_be))) = self.slot_mut(target) {
            close_backend_gui(Some(&slot_be.backend), &mut slot_be.window);
        }
    }

    pub fn is_insert_window_open(&self, target: MixerInsertTarget) -> bool {
        self.slot_ref(target)
            .and_then(|(_, slot_be)| slot_be.as_ref())
            .is_some_and(|be| is_backend_gui_open(&be.backend, &be.window))
    }

    pub fn any_insert_window_open(&self) -> bool {
        for ch_slots in &self.channel_backends[..self.channels.len()] {
            for slot in ch_slots.iter().flatten() {
                if is_backend_gui_open(&slot.backend, &slot.window) {
                    return true;
                }
            }
        }
        for slot in self.master_backends.iter().flatten() {
            if is_backend_gui_open(&slot.backend, &slot.window) {
                return true;
            }
        }
        false
    }

    pub fn service_all_plugin_windows(&mut self) {
        for ch_slots in &mut self.channel_backends[..self.channels.len()] {
            for slot in ch_slots.iter_mut().flatten() {
                service_backend_gui(Some(&slot.backend), &mut slot.window);
            }
        }
        for slot in self.master_backends.iter_mut().flatten() {
            service_backend_gui(Some(&slot.backend), &mut slot.window);
        }
    }

    pub fn start_recording(&mut self, base_dir: PathBuf) -> Result<PathBuf, EngineError> {
        if self.recording.is_some() {
            return Ok(self
                .recording
                .as_ref()
                .expect("checked some")
                .report
                .directory
                .clone());
        }
        if self.cmd_tx.is_none() {
            return Err(EngineError::Recording(
                "Start the Mixer audio engine before recording".into(),
            ));
        }

        let armed_indices: Vec<usize> = self
            .channels
            .iter()
            .enumerate()
            .filter(|(_, ch)| ch.dsp.record_arm)
            .map(|(idx, _)| idx)
            .collect();
        if armed_indices.is_empty() && !self.master.record_mix {
            return Err(EngineError::Recording(
                "Arm at least one channel (REC) or enable Master Mix recording".into(),
            ));
        }

        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let session_dir = base_dir.join(format!("chronomix-mix-{stamp}"));
        fs::create_dir_all(&session_dir).map_err(|e| {
            EngineError::Recording(format!("Failed to create {}: {e}", session_dir.display()))
        })?;

        let sr = self.sample_rate();
        let spec = WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: WavSampleFormat::Float,
        };

        let mut ch_tx: [Option<Producer<f32>>; MAX_MIXER_CHANNELS] =
            std::array::from_fn(|_| None);
        let mut tracks: Vec<(Consumer<f32>, WavWriter<BufWriter<File>>)> = Vec::new();
        let mut files = Vec::new();

        for ch_idx in armed_indices {
            let safe = sanitize_stem_name(&self.channels[ch_idx].name);
            let file_path = session_dir.join(format!("ch{:02}_{safe}.wav", ch_idx + 1));
            let file = File::create(&file_path).map_err(|e| {
                EngineError::Recording(format!("Failed to create {}: {e}", file_path.display()))
            })?;
            let writer = WavWriter::new(BufWriter::new(file), spec)
                .map_err(|e| EngineError::Recording(format!("WAV header error: {e}")))?;
            let (tx, rx) = RingBuffer::<f32>::new(sr as usize * 4);
            ch_tx[ch_idx] = Some(tx);
            tracks.push((rx, writer));
            files.push(file_path);
        }

        let mut master_tx = None;
        if self.master.record_mix {
            let master_path = session_dir.join("master_mix.wav");
            let file = File::create(&master_path).map_err(|e| {
                EngineError::Recording(format!("Failed to create {}: {e}", master_path.display()))
            })?;
            let writer = WavWriter::new(BufWriter::new(file), spec)
                .map_err(|e| EngineError::Recording(format!("WAV header error: {e}")))?;
            let (tx, rx) = RingBuffer::<f32>::new(sr as usize * 4);
            master_tx = Some(tx);
            tracks.push((rx, writer));
            files.push(master_path);
        }

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop_flag);

        self.telemetry.recorded_frames.store(0, Ordering::Relaxed);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetRecorders(Some(Box::new(
                RecorderChannelTx { ch_tx, master_tx },
            ))));
        }
        self.telemetry
            .recording_active
            .store(true, Ordering::Relaxed);

        let join_handle = thread::spawn(move || -> Result<(), String> {
            loop {
                let stopping = stop_thread.load(Ordering::Relaxed);
                let mut wrote_any = false;
                for (rx, writer) in &mut tracks {
                    while let Ok(sample) = rx.pop() {
                        writer
                            .write_sample(sample)
                            .map_err(|e| format!("WAV write error: {e}"))?;
                        wrote_any = true;
                    }
                }
                if stopping && !wrote_any {
                    break;
                }
                if !wrote_any {
                    thread::sleep(Duration::from_millis(4));
                }
            }
            for (_, writer) in tracks {
                writer
                    .finalize()
                    .map_err(|e| format!("WAV finalize error: {e}"))?;
            }
            Ok(())
        });

        self.recording = Some(ActiveMixerRecording {
            stop_flag,
            join_handle: Some(join_handle),
            report: MixerRecordingReport {
                directory: session_dir.clone(),
                files,
                frames: 0,
            },
        });
        Ok(session_dir)
    }

    pub fn stop_recording(&mut self) -> Result<Option<MixerRecordingReport>, EngineError> {
        self.telemetry
            .recording_active
            .store(false, Ordering::Relaxed);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(MixerCommand::SetRecorders(None));
        }
        let Some(mut handle) = self.recording.take() else {
            return Ok(None);
        };
        handle.stop_flag.store(true, Ordering::Relaxed);
        if let Some(jh) = handle.join_handle.take() {
            match jh.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(EngineError::Recording(e)),
                Err(_) => return Err(EngineError::Recording("Mixer recorder thread panicked".into())),
            }
        }
        handle.report.frames = self.telemetry.recorded_frames.load(Ordering::Relaxed);
        Ok(Some(handle.report))
    }

    pub fn take_telemetry(&mut self) -> MixerMeterSnapshot {
        self.retired_media_buffers
            .retain(|arc| Arc::strong_count(arc) > 1);
        let mut channels = Vec::with_capacity(self.channels.len());
        for (idx, c) in self.telemetry.channels[..self.channels.len()]
            .iter()
            .enumerate()
        {
            let media_pos_frames = c.media_pos_frames.load(Ordering::Relaxed);
            let media_playing = c.media_playing.load(Ordering::Relaxed);
            if self.session.is_some()
                && c.media_seq.load(Ordering::Relaxed) == self.channel_media_seqs[idx]
                && self.channels[idx].source.mode == ChannelInputMode::Multimedia
            {
                let total = self.channels[idx].media.total_frames as u64;
                self.channels[idx].media.current_frame = media_pos_frames.min(total) as usize;
                self.channels[idx].media.playing = media_playing;
            }
            channels.push(ChannelMeterValues {
                in_left: f32::from_bits(c.in_peak_l.swap(0, Ordering::Relaxed)),
                in_right: f32::from_bits(c.in_peak_r.swap(0, Ordering::Relaxed)),
                out_left: f32::from_bits(c.out_peak_l.swap(0, Ordering::Relaxed)),
                out_right: f32::from_bits(c.out_peak_r.swap(0, Ordering::Relaxed)),
                comp_gr_db: f32::from_bits(c.comp_gr_db.swap(0, Ordering::Relaxed)),
                media_pos_frames,
                media_playing,
            });
        }

        MixerMeterSnapshot {
            channels,
            master_left: f32::from_bits(self.telemetry.master_peak_l.swap(0, Ordering::Relaxed)),
            master_right: f32::from_bits(self.telemetry.master_peak_r.swap(0, Ordering::Relaxed)),
            virtual_mic_left: f32::from_bits(
                self.telemetry.virtual_mic_peak_l.swap(0, Ordering::Relaxed),
            ),
            virtual_mic_right: f32::from_bits(
                self.telemetry.virtual_mic_peak_r.swap(0, Ordering::Relaxed),
            ),
            is_virtual_mic_active: self.telemetry.virtual_mic_active.load(Ordering::Relaxed),
            recorded_frames: self.telemetry.recorded_frames.load(Ordering::Relaxed),
            is_recording: self.telemetry.recording_active.load(Ordering::Relaxed),
        }
    }
}

fn load_pactl_module(args: &[&str]) -> Result<u32, EngineError> {
    let out = Command::new("pactl")
        .args(args)
        .output()
        .map_err(|e| EngineError::Recording(format!("Failed to execute pactl: {e}")))?;
    if !out.status.success() {
        let stderr = String::from_utf8_lossy(&out.stderr).trim().to_string();
        return Err(EngineError::Recording(format!("pactl error: {stderr}")));
    }
    let stdout = String::from_utf8_lossy(&out.stdout).trim().to_string();
    stdout
        .parse::<u32>()
        .map_err(|e| EngineError::Recording(format!("Invalid pactl module ID '{stdout}': {e}")))
}

fn unload_pactl_module(id: u32) {
    let _ = Command::new("pactl")
        .args(["unload-module", &id.to_string()])
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

fn cleanup_stale_virtual_mic_modules() {
    let Ok(out) = Command::new("pactl").args(["list", "short", "modules"]).output() else {
        return;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    let mut ids: Vec<u32> = Vec::new();
    for line in text.lines() {
        if line.contains(VIRTUAL_MIC_SINK_NAME) || line.contains(VIRTUAL_MIC_SOURCE_NAME) {
            if let Some(first) = line.split_whitespace().next() {
                if let Ok(id) = first.parse::<u32>() {
                    ids.push(id);
                }
            }
        }
    }
    ids.reverse();
    for id in ids {
        unload_pactl_module(id);
    }
}

fn spawn_virtual_mic_writer_process(sample_rate: u32) -> Result<Child, EngineError> {
    let sr_str = sample_rate.max(8000).to_string();
    if let Ok(child) = Command::new("pacat")
        .args([
            "--playback",
            &format!("--device={VIRTUAL_MIC_SINK_NAME}"),
            &format!("--rate={sr_str}"),
            "--channels=2",
            "--format=float32le",
            "--latency-msec=15",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        return Ok(child);
    }
    Command::new("pw-cat")
        .args([
            "--playback",
            &format!("--target={VIRTUAL_MIC_SINK_NAME}"),
            &format!("--rate={sr_str}"),
            "--channels=2",
            "--format=f32",
            "-",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            EngineError::Recording(format!(
                "Failed to start virtual mic audio stream (pacat/pw-cat): {e}"
            ))
        })
}

fn decode_audio_file(path: &Path, target_sr: u32) -> Result<MediaBuffer, EngineError> {
    if !path.is_file() {
        return Err(EngineError::InvalidPluginPath(format!(
            "Audio file not found: {}",
            path.display()
        )));
    }
    let title = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Media")
        .to_string();
    let is_wav = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"));
    if is_wav {
        if let Ok(buf) = decode_wav_with_hound(path, &title) {
            return Ok(buf);
        }
    }
    decode_with_ffmpeg(path, target_sr.max(8000), &title)
}

fn decode_wav_with_hound(path: &Path, title: &str) -> Result<MediaBuffer, EngineError> {
    let mut reader = WavReader::open(path)
        .map_err(|e| EngineError::Recording(format!("WAV decode error: {e}")))?;
    let spec = reader.spec();
    let ch = spec.channels.max(1) as usize;
    let raw: Vec<f32> = match spec.sample_format {
        WavSampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<f32>, _>>()
            .map_err(|e| EngineError::Recording(format!("WAV sample error: {e}")))?,
        WavSampleFormat::Int => {
            let shift = spec.bits_per_sample.saturating_sub(1).min(31);
            let scale = (1_i64 << shift) as f32;
            reader
                .samples::<i32>()
                .map(|r| r.map(|v| v as f32 / scale))
                .collect::<Result<Vec<f32>, _>>()
                .map_err(|e| EngineError::Recording(format!("WAV sample error: {e}")))?
        }
    };
    let total_frames = raw.len() / ch;
    let mut interleaved_stereo = Vec::with_capacity(total_frames * 2);
    for frame in raw.chunks_exact(ch) {
        let l = frame[0];
        let r = if ch >= 2 { frame[1] } else { frame[0] };
        interleaved_stereo.push(l);
        interleaved_stereo.push(r);
    }
    Ok(MediaBuffer {
        interleaved_stereo,
        total_frames,
        sample_rate: spec.sample_rate.max(1),
        title: title.to_string(),
    })
}

fn decode_with_ffmpeg(path: &Path, target_sr: u32, title: &str) -> Result<MediaBuffer, EngineError> {
    let output = Command::new("ffmpeg")
        .args(["-v", "error", "-i"])
        .arg(path)
        .args([
            "-vn",
            "-f",
            "f32le",
            "-acodec",
            "pcm_f32le",
            "-ac",
            "2",
            "-ar",
            &target_sr.to_string(),
            "pipe:1",
        ])
        .output()
        .map_err(|e| {
            if e.kind() == std::io::ErrorKind::NotFound {
                EngineError::Recording(
                    "ffmpeg is required to decode non-WAV audio files (MP3/FLAC/OGG/M4A)".into(),
                )
            } else {
                EngineError::Recording(format!("Failed to run ffmpeg: {e}"))
            }
        })?;
    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(EngineError::Recording(format!(
            "Failed to decode audio file: {}",
            err.trim()
        )));
    }
    let mut interleaved_stereo = Vec::with_capacity(output.stdout.len() / 4);
    for chunk in output.stdout.chunks_exact(4) {
        interleaved_stereo.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
    }
    if interleaved_stereo.len() < 2 {
        return Err(EngineError::Recording(
            "Audio file contains no decodable audio frames".into(),
        ));
    }
    let total_frames = interleaved_stereo.len() / 2;
    Ok(MediaBuffer {
        interleaved_stereo,
        total_frames,
        sample_rate: target_sr,
        title: title.to_string(),
    })
}

impl Drop for MixerEngine {
    fn drop(&mut self) {
        self.stop_audio();
        for ch in 0..self.channels.len() {
            for slot in 0..MAX_MIXER_INSERTS {
                self.unload_insert_plugin(MixerInsertTarget::Channel { ch, slot });
            }
        }
        for slot in 0..MAX_MIXER_INSERTS {
            self.unload_insert_plugin(MixerInsertTarget::Master { slot });
        }
    }
}

fn sanitize_stem_name(raw: &str) -> String {
    let clean: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = clean.trim_matches('_');
    if trimmed.is_empty() {
        "track".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mixer_sums_multiple_inputs_respecting_solo_and_mute() {
        let telemetry = Arc::new(SharedMixerTelemetry::default());
        let (mut tx, rx) = RingBuffer::<MixerCommand>::new(64);
        let mut proc = MixerDspProcessor::new(Arc::clone(&telemetry), rx, 2, 48_000.0);

        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 0,
            feed_idx: Some(0),
            mode: ChannelInputMode::Mono(0),
        });
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 1,
            feed_idx: Some(0),
            mode: ChannelInputMode::Mono(1),
        });

        let feed = [0.25_f32, 0.50, 0.25, 0.50];
        let mut out_l = [0.0_f32; 2];
        let mut out_r = [0.0_f32; 2];

        proc.process(&[&feed], &[2], &mut out_l, &mut out_r, 2, 48_000.0);
        assert!((out_l[0] - 0.75).abs() < 1e-4);
        assert!((out_r[0] - 0.75).abs() < 1e-4);

        let mut ch1_dsp = ChannelDspConfig::default();
        ch1_dsp.solo = true;
        let _ = tx.push(MixerCommand::SetChannelDsp { ch: 0, dsp: ch1_dsp });

        proc.process(&[&feed], &[2], &mut out_l, &mut out_r, 2, 48_000.0);
        assert!((out_l[0] - 0.25).abs() < 1e-4);
    }

    #[test]
    fn mixer_records_simultaneous_channel_and_master_rings() {
        let telemetry = Arc::new(SharedMixerTelemetry::default());
        let (mut tx, rx) = RingBuffer::<MixerCommand>::new(64);
        let mut proc = MixerDspProcessor::new(Arc::clone(&telemetry), rx, 2, 48_000.0);

        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 0,
            feed_idx: Some(0),
            mode: ChannelInputMode::Mono(0),
        });
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 1,
            feed_idx: Some(1),
            mode: ChannelInputMode::Mono(0),
        });

        let mut ch_tx: [Option<Producer<f32>>; MAX_MIXER_CHANNELS] =
            std::array::from_fn(|_| None);
        let (p0, mut c0) = RingBuffer::<f32>::new(64);
        let (p1, mut c1) = RingBuffer::<f32>::new(64);
        let (pm, mut cm) = RingBuffer::<f32>::new(64);
        ch_tx[0] = Some(p0);
        ch_tx[1] = Some(p1);
        let _ = tx.push(MixerCommand::SetRecorders(Some(Box::new(RecorderChannelTx {
            ch_tx,
            master_tx: Some(pm),
        }))));
        telemetry.recording_active.store(true, Ordering::Relaxed);

        let f0 = [0.2_f32; 4];
        let f1 = [0.3_f32; 4];
        let mut out_l = [0.0_f32; 4];
        let mut out_r = [0.0_f32; 4];
        proc.process(&[&f0, &f1], &[1, 1], &mut out_l, &mut out_r, 4, 48_000.0);

        assert_eq!(telemetry.recorded_frames.load(Ordering::Relaxed), 4);
        assert!((c0.pop().unwrap() - 0.2).abs() < 1e-4);
        assert!((c1.pop().unwrap() - 0.3).abs() < 1e-4);
        assert!((cm.pop().unwrap() - 0.5).abs() < 1e-4);
    }

    #[test]
    fn eq_curve_evaluates_boost_and_high_pass_cut() {
        let mut dsp = ChannelDspConfig::default();
        dsp.hpf_enabled = true;
        dsp.hpf_freq_hz = 120.0;
        dsp.eq_bands[2].gain_db = 6.0;
        let low = eval_channel_eq_db(&dsp, 30.0, 48_000.0);
        let mid = eval_channel_eq_db(&dsp, 2400.0, 48_000.0);
        assert!(low < -10.0);
        assert!(mid > 4.5);
    }

    #[test]
    fn multimedia_channel_plays_and_loops_background_music() {
        let telemetry = Arc::new(SharedMixerTelemetry::default());
        let (mut tx, rx) = RingBuffer::<MixerCommand>::new(64);
        let mut proc = MixerDspProcessor::new(Arc::clone(&telemetry), rx, 2, 48_000.0);

        let buf = Arc::new(MediaBuffer {
            interleaved_stereo: vec![0.4, -0.4, 0.2, -0.2],
            total_frames: 2,
            sample_rate: 48_000,
            title: "bg".into(),
        });
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 0,
            feed_idx: None,
            mode: ChannelInputMode::Multimedia,
        });
        let _ = tx.push(MixerCommand::SetChannelMediaBuffer {
            ch: 0,
            buffer: Some(buf),
            playing: true,
            looping: true,
            seek_frame: 0,
            seq: 1,
        });

        let mut out_l = [0.0_f32; 4];
        let mut out_r = [0.0_f32; 4];
        proc.process(&[], &[], &mut out_l, &mut out_r, 4, 48_000.0);

        assert!((out_l[0] - 0.4).abs() < 1e-4);
        assert!((out_r[0] + 0.4).abs() < 1e-4);
        assert!((out_l[2] - 0.4).abs() < 1e-4);
        assert!((out_r[3] + 0.2).abs() < 1e-4);
        assert!(telemetry.channels[0].media_playing.load(Ordering::Relaxed));
    }

    #[test]
    fn recording_excludes_unarmed_channels_by_default() {
        let engine = MixerEngine::new();
        assert!(!engine.master().record_mix);

        let telemetry = Arc::new(SharedMixerTelemetry::default());
        let (mut tx, rx) = RingBuffer::<MixerCommand>::new(64);
        let mut proc = MixerDspProcessor::new(Arc::clone(&telemetry), rx, 2, 48_000.0);
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 0,
            feed_idx: Some(0),
            mode: ChannelInputMode::Mono(0),
        });
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 1,
            feed_idx: Some(1),
            mode: ChannelInputMode::Mono(0),
        });

        let mut ch_tx: [Option<Producer<f32>>; MAX_MIXER_CHANNELS] =
            std::array::from_fn(|_| None);
        let (p0, mut c0) = RingBuffer::<f32>::new(64);
        ch_tx[0] = Some(p0);
        let _ = tx.push(MixerCommand::SetRecorders(Some(Box::new(RecorderChannelTx {
            ch_tx,
            master_tx: None,
        }))));
        telemetry.recording_active.store(true, Ordering::Relaxed);

        let f0 = [0.25_f32; 4];
        let f1 = [0.55_f32; 4];
        let mut out_l = [0.0_f32; 4];
        let mut out_r = [0.0_f32; 4];
        proc.process(&[&f0, &f1], &[1, 1], &mut out_l, &mut out_r, 4, 48_000.0);

        assert!((c0.pop().unwrap() - 0.25).abs() < 1e-4);
        assert!((out_l[0] - 0.80).abs() < 1e-4);
    }

    #[test]
    fn channels_expand_to_max_and_remove_shifts_cleanly() {
        let mut engine = MixerEngine::new();
        assert_eq!(engine.channels().len(), DEFAULT_MIXER_CHANNELS);
        assert_eq!(DEFAULT_MIXER_CHANNELS, 2);
        let media_idx = engine.add_multimedia_channel().unwrap();
        assert_eq!(
            engine.channel(media_idx).unwrap().source.mode,
            ChannelInputMode::Multimedia
        );
        assert_eq!(engine.channel(media_idx).unwrap().icon, ChannelIcon::Media);
        while engine.channels().len() < MAX_MIXER_CHANNELS {
            assert!(engine.add_channel().is_some());
        }
        assert!(engine.add_channel().is_none());
        assert!(engine.remove_channel(2));
        assert_eq!(engine.channels().len(), MAX_MIXER_CHANNELS - 1);
        for (i, ch) in engine.channels().iter().enumerate() {
            assert_eq!(ch.index, i);
        }
    }

    #[test]
    fn trim_balance_lpf_and_virtual_mic_routing_process_cleanly() {
        let telemetry = Arc::new(SharedMixerTelemetry::default());
        let (mut tx, rx) = RingBuffer::<MixerCommand>::new(64);
        let mut proc = MixerDspProcessor::new(Arc::clone(&telemetry), rx, 2, 48_000.0);

        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 0,
            feed_idx: Some(0),
            mode: ChannelInputMode::Stereo(0, 1),
        });
        let _ = tx.push(MixerCommand::SetChannelRouting {
            ch: 1,
            feed_idx: Some(1),
            mode: ChannelInputMode::Mono(0),
        });

        let mut ch0_dsp = ChannelDspConfig::default();
        ch0_dsp.balance = -1.0;
        ch0_dsp.solo = true;
        ch0_dsp.send_to_virtual_mic = true;
        let _ = tx.push(MixerCommand::SetChannelDsp { ch: 0, dsp: ch0_dsp });

        let mut ch1_dsp = ChannelDspConfig::default();
        ch1_dsp.solo_safe = true;
        ch1_dsp.send_to_virtual_mic = false;
        let _ = tx.push(MixerCommand::SetChannelDsp { ch: 1, dsp: ch1_dsp });

        let (vtx, mut vrx) = RingBuffer::<f32>::new(64);
        let _ = tx.push(MixerCommand::SetVirtualMicTx(Some(vtx)));

        let f0 = [0.4_f32, 0.4, 0.4, 0.4];
        let f1 = [0.2_f32, 0.2];
        let mut out_l = [0.0_f32; 2];
        let mut out_r = [0.0_f32; 2];
        proc.process(&[&f0, &f1], &[2, 1], &mut out_l, &mut out_r, 2, 48_000.0);

        assert!((out_l[0] - 0.6).abs() < 1e-4);
        assert!((out_r[0] - 0.2).abs() < 1e-4);
        let v_l = vrx.pop().unwrap();
        let v_r = vrx.pop().unwrap();
        assert!((v_l - 0.4).abs() < 1e-4);
        assert!(v_r.abs() < 1e-4);
    }

    #[test]
    fn multitrack_media_transport_synchronizes_all_playback() {
        let mut engine = MixerEngine::new();
        assert_eq!(engine.count_media_channels(), 0);
        let m1 = engine.add_multimedia_channel().unwrap();
        let m2 = engine.add_multimedia_channel().unwrap();
        assert_eq!(engine.count_media_channels(), 2);
        assert!(!engine.is_any_media_playing());

        let buf1 = MediaBuffer {
            interleaved_stereo: vec![0.1_f32; 48000 * 2],
            total_frames: 48000,
            sample_rate: 48000,
            title: "stem1".into(),
        };
        let buf2 = MediaBuffer {
            interleaved_stereo: vec![0.2_f32; 48000 * 2],
            total_frames: 48000,
            sample_rate: 48000,
            title: "stem2".into(),
        };
        let _ = engine.install_channel_media_buffer(m1, PathBuf::from("stem1.wav"), buf1, false, None);
        let _ = engine.install_channel_media_buffer(m2, PathBuf::from("stem2.wav"), buf2, false, None);

        assert_eq!(engine.channel(m1).unwrap().media.total_frames, 48000);
        assert_eq!(engine.channel(m2).unwrap().media.total_frames, 48000);

        let _ = engine.set_all_media_transport(true, Some(true), None, None);
        assert!(engine.channel(m1).unwrap().media.playing);
        assert!(engine.channel(m2).unwrap().media.playing);
        assert!(engine.is_any_media_playing());

        let _ = engine.seek_all_media_relative_seconds(10.0, None);
        assert_eq!(engine.channel(m1).unwrap().media.current_frame, 48000);
        assert_eq!(engine.channel(m2).unwrap().media.current_frame, 48000);

        let _ = engine.set_all_media_transport(false, None, Some(0), None);
        assert!(!engine.channel(m1).unwrap().media.playing);
        assert!(!engine.channel(m2).unwrap().media.playing);
        assert_eq!(engine.channel(m1).unwrap().media.current_frame, 0);
        assert_eq!(engine.channel(m2).unwrap().media.current_frame, 0);
        assert!(!engine.is_any_media_playing());
    }
}
