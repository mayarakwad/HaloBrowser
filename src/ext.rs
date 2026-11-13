//! Browser extensions: fetch from the Chrome Web Store, unpack, read the
//! manifest, and add / list / switch / remove them in the engine's profile.

use std::io::Read;
use std::path::{Path, PathBuf};

/// Chromium version the Web Store is told we are (the original's version).
const PROD_VERSION: &str = "138.0.7204.303";

/// A Web Store id is 32 letters a–p. Accepts a bare id or a store address.
pub fn store_id_from(text: &str) -> Option<String> {
    let t = text.trim();
    let is_id = |s: &str| s.len() == 32 && s.bytes().all(|b| (b'a'..=b'p').contains(&b));
    if is_id(t) {
        return Some(t.to_string());
    }
    t.split(['/', '?', '#', '&', '='])
        .find(|s| is_id(s))
        .map(|s| s.to_string())
}

pub fn download_crx(store_id: &str) -> Result<Vec<u8>, String> {
    let url = format!(
        "https://clients2.google.com/service/update2/crx?response=redirect&prodversion={PROD_VERSION}\
&acceptformat=crx2,crx3&x=id%3D{store_id}%26installsource%3Dondemand%26uc"
    );
    // Secure connection through Windows' own TLS.
    let agent = ureq::Agent::config_builder()
        .tls_config(ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build())
        .timeout_global(Some(std::time::Duration::from_secs(120)))
        .build()
        .new_agent();
    let mut res = agent.get(&url).call().map_err(|e| format!("could not reach the Chrome Web Store ({e})"))?;
    let mut buf = Vec::new();
    res.body_mut()
        .as_reader()
        .take(200 * 1024 * 1024)
        .read_to_end(&mut buf)
        .map_err(|e| format!("download stopped ({e})"))?;
    if buf.len() < 16 {
        return Err("the Web Store sent nothing for this id".into());
    }
    Ok(buf)
}

/// Where the zip part of a CRX (v2 or v3) starts; 0 for a plain zip.
fn zip_offset(b: &[u8]) -> Result<usize, String> {
    if b.len() >= 4 && &b[0..4] == b"PK\x03\x04" {
        return Ok(0);
    }
    if b.len() < 16 || &b[0..4] != b"Cr24" {
        return Err("this is not an extension package (.crx or .zip)".into());
    }
    let u = |i: usize| u32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]) as usize;
    match u(4) {
        2 => Ok(16 + u(8) + u(12)),
        3 => Ok(12 + u(8)),
        v => Err(format!("unknown package version {v}")),
    }
}

pub fn unpack(bytes: &[u8], dest: &Path) -> Result<(), String> {
    let off = zip_offset(bytes)?;
    if off >= bytes.len() {
        return Err("the package is damaged".into());
    }
    let _ = std::fs::remove_dir_all(dest);
    std::fs::create_dir_all(dest).map_err(|e| e.to_string())?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&bytes[off..])).map_err(|e| format!("the package is damaged ({e})"))?;
    for i in 0..zip.len() {
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        let Some(rel) = f.enclosed_name() else { continue };
        // Store signature data is only meaningful to Chrome's own installer.
        if rel.starts_with("_metadata") {
            continue;
        }
        let out = dest.join(rel);
        if f.is_dir() {
            let _ = std::fs::create_dir_all(&out);
            continue;
        }
        if let Some(p) = out.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        let mut data = Vec::new();
        f.read_to_end(&mut data).map_err(|e| e.to_string())?;
        std::fs::write(&out, data).map_err(|e| e.to_string())?;
    }
    if !dest.join("manifest.json").exists() {
        return Err("the package has no manifest.json".into());
    }
    Ok(())
}

#[derive(Default, Clone)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub popup: String,
    pub options: String,
    pub icon: String,
}

/// Resolve `__MSG_name__` texts from the extension's default language file.
fn localize(folder: &Path, default_locale: &str, s: &str) -> String {
    let Some(key) = s.strip_prefix("__MSG_").and_then(|r| r.strip_suffix("__")) else { return s.to_string() };
    for loc in [default_locale, "en", "en_US"] {
        if loc.is_empty() {
            continue;
        }
        let p = folder.join("_locales").join(loc).join("messages.json");
        if let Ok(txt) = std::fs::read_to_string(&p) {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(txt.trim_start_matches('\u{feff}')) {
                if let Some(obj) = v.as_object() {
                    for (k, m) in obj {
                        if k.eq_ignore_ascii_case(key) {
                            if let Some(msg) = m["message"].as_str() {
                                return msg.to_string();
                            }
                        }
                    }
                }
            }
        }
    }
    key.to_string()
}

pub fn read_manifest(folder: &Path) -> Manifest {
    let Ok(txt) = std::fs::read_to_string(folder.join("manifest.json")) else { return Manifest::default() };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(txt.trim_start_matches('\u{feff}')) else {
        return Manifest::default();
    };
    let loc = v["default_locale"].as_str().unwrap_or("");
    let s = |x: &serde_json::Value| x.as_str().map(|t| localize(folder, loc, t)).unwrap_or_default();
    let popup = ["action", "browser_action", "page_action"]
        .iter()
        .find_map(|k| v[*k]["default_popup"].as_str())
        .unwrap_or("")
        .to_string();
    let options = v["options_ui"]["page"].as_str().or(v["options_page"].as_str()).unwrap_or("").to_string();
    // largest icon listed
    let mut icon = String::new();
    let mut best = 0u32;
    for src in [&v["icons"], &v["action"]["default_icon"], &v["browser_action"]["default_icon"]] {
        if let Some(obj) = src.as_object() {
            for (k, p) in obj {
                let n = k.parse::<u32>().unwrap_or(0);
                if n >= best {
                    if let Some(p) = p.as_str() {
                        best = n;
                        icon = p.trim_start_matches('/').to_string();
                    }
                }
            }
        }
    }
    Manifest {
        name: s(&v["name"]),
        version: v["version"].as_str().unwrap_or("").to_string(),
        description: s(&v["description"]),
        popup: popup.trim_start_matches('/').to_string(),
        options: options.trim_start_matches('/').to_string(),
        icon,
    }
}

pub fn safe_folder_name(s: &str) -> String {
    let n: String = s.chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '-' }).take(40).collect();
    if n.is_empty() { "extension".into() } else { n }
}

/// One extension as the engine reports it.
#[derive(Clone, Debug)]
pub struct EngineExt {
    pub id: String,
    pub name: String,
    pub enabled: bool,
}

#[cfg(windows)]
mod engine {
    use super::EngineExt;
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{
        BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
        ProfileAddBrowserExtensionCompletedHandler, ProfileGetBrowserExtensionsCompletedHandler, take_pwstr,
    };
    use windows::core::{BOOL, HSTRING, Interface, PWSTR};
    use wry::{WebView, WebViewExtWindows};

    fn profile(wv: &WebView) -> Result<ICoreWebView2Profile7, String> {
        unsafe {
            wv.webview()
                .cast::<ICoreWebView2_13>()
                .and_then(|w| w.Profile())
                .and_then(|p| p.cast::<ICoreWebView2Profile7>())
                .map_err(|_| "this WebView2 version has no extension support (update Microsoft Edge WebView2)".to_string())
        }
    }

    fn info(e: &ICoreWebView2BrowserExtension) -> windows::core::Result<EngineExt> {
        unsafe {
            let mut id = PWSTR::null();
            e.Id(&mut id)?;
            let mut name = PWSTR::null();
            e.Name(&mut name)?;
            let mut on = BOOL::default();
            e.IsEnabled(&mut on)?;
            Ok(EngineExt { id: take_pwstr(id), name: take_pwstr(name), enabled: on.as_bool() })
        }
    }

    pub fn add(wv: &WebView, folder: &std::path::Path, done: Box<dyn FnOnce(Result<EngineExt, String>)>) -> Result<(), String> {
        let p = profile(wv)?;
        let path = HSTRING::from(folder);
        let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |hr, ext| {
            let r = match (hr, ext) {
                (Ok(()), Some(e)) => info(&e).map_err(|e| e.to_string()),
                (Err(e), _) => Err(format!("the engine refused it ({})", e.message())),
                _ => Err("the engine refused it".into()),
            };
            done(r);
            Ok(())
        }));
        unsafe { p.AddBrowserExtension(&path, &handler) }.map_err(|e| e.to_string())
    }

    fn with_list(wv: &WebView, f: Box<dyn FnOnce(Vec<(EngineExt, ICoreWebView2BrowserExtension)>)>) -> Result<(), String> {
        let p = profile(wv)?;
        let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(move |_hr, list| {
            let mut out = Vec::new();
            if let Some(list) = list {
                let mut n = 0u32;
                unsafe { list.Count(&mut n)? };
                for i in 0..n {
                    let e = unsafe { list.GetValueAtIndex(i)? };
                    if let Ok(x) = info(&e) {
                        out.push((x, e));
                    }
                }
            }
            f(out);
            Ok(())
        }));
        unsafe { p.GetBrowserExtensions(&handler) }.map_err(|e| e.to_string())
    }

    pub fn list(wv: &WebView, done: Box<dyn FnOnce(Vec<EngineExt>)>) -> Result<(), String> {
        with_list(wv, Box::new(move |v| done(v.into_iter().map(|(x, _)| x).collect())))
    }

    pub fn remove(wv: &WebView, id: String, done: Box<dyn FnOnce()>) -> Result<(), String> {
        with_list(
            wv,
            Box::new(move |v| {
                if let Some((_, e)) = v.into_iter().find(|(x, _)| x.id == id) {
                    let h = BrowserExtensionRemoveCompletedHandler::create(Box::new(move |_| {
                        done();
                        Ok(())
                    }));
                    let _ = unsafe { e.Remove(&h) };
                } else {
                    done();
                }
            }),
        )
    }

    pub fn enable(wv: &WebView, id: String, on: bool, done: Box<dyn FnOnce()>) -> Result<(), String> {
        with_list(
            wv,
            Box::new(move |v| {
                if let Some((_, e)) = v.into_iter().find(|(x, _)| x.id == id) {
                    let h = BrowserExtensionEnableCompletedHandler::create(Box::new(move |_| {
                        done();
                        Ok(())
                    }));
                    let _ = unsafe { e.Enable(on, &h) };
                } else {
                    done();
                }
            }),
        )
    }
}

#[cfg(windows)]
pub use engine::{add, enable, list, remove};

#[cfg(not(windows))]
mod engine_stub {
    use super::EngineExt;
    use wry::WebView;
    const NO: &str = "extensions need Windows (WebView2)";
    pub fn add(_: &WebView, _: &std::path::Path, _: Box<dyn FnOnce(Result<EngineExt, String>)>) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn list(_: &WebView, _: Box<dyn FnOnce(Vec<EngineExt>)>) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn remove(_: &WebView, _: String, _: Box<dyn FnOnce()>) -> Result<(), String> {
        Err(NO.into())
    }
    pub fn enable(_: &WebView, _: String, _: bool, _: Box<dyn FnOnce()>) -> Result<(), String> {
        Err(NO.into())
    }
}
#[cfg(not(windows))]
pub use engine_stub::{add, enable, list, remove};

pub fn extensions_dir(data: &Path) -> PathBuf {
    data.join("Extensions")
}
