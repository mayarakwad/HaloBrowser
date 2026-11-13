//! Draws Halo's round gradient icon and embeds it (with version details) into the
//! Windows program, so shortcuts, the taskbar and "Installed apps" show it.

use std::path::Path;

#[path = "src/logo.rs"]
mod logo;

/// Reads the Google sign-in file (an OAuth client of type "Desktop app", downloaded
/// from Google Cloud as client_secret_....json) from the google/ folder, and builds
/// its client id into Halo. Without the file Halo builds fine; Google sign-in is off.
fn google_client() {
    println!("cargo:rerun-if-changed=google");
    println!("cargo:rerun-if-env-changed=HALO_GOOGLE_CLIENT_JSON");
    let text = std::env::var("HALO_GOOGLE_CLIENT_JSON").ok().filter(|t| !t.trim().is_empty()).or_else(|| {
        let dir = Path::new("google");
        let mut files: Vec<_> = std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().map(|e| e.eq_ignore_ascii_case("json")).unwrap_or(false))
            .collect();
        files.sort();
        for f in &files {
            println!("cargo:rerun-if-changed={}", f.display());
        }
        files.first().and_then(|f| std::fs::read_to_string(f).ok())
    });
    let Some(text) = text else {
        println!("cargo:warning=No Google sign-in file in google/ - building without Google sign-in.");
        return;
    };
    let v: serde_json::Value = match serde_json::from_str(text.trim_start_matches('\u{feff}')) {
        Ok(v) => v,
        Err(e) => {
            println!("cargo:warning=The Google sign-in file isn't valid JSON ({e}) - building without Google sign-in.");
            return;
        }
    };
    // Desktop clients are under "installed"; accept "web" too so a wrong pick still builds.
    let c = if v["installed"].is_object() { &v["installed"] } else { &v["web"] };
    let id = c["client_id"].as_str().unwrap_or("");
    let secret = c["client_secret"].as_str().unwrap_or("");
    if id.is_empty() {
        println!("cargo:warning=The Google sign-in file has no client_id - building without Google sign-in.");
        return;
    }
    if v["web"].is_object() {
        println!("cargo:warning=The Google sign-in file is a \"Web application\" client; create a \"Desktop app\" client for Halo.");
    }
    println!("cargo:rustc-env=HALO_GOOGLE_CLIENT_ID={id}");
    println!("cargo:rustc-env=HALO_GOOGLE_CLIENT_SECRET={secret}");
}

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/logo.rs");
    google_client();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").unwrap();
    let icon = Path::new(&out).join("halo.ico");
    std::fs::write(&icon, logo::ico()).unwrap();
    let version = std::env::var("CARGO_PKG_VERSION").unwrap();
    let parts: Vec<u32> = version.split('.').map(|p| p.parse().unwrap_or(0)).collect();
    let (a, b, c) = (parts.first().copied().unwrap_or(0), parts.get(1).copied().unwrap_or(0), parts.get(2).copied().unwrap_or(0));
    let rc = format!(
        r#"1 ICON "{icon}"
1 VERSIONINFO
FILEVERSION {a},{b},{c},0
PRODUCTVERSION {a},{b},{c},0
FILEOS 0x40004
FILETYPE 0x1
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "CompanyName", "Halo"
      VALUE "FileDescription", "Halo browser"
      VALUE "FileVersion", "{version}"
      VALUE "InternalName", "Halo"
      VALUE "OriginalFilename", "Halo.exe"
      VALUE "ProductName", "Halo"
      VALUE "ProductVersion", "{version}"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
"#,
        icon = icon.display().to_string().replace('\\', "\\\\")
    );
    let rc_path = Path::new(&out).join("halo.rc");
    std::fs::write(&rc_path, rc).unwrap();
    // If no resource compiler is installed the program still builds, just without the icon.
    if let Err(e) = embed_resource::compile(&rc_path, embed_resource::NONE).manifest_optional() {
        println!("cargo:warning=icon not embedded: {e}");
    }
}
