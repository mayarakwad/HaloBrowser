//! Halo's logo: a round mark with a teal → pink → blue sweep and a soft highlight.
//! Shared by the build script (program icon), the window icon and `--export-logo`.

/// The sweep colour at `t` (0..1, clockwise from the top), seamless like the
/// sidebar mark: teal → pink → blue → back to teal, starting at 200°.
pub fn sweep(t: f32) -> (f32, f32, f32) {
    const STOPS: [(f32, f32, f32); 4] =
        [(94.0, 234.0, 212.0), (244.0, 114.0, 182.0), (96.0, 165.0, 250.0), (94.0, 234.0, 212.0)];
    let t = (t - 200.0 / 360.0).rem_euclid(1.0) * 3.0;
    let i = (t.floor() as usize).min(2);
    let f = t - i as f32;
    let f = f * f * (3.0 - 2.0 * f); // smooth between stops
    let (a, b) = (STOPS[i], STOPS[i + 1]);
    (a.0 + (b.0 - a.0) * f, a.1 + (b.1 - a.1) * f, a.2 + (b.2 - a.2) * f)
}

/// One square picture of the logo, RGBA, `n` x `n`, transparent outside the circle.
pub fn draw(n: u32) -> Vec<u8> {
    let c = (n as f32 - 1.0) / 2.0;
    let r = n as f32 / 2.0 - 0.5;
    let edge = (n as f32 / 96.0).max(0.9); // anti-aliasing width
    let mut px = Vec::with_capacity((n * n * 4) as usize);
    for y in 0..n {
        for x in 0..n {
            let dx = x as f32 - c;
            let dy = y as f32 - c;
            let d = (dx * dx + dy * dy).sqrt();
            let a = ((r - d) / edge + 0.5).clamp(0.0, 1.0);
            // angle clockwise from the top, 0..1
            let t = (dx.atan2(-dy) / std::f32::consts::TAU).rem_euclid(1.0);
            let (red, green, blue) = sweep(t);
            // soft light from the top-left, and a faint inner halo ring
            let hl = (1.0 - ((dx + c * 0.45).powi(2) + (dy + c * 0.45).powi(2)).sqrt() / (n as f32 * 0.9)).clamp(0.0, 1.0)
                * 0.30;
            let ring = if n >= 32 { (-((d / r - 0.64) / 0.07).powi(2)).exp() * 0.22 } else { 0.0 };
            let w = (hl + ring).min(0.6);
            let lift = |v: f32| (v + (255.0 - v) * w).min(255.0) as u8;
            px.extend_from_slice(&[lift(red), lift(green), lift(blue), (a * 255.0) as u8]);
        }
    }
    px
}
/// A Windows .ico with 32-bit pictures at the usual sizes.
pub fn ico() -> Vec<u8> {
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

/// The logo as a scalable drawing (SVG). SVG has no sweep gradient, so the colour
/// sweep is built from 72 thin wedges, with the same highlight on top.
#[allow(dead_code)]
pub fn svg() -> String {
    let mut s = String::from(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 512 512\" width=\"512\" height=\"512\">\n\
<defs><clipPath id=\"c\"><circle cx=\"256\" cy=\"256\" r=\"256\"/></clipPath>\n\
<radialGradient id=\"h\" cx=\"0.275\" cy=\"0.275\" r=\"0.9\"><stop offset=\"0\" stop-color=\"#fff\" stop-opacity=\"0.35\"/>\
<stop offset=\"1\" stop-color=\"#fff\" stop-opacity=\"0\"/></radialGradient></defs>\n<g clip-path=\"url(#c)\">\n",
    );
    let steps = 72;
    for i in 0..steps {
        let t0 = i as f32 / steps as f32;
        let t1 = (i + 1) as f32 / steps as f32;
        let t = (t0 + t1) / 2.0;
        let (r, g, b) = sweep(t);
        // t is clockwise from the top; SVG angles are clockwise from the right
        let ang = |t: f32| t * std::f32::consts::TAU - std::f32::consts::FRAC_PI_2;
        let (a0, a1) = (ang(t0) - 0.004, ang(t1) + 0.004);
        let p = |a: f32| (256.0 + 400.0 * a.cos(), 256.0 + 400.0 * a.sin());
        let (x0, y0) = p(a0);
        let (x1, y1) = p(a1);
        s.push_str(&format!(
            "<path d=\"M256 256 L{x0:.1} {y0:.1} L{x1:.1} {y1:.1} Z\" fill=\"#{:02x}{:02x}{:02x}\"/>\n",
            r as u8, g as u8, b as u8
        ));
    }
    s.push_str("<circle cx=\"256\" cy=\"256\" r=\"256\" fill=\"url(#h)\"/>\n");
    s.push_str("<circle cx=\"256\" cy=\"256\" r=\"164\" fill=\"none\" stroke=\"#fff\" stroke-opacity=\"0.18\" stroke-width=\"36\"/>\n</g>\n</svg>\n");
    s
}
