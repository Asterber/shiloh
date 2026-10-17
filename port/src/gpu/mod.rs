//! Presentation of the 640×480 picture in a window of any size: letterbox layout and the mouse mapping back to
//! 640×480 (pure math, always built), and with the `sdl` feature an OpenGL 3.3 upscaler ([`gl`]: AMD FSR 1.0 EASU +
//! RCAS, or plain linear / nearest scaling).
#[cfg(feature = "sdl")]
pub mod gl;

/// A rectangle in framebuffer pixels (origin top-left, y down).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

/// The largest `src_w`:`src_h` rectangle centred in a `dst_w`×`dst_h` drawable (letterbox / pillarbox bars around).
pub fn letterbox(dst_w: u32, dst_h: u32, src_w: u32, src_h: u32) -> Rect {
    let (dw, dh, sw, sh) = (dst_w as u64, dst_h as u64, src_w.max(1) as u64, src_h.max(1) as u64);
    let (w, h) = if dw * sh <= dh * sw {
        (dw, (dw * sh + sw / 2) / sw) // width-limited: bars top and bottom
    } else {
        ((dh * sw + sh / 2) / sh, dh) // height-limited: bars left and right
    };
    let (w, h) = (w.min(dw), h.min(dh));
    Rect { x: ((dw - w) / 2) as i32, y: ((dh - h) / 2) as i32, w: w as i32, h: h as i32 }
}

/// A mouse position in window coordinates (points; SDL's event x / y) → a pixel of the `src_w`×`src_h` picture shown
/// letterboxed in the drawable. `win` is the window size in points, `drawable` its size in pixels (larger on high-DPI
/// screens). Positions on the bars (outside the picture) → `None`.
pub fn window_to_picture(
    x: i32,
    y: i32,
    win: (u32, u32),
    drawable: (u32, u32),
    src_w: u32,
    src_h: u32,
) -> Option<(i32, i32)> {
    if win.0 == 0 || win.1 == 0 {
        return None;
    }
    let r = letterbox(drawable.0, drawable.1, src_w, src_h);
    if r.w <= 0 || r.h <= 0 {
        return None;
    }
    // the centre of the point, in drawable pixels
    let px = (x as f64 + 0.5) * drawable.0 as f64 / win.0 as f64;
    let py = (y as f64 + 0.5) * drawable.1 as f64 / win.1 as f64;
    let gx = ((px - r.x as f64) * src_w as f64 / r.w as f64).floor();
    let gy = ((py - r.y as f64) * src_h as f64 / r.h as f64).floor();
    if gx < 0.0 || gy < 0.0 || gx >= src_w as f64 || gy >= src_h as f64 {
        return None;
    }
    Some((gx as i32, gy as i32))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn letterbox_exact_and_bars() {
        assert_eq!(letterbox(1280, 960, 640, 480), Rect { x: 0, y: 0, w: 1280, h: 960 });
        // 16:10 high-DPI full screen: pillarbox
        assert_eq!(letterbox(2880, 1800, 640, 480), Rect { x: 240, y: 0, w: 2400, h: 1800 });
        // tall window: letterbox
        assert_eq!(letterbox(800, 1000, 640, 480), Rect { x: 0, y: 200, w: 800, h: 600 });
        // non-integer scale keeps 4:3 within a pixel
        let r = letterbox(1001, 700, 640, 480);
        assert_eq!((r.w, r.h), (933, 700));
        assert_eq!(r.x, 34);
    }

    #[test]
    fn mouse_plain_2x() {
        let m = |x, y| window_to_picture(x, y, (1280, 960), (1280, 960), 640, 480);
        assert_eq!(m(0, 0), Some((0, 0)));
        assert_eq!(m(1, 1), Some((0, 0)));
        assert_eq!(m(2, 3), Some((1, 1)));
        assert_eq!(m(1279, 959), Some((639, 479)));
        assert_eq!(m(1280, 960), None);
        assert_eq!(m(-1, 0), None);
    }

    #[test]
    fn mouse_high_dpi_points_vs_pixels() {
        // window 1280×960 points, drawable 2560×1920 pixels (Retina): mapping goes through the pixel size
        let m = |x, y| window_to_picture(x, y, (1280, 960), (2560, 1920), 640, 480);
        assert_eq!(m(0, 0), Some((0, 0)));
        assert_eq!(m(640, 480), Some((320, 240)));
        assert_eq!(m(1279, 959), Some((639, 479)));
    }

    #[test]
    fn mouse_pillarbox_bars_go_nowhere() {
        // 1440×900 points, 2880×1800 pixels: picture at x 240..2640 px = 120..1320 points
        let m = |x, y| window_to_picture(x, y, (1440, 900), (2880, 1800), 640, 480);
        assert_eq!(m(119, 450), None);
        assert_eq!(m(120, 0), Some((0, 0)));
        assert_eq!(m(720, 450), Some((320, 240)));
        assert_eq!(m(1319, 899), Some((639, 479)));
        assert_eq!(m(1320, 450), None);
    }

    #[test]
    fn mouse_letterbox_bars_go_nowhere() {
        let m = |x, y| window_to_picture(x, y, (800, 1000), (800, 1000), 640, 480);
        assert_eq!(m(400, 199), None);
        assert_eq!(m(400, 200), Some((320, 0)));
        assert_eq!(m(0, 799), Some((0, 479)));
        assert_eq!(m(0, 800), None);
        assert_eq!(window_to_picture(5, 5, (0, 0), (0, 0), 640, 480), None);
    }
}
