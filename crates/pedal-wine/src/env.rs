use std::{
    collections::HashSet,
    env, fs,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

use serde::{Deserialize, Serialize};

use crate::WineError;

const WINE_HOST_CPP: &str = include_str!("../cpp/wine_host.cpp");

/// Information about a discovered Wine executable on the host system.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WineRunnerInfo {
    pub path: PathBuf,
    pub label: String,
    pub version: String,
    pub is_default: bool,
}

/// Classification of where a Wine prefix originated.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WinePrefixKind {
    ChronomixDefault,
    SystemDefault,
    EnvOverride,
    Bottles,
    Lutris,
    Heroic,
    Custom,
}

/// Discovered Wine prefix (`WINEPREFIX`) metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WinePrefixInfo {
    pub path: PathBuf,
    pub name: String,
    pub kind: WinePrefixKind,
    pub initialized: bool,
    pub wayland_x11_configured: bool,
    pub vst_dir_count: usize,
}

/// Status of the Winelib bridge used to host Windows VST2/VST3 plugins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WineBridgeMode {
    EmbeddedPrebuilt,
    RuntimeCached,
    CanCompileAtRuntime,
    Unavailable,
}

/// Standard Wine utility/tool launched from within Chronomix.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WineAction {
    Winecfg,
    Explorer,
    Regedit,
    Taskmgr,
    Control,
    Uninstaller,
    Cmd,
    Winetricks,
    KillServer,
}

/// Persistent Wine runner and prefix configuration used by Chronomix.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WineConfig {
    /// Custom `wine` / `wine64` binary path (`None` = auto-detect).
    pub wine_binary: Option<PathBuf>,
    /// Custom `WINEPREFIX` directory (`None` = auto-select `$WINEPREFIX` or `~/.wine`).
    pub wine_prefix: Option<PathBuf>,
    /// Automatically configure display vars (`DISPLAY` / `WAYLAND_DISPLAY`) and `x11,wayland` driver.
    #[serde(default = "default_true")]
    pub auto_display: bool,
}

impl Default for WineConfig {
    fn default() -> Self {
        Self {
            wine_binary: None,
            wine_prefix: None,
            auto_display: true,
        }
    }
}

fn default_true() -> bool {
    true
}

/// Snapshot of the host Wine environment, discovered prefixes, runners, and bridge health.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WineEnvironmentStatus {
    pub active_wine_binary: Option<PathBuf>,
    pub active_wine_version: Option<String>,
    pub active_prefix: Option<PathBuf>,
    pub active_prefix_initialized: bool,
    pub wayland_x11_configured: bool,
    pub display_session: String,
    pub available_runners: Vec<WineRunnerInfo>,
    pub available_prefixes: Vec<WinePrefixInfo>,
    pub vst_directories: Vec<PathBuf>,
    pub bridge_mode: WineBridgeMode,
    pub winetricks_available: bool,
    pub winegpp_path: Option<PathBuf>,
}

/// Detected Linux display server sockets (`DISPLAY` / `WAYLAND_DISPLAY`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DisplayEnvResolution {
    pub session_summary: String,
    pub display: Option<String>,
    pub wayland_display: Option<String>,
}

fn user_home_dir() -> Option<PathBuf> {
    env::var_os("HOME").filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn xdg_data_dir() -> Option<PathBuf> {
    env::var_os("XDG_DATA_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home_dir().map(|h| h.join(".local/share")))
}

fn xdg_cache_dir() -> PathBuf {
    env::var_os("XDG_CACHE_HOME")
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
        .or_else(|| user_home_dir().map(|h| h.join(".cache")))
        .unwrap_or_else(env::temp_dir)
}

/// Chronomix's isolated default Wine prefix (`~/.local/share/chronomix/wine`).
pub fn default_chronomix_wine_prefix() -> Option<PathBuf> {
    xdg_data_dir().map(|d| d.join("chronomix/wine"))
}

/// Standard system Wine prefix (`~/.wine`).
pub fn system_default_wine_prefix() -> Option<PathBuf> {
    user_home_dir().map(|h| h.join(".wine"))
}

/// Returns a short directory suitable for `AF_UNIX` domain sockets (stays well below the 108-byte `sun_path` limit).
pub fn short_ipc_dir() -> PathBuf {
    if let Some(runtime_dir) = env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
        let candidate = PathBuf::from(runtime_dir).join("cxw");
        if candidate.to_string_lossy().len() <= 72 && ensure_private_dir(&candidate).is_ok() {
            return candidate;
        }
    }

    #[cfg(target_os = "linux")]
    let uid = unsafe { libc::geteuid() };
    #[cfg(not(target_os = "linux"))]
    let uid = 0_u32;

    let fallback = PathBuf::from(format!("/tmp/cxw-{uid}"));
    let _ = ensure_private_dir(&fallback);
    fallback
}

fn ensure_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir_all(dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if let Ok(meta) = fs::metadata(dir) {
            let mut perms = meta.permissions();
            perms.set_mode(0o700);
            let _ = fs::set_permissions(dir, perms);
        }
    }
    Ok(())
}

/// Standard VST2 and VST3 install directories inside a given Wine prefix.
pub fn standard_prefix_vst_dirs(prefix: &Path) -> Vec<PathBuf> {
    let drive_c = prefix.join("drive_c");
    let mut raw = vec![
        drive_c.join("Program Files/Common Files/VST3"),
        drive_c.join("Program Files/Common Files/VST2"),
        drive_c.join("Program Files/VSTPlugins"),
        drive_c.join("Program Files/VstPlugins"),
        drive_c.join("Program Files/Steinberg/VstPlugins"),
        drive_c.join("Program Files/Steinberg/VSTPlugins"),
        drive_c.join("Program Files (x86)/Common Files/VST3"),
        drive_c.join("Program Files (x86)/VSTPlugins"),
        drive_c.join("Program Files (x86)/Steinberg/VstPlugins"),
    ];

    let users_dir = drive_c.join("users");
    if let Ok(entries) = fs::read_dir(&users_dir) {
        let mut user_paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|entry| entry.path())).collect();
        user_paths.sort();
        for user_dir in user_paths {
            if !user_dir.is_dir() {
                continue;
            }
            let Some(name) = user_dir.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            if name.starts_with('.') || name.eq_ignore_ascii_case("All Users") {
                continue;
            }
            raw.push(user_dir.join("AppData/Local/Programs/Common/VST3"));
            raw.push(user_dir.join("AppData/Local/VST3"));
            raw.push(user_dir.join("AppData/Roaming/VST3"));
            raw.push(user_dir.join("Documents/VST3"));
        }
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for dir in raw {
        let key = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
        if seen.insert(key) {
            out.push(dir);
        }
    }
    out
}

fn is_executable_file(path: &Path) -> bool {
    let Ok(meta) = fs::metadata(path) else {
        return false;
    };
    if !meta.is_file() {
        return false;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o111 != 0
    }
    #[cfg(not(unix))]
    {
        true
    }
}

fn find_in_path(binary_name: &str) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let Some(path_env) = env::var_os("PATH") else {
        return found;
    };
    for dir in env::split_paths(&path_env) {
        let candidate = dir.join(binary_name);
        if is_executable_file(&candidate) {
            found.push(candidate);
        }
    }
    found
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if start.elapsed() >= timeout {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(25));
    }
}

fn probe_wine_version(wine_bin: &Path) -> String {
    let mut cmd = Command::new(wine_bin);
    cmd.arg("--version")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    let Ok(mut child) = cmd.spawn() else {
        return String::new();
    };
    if wait_with_timeout(&mut child, Duration::from_millis(1500))
        .ok()
        .flatten()
        .is_none()
    {
        let _ = child.kill();
        let _ = child.wait();
        return String::new();
    }
    let Some(mut stdout) = child.stdout.take() else {
        return String::new();
    };
    let mut buf = String::new();
    let _ = std::io::Read::read_to_string(&mut stdout, &mut buf);
    buf.lines().next().unwrap_or("").trim().to_string()
}

fn runner_source_tag(path: &Path) -> Option<&'static str> {
    let s = path.to_string_lossy();
    if s.contains("/lutris/") {
        Some("Lutris")
    } else if s.contains("/bottles/") {
        Some("Bottles")
    } else if s.contains("/heroic/") {
        Some("Heroic")
    } else if s.contains("/compatibilitytools.d/") {
        Some("Proton/GE")
    } else if s.contains("/opt/wine-staging") {
        Some("Wine Staging")
    } else if s.contains("/opt/wine-devel") {
        Some("Wine Devel")
    } else if s.contains("/opt/wine-stable") {
        Some("Wine Stable")
    } else {
        None
    }
}

fn format_runner_label(path: &Path, version: &str) -> String {
    let tag = runner_source_tag(path);
    let path_disp = path.display();
    match (version.is_empty(), tag) {
        (false, Some(t)) => format!("{version} [{t}] — {path_disp}"),
        (false, None) => format!("{version} — {path_disp}"),
        (true, Some(t)) => format!("{t} — {path_disp}"),
        (true, None) => format!("{path_disp}"),
    }
}

fn collect_subdir_runners(base_dirs: &[PathBuf], rel_bins: &[&str], out: &mut Vec<PathBuf>) {
    for base in base_dirs {
        let Ok(entries) = fs::read_dir(base) else {
            continue;
        };
        let mut children: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|d| d.path())).collect();
        children.sort();
        for child in children {
            if !child.is_dir() {
                continue;
            }
            for rel in rel_bins {
                let bin = child.join(rel);
                if is_executable_file(&bin) {
                    out.push(bin);
                }
            }
        }
    }
}

/// Discovers installed `wine` / `wine64` executables across system paths and Wine managers.
pub fn discover_wine_binaries() -> Vec<WineRunnerInfo> {
    let mut candidates = Vec::new();
    if let Some(env_bin) = env::var_os("CHRONOMIX_WINE").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(env_bin);
        if is_executable_file(&p) {
            candidates.push(p);
        }
    }

    candidates.extend(find_in_path("wine"));
    candidates.extend(find_in_path("wine64"));

    for static_path in [
        "/usr/bin/wine",
        "/usr/bin/wine64",
        "/usr/local/bin/wine",
        "/usr/local/bin/wine64",
        "/opt/wine-staging/bin/wine",
        "/opt/wine-staging/bin/wine64",
        "/opt/wine-devel/bin/wine",
        "/opt/wine-devel/bin/wine64",
        "/opt/wine-stable/bin/wine",
        "/opt/wine-stable/bin/wine64",
        "/app/bin/wine",
    ] {
        let p = PathBuf::from(static_path);
        if is_executable_file(&p) {
            candidates.push(p);
        }
    }

    if let Some(home) = user_home_dir() {
        let wine_manager_roots = [
            home.join(".local/share/lutris/runners/wine"),
            home.join(".local/share/bottles/runners"),
            home.join(".var/app/com.usebottles.bottles/data/bottles/runners"),
            home.join(".config/heroic/tools/wine"),
        ];
        collect_subdir_runners(&wine_manager_roots, &["bin/wine", "bin/wine64"], &mut candidates);

        let proton_roots = [
            home.join(".config/heroic/tools/proton"),
            home.join(".local/share/Steam/compatibilitytools.d"),
        ];
        collect_subdir_runners(
            &proton_roots,
            &["files/bin/wine", "files/bin/wine64", "dist/bin/wine"],
            &mut candidates,
        );
    }

    let mut seen_canon = HashSet::new();
    let mut runners = Vec::new();
    for path in candidates {
        let canon = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if !seen_canon.insert(canon) {
            continue;
        }
        let version = probe_wine_version(&path);
        let label = format_runner_label(&path, &version);
        runners.push(WineRunnerInfo {
            path,
            label,
            version,
            is_default: runners.is_empty(),
        });
    }
    runners
}

/// Fast resolution of the active `wine` binary given user config and environment overrides.
pub fn resolve_wine_binary(config: &WineConfig) -> Option<PathBuf> {
    if let Some(custom) = &config.wine_binary {
        if is_executable_file(custom) {
            return Some(custom.clone());
        }
    }
    if let Some(env_bin) = env::var_os("CHRONOMIX_WINE").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(env_bin);
        if is_executable_file(&p) {
            return Some(p);
        }
    }
    for name in ["wine", "wine64"] {
        if let Some(first) = find_in_path(name).into_iter().next() {
            return Some(first);
        }
    }
    for static_path in [
        "/usr/bin/wine",
        "/usr/bin/wine64",
        "/usr/local/bin/wine",
        "/usr/local/bin/wine64",
        "/opt/wine-staging/bin/wine",
        "/opt/wine-devel/bin/wine",
        "/opt/wine-stable/bin/wine",
    ] {
        let p = PathBuf::from(static_path);
        if is_executable_file(&p) {
            return Some(p);
        }
    }
    discover_wine_binaries().into_iter().next().map(|r| r.path)
}

pub fn find_wineserver(preferred_wine: Option<&Path>) -> Option<PathBuf> {
    if let Some(wine_bin) = preferred_wine {
        if let Some(parent) = wine_bin.parent() {
            let sibling = parent.join("wineserver");
            if is_executable_file(&sibling) {
                return Some(sibling);
            }
        }
    }
    if let Some(first) = find_in_path("wineserver").into_iter().next() {
        return Some(first);
    }
    for static_path in [
        "/usr/bin/wineserver",
        "/usr/local/bin/wineserver",
        "/opt/wine-staging/bin/wineserver",
        "/opt/wine-devel/bin/wineserver",
        "/opt/wine-stable/bin/wineserver",
    ] {
        let p = PathBuf::from(static_path);
        if is_executable_file(&p) {
            return Some(p);
        }
    }
    None
}

pub fn find_winegpp(preferred_wine: Option<&Path>) -> Option<PathBuf> {
    if let Some(wine_bin) = preferred_wine {
        if let Some(parent) = wine_bin.parent() {
            let sibling = parent.join("wineg++");
            if is_executable_file(&sibling) {
                return Some(sibling);
            }
        }
    }
    if let Some(env_bin) = env::var_os("CHRONOMIX_WINEGPP").filter(|v| !v.is_empty()) {
        let p = PathBuf::from(env_bin);
        if is_executable_file(&p) {
            return Some(p);
        }
    }
    if let Some(first) = find_in_path("wineg++").into_iter().next() {
        return Some(first);
    }
    for static_path in [
        "/usr/bin/wineg++",
        "/usr/local/bin/wineg++",
        "/opt/wine-staging/bin/wineg++",
        "/opt/wine-devel/bin/wineg++",
        "/opt/wine-stable/bin/wineg++",
    ] {
        let p = PathBuf::from(static_path);
        if is_executable_file(&p) {
            return Some(p);
        }
    }
    None
}

pub fn find_winetricks() -> Option<PathBuf> {
    if let Some(first) = find_in_path("winetricks").into_iter().next() {
        return Some(first);
    }
    for static_path in ["/usr/bin/winetricks", "/usr/local/bin/winetricks"] {
        let p = PathBuf::from(static_path);
        if is_executable_file(&p) {
            return Some(p);
        }
    }
    None
}

/// Checks whether a Wine prefix has `drive_c` and `system.reg`.
pub fn is_prefix_initialized(prefix: &Path) -> bool {
    prefix.join("drive_c").is_dir() && prefix.join("system.reg").is_file()
}

/// Checks whether `user.reg` configures Wine's `Graphics` driver to support both X11 and Wayland.
pub fn user_reg_has_wayland_and_x11(user_reg: &Path) -> bool {
    let Ok(bytes) = fs::read(user_reg) else {
        return false;
    };
    let text = String::from_utf8_lossy(&bytes);
    let Some(val) = read_graphics_driver_in_reg(&text) else {
        return false;
    };
    let tokens: Vec<String> = val
        .split(',')
        .map(|s| s.trim().to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    tokens.iter().any(|t| t == "x11") && tokens.iter().any(|t| t == "wayland")
}

fn is_drivers_section_header(line: &str) -> bool {
    let trimmed = line.trim();
    if !trimmed.starts_with('[') {
        return false;
    }
    let Some(end_idx) = trimmed.find(']') else {
        return false;
    };
    trimmed[1..end_idx].eq_ignore_ascii_case("Software\\\\Wine\\\\Drivers")
}

fn read_graphics_driver_in_reg(content: &str) -> Option<String> {
    let mut in_section = false;
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            in_section = is_drivers_section_header(trimmed);
            continue;
        }
        if !in_section {
            continue;
        }
        if let Some((key, val)) = trimmed.split_once('=') {
            if key.trim().eq_ignore_ascii_case("\"Graphics\"") {
                return Some(val.trim().trim_matches('"').to_string());
            }
        }
    }
    None
}

/// Updates or inserts `HKCU\Software\Wine\Drivers` `"Graphics"="x11,wayland"` inside `user.reg` text.
/// Returns `Some(updated_text)` if modified, or `None` if already configured cleanly.
pub fn upsert_graphics_driver_in_reg(content: &str) -> Option<String> {
    if let Some(existing) = read_graphics_driver_in_reg(content) {
        let tokens: Vec<String> = existing
            .split(',')
            .map(|s| s.trim().to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect();
        if tokens.iter().any(|t| t == "x11") && tokens.iter().any(|t| t == "wayland") {
            return None;
        }
    }

    let trailing_newline = content.ends_with('\n');
    let mut lines: Vec<String> = content.lines().map(ToString::to_string).collect();
    let mut in_section = false;
    let mut section_header_idx: Option<usize> = None;
    let mut section_time_idx: Option<usize> = None;
    let mut graphics_idx: Option<usize> = None;

    for (idx, line) in lines.iter().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            if is_drivers_section_header(trimmed) {
                in_section = true;
                section_header_idx = Some(idx);
            } else if in_section {
                break;
            }
            continue;
        }
        if !in_section {
            continue;
        }
        if trimmed.starts_with("#time=") && section_time_idx.is_none() {
            section_time_idx = Some(idx);
        } else if let Some((key, _)) = trimmed.split_once('=') {
            if key.trim().eq_ignore_ascii_case("\"Graphics\"") {
                graphics_idx = Some(idx);
                break;
            }
        }
    }

    const DESIRED_LINE: &str = "\"Graphics\"=\"x11,wayland\"";
    if let Some(g_idx) = graphics_idx {
        lines[g_idx] = DESIRED_LINE.to_string();
    } else if let Some(h_idx) = section_header_idx {
        let insert_pos = section_time_idx.unwrap_or(h_idx) + 1;
        lines.insert(insert_pos, DESIRED_LINE.to_string());
    } else {
        if !lines.is_empty() && !lines.last().is_some_and(String::is_empty) {
            lines.push(String::new());
        }
        lines.push("[Software\\\\Wine\\\\Drivers]".to_string());
        lines.push(DESIRED_LINE.to_string());
    }

    let mut out = lines.join("\n");
    if trailing_newline || !out.ends_with('\n') {
        out.push('\n');
    }
    Some(out)
}

/// Ensures `user.reg` inside `prefix` enables both `x11` and `wayland` graphics drivers.
pub fn ensure_user_reg_graphics_driver(prefix: &Path) -> std::io::Result<bool> {
    let user_reg = prefix.join("user.reg");
    if !user_reg.is_file() {
        return Ok(false);
    }
    let bytes = fs::read(&user_reg)?;
    let content = String::from_utf8_lossy(&bytes);
    if let Some(updated) = upsert_graphics_driver_in_reg(&content) {
        fs::write(&user_reg, updated.as_bytes())?;
        Ok(true)
    } else {
        Ok(false)
    }
}

/// Detects active Wayland/X11 sockets from environment variables or `/tmp/.X11-unix` & `$XDG_RUNTIME_DIR`.
pub fn detect_display_env() -> DisplayEnvResolution {
    let mut display = env::var("DISPLAY").ok().filter(|s| !s.trim().is_empty());
    let mut wayland_display = env::var("WAYLAND_DISPLAY")
        .ok()
        .filter(|s| !s.trim().is_empty());

    if display.is_none() {
        if let Ok(entries) = fs::read_dir("/tmp/.X11-unix") {
            let mut x_nums: Vec<u32> = entries
                .filter_map(|e| e.ok())
                .filter_map(|e| e.file_name().to_str().and_then(|s| s.strip_prefix('X')).and_then(|n| n.parse::<u32>().ok()))
                .collect();
            x_nums.sort_unstable();
            if let Some(first) = x_nums.first() {
                display = Some(format!(":{first}"));
            }
        }
    }

    if wayland_display.is_none() {
        if let Some(runtime_dir) = env::var_os("XDG_RUNTIME_DIR").filter(|v| !v.is_empty()) {
            if let Ok(entries) = fs::read_dir(runtime_dir) {
                let mut wl_names: Vec<String> = entries
                    .filter_map(|e| e.ok())
                    .filter_map(|e| {
                        let name = e.file_name().to_string_lossy().into_owned();
                        if name.starts_with("wayland-") && !name.ends_with(".lock") {
                            Some(name)
                        } else {
                            None
                        }
                    })
                    .collect();
                wl_names.sort();
                wayland_display = wl_names.into_iter().next();
            }
        }
    }

    let session_summary = match (&wayland_display, &display) {
        (Some(w), Some(d)) => format!("Wayland ({w}) + X11/XWayland ({d})"),
        (Some(w), None) => format!("Wayland ({w})"),
        (None, Some(d)) => format!("X11 ({d})"),
        (None, None) => "Headless / sin display".to_string(),
    };

    DisplayEnvResolution {
        session_summary,
        display,
        wayland_display,
    }
}

fn build_prefix_info(path: PathBuf, name: String, kind: WinePrefixKind) -> WinePrefixInfo {
    let initialized = is_prefix_initialized(&path);
    let wayland_x11_configured = if initialized {
        user_reg_has_wayland_and_x11(&path.join("user.reg"))
    } else {
        false
    };
    let vst_dir_count = if initialized {
        standard_prefix_vst_dirs(&path)
            .into_iter()
            .filter(|d| d.is_dir())
            .count()
    } else {
        0
    };
    WinePrefixInfo {
        path,
        name,
        kind,
        initialized,
        wayland_x11_configured,
        vst_dir_count,
    }
}

fn collect_prefix_subdirs(
    root: &Path,
    kind: WinePrefixKind,
    out: &mut Vec<(PathBuf, String, WinePrefixKind)>,
) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let mut dirs: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|d| d.path())).collect();
    dirs.sort();
    for dir in dirs {
        if !dir.join("drive_c").is_dir() {
            continue;
        }
        let dir_name = dir
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("prefix")
            .to_string();
        let label = match kind {
            WinePrefixKind::Bottles => format!("Bottles: {dir_name}"),
            WinePrefixKind::Lutris => format!("Lutris: {dir_name}"),
            WinePrefixKind::Heroic => format!("Heroic: {dir_name}"),
            _ => dir_name,
        };
        out.push((dir, label, kind));
    }
}

/// Discovers Wine prefixes (`~/.wine`, Chronomix prefix, Bottles, Lutris, Heroic, `$WINEPREFIX`).
pub fn discover_wine_prefixes() -> Vec<WinePrefixInfo> {
    let mut raw: Vec<(PathBuf, String, WinePrefixKind)> = Vec::new();

    for var in ["CHRONOMIX_WINEPREFIX", "WINEPREFIX"] {
        if let Some(p) = env::var_os(var).filter(|v| !v.is_empty()) {
            let path = PathBuf::from(p);
            let name = format!("${var} ({})", path.display());
            raw.push((path, name, WinePrefixKind::EnvOverride));
        }
    }

    if let Some(sys_prefix) = system_default_wine_prefix() {
        raw.push((
            sys_prefix,
            "Wine estándar (~/.wine)".to_string(),
            WinePrefixKind::SystemDefault,
        ));
    }

    if let Some(cx_prefix) = default_chronomix_wine_prefix() {
        raw.push((
            cx_prefix,
            "Chronomix (~/.local/share/chronomix/wine)".to_string(),
            WinePrefixKind::ChronomixDefault,
        ));
    }

    if let Some(home) = user_home_dir() {
        collect_prefix_subdirs(
            &home.join(".local/share/bottles/bottles"),
            WinePrefixKind::Bottles,
            &mut raw,
        );
        collect_prefix_subdirs(
            &home.join(".var/app/com.usebottles.bottles/data/bottles/bottles"),
            WinePrefixKind::Bottles,
            &mut raw,
        );
        collect_prefix_subdirs(
            &home.join(".local/share/lutris/prefixes"),
            WinePrefixKind::Lutris,
            &mut raw,
        );
        collect_prefix_subdirs(
            &home.join("Games"),
            WinePrefixKind::Lutris,
            &mut raw,
        );
        collect_prefix_subdirs(
            &home.join("Games/Heroic/Prefixes"),
            WinePrefixKind::Heroic,
            &mut raw,
        );
    }

    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (path, name, kind) in raw {
        let key = fs::canonicalize(&path).unwrap_or_else(|_| path.clone());
        if seen.insert(key) {
            out.push(build_prefix_info(path, name, kind));
        }
    }
    out
}

/// Resolves the active `WINEPREFIX` from explicit config, environment, or installed defaults.
pub fn resolve_wine_prefix(config: &WineConfig) -> Option<PathBuf> {
    if let Some(custom) = &config.wine_prefix {
        return Some(custom.clone());
    }
    for var in ["CHRONOMIX_WINEPREFIX", "WINEPREFIX"] {
        if let Some(p) = env::var_os(var).filter(|v| !v.is_empty()) {
            return Some(PathBuf::from(p));
        }
    }
    if let Some(sys_prefix) = system_default_wine_prefix() {
        if is_prefix_initialized(&sys_prefix) {
            return Some(sys_prefix);
        }
    }
    if let Some(cx_prefix) = default_chronomix_wine_prefix() {
        if is_prefix_initialized(&cx_prefix) {
            return Some(cx_prefix);
        }
    }
    for prefix in discover_wine_prefixes() {
        if prefix.initialized {
            return Some(prefix.path);
        }
    }
    system_default_wine_prefix().or_else(default_chronomix_wine_prefix)
}

/// Applies `WINEPREFIX`, `CHRONOMIX_PREFIX`, matching `WINESERVER`, display fallback, and quiet overrides.
pub fn apply_wine_process_env(
    cmd: &mut Command,
    wine_bin: &Path,
    prefix: &Path,
    auto_display: bool,
    headless_or_bridge: bool,
) {
    cmd.env("WINEPREFIX", prefix)
        .env("CHRONOMIX_PREFIX", prefix);

    if let Some(wineserver) = find_wineserver(Some(wine_bin)) {
        cmd.env("WINESERVER", wineserver);
    }

    if auto_display {
        let disp = detect_display_env();
        if env::var_os("DISPLAY").is_none_or(|v| v.is_empty()) {
            if let Some(d) = disp.display {
                cmd.env("DISPLAY", d);
            }
        }
        if env::var_os("WAYLAND_DISPLAY").is_none_or(|v| v.is_empty()) {
            if let Some(w) = disp.wayland_display {
                cmd.env("WAYLAND_DISPLAY", w);
            }
        }
    }

    if headless_or_bridge {
        if env::var_os("WINEDLLOVERRIDES").is_none() {
            cmd.env("WINEDLLOVERRIDES", "winemenubuilder.exe=d");
        }
        if env::var_os("CHRONOMIX_WINE_DEBUG").is_none() {
            cmd.env("WINEDEBUG", "-all");
        }
    } else if env::var_os("CHRONOMIX_WINE_DEBUG").is_none() && env::var_os("WINEDEBUG").is_none() {
        cmd.env("WINEDEBUG", "fixme-all");
    }
}

/// Initializes or repairs a Wine prefix cleanly, creating standard VST dirs and configuring `x11,wayland`.
pub fn initialize_wine_prefix(config: &WineConfig) -> Result<PathBuf, String> {
    let wine_bin = resolve_wine_binary(config)
        .ok_or_else(|| "Wine no está instalado en el sistema (wine/wine64 no encontrado)".to_string())?;
    let prefix = resolve_wine_prefix(config)
        .ok_or_else(|| "No se pudo determinar la carpeta WINEPREFIX (HOME vacío)".to_string())?;

    fs::create_dir_all(&prefix)
        .map_err(|e| format!("No se pudo crear el prefijo Wine {}: {e}", prefix.display()))?;

    let mut boot = Command::new(&wine_bin);
    boot.args(["wineboot", "--init"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_wine_process_env(&mut boot, &wine_bin, &prefix, config.auto_display, true);

    let mut child = boot
        .spawn()
        .map_err(|e| format!("Error al ejecutar wineboot ({}): {e}", wine_bin.display()))?;

    match wait_with_timeout(&mut child, Duration::from_secs(60)) {
        Ok(Some(status)) if status.success() => {}
        Ok(Some(status)) => {
            return Err(format!("wineboot --init finalizó con código {status}"));
        }
        Ok(None) => {
            let _ = child.kill();
            let _ = child.wait();
            return Err("wineboot --init excedió el tiempo límite de 60s".to_string());
        }
        Err(e) => return Err(format!("Fallo esperando a wineboot: {e}")),
    }

    if let Some(wineserver) = find_wineserver(Some(&wine_bin)) {
        let mut wait_srv = Command::new(wineserver);
        wait_srv
            .arg("-w")
            .env("WINEPREFIX", &prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Ok(mut srv_child) = wait_srv.spawn() {
            if wait_with_timeout(&mut srv_child, Duration::from_secs(15))
                .ok()
                .flatten()
                .is_none()
            {
                let _ = srv_child.kill();
                let _ = srv_child.wait();
            }
        }
    }

    if config.auto_display {
        let _ = ensure_user_reg_graphics_driver(&prefix);
        if !user_reg_has_wayland_and_x11(&prefix.join("user.reg")) {
            let mut reg = Command::new(&wine_bin);
            reg.args([
                "reg",
                "add",
                r"HKCU\Software\Wine\Drivers",
                "/v",
                "Graphics",
                "/t",
                "REG_SZ",
                "/d",
                "x11,wayland",
                "/f",
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
            apply_wine_process_env(&mut reg, &wine_bin, &prefix, config.auto_display, true);
            if let Ok(mut reg_child) = reg.spawn() {
                if wait_with_timeout(&mut reg_child, Duration::from_secs(10))
                    .ok()
                    .flatten()
                    .is_none()
                {
                    let _ = reg_child.kill();
                    let _ = reg_child.wait();
                }
            }
            let _ = ensure_user_reg_graphics_driver(&prefix);
        }
    }

    if prefix.join("drive_c").is_dir() {
        let _ = fs::create_dir_all(prefix.join("drive_c/Program Files/Common Files/VST3"));
        let _ = fs::create_dir_all(prefix.join("drive_c/Program Files/VSTPlugins"));
    }

    Ok(prefix)
}

/// Ensures the configured Wine prefix exists and is configured for Wayland/X11 before hosting plugins.
pub fn ensure_prefix_ready_for_plugin(config: &WineConfig) -> Result<(PathBuf, PathBuf), WineError> {
    let wine_bin = resolve_wine_binary(config)
        .ok_or_else(|| WineError::Launch("wine executable not found".into()))?;
    let prefix = resolve_wine_prefix(config)
        .ok_or_else(|| WineError::Launch("unable to resolve WINEPREFIX".into()))?;

    let needs_init = !is_prefix_initialized(&prefix);
    let needs_reg = config.auto_display && !user_reg_has_wayland_and_x11(&prefix.join("user.reg"));

    if needs_init {
        initialize_wine_prefix(config).map_err(WineError::Launch)?;
    } else if needs_reg {
        let _ = ensure_user_reg_graphics_driver(&prefix);
        if !user_reg_has_wayland_and_x11(&prefix.join("user.reg")) {
            let _ = initialize_wine_prefix(config);
        }
    }

    Ok((wine_bin, prefix))
}

fn source_fingerprint() -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in WINE_HOST_CPP.as_bytes() {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{hash:016x}-{}", WINE_HOST_CPP.len())
}

fn runtime_bridge_dir() -> PathBuf {
    xdg_cache_dir().join("chronomix").join("wine-bridge-rt")
}

/// Checks if a runtime-compiled Wine host matching the embedded C++ source is cached on disk.
pub fn cached_runtime_bridge_paths() -> Option<(PathBuf, PathBuf)> {
    let dir = runtime_bridge_dir();
    let stamp = fs::read_to_string(dir.join("wine_host.stamp")).ok()?;
    if stamp.trim() != source_fingerprint() {
        return None;
    }
    let so_path = dir.join("chronomix-wine-host.exe.so");
    if so_path.is_file() {
        Some((so_path, dir))
    } else {
        None
    }
}

/// Compiles `wine_host.cpp` at runtime with `wineg++` into the Chronomix cache directory.
pub fn rebuild_wine_bridge(config: &WineConfig) -> Result<(PathBuf, PathBuf), WineError> {
    let resolved_wine = resolve_wine_binary(config);
    let winegpp = find_winegpp(resolved_wine.as_deref()).ok_or(WineError::BridgeNotBuilt)?;

    let dir = runtime_bridge_dir();
    fs::create_dir_all(&dir)
        .map_err(|e| WineError::Ipc(format!("create wine bridge cache dir: {e}")))?;

    let cpp_path = dir.join("wine_host.cpp");
    fs::write(&cpp_path, WINE_HOST_CPP.as_bytes())
        .map_err(|e| WineError::Ipc(format!("write {}: {e}", cpp_path.display())))?;

    let out_base = dir.join("chronomix-wine-host");
    let output = Command::new(&winegpp)
        .arg("-O2")
        .arg("-std=c++17")
        .arg(&cpp_path)
        .arg("-o")
        .arg(&out_base)
        .arg("-lole32")
        .arg("-lpthread")
        .output()
        .map_err(|e| WineError::Launch(format!("wineg++ {}: {e}", winegpp.display())))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let msg = stderr.trim();
        return Err(WineError::Launch(format!(
            "wineg++ failed ({}): {}",
            output.status,
            if msg.is_empty() { "unknown error" } else { msg }
        )));
    }

    let so_path = dir.join("chronomix-wine-host.exe.so");
    if !so_path.is_file() {
        return Err(WineError::Ipc(format!(
            "wineg++ succeeded but {} was not generated",
            so_path.display()
        )));
    }

    let _ = fs::write(dir.join("wine_host.stamp"), source_fingerprint());
    Ok((so_path, dir))
}

/// Detects how the Wine VST bridge can be provided on this host.
pub fn current_bridge_mode(preferred_wine: Option<&Path>) -> WineBridgeMode {
    if cfg!(target_os = "linux") && cached_runtime_bridge_paths().is_some() {
        WineBridgeMode::RuntimeCached
    } else if cfg!(all(target_os = "linux", has_wine_host)) {
        WineBridgeMode::EmbeddedPrebuilt
    } else if cfg!(target_os = "linux") && find_winegpp(preferred_wine).is_some() {
        WineBridgeMode::CanCompileAtRuntime
    } else {
        WineBridgeMode::Unavailable
    }
}

pub fn wine_bridge_runtime_warning(config: &WineConfig) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let resolved_wine = resolve_wine_binary(config);
    match current_bridge_mode(resolved_wine.as_deref()) {
        WineBridgeMode::Unavailable => Some(
            "Aviso Wine: 'wineg++' no estuvo disponible al compilar ni se detecta en el sistema. Los plugins de Windows (.dll / .vst3) no podrán cargarse hasta instalar Wine con 'wineg++' (paquete wine / wine-devel / wine-staging).".to_string(),
        ),
        _ => None,
    }
}

/// Builds the command invocation for a Windows `.exe`, `.msi`, or `.bat`/`.cmd` file under Wine.
pub fn wine_executable_args(target: &Path) -> Vec<String> {
    let ext = target
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    let target_str = target.to_string_lossy().into_owned();
    match ext.as_str() {
        "msi" => vec!["msiexec".to_string(), "/i".to_string(), target_str],
        "bat" | "cmd" => vec!["cmd".to_string(), "/c".to_string(), target_str],
        _ => vec![target_str],
    }
}

/// Launches a Windows `.exe`, `.msi`, or script inside the configured Wine environment and waits for completion.
pub fn launch_wine_executable(target: &Path, config: &WineConfig) -> Result<String, String> {
    if !target.is_file() {
        return Err(format!("Archivo no encontrado: {}", target.display()));
    }
    let (wine_bin, prefix) = ensure_prefix_ready_for_plugin(config).map_err(|e| e.to_string())?;
    let args = wine_executable_args(target);

    let err_log_path = short_ipc_dir().join(format!(
        "run_{}_{}.log",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    ));

    let err_file = fs::File::create(&err_log_path)
        .map(Stdio::from)
        .unwrap_or_else(|_| Stdio::null());

    let mut cmd = Command::new(&wine_bin);
    cmd.args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(err_file);
    if let Some(parent) = target.parent() {
        if parent.is_dir() {
            cmd.current_dir(parent);
        }
    }
    apply_wine_process_env(&mut cmd, &wine_bin, &prefix, config.auto_display, false);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("No se pudo abrir {} con Wine: {e}", target.display()))?;
    let status = child
        .wait()
        .map_err(|e| format!("Error esperando a {}: {e}", target.display()))?;

    let err_output = fs::read_to_string(&err_log_path).unwrap_or_default();
    let _ = fs::remove_file(&err_log_path);

    let file_name = target
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("app");
    if status.success() {
        Ok(format!("{file_name} cerrado correctamente (Wine prefix {})", prefix.display()))
    } else {
        let last_err = err_output
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("")
            .trim();
        if last_err.is_empty() {
            Err(format!("{file_name} finalizó con estado {status}"))
        } else {
            Err(format!("{file_name} finalizó con estado {status}: {last_err}"))
        }
    }
}

/// Runs a Wine desktop tool (`winecfg`, `explorer`, `winetricks`, `wineserver -k`, etc.).
pub fn launch_wine_tool(action: WineAction, config: &WineConfig) -> Result<String, String> {
    if action == WineAction::KillServer {
        let wine_bin = resolve_wine_binary(config);
        let prefix = resolve_wine_prefix(config)
            .ok_or_else(|| "No hay WINEPREFIX configurado".to_string())?;
        let wineserver = find_wineserver(wine_bin.as_deref())
            .ok_or_else(|| "No se encontró el binario wineserver".to_string())?;
        let status = Command::new(wineserver)
            .arg("-k")
            .env("WINEPREFIX", &prefix)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .map_err(|e| format!("Error al ejecutar wineserver -k: {e}"))?;
        if status.success() {
            return Ok(format!("wineserver detenido en {}", prefix.display()));
        }
        return Ok(format!("wineserver -k ejecutado ({status}) en {}", prefix.display()));
    }

    let (wine_bin, prefix) = ensure_prefix_ready_for_plugin(config).map_err(|e| e.to_string())?;

    if action == WineAction::Winetricks {
        let wt = find_winetricks()
            .ok_or_else(|| "winetricks no está instalado en el sistema".to_string())?;
        let mut cmd = Command::new(&wt);
        cmd.arg("--gui")
            .env("WINE", &wine_bin)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        apply_wine_process_env(&mut cmd, &wine_bin, &prefix, config.auto_display, false);
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("No se pudo abrir winetricks: {e}"))?;
        let status = child
            .wait()
            .map_err(|e| format!("Error esperando winetricks: {e}"))?;
        if status.success() {
            return Ok(format!("winetricks cerrado (prefijo {})", prefix.display()));
        }
        return Err(format!("winetricks finalizó con {status}"));
    }

    let args: &[&str] = match action {
        WineAction::Winecfg => &["winecfg"],
        WineAction::Explorer => &["explorer"],
        WineAction::Regedit => &["regedit"],
        WineAction::Taskmgr => &["taskmgr"],
        WineAction::Control => &["control"],
        WineAction::Uninstaller => &["uninstaller"],
        WineAction::Cmd => &["wineconsole", "cmd"],
        WineAction::Winetricks | WineAction::KillServer => unreachable!(),
    };

    let mut cmd = Command::new(&wine_bin);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    apply_wine_process_env(&mut cmd, &wine_bin, &prefix, config.auto_display, false);

    let mut child = cmd
        .spawn()
        .map_err(|e| format!("Error al abrir Wine ({}): {e}", args.join(" ")))?;
    let status = child
        .wait()
        .map_err(|e| format!("Error esperando {}: {e}", args.join(" ")))?;
    if status.success() {
        Ok(format!("{} cerrado", args[0]))
    } else {
        Err(format!("{} finalizó con {status}", args[0]))
    }
}

/// Inspects the entire Wine setup for UI presentation and diagnostics.
pub fn inspect_wine_environment(config: &WineConfig) -> WineEnvironmentStatus {
    let available_runners = discover_wine_binaries();
    let available_prefixes = discover_wine_prefixes();
    let active_wine_binary = resolve_wine_binary(config);
    let active_wine_version = active_wine_binary.as_ref().and_then(|active| {
        available_runners
            .iter()
            .find(|r| r.path == *active)
            .map(|r| r.version.clone())
            .filter(|v| !v.is_empty())
            .or_else(|| {
                let v = probe_wine_version(active);
                if v.is_empty() { None } else { Some(v) }
            })
    });

    let active_prefix = resolve_wine_prefix(config);
    let active_prefix_initialized = active_prefix
        .as_deref()
        .is_some_and(is_prefix_initialized);
    let wayland_x11_configured = active_prefix
        .as_deref()
        .is_some_and(|p| user_reg_has_wayland_and_x11(&p.join("user.reg")));

    let mut vst_directories = Vec::new();
    let mut seen_vst = HashSet::new();
    let mut add_prefix_dirs = |p: &Path| {
        for dir in standard_prefix_vst_dirs(p) {
            if seen_vst.insert(dir.clone()) {
                vst_directories.push(dir);
            }
        }
    };
    if let Some(active_p) = &active_prefix {
        add_prefix_dirs(active_p);
    }
    for info in &available_prefixes {
        if info.initialized {
            add_prefix_dirs(&info.path);
        }
    }

    let display_session = detect_display_env().session_summary;
    let bridge_mode = current_bridge_mode(active_wine_binary.as_deref());
    let winegpp_path = find_winegpp(active_wine_binary.as_deref());
    let winetricks_available = find_winetricks().is_some();

    WineEnvironmentStatus {
        active_wine_binary,
        active_wine_version,
        active_prefix,
        active_prefix_initialized,
        wayland_x11_configured,
        display_session,
        available_runners,
        available_prefixes,
        vst_directories,
        bridge_mode,
        winetricks_available,
        winegpp_path,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_graphics_driver_creates_section_when_missing() {
        let initial = "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\Fonts]\n\"LogPixels\"=dword:00000060\n";
        let updated = upsert_graphics_driver_in_reg(initial).expect("should update");
        assert!(updated.contains("[Software\\\\Wine\\\\Drivers]"));
        assert!(updated.contains("\"Graphics\"=\"x11,wayland\""));
        assert!(upsert_graphics_driver_in_reg(&updated).is_none());
    }

    #[test]
    fn upsert_graphics_driver_updates_existing_x11_only_value() {
        let initial = "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\Drivers] 1730000000\n#time=1db\n\"Audio\"=\"pulse\"\n\"Graphics\"=\"x11\"\n";
        let updated = upsert_graphics_driver_in_reg(initial).expect("should update");
        assert!(updated.contains("\"Audio\"=\"pulse\""));
        assert!(updated.contains("\"Graphics\"=\"x11,wayland\""));
        assert!(!updated.contains("\"Graphics\"=\"x11\"\n"));
    }

    #[test]
    fn upsert_graphics_driver_inserts_into_existing_drivers_section() {
        let initial = "WINE REGISTRY Version 2\n\n[Software\\\\Wine\\\\Drivers] 1730000000\n#time=1db\n\"Audio\"=\"alsa\"\n";
        let updated = upsert_graphics_driver_in_reg(initial).expect("should update");
        assert!(updated.contains("\"Graphics\"=\"x11,wayland\""));
        assert!(updated.contains("\"Audio\"=\"alsa\""));
    }

    #[test]
    fn wine_executable_args_handles_msi_bat_and_exe() {
        assert_eq!(
            wine_executable_args(Path::new("/tmp/Installer.MSI")),
            vec!["msiexec", "/i", "/tmp/Installer.MSI"]
        );
        assert_eq!(
            wine_executable_args(Path::new("/tmp/setup.bat")),
            vec!["cmd", "/c", "/tmp/setup.bat"]
        );
        assert_eq!(
            wine_executable_args(Path::new("/tmp/Setup.exe")),
            vec!["/tmp/Setup.exe"]
        );
    }

    #[test]
    fn standard_prefix_vst_dirs_includes_user_appdata_vst3() {
        let unique = format!("cx-wine-dir-test-{}", std::process::id());
        let root = env::temp_dir().join(unique);
        let user_vst3 = root.join("drive_c/users/alice/AppData/Local/Programs/Common/VST3");
        fs::create_dir_all(&user_vst3).unwrap();

        let dirs = standard_prefix_vst_dirs(&root);
        assert!(dirs.contains(&root.join("drive_c/Program Files/Common Files/VST3")));
        assert!(dirs.contains(&root.join("drive_c/Program Files/VSTPlugins")));
        assert!(dirs.contains(&user_vst3));

        let _ = fs::remove_dir_all(&root);
    }
}
