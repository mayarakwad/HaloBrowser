//! The browser's tabs, page views and the frame's state.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::common::*;
use crate::store::{Page, SessionTab, Settings, Store};
use serde_json::{Value, json};
use wry::dpi::{LogicalPosition, LogicalSize as WLogicalSize};
use wry::{NewWindowResponse, PageLoadEvent, PermissionKind, PermissionResponse, Rect, WebView, WebViewBuilder};

impl App {
    pub fn index(&self, id: u32) -> Option<usize> {
        self.tabs.iter().position(|t| t.id == id)
    }

    pub fn content_bounds(&self) -> Rect {
        let (x, y, w, h) = self.rect;
        Rect {
            position: LogicalPosition::new(x, y).into(),
            size: WLogicalSize::new(w.max(1.0), h.max(1.0)).into(),
        }
    }

    /// Colour shown before a page paints: the theme's shade for our own pages
    /// (so a new tab never flashes white), white for websites (as Chromium).
    pub fn page_bg(&self, url: &str) -> wry::RGBA {
        if !is_internal(url) {
            return (255, 255, 255, 255);
        }
        let s = &self.store.settings;
        let dark =
            if s.custom_bg { s.custom_dark } else { !matches!(s.theme.as_str(), "rose" | "lavender" | "sand" | "paper") };
        if dark { (14, 16, 30, 255) } else { (244, 242, 248, 255) }
    }

    pub fn build_view(&mut self, id: u32, url: &str, visible: bool, focus: bool) -> Option<WebView> {
        let (p1, p2, p3, p4, p5) =
            (self.proxy.clone(), self.proxy.clone(), self.proxy.clone(), self.proxy.clone(), self.proxy.clone());
        let (p6, p7) = (self.proxy.clone(), self.proxy.clone());
        let (sh1, sh2, sh3, sh4) = (self.shared.clone(), self.shared.clone(), self.shared.clone(), self.shared.clone());
        let bounds = self.content_bounds();
        let bg = self.page_bg(url);
        let s = self.store.settings.clone();
        let boot = self.boot.clone();
        let mut builder = WebViewBuilder::new_with_web_context(&mut self.ctx)
            .with_url(url)
            .with_background_color(bg)
            .with_bounds(bounds)
            .with_visible(visible)
            .with_focused(focus)
            .with_hotkeys_zoom(true)
            .with_clipboard(true)
            .with_autoplay(s.autoplay)
            .with_general_autofill_enabled(s.autofill)
            .with_initialization_script(CONTENT_JS)
            .with_custom_protocol("halo".into(), protocol(self.shared.clone()))
            .with_ipc_handler(move |req| {
                let _ = p1.send_event(Ev::Content(id, req.body().clone()));
            })
            .with_document_title_changed_handler(move |t| {
                let _ = p2.send_event(Ev::Title(id, t));
            })
            .with_navigation_handler(move |u| {
                let https_only = sh1.lock().map(|s| s.settings.https_only).unwrap_or(false);
                if https_only {
                    if let Some(up) = https_upgrade(&u) {
                        let _ = p3.send_event(Ev::Upgrade(id, up));
                        return false;
                    }
                }
                let _ = p3.send_event(Ev::Nav(id, u));
                true
            })
            .with_on_page_load_handler(move |e, u| {
                let _ = p4.send_event(match e {
                    PageLoadEvent::Started => Ev::Nav(id, u),
                    PageLoadEvent::Finished => Ev::Loaded(id, u),
                });
            })
            .with_new_window_req_handler(move |u, f| {
                // Sized pop-ups (sign-in windows) keep their own window unless blocked;
                // links that open a new window become tabs.
                if f.size.is_some() {
                    let block = sh2.lock().map(|s| s.settings.block_popups).unwrap_or(false);
                    return if block { NewWindowResponse::Deny } else { NewWindowResponse::Allow };
                }
                let _ = p5.send_event(Ev::NewWindow(u));
                NewWindowResponse::Deny
            })
            .with_permission_handler(move |kind| {
                let s = match sh3.lock() {
                    Ok(s) => s.settings.clone(),
                    Err(_) => return PermissionResponse::Default,
                };
                let choice = match kind {
                    PermissionKind::Camera => s.perm_camera,
                    PermissionKind::Microphone => s.perm_microphone,
                    PermissionKind::Geolocation => s.perm_location,
                    PermissionKind::Notifications => s.perm_notifications,
                    PermissionKind::ClipboardRead => s.perm_clipboard,
                    PermissionKind::AutomaticDownloads => s.perm_downloads,
                    _ => "ask".into(),
                };
                match choice.as_str() {
                    "allow" => PermissionResponse::Allow,
                    "block" => PermissionResponse::Deny,
                    _ => PermissionResponse::Default,
                }
            })
            .with_download_started_handler(move |url, path| {
                let s = sh4.lock().map(|s| s.settings.clone()).unwrap_or_default();
                let name =
                    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "download".into());
                let dir = if s.download_dir.trim().is_empty() {
                    path.parent().map(Path::to_path_buf).unwrap_or_default()
                } else {
                    PathBuf::from(s.download_dir.trim())
                };
                if s.ask_download {
                    match rfd::FileDialog::new().set_directory(&dir).set_file_name(&name).save_file() {
                        Some(p) => *path = p,
                        None => return false,
                    }
                } else {
                    let _ = std::fs::create_dir_all(&dir);
                    *path = unique_path(dir.join(&name));
                }
                let _ = p6.send_event(Ev::DlStart(url, path.clone()));
                true
            })
            .with_download_completed_handler(move |url, path, ok| {
                let _ = p7.send_event(Ev::DlDone(url, path, ok));
            });
        if !s.javascript {
            builder = builder.with_javascript_disabled();
        }
        if !s.user_agent.trim().is_empty() {
            builder = builder.with_user_agent(s.user_agent.trim());
        }
        let builder = engine_options(builder, &boot);
        match builder.build_as_child(&self.window) {
            Ok(v) => {
                crate::dlwatch::watch(&v, self.proxy.clone(), self.dl_ops.clone());
                Some(v)
            }
            Err(e) => {
                eprintln!("could not open the page: {e}");
                None
            }
        }
    }

    pub fn extensions_json(&self) -> Value {
        Value::Array(
            self.store
                .extensions
                .iter()
                .map(|e| {
                    json!({
                        "id": e.id, "name": e.name, "version": e.version, "description": e.description,
                        "enabled": e.enabled, "popup": !e.popup.is_empty(), "options": !e.options.is_empty(),
                        "icon": !e.icon.is_empty(), "store_id": e.store_id,
                    })
                })
                .collect(),
        )
    }

    pub fn push(&self) {
        if !self.ui_ready {
            return;
        }
        let tabs: Vec<Value> = self
            .tabs
            .iter()
            .map(|t| {
                json!({
                    "id": t.id, "title": t.title, "url": t.url, "loading": t.loading, "icon": t.icon,
                    "pinned": t.pinned, "media": t.media, "asleep": t.view.is_none() && t.id != self.active,
                })
            })
            .collect();
        let active_downloads = self.store.downloads.iter().filter(|d| d.state == "downloading").count();
        let (engine_name, _) = engine(&self.store.settings);
        let state = json!({
            "tabs": tabs, "active": self.active,
            "settings": self.store.settings, "bookmarks": self.store.bookmarks,
            "engine_name": engine_name,
            "downloads_active": active_downloads,
            "downloads_total": self.store.downloads.len(),
            "extensions": self.extensions_json(),
            "installing": self.installing,
            "account": crate::account::public(&self.store.account),
        });
        let _ = self.ui.evaluate_script(&format!("window.halo&&halo.update({state})"));
        self.push_pages();
    }

    /// State for our own Settings page (when one is open).
    pub fn push_pages(&self) {
        let pages: Vec<&Tab> = self.tabs.iter().filter(|t| t.url.starts_with(SETTINGS) && t.view.is_some()).collect();
        if pages.is_empty() {
            return;
        }
        let engines: Vec<Value> = ENGINES.iter().map(|e| json!({"id": e.0, "name": e.1})).collect();
        let state = json!({
            "settings": self.store.settings,
            "boot": {
                "hardware_acceleration": self.boot.hardware_acceleration,
                "proxy": self.boot.proxy,
                "extensions_enabled": self.boot.extensions_enabled,
                "safe_downloads": self.boot.safe_downloads,
            },
            "engines": engines,
            "downloads": self.store.downloads,
            "extensions": self.extensions_json(),
            "installing": self.installing,
            "ext_error": self.ext_error,
            "bookmarks": self.store.bookmarks.len(),
            "history": self.store.history.len(),
            "version": VERSION,
            "data_dir": self.dir.display().to_string(),
            "default_download_dir": default_download_dir().display().to_string(),
            "installed": installed(),
            "account": crate::account::public(&self.store.account),
            "has_local": self.store.local_account.is_some(),
            "google": crate::account::google_configured(),
            "account_wait": self.account_wait,
            "account_error": self.account_error,
        });
        let js = format!("window.haloPage&&haloPage.update({state})");
        for t in pages {
            if let Some(v) = &t.view {
                let _ = v.evaluate_script(&js);
            }
        }
    }

    pub fn toast(&self, msg: &str) {
        let _ = self.ui.evaluate_script(&format!("window.halo&&halo.toast({})", Value::String(msg.to_string())));
    }

    pub fn refresh_shared(&self) {
        let (name, url) = engine(&self.store.settings);
        let data = json!({
            "settings": self.store.settings, "shortcuts": self.store.shortcuts(),
            "engine_name": name, "search_url": url,
        });
        let mut s = self.shared.lock().unwrap();
        s.newtab_json = data.to_string();
        s.bg_mime = self.store.settings.bg_mime.clone();
        s.settings = self.store.settings.clone();
        s.ext_icons = self
            .store
            .extensions
            .iter()
            .filter(|e| !e.icon.is_empty())
            .map(|e| (e.id.clone(), PathBuf::from(&e.folder).join(&e.icon)))
            .collect();
    }

    pub fn refresh_newtabs(&self) {
        self.refresh_shared();
        for t in &self.tabs {
            if t.url.starts_with(NEWTAB) {
                if let Some(v) = &t.view {
                    let _ = v.evaluate_script("window.haloRefresh&&haloRefresh()");
                }
            }
        }
    }

    pub fn update_title(&self) {
        let t = self.index(self.active).map(|i| &self.tabs[i]);
        let title = match t {
            Some(t) if !t.title.is_empty() && !t.url.starts_with(NEWTAB) => format!("{} - Halo", t.title),
            _ => "Halo".into(),
        };
        self.window.set_title(&title);
    }

    pub fn activate(&mut self, id: u32, focus: bool) {
        let Some(idx) = self.index(id) else { return };
        self.active = id;
        for t in &self.tabs {
            if t.id != id {
                if let Some(v) = &t.view {
                    let _ = v.set_visible(false);
                }
            }
        }
        let show = !self.overlay;
        let focus = focus && !self.shot;
        if self.tabs[idx].view.is_none() {
            let url = self.tabs[idx].url.clone();
            let v = self.build_view(id, &url, show, focus);
            let t = &mut self.tabs[idx];
            t.view = v;
            t.loading = true;
            if t.zoom != 1.0 {
                if let Some(v) = &t.view {
                    let _ = v.zoom(t.zoom);
                }
            }
        } else if let Some(v) = &self.tabs[idx].view {
            let _ = v.set_bounds(self.content_bounds());
            let _ = v.set_visible(show);
            if focus {
                let _ = v.focus();
            }
        }
        self.tabs[idx].last_active = Instant::now();
        self.update_title();
        self.push();
        self.dirty = true;
    }

    pub fn new_tab(&mut self, url: &str, activate: bool) -> u32 {
        let id = self.next_id;
        self.next_id += 1;
        let pos = if self.store.settings.new_tab_position == "end" {
            self.tabs.len()
        } else {
            self.index(self.active).map(|i| i + 1).unwrap_or(self.tabs.len())
        };
        let zoom = self.store.settings.default_zoom.clamp(0.25, 5.0);
        self.tabs.insert(pos, Tab::new(id, url, "", false, zoom));
        if activate {
            self.activate(id, true);
        } else {
            let v = self.build_view(id, url, false, false);
            let i = self.index(id).unwrap();
            if let Some(v) = &v {
                if zoom != 1.0 {
                    let _ = v.zoom(zoom);
                }
            }
            self.tabs[i].view = v;
            self.tabs[i].loading = true;
            self.push();
        }
        self.dirty = true;
        id
    }

    pub fn close_tab(&mut self, id: u32) {
        let Some(i) = self.index(id) else { return };
        let t = self.tabs.remove(i);
        if !is_internal(&t.url) {
            self.closed.push(SessionTab { url: t.url.clone(), title: t.title.clone(), pinned: t.pinned });
            if self.closed.len() > 25 {
                self.closed.remove(0);
            }
        }
        drop(t);
        if self.tabs.is_empty() {
            if self.store.settings.close_with_last_tab {
                self.quit = true;
            } else {
                self.new_tab(NEWTAB, true);
            }
            return;
        }
        if self.active == id {
            let next = self.tabs[i.min(self.tabs.len() - 1)].id;
            self.activate(next, true);
        } else {
            self.push();
        }
        self.dirty = true;
    }

    pub fn navigate(&mut self, text: &str, in_new_tab: bool) {
        let url = resolve(&self.store.settings, text);
        self.open_url(&url, in_new_tab);
    }

    pub fn open_url(&mut self, url: &str, in_new_tab: bool) {
        if in_new_tab {
            self.new_tab(url, true);
            return;
        }
        let Some(i) = self.index(self.active) else {
            self.new_tab(url, true);
            return;
        };
        self.tabs[i].url = url.to_string();
        self.tabs[i].loading = true;
        if let Some(v) = &self.tabs[i].view {
            let _ = v.load_url(url);
            if !self.shot {
                let _ = v.focus();
            }
        } else {
            self.activate(self.active, true);
        }
        self.push();
    }

    /// Open (or switch to) the Settings page at a section.
    pub fn open_settings(&mut self, section: &str) {
        let url = if section.is_empty() { SETTINGS.to_string() } else { format!("{SETTINGS}#{section}") };
        if let Some(t) = self.tabs.iter().find(|t| t.url.starts_with(SETTINGS)) {
            let id = t.id;
            self.activate(id, true);
            if let Some(i) = self.index(id) {
                if let Some(v) = &self.tabs[i].view {
                    let _ = v.evaluate_script(&format!(
                        "window.haloPage&&haloPage.go({})",
                        Value::String(section.into())
                    ));
                }
            }
        } else {
            self.new_tab(&url, true);
        }
    }

    pub fn home(&mut self) {
        let h = self.store.settings.home_url.trim().to_string();
        let url = if h.is_empty() { NEWTAB.to_string() } else { resolve(&self.store.settings, &h) };
        self.open_url(&url, false);
    }

    pub fn active_view(&self) -> Option<&WebView> {
        self.index(self.active).and_then(|i| self.tabs[i].view.as_ref())
    }

    pub fn focus_address(&self) {
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script("window.halo&&halo.cmd('focus')");
    }

    pub fn ui_cmd(&self, c: &str) {
        let _ = self.ui.focus();
        let _ = self.ui.evaluate_script(&format!("window.halo&&halo.cmd('{c}')"));
    }

    pub fn set_overlay(&mut self, on: bool) {
        self.overlay = on;
        if let Some(v) = self.active_view() {
            let _ = v.set_visible(!on);
            if !on && !self.shot {
                let _ = v.focus();
            }
        }
    }

    pub fn layout_views(&self) {
        let b = self.content_bounds();
        for t in &self.tabs {
            if let Some(v) = &t.view {
                let _ = v.set_bounds(b);
            }
        }
    }

    pub fn set_setting(&mut self, key: &str, value: Value) {
        let mut cur = serde_json::to_value(&self.store.settings).unwrap_or(json!({}));
        if let Some(obj) = cur.as_object_mut() {
            if obj.contains_key(key) {
                obj.insert(key.to_string(), value);
            }
        }
        if let Ok(s) = serde_json::from_value::<Settings>(cur) {
            self.store.settings = s;
        }
        let st = &mut self.store.settings;
        st.bg_dim = st.bg_dim.clamp(0.0, 0.7);
        st.default_zoom = st.default_zoom.clamp(0.25, 5.0);
        if st.layout != "horizontal" {
            st.layout = "vertical".into();
        }
        self.dirty = true;
        self.push();
        self.refresh_newtabs();
    }

    pub fn set_background(&mut self, data_url: &str) {
        use base64::Engine;
        let Some((head, b64)) = data_url.split_once(',') else { return };
        let mime = head.trim_start_matches("data:").split(';').next().unwrap_or("image/jpeg").to_string();
        let Ok(bytes) = base64::engine::general_purpose::STANDARD.decode(b64) else { return };
        if std::fs::write(Store::bg_path(&self.dir), bytes).is_err() {
            return;
        }
        let st = &mut self.store.settings;
        st.custom_bg = true;
        st.bg_mime = mime;
        st.bg_version += 1;
        self.dirty = true;
        self.push();
        self.refresh_newtabs();
    }

    pub fn toggle_bookmark(&mut self) {
        let Some(i) = self.index(self.active) else { return };
        let (url, title) = (self.tabs[i].url.clone(), self.tabs[i].title.clone());
        if is_internal(&url) {
            return;
        }
        if let Some(p) = self.store.bookmarks.iter().position(|b| b.url == url) {
            self.store.bookmarks.remove(p);
        } else {
            self.store.bookmarks.push(Page { url, title });
        }
        self.dirty = true;
        self.push();
        self.refresh_shared();
    }

    pub fn suggest(&self, q: &str) {
        let q = q.to_lowercase();
        let hit = |u: &str, t: &str| u.to_lowercase().contains(&q) || t.to_lowercase().contains(&q);
        let mut out: Vec<Value> = Vec::new();
        let mut seen: Vec<String> = Vec::new();
        for t in &self.tabs {
            if t.id != self.active && !is_internal(&t.url) && hit(&t.url, &t.title) && out.len() < 2 {
                out.push(json!({"kind":"tab","id":t.id,"title":t.title,"url":t.url}));
                seen.push(t.url.clone());
            }
        }
        for b in &self.store.bookmarks {
            if hit(&b.url, &b.title) && !seen.contains(&b.url) && out.len() < 4 {
                out.push(json!({"kind":"bookmark","title":b.title,"url":b.url}));
                seen.push(b.url.clone());
            }
        }
        let mut hist: Vec<&crate::store::Visit> =
            self.store.history.iter().filter(|v| hit(&v.url, &v.title)).collect();
        hist.sort_by(|a, b| b.count.cmp(&a.count).then(b.last.cmp(&a.last)));
        for v in hist {
            if out.len() >= 7 {
                break;
            }
            if !seen.contains(&v.url) {
                out.push(json!({"kind":"history","title":v.title,"url":v.url}));
                seen.push(v.url.clone());
            }
        }
        let _ = self.ui.evaluate_script(&format!("window.halo&&halo.suggestions({})", Value::Array(out)));
    }

    pub fn zoom(&mut self, d: i64) {
        let Some(i) = self.index(self.active) else { return };
        let cur = self.tabs[i].zoom;
        let z = match d {
            0 => 1.0,
            d if d > 0 => *ZOOM_STEPS.iter().find(|z| **z > cur + 1e-3).unwrap_or(&5.0),
            _ => *ZOOM_STEPS.iter().rev().find(|z| **z < cur - 1e-3).unwrap_or(&0.25),
        };
        self.tabs[i].zoom = z;
        if let Some(v) = &self.tabs[i].view {
            let _ = v.zoom(z);
        }
    }

    /// Put tabs to sleep (free their page) when unused for a while; never the
    /// visible tab or any tab playing sound.
    pub fn sleep_tabs(&mut self, force: bool) {
        let mins = self.store.settings.sleep_minutes;
        if mins == 0 && !force {
            return;
        }
        let limit = Duration::from_secs(mins as u64 * 60);
        let active = self.active;
        let mut changed = false;
        for t in self.tabs.iter_mut() {
            if t.id == active || t.view.is_none() || t.media {
                continue;
            }
            if force || t.last_active.elapsed() >= limit {
                t.view = None;
                t.loading = false;
                changed = true;
            }
        }
        if changed {
            self.push();
        }
    }

    pub fn key(&mut self, cmd: &str) {
        match cmd {
            "newtab" => {
                self.new_tab(NEWTAB, true);
            }
            "close" => self.close_tab(self.active),
            "reopen" => {
                if let Some(t) = self.closed.pop() {
                    let id = self.new_tab(&t.url, true);
                    if let Some(i) = self.index(id) {
                        self.tabs[i].pinned = t.pinned;
                        self.tabs[i].title = t.title;
                    }
                    self.push();
                }
            }
            "focus" => self.focus_address(),
            "palette" => self.ui_cmd("palette"),
            "themes" => self.ui_cmd("themes"),
            "settings" => self.open_settings(""),
            "downloads" => self.open_settings("downloads"),
            "extensions" => self.open_settings("extensions"),
            "home" => self.home(),
            "sidebar" => {
                let c = !self.store.settings.collapsed;
                self.set_setting("collapsed", json!(c));
            }
            "next" | "prev" => {
                if let Some(i) = self.index(self.active) {
                    let n = self.tabs.len();
                    let j = if cmd == "next" { (i + 1) % n } else { (i + n - 1) % n };
                    let id = self.tabs[j].id;
                    self.activate(id, true);
                }
            }
            "bookmark" => self.toggle_bookmark(),
            "reload" => {
                if let Some(v) = self.active_view() {
                    let _ = v.reload();
                }
            }
            "back" => {
                if let Some(v) = self.active_view() {
                    let _ = v.evaluate_script("history.back()");
                }
            }
            "forward" => {
                if let Some(v) = self.active_view() {
                    let _ = v.evaluate_script("history.forward()");
                }
            }
            c if c.starts_with("tab") => {
                if let Ok(n) = c[3..].parse::<usize>() {
                    // Ctrl+9 always means the last tab, as in Chromium.
                    let j = if n == 9 { self.tabs.len().saturating_sub(1) } else { n.saturating_sub(1) };
                    if let Some(t) = self.tabs.get(j) {
                        let id = t.id;
                        self.activate(id, true);
                    }
                }
            }
            _ => {}
        }
    }

    pub fn save(&mut self) {
        if self.shot {
            return;
        }
        let real: Vec<&Tab> = self.tabs.iter().filter(|t| !is_internal(&t.url)).collect();
        self.store.session =
            real.iter().map(|t| SessionTab { url: t.url.clone(), title: t.title.clone(), pinned: t.pinned }).collect();
        self.store.session_active = real.iter().position(|t| t.id == self.active).unwrap_or(0);
        self.store.save(&self.dir);
        self.dirty = false;
        self.last_save = Instant::now();
    }

    pub fn on_exit(&mut self) {
        if self.store.settings.clear_history_on_exit {
            self.store.history.clear();
        }
        self.save();
        if self.restart {
            if let Ok(exe) = std::env::current_exe() {
                let _ = std::process::Command::new(exe).spawn();
            }
        }
    }
}

/// Whether this copy of Halo is the one installed by the setup wizard.
fn installed() -> bool {
    #[cfg(windows)]
    {
        crate::setup::running_installed()
    }
    #[cfg(not(windows))]
    {
        false
    }
}
