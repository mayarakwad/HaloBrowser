//! Draws Halo's round gradient icon and embeds it (with version details) into the
//! Windows program, so shortcuts, the taskbar and "Installed apps" show it.

use std::path::Path;

/// One square picture of the icon, RGBA, `n` x `n`.
fn draw(n: u32) -> Vec<u8> {
    let c = (n as f32 - 1.0) / 2.0;
    let r = n as f32 / 2.0 - 0.5;
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let a = ((r - d) * 1.5).clamp(0.0, 1.0);
            let t = (dy.atan2(dx) / std::f32::consts::TAU + 0.5).fract();
            // soft highlight toward the top-left
            let hl = (1.0 - ((dx + c * 0.45).powi(2) + (dy + c * 0.45).powi(2)).sqrt() / (n as f32 * 0.9)).clamp(0.0, 1.0) * 0.35;
            let lift = |v: f32| (v + (255.0 - v) * hl).min(255.0) as u8;
            let red = 94.0 + 150.0 * t;
            let green = 234.0 - 120.0 * t;
            let blue = 212.0 + 30.0 * (1.0 - t);
            px.extend_from_slice(&[lift(red), lift(green), lift(blue), (a * 255.0) as u8]);
        }
    }
    px
}

/// A Windows .ico with 32-bit pictures at the usual sizes.
fn ico() -> Vec<u8> {
    let sizes = [16u32, 24, 32, 48, 64, 128, 256];
    let mut images: Vec<Vec<u8>> = Vec::new();
    for &n in &sizes {
        let rgba = draw(n);
        let mut b = Vec::new();
        // BITMAPINFOHEADER (height counts the colour and mask halves)
        b.extend_from_slice(&40u32.to_le_bytes());
        b.extend_from_slice(&(n as i32).to_le_bytes());
        b.extend_from_slice(&((n * 2) as i32).to_le_bytes());
        b.extend_from_slice(&1u16.to_le_bytes());
        b.extend_from_slice(&32u16.to_le_bytes());
        b.extend_from_slice(&[0u8; 24]);
        // colour rows, bottom-up, BGRA
        for y in (0..n).rev() {
            for x in 0..n {
                let i = ((y * n + x) * 4) as usize;
                b.extend_from_slice(&[rgba[i + 2], rgba[i + 1], rgba[i], rgba[i + 3]]);
            }
        }
        // transparency mask (unused with 32-bit pictures), rows padded to 4 bytes
        let row = (n.div_ceil(32) * 4) as usize;
        b.extend(std::iter::repeat_n(0u8, row * n as usize));
        images.push(b);
    }
    let mut out = Vec::new();
    out.extend_from_slice(&[0, 0, 1, 0]);
    out.extend_from_slice(&(sizes.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * sizes.len() as u32;
    for (i, &n) in sizes.iter().enumerate() {
        let side = if n >= 256 { 0 } else { n as u8 };
        out.extend_from_slice(&[side, side, 0, 0]);
        out.extend_from_slice(&1u16.to_le_bytes());
        out.extend_from_slice(&32u16.to_le_bytes());
        out.extend_from_slice(&(images[i].len() as u32).to_le_bytes());
        out.extend_from_slice(&offset.to_le_bytes());
        offset += images[i].len() as u32;
    }
    for im in images {
        out.extend_from_slice(&im);
    }
    out
}

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
    google_client();
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out = std::env::var("OUT_DIR").unwrap();
    let icon = Path::new(&out).join("halo.ico");
    std::fs::write(&icon, ico()).unwrap();
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
