//! Saved player settings under the browser game's own keys: `localStorage` on
//! the web (so `/play.html` and the Rust engine share them) and a JSON file in
//! the user's configuration directory on desktop.
//!
//! Desktop: every file-backed instance shares one process-wide map per file
//! (as every page script shares `localStorage`), and each write merges its
//! key into the file as it is on disk, so no instance's write drops another's
//! key (for example the slot key `atv-mp-session` beside the menu settings).
use std::collections::BTreeMap;

pub struct Settings {
    values: BTreeMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    path: Option<std::path::PathBuf>,
    /// The process-wide values of `path` (desktop file-backed instances).
    #[cfg(not(target_arch = "wasm32"))]
    shared: Option<Shared>,
}

#[cfg(not(target_arch = "wasm32"))]
type Shared = std::sync::Arc<std::sync::Mutex<BTreeMap<String, String>>>;

impl Settings {
    /// In-memory only (tests, parity captures).
    pub fn memory() -> Self {
        Self {
            values: BTreeMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            path: None,
            #[cfg(not(target_arch = "wasm32"))]
            shared: None,
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load() -> Self {
        Self::memory()
    }

    /// `~/Library/Application Support/Fury Remastered/settings.json`,
    /// `%APPDATA%\Fury Remastered\settings.json` or
    /// `$XDG_CONFIG_HOME/fury-remastered/settings.json`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn load() -> Self {
        match config_path() {
            Some(path) => Self::at(path),
            None => Self::memory(),
        }
    }

    /// The settings file at `path`, shared with every other instance of it
    /// in this process.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn at(path: std::path::PathBuf) -> Self {
        use std::sync::{Arc, Mutex, OnceLock};
        static FILES: OnceLock<Mutex<BTreeMap<std::path::PathBuf, Shared>>> = OnceLock::new();
        let mut files = FILES
            .get_or_init(Default::default)
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        let shared = files
            .entry(path.clone())
            .or_insert_with(|| Arc::new(Mutex::new(read_file(&path))))
            .clone();
        Self {
            values: BTreeMap::new(),
            path: Some(path),
            shared: Some(shared),
        }
    }

    pub fn get(&self, key: &str) -> Option<String> {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(shared) = &self.shared {
            return shared
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .get(key)
                .cloned();
        }
        // Prefer this session after a write, including quota/private-mode failures.
        if let Some(value) = self.values.get(key) {
            return Some(value.clone());
        }
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = storage() {
            return storage.get_item(key).ok().flatten();
        }
        self.values.get(key).cloned()
    }

    /// Saves a value; false when it lasts only for this session (no
    /// storage, private mode or quota, an unwritable file).
    pub fn set(&mut self, key: &str, value: &str) -> bool {
        #[cfg(not(target_arch = "wasm32"))]
        if let (Some(shared), Some(path)) = (&self.shared, &self.path) {
            let mut shared = shared.lock().unwrap_or_else(|e| e.into_inner());
            shared.insert(key.into(), value.into());
            // Read-modify-write: only this key changes in the file on disk.
            let mut file = read_file(path);
            file.insert(key.into(), value.into());
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            return serde_json::to_string_pretty(&file)
                .is_ok_and(|text| std::fs::write(path, text).is_ok());
        }
        self.values.insert(key.into(), value.into());
        #[cfg(target_arch = "wasm32")]
        {
            // Private mode or quota: keep the session value.
            storage().is_some_and(|storage| storage.set_item(key, value).is_ok())
        }
        // Desktop in-memory settings (tests, captures) need no storage.
        #[cfg(not(target_arch = "wasm32"))]
        true
    }
}

#[cfg(not(target_arch = "wasm32"))]
fn read_file(path: &std::path::Path) -> BTreeMap<String, String> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// bindings.js `storageError`: the notice when a binding cannot be saved.
pub const STORAGE_ERROR: &str = "Settings work for this session; browser storage is unavailable.";

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok().flatten()
}

#[cfg(not(target_arch = "wasm32"))]
fn config_path() -> Option<std::path::PathBuf> {
    use std::path::PathBuf;
    let env = |name| std::env::var_os(name).map(PathBuf::from);
    if let Some(path) = env("FURY_SETTINGS") {
        return Some(path);
    }
    let dir = if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/Fury Remastered"))
    } else if cfg!(windows) {
        env("APPDATA").map(|a| a.join("Fury Remastered"))
    } else {
        env("XDG_CONFIG_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".config")))
            .map(|c| c.join("fury-remastered"))
    };
    dir.map(|d| d.join("settings.json"))
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::Settings;

    #[test]
    fn desktop_writes_from_separate_instances_keep_each_others_keys() {
        let dir = std::env::temp_dir().join(format!("fury-settings-{}", std::process::id()));
        let path = dir.join("settings.json");
        let _ = std::fs::remove_file(&path);
        // A long-lived instance (the overlay) and a fresh one (the slot key).
        let mut overlay = Settings::at(path.clone());
        overlay.set("atv-graphics", "high");
        let mut session = Settings::at(path.clone());
        session.set("atv-mp-session", "{}");
        // The long-lived instance sees the other's key and keeps it.
        assert_eq!(overlay.get("atv-mp-session").as_deref(), Some("{}"));
        overlay.set("atv-sound", "original");
        let text = std::fs::read_to_string(&path).unwrap();
        let file: std::collections::BTreeMap<String, String> = serde_json::from_str(&text).unwrap();
        assert_eq!(file.get("atv-mp-session").map(String::as_str), Some("{}"));
        assert_eq!(file.get("atv-graphics").map(String::as_str), Some("high"));
        assert_eq!(file.get("atv-sound").map(String::as_str), Some("original"));
        // A key another process added to the file survives this process's write.
        std::fs::write(&path, r#"{"atv-other":"1","atv-sound":"original"}"#).unwrap();
        session.set("atv-graphics", "enhanced");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("atv-other") && text.contains("enhanced"));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
