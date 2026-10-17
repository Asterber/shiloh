//! Minimal QuickTime movie reader (atoms moov/trak/edts/mdia/minf/stbl) — enough to find the sample shown at a
//! movie time and to read its bytes. Format: Apple QuickTime File Format (public spec); the game opens these files
//! through QuickTime itself (C9), so no engine code is involved here.

use std::path::Path;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handler {
    Video,
    Sound,
    /// 'text' media: closed captions (H340)
    Text,
    Other,
}

#[derive(Clone, Debug)]
pub struct Edit {
    /// segment duration, movie timescale
    pub dur: u64,
    /// media time, media timescale; -1 = empty edit
    pub media_time: i64,
}

#[derive(Clone, Debug)]
pub struct Track {
    pub handler: Handler,
    pub fourcc: [u8; 4],
    pub width: u16,
    pub height: u16,
    pub timescale: u32,
    pub duration: u64,
    pub edits: Vec<Edit>,
    /// per sample: (file offset, size)
    pub samples: Vec<(u64, u32)>,
    /// per sample: media time of its start
    pub times: Vec<u64>,
    /// sound: channels, bits, sample rate, compression id, samples/packet, bytes/packet (version-1 desc)
    pub snd: Option<SoundDesc>,
    /// sample-to-chunk derived: per chunk (offset, samples) — for sound tracks (samples counted in sound frames)
    pub chunks: Vec<(u64, u32)>,
    /// tkhd: track enabled (flag bit 0)
    pub enabled: bool,
    /// tkhd: layer (lower = in front)
    pub layer: i16,
    /// tkhd: matrix translation (whole pixels) and track size — where the track is drawn in the movie box
    pub tx: i32,
    pub ty: i32,
    pub track_w: u32,
    pub track_h: u32,
    /// text tracks: the sample description (H340)
    pub text: Option<super::captions::TextDesc>,
    /// H1940: per sample, the codec of its sample description (stsc's description id → stsd entry) — only when the
    /// track's stsd entries name more than one codec (NAV2..NAV6.NMV track 1: a 'cvid' still among the 'jpeg' ones);
    /// empty = every sample is `fourcc`
    pub sample_codecs: Vec<[u8; 4]>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SoundDesc {
    pub channels: u16,
    pub bits: u16,
    pub rate: u32,
    pub samples_per_packet: u32,
    pub bytes_per_packet: u32,
}

#[derive(Clone, Debug)]
pub struct Movie {
    pub timescale: u32,
    pub duration: u64,
    pub tracks: Vec<Track>,
}

pub(crate) fn be16(b: &[u8], o: usize) -> u16 {
    b.get(o..o + 2).map_or(0, |x| u16::from_be_bytes([x[0], x[1]]))
}
pub(crate) fn be32(b: &[u8], o: usize) -> u32 {
    b.get(o..o + 4).map_or(0, |x| u32::from_be_bytes([x[0], x[1], x[2], x[3]]))
}
fn be64(b: &[u8], o: usize) -> u64 {
    (be32(b, o) as u64) << 32 | be32(b, o + 4) as u64
}

/// Iterate child atoms of `b`: (type, body).
pub(crate) fn atoms(b: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    let mut o = 0usize;
    while o + 8 <= b.len() {
        let mut sz = be32(b, o) as usize;
        let typ = [b[o + 4], b[o + 5], b[o + 6], b[o + 7]];
        let mut hdr = 8;
        if sz == 1 {
            sz = be64(b, o + 8) as usize;
            hdr = 16;
        } else if sz == 0 {
            sz = b.len() - o;
        }
        if sz < hdr || o + sz > b.len() {
            break;
        }
        out.push((typ, &b[o + hdr..o + sz]));
        o += sz;
    }
    out
}

fn child<'a>(b: &'a [u8], t: &[u8; 4]) -> Option<&'a [u8]> {
    atoms(b).into_iter().find(|(x, _)| x == t).map(|(_, v)| v)
}

impl Movie {
    pub fn open(path: &Path) -> Result<Movie, String> {
        let d = crate::io::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Movie::parse(&d).ok_or_else(|| format!("{}: not a QuickTime movie", path.display()))
    }

    /// The movie header read by ranges ([`crate::io::read_range`]): the top-level atom headers, then only the `moov`
    /// atom — for remote files, where the whole file would be a large download.
    pub fn open_header(path: &Path) -> Result<Movie, String> {
        let err = |e: std::io::Error| format!("{}: {e}", path.display());
        let not = || format!("{}: not a QuickTime movie", path.display());
        let size = crate::io::fs().size(path).map_err(err)?;
        let mut o = 0u64;
        while o + 8 <= size {
            let h = crate::io::read_range(path, o, 16).map_err(err)?;
            let mut sz = be32(&h, 0) as u64;
            let typ = [h[4], h[5], h[6], h[7]];
            if sz == 1 {
                sz = be64(&h, 8);
            } else if sz == 0 {
                sz = size - o;
            }
            if sz < 8 || o + sz > size {
                break;
            }
            if &typ == b"moov" {
                let d = crate::io::read_range(path, o, sz as usize).map_err(err)?;
                return Movie::parse(&d).ok_or_else(not);
            }
            o += sz;
        }
        Err(not())
    }

    /// Parse the movie header from the whole file (sample bytes are read later from the file by offset).
    pub fn parse(d: &[u8]) -> Option<Movie> {
        let moov = child(d, b"moov")?;
        // H2210: a compressed movie header — moov holds cmov { dcom 'zlib', cmvd { u32 size, zlib stream } } whose
        // inflated bytes are the real moov atom (QuickTime File Format spec; XN/25949.XMV, Smolnikoff's lair View
        // 0x1549 background, is the game's only such file). Sample offsets stay file offsets.
        if let Some(cmov) = child(moov, b"cmov") {
            if child(cmov, b"dcom")?.get(..4)? != b"zlib" {
                return None;
            }
            let cmvd = child(cmov, b"cmvd")?;
            let inner = super::inflate::zlib(cmvd.get(4..)?)?;
            if inner.len() != be32(cmvd, 0) as usize || child(&inner, b"moov").and_then(|m| child(m, b"cmov")).is_some() {
                return None;
            }
            return Movie::parse(&inner);
        }
        let mvhd = child(moov, b"mvhd")?;
        let (timescale, duration) = if mvhd[0] == 1 { (be32(mvhd, 20), be64(mvhd, 24)) } else { (be32(mvhd, 12), be32(mvhd, 16) as u64) };
        let mut tracks = Vec::new();
        for (t, trak) in atoms(moov) {
            if &t == b"trak" {
                if let Some(tr) = Track::parse(trak) {
                    tracks.push(tr);
                }
            }
        }
        Some(Movie { timescale: timescale.max(1), duration, tracks })
    }

    pub fn duration_ms(&self) -> u64 {
        self.duration * 1000 / self.timescale as u64
    }

    /// The video sample shown at movie time `t_ms`: (track index, sample index). Tracks are walked in order and the
    /// first one with a non-empty edit at that time wins (the FO .nmv files interleave two tracks whose edits never
    /// overlap).
    pub fn video_sample_at(&self, t_ms: u64) -> Option<(usize, usize)> {
        let t = t_ms * self.timescale as u64 / 1000; // movie timescale
        for (i, tr) in self.tracks.iter().enumerate() {
            if tr.handler != Handler::Video || tr.samples.is_empty() {
                continue;
            }
            if let Some(mt) = tr.media_time_at(t, self.timescale) {
                return Some((i, tr.sample_at(mt)));
            }
        }
        None
    }
}

impl Track {
    fn parse(trak: &[u8]) -> Option<Track> {
        let mdia = child(trak, b"mdia")?;
        let mdhd = child(mdia, b"mdhd")?;
        let (timescale, duration) = if mdhd[0] == 1 { (be32(mdhd, 20), be64(mdhd, 24)) } else { (be32(mdhd, 12), be32(mdhd, 16) as u64) };
        let hdlr = child(mdia, b"hdlr")?;
        let handler = match &hdlr[8..12] {
            b"vide" => Handler::Video,
            b"soun" => Handler::Sound,
            b"text" => Handler::Text,
            _ => Handler::Other,
        };
        let mut edits = Vec::new();
        if let Some(elst) = child(trak, b"edts").and_then(|e| child(e, b"elst")) {
            let n = be32(elst, 4) as usize;
            for i in 0..n {
                let o = 8 + 12 * i;
                edits.push(Edit { dur: be32(elst, o) as u64, media_time: be32(elst, o + 4) as i32 as i64 });
            }
        }
        let stbl = child(child(mdia, b"minf")?, b"stbl")?;
        let stsd = child(stbl, b"stsd")?;
        let e = &stsd[8..];
        let fourcc = [e.get(4).copied()?, e[5], e[6], e[7]];
        let (mut width, mut height, mut snd, mut text) = (0, 0, None, None);
        match handler {
            Handler::Video => {
                width = be16(e, 32);
                height = be16(e, 34);
            }
            Handler::Sound => {
                let ver = be16(e, 16);
                let mut s = SoundDesc {
                    channels: be16(e, 24),
                    bits: be16(e, 26),
                    rate: be32(e, 32) >> 16,
                    samples_per_packet: 1,
                    bytes_per_packet: 0,
                };
                if ver >= 1 {
                    s.samples_per_packet = be32(e, 36);
                    s.bytes_per_packet = be32(e, 40);
                }
                snd = Some(s);
            }
            Handler::Text => text = super::captions::TextDesc::parse(e),
            Handler::Other => {}
        }
        // sample sizes
        let stsz = child(stbl, b"stsz")?;
        let (fixed, n) = (be32(stsz, 4), be32(stsz, 8) as usize);
        // a sound track counts every sound frame as a sample (XS/80788.mus: 4 276 224) and is read by chunks
        // (`audio::decode`): no per-sample table for it — robustness: the table took ~100 MB per parsed .mus, kept by
        // every cached MoviePlayer
        let per_sample = handler != Handler::Sound;
        let n = if per_sample { n } else { 0 };
        let sizes: Vec<u32> = (0..n).map(|i| if fixed != 0 { fixed } else { be32(stsz, 12 + 4 * i) }).collect();
        // chunk offsets
        let offs: Vec<u64> = if let Some(stco) = child(stbl, b"stco") {
            (0..be32(stco, 4) as usize).map(|i| be32(stco, 8 + 4 * i) as u64).collect()
        } else if let Some(co64) = child(stbl, b"co64") {
            (0..be32(co64, 4) as usize).map(|i| be64(co64, 8 + 8 * i)).collect()
        } else {
            return None;
        };
        // sample-to-chunk
        let stsc = child(stbl, b"stsc")?;
        let runs: Vec<(usize, u32, u32)> = (0..be32(stsc, 4) as usize)
            .map(|i| (be32(stsc, 8 + 12 * i) as usize, be32(stsc, 12 + 12 * i), be32(stsc, 16 + 12 * i)))
            .collect();
        // every sample description's codec (stsd entries: size, data format, …)
        let mut descs: Vec<[u8; 4]> = Vec::new();
        let mut eo = 0usize;
        for _ in 0..be32(stsd, 4) {
            let Some(d) = stsd.get(8 + eo..) else { break };
            if d.len() < 8 {
                break;
            }
            descs.push([d[4], d[5], d[6], d[7]]);
            let sz = be32(d, 0) as usize;
            if sz < 8 {
                break;
            }
            eo += sz;
        }
        let mixed = per_sample && descs.iter().any(|d| *d != fourcc);
        let mut sample_codecs = Vec::new();
        let mut samples = Vec::with_capacity(n);
        let mut chunks = Vec::with_capacity(offs.len());
        let mut si = 0usize;
        for (ci, &off) in offs.iter().enumerate() {
            let chunk_no = ci + 1;
            let run = runs.iter().rev().find(|r| r.0 <= chunk_no);
            let per = run.map_or(1, |r| r.1);
            let codec = run.and_then(|r| descs.get((r.2 as usize).wrapping_sub(1))).copied().unwrap_or(fourcc);
            chunks.push((off, per));
            let mut o = off;
            for _ in 0..per {
                if si >= n {
                    break;
                }
                if mixed {
                    sample_codecs.push(codec);
                }
                samples.push((o, sizes[si]));
                o += sizes[si] as u64;
                si += 1;
            }
        }
        // sample times
        let stts = child(stbl, b"stts")?;
        let mut times = Vec::with_capacity(n);
        let mut t = 0u64;
        for i in 0..if per_sample { be32(stts, 4) as usize } else { 0 } {
            let (cnt, delta) = (be32(stts, 8 + 8 * i), be32(stts, 12 + 8 * i));
            for _ in 0..cnt {
                times.push(t);
                t += delta as u64;
            }
        }
        times.resize(samples.len(), t);
        let tkhd = child(trak, b"tkhd").unwrap_or(&[]);
        let fixed = |o: usize| (be32(tkhd, o) as i32) >> 16;
        Some(Track {
            handler,
            fourcc,
            width,
            height,
            timescale: timescale.max(1),
            duration,
            edits,
            samples,
            times,
            snd,
            chunks,
            enabled: tkhd.len() < 4 || tkhd[3] & 1 != 0,
            layer: be16(tkhd, 32) as i16,
            tx: fixed(64),
            ty: fixed(68),
            track_w: be32(tkhd, 76) >> 16,
            track_h: be32(tkhd, 80) >> 16,
            text,
            sample_codecs,
        })
    }

    /// The codec of sample `s` (H1940: its own sample description's, when the track mixes codecs).
    pub fn sample_fourcc(&self, s: usize) -> [u8; 4] {
        self.sample_codecs.get(s).copied().unwrap_or(self.fourcc)
    }

    /// Media time at movie time `t` (movie timescale `mts`), None = no media (empty edit or past the end).
    pub fn media_time_at(&self, t: u64, mts: u32) -> Option<u64> {
        if self.edits.is_empty() {
            let m = t * self.timescale as u64 / mts as u64;
            return (m < self.duration.max(1)).then_some(m);
        }
        let mut start = 0u64;
        for e in &self.edits {
            if t < start + e.dur {
                if e.media_time < 0 {
                    return None;
                }
                return Some(e.media_time as u64 + (t - start) * self.timescale as u64 / mts as u64);
            }
            start += e.dur;
        }
        None
    }

    /// Index of the sample covering media time `mt`.
    pub fn sample_at(&self, mt: u64) -> usize {
        match self.times.binary_search(&mt) {
            Ok(i) => i,
            Err(0) => 0,
            Err(i) => i - 1,
        }
    }
}

#[cfg(test)]
mod tests {
    /// the header read by ranges is the header parsed from the whole file
    #[test]
    fn open_header_same_as_parse() {
        let d = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
        for f in ["NAV1.NMV", "XV/19937.xmv"] {
            let p = d.join(f);
            let Ok(b) = std::fs::read(&p) else { continue };
            let a = super::Movie::parse(&b).unwrap();
            let h = super::Movie::open_header(&p).unwrap();
            assert_eq!(format!("{:?}", (a.timescale, a.duration, a.tracks.len())), format!("{:?}", (h.timescale, h.duration, h.tracks.len())));
            for (x, y) in a.tracks.iter().zip(&h.tracks) {
                assert_eq!(x.samples, y.samples);
                assert_eq!(x.chunks, y.chunks);
            }
        }
    }
}
