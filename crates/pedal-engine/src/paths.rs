use std::{
    env,
    fs,
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub struct AppPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
}

impl AppPaths {
    pub fn resolve() -> Self {
        let (config_dir, data_dir, cache_dir) = resolve_os_dirs();
        let _ = fs::create_dir_all(&config_dir);
        let _ = fs::create_dir_all(&data_dir);
        let _ = fs::create_dir_all(&cache_dir);
        Self {
            config_dir,
            data_dir,
            cache_dir,
        }
    }

    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }

    pub fn legacy_settings_file() -> Option<PathBuf> {
        let p = env::current_dir().ok()?.join("chronomix-settings.json");
        p.is_file().then_some(p)
    }

    pub fn vst_plugins_dir(&self) -> PathBuf {
        let dir = self.data_dir.join("vst_plugins");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn recordings_dir(&self) -> PathBuf {
        let dir = self.data_dir.join("recordings");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn profiles_dir(&self) -> PathBuf {
        let dir = self.data_dir.join("profiles");
        let _ = fs::create_dir_all(&dir);
        dir
    }

    pub fn window_state_file(&self) -> PathBuf {
        self.config_dir.join("window.json")
    }
}

fn non_empty_env(key: &str) -> Option<PathBuf> {
    env::var_os(key)
        .filter(|v| !v.is_empty())
        .map(PathBuf::from)
}

#[cfg(target_os = "windows")]
fn resolve_os_dirs() -> (PathBuf, PathBuf, PathBuf) {
    let appdata = non_empty_env("APPDATA")
        .or_else(|| non_empty_env("USERPROFILE").map(|h| h.join("AppData").join("Roaming")))
        .unwrap_or_else(env::temp_dir);
    let local_appdata = non_empty_env("LOCALAPPDATA")
        .or_else(|| non_empty_env("USERPROFILE").map(|h| h.join("AppData").join("Local")))
        .unwrap_or_else(|| appdata.clone());
    (
        appdata.join("Chronomix"),
        appdata.join("Chronomix"),
        local_appdata.join("Chronomix").join("cache"),
    )
}

#[cfg(target_os = "macos")]
fn resolve_os_dirs() -> (PathBuf, PathBuf, PathBuf) {
    let home = non_empty_env("HOME").unwrap_or_else(env::temp_dir);
    let app_support = home
        .join("Library")
        .join("Application Support")
        .join("Chronomix");
    let cache = home.join("Library").join("Caches").join("Chronomix");
    (app_support.clone(), app_support, cache)
}

#[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
fn resolve_os_dirs() -> (PathBuf, PathBuf, PathBuf) {
    let home = non_empty_env("HOME").unwrap_or_else(env::temp_dir);
    let config = non_empty_env("XDG_CONFIG_HOME")
        .unwrap_or_else(|| home.join(".config"))
        .join("chronomix");
    let data = non_empty_env("XDG_DATA_HOME")
        .unwrap_or_else(|| home.join(".local").join("share"))
        .join("chronomix");
    let cache = non_empty_env("XDG_CACHE_HOME")
        .unwrap_or_else(|| home.join(".cache"))
        .join("chronomix");
    (config, data, cache)
}

pub fn load_settings_json_with_migration(paths: &AppPaths) -> Option<String> {
    let primary = paths.settings_file();
    if let Ok(text) = fs::read_to_string(&primary) {
        return Some(text);
    }
    if let Some(legacy) = AppPaths::legacy_settings_file() {
        if let Ok(text) = fs::read_to_string(&legacy) {
            let _ = fs::create_dir_all(&paths.config_dir);
            let _ = fs::write(&primary, &text);
            return Some(text);
        }
    }
    None
}

pub fn save_settings_json(paths: &AppPaths, json: &str) -> Result<(), String> {
    fs::create_dir_all(&paths.config_dir).map_err(|e| e.to_string())?;
    fs::write(paths.settings_file(), json).map_err(|e| e.to_string())
}

pub fn ensure_dir(path: &Path) -> PathBuf {
    let _ = fs::create_dir_all(path);
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolved_paths_are_not_relative_cwd() {
        let paths = AppPaths::resolve();
        assert!(paths.config_dir.is_absolute());
        assert!(paths.data_dir.is_absolute());
        assert!(paths.cache_dir.is_absolute());
        assert!(paths.vst_plugins_dir().ends_with("vst_plugins"));
        assert!(paths.recordings_dir().ends_with("recordings"));
        assert!(paths.settings_file().ends_with("settings.json"));
    }
}
