//! Follows each download inside the page engine: bytes received, total size,
//! why it stopped, and pause / resume / cancel. (The page library only reports
//! start and finish, so this listens to the engine directly.)

use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use tao::event_loop::EventLoopProxy;
use wry::WebView;

use crate::common::Ev;

/// Running downloads by their address, so the Downloads page can pause or cancel them.
#[cfg(windows)]
pub type Ops = Rc<RefCell<HashMap<String, webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2DownloadOperation>>>;
#[cfg(not(windows))]
pub type Ops = Rc<RefCell<HashMap<String, ()>>>;

pub fn new_ops() -> Ops {
    Rc::new(RefCell::new(HashMap::new()))
}

/// Plain words for why a download stopped.
#[cfg(windows)]
fn reason_text(r: webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON) -> (&'static str, &'static str) {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    match r {
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_CANCELED | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_SHUTDOWN => {
            ("cancelled", "Cancelled")
        }
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_MALICIOUS => ("blocked", "Blocked: this file may harm your computer"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_BLOCKED_BY_POLICY => ("blocked", "Blocked by a Windows setting"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_SECURITY_CHECK_FAILED => ("blocked", "Blocked: the virus check failed"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NO_SPACE => ("failed", "The disk is full"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_ACCESS_DENIED => ("failed", "Halo isn't allowed to save in that folder"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_NAME_TOO_LONG => ("failed", "The file name is too long"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_FILE_TOO_LARGE => ("failed", "The file is too large for this disk"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_DISCONNECTED => ("failed", "The internet connection dropped"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_TIMEOUT => ("failed", "The connection timed out"),
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_FAILED | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_NETWORK_SERVER_DOWN => {
            ("failed", "Network error")
        }
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_FORBIDDEN | COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_UNAUTHORIZED => {
            ("failed", "The website refused the download")
        }
        COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_SERVER_BAD_CONTENT => ("failed", "The file is no longer on the website"),
        _ => ("failed", "Failed"),
    }
}

/// Start following the downloads of one page view.
#[cfg(windows)]
pub fn watch(view: &WebView, proxy: EventLoopProxy<Ev>, ops: Ops) {
    use webview2_com::Microsoft::Web::WebView2::Win32::*;
    use webview2_com::{BytesReceivedChangedEventHandler, DownloadStartingEventHandler, StateChangedEventHandler, take_pwstr};
    use windows::core::{Interface, PWSTR};
    use wry::WebViewExtWindows;

    let Ok(wv4) = view.webview().cast::<ICoreWebView2_4>() else { return };
    let handler = DownloadStartingEventHandler::create(Box::new(move |_, args| {
        let Some(args) = args else { return Ok(()) };
        let op = unsafe { args.DownloadOperation()? };
        let uri = unsafe {
            let mut p = PWSTR::null();
            op.Uri(&mut p)?;
            take_pwstr(p)
        };
        ops.borrow_mut().insert(uri.clone(), op.clone());

        let (p1, u1) = (proxy.clone(), uri.clone());
        let progress = BytesReceivedChangedEventHandler::create(Box::new(move |op, _| {
            if let Some(op) = op {
                let (mut got, mut total) = (0i64, 0i64);
                unsafe {
                    let _ = op.BytesReceived(&mut got);
                    let _ = op.TotalBytesToReceive(&mut total);
                }
                let _ = p1.send_event(Ev::DlProgress(u1.clone(), got.max(0) as u64, total.max(0) as u64));
            }
            Ok(())
        }));
        let (p2, u2, ops2) = (proxy.clone(), uri.clone(), ops.clone());
        let state = StateChangedEventHandler::create(Box::new(move |op, _| {
            if let Some(op) = op {
                let mut st = COREWEBVIEW2_DOWNLOAD_STATE::default();
                unsafe {
                    let _ = op.State(&mut st);
                }
                if st == COREWEBVIEW2_DOWNLOAD_STATE_INTERRUPTED {
                    let mut why = COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON::default();
                    unsafe {
                        let _ = op.InterruptReason(&mut why);
                    }
                    if why == COREWEBVIEW2_DOWNLOAD_INTERRUPT_REASON_USER_PAUSED {
                        return Ok(());
                    }
                    let (kind, text) = reason_text(why);
                    let _ = p2.send_event(Ev::DlStopped(u2.clone(), kind.into(), text.into()));
                }
                if st != COREWEBVIEW2_DOWNLOAD_STATE_IN_PROGRESS {
                    ops2.borrow_mut().remove(&u2);
                }
            }
            Ok(())
        }));
        let mut t = 0i64;
        unsafe {
            let _ = op.add_BytesReceivedChanged(&progress, &mut t);
            let _ = op.add_StateChanged(&state, &mut t);
        }
        Ok(())
    }));
    let mut token = 0i64;
    unsafe {
        let _ = wv4.add_DownloadStarting(&handler, &mut token);
    }
}

#[cfg(not(windows))]
pub fn watch(_view: &WebView, _proxy: EventLoopProxy<Ev>, _ops: Ops) {}

/// pause, resume or cancel a running download; false when it isn't running any more.
#[cfg(windows)]
pub fn control(ops: &Ops, uri: &str, action: &str) -> bool {
    let Some(op) = ops.borrow().get(uri).cloned() else { return false };
    unsafe {
        match action {
            "pause" => op.Pause().is_ok(),
            "resume" => {
                let mut can = windows::core::BOOL::default();
                let _ = op.CanResume(&mut can);
                can.as_bool() && op.Resume().is_ok()
            }
            "cancel" => op.Cancel().is_ok(),
            _ => false,
        }
    }
}

#[cfg(not(windows))]
pub fn control(_ops: &Ops, _uri: &str, _action: &str) -> bool {
    false
}
