//! fsr IN.ppm OUTDIR [--sharpness STOPS] [--bench]
//!
//! Offscreen check of the window's upscaler (src/gpu/gl.rs) on a hidden SDL window's OpenGL 3.3 context: the 640×480
//! picture IN.ppm (e.g. a frame of `xf --frames N --dump DIR`) scaled 2× and 3× with FSR (EASU + RCAS), linear and
//! nearest → OUTDIR/{fsr,linear,nearest}-{2,3}x.ppm, and FSR into a 16:9
//! target (pillarbox) → OUTDIR/fsr-1600x900.ppm. `--bench`: ms per frame (upload + passes, glFinish) at several
//! window sizes.
use std::time::Instant;
use xf::gpu::gl::{load, Scale, Target, Upscaler};

fn read_ppm(p: &str) -> (u32, u32, Vec<u8>) {
    let b = std::fs::read(p).unwrap_or_else(|e| panic!("{p}: {e}"));
    let mut fields = Vec::new();
    let mut i = 0;
    while fields.len() < 4 {
        while b[i].is_ascii_whitespace() {
            i += 1;
        }
        if b[i] == b'#' {
            while b[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        let s = i;
        while !b[i].is_ascii_whitespace() {
            i += 1;
        }
        fields.push(String::from_utf8_lossy(&b[s..i]).into_owned());
    }
    assert_eq!(fields[0], "P6", "{p}: not a binary PPM");
    let (w, h): (u32, u32) = (fields[1].parse().unwrap(), fields[2].parse().unwrap());
    (w, h, b[i + 1..i + 1 + (w * h * 3) as usize].to_vec())
}

fn write_ppm(p: &std::path::Path, w: u32, h: u32, rgb: &[u8]) {
    let mut out = format!("P6\n{w} {h}\n255\n").into_bytes();
    out.extend_from_slice(rgb);
    std::fs::write(p, out).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
}

fn main() {
    let a: Vec<String> = std::env::args().skip(1).collect();
    if a.len() < 2 {
        eprintln!("usage: fsr IN.ppm OUTDIR [--sharpness STOPS] [--bench]");
        std::process::exit(2);
    }
    let sharpness: f32 = a.iter().position(|x| x == "--sharpness").map(|i| a[i + 1].parse().unwrap()).unwrap_or(0.2);
    let bench = a.iter().any(|x| x == "--bench");
    let (w, h, rgb) = read_ppm(&a[0]);
    let out = std::path::Path::new(&a[1]);
    std::fs::create_dir_all(out).unwrap();

    let sdl = sdl2::init().unwrap();
    let video = sdl.video().unwrap();
    let attr = video.gl_attr();
    attr.set_context_profile(sdl2::video::GLProfile::Core);
    attr.set_context_version(3, 3);
    attr.set_context_flags().forward_compatible().set();
    let win = video.window("fsr", 64, 64).opengl().hidden().build().unwrap();
    let _ctx = win.gl_create_context().unwrap();
    load(|s| video.gl_get_proc_address(s) as *const _);
    let _ = video.gl_set_swap_interval(0);
    unsafe {
        let v = std::ffi::CStr::from_ptr(gl::GetString(gl::RENDERER) as *const _);
        let ver = std::ffi::CStr::from_ptr(gl::GetString(gl::VERSION) as *const _);
        println!("GL renderer {:?}, version {:?}", v, ver);
    }
    let mut up = Upscaler::new(w, h).unwrap_or_else(|e| panic!("upscaler: {e}"));
    up.sharpness = sharpness;
    up.upload(&rgb);
    for k in [2u32, 3] {
        let t = Target::new(w * k, h * k);
        for m in [Scale::Fsr, Scale::Linear, Scale::Nearest] {
            up.draw(t.fbo, (t.w, t.h), m);
            let p = out.join(format!("{}-{k}x.ppm", m.name()));
            write_ppm(&p, t.w, t.h, &t.read_rgb());
            println!("{}", p.display());
        }
    }
    // a 16:9 window: pillarbox bars, the picture centred (rect 1200×900 at x 200)
    let t = Target::new(1600, 900);
    up.draw(t.fbo, (t.w, t.h), Scale::Fsr);
    let p = out.join("fsr-1600x900.ppm");
    write_ppm(&p, t.w, t.h, &t.read_rgb());
    println!("{}", p.display());
    if bench {
        // 2×, 3×, a 14" MacBook Pro full screen (3024×1964 px), a 4K screen
        for (tw, th) in [(1280u32, 960u32), (1920, 1440), (3024, 1964), (3840, 2160)] {
            let t = Target::new(tw, th);
            for m in [Scale::Fsr, Scale::Linear] {
                let run = |up: &mut Upscaler, n: u32| {
                    let t0 = Instant::now();
                    for _ in 0..n {
                        up.upload(&rgb);
                        up.draw(t.fbo, (t.w, t.h), m);
                        unsafe { gl::Finish() };
                    }
                    t0.elapsed().as_secs_f64() * 1000.0 / n as f64
                };
                run(&mut up, 20);
                let ms = run(&mut up, 300);
                println!("{tw}x{th} {:<7} {ms:.3} ms/frame (upload + passes + glFinish)", m.name());
            }
        }
    }
}
