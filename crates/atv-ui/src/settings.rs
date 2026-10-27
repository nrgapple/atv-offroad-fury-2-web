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

/// The loaded player profile (Profile Points and race record, PROFILE.md).
/// Before profile books this was the one implicit profile; it now mirrors
/// the book's loaded profile, so older saves and tools keep reading it.
pub const PROFILE_KEY: &str = "fury-profile";
/// The saved profiles and which one is loaded (the memory card's profile
/// save files; PROFILE.md "Port").
pub const PROFILES_KEY: &str = "fury-profiles";

/// The saved profile book. A save from before profile books migrates: its
/// implicit profile becomes the loaded "PLAYER1". The `fury-profile` mirror
/// is authoritative for the loaded profile's record (it is what races
/// credit and what older tools write).
pub fn load_book(settings: &Settings) -> atv_core::profile::ProfileBook {
    use atv_core::profile::{Profile, ProfileBook};
    let mirror: Option<Profile> = settings
        .get(PROFILE_KEY)
        .and_then(|text| serde_json::from_str(&text).ok());
    let saved: Option<ProfileBook> = settings
        .get(PROFILES_KEY)
        .and_then(|text| serde_json::from_str(&text).ok());
    let mut book = match saved {
        Some(book) => book,
        None => return ProfileBook::migrate(mirror),
    };
    book.normalize();
    if let (Some(current), Some(mut mirror)) = (book.current_mut(), mirror) {
        mirror.name = std::mem::take(&mut current.name);
        *current = mirror;
    }
    book
}

/// Saves the book and mirrors its loaded profile under `fury-profile`
/// (`null` with none loaded); false when it lasts only for this session.
pub fn save_book(settings: &mut Settings, book: &atv_core::profile::ProfileBook) -> bool {
    let mirror = match book.current_profile() {
        Some(profile) => serde_json::to_string(profile).ok(),
        None => Some("null".into()),
    };
    let saved = serde_json::to_string(book).is_ok_and(|text| settings.set(PROFILES_KEY, &text));
    saved && mirror.is_some_and(|text| settings.set(PROFILE_KEY, &text))
}

/// The loaded profile; with none loaded, a new unnamed one (which nothing
/// saves).
pub fn load_profile(settings: &Settings) -> atv_core::profile::Profile {
    load_book(settings)
        .current_profile()
        .cloned()
        .unwrap_or_default()
}

/// Whether a profile is loaded (races credit it only then, `001f1e30`).
pub fn profile_loaded(settings: &Settings) -> bool {
    load_book(settings).current.is_some()
}

/// Autosaves the loaded profile; false when none is loaded or it lasts only
/// for this session.
pub fn save_profile(settings: &mut Settings, profile: &atv_core::profile::Profile) -> bool {
    let mut book = load_book(settings);
    let Some(current) = book.current_mut() else {
        return false;
    };
    let name = std::mem::take(&mut current.name);
    *current = profile.clone();
    if current.name.is_empty() {
        current.name = name;
    }
    save_book(settings, &book)
}

/// The Hall of Fame (`hall_of_fame`): every course's records, kept apart
/// from the profiles.
pub const HALL_OF_FAME_KEY: &str = "fury-hall-of-fame";

pub fn load_hall_of_fame(settings: &Settings) -> atv_core::hall_of_fame::HallOfFame {
    settings
        .get(HALL_OF_FAME_KEY)
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

/// False when it lasts only for this session.
pub fn save_hall_of_fame(
    settings: &mut Settings,
    hall: &atv_core::hall_of_fame::HallOfFame,
) -> bool {
    serde_json::to_string(hall).is_ok_and(|text| settings.set(HALL_OF_FAME_KEY, &text))
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

#[cfg(test)]
mod profile_book_tests {
    use super::*;
    use atv_core::profile::Profile;

    #[test]
    fn an_implicit_profile_migrates_and_stays_mirrored() {
        let mut settings = Settings::memory();
        settings.set(PROFILE_KEY, "{\"points\":40,\"races\":2}");
        let book = load_book(&settings);
        assert_eq!(book.current, Some(0));
        assert_eq!(book.profiles[0].name, "PLAYER1");
        assert_eq!(load_profile(&settings).points, 40);
        // A race credit keeps the name and lands in both keys.
        let mut p = load_profile(&settings);
        p.record_race(1, 10, 0.);
        assert!(save_profile(&mut settings, &p));
        let book = load_book(&settings);
        assert_eq!(
            (book.profiles[0].name.as_str(), book.profiles[0].points),
            ("PLAYER1", 50)
        );
        let mirror: Profile = serde_json::from_str(&settings.get(PROFILE_KEY).unwrap()).unwrap();
        assert_eq!((mirror.name.as_str(), mirror.points), ("PLAYER1", 50));
    }

    #[test]
    fn no_loaded_profile_saves_nothing() {
        let mut settings = Settings::memory();
        let mut book = load_book(&settings);
        book.create("ACE").unwrap();
        book.delete(0).unwrap();
        book.delete(0).unwrap();
        assert!(save_book(&mut settings, &book));
        assert!(!profile_loaded(&settings));
        assert!(!save_profile(&mut settings, &Profile::default()));
        assert_eq!(settings.get(PROFILE_KEY).as_deref(), Some("null"));
        assert!(load_book(&settings).profiles.is_empty());
    }

    #[test]
    fn a_written_mirror_updates_the_loaded_profile() {
        let mut settings = Settings::memory();
        let mut book = load_book(&settings);
        book.create("TWO").unwrap();
        save_book(&mut settings, &book);
        // Tools and older code write the mirror directly.
        settings.set(PROFILE_KEY, "{\"points\":9}");
        let book = load_book(&settings);
        let p = book.current_profile().unwrap();
        assert_eq!((p.name.as_str(), p.points), ("TWO", 9));
        assert_eq!(book.profiles[0].name, "PLAYER1");
    }
}
