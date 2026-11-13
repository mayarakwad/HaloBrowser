#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
//! Halo: a light Rust browser shell. Web pages run on Blink + V8 (through the
//! WebView2 runtime on Windows); the frame (tabs, address bar, themes,
//! settings, downloads, extensions) is ours.

mod account;
mod app;
mod capture;
mod common;
mod ext;
mod features;
mod logo;
#[cfg(windows)]
mod setup;
mod store;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use common::*;
use store::Store;
use tao::dpi::LogicalSize;
use tao::event::{Event, StartCause, WindowEvent};
use tao::event_loop::{ControlFlow, EventLoopBuilder};
use tao::window::{Window, WindowBuilder};
use wry::dpi::{PhysicalPosition, PhysicalSize};
use wry::{Rect, WebContext, WebViewBuilder};

fn main() {
    let args: Vec<String> = std::env::args().collect();

    // --export-logo <folder>: write the logo as PNG pictures, an .ico and an .svg.
    if let Some(dir) = args.iter().position(|a| a == "--export-logo").and_then(|i| args.get(i + 1)) {
        match export_logo(std::path::Path::new(dir)) {
            Ok(n) => println!("wrote {n} logo files to {dir}"),
            Err(e) => eprintln!("could not write the logo: {e}"),
        }
        return;
    }

    // Installer and uninstaller: "HaloSetup.exe" (or --setup) opens the setup wizard,
    // --uninstall is what Windows runs from "Installed apps".
    #[cfg(windows)]
    {
        let named_setup = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_stem().map(|s| s.to_string_lossy().to_lowercase()))
            .map(|s| s.contains("setup") || s.contains("install"))
            .unwrap_or(false);
        let shot = args.iter().position(|a| a == "--screenshot").and_then(|i| args.get(i + 1)).map(PathBuf::from);
        if args.iter().any(|a| a == "--uninstall") {
            setup::run(setup::Mode::Uninstall, shot);
            return;
        }
        if named_setup || args.iter().any(|a| a == "--setup") {
            setup::run(setup::Mode::Install, shot);
            return;
        }
    }
    let mut shot_path: Option<PathBuf> = None;
    let mut open_urls: Vec<String> = Vec::new();
    let mut look: Vec<(String, String)> = Vec::new();
    let mut install: Vec<String> = Vec::new();
    let mut i = 1;
    while i < args.len() {
        if args[i] == "--screenshot" && i + 1 < args.len() {
            shot_path = Some(PathBuf::from(&args[i + 1]));
            i += 2;
            continue;
        }
        if (args[i] == "--theme" || args[i] == "--layout") && i + 1 < args.len() {
            look.push((args[i][2..].to_string(), args[i + 1].clone()));
            i += 2;
            continue;
        }
        if args[i] == "--install-ext" && i + 1 < args.len() {
            install.push(args[i + 1].clone());
            i += 2;
            continue;
        }
        if !args[i].starts_with("--") {
            open_urls.push(args[i].clone());
        }
        i += 1;
    }
    let shot = shot_path.is_some();
    let dir = data_dir(shot);
    let mut store = if shot { Store::default() } else { Store::load(&dir) };
    for (k, v) in &look {
        match k.as_str() {
            "theme" => store.settings.theme = v.clone(),
            _ => store.settings.layout = v.clone(),
        }
    }
    if shot {
        // Test runs never write into the person's own Downloads folder.
        store.settings.download_dir = dir.join("Downloads").display().to_string();
    }
    let boot = store.settings.clone();

    let event_loop = EventLoopBuilder::<Ev>::with_user_event().build();
    let proxy = event_loop.create_proxy();

    let mut wb = WindowBuilder::new()
        .with_title("Halo")
        .with_inner_size(LogicalSize::new(1280.0, 800.0))
        .with_min_inner_size(LogicalSize::new(560.0, 380.0))
        .with_decorations(false)
        .with_window_icon(app_icon());
    #[cfg(windows)]
    {
        use tao::platform::windows::WindowBuilderExtWindows;
        wb = wb.with_undecorated_shadow(true);
    }
    if shot {
        wb = wb.with_focused(false);
        #[cfg(windows)]
        {
            wb = wb.with_always_on_bottom(true);
        }
    }
    let window = wb.build(&event_loop).expect("could not open the window");

    let shared = Arc::new(Mutex::new(Shared {
        newtab_json: "{}".into(),
        bg_mime: "image/jpeg".into(),
        dir: dir.clone(),
        settings: store.settings.clone(),
        ext_icons: HashMap::new(),
    }));
    let mut ctx = WebContext::new(Some(dir.join("WebData")));

    let size = window.inner_size();
    let ui_proxy = proxy.clone();
    let ui_builder = WebViewBuilder::new_with_web_context(&mut ctx)
        .with_url(UI_URL)
        .with_bounds(Rect {
            position: PhysicalPosition::new(0, 0).into(),
            size: PhysicalSize::new(size.width, size.height).into(),
        })
        .with_focused(!shot)
        .with_background_color((11, 16, 32, 255))
        .with_custom_protocol("halo".into(), protocol(shared.clone()))
        .with_ipc_handler(move |req| {
            let _ = ui_proxy.send_event(Ev::Ui(req.body().clone()));
        });
    let ui_builder = engine_options(ui_builder, &boot);
    #[cfg(windows)]
    let ui_builder = {
        use wry::WebViewBuilderExtWindows;
        ui_builder.with_browser_accelerator_keys(false)
    };
    let ui = ui_builder.build_as_child(&window).expect("could not start the web engine (WebView2)");

    // "Clear cookies and site data when I quit": done as the browser starts again,
    // before any site loads, so it is never cut short by the exit.
    if !shot && boot.clear_site_data_on_exit {
        let _ = ui.clear_all_browsing_data();
    }

    let next_download = store.downloads.iter().map(|d| d.id + 1).max().unwrap_or(1);
    // Anything still marked as downloading was interrupted by the last exit.
    for d in store.downloads.iter_mut().filter(|d| d.state == "downloading") {
        d.state = "failed".into();
    }

    let mut app = App {
        window,
        ui,
        ctx,
        proxy,
        shared,
        dir,
        store,
        boot,
        tabs: Vec::new(),
        active: 0,
        next_id: 1,
        rect: (258.0, 49.0, 1012.0, 742.0),
        overlay: false,
        ui_ready: false,
        closed: Vec::new(),
        dirty: false,
        last_save: Instant::now(),
        last_sleep_check: Instant::now(),
        shot,
        quit: false,
        restart: false,
        last_load_done: 0,
        installing: Vec::new(),
        ext_error: String::new(),
        next_download,
        account_cancel: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        account_wait: false,
        account_error: String::new(),
        account_url: String::new(),
    };
    app.refresh_shared();
    if app.boot.extensions_enabled {
        app.ext_refresh();
        for id in &install {
            app.ext_install_store(id);
        }
    }

    // Start-up: reopen the last session lazily (tabs come back asleep and load when
    // opened), or open the home page, or a new tab.
    let startup = app.store.settings.startup.clone();
    if !shot && startup == "restore" && app.store.settings.restore_session && !app.store.session.is_empty() {
        let session = app.store.session.clone();
        let zoom = app.store.settings.default_zoom.clamp(0.25, 5.0);
        for t in &session {
            let id = app.next_id;
            app.next_id += 1;
            app.tabs.push(Tab::new(id, &t.url, &t.title, t.pinned, zoom));
        }
        let idx = app.store.session_active.min(app.tabs.len() - 1);
        app.active = app.tabs[idx].id;
    }
    if open_urls.is_empty() {
        if app.tabs.is_empty() {
            let home = app.store.settings.home_url.trim().to_string();
            if !shot && startup == "home" && !home.is_empty() {
                let url = resolve(&app.store.settings, &home);
                app.new_tab(&url, true);
            } else {
                app.new_tab(NEWTAB, true);
            }
        } else {
            let id = app.active;
            app.activate(id, true);
        }
    } else {
        for u in &open_urls {
            let url = resolve(&app.store.settings, u);
            app.new_tab(&url, true);
        }
    }

    let mut frames: u64 = 0;
    let mut shot_done = false;
    let mut last_poll = Instant::now();
    event_loop.run(move |event, _, control_flow| {
        match event {
            Event::NewEvents(StartCause::ResumeTimeReached { .. }) => {
                frames += 1;
                app.last_load_done += 1;
            }
            Event::WindowEvent { event: WindowEvent::CloseRequested, .. } => app.quit = true,
            Event::WindowEvent { event: WindowEvent::Resized(size), .. } => {
                let _ = app.ui.set_bounds(Rect {
                    position: PhysicalPosition::new(0, 0).into(),
                    size: PhysicalSize::new(size.width, size.height).into(),
                });
            }
            Event::UserEvent(e) => app.handle(e),
            _ => {}
        }

        if shot_done {
            *control_flow = ControlFlow::Exit;
            return;
        }
        if let Some(path) = &shot_path {
            // Screenshot run: draw about 120 frames (and let the first page settle), save, exit.
            let page_done = app.index(app.active).map(|i| !app.tabs[i].loading).unwrap_or(true);
            if (frames >= 120 && app.ui_ready && page_done && app.last_load_done >= 40 && app.installing.is_empty()
                && !app.store.downloads.iter().any(|d| d.state == "downloading"))
                || frames >= 1800
            {
                match capture::window_png(window_handle(&app.window), path) {
                    Ok(()) => println!("saved {}", path.display()),
                    Err(e) => eprintln!("screenshot failed: {e}"),
                }
                shot_done = true;
                *control_flow = ControlFlow::Exit;
                return;
            }
            *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(16));
            return;
        }

        if app.quit {
            app.on_exit();
            *control_flow = ControlFlow::Exit;
            return;
        }
        if last_poll.elapsed() >= Duration::from_millis(900) {
            last_poll = Instant::now();
            app.poll_downloads();
        }
        if app.last_sleep_check.elapsed() >= Duration::from_secs(30) {
            app.last_sleep_check = Instant::now();
            app.sleep_tabs(false);
        }
        if app.dirty && app.last_save.elapsed() >= Duration::from_secs(2) {
            app.save();
        }
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(1000));
    });
}

#[cfg(windows)]
fn window_handle(w: &Window) -> isize {
    use tao::platform::windows::WindowExtWindows;
    w.hwnd()
}

#[cfg(not(windows))]
fn window_handle(_w: &Window) -> isize {
    0
}

fn export_logo(dir: &std::path::Path) -> Result<usize, String> {
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let mut n = 0;
    for size in [16u32, 32, 48, 64, 128, 256, 512, 1024] {
        let path = dir.join(format!("halo-logo-{size}.png"));
        let file = std::fs::File::create(&path).map_err(|e| e.to_string())?;
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size, size);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().map_err(|e| e.to_string())?;
        w.write_image_data(&logo::draw(size)).map_err(|e| e.to_string())?;
        n += 1;
    }
    std::fs::write(dir.join("halo.ico"), logo::ico()).map_err(|e| e.to_string())?;
    std::fs::write(dir.join("halo-logo.svg"), logo::svg()).map_err(|e| e.to_string())?;
    Ok(n + 2)
}
