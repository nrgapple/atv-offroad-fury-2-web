//! Saved player settings under the browser game's own keys: `localStorage` on
//! the web (so `/play.html` and the Rust engine share them) and a JSON file in
//! the user's configuration directory on desktop.
use std::collections::BTreeMap;

pub struct Settings {
    values: BTreeMap<String, String>,
    #[cfg(not(target_arch = "wasm32"))]
    path: Option<std::path::PathBuf>,
}

impl Settings {
    /// In-memory only (tests, parity captures).
    pub fn memory() -> Self {
        Self {
            values: BTreeMap::new(),
            #[cfg(not(target_arch = "wasm32"))]
            path: None,
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
        let path = config_path();
        let values = path
            .as_ref()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| serde_json::from_str::<BTreeMap<String, String>>(&text).ok())
            .unwrap_or_default();
        Self { values, path }
    }

    pub fn get(&self, key: &str) -> Option<String> {
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

    pub fn set(&mut self, key: &str, value: &str) {
        self.values.insert(key.into(), value.into());
        #[cfg(target_arch = "wasm32")]
        if let Some(storage) = storage() {
            // Private mode or quota: keep the session value.
            storage.set_item(key, value).ok();
        }
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = &self.path {
            if let Some(dir) = path.parent() {
                std::fs::create_dir_all(dir).ok();
            }
            if let Ok(text) = serde_json::to_string_pretty(&self.values) {
                std::fs::write(path, text).ok();
            }
        }
    }
}

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
