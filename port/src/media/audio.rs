//! Sound tracks of QuickTime movies (*.xmv, *.amv, *.mus, *.dmv) → 16-bit PCM.
//!
//! Codecs in data/InstalledGame (ffprobe over XV and XS): 'ima4' (IMA ADPCM, QuickTime packing) in 1262+ XV movies and
//! 148 XS files, 'twos' (big-endian signed PCM, 8 or 16 bit) in 15 XS files, 'sowt' (little-endian 16 bit) in 8 XV
//! movies; 'QDMC' (14 files) is not decoded here — its PCM comes from a
//! cache the user fills once with ffmpeg (`pcmcache`, `xf-prep`). Format: Apple QuickTime File Format (public spec) + Apple IMA4 packing:
//! per channel and packet 34 bytes = big-endian u16 header (predictor = high 9 bits as i16, step index = low 7 bits) +
//! 32 bytes of 64 nibbles, low nibble first; packets of the channels follow each other (ch0, ch1, ch0, …).
//! Decoding follows ffmpeg's `adpcm_ima_qt` (also its rule of keeping the running predictor when a packet header has the
//! same step index and a predictor within 0x7f of it), so the output can be compared with `ffmpeg -f s16le` sample by
//! sample (`tests/media.rs`). The game plays these through QuickTime (C9): no engine code is involved.

use super::qt::{Handler, Movie, Track};

/// Interleaved 16-bit PCM.
#[derive(Clone, Debug, Default)]
pub struct Pcm {
    pub rate: u32,
    pub channels: u16,
    pub samples: Vec<i16>,
}

impl Pcm {
    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1) as usize
    }
    pub fn duration_ms(&self) -> u64 {
        self.frames() as u64 * 1000 / self.rate.max(1) as u64
    }
}

const STEP: [i32; 89] = [
    7, 8, 9, 10, 11, 12, 13, 14, 16, 17, 19, 21, 23, 25, 28, 31, 34, 37, 41, 45, 50, 55, 60, 66, 73, 80, 88, 97, 107, 118, 130,
    143, 157, 173, 190, 209, 230, 253, 279, 307, 337, 371, 408, 449, 494, 544, 598, 658, 724, 796, 876, 963, 1060, 1166, 1282,
    1411, 1552, 1707, 1878, 2066, 2272, 2499, 2749, 3024, 3327, 3660, 4026, 4428, 4871, 5358, 5894, 6484, 7132, 7845, 8630,
    9493, 10442, 11487, 12635, 13899, 15289, 16818, 18500, 20350, 22385, 24623, 27086, 29794, 32767,
];
const INDEX: [i32; 16] = [-1, -1, -1, -1, 2, 4, 6, 8, -1, -1, -1, -1, 2, 4, 6, 8];

#[derive(Clone, Copy, Default)]
struct Ima {
    pred: i32,
    idx: i32,
}

impl Ima {
    fn header(&mut self, h: u16) {
        let pred = (h & 0xff80) as i16 as i32;
        let idx = ((h & 0x7f) as i32).min(88);
        // ffmpeg adpcm_ima_qt: keep the running state when the header repeats it closely
        if idx != self.idx || (pred - self.pred).abs() > 0x7f {
            self.pred = pred;
            self.idx = idx;
        }
    }
    fn nibble(&mut self, n: u8) -> i16 {
        let step = STEP[self.idx as usize];
        let mut diff = step >> 3;
        if n & 4 != 0 {
            diff += step;
        }
        if n & 2 != 0 {
            diff += step >> 1;
        }
        if n & 1 != 0 {
            diff += step >> 2;
        }
        let p = if n & 8 != 0 { self.pred - diff } else { self.pred + diff };
        self.pred = p.clamp(-32768, 32767);
        self.idx = (self.idx + INDEX[n as usize]).clamp(0, 88);
        self.pred as i16
    }
}

/// Decode IMA4 packets (34 bytes per channel and packet) into interleaved PCM, continuing the channel states `st`.
fn ima4(b: &[u8], ch: usize, st: &mut [Ima], out: &mut Vec<i16>) {
    let pk = 34 * ch;
    for p in b.chunks_exact(pk) {
        let base = out.len();
        out.resize(base + 64 * ch, 0);
        for c in 0..ch {
            let q = &p[34 * c..34 * c + 34];
            st[c].header(u16::from_be_bytes([q[0], q[1]]));
            for (i, &x) in q[2..].iter().enumerate() {
                out[base + (2 * i) * ch + c] = st[c].nibble(x & 15);
                out[base + (2 * i + 1) * ch + c] = st[c].nibble(x >> 4);
            }
        }
    }
}

/// The first sound track of a movie (the one [`decode`] plays).
pub fn sound_track(movie: &Movie) -> Option<&Track> {
    movie.tracks.iter().find(|t| t.handler == Handler::Sound)
}

/// Media PCM of the first sound track (all its samples, media time 0 first) → movie time: cut to the track's length and
/// apply its edit list. Used for decoded tracks and for the cached PCM of QDMC tracks ([`super::pcmcache`]).
pub fn place(movie: &Movie, mut pcm: Pcm) -> Pcm {
    let Some(t) = sound_track(movie) else { return pcm };
    let ch = pcm.channels.max(1) as usize;
    let rate = pcm.rate;
    // the last packet may carry padding past the track's length (media duration in sample frames)
    if t.timescale == rate && t.duration > 0 {
        pcm.samples.truncate(t.duration as usize * ch);
    }
    // edit list → movie time: XV sound tracks start at media time 30–60 frames (e.g. 20193.xmv: one edit, media time 34,
    // 680/600 s), so frame 0 of the result is movie time 0
    if !t.edits.is_empty() && t.timescale == rate {
        let mut out = Vec::new();
        for e in &t.edits {
            let n = (e.dur * rate as u64 / movie.timescale.max(1) as u64) as usize * ch;
            if e.media_time < 0 {
                out.resize(out.len() + n, 0);
            } else {
                let s0 = (e.media_time as usize * ch).min(pcm.samples.len());
                let s1 = (s0 + n).min(pcm.samples.len());
                out.extend_from_slice(&pcm.samples[s0..s1]);
            }
        }
        pcm.samples = out;
    }
    pcm
}

/// Decode the first sound track of a movie (`data` = the whole file). Err if there is none or its codec is not
/// supported ('QDMC': see [`super::pcmcache`]).
pub fn decode(movie: &Movie, data: &[u8]) -> Result<Pcm, String> {
    let t = sound_track(movie).ok_or("no sound track")?;
    let sd = t.snd.ok_or("no sound description")?;
    let ch = sd.channels.max(1) as usize;
    let mut pcm = Pcm { rate: sd.rate, channels: ch as u16, samples: Vec::new() };
    let mut st = vec![Ima::default(); ch];
    for &(off, frames) in &t.chunks {
        let o = off as usize;
        let frames = frames as usize;
        match &t.fourcc {
            b"ima4" => {
                let n = frames.div_ceil(64) * 34 * ch;
                ima4(data.get(o..o + n).ok_or("ima4 chunk past the end of the file")?, ch, &mut st, &mut pcm.samples);
            }
            b"twos" | b"sowt" | b"raw " => {
                let bits = sd.bits as usize;
                let n = frames * ch * bits / 8;
                let b = data.get(o..o + n).ok_or("pcm chunk past the end of the file")?;
                match (bits, &t.fourcc) {
                    (8, b"raw ") => pcm.samples.extend(b.iter().map(|&x| ((x as i16) - 128) << 8)),
                    (8, _) => pcm.samples.extend(b.iter().map(|&x| (x as i8 as i16) << 8)),
                    (16, b"sowt") => pcm.samples.extend(b.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]]))),
                    (16, _) => pcm.samples.extend(b.chunks_exact(2).map(|x| i16::from_be_bytes([x[0], x[1]]))),
                    _ => return Err(format!("{bits}-bit PCM not supported")),
                }
            }
            f => return Err(format!("sound codec {:?} not supported", String::from_utf8_lossy(f))),
        }
    }
    Ok(place(movie, pcm))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ima4_packet() {
        // header: predictor 0, step index 0; nibbles 0x7 then 0xf: +step*(1/8+1+1/2+1/4) then the reverse
        let mut p = vec![0u8, 0];
        p.push(0xf7);
        p.extend([0u8; 31]);
        let mut out = Vec::new();
        ima4(&p, 1, &mut [Ima::default()], &mut out);
        assert_eq!(out.len(), 64);
        // step 7: diff = 0 + 7 + 3 + 1 = 11 → 11; index 0 + 8 = 8 (step 16): diff = 2 + 16 + 8 + 4 = 30 → -19
        assert_eq!(&out[..2], &[11, -19]);
    }
}
