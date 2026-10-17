//! PCM cache for sound tracks the port does not decode itself: 'QDMC' (QDesign Music, 14 files of data/InstalledGame:
//! XS/72106–72108.mus, XS/80788.mus, XS/89487.amv and 9 XV movies incl. the New Game intro XV/64421.xmv = asset 0xfba5).
//! The user runs `xf-prep` once (`src/bin/xf-prep.rs`): it decodes each such track with the user's own ffmpeg
//! (`-ignore_editlist 1`, so the cache holds the whole *media* of the track, sample rate and channels unchanged) and
//! writes here; no decoder code is part of the port. The engine then reads the cached media and applies the track's
//! length and edit list itself ([`super::audio::place`]), exactly as for a decoded IMA4 track.
//!
//! Layout: `<dir>/qdmc-v1/<key>.wav` (RIFF WAVE, 16-bit PCM, interleaved) + `<key>.json` (source size, FNV-1a 64 hash
//! of the whole source file, rate, channels, frames, ffmpeg version). `key` = the movie's directory and file name in
//! lower case (`xv/64421.xmv`). An entry is used only when size and hash match the movie being played.
//!
//! The cache dir: [`set_dir`] (`--cache DIR` of xf / xf-serve / xf-prep), else `XF_CACHE`, else the platform user cache
//! dir + `/xf` (macOS `~/Library/Caches/xf`, Linux `$XDG_CACHE_HOME/xf` or `~/.cache/xf`, Windows
//! `%LOCALAPPDATA%\xf\cache`). Never inside the game's data dir. Reads go through [`crate::io`], so the web front gets
//! the files from xf-serve (`/cache/…`, the web build sets the dir to `cache`).
use super::audio::Pcm;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

/// Sub-directory of the cache dir (bump when the layout changes).
pub const SUBDIR: &str = "qdmc-v1";

static DIR: RwLock<Option<PathBuf>> = RwLock::new(None);

/// Use `p` as the cache dir from now on.
pub fn set_dir(p: impl Into<PathBuf>) {
    *DIR.write().unwrap() = Some(p.into());
}

/// The cache dir: [`set_dir`], else `XF_CACHE`, else [`default_dir`].
pub fn dir() -> Option<PathBuf> {
    if let Some(d) = DIR.read().unwrap().clone() {
        return Some(d);
    }
    std::env::var_os("XF_CACHE").filter(|v| !v.is_empty()).map(PathBuf::from).or_else(default_dir)
}

/// The platform's per-user cache dir + `xf` (None when it cannot be determined, e.g. in the web build).
pub fn default_dir() -> Option<PathBuf> {
    let env = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Caches/xf"))
    } else if cfg!(windows) {
        env("LOCALAPPDATA").map(|h| h.join("xf").join("cache"))
    } else {
        env("XDG_CACHE_HOME").or_else(|| env("HOME").map(|h| h.join(".cache"))).map(|h| h.join("xf"))
    }
}

/// `XV/64421.xmv` → `xv/64421.xmv` (parent directory name + file name, lower case).
pub fn key(movie: &Path) -> String {
    let name = movie.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let k = match movie.parent().and_then(|p| p.file_name()) {
        Some(d) => format!("{}/{name}", d.to_string_lossy()),
        None => name,
    };
    k.to_ascii_lowercase()
}

/// FNV-1a, 64 bit.
pub fn fnv64(b: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &x in b {
        h ^= x as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// (wav, json) paths of an entry.
pub fn paths(dir: &Path, key: &str) -> (PathBuf, PathBuf) {
    let base = dir.join(SUBDIR).join(key);
    let mut w = base.clone().into_os_string();
    w.push(".wav");
    let mut j = base.into_os_string();
    j.push(".json");
    (w.into(), j.into())
}

/// RIFF WAVE, 16-bit PCM.
pub fn wav_bytes(p: &Pcm) -> Vec<u8> {
    let ch = p.channels.max(1) as u32;
    let data = (p.samples.len() * 2) as u32;
    let mut b = Vec::with_capacity(44 + data as usize);
    b.extend(b"RIFF");
    b.extend((36 + data).to_le_bytes());
    b.extend(b"WAVEfmt ");
    b.extend(16u32.to_le_bytes());
    b.extend(1u16.to_le_bytes());
    b.extend((ch as u16).to_le_bytes());
    b.extend(p.rate.to_le_bytes());
    b.extend((p.rate * ch * 2).to_le_bytes());
    b.extend(((ch * 2) as u16).to_le_bytes());
    b.extend(16u16.to_le_bytes());
    b.extend(b"data");
    b.extend(data.to_le_bytes());
    for s in &p.samples {
        b.extend(s.to_le_bytes());
    }
    b
}

/// A 16-bit PCM RIFF WAVE (the chunks `fmt ` and `data`, others skipped).
pub fn parse_wav(b: &[u8]) -> Option<Pcm> {
    if b.get(0..4)? != b"RIFF" || b.get(8..12)? != b"WAVE" {
        return None;
    }
    let u16_at = |o: usize| b.get(o..o + 2).map(|x| u16::from_le_bytes([x[0], x[1]]));
    let u32_at = |o: usize| b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]));
    let mut o = 12;
    let mut fmt = None;
    while o + 8 <= b.len() {
        let id = &b[o..o + 4];
        let n = u32_at(o + 4)? as usize;
        let body = o + 8;
        match id {
            b"fmt " => {
                let (tag, ch, rate, bits) = (u16_at(body)?, u16_at(body + 2)?, u32_at(body + 4)?, u16_at(body + 14)?);
                if tag != 1 || bits != 16 || ch == 0 {
                    return None;
                }
                fmt = Some((ch, rate));
            }
            b"data" => {
                let (channels, rate) = fmt?;
                let d = b.get(body..(body + n).min(b.len()))?;
                let samples = d.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect();
                return Some(Pcm { rate, channels, samples });
            }
            _ => {}
        }
        o = body + n + (n & 1);
    }
    None
}

/// Why there is no usable entry.
#[derive(Debug, PartialEq, Eq)]
pub enum Miss {
    /// no cache dir, or no entry for the movie
    Absent,
    /// an entry of another file (size or hash differ) or an unreadable one
    Stale(String),
}

/// The cached media PCM of the movie `movie` whose whole file is `data` (before the edit list).
pub fn load(dir: &Path, movie: &Path, data: &[u8]) -> Result<Pcm, Miss> {
    let (w, j) = paths(dir, &key(movie));
    let meta = crate::io::read(&j).map_err(|_| Miss::Absent)?;
    let m: serde_json::Value =
        serde_json::from_slice(&meta).map_err(|e| Miss::Stale(format!("{}: {e}", j.display())))?;
    let size = m["size"].as_u64();
    let hash = m["fnv64"].as_str().and_then(|s| u64::from_str_radix(s, 16).ok());
    if size != Some(data.len() as u64) || hash != Some(fnv64(data)) {
        return Err(Miss::Stale(format!("{}: made from another file (size / hash differ)", j.display())));
    }
    let b = crate::io::read(&w).map_err(|e| Miss::Stale(format!("{}: {e}", w.display())))?;
    parse_wav(&b).ok_or_else(|| Miss::Stale(format!("{}: not a 16-bit PCM WAVE file", w.display())))
}

/// Write the entry for `movie` (whole file `data`) with std::fs: the WAV first, then the JSON (an entry counts only with
/// its JSON), each through a temporary file + rename.
pub fn store(dir: &Path, movie: &Path, data: &[u8], pcm: &Pcm, ffmpeg: &str) -> std::io::Result<()> {
    let (w, j) = paths(dir, &key(movie));
    if let Some(p) = w.parent() {
        std::fs::create_dir_all(p)?;
    }
    let meta = serde_json::json!({
        "source": key(movie),
        "size": data.len() as u64,
        "fnv64": format!("{:016x}", fnv64(data)),
        "rate": pcm.rate,
        "channels": pcm.channels,
        "frames": pcm.frames(),
        "decoder": ffmpeg,
    });
    let put = |p: &Path, b: &[u8]| -> std::io::Result<()> {
        let mut t = p.as_os_str().to_owned();
        t.push(".tmp");
        std::fs::write(&t, b)?;
        std::fs::rename(&t, p)
    };
    let _ = std::fs::remove_file(&j);
    put(&w, &wav_bytes(pcm))?;
    put(&j, serde_json::to_string_pretty(&meta).unwrap().as_bytes())
}

/// Whether `dir` holds a valid entry for `movie` (whole file `data`): JSON matches and the WAV header parses.
pub fn is_fresh(dir: &Path, movie: &Path, data: &[u8]) -> bool {
    load(dir, movie, data).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_and_wav_roundtrip() {
        assert_eq!(key(Path::new("data/InstalledGame/XV/64421.xmv")), "xv/64421.xmv");
        assert_eq!(key(Path::new("72106.mus")), "72106.mus");
        let p = Pcm { rate: 44100, channels: 2, samples: vec![1, -2, 300, -32768, 32767, 0] };
        let q = parse_wav(&wav_bytes(&p)).unwrap();
        assert_eq!((q.rate, q.channels, &q.samples), (44100, 2, &p.samples));
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }

    /// A synthetic cache entry for a QDMC track with two edits (like XV/64657.xmv: media 0..3600/600 s, then a jump):
    /// the engine path ([`crate::media::qdmc`]) loads it and places it by the track's length and edit list; a changed
    /// source file makes the entry stale; no entry gives the error with the one-time hint.
    #[test]
    fn synthetic_entry_placed_by_edit_list() {
        use crate::media::qt::{Edit, Handler, Movie, SoundDesc, Track};
        let rate = 1000;
        let track = Track {
            handler: Handler::Sound,
            fourcc: *b"QDMC",
            width: 0,
            height: 0,
            timescale: rate,
            duration: 100, // media frames; the cached PCM carries 4 frames of decoder padding past it
            edits: vec![
                Edit { dur: 30, media_time: 0 },
                Edit { dur: 6, media_time: -1 },
                Edit { dur: 12, media_time: 90 },
            ],
            samples: Vec::new(),
            times: Vec::new(),
            snd: Some(SoundDesc { channels: 2, bits: 16, rate, ..Default::default() }),
            chunks: Vec::new(),
            enabled: true,
            layer: 0,
            tx: 0,
            ty: 0,
            track_w: 0,
            track_h: 0,
            text: None,
            sample_codecs: Vec::new(),
        };
        let movie = Movie { timescale: 600, duration: 48, tracks: vec![track] };
        let dir = std::env::temp_dir().join(format!("xf-pcmcache-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let src = Path::new("/game/XV/99999.xmv");
        let data = b"not really a movie".to_vec();
        // frame i = (i, -i)
        let media = Pcm { rate, channels: 2, samples: (0..104).flat_map(|i| [i as i16, -(i as i16)]).collect() };
        store(&dir, src, &data, &media, "test").unwrap();
        assert!(dir.join("qdmc-v1/xv/99999.xmv.wav").is_file() && dir.join("qdmc-v1/xv/99999.xmv.json").is_file());
        let p = crate::media::qdmc(Some(dir.clone()), src, &movie, &data).unwrap();
        // edits in movie time (600/s) → frames at 1000/s: 50 from media 0, 10 silent, 20 from media 90 (cut at 100)
        assert_eq!((p.rate, p.channels), (rate, 2));
        let left: Vec<i16> = p.samples.iter().step_by(2).copied().collect();
        let want: Vec<i16> = (0..50).chain(std::iter::repeat(0).take(10)).chain(90..100).collect();
        assert_eq!(left, want);
        assert_eq!(p.samples[1], 0);
        assert_eq!(p.samples[3], -1);
        // another file under the same name: stale, not played
        let e = crate::media::qdmc(Some(dir.clone()), src, &movie, b"another file").unwrap_err();
        assert!(e.contains("size / hash differ"), "{e}");
        let e = crate::media::qdmc(Some(dir.clone()), Path::new("/game/XV/88888.xmv"), &movie, &data).unwrap_err();
        assert!(e.contains("not in the PCM cache"), "{e}");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
