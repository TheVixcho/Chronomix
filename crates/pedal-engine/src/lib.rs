pub mod mixer;
pub use mixer::*;
pub mod vst2;

use std::{
    fs::File,
    io::BufWriter,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use pedal_audio::{
    ActiveAudioInfo, AudioDeviceInfo, AudioError, AudioSession, AudioSessionConfig, AudioStats,
    StereoAudioProcessor,
};
use pedal_wine::{
    Vst3BinaryKind, WineAudioChannel, WineError, WineParamInfo, WineVst3Plugin,
};
pub use pedal_wine::{
    WineAction, WineBridgeMode, WineConfig, WineEnvironmentStatus, WinePrefixInfo, WinePrefixKind,
    WineRunnerInfo, default_chronomix_wine_prefix, detect_display_env, discover_wine_binaries,
    discover_wine_prefixes, initialize_wine_prefix, inspect_wine_environment,
    launch_wine_executable, launch_wine_tool, rebuild_wine_bridge, resolve_wine_binary,
    resolve_wine_prefix, standard_prefix_vst_dirs,
};
use rtrb::{Consumer, Producer, RingBuffer};
use thiserror::Error;
use vst2::NativeVst2Plugin;
use vst3_host::{
    AudioBuffers, Parameter, Plugin, PluginInfo, PluginWindow, Vst3Host,
    discovery::{DEFAULT_PROBE_TIMEOUT, discover_plugins_safe},
};

/// Errors produced by the Chronomix DSP, VST2/VST3, or recording engine.
#[derive(Debug, Error)]
pub enum EngineError {
    #[error("Audio error: {0}")]
    Audio(#[from] AudioError),
    #[error("VST3 host error: {0}")]
    Vst3(#[from] vst3_host::Error),
    #[error("VST2 host error: {0}")]
    Vst2(String),
    #[error("Wine VST bridge error: {0}")]
    Wine(#[from] WineError),
    #[error("Invalid plugin path: {0}")]
    InvalidPluginPath(String),
    #[error("Recording error: {0}")]
    Recording(String),
}

/// Snapshot of a loaded VST3 plugin's metadata and editable parameters.
#[derive(Debug, Clone)]
pub struct LoadedPluginMeta {
    pub info: PluginInfo,
    pub input_channels: usize,
    pub output_channels: usize,
    pub parameters: Vec<ParameterDisplay>,
    pub is_wine_bridged: bool,
}

/// Parameter metadata paired with its plugin-formatted display string (e.g. `"8.2 kHz"`).
#[derive(Debug, Clone)]
pub struct ParameterDisplay {
    pub param: Parameter,
    pub formatted: String,
}

/// Peak meter readings in linear amplitude (`0.0..=1.0+`).
#[derive(Debug, Clone, Copy, Default)]
pub struct MeterSnapshot {
    pub in_left: f32,
    pub in_right: f32,
    pub out_left: f32,
    pub out_right: f32,
}

/// Lock-free atomic state shared between the UI thread and the real-time audio callback.
pub struct SharedEngineState {
    input_gain: AtomicU32,
    output_gain: AtomicU32,
    mix: AtomicU32,
    bypass: AtomicBool,
    monitor_muted: AtomicBool,
    recording_active: AtomicBool,
    in_peak_l: AtomicU32,
    in_peak_r: AtomicU32,
    out_peak_l: AtomicU32,
    out_peak_r: AtomicU32,
    recorded_frames: AtomicU64,
}

impl Default for SharedEngineState {
    fn default() -> Self {
        Self {
            input_gain: AtomicU32::new(1.0_f32.to_bits()),
            output_gain: AtomicU32::new(1.0_f32.to_bits()),
            mix: AtomicU32::new(1.0_f32.to_bits()),
            bypass: AtomicBool::new(false),
            monitor_muted: AtomicBool::new(false),
            recording_active: AtomicBool::new(false),
            in_peak_l: AtomicU32::new(0.0_f32.to_bits()),
            in_peak_r: AtomicU32::new(0.0_f32.to_bits()),
            out_peak_l: AtomicU32::new(0.0_f32.to_bits()),
            out_peak_r: AtomicU32::new(0.0_f32.to_bits()),
            recorded_frames: AtomicU64::new(0),
        }
    }
}

impl SharedEngineState {
    pub fn set_input_gain_db(&self, db: f32) {
        self.input_gain
            .store(db_to_linear(db).to_bits(), Ordering::Relaxed);
    }

    pub fn set_output_gain_db(&self, db: f32) {
        self.output_gain
            .store(db_to_linear(db).to_bits(), Ordering::Relaxed);
    }

    pub fn set_mix(&self, mix: f32) {
        self.mix
            .store(mix.clamp(0.0, 1.0).to_bits(), Ordering::Relaxed);
    }

    pub fn set_bypass(&self, bypass: bool) {
        self.bypass.store(bypass, Ordering::Relaxed);
    }

    pub fn set_monitor_muted(&self, muted: bool) {
        self.monitor_muted.store(muted, Ordering::Relaxed);
    }

    pub fn take_meters(&self) -> MeterSnapshot {
        MeterSnapshot {
            in_left: f32::from_bits(self.in_peak_l.swap(0, Ordering::Relaxed)),
            in_right: f32::from_bits(self.in_peak_r.swap(0, Ordering::Relaxed)),
            out_left: f32::from_bits(self.out_peak_l.swap(0, Ordering::Relaxed)),
            out_right: f32::from_bits(self.out_peak_r.swap(0, Ordering::Relaxed)),
        }
    }

    pub fn recorded_frames(&self) -> u64 {
        self.recorded_frames.load(Ordering::Relaxed)
    }

    pub fn is_recording(&self) -> bool {
        self.recording_active.load(Ordering::Relaxed)
    }
}

pub(crate) fn db_to_linear(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        10.0_f32.powf(db / 20.0)
    }
}

pub(crate) fn store_peak(target: &AtomicU32, peak: f32) {
    let clamped = if peak.is_finite() {
        peak.max(0.0)
    } else {
        0.0
    };
    target.fetch_max(clamped.to_bits(), Ordering::Relaxed);
}

pub(crate) fn slice_peak(buf: &[f32]) -> f32 {
    buf.iter()
        .fold(0.0_f32, |m, &s| if s.is_finite() { m.max(s.abs()) } else { m })
}

#[derive(Clone)]
pub(crate) enum ActivePluginHandle {
    Native {
        plugin: Arc<Mutex<Plugin>>,
        in_channels: usize,
        out_channels: usize,
    },
    NativeVst2 {
        plugin: Arc<Mutex<NativeVst2Plugin>>,
    },
    Wine {
        channel: Arc<Mutex<WineAudioChannel>>,
    },
}

enum EngineCommand {
    SetPlugin(Option<ActivePluginHandle>),
    SetParam { id: u32, value: f64 },
    SetRecorder(Option<Producer<f32>>),
}

struct PedalDspProcessor {
    shared: Arc<SharedEngineState>,
    cmd_rx: Consumer<EngineCommand>,
    active_plugin: Option<ActivePluginHandle>,
    vst_buffers: AudioBuffers,
    wine_wet_l: Vec<f32>,
    wine_wet_r: Vec<f32>,
    rec_tx: Option<Producer<f32>>,
}

impl PedalDspProcessor {
    fn new(shared: Arc<SharedEngineState>, cmd_rx: Consumer<EngineCommand>, sample_rate: f64) -> Self {
        Self {
            shared,
            cmd_rx,
            active_plugin: None,
            vst_buffers: AudioBuffers::new(2, 2, 4096, sample_rate),
            wine_wet_l: vec![0.0; 4096],
            wine_wet_r: vec![0.0; 4096],
            rec_tx: None,
        }
    }

    fn drain_commands(&mut self) {
        let budget = self.cmd_rx.buffer().capacity();
        for _ in 0..budget {
            let Ok(cmd) = self.cmd_rx.pop() else {
                break;
            };
            match cmd {
                EngineCommand::SetPlugin(handle) => {
                    self.active_plugin = handle;
                    if let Some(ActivePluginHandle::Native {
                        in_channels,
                        out_channels,
                        ..
                    }) = self.active_plugin
                    {
                        let in_ch = in_channels;
                        let out_ch = out_channels.max(1);
                        if self.vst_buffers.inputs.len() != in_ch
                            || self.vst_buffers.outputs.len() != out_ch
                        {
                            self.vst_buffers =
                                AudioBuffers::new(in_ch, out_ch, 4096, self.vst_buffers.sample_rate);
                        }
                    }
                }
                EngineCommand::SetParam { id, value } => {
                    if let Some(ref handle) = self.active_plugin {
                        set_backend_param_live(handle, id, value);
                    }
                }
                EngineCommand::SetRecorder(tx) => {
                    self.rec_tx = tx;
                }
            }
        }
    }

    fn prepare_vst_buffers(
        vst_buffers: &mut AudioBuffers,
        frames: usize,
        sample_rate: f64,
        in_l: &[f32],
        in_r: &[f32],
    ) {
        vst_buffers.sample_rate = sample_rate;
        vst_buffers.block_size = frames;

        for ch in &mut vst_buffers.inputs {
            if ch.len() != frames {
                ch.resize(frames, 0.0);
            }
        }
        for ch in &mut vst_buffers.outputs {
            if ch.len() != frames {
                ch.resize(frames, 0.0);
            }
            ch.fill(0.0);
        }

        match vst_buffers.inputs.len() {
            0 => {}
            1 => {
                for i in 0..frames {
                    vst_buffers.inputs[0][i] = (in_l[i] + in_r[i]) * 0.5;
                }
            }
            _ => {
                vst_buffers.inputs[0][..frames].copy_from_slice(&in_l[..frames]);
                vst_buffers.inputs[1][..frames].copy_from_slice(&in_r[..frames]);
                for ch in vst_buffers.inputs.iter_mut().skip(2) {
                    ch[..frames].fill(0.0);
                }
            }
        }
    }
}

impl StereoAudioProcessor for PedalDspProcessor {
    fn process(
        &mut self,
        in_left: &[f32],
        in_right: &[f32],
        out_left: &mut [f32],
        out_right: &mut [f32],
        frames: usize,
        sample_rate: f64,
    ) {
        self.drain_commands();

        let in_gain = f32::from_bits(self.shared.input_gain.load(Ordering::Relaxed));
        let out_gain = f32::from_bits(self.shared.output_gain.load(Ordering::Relaxed));
        let mix = f32::from_bits(self.shared.mix.load(Ordering::Relaxed));
        let bypass = self.shared.bypass.load(Ordering::Relaxed);
        let muted = self.shared.monitor_muted.load(Ordering::Relaxed);

        // Apply input gain into out_left/out_right temporarily as our dry stereo buffer.
        for i in 0..frames {
            out_left[i] = in_left[i] * in_gain;
            out_right[i] = in_right[i] * in_gain;
        }

        store_peak(&self.shared.in_peak_l, slice_peak(&out_left[..frames]));
        store_peak(&self.shared.in_peak_r, slice_peak(&out_right[..frames]));

        if !bypass {
            if let Some(ref handle) = self.active_plugin {
                process_active_handle(
                    handle,
                    &mut self.vst_buffers,
                    &mut self.wine_wet_l,
                    &mut self.wine_wet_r,
                    &mut out_left[..frames],
                    &mut out_right[..frames],
                    frames,
                    sample_rate,
                    mix,
                );
            }
        }

        // Apply post-gain before metering and WAV capture.
        for i in 0..frames {
            out_left[i] *= out_gain;
            out_right[i] *= out_gain;
        }

        if self.shared.recording_active.load(Ordering::Relaxed) {
            if let Some(ref mut tx) = self.rec_tx {
                let mut pushed_frames = 0_u64;
                for i in 0..frames {
                    if tx.slots() >= 2 {
                        let _ = tx.push(out_left[i]);
                        let _ = tx.push(out_right[i]);
                        pushed_frames += 1;
                    } else {
                        break;
                    }
                }
                self.shared
                    .recorded_frames
                    .fetch_add(pushed_frames, Ordering::Relaxed);
            }
        }

        if muted {
            out_left[..frames].fill(0.0);
            out_right[..frames].fill(0.0);
        }

        store_peak(&self.shared.out_peak_l, slice_peak(&out_left[..frames]));
        store_peak(&self.shared.out_peak_r, slice_peak(&out_right[..frames]));
    }
}

struct RecordingHandle {
    stop_flag: Arc<AtomicBool>,
    join_handle: Option<JoinHandle<Result<(), String>>>,
    path: PathBuf,
}

pub(crate) enum LoadedBackend {
    Native(Arc<Mutex<Plugin>>),
    NativeVst2(Arc<Mutex<NativeVst2Plugin>>),
    #[cfg(target_os = "linux")]
    Wine(WineVst3Plugin),
}

/// High-level coordinator owning the active audio session, loaded VST3 plugin, native editor
/// window, and WAV recording thread.
pub struct PedalEngine {
    pub shared: Arc<SharedEngineState>,
    session: Option<AudioSession>,
    cmd_tx: Option<Producer<EngineCommand>>,
    loaded_backend: Option<LoadedBackend>,
    loaded_meta: Option<LoadedPluginMeta>,
    plugin_window: Option<PluginWindow>,
    recording: Option<RecordingHandle>,
    sample_rate: u32,
    block_size: usize,
    wine_config: WineConfig,
}

impl Default for PedalEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl PedalEngine {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(SharedEngineState::default()),
            session: None,
            cmd_tx: None,
            loaded_backend: None,
            loaded_meta: None,
            plugin_window: None,
            recording: None,
            sample_rate: 48_000,
            block_size: 256,
            wine_config: WineConfig::default(),
        }
    }

    /// Updates the Wine runner/prefix configuration used when loading Windows VST2/VST3 plugins.
    pub fn set_wine_config(&mut self, config: WineConfig) {
        self.wine_config = config;
    }

    /// Returns the active Wine runner/prefix configuration.
    pub fn wine_config(&self) -> &WineConfig {
        &self.wine_config
    }

    pub fn list_input_devices(&self) -> Result<Vec<AudioDeviceInfo>, EngineError> {
        Ok(pedal_audio::list_input_devices()?)
    }

    pub fn list_output_devices(&self) -> Result<Vec<AudioDeviceInfo>, EngineError> {
        Ok(pedal_audio::list_output_devices()?)
    }

    pub fn default_input_device(&self) -> Option<String> {
        pedal_audio::default_input_device_id()
    }

    pub fn default_output_device(&self) -> Option<String> {
        pedal_audio::default_output_device_id()
    }

    pub fn is_audio_running(&self) -> bool {
        self.session.is_some()
    }

    pub fn active_audio_info(&self) -> Option<&ActiveAudioInfo> {
        self.session.as_ref().map(|s| &s.info)
    }

    pub fn audio_stats(&self) -> Option<&Arc<AudioStats>> {
        self.session.as_ref().map(|s| &s.stats)
    }

    pub fn sample_rate(&self) -> u32 {
        self.session
            .as_ref()
            .map(|s| s.info.output_sample_rate)
            .unwrap_or(self.sample_rate)
    }

    pub fn start_audio(
        &mut self,
        input_device: &str,
        output_device: &str,
        preferred_sample_rate: u32,
        block_size: usize,
    ) -> Result<(), EngineError> {
        self.stop_audio();

        let (mut cmd_tx, cmd_rx) = RingBuffer::<EngineCommand>::new(512);
        let dsp = PedalDspProcessor::new(
            Arc::clone(&self.shared),
            cmd_rx,
            preferred_sample_rate as f64,
        );

        let session = pedal_audio::start_audio_session(
            &AudioSessionConfig {
                input_device: input_device.to_string(),
                output_device: output_device.to_string(),
                preferred_sample_rate,
                block_size,
            },
            dsp,
        )?;

        self.sample_rate = session.info.output_sample_rate;
        self.block_size = session.info.block_size;

        // Re-attach currently loaded plugin if one was loaded before starting audio.
        if let Some(handle) = self.make_active_handle() {
            let _ = cmd_tx.push(EngineCommand::SetPlugin(Some(handle)));
        }

        self.cmd_tx = Some(cmd_tx);
        self.session = Some(session);
        Ok(())
    }

    fn make_active_handle(&self) -> Option<ActivePluginHandle> {
        let meta = self.loaded_meta.as_ref()?;
        match self.loaded_backend.as_ref()? {
            LoadedBackend::Native(plugin) => Some(ActivePluginHandle::Native {
                plugin: Arc::clone(plugin),
                in_channels: meta.input_channels,
                out_channels: meta.output_channels,
            }),
            LoadedBackend::NativeVst2(plugin) => Some(ActivePluginHandle::NativeVst2 {
                plugin: Arc::clone(plugin),
            }),
            #[cfg(target_os = "linux")]
            LoadedBackend::Wine(wine) => Some(ActivePluginHandle::Wine {
                channel: wine.audio_channel(),
            }),
        }
    }

    pub fn stop_audio(&mut self) {
        let _ = self.stop_recording();
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(EngineCommand::SetPlugin(None));
        }
        self.session = None;
        self.cmd_tx = None;
    }

    /// Returns the default program VST folder (`<workspace_or_exe_dir>/vst_plugins`),
    /// creating it if it does not exist yet.
    pub fn default_program_vst_dir() -> PathBuf {
        let candidates = [
            std::env::current_dir().ok().map(|d| d.join("vst_plugins")),
            std::env::current_exe()
                .ok()
                .and_then(|p| p.parent().map(|d| d.join("vst_plugins"))),
            std::env::var("HOME")
                .ok()
                .map(|h| PathBuf::from(h).join(".local/share/chronomix/vst_plugins")),
        ];
        for cand in candidates.into_iter().flatten() {
            if std::fs::create_dir_all(&cand).is_ok() {
                return cand.canonicalize().unwrap_or(cand);
            }
        }
        PathBuf::from("vst_plugins")
    }

    /// Scan a specific VST folder (program folder or custom user folder) for VST2 & VST3 plugins.
    pub fn discover_vst_in_directory(&self, dir: &Path) -> Vec<PluginInfo> {
        let _ = std::fs::create_dir_all(dir);
        discover_vst_targets_in_dir(dir)
    }

    /// Scan the configured VST folder and optionally standard OS/Wine system VST directories.
    pub fn discover_plugins(&self, vst_dir: &Path, include_system: bool) -> Vec<PluginInfo> {
        let mut plugins = self.discover_vst_in_directory(vst_dir);
        if include_system {
            for sys_plugin in self.discover_vst3_plugins() {
                if !plugins.iter().any(|p| p.path == sys_plugin.path) {
                    plugins.push(sys_plugin);
                }
            }
        }
        plugins
    }

    /// Scan standard OS VST3 directories safely (isolated subprocess probes prevent bad plugins
    /// from crashing the host during scan), plus Wine Windows VST2/VST3 folders on Linux.
    pub fn discover_vst3_plugins(&self) -> Vec<PluginInfo> {
        let mut plugins = self.discover_vst_in_directory(&Self::default_program_vst_dir());

        if let Ok(host) = Vst3Host::new() {
            let report = discover_plugins_safe(&host.scan_plugin_paths(), DEFAULT_PROBE_TIMEOUT);
            for d in report.plugins {
                if !plugins.iter().any(|p| p.path == d.info.path) {
                    plugins.push(d.info);
                }
            }
        }

        #[cfg(target_os = "linux")]
        {
            for wine_info in discover_wine_vst3_plugins(&self.wine_config) {
                if !plugins.iter().any(|p| p.path == wine_info.path) {
                    plugins.push(wine_info);
                }
            }
        }

        plugins
    }

    pub fn load_vst3_plugin(&mut self, path: impl AsRef<Path>) -> Result<LoadedPluginMeta, EngineError> {
        let requested_path = path.as_ref();
        self.unload_vst3_plugin();

        let sr = self.sample_rate() as f64;
        let block = self.block_size;
        let (backend, meta, handle) =
            load_plugin_backend(requested_path, sr, block, &self.wine_config)?;

        self.loaded_backend = Some(backend);
        self.loaded_meta = Some(meta.clone());
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(EngineCommand::SetPlugin(Some(handle)));
        }

        Ok(meta)
    }

    pub fn unload_vst3_plugin(&mut self) {
        self.close_plugin_window();
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(EngineCommand::SetPlugin(None));
        }
        if let Some(backend) = self.loaded_backend.take() {
            unload_backend_resources(backend);
        }
        self.loaded_meta = None;
    }

    pub fn loaded_plugin(&self) -> Option<&LoadedPluginMeta> {
        self.loaded_meta.as_ref()
    }

    /// Update a normalized parameter (`0.0..=1.0`) and return its refreshed display string.
    pub fn set_plugin_parameter(&mut self, id: u32, normalized_value: f64) -> Option<String> {
        let val = normalized_value.clamp(0.0, 1.0);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(EngineCommand::SetParam { id, value: val });
        }
        let backend = self.loaded_backend.as_ref()?;
        let meta = self.loaded_meta.as_mut()?;
        Some(set_backend_param_ui(backend, meta, id, val))
    }

    /// Refresh parameter values & display strings from the plugin (useful after native GUI edits).
    pub fn refresh_plugin_parameters(&mut self) {
        if let (Some(backend), Some(meta)) = (&self.loaded_backend, &mut self.loaded_meta) {
            refresh_backend_params_ui(backend, meta);
        }
    }

    pub fn open_plugin_window(&mut self) -> Result<(), EngineError> {
        let Some(ref backend) = self.loaded_backend else {
            return Ok(());
        };
        open_backend_gui(backend, &mut self.plugin_window)
    }

    pub fn close_plugin_window(&mut self) {
        close_backend_gui(self.loaded_backend.as_ref(), &mut self.plugin_window);
    }

    pub fn is_plugin_window_open(&self) -> bool {
        self.loaded_backend
            .as_ref()
            .is_some_and(|b| is_backend_gui_open(b, &self.plugin_window))
    }

    /// Pump native editor window events; call once per UI frame when a plugin GUI is open.
    pub fn service_plugin_window(&mut self) {
        service_backend_gui(self.loaded_backend.as_ref(), &mut self.plugin_window);
    }

    pub fn start_recording(&mut self, path: PathBuf) -> Result<(), EngineError> {
        if self.recording.is_some() {
            return Ok(());
        }
        if self.cmd_tx.is_none() {
            return Err(EngineError::Recording(
                "Start the audio engine before recording".into(),
            ));
        }

        let sr = self.sample_rate();
        let spec = WavSpec {
            channels: 2,
            sample_rate: sr,
            bits_per_sample: 32,
            sample_format: WavSampleFormat::Float,
        };
        let file = File::create(&path)
            .map_err(|e| EngineError::Recording(format!("Failed to create {}: {e}", path.display())))?;
        let mut writer = WavWriter::new(BufWriter::new(file), spec)
            .map_err(|e| EngineError::Recording(format!("WAV header error: {e}")))?;

        let (rec_tx, mut rec_rx) = RingBuffer::<f32>::new(sr as usize * 4);
        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop_thread = Arc::clone(&stop_flag);

        self.shared.recorded_frames.store(0, Ordering::Relaxed);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(EngineCommand::SetRecorder(Some(rec_tx)));
        }
        self.shared.recording_active.store(true, Ordering::Relaxed);

        let join_handle = thread::spawn(move || -> Result<(), String> {
            while !stop_thread.load(Ordering::Relaxed) || rec_rx.slots() > 0 {
                let mut wrote_any = false;
                while let Ok(sample) = rec_rx.pop() {
                    writer
                        .write_sample(sample)
                        .map_err(|e| format!("WAV write error: {e}"))?;
                    wrote_any = true;
                }
                if !wrote_any {
                    thread::sleep(Duration::from_millis(5));
                }
            }
            writer
                .finalize()
                .map_err(|e| format!("WAV finalize error: {e}"))?;
            Ok(())
        });

        self.recording = Some(RecordingHandle {
            stop_flag,
            join_handle: Some(join_handle),
            path,
        });
        Ok(())
    }

    pub fn stop_recording(&mut self) -> Result<Option<PathBuf>, EngineError> {
        self.shared.recording_active.store(false, Ordering::Relaxed);
        if let Some(ref mut tx) = self.cmd_tx {
            let _ = tx.push(EngineCommand::SetRecorder(None));
        }
        let Some(mut handle) = self.recording.take() else {
            return Ok(None);
        };
        handle.stop_flag.store(true, Ordering::Relaxed);
        if let Some(jh) = handle.join_handle.take() {
            match jh.join() {
                Ok(Ok(())) => {}
                Ok(Err(e)) => return Err(EngineError::Recording(e)),
                Err(_) => return Err(EngineError::Recording("Recorder thread panicked".into())),
            }
        }
        Ok(Some(handle.path))
    }

    pub fn active_recording_path(&self) -> Option<&Path> {
        self.recording.as_ref().map(|r| r.path.as_path())
    }
}

impl Drop for PedalEngine {
    fn drop(&mut self) {
        self.stop_audio();
        self.unload_vst3_plugin();
    }
}

pub(crate) fn set_backend_param_live(handle: &ActivePluginHandle, id: u32, value: f64) {
    match handle {
        ActivePluginHandle::Native { plugin, .. } => {
            if let Ok(mut p) = plugin.try_lock() {
                let _ = p.set_parameter_at(id, value, 0);
            }
        }
        ActivePluginHandle::NativeVst2 { plugin } => {
            if let Ok(mut p) = plugin.try_lock() {
                let _ = p.set_parameter(id, value);
            }
        }
        ActivePluginHandle::Wine { channel } => {
            if let Ok(mut ch) = channel.try_lock() {
                ch.queue_param(id, value);
            }
        }
    }
}

pub(crate) fn ensure_scratch_len(wet_l: &mut Vec<f32>, wet_r: &mut Vec<f32>, frames: usize) {
    if wet_l.len() < frames {
        wet_l.resize(frames, 0.0);
        wet_r.resize(frames, 0.0);
    }
}

pub(crate) fn process_active_handle(
    handle: &ActivePluginHandle,
    vst_buffers: &mut AudioBuffers,
    wet_l: &mut Vec<f32>,
    wet_r: &mut Vec<f32>,
    io_l: &mut [f32],
    io_r: &mut [f32],
    frames: usize,
    sample_rate: f64,
    mix: f32,
) {
    match handle {
        ActivePluginHandle::Native {
            plugin,
            in_channels,
            out_channels,
        } => {
            let in_ch = *in_channels;
            let out_ch = (*out_channels).max(1);
            if vst_buffers.inputs.len() != in_ch || vst_buffers.outputs.len() != out_ch {
                *vst_buffers = AudioBuffers::new(in_ch, out_ch, 4096, sample_rate);
            }
            PedalDspProcessor::prepare_vst_buffers(vst_buffers, frames, sample_rate, io_l, io_r);
            if let Ok(mut p) = plugin.try_lock() {
                if p.process_audio(vst_buffers).is_ok() {
                    let dry = 1.0 - mix;
                    let out_chs = vst_buffers.outputs.len();
                    for i in 0..frames {
                        let wl = if out_chs >= 1 {
                            vst_buffers.outputs[0][i]
                        } else {
                            io_l[i]
                        };
                        let wr = if out_chs >= 2 {
                            vst_buffers.outputs[1][i]
                        } else {
                            wl
                        };
                        io_l[i] = io_l[i] * dry + wl * mix;
                        io_r[i] = io_r[i] * dry + wr * mix;
                    }
                }
            }
        }
        ActivePluginHandle::NativeVst2 { plugin } => {
            ensure_scratch_len(wet_l, wet_r, frames);
            if let Ok(mut p) = plugin.try_lock() {
                if p.process_stereo(io_l, io_r, &mut wet_l[..frames], &mut wet_r[..frames], frames, sample_rate) {
                    let dry = 1.0 - mix;
                    for i in 0..frames {
                        io_l[i] = io_l[i] * dry + wet_l[i] * mix;
                        io_r[i] = io_r[i] * dry + wet_r[i] * mix;
                    }
                }
            }
        }
        ActivePluginHandle::Wine { channel } => {
            ensure_scratch_len(wet_l, wet_r, frames);
            if let Ok(mut ch) = channel.try_lock() {
                if ch.process_stereo(io_l, io_r, &mut wet_l[..frames], &mut wet_r[..frames], frames, sample_rate) {
                    let dry = 1.0 - mix;
                    for i in 0..frames {
                        io_l[i] = io_l[i] * dry + wet_l[i] * mix;
                        io_r[i] = io_r[i] * dry + wet_r[i] * mix;
                    }
                }
            }
        }
    }
}

pub(crate) fn load_plugin_backend(
    requested_path: &Path,
    sr: f64,
    block: usize,
    wine_config: &WineConfig,
) -> Result<(LoadedBackend, LoadedPluginMeta, ActivePluginHandle), EngineError> {
    if !requested_path.exists() {
        return Err(EngineError::InvalidPluginPath(format!(
            "Path does not exist: {}",
            requested_path.display()
        )));
    }

    if let Some(target) = pedal_wine::resolve_vst3_target(requested_path) {
        #[cfg(target_os = "linux")]
        match target.kind {
            Vst3BinaryKind::WindowsPe64 => {
                return load_wine_backend(&target.bundle_or_file, &target.binary_path, sr, block, wine_config);
            }
            Vst3BinaryKind::WindowsPe32 => {
                return Err(EngineError::Wine(WineError::UnsupportedArch32(
                    target.bundle_or_file.display().to_string(),
                )));
            }
            Vst3BinaryKind::NativeElf | Vst3BinaryKind::Unknown => {}
        }

        let is_standalone_vst2 = is_vst2_extension(&target.bundle_or_file);
        if is_standalone_vst2 {
            if let Ok(res) = load_native_vst2_backend(&target.binary_path, sr, block) {
                return Ok(res);
            }
        }
        return match load_native_vst3_backend(&target.bundle_or_file, sr, block) {
            Ok(res) => Ok(res),
            Err(vst3_err) => {
                if let Ok(res) = load_native_vst2_backend(&target.binary_path, sr, block) {
                    Ok(res)
                } else {
                    Err(vst3_err)
                }
            }
        };
    }

    if requested_path.is_dir() {
        return Err(EngineError::InvalidPluginPath(format!(
            "No VST2 (.dll/.so) or VST3 (.vst3) plugin found inside folder '{}'",
            requested_path.display()
        )));
    }

    if is_vst2_extension(requested_path) {
        if let Ok(res) = load_native_vst2_backend(requested_path, sr, block) {
            return Ok(res);
        }
    }

    load_native_vst3_backend(requested_path, sr, block)
}

fn load_native_vst2_backend(
    path: &Path,
    sr: f64,
    block: usize,
) -> Result<(LoadedBackend, LoadedPluginMeta, ActivePluginHandle), EngineError> {
    let vst2 = crate::vst2::NativeVst2Plugin::load(path, sr, block).map_err(EngineError::Vst2)?;
    let info = vst2.info().clone();
    let in_channels = vst2.input_channels();
    let out_channels = vst2.output_channels().max(1);
    let parameters = vst2.get_parameters();

    let meta = LoadedPluginMeta {
        info,
        input_channels: in_channels,
        output_channels: out_channels,
        parameters,
        is_wine_bridged: false,
    };
    let plugin_arc = Arc::new(Mutex::new(vst2));
    let handle = ActivePluginHandle::NativeVst2 {
        plugin: Arc::clone(&plugin_arc),
    };
    Ok((LoadedBackend::NativeVst2(plugin_arc), meta, handle))
}

fn load_native_vst3_backend(
    path: &Path,
    sr: f64,
    block: usize,
) -> Result<(LoadedBackend, LoadedPluginMeta, ActivePluginHandle), EngineError> {
    let mut host = Vst3Host::builder()
        .sample_rate(sr)
        .block_size(block)
        .input_channels(2)
        .output_channels(2)
        .build()?;

    let mut plugin = host.load_plugin(path)?;
    plugin.start_processing()?;

    let layout = plugin.audio_bus_layout().ok();
    let in_channels = layout
        .as_ref()
        .and_then(|l| l.inputs.first())
        .map(|b| b.channel_count)
        .unwrap_or(2);
    let out_channels = layout
        .as_ref()
        .and_then(|l| l.outputs.first())
        .map(|b| b.channel_count)
        .unwrap_or(2)
        .max(1);

    let info = plugin.info().clone();
    let parameters = collect_parameters(&plugin);

    let meta = LoadedPluginMeta {
        info,
        input_channels: in_channels,
        output_channels: out_channels,
        parameters,
        is_wine_bridged: false,
    };
    let plugin_arc = Arc::new(Mutex::new(plugin));
    let handle = ActivePluginHandle::Native {
        plugin: Arc::clone(&plugin_arc),
        in_channels,
        out_channels,
    };
    Ok((LoadedBackend::Native(plugin_arc), meta, handle))
}

#[cfg(target_os = "linux")]
fn load_wine_backend(
    requested_path: &Path,
    dll_path: &Path,
    sr: f64,
    block: usize,
    wine_config: &WineConfig,
) -> Result<(LoadedBackend, LoadedPluginMeta, ActivePluginHandle), EngineError> {
    let wine_plugin = WineVst3Plugin::load_with_config(dll_path, sr, block, wine_config)?;
    let wmeta = wine_plugin.metadata();
    let fallback_name = requested_path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Wine VST")
        .to_string();

    let info = PluginInfo {
        path: requested_path.to_path_buf(),
        name: if wmeta.name.is_empty() {
            fallback_name
        } else {
            wmeta.name.clone()
        },
        vendor: if wmeta.vendor.is_empty() {
            "Windows VST (Wine)".to_string()
        } else {
            wmeta.vendor.clone()
        },
        version: wmeta.version.clone(),
        category: wmeta.category.clone(),
        uid: format!("wine:{}", requested_path.display()),
        audio_inputs: if wmeta.in_channels > 0 { 1 } else { 0 },
        audio_outputs: if wmeta.out_channels > 0 { 1 } else { 0 },
        has_midi_input: false,
        has_midi_output: false,
        has_gui: wmeta.has_gui,
    };

    let meta = LoadedPluginMeta {
        info,
        input_channels: wmeta.in_channels,
        output_channels: wmeta.out_channels.max(1),
        parameters: wine_params_to_display(&wmeta.params),
        is_wine_bridged: true,
    };
    let handle = ActivePluginHandle::Wine {
        channel: wine_plugin.audio_channel(),
    };
    Ok((LoadedBackend::Wine(wine_plugin), meta, handle))
}

pub(crate) fn make_backend_active_handle(
    backend: &LoadedBackend,
    meta: &LoadedPluginMeta,
) -> ActivePluginHandle {
    match backend {
        LoadedBackend::Native(plugin) => ActivePluginHandle::Native {
            plugin: Arc::clone(plugin),
            in_channels: meta.input_channels,
            out_channels: meta.output_channels,
        },
        LoadedBackend::NativeVst2(plugin) => ActivePluginHandle::NativeVst2 {
            plugin: Arc::clone(plugin),
        },
        #[cfg(target_os = "linux")]
        LoadedBackend::Wine(wine) => ActivePluginHandle::Wine {
            channel: wine.audio_channel(),
        },
    }
}

pub(crate) fn unload_backend_resources(backend: LoadedBackend) {
    match backend {
        LoadedBackend::Native(plugin_arc) => {
            if let Ok(mut p) = plugin_arc.lock() {
                let _ = p.stop_processing();
            }
        }
        LoadedBackend::NativeVst2(plugin_arc) => {
            if let Ok(mut p) = plugin_arc.lock() {
                p.close_editor_window();
            }
        }
        #[cfg(target_os = "linux")]
        LoadedBackend::Wine(_) => {}
    }
}

pub(crate) fn set_backend_param_ui(
    backend: &LoadedBackend,
    meta: &mut LoadedPluginMeta,
    id: u32,
    val: f64,
) -> String {
    let formatted = match backend {
        LoadedBackend::Native(plugin_arc) => {
            if let Ok(mut plugin) = plugin_arc.try_lock() {
                let _ = plugin.set_parameter(id, val);
                plugin
                    .format_parameter(id, val)
                    .unwrap_or_else(|_| format!("{:.2}", val))
            } else {
                format!("{:.2}", val)
            }
        }
        LoadedBackend::NativeVst2(plugin_arc) => {
            if let Ok(mut plugin) = plugin_arc.try_lock() {
                plugin.set_parameter(id, val)
            } else {
                format!("{:.2}", val)
            }
        }
        #[cfg(target_os = "linux")]
        LoadedBackend::Wine(wine) => wine
            .set_parameter(id, val)
            .unwrap_or_else(|_| format!("{:.2}", val)),
    };
    if let Some(entry) = meta.parameters.iter_mut().find(|p| p.param.id == id) {
        entry.param.value = val;
        entry.formatted = formatted.clone();
    }
    formatted
}

pub(crate) fn refresh_backend_params_ui(backend: &LoadedBackend, meta: &mut LoadedPluginMeta) {
    let params = match backend {
        LoadedBackend::Native(plugin_arc) => {
            let Ok(plugin) = plugin_arc.try_lock() else {
                return;
            };
            collect_parameters(&plugin)
        }
        LoadedBackend::NativeVst2(plugin_arc) => {
            let Ok(plugin) = plugin_arc.try_lock() else {
                return;
            };
            plugin.get_parameters()
        }
        #[cfg(target_os = "linux")]
        LoadedBackend::Wine(wine) => match wine.get_parameters() {
            Ok(p) => wine_params_to_display(&p),
            Err(_) => return,
        },
    };
    meta.parameters = params;
}

pub(crate) fn open_backend_gui(
    backend: &LoadedBackend,
    plugin_window: &mut Option<PluginWindow>,
) -> Result<(), EngineError> {
    match backend {
        LoadedBackend::Native(plugin_arc) => {
            if let Some(win) = plugin_window {
                if win.is_open() {
                    return Ok(());
                }
            }
            let mut window = PluginWindow::new(Arc::clone(plugin_arc));
            window.open()?;
            *plugin_window = Some(window);
            Ok(())
        }
        LoadedBackend::NativeVst2(plugin_arc) => {
            let mut plugin = plugin_arc
                .lock()
                .map_err(|_| EngineError::Vst2("VST2 mutex poisoned".into()))?;
            plugin.open_editor_window().map_err(EngineError::Vst2)
        }
        #[cfg(target_os = "linux")]
        LoadedBackend::Wine(wine) => {
            wine.open_gui()?;
            Ok(())
        }
    }
}

pub(crate) fn close_backend_gui(
    backend: Option<&LoadedBackend>,
    plugin_window: &mut Option<PluginWindow>,
) {
    if let Some(mut win) = plugin_window.take() {
        win.close();
    }
    if let Some(b) = backend {
        match b {
            LoadedBackend::NativeVst2(plugin_arc) => {
                if let Ok(mut plugin) = plugin_arc.lock() {
                    plugin.close_editor_window();
                }
            }
            #[cfg(target_os = "linux")]
            LoadedBackend::Wine(wine) => {
                wine.close_gui();
            }
            LoadedBackend::Native(_) => {}
        }
    }
}

pub(crate) fn is_backend_gui_open(
    backend: &LoadedBackend,
    plugin_window: &Option<PluginWindow>,
) -> bool {
    match backend {
        LoadedBackend::Native(_) => plugin_window
            .as_ref()
            .is_some_and(|w| w.is_open() && !w.closed_by_user()),
        LoadedBackend::NativeVst2(plugin_arc) => plugin_arc
            .try_lock()
            .map(|p| p.is_editor_window_open())
            .unwrap_or(true),
        #[cfg(target_os = "linux")]
        LoadedBackend::Wine(wine) => wine.is_gui_open(),
    }
}

pub(crate) fn service_backend_gui(
    backend: Option<&LoadedBackend>,
    plugin_window: &mut Option<PluginWindow>,
) {
    let mut should_close = false;
    if let Some(win) = plugin_window {
        if win.closed_by_user() {
            should_close = true;
        } else if let Err(e) = win.service_platform_events() {
            log::warn!("Plugin window event error: {e}");
        }
    }
    if let Some(LoadedBackend::NativeVst2(plugin_arc)) = backend {
        if let Ok(mut plugin) = plugin_arc.try_lock() {
            plugin.service_editor_window();
        }
    }
    if should_close {
        close_backend_gui(backend, plugin_window);
    }
}

fn collect_parameters(plugin: &Plugin) -> Vec<ParameterDisplay> {
    let Ok(params) = plugin.get_parameters() else {
        return Vec::new();
    };
    params
        .into_iter()
        .map(|param| {
            let formatted = plugin
                .format_parameter(param.id, param.value)
                .unwrap_or_else(|_| format!("{:.2}", param.value));
            ParameterDisplay { param, formatted }
        })
        .collect()
}

#[cfg(target_os = "linux")]
fn wine_params_to_display(params: &[WineParamInfo]) -> Vec<ParameterDisplay> {
    params
        .iter()
        .map(|wp| ParameterDisplay {
            param: Parameter {
                id: wp.id,
                name: wp.name.clone(),
                value: wp.value,
                min: 0.0,
                max: 1.0,
                default: wp.default,
                unit: wp.unit.clone(),
                step_count: wp.step_count,
                can_automate: wp.can_automate,
                is_read_only: wp.is_read_only,
                is_bypass: wp.is_bypass,
                flags: wp.flags,
            },
            formatted: wp.formatted.clone(),
        })
        .collect()
}

fn discover_vst_targets_in_dir(dir: &Path) -> Vec<PluginInfo> {
    let mut out = Vec::new();
    for target in pedal_wine::collect_all_vst_targets(dir) {
        let is_vst3 = target
            .bundle_or_file
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("vst3"));
        let is_wine = matches!(target.kind, Vst3BinaryKind::WindowsPe64);
        let name = target
            .bundle_or_file
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("VST Plugin")
            .to_string();
        let (vendor, category) = match (is_vst3, is_wine) {
            (true, true) => ("Windows VST3 (Wine)".to_string(), "VST3 [Wine]".to_string()),
            (false, true) => ("Windows VST2 (Wine)".to_string(), "VST2 [Wine]".to_string()),
            (true, false) => ("Native VST3".to_string(), "VST3".to_string()),
            (false, false) => ("Native VST2".to_string(), "VST2".to_string()),
        };
        out.push(PluginInfo {
            path: target.bundle_or_file.clone(),
            name,
            vendor,
            version: String::new(),
            category,
            uid: format!("vst:{}", target.bundle_or_file.display()),
            audio_inputs: 1,
            audio_outputs: 1,
            has_midi_input: false,
            has_midi_output: false,
            has_gui: true,
        });
    }
    out
}

#[cfg(target_os = "linux")]
fn discover_wine_vst3_plugins(config: &WineConfig) -> Vec<PluginInfo> {
    let mut prefixes = Vec::new();
    let mut push_prefix = |p: PathBuf| {
        if !prefixes.contains(&p) {
            prefixes.push(p);
        }
    };

    if let Some(custom) = &config.wine_prefix {
        push_prefix(custom.clone());
    }
    if let Some(resolved) = resolve_wine_prefix(config) {
        push_prefix(resolved);
    }
    for info in discover_wine_prefixes() {
        if info.initialized {
            push_prefix(info.path);
        }
    }

    let mut found = Vec::new();
    for prefix in prefixes {
        for root in standard_prefix_vst_dirs(&prefix) {
            if root.is_dir() {
                for info in discover_vst_targets_in_dir(&root) {
                    if !found.iter().any(|p: &PluginInfo| p.path == info.path) {
                        found.push(info);
                    }
                }
            }
        }
    }
    found
}

fn is_vst2_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dll") || e.eq_ignore_ascii_case("so"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dsp_gain_mute_and_meters_work_without_plugin() {
        let shared = Arc::new(SharedEngineState::default());
        shared.set_input_gain_db(6.0206); // ~2x amplitude
        shared.set_output_gain_db(0.0);

        let (_tx, rx) = RingBuffer::<EngineCommand>::new(16);
        let mut dsp = PedalDspProcessor::new(Arc::clone(&shared), rx, 48_000.0);

        let in_l = [0.25_f32, -0.25, 0.1, 0.0];
        let in_r = [0.1_f32, 0.2, -0.3, 0.0];
        let mut out_l = [0.0_f32; 4];
        let mut out_r = [0.0_f32; 4];

        dsp.process(&in_l, &in_r, &mut out_l, &mut out_r, 4, 48_000.0);

        assert!((out_l[0] - 0.5).abs() < 0.01);
        assert!((out_r[2] + 0.6).abs() < 0.01);

        let meters = shared.take_meters();
        assert!((meters.in_left - 0.5).abs() < 0.01);
        assert!((meters.out_right - 0.6).abs() < 0.01);

        shared.set_monitor_muted(true);
        dsp.process(&in_l, &in_r, &mut out_l, &mut out_r, 4, 48_000.0);
        assert_eq!(out_l, [0.0; 4]);
        assert_eq!(out_r, [0.0; 4]);
    }

    #[test]
    fn dsp_pushes_recorded_samples_when_recording_active() {
        let shared = Arc::new(SharedEngineState::default());
        let (mut cmd_tx, cmd_rx) = RingBuffer::<EngineCommand>::new(16);
        let (rec_tx, mut rec_rx) = RingBuffer::<f32>::new(64);
        cmd_tx.push(EngineCommand::SetRecorder(Some(rec_tx))).unwrap();
        shared.recording_active.store(true, Ordering::Relaxed);

        let mut dsp = PedalDspProcessor::new(Arc::clone(&shared), cmd_rx, 48_000.0);
        let in_l = [0.5_f32, -0.5];
        let in_r = [0.25_f32, -0.25];
        let mut out_l = [0.0_f32; 2];
        let mut out_r = [0.0_f32; 2];

        dsp.process(&in_l, &in_r, &mut out_l, &mut out_r, 2, 48_000.0);

        assert_eq!(shared.recorded_frames(), 2);
        assert_eq!(rec_rx.pop().unwrap(), 0.5);
        assert_eq!(rec_rx.pop().unwrap(), 0.25);
        assert_eq!(rec_rx.pop().unwrap(), -0.5);
        assert_eq!(rec_rx.pop().unwrap(), -0.25);
    }
}
