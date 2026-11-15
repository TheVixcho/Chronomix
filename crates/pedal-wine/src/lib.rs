use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use serde::Deserialize;
use thiserror::Error;

pub mod env;
pub use env::*;

/// Detected executable format of a VST3 plugin binary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Vst3BinaryKind {
    NativeElf,
    WindowsPe64,
    WindowsPe32,
    Unknown,
}

/// Resolution result for a VST3 file, bundle, or parent directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedVst3Target {
    pub bundle_or_file: PathBuf,
    pub binary_path: PathBuf,
    pub kind: Vst3BinaryKind,
}

/// Errors returned by the Wine VST3 bridge.
#[derive(Debug, Error)]
pub enum WineError {
    #[error("Wine VST3 bridge is only used on Linux")]
    UnsupportedOs,
    #[error("Wine VST3 host was not compiled (wineg++ missing at build time and runtime)")]
    BridgeNotBuilt,
    #[error("32-bit (x86) Windows VST3 is not supported by this 64-bit Wine bridge: {0}. Use the 64-bit (x64) .vst3 instead.")]
    UnsupportedArch32(String),
    #[error("Failed to launch Wine ({0}). Ensure `wine` is installed.")]
    Launch(String),
    #[error("Wine plugin initialization failed: {0}")]
    PluginInit(String),
    #[error("Wine bridge IPC error: {0}")]
    Ipc(String),
}

/// Parameter metadata returned by the Wine VST3 host.
#[derive(Debug, Clone, Deserialize)]
pub struct WineParamInfo {
    pub id: u32,
    pub name: String,
    pub value: f64,
    #[serde(default)]
    pub default: f64,
    #[serde(default)]
    pub unit: String,
    #[serde(default)]
    pub step_count: i32,
    #[serde(default)]
    pub can_automate: bool,
    #[serde(default)]
    pub is_read_only: bool,
    #[serde(default)]
    pub is_bypass: bool,
    #[serde(default)]
    pub flags: u32,
    #[serde(default)]
    pub formatted: String,
}

/// Metadata returned on initial Wine VST3 handshake.
#[derive(Debug, Clone, Deserialize)]
pub struct WinePluginMetadata {
    pub name: String,
    pub vendor: String,
    pub version: String,
    pub category: String,
    pub in_channels: usize,
    pub out_channels: usize,
    pub has_gui: bool,
    pub params: Vec<WineParamInfo>,
}

/// Inspect magic bytes and COFF machine header of a file to determine ELF vs PE64 vs PE32.
pub fn inspect_binary_kind(path: &Path) -> Vst3BinaryKind {
    let Ok(mut f) = File::open(path) else {
        return Vst3BinaryKind::Unknown;
    };
    let mut dos = [0_u8; 64];
    let n = f.read(&mut dos).unwrap_or(0);
    if n < 4 {
        return Vst3BinaryKind::Unknown;
    }
    if dos[0..4] == [0x7F, b'E', b'L', b'F'] {
        return Vst3BinaryKind::NativeElf;
    }
    if dos[0] != b'M' || dos[1] != b'Z' {
        return Vst3BinaryKind::Unknown;
    }
    if n < 64 {
        return Vst3BinaryKind::Unknown;
    }
    let e_lfanew = u32::from_le_bytes([dos[60], dos[61], dos[62], dos[63]]) as u64;
    if f.seek(SeekFrom::Start(e_lfanew)).is_err() {
        return Vst3BinaryKind::Unknown;
    }
    let mut pe = [0_u8; 6];
    if f.read_exact(&mut pe).is_err() || &pe[0..4] != b"PE\0\0" {
        return Vst3BinaryKind::Unknown;
    }
    let machine = u16::from_le_bytes([pe[4], pe[5]]);
    match machine {
        0x014c => Vst3BinaryKind::WindowsPe32,
        0x8664 | 0xaa64 => Vst3BinaryKind::WindowsPe64,
        _ => Vst3BinaryKind::Unknown,
    }
}

/// Resolve a `.vst3` / `.dll` / `.so` file, bundle directory, or parent folder into the best
/// binary candidate.
///
/// On Linux, priority order is `NativeElf` > `WindowsPe64` > `WindowsPe32`, preferring `.vst3`
/// over standalone `.dll`/`.so` when both exist.
pub fn resolve_vst3_target(path: &Path) -> Option<ResolvedVst3Target> {
    if path.is_file() {
        return Some(ResolvedVst3Target {
            bundle_or_file: path.to_path_buf(),
            binary_path: path.to_path_buf(),
            kind: inspect_binary_kind(path),
        });
    }
    if !path.is_dir() {
        return None;
    }

    if has_vst3_extension(path) {
        return resolve_vst3_bundle_dir(path);
    }

    let mut candidates = Vec::new();
    collect_vst3_targets(path, 0, &mut candidates);
    candidates.sort_by_key(|c| {
        let kind_rank = match c.kind {
            Vst3BinaryKind::NativeElf => 0,
            Vst3BinaryKind::WindowsPe64 => 1,
            Vst3BinaryKind::WindowsPe32 => 2,
            Vst3BinaryKind::Unknown => 3,
        };
        let ext_rank = if has_vst3_extension(&c.bundle_or_file) {
            0
        } else {
            1
        };
        (kind_rank, ext_rank)
    });
    candidates.into_iter().next()
}

/// Collect all VST3 and VST2 plugin targets inside a directory tree, deduplicating 32-bit (`x86`)
/// variants when a 64-bit (`NativeElf` or `WindowsPe64`) plugin with the same stem is present.
pub fn collect_all_vst_targets(dir: &Path) -> Vec<ResolvedVst3Target> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut raw = Vec::new();
    collect_vst3_targets(dir, 0, &mut raw);

    let mut out: Vec<ResolvedVst3Target> = Vec::new();
    for item in raw {
        if item.kind == Vst3BinaryKind::WindowsPe32 {
            // Skip 32-bit if a 64-bit target exists, since 32-bit is not supported on x86_64 host.
            continue;
        }
        if !out.iter().any(|existing| existing.binary_path == item.binary_path) {
            out.push(item);
        }
    }
    out
}

/// Backwards-compatible wrapper returning only `(binary_path, kind)`.
pub fn resolve_vst3_binary(path: &Path) -> Option<(PathBuf, Vst3BinaryKind)> {
    resolve_vst3_target(path).map(|t| (t.binary_path, t.kind))
}

fn has_vst3_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("vst3"))
}

fn has_vst2_binary_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("dll") || e.eq_ignore_ascii_case("so"))
}

fn resolve_vst3_bundle_dir(bundle: &Path) -> Option<ResolvedVst3Target> {
    let contents = bundle.join("Contents");
    let checks = [
        (contents.join("x86_64-linux"), "so", Vst3BinaryKind::NativeElf),
        (contents.join("x86_64-win"), "vst3", Vst3BinaryKind::WindowsPe64),
        (contents.join("x86_64-win"), "dll", Vst3BinaryKind::WindowsPe64),
        (contents.join("x86-win"), "vst3", Vst3BinaryKind::WindowsPe32),
        (contents.join("x86-win"), "dll", Vst3BinaryKind::WindowsPe32),
    ];
    for (dir, ext, expected) in checks {
        if let Some((bin, kind)) = first_matching_binary(&dir, ext, expected) {
            return Some(ResolvedVst3Target {
                bundle_or_file: bundle.to_path_buf(),
                binary_path: bin,
                kind,
            });
        }
    }
    None
}

fn collect_vst3_targets(dir: &Path, depth: usize, out: &mut Vec<ResolvedVst3Target>) {
    if depth > 4 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let p = entry.path();
        if has_vst3_extension(&p) {
            if p.is_file() {
                let kind = inspect_binary_kind(&p);
                if kind != Vst3BinaryKind::Unknown {
                    out.push(ResolvedVst3Target {
                        bundle_or_file: p.clone(),
                        binary_path: p,
                        kind,
                    });
                }
            } else if p.is_dir() {
                if let Some(target) = resolve_vst3_bundle_dir(&p) {
                    out.push(target);
                }
            }
        } else if p.is_file() && has_vst2_binary_extension(&p) {
            let kind = inspect_binary_kind(&p);
            if kind != Vst3BinaryKind::Unknown {
                out.push(ResolvedVst3Target {
                    bundle_or_file: p.clone(),
                    binary_path: p,
                    kind,
                });
            }
        } else if p.is_dir() {
            subdirs.push(p);
        }
    }
    for sub in subdirs {
        collect_vst3_targets(&sub, depth + 1, out);
    }
}

fn first_matching_binary(
    dir: &Path,
    ext: &str,
    expected: Vst3BinaryKind,
) -> Option<(PathBuf, Vst3BinaryKind)> {
    let entries = std::fs::read_dir(dir).ok()?;
    for entry in entries.flatten() {
        let p = entry.path();
        if p.is_file()
            && p.extension()
                .and_then(|e| e.to_str())
                .is_some_and(|e| e.eq_ignore_ascii_case(ext))
        {
            let kind = inspect_binary_kind(&p);
            if kind == expected {
                return Some((p, kind));
            }
        }
    }
    None
}

#[cfg(target_os = "linux")]
mod linux_impl {
    use super::*;
    use std::{
        fs::{self, OpenOptions},
        io::{BufRead, BufReader, Write},
        os::unix::{
            fs::PermissionsExt,
            net::{UnixListener, UnixStream},
        },
        process::{Child, Command, Stdio},
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicU64, Ordering},
        },
        time::{Duration, Instant},
    };

    #[cfg(has_wine_host)]
    const WINE_HOST_EXE: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/chronomix-wine-host.exe"));
    #[cfg(has_wine_host)]
    const WINE_HOST_SO: &[u8] =
        include_bytes!(concat!(env!("OUT_DIR"), "/chronomix-wine-host.exe.so"));

    const SHM_MAGIC: u32 = 0x4F50_454E;
    const SHM_MAX_FRAMES: usize = 4096;
    const SHM_MAX_PARAMS: usize = 128;

    #[repr(C)]
    struct ShmAudioBlock {
        magic: u32,
        max_frames: u32,
        num_samples: u32,
        pending_param_count: u32,
        sample_rate: f64,
        pending_param_ids: [u32; SHM_MAX_PARAMS],
        pending_param_vals: [f64; SHM_MAX_PARAMS],
        in_l: [f32; SHM_MAX_FRAMES],
        in_r: [f32; SHM_MAX_FRAMES],
        out_l: [f32; SHM_MAX_FRAMES],
        out_r: [f32; SHM_MAX_FRAMES],
    }

    struct ShmMapping {
        ptr: *mut ShmAudioBlock,
    }

    // SAFETY: Access to `ShmMapping` is serialized through `Mutex<WineAudioChannel>` on the host
    // and synchronized via `audio_sock` round-trips with the Wine audio thread.
    unsafe impl Send for ShmMapping {}

    impl Drop for ShmMapping {
        fn drop(&mut self) {
            if !self.ptr.is_null() {
                unsafe {
                    libc::munmap(self.ptr as *mut libc::c_void, std::mem::size_of::<ShmAudioBlock>());
                }
            }
        }
    }

    #[derive(Deserialize)]
    #[serde(untagged)]
    enum HandshakeReply {
        Error { error: String },
        Ok(WinePluginMetadata),
    }

    pub struct WineAudioChannel {
        shm: ShmMapping,
        audio_sock: UnixStream,
        out_channels: usize,
        queued_params: Vec<(u32, f64)>,
    }

    impl WineAudioChannel {
        pub fn queue_param(&mut self, id: u32, value: f64) {
            if let Some(existing) = self.queued_params.iter_mut().find(|(pid, _)| *pid == id) {
                existing.1 = value;
            } else if self.queued_params.len() < SHM_MAX_PARAMS {
                self.queued_params.push((id, value));
            }
        }

        pub fn process_stereo(
            &mut self,
            in_l: &[f32],
            in_r: &[f32],
            out_l: &mut [f32],
            out_r: &mut [f32],
            frames: usize,
            sample_rate: f64,
        ) -> bool {
            let mut offset = 0;
            while offset < frames {
                let n = (frames - offset).min(SHM_MAX_FRAMES);
                if n == 0 {
                    break;
                }

                let shm = unsafe { &mut *self.shm.ptr };
                shm.num_samples = n as u32;
                shm.sample_rate = sample_rate;
                shm.in_l[..n].copy_from_slice(&in_l[offset..offset + n]);
                shm.in_r[..n].copy_from_slice(&in_r[offset..offset + n]);

                let pcount = self.queued_params.len().min(SHM_MAX_PARAMS);
                shm.pending_param_count = pcount as u32;
                for (idx, (pid, val)) in self.queued_params.drain(..pcount).enumerate() {
                    shm.pending_param_ids[idx] = pid;
                    shm.pending_param_vals[idx] = val;
                }

                if self.audio_sock.write_all(b"P").is_err() {
                    return false;
                }
                let mut ack = [0_u8; 1];
                if self.audio_sock.read_exact(&mut ack).is_err() || ack[0] != 1 {
                    return false;
                }

                out_l[offset..offset + n].copy_from_slice(&shm.out_l[..n]);
                if self.out_channels >= 2 {
                    out_r[offset..offset + n].copy_from_slice(&shm.out_r[..n]);
                } else {
                    out_r[offset..offset + n].copy_from_slice(&shm.out_l[..n]);
                }
                offset += n;
            }
            true
        }
    }

    pub struct WineVst3Plugin {
        meta: WinePluginMetadata,
        audio_channel: Arc<Mutex<WineAudioChannel>>,
        ctrl_reader: Mutex<BufReader<UnixStream>>,
        ctrl_writer: Mutex<UnixStream>,
        child: Mutex<Option<Child>>,
        temp_dir: PathBuf,
        gui_open: AtomicBool,
        last_gui_poll: Mutex<Instant>,
    }

    static WINE_INSTANCE_SEQ: AtomicU64 = AtomicU64::new(1);

    impl WineVst3Plugin {
        pub fn load(dll_path: &Path, sample_rate: f64, block_size: usize) -> Result<Self, WineError> {
            Self::load_with_config(dll_path, sample_rate, block_size, &WineConfig::default())
        }

        pub fn load_with_config(
            dll_path: &Path,
            sample_rate: f64,
            block_size: usize,
            config: &WineConfig,
        ) -> Result<Self, WineError> {
            let (wine_bin, prefix) = ensure_prefix_ready_for_plugin(config)?;
            let (host_so, host_dir) = materialize_wine_host_files(config)?;

            let seq = WINE_INSTANCE_SEQ.fetch_add(1, Ordering::Relaxed);
            let unique = format!(
                "c-{}-{}-{}",
                std::process::id(),
                seq,
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos() % 1_000_000_000)
                    .unwrap_or(0)
            );
            let temp_dir = short_ipc_dir().join(unique);
            fs::create_dir_all(&temp_dir)
                .map_err(|e| WineError::Ipc(format!("create temp dir: {e}")))?;

            let shm_path = temp_dir.join("a.shm");
            let ctrl_sock_path = temp_dir.join("c.sock");
            let audio_sock_path = temp_dir.join("a.sock");

            let shm_len = std::mem::size_of::<ShmAudioBlock>();
            let shm_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .truncate(true)
                .open(&shm_path)
                .map_err(|e| {
                    let _ = fs::remove_dir_all(&temp_dir);
                    WineError::Ipc(format!("create shm: {e}"))
                })?;
            shm_file.set_len(shm_len as u64).map_err(|e| {
                let _ = fs::remove_dir_all(&temp_dir);
                WineError::Ipc(format!("size shm: {e}"))
            })?;

            use std::os::fd::AsRawFd;
            let raw_ptr = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    shm_len,
                    libc::PROT_READ | libc::PROT_WRITE,
                    libc::MAP_SHARED,
                    shm_file.as_raw_fd(),
                    0,
                )
            };
            if raw_ptr == libc::MAP_FAILED {
                let _ = fs::remove_dir_all(&temp_dir);
                return Err(WineError::Ipc("mmap failed".into()));
            }
            let shm_ptr = raw_ptr as *mut ShmAudioBlock;
            unsafe {
                std::ptr::write_bytes(shm_ptr, 0, 1);
                (*shm_ptr).magic = SHM_MAGIC;
                (*shm_ptr).max_frames = SHM_MAX_FRAMES as u32;
                (*shm_ptr).sample_rate = sample_rate;
            }
            let shm = ShmMapping { ptr: shm_ptr };

            let ctrl_listener = UnixListener::bind(&ctrl_sock_path).map_err(|e| {
                let _ = fs::remove_dir_all(&temp_dir);
                WineError::Ipc(format!("bind ctrl socket: {e}"))
            })?;
            let audio_listener = UnixListener::bind(&audio_sock_path).map_err(|e| {
                let _ = fs::remove_dir_all(&temp_dir);
                WineError::Ipc(format!("bind audio socket: {e}"))
            })?;
            ctrl_listener.set_nonblocking(true).ok();
            audio_listener.set_nonblocking(true).ok();

            let mut winedllpath_entries = vec![host_dir];
            if let Some(existing) = std::env::var_os("WINEDLLPATH") {
                winedllpath_entries.extend(std::env::split_paths(&existing));
            }
            let winedllpath = std::env::join_paths(winedllpath_entries)
                .map_err(|e| WineError::Launch(format!("invalid WINEDLLPATH: {e}")))?;

            let mut cmd = Command::new(&wine_bin);
            cmd.arg(&host_so)
                .env("WINEDLLPATH", winedllpath)
                .env("CHRONOMIX_DLL", dll_path)
                .env("CHRONOMIX_SHM", &shm_path)
                .env("CHRONOMIX_CTRL_SOCK", &ctrl_sock_path)
                .env("CHRONOMIX_AUDIO_SOCK", &audio_sock_path)
                .env("CHRONOMIX_SR", format!("{sample_rate}"))
                .env("CHRONOMIX_BLOCK", format!("{block_size}"))
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::piped());
            apply_wine_process_env(&mut cmd, &wine_bin, &prefix, config.auto_display, true);

            let mut child = cmd
                .spawn()
                .map_err(|e| {
                    let _ = fs::remove_dir_all(&temp_dir);
                    WineError::Launch(e.to_string())
                })?;

            let deadline = Instant::now() + Duration::from_secs(30);
            let ctrl_stream = accept_with_timeout(&ctrl_listener, &mut child, deadline)?;
            let audio_stream = accept_with_timeout(&audio_listener, &mut child, deadline)?;

            ctrl_stream.set_nonblocking(false).ok();
            audio_stream.set_nonblocking(false).ok();
            ctrl_stream
                .set_read_timeout(Some(Duration::from_secs(10)))
                .ok();
            audio_stream
                .set_read_timeout(Some(Duration::from_millis(500)))
                .ok();

            let ctrl_writer = ctrl_stream
                .try_clone()
                .map_err(|e| WineError::Ipc(format!("clone ctrl socket: {e}")))?;
            let mut ctrl_reader = BufReader::new(ctrl_stream);

            let mut line = String::new();
            ctrl_reader
                .read_line(&mut line)
                .map_err(|e| WineError::PluginInit(format!("handshake read failed: {e}")))?;
            if line.trim().is_empty() {
                return Err(WineError::PluginInit(
                    "Wine host exited before sending metadata".into(),
                ));
            }

            let reply: HandshakeReply = serde_json::from_str(&line)
                .map_err(|e| WineError::PluginInit(format!("invalid handshake JSON: {e}")))?;
            let meta = match reply {
                HandshakeReply::Error { error } => return Err(WineError::PluginInit(error)),
                HandshakeReply::Ok(m) => m,
            };

            let out_channels = meta.out_channels;
            let audio_channel = Arc::new(Mutex::new(WineAudioChannel {
                shm,
                audio_sock: audio_stream,
                out_channels,
                queued_params: Vec::with_capacity(SHM_MAX_PARAMS),
            }));

            Ok(Self {
                meta,
                audio_channel,
                ctrl_reader: Mutex::new(ctrl_reader),
                ctrl_writer: Mutex::new(ctrl_writer),
                child: Mutex::new(Some(child)),
                temp_dir,
                gui_open: AtomicBool::new(false),
                last_gui_poll: Mutex::new(Instant::now()),
            })
        }

        pub fn metadata(&self) -> &WinePluginMetadata {
            &self.meta
        }

        pub fn audio_channel(&self) -> Arc<Mutex<WineAudioChannel>> {
            Arc::clone(&self.audio_channel)
        }

        pub fn set_parameter(&self, id: u32, value: f64) -> Result<String, WineError> {
            let mut writer = self
                .ctrl_writer
                .lock()
                .map_err(|_| WineError::Ipc("ctrl writer poisoned".into()))?;
            writeln!(writer, "SET_PARAM {id} {value}")
                .map_err(|e| WineError::Ipc(e.to_string()))?;
            writer.flush().map_err(|e| WineError::Ipc(e.to_string()))?;
            drop(writer);

            let mut reader = self
                .ctrl_reader
                .lock()
                .map_err(|_| WineError::Ipc("ctrl reader poisoned".into()))?;
            let mut resp = String::new();
            reader
                .read_line(&mut resp)
                .map_err(|e| WineError::Ipc(e.to_string()))?;
            Ok(resp.trim().to_string())
        }

        pub fn get_parameters(&self) -> Result<Vec<WineParamInfo>, WineError> {
            let mut writer = self
                .ctrl_writer
                .lock()
                .map_err(|_| WineError::Ipc("ctrl writer poisoned".into()))?;
            writer
                .write_all(b"GET_PARAMS\n")
                .map_err(|e| WineError::Ipc(e.to_string()))?;
            writer.flush().map_err(|e| WineError::Ipc(e.to_string()))?;
            drop(writer);

            let mut reader = self
                .ctrl_reader
                .lock()
                .map_err(|_| WineError::Ipc("ctrl reader poisoned".into()))?;
            let mut resp = String::new();
            reader
                .read_line(&mut resp)
                .map_err(|e| WineError::Ipc(e.to_string()))?;
            serde_json::from_str(&resp).map_err(|e| WineError::Ipc(format!("params JSON: {e}")))
        }

        pub fn open_gui(&self) -> Result<(), WineError> {
            let resp = self.send_ctrl_line("OPEN_GUI")?;
            if resp == "OK" {
                self.gui_open.store(true, Ordering::Relaxed);
                Ok(())
            } else {
                Err(WineError::Ipc(resp))
            }
        }

        pub fn close_gui(&self) {
            self.gui_open.store(false, Ordering::Relaxed);
            let _ = self.send_ctrl_line("CLOSE_GUI");
        }

        pub fn is_gui_open(&self) -> bool {
            if !self.gui_open.load(Ordering::Relaxed) {
                return false;
            }
            if let Ok(mut last) = self.last_gui_poll.try_lock() {
                if last.elapsed() >= Duration::from_millis(250) {
                    *last = Instant::now();
                    let open = self
                        .send_ctrl_line("IS_GUI_OPEN")
                        .map(|s| s == "1")
                        .unwrap_or(false);
                    self.gui_open.store(open, Ordering::Relaxed);
                    return open;
                }
            }
            true
        }

        fn send_ctrl_line(&self, line: &str) -> Result<String, WineError> {
            let mut writer = self
                .ctrl_writer
                .lock()
                .map_err(|_| WineError::Ipc("ctrl writer poisoned".into()))?;
            writeln!(writer, "{line}").map_err(|e| WineError::Ipc(e.to_string()))?;
            writer.flush().map_err(|e| WineError::Ipc(e.to_string()))?;
            drop(writer);

            let mut reader = self
                .ctrl_reader
                .lock()
                .map_err(|_| WineError::Ipc("ctrl reader poisoned".into()))?;
            let mut resp = String::new();
            reader
                .read_line(&mut resp)
                .map_err(|e| WineError::Ipc(e.to_string()))?;
            Ok(resp.trim().to_string())
        }
    }

    impl Drop for WineVst3Plugin {
        fn drop(&mut self) {
            if let Ok(mut writer) = self.ctrl_writer.lock() {
                let _ = writer.write_all(b"QUIT\n");
                let _ = writer.flush();
            }
            if let Ok(mut guard) = self.child.lock() {
                if let Some(mut child) = guard.take() {
                    let deadline = Instant::now() + Duration::from_secs(1);
                    loop {
                        match child.try_wait() {
                            Ok(Some(_)) => break,
                            Ok(None) if Instant::now() < deadline => {
                                std::thread::sleep(Duration::from_millis(20));
                            }
                            _ => {
                                let _ = child.kill();
                                let _ = child.wait();
                                break;
                            }
                        }
                    }
                }
            }
            let _ = fs::remove_dir_all(&self.temp_dir);
        }
    }

    fn accept_with_timeout(
        listener: &UnixListener,
        child: &mut Child,
        deadline: Instant,
    ) -> Result<UnixStream, WineError> {
        loop {
            match listener.accept() {
                Ok((stream, _)) => return Ok(stream),
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    if let Ok(Some(status)) = child.try_wait() {
                        let mut err_text = String::new();
                        if let Some(ref mut stderr) = child.stderr {
                            let _ = stderr.read_to_string(&mut err_text);
                        }
                        let summary = summarize_wine_stderr(&err_text);
                        return Err(WineError::PluginInit(format!(
                            "Wine host exited early ({status}){summary}"
                        )));
                    }
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        return Err(WineError::PluginInit(
                            "Timed out waiting for Wine host to connect".into(),
                        ));
                    }
                    std::thread::sleep(Duration::from_millis(15));
                }
                Err(e) => return Err(WineError::Ipc(format!("accept failed: {e}"))),
            }
        }
    }

    fn summarize_wine_stderr(raw: &str) -> String {
        let filtered: Vec<&str> = raw
            .lines()
            .map(str::trim)
            .filter(|line| {
                !line.is_empty()
                    && !line.starts_with("MESA-EGL:")
                    && !line.starts_with("pci id for fd")
            })
            .collect();
        let chosen = if filtered.is_empty() {
            raw.lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect::<Vec<_>>()
        } else {
            filtered
        };
        if chosen.is_empty() {
            String::new()
        } else {
            let start = chosen.len().saturating_sub(6);
            format!(": {}", chosen[start..].join(" | "))
        }
    }

    #[cfg_attr(has_wine_host, allow(unused_variables))]
    fn materialize_wine_host_files(config: &WineConfig) -> Result<(PathBuf, PathBuf), WineError> {
        if let Some(cached) = cached_runtime_bridge_paths() {
            return Ok(cached);
        }

        #[cfg(has_wine_host)]
        {
            let base_dir = std::env::var_os("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
                .unwrap_or_else(std::env::temp_dir)
                .join("chronomix")
                .join("wine-host");

            fs::create_dir_all(&base_dir)
                .map_err(|e| WineError::Ipc(format!("create cache dir: {e}")))?;

            let exe_path = base_dir.join("chronomix-wine-host.exe");
            let so_path = base_dir.join("chronomix-wine-host.exe.so");

            write_if_changed(&exe_path, WINE_HOST_EXE)?;
            write_if_changed(&so_path, WINE_HOST_SO)?;
            return Ok((so_path, base_dir));
        }

        #[cfg(not(has_wine_host))]
        {
            rebuild_wine_bridge(config)
        }
    }

    #[cfg(has_wine_host)]
    fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<(), WineError> {
        let same = fs::read(path).map(|existing| existing == bytes).unwrap_or(false);
        if !same {
            fs::write(path, bytes).map_err(|e| WineError::Ipc(format!("write {}: {e}", path.display())))?;
            let mut perms = fs::metadata(path)
                .map_err(|e| WineError::Ipc(e.to_string()))?
                .permissions();
            perms.set_mode(0o755);
            fs::set_permissions(path, perms).map_err(|e| WineError::Ipc(e.to_string()))?;
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
mod stub_impl {
    use super::*;
    use std::sync::{Arc, Mutex};

    pub struct WineAudioChannel;

    impl WineAudioChannel {
        pub fn queue_param(&mut self, _id: u32, _value: f64) {}

        pub fn process_stereo(
            &mut self,
            _in_l: &[f32],
            _in_r: &[f32],
            _out_l: &mut [f32],
            _out_r: &mut [f32],
            _frames: usize,
            _sample_rate: f64,
        ) -> bool {
            false
        }
    }

    pub struct WineVst3Plugin {
        meta: WinePluginMetadata,
        audio_channel: Arc<Mutex<WineAudioChannel>>,
    }

    impl WineVst3Plugin {
        pub fn load(dll_path: &Path, sample_rate: f64, block_size: usize) -> Result<Self, WineError> {
            Self::load_with_config(dll_path, sample_rate, block_size, &WineConfig::default())
        }

        pub fn load_with_config(
            _dll_path: &Path,
            _sample_rate: f64,
            _block_size: usize,
            _config: &WineConfig,
        ) -> Result<Self, WineError> {
            Err(WineError::UnsupportedOs)
        }

        pub fn metadata(&self) -> &WinePluginMetadata {
            &self.meta
        }

        pub fn audio_channel(&self) -> Arc<Mutex<WineAudioChannel>> {
            Arc::clone(&self.audio_channel)
        }

        pub fn set_parameter(&self, _id: u32, _value: f64) -> Result<String, WineError> {
            Err(WineError::UnsupportedOs)
        }

        pub fn get_parameters(&self) -> Result<Vec<WineParamInfo>, WineError> {
            Err(WineError::UnsupportedOs)
        }

        pub fn open_gui(&self) -> Result<(), WineError> {
            Err(WineError::UnsupportedOs)
        }

        pub fn close_gui(&self) {}

        pub fn is_gui_open(&self) -> bool {
            false
        }
    }
}

#[cfg(target_os = "linux")]
pub use linux_impl::{WineAudioChannel, WineVst3Plugin};

#[cfg(not(target_os = "linux"))]
pub use stub_impl::{WineAudioChannel, WineVst3Plugin};

/// Whether the Wine bridge is compiled or compilable at runtime on this host.
pub fn is_wine_bridge_available() -> bool {
    cfg!(target_os = "linux")
        && !matches!(
            current_bridge_mode(resolve_wine_binary(&WineConfig::default()).as_deref()),
            WineBridgeMode::Unavailable
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fake_pe_bytes(machine: u16) -> Vec<u8> {
        let mut buf = vec![0_u8; 128];
        buf[0] = b'M';
        buf[1] = b'Z';
        buf[60..64].copy_from_slice(&64_u32.to_le_bytes());
        buf[64..68].copy_from_slice(b"PE\0\0");
        buf[68..70].copy_from_slice(&machine.to_le_bytes());
        buf
    }

    #[test]
    fn detects_elf_and_pe_headers() {
        let dir = std::env::temp_dir().join(format!("openpedal-bin-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let elf = dir.join("native.so");
        std::fs::write(&elf, [0x7F, b'E', b'L', b'F', 2, 1, 1, 0]).unwrap();
        assert_eq!(inspect_binary_kind(&elf), Vst3BinaryKind::NativeElf);

        let pe64 = dir.join("win64.vst3");
        std::fs::write(&pe64, fake_pe_bytes(0x8664)).unwrap();
        assert_eq!(inspect_binary_kind(&pe64), Vst3BinaryKind::WindowsPe64);

        let pe32 = dir.join("win32.vst3");
        std::fs::write(&pe32, fake_pe_bytes(0x014c)).unwrap();
        assert_eq!(inspect_binary_kind(&pe32), Vst3BinaryKind::WindowsPe32);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn resolves_windows_vst3_bundle_layout_and_prefers_x64_in_parent_dir() {
        let dir = std::env::temp_dir().join(format!("openpedal-bundle-test-{}", std::process::id()));
        let bundle_win = dir.join("MyPlugin.vst3").join("Contents").join("x86_64-win");
        std::fs::create_dir_all(&bundle_win).unwrap();
        let dll = bundle_win.join("MyPlugin.vst3");
        std::fs::write(&dll, fake_pe_bytes(0x8664)).unwrap();

        let resolved = resolve_vst3_binary(&dir.join("MyPlugin.vst3")).unwrap();
        assert_eq!(resolved.0, dll);
        assert_eq!(resolved.1, Vst3BinaryKind::WindowsPe64);

        // Parent directory containing both x86 and x64 plugins automatically selects x64.
        let suite = dir.join("OrilRiver");
        let dir_x86 = suite.join("OrilRiver (x86)");
        let dir_x64 = suite.join("OrilRiver (x64)");
        std::fs::create_dir_all(&dir_x86).unwrap();
        std::fs::create_dir_all(&dir_x64).unwrap();
        std::fs::write(dir_x86.join("OrilRiver.vst3"), fake_pe_bytes(0x014c)).unwrap();
        let x64_file = dir_x64.join("OrilRiver.vst3");
        std::fs::write(&x64_file, fake_pe_bytes(0x8664)).unwrap();

        let picked = resolve_vst3_target(&suite).unwrap();
        assert_eq!(picked.binary_path, x64_file);
        assert_eq!(picked.kind, Vst3BinaryKind::WindowsPe64);

        let only_32 = resolve_vst3_target(&dir_x86).unwrap();
        assert_eq!(only_32.kind, Vst3BinaryKind::WindowsPe32);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
