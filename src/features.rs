//! Messages from the frame and our pages, downloads and extensions.

use std::path::PathBuf;

use crate::common::*;
use crate::ext;
use crate::store::{Download, Settings};
use serde_json::{Value, json};
use tao::window::ResizeDirection;

impl App {
    // ---------- downloads ----------

    pub fn download_started(&mut self, url: String, path: PathBuf) {
        eprintln!("download started: {url} -> {}", path.display());
        let id = self.next_download;
        self.next_download += 1;
        let file = path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
        self.store.downloads.insert(0, Download {
            id,
            url,
            path: path.display().to_string(),
            file,
            state: "downloading".into(),
            started: now_secs(),
            bytes: 0,
        });
        self.store.downloads.truncate(200);
        self.dirty = true;
        // A tab that was opened only to fetch this file has nothing to show: close it (as Chromium does).
        let url = &self.store.downloads[0].url;
        let blank = self
            .tabs
            .iter()
            .find(|t| &t.url == url && (t.title.is_empty() || t.title == "about:blank"))
            .map(|t| t.id);
        if let Some(tid) = blank {
            if self.tabs.len() > 1 {
                self.close_tab(tid);
            }
        }
        self.push();
        if self.store.settings.show_downloads_on_start {
            let _ = self.ui.evaluate_script("window.halo&&halo.cmd('dlpulse')");
        }
    }

    pub fn download_done(&mut self, url: String, path: Option<PathBuf>, ok: bool) {
        eprintln!("download done: {url} ok={ok} path={path:?}");
        let mut msg = None;
        if let Some(d) = self.store.downloads.iter_mut().find(|d| d.url == url && d.state == "downloading") {
            d.state = if ok { "done".into() } else { "failed".into() };
            if let Some(p) = path {
                d.path = p.display().to_string();
                d.file = p.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or(d.file.clone());
            }
            if let Ok(m) = std::fs::metadata(&d.path) {
                d.bytes = m.len();
            }
            msg = Some(if ok { format!("Downloaded {}", d.file) } else { format!("Download failed: {}", d.file) });
        }
        if let Some(m) = msg {
            self.toast(&m);
        }
        self.dirty = true;
        self.push();
    }

    /// Bytes received so far, read from the file being written.
    pub fn poll_downloads(&mut self) {
        let mut changed = false;
        for d in self.store.downloads.iter_mut().filter(|d| d.state == "downloading") {
            let mut size = 0;
            for p in [d.path.clone(), format!("{}.crdownload", d.path)] {
                if let Ok(m) = std::fs::metadata(&p) {
                    size = size.max(m.len());
                }
            }
            if size != d.bytes {
                d.bytes = size;
                changed = true;
            }
        }
        if changed {
            self.push();
        }
    }

    // ---------- extensions ----------

    pub fn ext_refresh(&self) {
        let p = self.proxy.clone();
        if let Err(e) = ext::list(
            &self.ui,
            Box::new(move |v| {
                let _ = p.send_event(Ev::ExtList(v));
            }),
        ) {
            eprintln!("extensions: {e}");
        }
    }

    pub fn ext_install_store(&mut self, text: &str) {
        let Some(id) = ext::store_id_from(text) else {
            self.ext_error = "That doesn't look like a Chrome Web Store address or extension id.".into();
            self.push();
            return;
        };
        if !self.boot.extensions_enabled {
            self.ext_error = "Extensions are switched off in Settings (System). Turn them on and restart.".into();
            self.push();
            return;
        }
        if self.installing.contains(&id) {
            return;
        }
        self.installing.push(id.clone());
        self.ext_error.clear();
        self.push();
        let dest = ext::extensions_dir(&self.dir).join(&id);
        let p = self.proxy.clone();
        std::thread::spawn(move || {
            let r = ext::download_crx(&id).and_then(|b| ext::unpack(&b, &dest)).map(|_| dest);
            let _ = p.send_event(Ev::ExtUnpacked(id, r));
        });
    }

    fn ext_install_file(&mut self) {
        let Some(file) = rfd::FileDialog::new()
            .add_filter("Extension package", &["crx", "zip"])
            .set_title("Choose an extension package")
            .pick_file()
        else {
            return;
        };
        let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
        let dest = ext::extensions_dir(&self.dir).join(format!("{}-{}", ext::safe_folder_name(&stem), now_secs()));
        let r = std::fs::read(&file).map_err(|e| e.to_string()).and_then(|b| ext::unpack(&b, &dest)).map(|_| dest);
        self.ext_unpacked(String::new(), r);
    }

    fn ext_load_unpacked(&mut self) {
        if let Some(dir) = rfd::FileDialog::new().set_title("Choose the extension's folder (with manifest.json)").pick_folder() {
            if !dir.join("manifest.json").exists() {
                self.ext_error = "That folder has no manifest.json, so it isn't an extension.".into();
                self.push();
                return;
            }
            self.ext_unpacked(String::new(), Ok(dir));
        }
    }

    pub fn ext_unpacked(&mut self, store_id: String, r: Result<PathBuf, String>) {
        match r {
            Ok(folder) => {
                let p = self.proxy.clone();
                let (sid, f2) = (store_id.clone(), folder.clone());
                if let Err(e) = ext::add(
                    &self.ui,
                    &folder,
                    Box::new(move |res| {
                        let _ = p.send_event(Ev::ExtAdded(sid, f2, res));
                    }),
                ) {
                    self.installing.retain(|i| *i != store_id);
                    self.ext_error = e;
                    self.push();
                }
            }
            Err(e) => {
                self.installing.retain(|i| *i != store_id);
                self.ext_error = e;
                self.push();
            }
        }
    }

    pub fn ext_added(&mut self, store_id: String, folder: PathBuf, r: Result<ext::EngineExt, String>) {
        self.installing.retain(|i| *i != store_id);
        match r {
            Ok(e) => {
                let m = ext::read_manifest(&folder);
                let name = if m.name.is_empty() { e.name.clone() } else { m.name.clone() };
                self.store.extensions.retain(|x| x.id != e.id);
                self.store.extensions.push(crate::store::Extension {
                    id: e.id.clone(),
                    name: name.clone(),
                    version: m.version,
                    description: m.description,
                    folder: folder.display().to_string(),
                    store_id,
                    enabled: e.enabled,
                    popup: m.popup,
                    options: m.options,
                    icon: m.icon,
                });
                self.ext_error.clear();
                self.toast(&format!("Added {name}"));
                println!("extension added: {} id={} folder={}", name, e.id, folder.display());
                self.dirty = true;
                self.refresh_shared();
            }
            Err(e) => {
                eprintln!("extension failed: {e}");
                self.ext_error = e;
            }
        }
        self.push();
        self.ext_refresh();
    }

    pub fn ext_list(&mut self, list: Vec<ext::EngineExt>) {
        // Keep our records in step with what the engine has.
        self.store.extensions.retain(|x| list.iter().any(|e| e.id == x.id));
        for e in list {
            if let Some(x) = self.store.extensions.iter_mut().find(|x| x.id == e.id) {
                x.enabled = e.enabled;
            } else if e.name.starts_with("Microsoft ") {
                // The engine's own built-in helpers (PDF viewer, clipboard): not the person's.
                continue;
            } else {
                self.store.extensions.push(crate::store::Extension {
                    id: e.id,
                    name: e.name,
                    enabled: e.enabled,
                    ..Default::default()
                });
            }
        }
        self.dirty = true;
        self.refresh_shared();
        self.push();
    }

    fn ext_remove(&mut self, id: &str) {
        let ours = ext::extensions_dir(&self.dir);
        let folder = self.store.extensions.iter().find(|x| x.id == id).map(|x| PathBuf::from(&x.folder));
        let p = self.proxy.clone();
        let _ = ext::remove(
            &self.ui,
            id.to_string(),
            Box::new(move || {
                if let Some(f) = folder {
                    if f.starts_with(&ours) && f != ours {
                        let _ = std::fs::remove_dir_all(&f);
                    }
                }
                let _ = p.send_event(Ev::ExtChanged);
            }),
        );
    }

    fn ext_open(&mut self, id: &str, kind: &str) {
        let Some(x) = self.store.extensions.iter().find(|x| x.id == id) else { return };
        let page = if kind == "options" || x.popup.is_empty() { x.options.clone() } else { x.popup.clone() };
        if page.is_empty() {
            let msg = format!("{} has no page to open; it works in the background.", x.name);
            self.toast(&msg);
            return;
        }
        let url = format!("chrome-extension://{}/{}", x.id, page);
        self.new_tab(&url, true);
    }

    // ---------- accounts ----------

    fn signed_in(&mut self, r: Result<crate::store::Account, String>) {
        self.account_wait = false;
        match r {
            Ok(a) => {
                self.account_error.clear();
                if a.kind == "local" {
                    self.store.local_account = Some(a.clone());
                }
                let msg = format!("Signed in as {}", if a.name.is_empty() { &a.email } else { &a.name });
                self.store.account = Some(a);
                self.dirty = true;
                self.toast(&msg);
            }
            Err(e) => self.account_error = e,
        }
        self.push();
    }

    // ---------- messages ----------

    pub fn on_ui(&mut self, msg: &str) {
        let Ok(v) = serde_json::from_str::<Value>(msg) else { return };
        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
        let id = v["id"].as_u64().unwrap_or(0) as u32;
        let cmd = s("cmd");
        match cmd.as_str() {
            "ready" => {
                self.ui_ready = true;
                self.push();
            }
            "pageReady" => self.push_pages(),
            "rect" => {
                let f = |k: &str| v[k].as_f64().unwrap_or(0.0);
                self.rect = (f("x"), f("y"), f("w"), f("h"));
                self.layout_views();
            }
            "navigate" => self.navigate(&s("text"), v["newTab"].as_bool().unwrap_or(false)),
            "newtab" => {
                let url = s("url");
                self.new_tab(if url.is_empty() { NEWTAB } else { &url }, true);
            }
            "open" => {
                let url = resolve(&self.store.settings, &s("url"));
                self.open_url(&url, v["newTab"].as_bool().unwrap_or(false));
            }
            "close" => self.close_tab(id),
            "activate" => self.activate(id, true),
            "back" | "forward" | "reload" | "home" => self.key(&cmd),
            "stop" => {
                if let Some(v) = self.active_view() {
                    let _ = v.evaluate_script("window.stop()");
                }
            }
            "key" => self.key(&s("key")),
            "openSettings" => self.open_settings(&s("section")),
            "pin" => {
                if let Some(i) = self.index(id) {
                    let mut t = self.tabs.remove(i);
                    t.pinned = !t.pinned;
                    // pinned tabs gather at the start of the strip
                    let pos = self.tabs.iter().take_while(|x| x.pinned).count();
                    self.tabs.insert(pos, t);
                    self.dirty = true;
                    self.push();
                }
            }
            "move" => {
                let before = v["before"].as_u64().unwrap_or(0) as u32;
                if let (Some(i), Some(_)) = (self.index(id), self.index(before)) {
                    if id != before {
                        let t = self.tabs.remove(i);
                        let j = self.index(before).unwrap();
                        self.tabs.insert(j, t);
                        self.dirty = true;
                        self.push();
                    }
                }
            }
            "setting" => self.set_setting(&s("key"), v["value"].clone()),
            "resetSettings" => {
                let keep = (self.store.settings.bg_version, self.store.settings.bg_mime.clone());
                self.store.settings = Settings::default();
                self.store.settings.bg_version = keep.0;
                self.store.settings.bg_mime = keep.1;
                self.dirty = true;
                self.push();
                self.refresh_newtabs();
            }
            "bgimage" => self.set_background(&s("data")),
            "bookmark" => self.toggle_bookmark(),
            "unbookmark" => {
                let url = s("url");
                self.store.bookmarks.retain(|b| b.url != url);
                self.dirty = true;
                self.push();
                self.refresh_shared();
            }
            "overlay" => self.set_overlay(v["on"].as_bool().unwrap_or(false)),
            "suggest" => self.suggest(&s("q")),
            "clearHistory" => {
                self.store.history.clear();
                self.closed.clear();
                self.dirty = true;
                self.refresh_newtabs();
                self.push();
            }
            "clearSiteData" => {
                let _ = self.ui.clear_all_browsing_data();
                self.toast("Cookies and site data cleared");
            }
            "sleepOthers" => self.sleep_tabs(true),
            "zoom" => self.zoom(v["d"].as_i64().unwrap_or(0)),
            "focusContent" => {
                if let Some(v) = self.active_view() {
                    let _ = v.focus();
                }
            }
            // downloads
            "pickDownloadDir" => {
                if let Some(d) = rfd::FileDialog::new().set_title("Choose where downloads are saved").pick_folder() {
                    self.set_setting("download_dir", json!(d.display().to_string()));
                }
            }
            "openDownload" | "showDownload" => {
                let did = v["id"].as_u64().unwrap_or(0);
                if let Some(d) = self.store.downloads.iter().find(|d| d.id == did) {
                    open_path(&d.path, cmd == "showDownload");
                }
            }
            "removeDownload" => {
                let did = v["id"].as_u64().unwrap_or(0);
                self.store.downloads.retain(|d| d.id != did);
                self.dirty = true;
                self.push();
            }
            "retryDownload" => {
                let did = v["id"].as_u64().unwrap_or(0);
                if let Some(d) = self.store.downloads.iter().find(|d| d.id == did) {
                    let url = d.url.clone();
                    self.store.downloads.retain(|d| d.id != did);
                    // Fetch it again in the background through the current page.
                    if let Some(view) = self.active_view() {
                        let _ = view.evaluate_script(&format!(
                            "(function(){{var a=document.createElement('a');a.href={};a.download='';document.body.appendChild(a);a.click();a.remove();}})()",
                            Value::String(url)
                        ));
                    }
                    self.push();
                }
            }
            "clearDownloads" => {
                self.store.downloads.retain(|d| d.state == "downloading");
                self.dirty = true;
                self.push();
            }
            "openDownloadsFolder" => {
                let d = self.store.settings.download_dir.trim().to_string();
                let d = if d.is_empty() { default_download_dir().display().to_string() } else { d };
                open_path(&d, false);
            }
            // extensions
            "installExt" => self.ext_install_store(&s("store")),
            "installExtFile" => self.ext_install_file(),
            "loadUnpacked" => self.ext_load_unpacked(),
            "removeExt" => self.ext_remove(&s("ext")),
            "toggleExt" => {
                let p = self.proxy.clone();
                let _ = ext::enable(
                    &self.ui,
                    s("ext"),
                    v["on"].as_bool().unwrap_or(true),
                    Box::new(move || {
                        let _ = p.send_event(Ev::ExtChanged);
                    }),
                );
            }
            "openExt" => self.ext_open(&s("ext"), &s("kind")),
            "openExtFolder" => {
                let want = s("ext");
                if let Some(x) = self.store.extensions.iter().find(|x| x.id == want) {
                    open_path(&x.folder, false);
                }
            }
            "defaultApps" => {
                #[cfg(windows)]
                crate::setup::open_default_apps();
            }
            "uninstallHalo" => {
                #[cfg(windows)]
                crate::setup::start_uninstaller();
            }
            // accounts
            "google" => {
                if !self.account_wait {
                    self.account_wait = true;
                    self.account_error.clear();
                    self.account_cancel.store(false, std::sync::atomic::Ordering::SeqCst);
                    let (p, c) = (self.proxy.clone(), self.account_cancel.clone());
                    std::thread::spawn(move || {
                        let _ = p.send_event(Ev::Account(crate::account::google_sign_in(c)));
                    });
                    self.push();
                }
            }
            "googleCancel" => self.account_cancel.store(true, std::sync::atomic::Ordering::SeqCst),
            "createAccount" => {
                let r = crate::account::create_local(&s("name"), &s("email"), &s("password"));
                self.signed_in(r);
            }
            "login" => {
                let r = crate::account::log_in_local(self.store.local_account.as_ref(), &s("email"), &s("password"));
                self.signed_in(r);
            }
            "signOut" => {
                self.store.account = None;
                self.account_error.clear();
                self.dirty = true;
                self.toast("Signed out");
                self.push();
            }
            "forgetLocal" => {
                self.store.local_account = None;
                if self.store.account.as_ref().map(|a| a.kind == "local").unwrap_or(false) {
                    self.store.account = None;
                }
                self.dirty = true;
                self.push();
            }
            "restart" => {
                self.restart = true;
                self.quit = true;
            }
            "win" => match s("action").as_str() {
                "min" => self.window.set_minimized(true),
                "max" => self.window.set_maximized(!self.window.is_maximized()),
                "close" => self.quit = true,
                "drag" => {
                    let _ = self.window.drag_window();
                }
                "resize" => {
                    let dir = match s("dir").as_str() {
                        "n" => ResizeDirection::North,
                        "s" => ResizeDirection::South,
                        "e" => ResizeDirection::East,
                        "w" => ResizeDirection::West,
                        "ne" => ResizeDirection::NorthEast,
                        "nw" => ResizeDirection::NorthWest,
                        "se" => ResizeDirection::SouthEast,
                        _ => ResizeDirection::SouthWest,
                    };
                    let _ = self.window.drag_resize_window(dir);
                }
                _ => {}
            },
            _ => {}
        }
    }

    pub fn on_content(&mut self, id: u32, msg: &str) {
        let Ok(v) = serde_json::from_str::<Value>(msg) else { return };
        let Some(i) = self.index(id) else { return };
        match v["t"].as_str().unwrap_or("") {
            "key" => {
                let cmd = v["cmd"].as_str().unwrap_or("").to_string();
                self.key(&cmd);
            }
            // Only our own pages may change settings.
            "ui" if is_internal(&self.tabs[i].url) => {
                let m = v["m"].to_string();
                self.on_ui(&m);
            }
            "icon" => {
                let href = v["href"].as_str().unwrap_or("");
                if href.len() < 2048 && (href.starts_with("http") || href.starts_with("data:image") || href.is_empty()) {
                    self.tabs[i].icon = href.to_string();
                    self.push();
                }
            }
            "media" => {
                self.tabs[i].media = v["on"].as_bool().unwrap_or(false);
                self.push();
            }
            "url" => {
                if let Some(h) = v["href"].as_str() {
                    self.tabs[i].url = h.to_string();
                    self.dirty = true;
                    self.push();
                }
            }
            _ => {}
        }
    }

    pub fn handle(&mut self, e: Ev) {
        match e {
            Ev::Ui(m) => self.on_ui(&m),
            Ev::Content(id, m) => self.on_content(id, &m),
            Ev::Title(id, t) => {
                if let Some(i) = self.index(id) {
                    let url = self.tabs[i].url.clone();
                    self.tabs[i].title = t.clone();
                    if !is_internal(&url) {
                        self.store.set_title(&url, &t);
                    }
                    self.update_title();
                    self.push();
                    self.dirty = true;
                }
            }
            Ev::Nav(id, u) => {
                if let Some(i) = self.index(id) {
                    let bg = self.page_bg(&u);
                    let t = &mut self.tabs[i];
                    if t.url != u {
                        t.icon.clear();
                        t.media = false;
                        if is_internal(&t.url) != is_internal(&u) {
                            if let Some(v) = &t.view {
                                let _ = v.set_background_color(bg);
                            }
                        }
                    }
                    t.url = u;
                    t.loading = true;
                    self.push();
                }
            }
            Ev::Upgrade(id, u) => {
                if let Some(i) = self.index(id) {
                    self.tabs[i].url = u.clone();
                    if let Some(v) = &self.tabs[i].view {
                        let _ = v.load_url(&u);
                    }
                    self.push();
                }
            }
            Ev::Loaded(id, u) => {
                if let Some(i) = self.index(id) {
                    let t = &mut self.tabs[i];
                    t.url = u.clone();
                    t.loading = false;
                    let title = t.title.clone();
                    if !is_internal(&u) && (u.starts_with("http://") || u.starts_with("https://")) {
                        self.store.visit(&u, &title, now_secs());
                        self.refresh_shared();
                    }
                    self.last_load_done = 0;
                    self.update_title();
                    self.push();
                    self.dirty = true;
                }
            }
            Ev::NewWindow(u) => {
                self.new_tab(&u, true);
            }
            Ev::DlStart(url, path) => self.download_started(url, path),
            Ev::DlDone(url, path, ok) => self.download_done(url, path, ok),
            Ev::ExtUnpacked(id, r) => self.ext_unpacked(id, r),
            Ev::ExtAdded(id, folder, r) => self.ext_added(id, folder, r),
            Ev::ExtList(list) => self.ext_list(list),
            Ev::ExtChanged => self.ext_refresh(),
            Ev::Account(r) => self.signed_in(r),
        }
    }
}
