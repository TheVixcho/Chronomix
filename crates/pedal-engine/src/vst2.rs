use std::{
    ffi::{CStr, c_char, c_void},
    path::{Path, PathBuf},
    ptr,
    sync::Mutex,
};

use libloading::Library;
use vst3_host::{Parameter, PluginInfo};

use crate::ParameterDisplay;

pub const VST2_MAGIC: i32 = 0x5673_7450; // 'VstP'

const EFF_OPEN: i32 = 0;
const EFF_CLOSE: i32 = 1;
const EFF_GET_PARAM_LABEL: i32 = 6;
const EFF_GET_PARAM_DISPLAY: i32 = 7;
const EFF_GET_PARAM_NAME: i32 = 8;
const EFF_SET_SAMPLE_RATE: i32 = 10;
const EFF_SET_BLOCK_SIZE: i32 = 11;
const EFF_MAINS_CHANGED: i32 = 12;
const EFF_EDIT_GET_RECT: i32 = 13;
const EFF_EDIT_OPEN: i32 = 14;
const EFF_EDIT_CLOSE: i32 = 15;
const EFF_EDIT_IDLE: i32 = 19;
const EFF_GET_EFFECT_NAME: i32 = 45;
const EFF_GET_VENDOR_STRING: i32 = 47;
const EFF_GET_PRODUCT_STRING: i32 = 48;
const EFF_START_PROCESS: i32 = 71;
const EFF_STOP_PROCESS: i32 = 72;

const EFF_FLAGS_HAS_EDITOR: i32 = 1 << 0;

const MAX_SCRATCH_FRAMES: usize = 4096;
const MAX_VST2_CHANNELS: usize = 16;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ERect {
    pub top: i16,
    pub left: i16,
    pub bottom: i16,
    pub right: i16,
}

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct VstTimeInfo {
    pub sample_pos: f64,
    pub sample_rate: f64,
    pub nano_seconds: f64,
    pub ppq_pos: f64,
    pub tempo: f64,
    pub bar_start_pos: f64,
    pub cycle_start_pos: f64,
    pub cycle_end_pos: f64,
    pub time_sig_numerator: i32,
    pub time_sig_denominator: i32,
    pub smpte_offset: i32,
    pub smpte_frame_rate: i32,
    pub samples_to_next_clock: i32,
    pub flags: i32,
}

pub type AudioMasterCallback = extern "C" fn(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    opt: f32,
) -> isize;

pub type DispatcherProc = extern "C" fn(
    effect: *mut AEffect,
    opcode: i32,
    index: i32,
    value: isize,
    ptr: *mut c_void,
    opt: f32,
) -> isize;

pub type ProcessProc = extern "C" fn(
    effect: *mut AEffect,
    inputs: *mut *mut f32,
    outputs: *mut *mut f32,
    sample_frames: i32,
);

pub type SetParameterProc = extern "C" fn(effect: *mut AEffect, index: i32, parameter: f32);
pub type GetParameterProc = extern "C" fn(effect: *mut AEffect, index: i32) -> f32;
pub type PluginEntryProc = extern "C" fn(audio_master: AudioMasterCallback) -> *mut AEffect;

#[repr(C)]
pub struct AEffect {
    pub magic: i32,
    pub dispatcher: DispatcherProc,
    pub process: Option<ProcessProc>,
    pub set_parameter: Option<SetParameterProc>,
    pub get_parameter: Option<GetParameterProc>,
    pub num_programs: i32,
    pub num_params: i32,
    pub num_inputs: i32,
    pub num_outputs: i32,
    pub flags: i32,
    pub resvd1: isize,
    pub resvd2: isize,
    pub initial_delay: i32,
    pub real_qualities: i32,
    pub off_qualities: i32,
    pub io_ratio: f32,
    pub object: *mut c_void,
    pub user: *mut c_void,
    pub unique_id: i32,
    pub version: i32,
    pub process_replacing: Option<ProcessProc>,
    pub process_double_replacing: *mut c_void,
    pub future: [u8; 56],
}

static GLOBAL_TIME_INFO: Mutex<VstTimeInfo> = Mutex::new(VstTimeInfo {
    sample_pos: 0.0,
    sample_rate: 48_000.0,
    nano_seconds: 0.0,
    ppq_pos: 0.0,
    tempo: 120.0,
    bar_start_pos: 0.0,
    cycle_start_pos: 0.0,
    cycle_end_pos: 0.0,
    time_sig_numerator: 4,
    time_sig_denominator: 4,
    smpte_offset: 0,
    smpte_frame_rate: 0,
    samples_to_next_clock: 0,
    flags: (1 << 9) | (1 << 10) | (1 << 13),
});

static mut STATIC_TIME_SNAPSHOT: VstTimeInfo = VstTimeInfo {
    sample_pos: 0.0,
    sample_rate: 48_000.0,
    nano_seconds: 0.0,
    ppq_pos: 0.0,
    tempo: 120.0,
    bar_start_pos: 0.0,
    cycle_start_pos: 0.0,
    cycle_end_pos: 0.0,
    time_sig_numerator: 4,
    time_sig_denominator: 4,
    smpte_offset: 0,
    smpte_frame_rate: 0,
    samples_to_next_clock: 0,
    flags: (1 << 9) | (1 << 10) | (1 << 13),
};

extern "C" fn host_audio_master_cb(
    _effect: *mut AEffect,
    opcode: i32,
    _index: i32,
    _value: isize,
    ptr: *mut c_void,
    _opt: f32,
) -> isize {
    match opcode {
        1 => 2400, // audioMasterVersion (VST 2.4)
        7 => {
            // audioMasterGetTime
            if let Ok(guard) = GLOBAL_TIME_INFO.try_lock() {
                unsafe {
                    STATIC_TIME_SNAPSHOT = *guard;
                }
            }
            ptr::addr_of_mut!(STATIC_TIME_SNAPSHOT) as isize
        }
        16 => 48_000, // audioMasterGetSampleRate
        17 => MAX_SCRATCH_FRAMES as isize, // audioMasterGetBlockSize
        32 => {
            // audioMasterGetVendorString
            write_c_str(ptr as *mut c_char, "Chronomix", 64);
            1
        }
        33 => {
            // audioMasterGetProductString
            write_c_str(ptr as *mut c_char, "Chronomix DAW", 64);
            1
        }
        34 => 1000, // audioMasterGetVendorVersion
        37 => 1,    // audioMasterCanDo
        _ => 0,
    }
}

fn write_c_str(dst: *mut c_char, text: &str, max_len: usize) {
    if dst.is_null() || max_len == 0 {
        return;
    }
    let bytes = text.as_bytes();
    let n = bytes.len().min(max_len - 1);
    unsafe {
        ptr::copy_nonoverlapping(bytes.as_ptr() as *const c_char, dst, n);
        *dst.add(n) = 0;
    }
}

fn read_c_buf(buf: &[c_char]) -> String {
    unsafe {
        CStr::from_ptr(buf.as_ptr())
            .to_string_lossy()
            .trim()
            .to_string()
    }
}

/// Native VST 2.4 plugin instance loaded directly in-process.
pub struct NativeVst2Plugin {
    effect: *mut AEffect,
    _library: Option<Library>,
    info: PluginInfo,
    in_channels: usize,
    out_channels: usize,
    in_scratch: Vec<Vec<f32>>,
    out_scratch: Vec<Vec<f32>>,
    in_ptrs: Vec<*mut f32>,
    out_ptrs: Vec<*mut f32>,
    sample_rate: f64,
    #[cfg(target_os = "linux")]
    x11_window: Option<X11EditorWindow>,
}

// Safety: `AEffect` calls are synchronized behind `Arc<Mutex<NativeVst2Plugin>>`.
unsafe impl Send for NativeVst2Plugin {}

impl NativeVst2Plugin {
    pub fn load(path: &Path, sample_rate: f64, block_size: usize) -> Result<Self, String> {
        let lib = unsafe { Library::new(path) }
            .map_err(|e| format!("Failed to load VST2 shared library '{}': {e}", path.display()))?;

        let entry: PluginEntryProc = unsafe {
            if let Ok(sym) = lib.get::<PluginEntryProc>(b"VSTPluginMain\0") {
                *sym
            } else if let Ok(sym) = lib.get::<PluginEntryProc>(b"main_plugin\0") {
                *sym
            } else if let Ok(sym) = lib.get::<PluginEntryProc>(b"main\0") {
                *sym
            } else {
                return Err(format!(
                    "'{}' does not export VSTPluginMain or main (not a VST2 plugin)",
                    path.display()
                ));
            }
        };

        let effect = entry(host_audio_master_cb);
        if effect.is_null() {
            return Err("VSTPluginMain returned null AEffect".into());
        }

        let magic = unsafe { (*effect).magic };
        if magic != VST2_MAGIC {
            return Err(format!("Invalid VST2 magic 0x{magic:08x} (expected 'VstP')"));
        }

        Self::from_raw_effect(effect, Some(lib), path.to_path_buf(), sample_rate, block_size)
    }

    pub(crate) fn from_raw_effect(
        effect: *mut AEffect,
        library: Option<Library>,
        path: PathBuf,
        sample_rate: f64,
        block_size: usize,
    ) -> Result<Self, String> {
        if let Ok(mut t) = GLOBAL_TIME_INFO.lock() {
            t.sample_rate = sample_rate;
        }

        unsafe {
            let disp = (*effect).dispatcher;
            disp(effect, EFF_OPEN, 0, 0, ptr::null_mut(), 0.0);
            disp(
                effect,
                EFF_SET_SAMPLE_RATE,
                0,
                0,
                ptr::null_mut(),
                sample_rate as f32,
            );
            disp(
                effect,
                EFF_SET_BLOCK_SIZE,
                0,
                block_size.max(64) as isize,
                ptr::null_mut(),
                0.0,
            );
            disp(effect, EFF_MAINS_CHANGED, 0, 1, ptr::null_mut(), 0.0);
            disp(effect, EFF_START_PROCESS, 0, 0, ptr::null_mut(), 0.0);
        }

        let eff_name = Self::dispatch_string(effect, EFF_GET_EFFECT_NAME);
        let prod_name = Self::dispatch_string(effect, EFF_GET_PRODUCT_STRING);
        let vendor = Self::dispatch_string(effect, EFF_GET_VENDOR_STRING);
        let fallback_name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("VST2 Plugin")
            .to_string();

        let name = if !eff_name.is_empty() {
            eff_name
        } else if !prod_name.is_empty() {
            prod_name
        } else {
            fallback_name
        };

        let (num_in, num_out, flags) = unsafe {
            (
                (*effect).num_inputs.clamp(0, MAX_VST2_CHANNELS as i32) as usize,
                (*effect).num_outputs.clamp(1, MAX_VST2_CHANNELS as i32) as usize,
                (*effect).flags,
            )
        };
        let has_gui = (flags & EFF_FLAGS_HAS_EDITOR) != 0;

        let info = PluginInfo {
            path: path.clone(),
            name,
            vendor: if vendor.is_empty() {
                "VST2 Plugin".into()
            } else {
                vendor
            },
            version: "2.4".into(),
            category: "VST2 Fx".into(),
            uid: format!("vst2:{}", path.display()),
            audio_inputs: if num_in > 0 { 1 } else { 0 },
            audio_outputs: if num_out > 0 { 1 } else { 0 },
            has_midi_input: false,
            has_midi_output: false,
            has_gui,
        };

        let in_count = num_in.max(2);
        let out_count = num_out.max(2);
        let in_scratch = vec![vec![0.0_f32; MAX_SCRATCH_FRAMES]; in_count];
        let out_scratch = vec![vec![0.0_f32; MAX_SCRATCH_FRAMES]; out_count];
        let in_ptrs = vec![ptr::null_mut(); in_count];
        let out_ptrs = vec![ptr::null_mut(); out_count];

        Ok(Self {
            effect,
            _library: library,
            info,
            in_channels: num_in.min(2),
            out_channels: num_out.min(2).max(1),
            in_scratch,
            out_scratch,
            in_ptrs,
            out_ptrs,
            sample_rate,
            #[cfg(target_os = "linux")]
            x11_window: None,
        })
    }

    fn dispatch_string(effect: *mut AEffect, opcode: i32) -> String {
        let mut buf = [0 as c_char; 128];
        unsafe {
            let disp = (*effect).dispatcher;
            disp(
                effect,
                opcode,
                0,
                0,
                buf.as_mut_ptr() as *mut c_void,
                0.0,
            );
        }
        buf[127] = 0;
        read_c_buf(&buf)
    }

    pub fn info(&self) -> &PluginInfo {
        &self.info
    }

    pub fn input_channels(&self) -> usize {
        self.in_channels
    }

    pub fn output_channels(&self) -> usize {
        self.out_channels
    }

    pub fn get_parameters(&self) -> Vec<ParameterDisplay> {
        let count = unsafe { (*self.effect).num_params.clamp(0, 2048) as u32 };
        let mut out = Vec::with_capacity(count as usize);
        for id in 0..count {
            let mut name_buf = [0 as c_char; 128];
            let mut unit_buf = [0 as c_char; 128];
            unsafe {
                let disp = (*self.effect).dispatcher;
                disp(
                    self.effect,
                    EFF_GET_PARAM_NAME,
                    id as i32,
                    0,
                    name_buf.as_mut_ptr() as *mut c_void,
                    0.0,
                );
                disp(
                    self.effect,
                    EFF_GET_PARAM_LABEL,
                    id as i32,
                    0,
                    unit_buf.as_mut_ptr() as *mut c_void,
                    0.0,
                );
            }
            name_buf[127] = 0;
            unit_buf[127] = 0;
            let mut name = read_c_buf(&name_buf);
            if name.is_empty() {
                name = format!("Param {}", id + 1);
            }
            let unit = read_c_buf(&unit_buf);
            let value = self.get_parameter_raw(id) as f64;
            let formatted = self.format_parameter(id, value);

            out.push(ParameterDisplay {
                param: Parameter {
                    id,
                    name,
                    value,
                    min: 0.0,
                    max: 1.0,
                    default: value,
                    unit,
                    step_count: 0,
                    can_automate: true,
                    is_read_only: false,
                    is_bypass: false,
                    flags: 1,
                },
                formatted,
            });
        }
        out
    }

    fn get_parameter_raw(&self, id: u32) -> f32 {
        unsafe {
            if let Some(get_fn) = (*self.effect).get_parameter {
                get_fn(self.effect, id as i32).clamp(0.0, 1.0)
            } else {
                0.0
            }
        }
    }

    pub fn set_parameter(&mut self, id: u32, normalized_value: f64) -> String {
        let val = normalized_value.clamp(0.0, 1.0) as f32;
        unsafe {
            if let Some(set_fn) = (*self.effect).set_parameter {
                set_fn(self.effect, id as i32, val);
            }
        }
        self.format_parameter(id, val as f64)
    }

    pub fn format_parameter(&self, id: u32, value: f64) -> String {
        let mut disp_buf = [0 as c_char; 128];
        let mut unit_buf = [0 as c_char; 128];
        unsafe {
            let disp = (*self.effect).dispatcher;
            disp(
                self.effect,
                EFF_GET_PARAM_DISPLAY,
                id as i32,
                0,
                disp_buf.as_mut_ptr() as *mut c_void,
                0.0,
            );
            disp(
                self.effect,
                EFF_GET_PARAM_LABEL,
                id as i32,
                0,
                unit_buf.as_mut_ptr() as *mut c_void,
                0.0,
            );
        }
        disp_buf[127] = 0;
        unit_buf[127] = 0;
        let disp_str = read_c_buf(&disp_buf);
        let unit_str = read_c_buf(&unit_buf);
        let base = if disp_str.is_empty() {
            format!("{value:.2}")
        } else {
            disp_str
        };
        if unit_str.is_empty() {
            base
        } else {
            format!("{base} {unit_str}")
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
        if (sample_rate - self.sample_rate).abs() > 1.0 && sample_rate > 0.0 {
            self.sample_rate = sample_rate;
            unsafe {
                let disp = (*self.effect).dispatcher;
                disp(
                    self.effect,
                    EFF_SET_SAMPLE_RATE,
                    0,
                    0,
                    ptr::null_mut(),
                    sample_rate as f32,
                );
            }
        }

        let mut offset = 0;
        while offset < frames {
            let chunk = (frames - offset).min(MAX_SCRATCH_FRAMES);
            if self.in_channels == 1 {
                for i in 0..chunk {
                    self.in_scratch[0][i] = (in_l[offset + i] + in_r[offset + i]) * 0.5;
                }
            } else {
                self.in_scratch[0][..chunk].copy_from_slice(&in_l[offset..offset + chunk]);
                self.in_scratch[1][..chunk].copy_from_slice(&in_r[offset..offset + chunk]);
            }
            for ch in self.in_scratch.iter_mut().skip(2) {
                ch[..chunk].fill(0.0);
            }
            for ch in self.out_scratch.iter_mut() {
                ch[..chunk].fill(0.0);
            }

            for (idx, ch) in self.in_scratch.iter_mut().enumerate() {
                self.in_ptrs[idx] = ch.as_mut_ptr();
            }
            for (idx, ch) in self.out_scratch.iter_mut().enumerate() {
                self.out_ptrs[idx] = ch.as_mut_ptr();
            }

            unsafe {
                if let Some(proc_rep) = (*self.effect).process_replacing {
                    proc_rep(
                        self.effect,
                        self.in_ptrs.as_mut_ptr(),
                        self.out_ptrs.as_mut_ptr(),
                        chunk as i32,
                    );
                } else if let Some(proc_acc) = (*self.effect).process {
                    proc_acc(
                        self.effect,
                        self.in_ptrs.as_mut_ptr(),
                        self.out_ptrs.as_mut_ptr(),
                        chunk as i32,
                    );
                } else {
                    return false;
                }
            }

            out_l[offset..offset + chunk].copy_from_slice(&self.out_scratch[0][..chunk]);
            if self.out_channels >= 2 {
                out_r[offset..offset + chunk].copy_from_slice(&self.out_scratch[1][..chunk]);
            } else {
                out_r[offset..offset + chunk].copy_from_slice(&self.out_scratch[0][..chunk]);
            }

            offset += chunk;
        }

        if let Ok(mut t) = GLOBAL_TIME_INFO.try_lock() {
            t.sample_pos += frames as f64;
            t.sample_rate = sample_rate;
        }
        true
    }

    pub fn open_editor_window(&mut self) -> Result<(), String> {
        if !self.info.has_gui {
            return Err("This VST2 plugin does not provide a native GUI editor".into());
        }
        #[cfg(target_os = "linux")]
        {
            if self.x11_window.is_some() {
                return Ok(());
            }
            let (mut w, mut h) = self.query_editor_size().unwrap_or((640, 420));
            let title = format!("Chronomix [VST2] - {}", self.info.name);
            let win = X11EditorWindow::open(&title, w, h)?;
            unsafe {
                let disp = (*self.effect).dispatcher;
                disp(
                    self.effect,
                    EFF_EDIT_OPEN,
                    0,
                    0,
                    win.window as usize as *mut c_void,
                    0.0,
                );
            }
            if let Some((pw, ph)) = self.query_editor_size() {
                if pw != w || ph != h {
                    w = pw;
                    h = ph;
                    win.resize(w, h);
                }
            }
            self.x11_window = Some(win);
            Ok(())
        }
        #[cfg(not(target_os = "linux"))]
        {
            Err("Native VST2 GUI editor window is currently supported on Linux".into())
        }
    }

    pub fn close_editor_window(&mut self) {
        #[cfg(target_os = "linux")]
        if let Some(win) = self.x11_window.take() {
            unsafe {
                let disp = (*self.effect).dispatcher;
                disp(self.effect, EFF_EDIT_CLOSE, 0, 0, ptr::null_mut(), 0.0);
            }
            drop(win);
        }
    }

    pub fn is_editor_window_open(&self) -> bool {
        #[cfg(target_os = "linux")]
        {
            self.x11_window.is_some()
        }
        #[cfg(not(target_os = "linux"))]
        {
            false
        }
    }

    pub fn service_editor_window(&mut self) {
        #[cfg(target_os = "linux")]
        {
            let mut close_requested = false;
            if let Some(ref mut win) = self.x11_window {
                if win.pump_events() {
                    close_requested = true;
                } else {
                    unsafe {
                        let disp = (*self.effect).dispatcher;
                        disp(self.effect, EFF_EDIT_IDLE, 0, 0, ptr::null_mut(), 0.0);
                    }
                }
            }
            if close_requested {
                self.close_editor_window();
            }
        }
    }

    fn query_editor_size(&self) -> Option<(u32, u32)> {
        let mut erect_ptr: *mut ERect = ptr::null_mut();
        unsafe {
            let disp = (*self.effect).dispatcher;
            disp(
                self.effect,
                EFF_EDIT_GET_RECT,
                0,
                0,
                ptr::addr_of_mut!(erect_ptr) as *mut c_void,
                0.0,
            );
            if erect_ptr.is_null() {
                return None;
            }
            let r = *erect_ptr;
            let w = (r.right - r.left).max(200) as u32;
            let h = (r.bottom - r.top).max(150) as u32;
            Some((w, h))
        }
    }
}

impl Drop for NativeVst2Plugin {
    fn drop(&mut self) {
        self.close_editor_window();
        if !self.effect.is_null() {
            unsafe {
                let disp = (*self.effect).dispatcher;
                disp(self.effect, EFF_STOP_PROCESS, 0, 0, ptr::null_mut(), 0.0);
                disp(self.effect, EFF_MAINS_CHANGED, 0, 0, ptr::null_mut(), 0.0);
                disp(self.effect, EFF_CLOSE, 0, 0, ptr::null_mut(), 0.0);
            }
        }
    }
}

#[cfg(target_os = "linux")]
struct X11EditorWindow {
    xlib: x11_dl::xlib::Xlib,
    display: *mut x11_dl::xlib::Display,
    window: x11_dl::xlib::Window,
    wm_delete_window: x11_dl::xlib::Atom,
}

#[cfg(target_os = "linux")]
impl X11EditorWindow {
    fn open(title: &str, width: u32, height: u32) -> Result<Self, String> {
        let xlib = x11_dl::xlib::Xlib::open()
            .map_err(|e| format!("Failed to load libX11 for VST2 editor: {e}"))?;
        unsafe {
            let display = (xlib.XOpenDisplay)(ptr::null());
            if display.is_null() {
                return Err("XOpenDisplay failed (ensure DISPLAY is active)".into());
            }
            let screen = (xlib.XDefaultScreen)(display);
            let root = (xlib.XRootWindow)(display, screen);
            let black = (xlib.XBlackPixel)(display, screen);
            let window = (xlib.XCreateSimpleWindow)(
                display,
                root,
                100,
                100,
                width.max(200),
                height.max(150),
                0,
                black,
                black,
            );
            if window == 0 {
                (xlib.XCloseDisplay)(display);
                return Err("XCreateSimpleWindow failed".into());
            }

            let c_title = std::ffi::CString::new(title).unwrap_or_default();
            (xlib.XStoreName)(display, window, c_title.as_ptr());

            let mut wm_delete = (xlib.XInternAtom)(
                display,
                b"WM_DELETE_WINDOW\0".as_ptr() as *const c_char,
                0,
            );
            (xlib.XSetWMProtocols)(display, window, &mut wm_delete, 1);
            (xlib.XSelectInput)(
                display,
                window,
                x11_dl::xlib::ExposureMask | x11_dl::xlib::StructureNotifyMask,
            );
            (xlib.XMapRaised)(display, window);
            (xlib.XFlush)(display);

            Ok(Self {
                xlib,
                display,
                window,
                wm_delete_window: wm_delete,
            })
        }
    }

    fn resize(&self, width: u32, height: u32) {
        unsafe {
            (self.xlib.XResizeWindow)(self.display, self.window, width.max(200), height.max(150));
            (self.xlib.XFlush)(self.display);
        }
    }

    /// Returns `true` if the user closed the window.
    fn pump_events(&mut self) -> bool {
        unsafe {
            while (self.xlib.XPending)(self.display) > 0 {
                let mut event: x11_dl::xlib::XEvent = std::mem::zeroed();
                (self.xlib.XNextEvent)(self.display, &mut event);
                if event.get_type() == x11_dl::xlib::ClientMessage {
                    let xclient = x11_dl::xlib::XClientMessageEvent::from(event);
                    if xclient.data.get_long(0) as x11_dl::xlib::Atom == self.wm_delete_window {
                        return true;
                    }
                }
            }
        }
        false
    }
}

#[cfg(target_os = "linux")]
impl Drop for X11EditorWindow {
    fn drop(&mut self) {
        unsafe {
            if self.window != 0 {
                (self.xlib.XDestroyWindow)(self.display, self.window);
            }
            if !self.display.is_null() {
                (self.xlib.XCloseDisplay)(self.display);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static mut MOCK_GAIN: f32 = 0.5;

    extern "C" fn mock_dispatcher(
        _effect: *mut AEffect,
        opcode: i32,
        index: i32,
        _value: isize,
        ptr: *mut c_void,
        _opt: f32,
    ) -> isize {
        match opcode {
            EFF_GET_EFFECT_NAME => {
                write_c_str(ptr as *mut c_char, "ChronomixMockVst2", 64);
                1
            }
            EFF_GET_VENDOR_STRING => {
                write_c_str(ptr as *mut c_char, "Chronomix", 64);
                1
            }
            EFF_GET_PARAM_NAME if index == 0 => {
                write_c_str(ptr as *mut c_char, "Drive", 64);
                1
            }
            EFF_GET_PARAM_DISPLAY if index == 0 => {
                let v = unsafe { MOCK_GAIN };
                write_c_str(ptr as *mut c_char, &format!("{v:.2}"), 64);
                1
            }
            EFF_GET_PARAM_LABEL if index == 0 => {
                write_c_str(ptr as *mut c_char, "x", 64);
                1
            }
            _ => 0,
        }
    }

    extern "C" fn mock_set_param(_effect: *mut AEffect, index: i32, val: f32) {
        if index == 0 {
            unsafe {
                MOCK_GAIN = val;
            }
        }
    }

    extern "C" fn mock_get_param(_effect: *mut AEffect, index: i32) -> f32 {
        if index == 0 {
            unsafe { MOCK_GAIN }
        } else {
            0.0
        }
    }

    extern "C" fn mock_process_replacing(
        _effect: *mut AEffect,
        inputs: *mut *mut f32,
        outputs: *mut *mut f32,
        sample_frames: i32,
    ) {
        unsafe {
            let gain = MOCK_GAIN;
            let in_l = *inputs.add(0);
            let in_r = *inputs.add(1);
            let out_l = *outputs.add(0);
            let out_r = *outputs.add(1);
            for i in 0..(sample_frames as usize) {
                *out_l.add(i) = *in_l.add(i) * gain;
                *out_r.add(i) = *in_r.add(i) * gain;
            }
        }
    }

    #[test]
    fn native_vst2_processes_audio_and_parameters() {
        let mut raw_effect = Box::new(AEffect {
            magic: VST2_MAGIC,
            dispatcher: mock_dispatcher,
            process: None,
            set_parameter: Some(mock_set_param),
            get_parameter: Some(mock_get_param),
            num_programs: 1,
            num_params: 1,
            num_inputs: 2,
            num_outputs: 2,
            flags: EFF_FLAGS_HAS_EDITOR,
            resvd1: 0,
            resvd2: 0,
            initial_delay: 0,
            real_qualities: 0,
            off_qualities: 0,
            io_ratio: 1.0,
            object: ptr::null_mut(),
            user: ptr::null_mut(),
            unique_id: 0x4348_524D,
            version: 2400,
            process_replacing: Some(mock_process_replacing),
            process_double_replacing: ptr::null_mut(),
            future: [0; 56],
        });

        let ptr_eff: *mut AEffect = &mut *raw_effect;
        let mut plugin = NativeVst2Plugin::from_raw_effect(
            ptr_eff,
            None,
            PathBuf::from("/tmp/ChronomixMockVst2.so"),
            48_000.0,
            256,
        )
        .unwrap();

        assert_eq!(plugin.info().name, "ChronomixMockVst2");
        assert_eq!(plugin.info().category, "VST2 Fx");

        let params = plugin.get_parameters();
        assert_eq!(params.len(), 1);
        assert_eq!(params[0].param.name, "Drive");

        let formatted = plugin.set_parameter(0, 0.25);
        assert_eq!(formatted, "0.25 x");

        let in_l = [1.0_f32, -0.5, 0.4, 0.2];
        let in_r = [0.8_f32, 0.4, -0.2, -0.6];
        let mut out_l = [0.0_f32; 4];
        let mut out_r = [0.0_f32; 4];

        assert!(plugin.process_stereo(&in_l, &in_r, &mut out_l, &mut out_r, 4, 48_000.0));
        assert!((out_l[0] - 0.25).abs() < 1e-5);
        assert!((out_l[1] + 0.125).abs() < 1e-5);
        assert!((out_r[0] - 0.20).abs() < 1e-5);
        assert!((out_r[3] + 0.15).abs() < 1e-5);
    }
}
