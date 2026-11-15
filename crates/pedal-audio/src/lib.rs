use std::{
    collections::{HashMap, HashSet},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

use cpal::{
    BufferSize, Device, HostId, Sample, SampleFormat, SizedSample, Stream, StreamConfig,
    SupportedBufferSize, SupportedStreamConfig,
    traits::{DeviceTrait, HostTrait, StreamTrait},
};
use rtrb::{Consumer, Producer, RingBuffer};
use thiserror::Error;

pub mod mixer;
pub use mixer::*;

/// Errors returned by audio device enumeration or stream initialization.
#[derive(Debug, Error)]
pub enum AudioError {
    #[error("Failed to enumerate audio devices: {0}")]
    Enumeration(String),
    #[error("Audio device not found: {0}")]
    DeviceNotFound(String),
    #[error("Failed to query device configuration: {0}")]
    Config(String),
    #[error("Unsupported sample format: {0:?}")]
    UnsupportedFormat(SampleFormat),
    #[error("Failed to build audio stream: {0}")]
    BuildStream(String),
    #[error("Failed to start audio stream: {0}")]
    PlayStream(String),
}

/// Basic metadata about an audio input or output device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AudioDeviceInfo {
    /// Stable backend-specific unique identifier (e.g. `pipewire:alsa_input...`).
    pub id: String,
    /// Clean human-readable label for display in UI dropdowns.
    pub name: String,
    pub default_channels: u16,
    pub default_sample_rate: u32,
}

/// Configuration requested when starting a full-duplex monitoring session.
#[derive(Debug, Clone)]
pub struct AudioSessionConfig {
    pub input_device: String,
    pub output_device: String,
    pub preferred_sample_rate: u32,
    pub block_size: usize,
}

/// Runtime telemetry counters updated lock-free from the audio callbacks.
#[derive(Debug, Default)]
pub struct AudioStats {
    pub input_overruns: AtomicU64,
    pub output_underruns: AtomicU64,
    pub input_callbacks: AtomicU64,
    pub output_callbacks: AtomicU64,
}

/// Negotiated stream parameters for an active audio session.
#[derive(Debug, Clone)]
pub struct ActiveAudioInfo {
    pub input_sample_rate: u32,
    pub output_sample_rate: u32,
    pub input_channels: u16,
    pub output_channels: u16,
    pub block_size: usize,
}

/// Callback interface executed on the real-time output audio thread.
///
/// Implementations must not allocate, block, or perform I/O inside [`process`](Self::process).
pub trait StereoAudioProcessor: Send + 'static {
    fn process(
        &mut self,
        in_left: &[f32],
        in_right: &[f32],
        out_left: &mut [f32],
        out_right: &mut [f32],
        frames: usize,
        sample_rate: f64,
    );
}

/// Active input and output audio streams. Dropping this stops both streams cleanly.
pub struct AudioSession {
    _input_stream: Stream,
    _output_stream: Stream,
    pub info: ActiveAudioInfo,
    pub stats: Arc<AudioStats>,
}

/// Enumerate available audio input devices using the default CPAL host.
pub fn list_input_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let devices = host
        .input_devices()
        .map_err(|e| AudioError::Enumeration(e.to_string()))?;
    Ok(collect_device_infos(devices, true))
}

/// Enumerate available audio output devices using the default CPAL host.
pub fn list_output_devices() -> Result<Vec<AudioDeviceInfo>, AudioError> {
    let host = cpal::default_host();
    let devices = host
        .output_devices()
        .map_err(|e| AudioError::Enumeration(e.to_string()))?;
    Ok(collect_device_infos(devices, false))
}

/// Return the default input device ID, if one exists.
pub fn default_input_device_id() -> Option<String> {
    cpal::default_host()
        .default_input_device()
        .map(|d| device_unique_id(&d))
}

/// Return the default output device ID, if one exists.
pub fn default_output_device_id() -> Option<String> {
    cpal::default_host()
        .default_output_device()
        .map(|d| device_unique_id(&d))
}

fn device_unique_id(dev: &Device) -> String {
    dev.id()
        .map(|id| id.to_string())
        .unwrap_or_else(|_| dev.to_string())
}

fn device_display_name(dev: &Device, id: &str, is_input: bool) -> String {
    let raw = dev
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_else(|_| dev.to_string());
    clean_device_label(&raw, id, is_input)
}

fn clean_device_label(raw: &str, id: &str, is_input: bool) -> String {
    let first_line = raw.lines().next().unwrap_or(raw).trim();
    match first_line {
        "default_input" | "input_default" => return "System Default (Input)".to_string(),
        "default_output" | "sink_default" | "default_sink" => {
            return "System Default (Output)".to_string()
        }
        "default" | "Default ALSA Output (currently PipeWire Media Server)" => {
            return if is_input {
                "System Default (Input)".to_string()
            } else {
                "System Default (Output)".to_string()
            };
        }
        _ => {}
    }

    if first_line.is_empty() {
        id.to_string()
    } else {
        first_line.to_string()
    }
}

#[allow(unreachable_patterns)]
fn should_keep_device(host_id: HostId, raw_id: &str, raw_name: &str, is_input: bool) -> bool {
    let dev_id = raw_id
        .split_once(':')
        .map(|(_, rest)| rest)
        .unwrap_or(raw_id);

    match host_id {
        #[cfg(target_os = "linux")]
        HostId::PipeWire => {
            // Skip virtual loopback sink monitor on inputs only; on outputs sink_default is the main desktop output.
            if is_input && (dev_id == "sink_default" || raw_name == "default_sink") {
                return false;
            }
            if is_input && raw_name.starts_with("Monitor of ") {
                return false;
            }
            true
        }
        #[cfg(target_os = "linux")]
        HostId::PulseAudio => {
            if is_input && (dev_id.ends_with(".monitor") || raw_name.starts_with("Monitor of ")) {
                return false;
            }
            true
        }
        #[cfg(target_os = "linux")]
        HostId::Alsa => should_keep_alsa_pcm(dev_id),
        _ => true,
    }
}

fn should_keep_alsa_pcm(pcm_id: &str) -> bool {
    if pcm_id == "default" {
        return true;
    }
    let prefix = pcm_id.split(':').next().unwrap_or(pcm_id);
    !matches!(
        prefix,
        "null"
            | "oss"
            | "pulse"
            | "pipewire"
            | "jack"
            | "sysdefault"
            | "front"
            | "rear"
            | "center_lfe"
            | "side"
            | "surround21"
            | "surround40"
            | "surround41"
            | "surround50"
            | "surround51"
            | "surround71"
            | "iec958"
            | "spdif"
            | "hdmi"
            | "dmix"
            | "dsnoop"
            | "plug"
            | "lavrate"
            | "samplerate"
            | "speex"
            | "speexrate"
            | "upmix"
            | "vdownmix"
            | "usbstream"
            | "hw"
    )
}

fn collect_device_infos(
    devices: impl Iterator<Item = Device>,
    is_input: bool,
) -> Vec<AudioDeviceInfo> {
    let mut seen_ids = HashSet::new();
    let mut entries = Vec::new();

    for dev in devices {
        let id = device_unique_id(&dev);
        if !seen_ids.insert(id.clone()) {
            continue;
        }
        let host_id = dev
            .id()
            .map(|d| d.host())
            .unwrap_or_else(|_| cpal::default_host().id());
        let raw_name = dev
            .description()
            .map(|d| d.name().to_string())
            .unwrap_or_else(|_| dev.to_string());

        if !should_keep_device(host_id, &id, &raw_name, is_input) {
            continue;
        }

        let name = device_display_name(&dev, &id, is_input);
        let cfg = if is_input {
            dev.default_input_config().ok()
        } else {
            dev.default_output_config().ok()
        };
        let (default_channels, default_sample_rate) = cfg
            .map(|c| (c.channels(), c.sample_rate()))
            .unwrap_or((2, 48_000));

        entries.push(AudioDeviceInfo {
            id,
            name,
            default_channels,
            default_sample_rate,
        });
    }

    disambiguate_duplicate_names(&mut entries);
    entries
}

fn disambiguate_duplicate_names(entries: &mut [AudioDeviceInfo]) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter() {
        *counts.entry(entry.name.clone()).or_insert(0) += 1;
    }

    let mut seen_idx: HashMap<String, usize> = HashMap::new();
    for entry in entries.iter_mut() {
        if counts.get(&entry.name).copied().unwrap_or(0) > 1 {
            let idx = seen_idx.entry(entry.name.clone()).or_insert(0);
            *idx += 1;
            let short_id = entry
                .id
                .split_once(':')
                .map(|(_, rest)| rest)
                .unwrap_or(&entry.id);
            entry.name = format!("{} [{} #{}]", entry.name, short_id, *idx);
        }
    }
}

pub(crate) fn find_device(id_or_name: &str, is_input: bool) -> Result<Device, AudioError> {
    let host = cpal::default_host();
    let devices: Vec<Device> = if is_input {
        host.input_devices()
    } else {
        host.output_devices()
    }
    .map_err(|e| AudioError::Enumeration(e.to_string()))?
    .collect();

    if let Some(dev) = devices.iter().find(|d| device_unique_id(d) == id_or_name) {
        return Ok(dev.clone());
    }

    devices
        .into_iter()
        .find(|d| d.to_string() == id_or_name)
        .ok_or_else(|| AudioError::DeviceNotFound(id_or_name.to_string()))
}

fn supported_configs(device: &Device, is_input: bool) -> Vec<cpal::SupportedStreamConfigRange> {
    if is_input {
        device
            .supported_input_configs()
            .map(|iter| iter.collect())
            .unwrap_or_default()
    } else {
        device
            .supported_output_configs()
            .map(|iter| iter.collect())
            .unwrap_or_default()
    }
}

pub(crate) fn resolve_buffer_size(
    device: &Device,
    is_input: bool,
    sample_rate: u32,
    channels: u16,
    block_size: usize,
) -> BufferSize {
    let host_id = device
        .id()
        .map(|d| d.host())
        .unwrap_or_else(|_| cpal::default_host().id());

    // In CPAL's ALSA backend, BufferSize::Fixed sets the total multi-period ring buffer size
    // rather than a single callback period; use 3x block_size so each period matches block_size
    // without constant XRUNs on pipewire-alsa / plughw.
    #[cfg(target_os = "linux")]
    let target_frames = if host_id == HostId::Alsa {
        (block_size as u32).saturating_mul(3).max(512)
    } else {
        block_size as u32
    };
    #[cfg(not(target_os = "linux"))]
    let target_frames = {
        let _ = host_id;
        block_size as u32
    };

    let ranges = supported_configs(device, is_input);
    let mut fallback = None;
    for range in ranges {
        if sample_rate < range.min_sample_rate() || sample_rate > range.max_sample_rate() {
            continue;
        }
        let resolved = match *range.buffer_size() {
            SupportedBufferSize::Range { min, max } => {
                BufferSize::Fixed(target_frames.clamp(min, max))
            }
            SupportedBufferSize::Unknown => BufferSize::Default,
        };
        if range.channels() == channels {
            return resolved;
        }
        fallback.get_or_insert(resolved);
    }

    fallback.unwrap_or(BufferSize::Default)
}

pub(crate) fn pick_sample_rate(
    device: &Device,
    is_input: bool,
    preferred: u32,
    default_cfg: &SupportedStreamConfig,
) -> u32 {
    for r in supported_configs(device, is_input) {
        if preferred >= r.min_sample_rate() && preferred <= r.max_sample_rate() {
            return preferred;
        }
    }
    default_cfg.sample_rate()
}

/// Start a real-time full-duplex audio session connecting `input_device` to `output_device`
/// through `processor`.
pub fn start_audio_session<P: StereoAudioProcessor>(
    config: &AudioSessionConfig,
    processor: P,
) -> Result<AudioSession, AudioError> {
    let in_device = find_device(&config.input_device, true)?;
    let out_device = find_device(&config.output_device, false)?;

    let in_default = in_device
        .default_input_config()
        .map_err(|e| AudioError::Config(format!("Input config: {e}")))?;
    let out_default = out_device
        .default_output_config()
        .map_err(|e| AudioError::Config(format!("Output config: {e}")))?;

    let out_sr = pick_sample_rate(&out_device, false, config.preferred_sample_rate, &out_default);
    let in_sr = pick_sample_rate(&in_device, true, out_sr, &in_default);

    let in_channels = in_default.channels().max(1);
    let out_channels = out_default.channels().max(1);

    let in_stream_cfg = StreamConfig {
        channels: in_channels,
        sample_rate: in_sr,
        buffer_size: resolve_buffer_size(&in_device, true, in_sr, in_channels, config.block_size),
    };
    let out_stream_cfg = StreamConfig {
        channels: out_channels,
        sample_rate: out_sr,
        buffer_size: resolve_buffer_size(
            &out_device,
            false,
            out_sr,
            out_channels,
            config.block_size,
        ),
    };

    // Bridge ring buffer stores interleaved stereo (L, R) frames at `out_sr`.
    // Capacity covers ~8 blocks (at least 8192 stereo samples = 4096 frames) to absorb clock jitter.
    let ring_frames = (config.block_size * 8).max(4096);
    let (producer, consumer) = RingBuffer::<f32>::new(ring_frames * 2);
    let stats = Arc::new(AudioStats::default());

    let resampler = LinearStereoResampler::new(in_sr, out_sr);
    let input_bridge = InputBridge {
        producer,
        in_channels: in_channels as usize,
        resampler,
        stats: Arc::clone(&stats),
    };

    let input_stream =
        build_input_stream(&in_device, &in_stream_cfg, in_default.sample_format(), input_bridge)?;

    let max_scratch_frames = config.block_size.max(4096);
    let output_bridge = OutputBridge {
        consumer,
        processor,
        out_channels: out_channels as usize,
        out_sample_rate: out_sr as f64,
        in_l: vec![0.0; max_scratch_frames],
        in_r: vec![0.0; max_scratch_frames],
        out_l: vec![0.0; max_scratch_frames],
        out_r: vec![0.0; max_scratch_frames],
        primed: false,
        stats: Arc::clone(&stats),
    };

    let output_stream = build_output_stream(
        &out_device,
        &out_stream_cfg,
        out_default.sample_format(),
        output_bridge,
    )?;

    input_stream
        .play()
        .map_err(|e| AudioError::PlayStream(format!("Input play: {e}")))?;
    output_stream
        .play()
        .map_err(|e| AudioError::PlayStream(format!("Output play: {e}")))?;

    Ok(AudioSession {
        _input_stream: input_stream,
        _output_stream: output_stream,
        info: ActiveAudioInfo {
            input_sample_rate: in_sr,
            output_sample_rate: out_sr,
            input_channels: in_channels,
            output_channels: out_channels,
            block_size: config.block_size,
        },
        stats,
    })
}

/// Real-time linear stereo resampler used only when input and output devices run at different
/// sample rates (e.g., 44.1 kHz USB mic + 48 kHz onboard audio).
struct LinearStereoResampler {
    ratio: f64,
    phase: f64,
    prev: [f32; 2],
    curr: [f32; 2],
    initialized: bool,
}

impl LinearStereoResampler {
    fn new(in_sr: u32, out_sr: u32) -> Self {
        Self {
            ratio: in_sr as f64 / out_sr.max(1) as f64,
            phase: 0.0,
            prev: [0.0, 0.0],
            curr: [0.0, 0.0],
            initialized: false,
        }
    }

    fn is_identity(&self) -> bool {
        (self.ratio - 1.0).abs() < 1e-7
    }

    fn push_frame(&mut self, l: f32, r: f32, producer: &mut Producer<f32>, overruns: &AtomicU64) {
        if self.is_identity() {
            if producer.slots() >= 2 {
                let _ = producer.push(l);
                let _ = producer.push(r);
            } else {
                overruns.fetch_add(1, Ordering::Relaxed);
            }
            return;
        }

        if !self.initialized {
            self.prev = [l, r];
            self.curr = [l, r];
            self.initialized = true;
            return;
        }

        self.prev = self.curr;
        self.curr = [l, r];

        while self.phase < 1.0 {
            let t = self.phase as f32;
            let out_l = self.prev[0] + (self.curr[0] - self.prev[0]) * t;
            let out_r = self.prev[1] + (self.curr[1] - self.prev[1]) * t;
            if producer.slots() >= 2 {
                let _ = producer.push(out_l);
                let _ = producer.push(out_r);
            } else {
                overruns.fetch_add(1, Ordering::Relaxed);
            }
            self.phase += self.ratio;
        }
        self.phase -= 1.0;
    }
}

struct InputBridge {
    producer: Producer<f32>,
    in_channels: usize,
    resampler: LinearStereoResampler,
    stats: Arc<AudioStats>,
}

impl InputBridge {
    fn push_samples<T: Sample>(&mut self, data: &[T])
    where
        f32: cpal::FromSample<T>,
    {
        self.stats.input_callbacks.fetch_add(1, Ordering::Relaxed);
        let ch = self.in_channels.max(1);
        for frame in data.chunks_exact(ch) {
            let l: f32 = frame[0].to_sample();
            let r: f32 = if ch >= 2 { frame[1].to_sample() } else { l };
            self.resampler
                .push_frame(l, r, &mut self.producer, &self.stats.input_overruns);
        }
    }
}

struct OutputBridge<P: StereoAudioProcessor> {
    consumer: Consumer<f32>,
    processor: P,
    out_channels: usize,
    out_sample_rate: f64,
    in_l: Vec<f32>,
    in_r: Vec<f32>,
    out_l: Vec<f32>,
    out_r: Vec<f32>,
    primed: bool,
    stats: Arc<AudioStats>,
}

impl<P: StereoAudioProcessor> OutputBridge<P> {
    fn render_samples<T: Sample + cpal::FromSample<f32>>(&mut self, output: &mut [T]) {
        self.stats.output_callbacks.fetch_add(1, Ordering::Relaxed);
        let ch = self.out_channels.max(1);
        let total_frames = output.len() / ch;
        if total_frames == 0 {
            return;
        }

        // Process in chunks bounded by our pre-allocated scratch buffers (never allocates on RT thread).
        let max_chunk = self.in_l.len();
        let mut frame_offset = 0;

        while frame_offset < total_frames {
            let chunk_frames = (total_frames - frame_offset).min(max_chunk);
            let mut available_frames = self.consumer.slots() / 2;

            // Cap excessive buffer accumulation (clock drift or startup burst) to keep monitoring latency low.
            let max_buffered = (chunk_frames * 5).max(1536);
            let target_keep = chunk_frames + (chunk_frames / 2).min(512);
            if available_frames > max_buffered {
                let drop_frames = available_frames.saturating_sub(target_keep);
                for _ in 0..(drop_frames * 2) {
                    let _ = self.consumer.pop();
                }
                available_frames = self.consumer.slots() / 2;
            }

            let prime_threshold = if chunk_frames >= 32 {
                target_keep
            } else {
                chunk_frames
            };

            if !self.primed && available_frames >= prime_threshold {
                self.primed = true;
            }

            if self.primed && available_frames >= chunk_frames {
                for i in 0..chunk_frames {
                    self.in_l[i] = self.consumer.pop().unwrap_or(0.0);
                    self.in_r[i] = self.consumer.pop().unwrap_or(0.0);
                }
            } else {
                if self.primed {
                    // Actual underrun: re-prime on the next callback without draining partial frames!
                    self.primed = false;
                    self.stats.output_underruns.fetch_add(1, Ordering::Relaxed);
                }
                self.in_l[..chunk_frames].fill(0.0);
                self.in_r[..chunk_frames].fill(0.0);
            }

            self.out_l[..chunk_frames].fill(0.0);
            self.out_r[..chunk_frames].fill(0.0);

            self.processor.process(
                &self.in_l[..chunk_frames],
                &self.in_r[..chunk_frames],
                &mut self.out_l[..chunk_frames],
                &mut self.out_r[..chunk_frames],
                chunk_frames,
                self.out_sample_rate,
            );

            let out_slice = &mut output[frame_offset * ch..(frame_offset + chunk_frames) * ch];
            for (i, frame) in out_slice.chunks_exact_mut(ch).enumerate() {
                let l = self.out_l[i];
                let r = self.out_r[i];
                if ch == 1 {
                    frame[0] = T::from_sample((l + r) * 0.5);
                } else {
                    frame[0] = T::from_sample(l);
                    frame[1] = T::from_sample(r);
                    for extra in frame.iter_mut().skip(2) {
                        *extra = T::from_sample(0.0);
                    }
                }
            }

            frame_offset += chunk_frames;
        }
    }
}

fn build_typed_input<T: SizedSample>(
    device: &Device,
    config: &StreamConfig,
    bridge: InputBridge,
) -> Result<Stream, AudioError>
where
    f32: cpal::FromSample<T>,
{
    let slot = Arc::new(Mutex::new(Some(bridge)));
    let make_cb = |slot: Arc<Mutex<Option<InputBridge>>>| {
        let mut local: Option<InputBridge> = None;
        move |data: &[T], _: &cpal::InputCallbackInfo| {
            let b = local.get_or_insert_with(|| {
                slot.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("input bridge already taken")
            });
            b.push_samples(data);
        }
    };

    match device.build_input_stream(
        *config,
        make_cb(Arc::clone(&slot)),
        |err| log::error!("Input audio stream error: {err}"),
        None,
    ) {
        Ok(stream) => Ok(stream),
        Err(first_err) if !matches!(config.buffer_size, BufferSize::Default) => {
            log::warn!(
                "Input stream rejected buffer_size {:?} ({first_err}); retrying with BufferSize::Default",
                config.buffer_size
            );
            let mut fallback = *config;
            fallback.buffer_size = BufferSize::Default;
            device
                .build_input_stream(
                    fallback,
                    make_cb(slot),
                    |err| log::error!("Input audio stream error: {err}"),
                    None,
                )
                .map_err(|e| AudioError::BuildStream(format!("Input stream: {e}")))
        }
        Err(e) => Err(AudioError::BuildStream(format!("Input stream: {e}"))),
    }
}

fn build_input_stream(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    bridge: InputBridge,
) -> Result<Stream, AudioError> {
    match format {
        SampleFormat::F32 => build_typed_input::<f32>(device, config, bridge),
        SampleFormat::I16 => build_typed_input::<i16>(device, config, bridge),
        SampleFormat::I32 => build_typed_input::<i32>(device, config, bridge),
        SampleFormat::U16 => build_typed_input::<u16>(device, config, bridge),
        other => Err(AudioError::UnsupportedFormat(other)),
    }
}

fn build_typed_output<T: SizedSample + cpal::FromSample<f32>, P: StereoAudioProcessor>(
    device: &Device,
    config: &StreamConfig,
    bridge: OutputBridge<P>,
) -> Result<Stream, AudioError> {
    let slot = Arc::new(Mutex::new(Some(bridge)));
    let make_cb = |slot: Arc<Mutex<Option<OutputBridge<P>>>>| {
        let mut local: Option<OutputBridge<P>> = None;
        move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
            let b = local.get_or_insert_with(|| {
                slot.lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .take()
                    .expect("output bridge already taken")
            });
            b.render_samples(data);
        }
    };

    match device.build_output_stream(
        *config,
        make_cb(Arc::clone(&slot)),
        |err| log::error!("Output audio stream error: {err}"),
        None,
    ) {
        Ok(stream) => Ok(stream),
        Err(first_err) if !matches!(config.buffer_size, BufferSize::Default) => {
            log::warn!(
                "Output stream rejected buffer_size {:?} ({first_err}); retrying with BufferSize::Default",
                config.buffer_size
            );
            let mut fallback = *config;
            fallback.buffer_size = BufferSize::Default;
            device
                .build_output_stream(
                    fallback,
                    make_cb(slot),
                    |err| log::error!("Output audio stream error: {err}"),
                    None,
                )
                .map_err(|e| AudioError::BuildStream(format!("Output stream: {e}")))
        }
        Err(e) => Err(AudioError::BuildStream(format!("Output stream: {e}"))),
    }
}

fn build_output_stream<P: StereoAudioProcessor>(
    device: &Device,
    config: &StreamConfig,
    format: SampleFormat,
    bridge: OutputBridge<P>,
) -> Result<Stream, AudioError> {
    match format {
        SampleFormat::F32 => build_typed_output::<f32, P>(device, config, bridge),
        SampleFormat::I16 => build_typed_output::<i16, P>(device, config, bridge),
        SampleFormat::I32 => build_typed_output::<i32, P>(device, config, bridge),
        SampleFormat::U16 => build_typed_output::<u16, P>(device, config, bridge),
        other => Err(AudioError::UnsupportedFormat(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct GainProcessor(f32);

    impl StereoAudioProcessor for GainProcessor {
        fn process(
            &mut self,
            in_left: &[f32],
            in_right: &[f32],
            out_left: &mut [f32],
            out_right: &mut [f32],
            frames: usize,
            _sample_rate: f64,
        ) {
            for i in 0..frames {
                out_left[i] = in_left[i] * self.0;
                out_right[i] = in_right[i] * self.0;
            }
        }
    }

    #[test]
    fn mono_input_upmixes_to_stereo_and_processes() {
        let (producer, consumer) = RingBuffer::<f32>::new(256);
        let stats = Arc::new(AudioStats::default());
        let mut input = InputBridge {
            producer,
            in_channels: 1,
            resampler: LinearStereoResampler::new(48_000, 48_000),
            stats: Arc::clone(&stats),
        };
        let mut output = OutputBridge {
            consumer,
            processor: GainProcessor(0.5),
            out_channels: 2,
            out_sample_rate: 48_000.0,
            in_l: vec![0.0; 64],
            in_r: vec![0.0; 64],
            out_l: vec![0.0; 64],
            out_r: vec![0.0; 64],
            primed: false,
            stats,
        };

        input.push_samples(&[0.4_f32, -0.8_f32]);
        let mut out_interleaved = [0.0_f32; 4];
        output.render_samples(&mut out_interleaved);
        assert_eq!(out_interleaved, [0.2, 0.2, -0.4, -0.4]);
    }

    #[test]
    fn resampler_handles_rate_mismatch() {
        let (mut producer, mut consumer) = RingBuffer::<f32>::new(2048);
        let overruns = AtomicU64::new(0);
        let mut resampler = LinearStereoResampler::new(44_100, 48_000);

        for i in 0..441 {
            let v = i as f32 / 441.0;
            resampler.push_frame(v, -v, &mut producer, &overruns);
        }

        let produced_frames = consumer.slots() / 2;
        assert!((475..=485).contains(&produced_frames));
        assert_eq!(overruns.load(Ordering::Relaxed), 0);
        let first_l = consumer.pop().unwrap();
        let first_r = consumer.pop().unwrap();
        assert!((first_l + first_r).abs() < 1e-6);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn alsa_and_pipewire_filters_hide_virtual_duplicates() {
        assert!(should_keep_device(HostId::Alsa, "alsa:default", "default", true));
        assert!(should_keep_device(
            HostId::Alsa,
            "alsa:plughw:CARD=Mic,DEV=0",
            "Samson C01U Pro Mic",
            true
        ));
        assert!(!should_keep_device(
            HostId::Alsa,
            "alsa:hw:CARD=Mic,DEV=0",
            "Samson C01U Pro Mic",
            true
        ));
        assert!(!should_keep_device(
            HostId::Alsa,
            "alsa:surround51:CARD=Generic,DEV=0",
            "5.1 Surround",
            false
        ));
        assert!(!should_keep_device(
            HostId::Alsa,
            "alsa:dmix:CARD=Generic,DEV=0",
            "Direct mixing",
            false
        ));
        assert!(!should_keep_device(
            HostId::PipeWire,
            "pipewire:sink_default",
            "default_sink",
            true
        ));
        assert!(should_keep_device(
            HostId::PipeWire,
            "pipewire:sink_default",
            "default_sink",
            false
        ));
        assert!(should_keep_device(
            HostId::PipeWire,
            "pipewire:alsa_input.usb-Samson",
            "Samson C01U Pro Mic",
            true
        ));
        assert_eq!(
            clean_device_label("default_input", "pipewire:input_default", true),
            "System Default (Input)"
        );
        assert_eq!(
            clean_device_label("sink_default", "pipewire:sink_default", false),
            "System Default (Output)"
        );
    }
}
