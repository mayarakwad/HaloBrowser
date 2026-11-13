//! The installation wizard (HaloSetup.exe, or `--setup`) and the uninstaller
//! (`--uninstall`, used by Windows' "Installed apps").
//!
//! Installs for the current user only, so no administrator rights are needed:
//! the program goes to %LOCALAPPDATA%\Programs\Halo, with Start menu and
//! desktop shortcuts, an "Installed apps" entry, and (optionally) registration
//! as a web browser so Windows lets you pick it as the default.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tao::dpi::{LogicalPosition, LogicalSize};
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder, EventLoopProxy};
use tao::window::WindowBuilder;
use winreg::RegKey;
use winreg::enums::HKEY_CURRENT_USER;
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebViewBuilder};

use crate::common::{THEMES_JS, VERSION, data_dir};

const SETUP_HTML: &str = include_str!("setup.html");
const UNINSTALL_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Uninstall\Halo";
const CLIENT_KEY: &str = r"Software\Clients\StartMenuInternet\Halo";
const PROGID_KEY: &str = r"Software\Classes\HaloHTML";
const REGISTERED_APPS: &str = r"Software\RegisteredApplications";
const EXE_NAME: &str = "Halo.exe";
const WEBVIEW2_DOWNLOAD: &str = "https://go.microsoft.com/fwlink/p/?LinkId=2124703";

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
}

enum SEv {
    Ui(String),
    Account(Result<crate::store::Account, String>),
    GoogleUrl(String),
    Progress(u32, String),
    Done(Result<(), String>),
}

/// Where Halo goes unless the person picks another folder.
pub fn default_dir() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Programs")
        .join("Halo")
}

/// The folder Halo is installed in, as Windows knows it.
pub fn installed_dir() -> Option<PathBuf> {
    let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(UNINSTALL_KEY).ok()?;
    let p: String = k.get_value("InstallLocation").ok()?;
    if p.is_empty() { None } else { Some(PathBuf::from(p)) }
}

/// True when this running copy is the installed one.
pub fn running_installed() -> bool {
    match (installed_dir(), std::env::current_exe().ok().and_then(|e| e.parent().map(Path::to_path_buf))) {
        (Some(a), Some(b)) => a.eq(&b) || a.to_string_lossy().eq_ignore_ascii_case(&b.to_string_lossy()),
        _ => false,
    }
}

/// Open Windows' "Default apps" page at Halo, so it can be made the default browser.
pub fn open_default_apps() {
    let _ = std::process::Command::new("explorer").arg("ms-settings:defaultapps?registeredAppUser=Halo").spawn();
}

/// Start the uninstaller of the installed copy.
pub fn start_uninstaller() {
    if let Some(d) = installed_dir() {
        let _ = std::process::Command::new(d.join(EXE_NAME)).arg("--uninstall").spawn();
    }
}

fn known_folder(id: &windows::core::GUID) -> Option<PathBuf> {
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::UI::Shell::{KNOWN_FOLDER_FLAG, SHGetKnownFolderPath};
    unsafe {
        let p = SHGetKnownFolderPath(id, KNOWN_FOLDER_FLAG(0), None).ok()?;
        let s = p.to_string().ok();
        CoTaskMemFree(Some(p.0 as *const _));
        s.map(PathBuf::from)
    }
}

fn desktop_link() -> Option<PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_Desktop).map(|d| d.join("Halo.lnk"))
}

fn start_menu_link() -> Option<PathBuf> {
    known_folder(&windows::Win32::UI::Shell::FOLDERID_Programs).map(|d| d.join("Halo.lnk"))
}

fn make_shortcut(lnk: &Path, target: &Path) -> Result<(), String> {
    use windows::Win32::System::Com::{
        CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx, IPersistFile,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};
    use windows::core::{HSTRING, Interface};
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let link: IShellLinkW =
            CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(|e| e.message().to_string())?;
        let t = HSTRING::from(target);
        link.SetPath(&t).map_err(|e| e.message().to_string())?;
        if let Some(dir) = target.parent() {
            let _ = link.SetWorkingDirectory(&HSTRING::from(dir));
        }
        let _ = link.SetDescription(&HSTRING::from("Halo browser"));
        let _ = link.SetIconLocation(&t, 0);
        let file: IPersistFile = link.cast().map_err(|e| e.message().to_string())?;
        if let Some(p) = lnk.parent() {
            let _ = std::fs::create_dir_all(p);
        }
        file.Save(&HSTRING::from(lnk), true).map_err(|e| e.message().to_string())?;
    }
    Ok(())
}

fn notify_associations_changed() {
    use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

fn reg_err(e: std::io::Error) -> String {
    format!("Windows refused a setting ({e})")
}

fn folder_size_kb(dir: &Path) -> u32 {
    std::fs::read_dir(dir)
        .map(|r| r.flatten().filter_map(|e| e.metadata().ok()).map(|m| m.len()).sum::<u64>() / 1024)
        .unwrap_or(0) as u32
}

/// Register Halo as a web browser Windows can offer as the default.
fn register_browser(exe: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let exe_s = exe.display().to_string();
    let open = format!("\"{exe_s}\" \"%1\"");
    let icon = format!("{exe_s},0");

    let (p, _) = hkcu.create_subkey(PROGID_KEY).map_err(reg_err)?;
    p.set_value("", &"Halo HTML Document").map_err(reg_err)?;
    let (pi, _) = p.create_subkey("DefaultIcon").map_err(reg_err)?;
    pi.set_value("", &icon).map_err(reg_err)?;
    let (pc, _) = p.create_subkey(r"shell\open\command").map_err(reg_err)?;
    pc.set_value("", &open).map_err(reg_err)?;
    let (pa, _) = p.create_subkey("Application").map_err(reg_err)?;
    pa.set_value("ApplicationName", &"Halo").map_err(reg_err)?;
    pa.set_value("ApplicationIcon", &icon).map_err(reg_err)?;

    let (c, _) = hkcu.create_subkey(CLIENT_KEY).map_err(reg_err)?;
    c.set_value("", &"Halo").map_err(reg_err)?;
    let (ci, _) = c.create_subkey("DefaultIcon").map_err(reg_err)?;
    ci.set_value("", &icon).map_err(reg_err)?;
    let (cc, _) = c.create_subkey(r"shell\open\command").map_err(reg_err)?;
    cc.set_value("", &format!("\"{exe_s}\"")).map_err(reg_err)?;
    let (cap, _) = c.create_subkey("Capabilities").map_err(reg_err)?;
    cap.set_value("ApplicationName", &"Halo").map_err(reg_err)?;
    cap.set_value("ApplicationIcon", &icon).map_err(reg_err)?;
    cap.set_value("ApplicationDescription", &"A fast, light web browser built on Blink and V8.").map_err(reg_err)?;
    let (sm, _) = cap.create_subkey("StartMenu").map_err(reg_err)?;
    sm.set_value("StartMenuInternet", &"Halo").map_err(reg_err)?;
    let (urls, _) = cap.create_subkey("URLAssociations").map_err(reg_err)?;
    for scheme in ["http", "https"] {
        urls.set_value(scheme, &"HaloHTML").map_err(reg_err)?;
    }
    let (files, _) = cap.create_subkey("FileAssociations").map_err(reg_err)?;
    for ext in [".htm", ".html", ".shtml", ".xhtml", ".svg", ".webp", ".pdf"] {
        files.set_value(ext, &"HaloHTML").map_err(reg_err)?;
    }
    let (ra, _) = hkcu.create_subkey(REGISTERED_APPS).map_err(reg_err)?;
    ra.set_value("Halo", &format!(r"{CLIENT_KEY}\Capabilities")).map_err(reg_err)?;
    notify_associations_changed();
    Ok(())
}

fn unregister_browser() {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let _ = hkcu.delete_subkey_all(CLIENT_KEY);
    let _ = hkcu.delete_subkey_all(PROGID_KEY);
    if let Ok(ra) = hkcu.open_subkey_with_flags(REGISTERED_APPS, winreg::enums::KEY_SET_VALUE) {
        let _ = ra.delete_value("Halo");
    }
    notify_associations_changed();
}

fn register_uninstall(dir: &Path, exe: &Path) -> Result<(), String> {
    let hkcu = RegKey::predef(HKEY_CURRENT_USER);
    let (k, _) = hkcu.create_subkey(UNINSTALL_KEY).map_err(reg_err)?;
    let exe_s = exe.display().to_string();
    k.set_value("DisplayName", &"Halo").map_err(reg_err)?;
    k.set_value("DisplayVersion", &VERSION).map_err(reg_err)?;
    k.set_value("Publisher", &"Halo").map_err(reg_err)?;
    k.set_value("DisplayIcon", &format!("{exe_s},0")).map_err(reg_err)?;
    k.set_value("InstallLocation", &dir.display().to_string()).map_err(reg_err)?;
    k.set_value("UninstallString", &format!("\"{exe_s}\" --uninstall")).map_err(reg_err)?;
    k.set_value("QuietUninstallString", &format!("\"{exe_s}\" --uninstall")).map_err(reg_err)?;
    k.set_value("NoModify", &1u32).map_err(reg_err)?;
    k.set_value("NoRepair", &1u32).map_err(reg_err)?;
    k.set_value("EstimatedSize", &folder_size_kb(dir)).map_err(reg_err)?;
    let today = {
        let secs = crate::common::now_secs() as i64;
        // days since 1970 → civil date (Howard Hinnant's algorithm)
        let z = secs / 86400 + 719468;
        let era = z.div_euclid(146097);
        let doe = z - era * 146097;
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + if m <= 2 { 1 } else { 0 };
        format!("{y:04}{m:02}{d:02}")
    };
    k.set_value("InstallDate", &today).map_err(reg_err)?;
    Ok(())
}

struct Opts {
    dir: PathBuf,
    desktop: bool,
    start_menu: bool,
    browser: bool,
    theme: String,
    layout: String,
    engine: String,
    account: Option<crate::store::Account>,
}

fn install(o: &Opts, step: &dyn Fn(u32, &str)) -> Result<PathBuf, String> {
    let pause = || std::thread::sleep(Duration::from_millis(220));
    step(6, "Preparing the folder");
    std::fs::create_dir_all(&o.dir).map_err(|e| format!("Couldn't create {} ({e})", o.dir.display()))?;
    pause();

    step(22, "Copying Halo");
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = o.dir.join(EXE_NAME);
    let same = me.canonicalize().ok() == exe.canonicalize().ok();
    if !same {
        if exe.exists() {
            // An older copy may still be running: move it aside, it is cleaned up next time.
            let old = o.dir.join("Halo.old.exe");
            let _ = std::fs::remove_file(&old);
            let _ = std::fs::rename(&exe, &old);
        }
        std::fs::copy(&me, &exe).map_err(|e| {
            format!("Couldn't copy Halo into {} ({e}). If Halo is open, close it and try again.", o.dir.display())
        })?;
    }
    pause();

    step(42, "Adding shortcuts");
    if o.start_menu {
        if let Some(l) = start_menu_link() {
            make_shortcut(&l, &exe).map_err(|e| format!("Couldn't add the Start menu shortcut ({e})"))?;
        }
    } else if let Some(l) = start_menu_link() {
        let _ = std::fs::remove_file(l);
    }
    if o.desktop {
        if let Some(l) = desktop_link() {
            make_shortcut(&l, &exe).map_err(|e| format!("Couldn't add the desktop shortcut ({e})"))?;
        }
    } else if let Some(l) = desktop_link() {
        let _ = std::fs::remove_file(l);
    }
    pause();

    step(62, "Adding Halo to Installed apps");
    register_uninstall(&o.dir, &exe)?;
    pause();

    if o.browser {
        step(78, "Registering Halo as a web browser");
        register_browser(&exe)?;
    } else {
        unregister_browser();
    }
    pause();

    step(90, "Saving your choices");
    let data = data_dir(false);
    let mut store = crate::store::Store::load(&data);
    store.settings.theme = o.theme.clone();
    store.settings.custom_bg = false;
    store.settings.layout = if o.layout == "horizontal" { "horizontal".into() } else { "vertical".into() };
    store.settings.engine = o.engine.clone();
    // Whoever is signed in when Install is pressed (nobody if they skipped or signed out).
    store.account = o.account.clone();
    if let Some(a) = o.account.as_ref().filter(|a| a.kind == "local") {
        store.local_account = Some(a.clone());
    }
    store.save(&data);
    pause();

    step(100, "Halo is installed");
    Ok(exe)
}

fn uninstall(remove_data: bool, step: &dyn Fn(u32, &str)) -> Result<(), String> {
    let pause = || std::thread::sleep(Duration::from_millis(220));
    step(10, "Removing shortcuts");
    for l in [start_menu_link(), desktop_link()].into_iter().flatten() {
        let _ = std::fs::remove_file(l);
    }
    pause();
    step(35, "Removing Halo from Windows");
    unregister_browser();
    pause();
    if remove_data {
        step(60, "Removing your data");
        let base = std::env::var_os("LOCALAPPDATA").map(PathBuf::from);
        if let Some(b) = base {
            let d = b.join("HaloBrowser");
            if d.ends_with("HaloBrowser") && d.exists() {
                std::fs::remove_dir_all(&d).map_err(|e| {
                    format!("Some of your data is still in use ({e}). Close Halo and run the uninstaller again.")
                })?;
            }
        }
        pause();
    }
    step(85, "Removing program files");
    if let Some(dir) = installed_dir() {
        // Everything except the running uninstaller now; the folder itself goes once it has closed.
        if let Ok(rd) = std::fs::read_dir(&dir) {
            let me = std::env::current_exe().ok();
            for e in rd.flatten() {
                let p = e.path();
                if Some(&p) != me.as_ref() && p.is_file() {
                    let _ = std::fs::remove_file(p);
                }
            }
        }
    }
    let _ = RegKey::predef(HKEY_CURRENT_USER).delete_subkey_all(UNINSTALL_KEY);
    pause();
    step(100, "Halo has been removed");
    Ok(())
}

/// After the uninstaller closes, remove the install folder (only Halo's own folder).
fn remove_folder_after_exit(dir: &Path) {
    let looks_right = dir.file_name().map(|n| n.eq_ignore_ascii_case("Halo")).unwrap_or(false)
        && dir.components().count() > 3;
    if !looks_right {
        return;
    }
    use std::os::windows::process::CommandExt;
    let script = format!("ping 127.0.0.1 -n 3 > nul & rmdir /s /q \"{}\"", dir.display());
    let _ = std::process::Command::new("cmd").args(["/C", &script]).creation_flags(0x0800_0000).spawn();
}

fn message_box(text: &str) {
    use windows::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
    use windows::core::HSTRING;
    unsafe {
        MessageBoxW(None, &HSTRING::from(text), &HSTRING::from("Halo Setup"), MB_OK | MB_ICONWARNING);
    }
}

pub fn run(mode: Mode, shot: Option<PathBuf>) {
    // The wizard and Halo itself both need the WebView2 page engine.
    let engine = wry::webview_version().ok();
    if engine.is_none() {
        message_box(
            "Halo needs the Microsoft Edge WebView2 Runtime, which provides the Blink/V8 page engine.\n\n\
             Your browser will now open Microsoft's download page. Install it, then run Halo Setup again.",
        );
        let _ = std::process::Command::new("explorer").arg(WEBVIEW2_DOWNLOAD).spawn();
        return;
    }

    let event_loop = EventLoopBuilder::<SEv>::with_user_event().build();
    let proxy: EventLoopProxy<SEv> = event_loop.create_proxy();
    let size = LogicalSize::new(780.0, 520.0);
    let mut wb = WindowBuilder::new()
        .with_title(if mode == Mode::Install { "Halo Setup" } else { "Uninstall Halo" })
        .with_inner_size(size)
        .with_resizable(false)
        .with_decorations(false)
        .with_window_icon(crate::common::app_icon());
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        wb = wb.with_undecorated_shadow(true);
    }
    if shot.is_some() {
        wb = wb.with_focused(false).with_always_on_bottom(true);
    }
    let window = wb.build(&event_loop).expect("could not open the setup window");
    if let Some(m) = window.current_monitor() {
        let ms = m.size().to_logical::<f64>(m.scale_factor());
        let mp = m.position().to_logical::<f64>(m.scale_factor());
        window.set_outer_position(LogicalPosition::new(
            mp.x + (ms.width - size.width) / 2.0,
            mp.y + (ms.height - size.height) / 2.0,
        ));
    }

    let existing = installed_dir();
    let saved = crate::store::Store::load(&data_dir(false));
    let init = json!({
        "mode": if mode == Mode::Install { "install" } else { "uninstall" },
        "version": VERSION,
        "engine": engine.unwrap_or_default(),
        "default_dir": default_dir().display().to_string(),
        "installed_dir": existing.as_ref().map(|d| d.display().to_string()),
        "google": crate::account::google_configured(),
        "account": crate::account::public(&saved.account),
        "has_local": saved.local_account.is_some(),
        // test option: open the wizard on a given step (--setup-page account)
        "start": std::env::args().skip_while(|a| a != "--setup-page").nth(1),
    });
    let px = proxy.clone();
    let inner = window.inner_size();
    // A private profile for the wizard's own page, away from the browser's.
    let mut ctx = wry::WebContext::new(Some(std::env::temp_dir().join("HaloSetupWebData")));
    let webview = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_html(SETUP_HTML)
        .with_initialization_script(THEMES_JS)
        .with_initialization_script(format!("window.SETUP = {init};"))
        .with_bounds(Rect {
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new(inner.width, inner.height).into(),
        })
        .with_background_color((11, 16, 32, 255))
        .with_focused(shot.is_none())
        .with_ipc_handler(move |req| {
            let _ = px.send_event(SEv::Ui(req.body().clone()));
        })
        .build_as_child(&window)
        .expect("could not start the page engine");

    let mut installed_exe: Option<PathBuf> = None;
    let mut account: Option<crate::store::Account> = saved.account.clone();
    let local_saved = saved.local_account.clone();
    let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let mut remove_dir: Option<PathBuf> = None;
    let mut launch = false;
    let start = Instant::now();
    let mut frames = 0u32;
    let mut done = false;
    let mut exiting = false;

    event_loop.run(move |event, _, flow| {
        if exiting {
            *flow = ControlFlow::Exit;
            return;
        }
        *flow = if shot.is_some() {
            ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16))
        } else {
            ControlFlow::Wait
        };
        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => frames += 1,
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => done = true,
            Event::UserEvent(SEv::Progress(pct, text)) => {
                let _ = webview.evaluate_script(&format!("setup.progress({pct}, {})", Value::String(text)));
            }
            Event::UserEvent(SEv::Account(r)) => {
                let arg = match r {
                    Ok(a) => {
                        let p = crate::account::public(&Some(a.clone()));
                        account = Some(a);
                        format!("{p}, null")
                    }
                    Err(e) => format!("null, {}", Value::String(e)),
                };
                let _ = webview.evaluate_script(&format!("setup.account({arg})"));
            }
            Event::UserEvent(SEv::GoogleUrl(u)) => {
                let _ = webview.evaluate_script(&format!("setup.googleUrl({})", Value::String(u)));
            }
            Event::UserEvent(SEv::Done(r)) => {
                let arg = match r {
                    Ok(()) => "null".to_string(),
                    Err(e) => Value::String(e).to_string(),
                };
                let _ = webview.evaluate_script(&format!("setup.finished({arg})"));
            }
            Event::UserEvent(SEv::Ui(msg)) => {
                let Ok(v) = serde_json::from_str::<Value>(&msg) else { return };
                match v["cmd"].as_str().unwrap_or("") {
                    "browse" => {
                        if let Some(mut d) = rfd::FileDialog::new().set_title("Choose where to install Halo").pick_folder() {
                            if !d.file_name().map(|n| n.eq_ignore_ascii_case("Halo")).unwrap_or(false) {
                                d = d.join("Halo");
                            }
                            let _ = webview.evaluate_script(&format!(
                                "setup.setDir({})",
                                Value::String(d.display().to_string())
                            ));
                        }
                    }
                    "install" => {
                        let o = Opts {
                            dir: PathBuf::from(v["dir"].as_str().unwrap_or("").trim()),
                            desktop: v["desktop"].as_bool().unwrap_or(true),
                            start_menu: v["startMenu"].as_bool().unwrap_or(true),
                            browser: v["browser"].as_bool().unwrap_or(true),
                            theme: v["theme"].as_str().unwrap_or("aurora").to_string(),
                            layout: v["layout"].as_str().unwrap_or("vertical").to_string(),
                            engine: v["engine"].as_str().unwrap_or("google").to_string(),
                            account: account.clone(),
                        };
                        let o = if o.dir.as_os_str().is_empty() { Opts { dir: default_dir(), ..o } } else { o };
                        installed_exe = Some(o.dir.join(EXE_NAME));
                        let p = proxy.clone();
                        std::thread::spawn(move || {
                            let step = |pct: u32, t: &str| {
                                let _ = p.send_event(SEv::Progress(pct, t.to_string()));
                            };
                            let r = install(&o, &step).map(|_| ());
                            let _ = p.send_event(SEv::Done(r));
                        });
                    }
                    "uninstall" => {
                        remove_dir = installed_dir();
                        let remove_data = v["removeData"].as_bool().unwrap_or(false);
                        let p = proxy.clone();
                        std::thread::spawn(move || {
                            let step = |pct: u32, t: &str| {
                                let _ = p.send_event(SEv::Progress(pct, t.to_string()));
                            };
                            let r = uninstall(remove_data, &step);
                            let _ = p.send_event(SEv::Done(r));
                        });
                    }
                    "google" => {
                        cancel.store(false, std::sync::atomic::Ordering::SeqCst);
                        let (p, c) = (proxy.clone(), cancel.clone());
                        std::thread::spawn(move || {
                            let p2 = p.clone();
                            let on_url = move |u: String| {
                                let _ = p2.send_event(SEv::GoogleUrl(u));
                            };
                            let _ = p.send_event(SEv::Account(crate::account::google_sign_in(c, &on_url)));
                        });
                    }
                    "openUrl" => {
                        if let Some(u) = v["url"].as_str().filter(|u| u.starts_with("https://accounts.google.com/")) {
                            crate::account::open_in_browser(u);
                        }
                    }
                    "googleCancel" => cancel.store(true, std::sync::atomic::Ordering::SeqCst),
                    "createAccount" => {
                        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                        let r = crate::account::create_local(&s("name"), &s("email"), &s("password"));
                        let _ = proxy.send_event(SEv::Account(r));
                    }
                    "login" => {
                        let s = |k: &str| v[k].as_str().unwrap_or("").to_string();
                        let r = crate::account::log_in_local(local_saved.as_ref(), &s("email"), &s("password"));
                        let _ = proxy.send_event(SEv::Account(r));
                    }
                    "signOut" => account = None,
                    "defaultApps" => open_default_apps(),
                    "finish" => {
                        launch = v["launch"].as_bool().unwrap_or(false);
                        done = true;
                    }
                    "close" => done = true,
                    "min" => window.set_minimized(true),
                    "drag" => {
                        let _ = window.drag_window();
                    }
                    _ => {}
                }
            }
            _ => {}
        }

        if let Some(path) = &shot {
            if frames >= 120 || start.elapsed() > Duration::from_secs(20) {
                use tao::platform::windows::WindowExtWindows;
                match crate::capture::window_png(window.hwnd(), path) {
                    Ok(()) => println!("saved {}", path.display()),
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
                exiting = true;
                *flow = ControlFlow::Exit;
            }
            return;
        }
        if done {
            if launch {
                if let Some(exe) = &installed_exe {
                    let _ = std::process::Command::new(exe).spawn();
                }
            }
            if let Some(d) = &remove_dir {
                remove_folder_after_exit(d);
            }
            exiting = true;
            *flow = ControlFlow::Exit;
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shortcut_is_written() {
        let dir = std::env::temp_dir().join("halo-setup-test");
        let _ = std::fs::create_dir_all(&dir);
        let lnk = dir.join("Halo test.lnk");
        let target = std::env::current_exe().unwrap();
        make_shortcut(&lnk, &target).expect("shortcut");
        assert!(std::fs::metadata(&lnk).unwrap().len() > 100);
    }

    #[test]
    fn windows_folders_are_found() {
        assert!(desktop_link().is_some());
        assert!(start_menu_link().unwrap().to_string_lossy().contains("Programs"));
        assert!(default_dir().ends_with(r"Programs\Halo"));
    }
}
