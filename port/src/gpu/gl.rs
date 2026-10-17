//! OpenGL 3.3 core upscaler for the 640×480 picture: AMD FSR 1.0 (EASU into an intermediate texture of the letterbox
//! rect's size, then RCAS into the framebuffer), or one linear / nearest pass. Shaders in `port/shaders/`
//! (FSR: MIT, `shaders/LICENSE-FSR.txt`).
//!
//! The caller owns the context (SDL here) and loads the function pointers with [`load`] before [`Upscaler::new`].
//! Every method needs that context current. The pass order leaves room for a later full-resolution overlay (an RGBA
//! texture blended into the rect after RCAS / the blit) without changing the passes.
use super::{letterbox, Rect};
use gl::types::{GLchar, GLenum, GLint, GLsizei, GLuint};
use std::ffi::{c_void, CString};

/// How the picture is scaled to the window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scale {
    /// FSR 1.0: EASU + RCAS (scales below 1× fall back to linear)
    Fsr,
    Linear,
    Nearest,
}

impl Scale {
    pub fn parse(s: &str) -> Option<Scale> {
        match s {
            "fsr" => Some(Scale::Fsr),
            "linear" => Some(Scale::Linear),
            "nearest" => Some(Scale::Nearest),
            _ => None,
        }
    }
    pub fn next(self) -> Scale {
        match self {
            Scale::Fsr => Scale::Linear,
            Scale::Linear => Scale::Nearest,
            Scale::Nearest => Scale::Fsr,
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Scale::Fsr => "fsr",
            Scale::Linear => "linear",
            Scale::Nearest => "nearest",
        }
    }
}

/// Load the GL function pointers (e.g. `|s| video.gl_get_proc_address(s) as *const _`).
pub fn load(f: impl FnMut(&'static str) -> *const c_void) {
    gl::load_with(f);
}

struct Prog {
    id: GLuint,
}

impl Prog {
    fn new(vs: &str, fs: &str) -> Result<Prog, String> {
        unsafe {
            let v = shader(gl::VERTEX_SHADER, vs)?;
            let f = shader(gl::FRAGMENT_SHADER, fs).map_err(|e| {
                gl::DeleteShader(v);
                e
            })?;
            let id = gl::CreateProgram();
            gl::AttachShader(id, v);
            gl::AttachShader(id, f);
            gl::LinkProgram(id);
            gl::DeleteShader(v);
            gl::DeleteShader(f);
            let mut ok = 0;
            gl::GetProgramiv(id, gl::LINK_STATUS, &mut ok);
            if ok == 0 {
                let log = info_log(id, false);
                gl::DeleteProgram(id);
                return Err(format!("link: {log}"));
            }
            Ok(Prog { id })
        }
    }
    fn loc(&self, name: &str) -> GLint {
        let c = CString::new(name).unwrap();
        unsafe { gl::GetUniformLocation(self.id, c.as_ptr()) }
    }
}

impl Drop for Prog {
    fn drop(&mut self) {
        unsafe { gl::DeleteProgram(self.id) }
    }
}

unsafe fn shader(kind: GLenum, src: &str) -> Result<GLuint, String> {
    let s = gl::CreateShader(kind);
    let c = CString::new(src).map_err(|e| e.to_string())?;
    gl::ShaderSource(s, 1, &c.as_ptr(), std::ptr::null());
    gl::CompileShader(s);
    let mut ok = 0;
    gl::GetShaderiv(s, gl::COMPILE_STATUS, &mut ok);
    if ok == 0 {
        let log = info_log(s, true);
        gl::DeleteShader(s);
        return Err(format!("compile: {log}"));
    }
    Ok(s)
}

unsafe fn info_log(id: GLuint, is_shader: bool) -> String {
    let mut len = 0;
    if is_shader {
        gl::GetShaderiv(id, gl::INFO_LOG_LENGTH, &mut len);
    } else {
        gl::GetProgramiv(id, gl::INFO_LOG_LENGTH, &mut len);
    }
    let mut buf = vec![0u8; len.max(1) as usize];
    let p = buf.as_mut_ptr() as *mut GLchar;
    if is_shader {
        gl::GetShaderInfoLog(id, len, std::ptr::null_mut(), p);
    } else {
        gl::GetProgramInfoLog(id, len, std::ptr::null_mut(), p);
    }
    String::from_utf8_lossy(&buf).trim_end_matches('\0').trim().to_string()
}

const VS: &str = include_str!("../../shaders/fullscreen.vert");
const EASU: &str = include_str!("../../shaders/fsr_easu.frag");
const RCAS: &str = include_str!("../../shaders/fsr_rcas.frag");
const BLIT: &str = include_str!("../../shaders/blit.frag");

/// The GL objects of the upscaler. Create, use and drop it with its context current.
pub struct Upscaler {
    src_w: u32,
    src_h: u32,
    easu: Prog,
    rcas: Prog,
    blit: Prog,
    vao: GLuint,
    /// the picture (RGB8)
    src: GLuint,
    /// EASU output (RGB10_A2), resized to the letterbox rect on demand
    mid: GLuint,
    mid_size: (i32, i32),
    mid_fbo: GLuint,
    /// RCAS strength in stops (0 = strongest; FSR samples default to 0.2)
    pub sharpness: f32,
}

impl Upscaler {
    /// Compile the shaders and create the textures for a `src_w`×`src_h` RGB picture.
    pub fn new(src_w: u32, src_h: u32) -> Result<Upscaler, String> {
        if !gl::CreateShader::is_loaded() {
            return Err("GL functions not loaded".into());
        }
        let easu = Prog::new(VS, EASU).map_err(|e| format!("EASU {e}"))?;
        let rcas = Prog::new(VS, RCAS).map_err(|e| format!("RCAS {e}"))?;
        let blit = Prog::new(VS, BLIT).map_err(|e| format!("blit {e}"))?;
        unsafe {
            let mut vao = 0;
            gl::GenVertexArrays(1, &mut vao);
            let src = texture();
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGB8 as GLint,
                src_w as GLsizei,
                src_h as GLsizei,
                0,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            let mid = texture();
            let mut mid_fbo = 0;
            gl::GenFramebuffers(1, &mut mid_fbo);
            for (p, name) in [(&easu, "src"), (&rcas, "src"), (&blit, "src")] {
                gl::UseProgram(p.id);
                gl::Uniform1i(p.loc(name), 0);
            }
            let e = gl::GetError();
            if e != gl::NO_ERROR {
                return Err(format!("GL error {e:#x} at setup"));
            }
            Ok(Upscaler { src_w, src_h, easu, rcas, blit, vao, src, mid, mid_size: (0, 0), mid_fbo, sharpness: 0.2 })
        }
    }

    /// Upload the picture: `src_w`×`src_h` RGB, rows top-down.
    pub fn upload(&mut self, rgb: &[u8]) {
        assert_eq!(rgb.len(), (self.src_w * self.src_h * 3) as usize);
        unsafe {
            gl::BindTexture(gl::TEXTURE_2D, self.src);
            gl::PixelStorei(gl::UNPACK_ALIGNMENT, 1);
            gl::TexSubImage2D(
                gl::TEXTURE_2D,
                0,
                0,
                0,
                self.src_w as GLsizei,
                self.src_h as GLsizei,
                gl::RGB,
                gl::UNSIGNED_BYTE,
                rgb.as_ptr() as *const c_void,
            );
        }
    }

    /// Draw the uploaded picture into framebuffer `fbo` (0 = the window) of `size` pixels: black bars, the picture
    /// scaled into the centred 4:3 rect. Returns the rect (top-left origin).
    pub fn draw(&mut self, fbo: GLuint, size: (u32, u32), mode: Scale) -> Rect {
        let r = letterbox(size.0, size.1, self.src_w, self.src_h);
        // GL viewports are bottom-up
        let gy = size.1 as i32 - r.y - r.h;
        unsafe {
            gl::Disable(gl::BLEND);
            gl::Disable(gl::DEPTH_TEST);
            gl::Disable(gl::SCISSOR_TEST);
            gl::BindVertexArray(self.vao);
            gl::ActiveTexture(gl::TEXTURE0);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::Viewport(0, 0, size.0 as GLsizei, size.1 as GLsizei);
            gl::ClearColor(0.0, 0.0, 0.0, 1.0);
            gl::Clear(gl::COLOR_BUFFER_BIT);
            if r.w <= 0 || r.h <= 0 {
                return r;
            }
            let fsr = mode == Scale::Fsr && r.w as u32 >= self.src_w && r.h as u32 >= self.src_h;
            if fsr {
                // pass 1: EASU, picture → mid (rect-sized, row 0 = top)
                self.ensure_mid(r.w, r.h);
                gl::BindFramebuffer(gl::FRAMEBUFFER, self.mid_fbo);
                gl::Viewport(0, 0, r.w, r.h);
                gl::BindTexture(gl::TEXTURE_2D, self.src);
                gl::UseProgram(self.easu.id);
                let (sx, sy) = (self.src_w as f32 / r.w as f32, self.src_h as f32 / r.h as f32);
                gl::Uniform4f(self.easu.loc("con0"), sx, sy, 0.5 * sx - 0.5, 0.5 * sy - 0.5);
                gl::DrawArrays(gl::TRIANGLES, 0, 3);
                // pass 2: RCAS, mid → the rect of the target
                gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
                gl::Viewport(r.x, gy, r.w, r.h);
                gl::BindTexture(gl::TEXTURE_2D, self.mid);
                gl::UseProgram(self.rcas.id);
                gl::Uniform2i(self.rcas.loc("origin"), r.x, gy);
                gl::Uniform1f(self.rcas.loc("sharp"), (-self.sharpness).exp2());
                gl::DrawArrays(gl::TRIANGLES, 0, 3);
            } else {
                let filter = if mode == Scale::Nearest { gl::NEAREST } else { gl::LINEAR } as GLint;
                gl::BindTexture(gl::TEXTURE_2D, self.src);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, filter);
                gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, filter);
                gl::Viewport(r.x, gy, r.w, r.h);
                gl::UseProgram(self.blit.id);
                gl::Uniform2f(self.blit.loc("origin"), r.x as f32, gy as f32);
                gl::Uniform2f(self.blit.loc("size"), r.w as f32, r.h as f32);
                gl::DrawArrays(gl::TRIANGLES, 0, 3);
            }
            // (a later full-resolution overlay would be blended into the rect here)
        }
        r
    }

    unsafe fn ensure_mid(&mut self, w: i32, h: i32) {
        if self.mid_size == (w, h) {
            return;
        }
        gl::BindTexture(gl::TEXTURE_2D, self.mid);
        gl::TexImage2D(
            gl::TEXTURE_2D,
            0,
            gl::RGB10_A2 as GLint,
            w,
            h,
            0,
            gl::RGBA,
            gl::UNSIGNED_BYTE,
            std::ptr::null(),
        );
        gl::BindFramebuffer(gl::FRAMEBUFFER, self.mid_fbo);
        gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, self.mid, 0);
        self.mid_size = (w, h);
    }
}

impl Drop for Upscaler {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.mid_fbo);
            gl::DeleteTextures(2, [self.src, self.mid].as_ptr());
            gl::DeleteVertexArrays(1, &self.vao);
        }
    }
}

/// A texture bound to TEXTURE_2D with clamped edges and linear filtering.
unsafe fn texture() -> GLuint {
    let mut t = 0;
    gl::GenTextures(1, &mut t);
    gl::BindTexture(gl::TEXTURE_2D, t);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_S, gl::CLAMP_TO_EDGE as GLint);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_WRAP_T, gl::CLAMP_TO_EDGE as GLint);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MIN_FILTER, gl::LINEAR as GLint);
    gl::TexParameteri(gl::TEXTURE_2D, gl::TEXTURE_MAG_FILTER, gl::LINEAR as GLint);
    t
}

/// An offscreen RGBA8 render target (for tests / examples): `draw` into `fbo`, then [`Target::read_rgb`].
pub struct Target {
    pub fbo: GLuint,
    tex: GLuint,
    pub w: u32,
    pub h: u32,
}

impl Target {
    pub fn new(w: u32, h: u32) -> Target {
        unsafe {
            let tex = texture();
            gl::TexImage2D(
                gl::TEXTURE_2D,
                0,
                gl::RGBA8 as GLint,
                w as GLsizei,
                h as GLsizei,
                0,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                std::ptr::null(),
            );
            let mut fbo = 0;
            gl::GenFramebuffers(1, &mut fbo);
            gl::BindFramebuffer(gl::FRAMEBUFFER, fbo);
            gl::FramebufferTexture2D(gl::FRAMEBUFFER, gl::COLOR_ATTACHMENT0, gl::TEXTURE_2D, tex, 0);
            Target { fbo, tex, w, h }
        }
    }
    /// The pixels as RGB, rows top-down.
    pub fn read_rgb(&self) -> Vec<u8> {
        let (w, h) = (self.w as usize, self.h as usize);
        let mut rgba = vec![0u8; w * h * 4];
        unsafe {
            gl::BindFramebuffer(gl::FRAMEBUFFER, self.fbo);
            gl::PixelStorei(gl::PACK_ALIGNMENT, 1);
            gl::ReadPixels(
                0,
                0,
                w as GLsizei,
                h as GLsizei,
                gl::RGBA,
                gl::UNSIGNED_BYTE,
                rgba.as_mut_ptr() as *mut c_void,
            );
        }
        let mut rgb = Vec::with_capacity(w * h * 3);
        for row in rgba.chunks_exact(w * 4).rev() {
            for p in row.chunks_exact(4) {
                rgb.extend_from_slice(&p[..3]);
            }
        }
        rgb
    }
}

impl Drop for Target {
    fn drop(&mut self) {
        unsafe {
            gl::DeleteFramebuffers(1, &self.fbo);
            gl::DeleteTextures(1, &self.tex);
        }
    }
}
