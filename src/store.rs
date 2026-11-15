//! Settings, bookmarks, history and the saved session, kept in one JSON file.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: String,
    pub accent: String,
    pub layout: String,
    pub collapsed: bool,
    pub engine: String,
    pub custom_bg: bool,
    pub custom_dark: bool,
    pub bg_dim: f64,
    pub bg_version: u64,
    pub bg_mime: String,
    pub sleep_minutes: u32,
    pub restore_session: bool,
    pub show_shortcuts: bool,
    pub show_clock: bool,
    pub animations: bool,
    // Startup and home
    pub startup: String,
    pub home_url: String,
    pub show_home: bool,
    // Look
    pub show_bookmarks_bar: bool,
    pub density: String,
    pub corners: String,
    // Search and address bar
    pub custom_engine_name: String,
    pub custom_engine_url: String,
    pub show_suggestions: bool,
    pub full_urls: bool,
    // Tabs
    pub close_with_last_tab: bool,
    pub new_tab_position: String,
    // Privacy and security
    pub https_only: bool,
    pub block_popups: bool,
    pub clear_history_on_exit: bool,
    pub clear_site_data_on_exit: bool,
    pub autofill: bool,
    pub javascript: bool,
    pub autoplay: bool,
    pub do_not_track: bool,
    // Site permissions: ask, allow or block
    pub perm_camera: String,
    pub perm_microphone: String,
    pub perm_location: String,
    pub perm_notifications: String,
    pub perm_clipboard: String,
    pub perm_downloads: String,
    // Downloads
    pub download_dir: String,
    pub ask_download: bool,
    pub show_downloads_on_start: bool,
    // System (applied when the browser starts)
    pub hardware_acceleration: bool,
    pub proxy: String,
    pub user_agent: String,
    pub default_zoom: f64,
    pub extensions_enabled: bool,
    /// Microsoft Defender SmartScreen checks downloads and sites for danger.
    pub safe_downloads: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            theme: "aurora".into(),
            accent: String::new(),
            layout: "vertical".into(),
            collapsed: false,
            engine: "google".into(),
            custom_bg: false,
            custom_dark: true,
            bg_dim: 0.0,
            bg_version: 0,
            bg_mime: "image/jpeg".into(),
            sleep_minutes: 30,
            restore_session: true,
            show_shortcuts: true,
            show_clock: true,
            animations: true,
            startup: "restore".into(),
            home_url: String::new(),
            show_home: false,
            show_bookmarks_bar: false,
            density: "comfortable".into(),
            corners: "rounded".into(),
            custom_engine_name: String::new(),
            custom_engine_url: String::new(),
            show_suggestions: true,
            full_urls: false,
            close_with_last_tab: true,
            new_tab_position: "next".into(),
            https_only: false,
            block_popups: false,
            clear_history_on_exit: false,
            clear_site_data_on_exit: false,
            autofill: true,
            javascript: true,
            autoplay: true,
            do_not_track: false,
            perm_camera: "ask".into(),
            perm_microphone: "ask".into(),
            perm_location: "ask".into(),
            perm_notifications: "ask".into(),
            perm_clipboard: "ask".into(),
            perm_downloads: "ask".into(),
            download_dir: String::new(),
            ask_download: false,
            show_downloads_on_start: true,
            hardware_acceleration: true,
            proxy: String::new(),
            user_agent: String::new(),
            default_zoom: 1.0,
            extensions_enabled: true,
            safe_downloads: true,
        }
    }
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Page {
    pub url: String,
    pub title: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct Visit {
    pub url: String,
    pub title: String,
    pub count: u32,
    pub last: u64,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct SessionTab {
    pub url: String,
    pub title: String,
    pub pinned: bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Download {
    pub id: u64,
    pub url: String,
    pub path: String,
    pub file: String,
    /// downloading, done, failed
    pub state: String,
    pub started: u64,
    pub bytes: u64,
    /// Full size when the website says it (0 = unknown).
    pub total: u64,
    /// Why it stopped, in plain words.
    pub error: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Extension {
    /// Id the engine gave it.
    pub id: String,
    pub name: String,
    pub version: String,
    pub description: String,
    /// Folder with its unpacked files.
    pub folder: String,
    /// Chrome Web Store id, when installed from the store.
    pub store_id: String,
    pub enabled: bool,
    pub popup: String,
    pub options: String,
    pub icon: String,
}

/// Who is signed in to Halo on this computer.
#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Account {
    /// "google" or "local"
    pub kind: String,
    pub name: String,
    pub email: String,
    /// Google profile picture address (Google accounts only).
    pub picture: String,
    pub created: u64,
    /// Local accounts: salted, stretched password hash (never the password itself).
    pub salt: String,
    pub hash: String,
}

#[derive(Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Store {
    pub settings: Settings,
    pub bookmarks: Vec<Page>,
    pub history: Vec<Visit>,
    pub session: Vec<SessionTab>,
    pub session_active: usize,
    pub downloads: Vec<Download>,
    pub extensions: Vec<Extension>,
    /// The signed-in account, if any.
    pub account: Option<Account>,
    /// A local Halo account kept on this computer while signed out, so you can log in again.
    pub local_account: Option<Account>,
}

pub const HISTORY_LIMIT: usize = 2000;

impl Store {
    pub fn load(dir: &Path) -> Store {
        std::fs::read_to_string(dir.join("state.json"))
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, dir: &Path) {
        if let Ok(s) = serde_json::to_string(self) {
            let tmp = dir.join("state.json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, dir.join("state.json"));
            }
        }
    }

    pub fn bg_path(dir: &Path) -> PathBuf {
        dir.join("background.img")
    }

    /// Remember a visit: one row per address, counting how often it was opened.
    pub fn visit(&mut self, url: &str, title: &str, now: u64) {
        if let Some(v) = self.history.iter_mut().find(|v| v.url == url) {
            v.count += 1;
            v.last = now;
            if !title.is_empty() {
                v.title = title.to_string();
            }
        } else {
            self.history.push(Visit { url: url.into(), title: title.into(), count: 1, last: now });
            if self.history.len() > HISTORY_LIMIT {
                // drop the oldest entry
                if let Some((i, _)) = self.history.iter().enumerate().min_by_key(|(_, v)| v.last) {
                    self.history.remove(i);
                }
            }
        }
    }

    pub fn set_title(&mut self, url: &str, title: &str) {
        if let Some(v) = self.history.iter_mut().find(|v| v.url == url) {
            v.title = title.to_string();
        }
    }

    /// Bookmarks first, then the most visited sites, for the new tab page.
    pub fn shortcuts(&self) -> Vec<Page> {
        let mut out: Vec<Page> = self.bookmarks.iter().take(8).cloned().collect();
        let mut top: Vec<&Visit> = self.history.iter().collect();
        top.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
        let mut hosts: Vec<String> = out.iter().map(|p| host_of(&p.url)).collect();
        for v in top {
            if out.len() >= 8 {
                break;
            }
            let h = host_of(&v.url);
            if h.is_empty() || hosts.contains(&h) {
                continue;
            }
            hosts.push(h);
            out.push(Page { url: v.url.clone(), title: v.title.clone() });
        }
        out
    }
}

pub fn host_of(url: &str) -> String {
    let rest = url.split("://").nth(1).unwrap_or("");
    let h = rest.split(['/', '?', '#']).next().unwrap_or("");
    h.trim_start_matches("www.").to_lowercase()
}
