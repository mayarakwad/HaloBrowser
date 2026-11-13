//! Shared types, constants and helpers.

use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crate::store::{SessionTab, Settings, Store};
use tao::event_loop::EventLoopProxy;
use tao::window::{Icon, Window};
use wry::http::{Request, Response, header::CONTENT_TYPE};
use wry::{ProxyConfig, ProxyEndpoint, WebContext, WebView, WebViewBuilder, WebViewId};

pub const VERSION: &str = env!("CARGO_PKG_VERSION");
pub const NEWTAB: &str = "http://halo.localhost/newtab";
pub const SETTINGS: &str = "http://halo.localhost/settings";
pub const UI_URL: &str = "http://halo.localhost/ui";
const UI_HTML: &str = include_str!("ui.html");
const NEWTAB_HTML: &str = include_str!("newtab.html");
const SETTINGS_HTML: &str = include_str!("settings.html");
pub const THEMES_JS: &str = include_str!("themes.js");

/// Engine switches handed to Blink/V8. The same switches are compiled into the
/// original (GPU rasterization, zero-copy uploads, parallel downloading).
pub const BROWSER_ARGS: &str = "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection \
--enable-features=ParallelDownloading --enable-gpu-rasterization --enable-zero-copy";

/// Chromium's preset zoom steps (zoom in/out move to the next one).
pub const ZOOM_STEPS: [f64; 17] = [
    0.25, 0.333, 0.5, 0.667, 0.75, 0.8, 0.9, 1.0, 1.1, 1.25, 1.5, 1.75, 2.0, 2.5, 3.0, 4.0, 5.0,
];

/// Search engines (id, name, address with {q}); Google is the default.
pub const ENGINES: [(&str, &str, &str); 8] = [
    ("google", "Google", "https://www.google.com/search?q={q}"),
    ("duckduckgo", "DuckDuckGo", "https://duckduckgo.com/?q={q}"),
    ("bing", "Bing", "https://www.bing.com/search?q={q}"),
    ("brave", "Brave Search", "https://search.brave.com/search?q={q}"),
    ("ecosia", "Ecosia", "https://www.ecosia.org/search?q={q}"),
    ("startpage", "Startpage", "https://www.startpage.com/do/search?query={q}"),
    ("qwant", "Qwant", "https://www.qwant.com/?q={q}"),
    ("kagi", "Kagi", "https://kagi.com/search?q={q}"),
];

/// The search engine in use: its name and address with {q}.
pub fn engine(s: &Settings) -> (String, String) {
    if s.engine == "custom" && s.custom_engine_url.contains("%s") {
        let name =
            if s.custom_engine_name.trim().is_empty() { "your search engine".into() } else { s.custom_engine_name.clone() };
        return (name, s.custom_engine_url.replace("%s", "{q}"));
    }
    let e = ENGINES.iter().find(|e| e.0 == s.engine).unwrap_or(&ENGINES[0]);
    (e.1.to_string(), e.2.to_string())
}

/// Runs in every page: browser shortcuts, favicon, sound and in-page address changes.
pub const CONTENT_JS: &str = r#"(function(){
if (window.top !== window || window.__halo) return; window.__halo = 1;
const post = m => { try { window.ipc.postMessage(JSON.stringify(m)); } catch (e) {} };
addEventListener('keydown', e => {
  const c = e.ctrlKey || e.metaKey, k = (e.key || '').toLowerCase(); let cmd = null;
  if (c && !e.shiftKey && !e.altKey && k === 't') cmd = 'newtab';
  else if (c && e.shiftKey && k === 't') cmd = 'reopen';
  else if (c && !e.shiftKey && k === 'w') cmd = 'close';
  else if ((c && k === 'l') || (e.altKey && k === 'd') || k === 'f6') cmd = 'focus';
  else if (c && !e.shiftKey && k === 'k') cmd = 'palette';
  else if (c && e.shiftKey && k === 'b') cmd = 'sidebar';
  else if (c && k === 'tab') cmd = e.shiftKey ? 'prev' : 'next';
  else if (c && !e.shiftKey && !e.altKey && k === 'd') cmd = 'bookmark';
  else if (c && !e.shiftKey && !e.altKey && k === 'j') cmd = 'downloads';
  else if (c && !e.shiftKey && !e.altKey && k === ',') cmd = 'settings';
  else if (c && !e.shiftKey && !e.altKey && /^[1-9]$/.test(k)) cmd = 'tab' + k;
  if (cmd) { e.preventDefault(); e.stopImmediatePropagation(); post({ t: 'key', cmd }); }
}, true);
function icon() {
  const l = document.querySelector('link[rel~="icon"][href],link[rel="apple-touch-icon"][href]');
  const h = l ? l.href : (location.protocol.indexOf('http') === 0 && location.hostname !== 'halo.localhost' ? location.origin + '/favicon.ico' : '');
  post({ t: 'icon', href: h });
}
addEventListener('DOMContentLoaded', icon); addEventListener('load', icon);
const media = () => post({ t: 'media', on: Array.from(document.querySelectorAll('video,audio')).some(m => !m.paused && !m.muted && m.volume > 0) });
['play', 'pause', 'ended', 'volumechange', 'emptied'].forEach(ev => document.addEventListener(ev, media, true));
const url = () => post({ t: 'url', href: location.href });
for (const f of ['pushState', 'replaceState']) { const o = history[f]; history[f] = function () { const r = o.apply(this, arguments); url(); return r; }; }
addEventListener('popstate', url); addEventListener('hashchange', url);
})();"#;

pub enum Ev {
    Ui(String),
    Content(u32, String),
    Title(u32, String),
    Nav(u32, String),
    Loaded(u32, String),
    NewWindow(String),
    Upgrade(u32, String),
    DlStart(String, PathBuf),
    DlDone(String, Option<PathBuf>, bool),
    ExtUnpacked(String, Result<PathBuf, String>),
    ExtAdded(String, PathBuf, Result<crate::ext::EngineExt, String>),
    ExtList(Vec<crate::ext::EngineExt>),
    ExtChanged,
}

/// What the page handlers (which may run on other threads) need to read.
pub struct Shared {
    pub newtab_json: String,
    pub bg_mime: String,
    pub dir: PathBuf,
    pub settings: Settings,
    pub ext_icons: HashMap<String, PathBuf>,
}

pub struct Tab {
    pub id: u32,
    pub url: String,
    pub title: String,
    pub icon: String,
    pub loading: bool,
    pub pinned: bool,
    pub media: bool,
    pub zoom: f64,
    pub view: Option<WebView>,
    pub last_active: Instant,
}

impl Tab {
    pub fn new(id: u32, url: &str, title: &str, pinned: bool, zoom: f64) -> Tab {
        Tab {
            id,
            url: url.into(),
            title: title.into(),
            icon: String::new(),
            loading: false,
            pinned,
            media: false,
            zoom,
            view: None,
            last_active: Instant::now(),
        }
    }
}

pub struct App {
    pub window: Window,
    pub ui: WebView,
    pub ctx: WebContext,
    pub proxy: EventLoopProxy<Ev>,
    pub shared: Arc<Mutex<Shared>>,
    pub dir: PathBuf,
    pub store: Store,
    /// Settings as they were at start (engine switches, proxy).
    pub boot: Settings,
    pub tabs: Vec<Tab>,
    pub active: u32,
    pub next_id: u32,
    pub rect: (f64, f64, f64, f64),
    pub overlay: bool,
    pub ui_ready: bool,
    pub closed: Vec<SessionTab>,
    pub dirty: bool,
    pub last_save: Instant,
    pub last_sleep_check: Instant,
    pub shot: bool,
    pub quit: bool,
    pub restart: bool,
    pub last_load_done: u64,
    pub installing: Vec<String>,
    pub ext_error: String,
    pub next_download: u64,
}

pub fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

pub fn is_internal(url: &str) -> bool {
    url.is_empty() || url.starts_with("http://halo.localhost/")
}

fn encode_query(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Address bar rule: a web address opens, anything else is searched.
pub fn resolve(settings: &Settings, text: &str) -> String {
    let t = text.trim();
    if t.is_empty() {
        return NEWTAB.into();
    }
    let lower = t.to_lowercase();
    if let Some(rest) = lower.strip_prefix("halo://") {
        let rest = rest.trim_end_matches('/');
        // halo://settings/downloads opens that section of the Settings page
        if let Some(section) = rest.strip_prefix("settings/") {
            return format!("{SETTINGS}#{section}");
        }
        return format!("http://halo.localhost/{rest}");
    }
    for p in [
        "http://", "https://", "file://", "about:", "data:", "view-source:", "edge://", "chrome://", "extension://",
        "chrome-extension://",
    ] {
        if lower.starts_with(p) {
            return t.into();
        }
    }
    if !t.contains(char::is_whitespace) {
        let hostport = t.split(['/', '?', '#']).next().unwrap_or("");
        let host = hostport.split(':').next().unwrap_or("");
        if host == "localhost" || host.parse::<std::net::Ipv4Addr>().is_ok() {
            return format!("http://{t}");
        }
        if let Some(tld) = host.rsplit('.').next() {
            if host.contains('.')
                && !host.starts_with('.')
                && tld.len() >= 2
                && tld.chars().all(|c| c.is_ascii_alphabetic())
                && host.chars().all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
            {
                return format!("https://{t}");
            }
        }
    }
    engine(settings).1.replace("{q}", &encode_query(t))
}

/// HTTPS-only mode: plain http addresses on the public web are opened over https.
pub fn https_upgrade(url: &str) -> Option<String> {
    let rest = url.strip_prefix("http://")?;
    let host = rest.split(['/', ':', '?', '#']).next().unwrap_or("");
    let local = host == "localhost"
        || host.ends_with(".localhost")
        || host.ends_with(".local")
        || host.parse::<std::net::Ipv4Addr>().map(|ip| ip.is_private() || ip.is_loopback()).unwrap_or(false)
        || !host.contains('.');
    if local { None } else { Some(format!("https://{rest}")) }
}

fn proxy_config(s: &str) -> Option<ProxyConfig> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let (socks, rest) = if let Some(r) = s.strip_prefix("socks5://") {
        (true, r)
    } else {
        (false, s.strip_prefix("http://").unwrap_or(s))
    };
    let (host, port) = rest.trim_end_matches('/').rsplit_once(':')?;
    if host.is_empty() || port.parse::<u16>().is_err() {
        return None;
    }
    let ep = ProxyEndpoint { host: host.to_string(), port: port.to_string() };
    Some(if socks { ProxyConfig::Socks5(ep) } else { ProxyConfig::Http(ep) })
}

fn mime_of(p: &Path) -> &'static str {
    match p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase().as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        _ => "application/octet-stream",
    }
}

/// Serves our own pages (frame, new tab, settings), the background picture and extension icons.
pub fn protocol(
    shared: Arc<Mutex<Shared>>,
) -> impl Fn(WebViewId, Request<Vec<u8>>) -> Response<Cow<'static, [u8]>> + Send + Sync + 'static {
    move |_id, req| {
        let path = req.uri().path().to_string();
        let query = req.uri().query().unwrap_or("").to_string();
        let html = "text/html; charset=utf-8".to_string();
        let (status, mime, body): (u16, String, Cow<'static, [u8]>) = match path.as_str() {
            "/ui" => (200, html, Cow::Borrowed(UI_HTML.as_bytes())),
            "/newtab" => (200, html, Cow::Borrowed(NEWTAB_HTML.as_bytes())),
            "/settings" => (200, html, Cow::Borrowed(SETTINGS_HTML.as_bytes())),
            "/themes.js" => (200, "text/javascript; charset=utf-8".into(), Cow::Borrowed(THEMES_JS.as_bytes())),
            "/newtab.json" => {
                let s = shared.lock().unwrap();
                (200, "application/json".into(), Cow::Owned(s.newtab_json.clone().into_bytes()))
            }
            "/bg" => {
                let (dir, mime) = {
                    let s = shared.lock().unwrap();
                    (s.dir.clone(), s.bg_mime.clone())
                };
                match std::fs::read(Store::bg_path(&dir)) {
                    Ok(b) => (200, mime, Cow::Owned(b)),
                    Err(_) => (404, "text/plain".into(), Cow::Borrowed(b"no picture".as_slice())),
                }
            }
            "/exticon" => {
                let id = query.strip_prefix("id=").unwrap_or("").to_string();
                let p = shared.lock().unwrap().ext_icons.get(&id).cloned();
                match p.and_then(|p| std::fs::read(&p).ok().map(|b| (b, p))) {
                    Some((b, p)) => (200, mime_of(&p).into(), Cow::Owned(b)),
                    None => (404, "text/plain".into(), Cow::Borrowed(b"no icon".as_slice())),
                }
            }
            _ => (404, "text/plain".into(), Cow::Borrowed(b"not found".as_slice())),
        };
        Response::builder()
            .status(status)
            .header(CONTENT_TYPE, mime)
            .header("Cache-Control", "no-store")
            .header("Access-Control-Allow-Origin", "*")
            .body(body)
            .unwrap()
    }
}

pub fn app_icon() -> Option<Icon> {
    // A small round gradient mark, drawn in code.
    let n = 32u32;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - 15.5;
            let dy = y as f32 - 15.5;
            let d = (dx * dx + dy * dy).sqrt();
            let a = ((15.5 - d) * 2.0).clamp(0.0, 1.0);
            let t = (dy.atan2(dx) / std::f32::consts::TAU + 0.5).fract();
            let r = (94.0 + 150.0 * t) as u8;
            let g = (234.0 - 120.0 * t) as u8;
            let b = (212.0 + 30.0 * (1.0 - t)) as u8;
            px.extend_from_slice(&[r, g, b, (a * 255.0) as u8]);
        }
    }
    Icon::from_rgba(px, n, n).ok()
}

/// Options every web view must share (they all live in one engine process).
pub fn engine_options<'a>(b: WebViewBuilder<'a>, boot: &Settings) -> WebViewBuilder<'a> {
    let b = match proxy_config(&boot.proxy) {
        Some(p) => b.with_proxy_config(p),
        None => b,
    };
    #[cfg(windows)]
    let b = {
        use wry::WebViewBuilderExtWindows;
        let mut args = BROWSER_ARGS.to_string();
        if !boot.hardware_acceleration {
            args.push_str(" --disable-gpu --disable-gpu-compositing");
        }
        b.with_additional_browser_args(args).with_browser_extensions_enabled(boot.extensions_enabled)
    };
    b
}

pub fn unique_path(p: PathBuf) -> PathBuf {
    if !p.exists() {
        return p;
    }
    let stem = p.file_stem().and_then(|s| s.to_str()).unwrap_or("download").to_string();
    let ext = p.extension().and_then(|s| s.to_str()).map(|e| format!(".{e}")).unwrap_or_default();
    let dir = p.parent().map(Path::to_path_buf).unwrap_or_default();
    for n in 1..1000 {
        let c = dir.join(format!("{stem} ({n}){ext}"));
        if !c.exists() {
            return c;
        }
    }
    p
}

/// Open a file or folder with Windows, or show it selected in its folder.
pub fn open_path(p: &str, reveal: bool) {
    #[cfg(windows)]
    {
        let mut c = std::process::Command::new("explorer");
        if reveal {
            c.arg(format!("/select,{p}"));
        } else {
            c.arg(p);
        }
        let _ = c.spawn();
    }
    #[cfg(not(windows))]
    {
        let _ = (p, reveal);
    }
}

pub fn default_download_dir() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .map(|p| PathBuf::from(p).join("Downloads"))
        .unwrap_or_else(std::env::temp_dir)
}

pub fn data_dir(shot: bool) -> PathBuf {
    let base = std::env::var_os(if shot { "TEMP" } else { "LOCALAPPDATA" })
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir);
    let dir = base.join(if shot { "HaloBrowserShot" } else { "HaloBrowser" });
    let _ = std::fs::create_dir_all(&dir);
    dir
}
