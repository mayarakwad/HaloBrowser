//! Saves a picture of the browser window (used by `--screenshot <file.png>`).

use std::path::Path;

#[cfg(windows)]
pub fn window_png(hwnd: isize, path: &Path) -> Result<(), String> {
    use windows::Win32::Foundation::{HWND, RECT};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
        DeleteObject, GetDC, GetDIBits, HGDIOBJ, ReleaseDC, SelectObject,
    };
    use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    unsafe {
        let hwnd = HWND(hwnd as *mut core::ffi::c_void);
        let mut r = RECT::default();
        GetClientRect(hwnd, &mut r).map_err(|e| e.to_string())?;
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        if w <= 0 || h <= 0 {
            return Err("window has no size".into());
        }
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        let bmp = CreateCompatibleBitmap(screen, w, h);
        let old = SelectObject(mem, HGDIOBJ(bmp.0));
        // PW_CLIENTONLY | PW_RENDERFULLCONTENT: includes the GPU-drawn web pages.
        let ok = PrintWindow(hwnd, mem, PRINT_WINDOW_FLAGS(1 | 2)).as_bool();
        let mut info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut buf = vec![0u8; (w * h * 4) as usize];
        SelectObject(mem, old);
        let lines = GetDIBits(mem, bmp, 0, h as u32, Some(buf.as_mut_ptr() as *mut _), &mut info, DIB_RGB_COLORS);
        let _ = DeleteObject(HGDIOBJ(bmp.0));
        let _ = DeleteDC(mem);
        ReleaseDC(None, screen);
        if !ok || lines == 0 {
            return Err("could not read the window".into());
        }
        for px in buf.chunks_exact_mut(4) {
            px.swap(0, 2);
            px[3] = 255;
        }
        write_png(path, w as u32, h as u32, &buf)
    }
}

#[cfg(not(windows))]
pub fn window_png(_hwnd: isize, _path: &Path) -> Result<(), String> {
    Err("screenshots are only supported on Windows".into())
}

#[allow(dead_code)]
fn write_png(path: &Path, w: u32, h: u32, rgba: &[u8]) -> Result<(), String> {
    if let Some(p) = path.parent() {
        let _ = std::fs::create_dir_all(p);
    }
    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut wr = enc.write_header().map_err(|e| e.to_string())?;
    wr.write_image_data(rgba).map_err(|e| e.to_string())?;
    Ok(())
}
