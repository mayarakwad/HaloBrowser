//! Accounts: "Continue with Google" (OAuth 2.0 for desktop apps: the person signs in
//! in their own web browser, Google sends them back to a one-time address on this
//! computer, PKCE protects the exchange) and local Halo accounts (name, email and a
//! password kept only as a salted, stretched hash).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};

use crate::store::Account;

const AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
/// How long Halo waits for the person to finish signing in in their browser.
const SIGN_IN_WAIT: Duration = Duration::from_secs(300);
/// Rounds of SHA-256 used to stretch local passwords.
const PASSWORD_ROUNDS: u32 = 120_000;

fn client() -> Option<(&'static str, &'static str)> {
    let id = option_env!("HALO_GOOGLE_CLIENT_ID")?;
    if id.is_empty() {
        return None;
    }
    Some((id, option_env!("HALO_GOOGLE_CLIENT_SECRET").unwrap_or("")))
}

/// Whether this build carries a Google sign-in client (from google/*.json).
pub fn google_configured() -> bool {
    client().is_some()
}

fn random_bytes(n: usize) -> Vec<u8> {
    let mut b = vec![0u8; n];
    if getrandom::fill(&mut b).is_err() {
        // Extremely unlikely; fall back to clock-derived bytes.
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap_or_default().as_nanos();
        for (i, x) in b.iter_mut().enumerate() {
            *x = (t >> ((i % 16) * 8)) as u8 ^ (i as u8).wrapping_mul(31);
        }
    }
    b
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

fn dec(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                if let Ok(v) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                    out.push(v);
                    i += 2;
                } else {
                    out.push(b'%');
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Open an address in the person's default web browser.
pub fn open_in_browser(url: &str) {
    #[cfg(windows)]
    {
        use windows::Win32::UI::Shell::ShellExecuteW;
        use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
        use windows::core::HSTRING;
        unsafe {
            ShellExecuteW(None, &HSTRING::from("open"), &HSTRING::from(url), None, None, SW_SHOWNORMAL);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = url;
    }
}

const DONE_PAGE: &str = r#"<!doctype html><meta charset="utf-8"><title>Halo</title>
<body style="margin:0;height:100vh;display:grid;place-items:center;font:16px 'Segoe UI',system-ui,sans-serif;color:#f4f4f5;background:radial-gradient(800px 500px at 10% 0%,rgba(94,234,212,.35),transparent 60%),radial-gradient(700px 500px at 90% 30%,rgba(129,140,248,.4),transparent 60%),#0b1020">
<div style="text-align:center"><div style="width:64px;height:64px;margin:0 auto 18px;border-radius:50%;background:conic-gradient(from 200deg,#5eead4,#f472b6,#60a5fa,#5eead4)"></div>
<h1 style="font-weight:650;margin:0 0 8px">{TITLE}</h1><p style="color:rgba(244,244,245,.7);margin:0">{TEXT}</p></div></body>"#;

fn page(title: &str, text: &str) -> String {
    let body = DONE_PAGE.replace("{TITLE}", title).replace("{TEXT}", text);
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    )
}

fn agent() -> ureq::Agent {
    ureq::Agent::config_builder()
        .tls_config(
            ureq::tls::TlsConfig::builder()
                .provider(ureq::tls::TlsProvider::NativeTls)
                .root_certs(ureq::tls::RootCerts::PlatformVerifier)
                .build(),
        )
        .timeout_global(Some(Duration::from_secs(30)))
        .http_status_as_error(false)
        .build()
        .new_agent()
}

/// Sign in with Google in the person's web browser. Blocks until they finish,
/// cancel (via `cancel`) or five minutes pass.
pub fn google_sign_in(cancel: Arc<AtomicBool>, on_url: &dyn Fn(String)) -> Result<Account, String> {
    let (client_id, client_secret) = client().ok_or_else(|| {
        "Google sign-in isn't set up in this build yet. Add the Google sign-in file to the google folder and build Halo again."
            .to_string()
    })?;
    let listener = TcpListener::bind("127.0.0.1:0").map_err(|e| format!("Couldn't start sign-in ({e})"))?;
    let port = listener.local_addr().map_err(|e| e.to_string())?.port();
    listener.set_nonblocking(true).map_err(|e| e.to_string())?;
    let redirect = format!("http://127.0.0.1:{port}");
    let verifier = URL_SAFE_NO_PAD.encode(random_bytes(48));
    let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
    let state = hex(&random_bytes(16));
    let url = format!(
        "{AUTH_URL}?client_id={}&redirect_uri={}&response_type=code&scope={}&code_challenge={challenge}\
&code_challenge_method=S256&state={state}&prompt=select_account",
        enc(client_id),
        enc(&redirect),
        enc("openid email profile"),
    );
    open_in_browser(&url);
    // Lets the window offer "open the sign-in page again" if the browser didn't come up.
    on_url(url.clone());

    // Wait for Google to send the browser back with a code.
    let deadline = Instant::now() + SIGN_IN_WAIT;
    let code = loop {
        if cancel.load(Ordering::SeqCst) {
            return Err("Sign-in was cancelled.".into());
        }
        if Instant::now() > deadline {
            return Err("Sign-in took too long. If Google said \"Access blocked\", add your Google account as a test user (Google Auth Platform → Audience) or publish the app, then try again.".into());
        }
        let (mut stream, _) = match listener.accept() {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(120));
                continue;
            }
            Err(e) => return Err(format!("Sign-in stopped ({e})")),
        };
        let _ = stream.set_nonblocking(false);
        let _ = stream.set_read_timeout(Some(Duration::from_secs(5)));
        let mut buf = [0u8; 8192];
        let n = stream.read(&mut buf).unwrap_or(0);
        let req = String::from_utf8_lossy(&buf[..n]).to_string();
        let target = req.split_whitespace().nth(1).unwrap_or("/").to_string();
        let query = target.split_once('?').map(|(_, q)| q).unwrap_or("");
        let mut got_code = None;
        let mut got_state = None;
        let mut got_error = None;
        for pair in query.split('&') {
            let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
            match k {
                "code" => got_code = Some(dec(v)),
                "state" => got_state = Some(dec(v)),
                "error" => got_error = Some(dec(v)),
                _ => {}
            }
        }
        if let Some(err) = got_error {
            let _ = stream.write_all(page("Sign-in cancelled", "You can close this tab and go back to Halo.").as_bytes());
            return Err(if err == "access_denied" { "Sign-in was cancelled.".into() } else { format!("Google said: {err}") });
        }
        match (got_code, got_state) {
            (Some(c), Some(s)) if s == state => {
                let _ = stream.write_all(page("You're signed in", "You can close this tab and go back to Halo.").as_bytes());
                break c;
            }
            (Some(_), _) => {
                let _ = stream.write_all(page("Something went wrong", "Please try signing in again from Halo.").as_bytes());
                return Err("The sign-in answer didn't match. Try again.".into());
            }
            _ => {
                // e.g. the browser asking for /favicon.ico
                let _ = stream.write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
            }
        }
    };

    // Exchange the one-time code for the person's basic profile.
    let mut res = agent()
        .post(TOKEN_URL)
        .send_form([
            ("code", code.as_str()),
            ("client_id", client_id),
            ("client_secret", client_secret),
            ("redirect_uri", redirect.as_str()),
            ("grant_type", "authorization_code"),
            ("code_verifier", verifier.as_str()),
        ])
        .map_err(|e| format!("Couldn't reach Google ({e})"))?;
    let text = res.body_mut().read_to_string().map_err(|e| e.to_string())?;
    let v: serde_json::Value = serde_json::from_str(&text).map_err(|_| "Google sent an unexpected answer.".to_string())?;
    if let Some(err) = v["error_description"].as_str().or(v["error"].as_str()) {
        return Err(format!("Google refused the sign-in: {err}"));
    }
    // The ID token came straight from Google over a secure connection, so its
    // contents can be read without checking the signature (OpenID Connect 3.1.3.7).
    let id_token = v["id_token"].as_str().ok_or("Google didn't send a profile.")?;
    let payload = id_token.split('.').nth(1).ok_or("Google sent a damaged profile.")?;
    let claims: serde_json::Value = URL_SAFE_NO_PAD
        .decode(payload.trim_end_matches('='))
        .ok()
        .and_then(|b| serde_json::from_slice(&b).ok())
        .ok_or("Google sent a damaged profile.")?;
    if claims["aud"].as_str() != Some(client_id) {
        return Err("The profile wasn't meant for Halo.".into());
    }
    let email = claims["email"].as_str().unwrap_or("").to_string();
    let name = claims["name"].as_str().map(str::to_string).unwrap_or_else(|| email.split('@').next().unwrap_or("").to_string());
    Ok(Account {
        kind: "google".into(),
        name,
        email,
        picture: claims["picture"].as_str().unwrap_or("").to_string(),
        created: crate::common::now_secs(),
        salt: String::new(),
        hash: String::new(),
    })
}

fn stretch(password: &str, salt: &str) -> String {
    let mut h = Sha256::digest(format!("{salt}:{password}").as_bytes());
    for _ in 0..PASSWORD_ROUNDS {
        let mut d = Sha256::new();
        d.update(h.as_slice());
        d.update(salt.as_bytes());
        d.update(password.as_bytes());
        h = d.finalize();
    }
    hex(h.as_slice())
}

fn valid_email(e: &str) -> bool {
    let e = e.trim();
    match e.split_once('@') {
        Some((a, b)) => !a.is_empty() && b.contains('.') && !b.starts_with('.') && !b.ends_with('.') && !e.contains(' '),
        None => false,
    }
}

/// Create a Halo account kept on this computer.
pub fn create_local(name: &str, email: &str, password: &str) -> Result<Account, String> {
    let name = name.trim();
    let email = email.trim().to_lowercase();
    if name.is_empty() {
        return Err("Please type your name.".into());
    }
    if !valid_email(&email) {
        return Err("Please type a valid email address.".into());
    }
    if password.chars().count() < 8 {
        return Err("Choose a password with at least 8 characters.".into());
    }
    let salt = hex(&random_bytes(16));
    Ok(Account {
        kind: "local".into(),
        name: name.to_string(),
        email,
        picture: String::new(),
        created: crate::common::now_secs(),
        hash: stretch(password, &salt),
        salt,
    })
}

/// Log in to the Halo account kept on this computer.
pub fn log_in_local(saved: Option<&Account>, email: &str, password: &str) -> Result<Account, String> {
    let Some(acc) = saved.filter(|a| a.kind == "local") else {
        return Err("There's no Halo account on this computer yet. Create one instead.".into());
    };
    if !acc.email.eq_ignore_ascii_case(email.trim()) || stretch(password, &acc.salt) != acc.hash {
        return Err("That email and password don't match.".into());
    }
    Ok(acc.clone())
}

/// What a page may see of an account (never the password hash).
pub fn public(a: &Option<Account>) -> serde_json::Value {
    match a {
        Some(a) => serde_json::json!({
            "kind": a.kind, "name": a.name, "email": a.email, "picture": a.picture, "created": a.created,
        }),
        None => serde_json::Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn local_account_round_trip() {
        let a = create_local("Sam", "Sam@Example.com", "correct horse").unwrap();
        assert_eq!(a.email, "sam@example.com");
        assert!(a.hash.len() == 64 && !a.hash.contains("correct"));
        assert!(log_in_local(Some(&a), "sam@example.com", "correct horse").is_ok());
        assert!(log_in_local(Some(&a), "sam@example.com", "wrong").is_err());
        assert!(create_local("Sam", "nope", "12345678").is_err());
        assert!(create_local("Sam", "a@b.co", "short").is_err());
    }

    #[test]
    fn query_decoding() {
        assert_eq!(dec("4%2F0Ab_x%20y+z"), "4/0Ab_x y z");
        assert_eq!(enc("openid email"), "openid%20email");
    }
}
