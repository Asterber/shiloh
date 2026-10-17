//! Media of the installed game: AssetRef blob → file (`asset`), QuickTime movies (`qt`), Cinepak / Photo-JPEG
//! frames (`cinepak2`, jpeg-decoder). Display only — what plays and when is decided by the core (`world`).
//!
//! Formats seen in data/InstalledGame (ffprobe + `qt`): NAV*.NMV / navm.nmv = QuickTime, 2 interleaved 'jpeg'
//! (Photo-JPEG) video tracks 600×240, movie timescale 600, media 1 frame per second; XV/XN/XG *.xmv = QuickTime
//! 'cvid' (Cinepak) 600×240 / 'rpza' (`rpza2`, XG icons 80×60) (+ 'ima4' sound in XV) or 'jpeg' (some XN loops); XS *.amv = QuickTime 'ima4' sound;
//! *.mus = music; sound tracks → PCM (`audio`: ima4, twos, sowt; QDMC from the
//! user's ffmpeg cache, `pcmcache`); *.pff = PICT picture containers (`pict`).

pub mod asset;
pub mod audio;
pub mod captions;
pub mod cinepak2;
pub mod hot;
pub mod inflate;
pub mod lang;
pub mod pcmcache;
pub mod pict;
pub mod qt;
pub mod rpza2;
pub mod subs;

use crate::hdb::Hdb;
use asset::AssetBlob;
use qt::{Handler, Movie};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Frame size of every view / video movie in the FO data (tkhd 600×240).
pub const FRAME_W: usize = 600;
pub const FRAME_H: usize = 240;

/// Install directory with a case-insensitive file index (the game's names are mixed case: `nav1.nmv` → NAV1.NMV).
#[derive(Clone)]
pub struct MediaLib {
    pub dir: PathBuf,
    index: HashMap<String, PathBuf>,
}

/// A resolved asset: which file, which time range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Clip {
    pub asset: u32,
    pub file: PathBuf,
    /// file name as written in the HDB blob
    pub name: String,
    pub start: u32,
    pub end: u32,
}

impl Clip {
    pub fn ext(&self) -> String {
        self.name.rsplit_once('.').map(|x| x.1.to_ascii_lowercase()).unwrap_or_default()
    }
    /// still picture: start == end != 0 (e.g. navm.nmv 54001..54001); HYPOTHESIS H981 (data: all 1212 .nmv AssetRefs are
    /// stills — 1208 at start == end != 0, 4 at 0..0 (0x432 View 0x2b3's fallback background, 0x1040, 0xe398, 0x1d6b),
    /// none a range): an .nmv 0..0 is its first still, not the whole movie (the port looped nav1.nmv through every Node-1
    /// still: the "slideshow in the Field Office" bug report)
    pub fn is_still(&self) -> bool {
        self.start == self.end && (self.start != 0 || self.ext() == "nmv")
    }
}

impl MediaLib {
    pub fn new(dir: impl AsRef<Path>) -> MediaLib {
        let dir = dir.as_ref().to_path_buf();
        let mut index = HashMap::new();
        let mut add = |rel: String, p: PathBuf| {
            index.insert(rel.to_ascii_lowercase(), p);
        };
        if let Ok(rd) = crate::io::read_dir(&dir) {
            for e in rd {
                if e.is_dir {
                    if let Ok(sub) = crate::io::read_dir(&e.path) {
                        for f in sub {
                            add(format!("{}/{}", e.name, f.name), f.path);
                        }
                    }
                } else {
                    add(e.name, e.path);
                }
            }
        }
        MediaLib { dir, index }
    }

    pub fn find(&self, rel: &str) -> Option<&PathBuf> {
        self.index.get(&rel.to_ascii_lowercase())
    }

    /// Every indexed file: (lower-case key `dir/name` or `name`, path).
    pub fn entries(&self) -> impl Iterator<Item = (&String, &PathBuf)> {
        self.index.iter()
    }

    /// H1280: the hotspots of a movie file (`XV/<n>.HOT` next to `XV/<n>.xmv`, as written in the blob); empty when the
    /// file is missing or not a `HSPT` file.
    pub fn movie_hotspots(&self, movie: &str) -> Vec<hot::Hotspot> {
        self.find(&hot::hot_name(movie))
            .and_then(|p| crate::io::read(p).ok())
            .and_then(|b| hot::parse(&b))
            .unwrap_or_default()
    }

    /// Asset → clip: the first blob entry whose file exists. (the original asks its entry chooser for the first entry
    /// (variant −1) in install mode 2, i.e. when NAV1.NMV exists, which is the case for data/InstalledGame.)
    pub fn resolve(&self, h: &Hdb, asset: u32) -> Option<Clip> {
        let r = h.read(crate::world::class::ASSET_REF, asset)?;
        let blob = AssetBlob::parse(h.blob_bytes(r.blob("asset")?)?, r.u32("f40"))?;
        blob.entries.iter().find_map(|e| {
            self.find(&e.file)
                .map(|p| Clip { asset, file: p.clone(), name: e.file.clone(), start: e.start, end: e.end })
        })
    }
}

/// The sound track of a movie file as PCM (see [`audio`]).
pub fn sound(path: &Path) -> Result<audio::Pcm, String> {
    // remote files: only the header when there is no sound track (same error as `audio::decode`)
    if crate::io::ranged() && !Movie::open_header(path)?.tracks.iter().any(|t| t.handler == Handler::Sound) {
        return Err(format!("{}: no sound track", path.display()));
    }
    let data = crate::io::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let movie = Movie::parse(&data).ok_or_else(|| format!("{}: not a QuickTime movie", path.display()))?;
    if audio::sound_track(&movie).is_some_and(|t| &t.fourcc == b"QDMC") {
        return qdmc(pcmcache::dir(), path, &movie, &data);
    }
    audio::decode(&movie, &data).map_err(|e| format!("{}: {e}", path.display()))
}

static QDMC_HINT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// A QDMC sound track: the PCM cached by `xf-prep` in `dir` ([`pcmcache`]) placed by the track's length and edit list. Without
/// an entry: Err, the first time with a hint how to make the cache.
pub(crate) fn qdmc(dir: Option<PathBuf>, path: &Path, movie: &Movie, data: &[u8]) -> Result<audio::Pcm, String> {
    let miss = match dir {
        Some(d) => match pcmcache::load(&d, path, data) {
            Ok(p) => return Ok(audio::place(movie, p)),
            Err(pcmcache::Miss::Stale(e)) => e,
            Err(pcmcache::Miss::Absent) => format!("not in the PCM cache {}", d.display()),
        },
        None => "no PCM cache dir (--cache DIR or XF_CACHE)".to_string(),
    };
    let mut e = format!("{}: QDMC sound {miss}", path.display());
    if !QDMC_HINT.swap(true, std::sync::atomic::Ordering::Relaxed) {
        e.push_str(" — QDMC music plays after a one-time conversion with your ffmpeg: cargo run --release --bin xf-prep -- --data DIR [--cache DIR] (see port/README.md)");
    }
    Err(e)
}

enum Dec {
    Cvid(cinepak2::Cinepak),
    Rpza(rpza2::Rpza),
    Jpeg,
}

impl Dec {
    fn for_fourcc(fourcc: &[u8; 4], w: usize, h: usize) -> Option<Dec> {
        match fourcc {
            b"cvid" => Some(Dec::Cvid(cinepak2::Cinepak::new(w, h))),
            // Motion-JPEG A (the system dialog box XG/54200, AssetRef 0xd3b8): one baseline JPEG per sample
            b"jpeg" | b"mjpa" => Some(Dec::Jpeg),
            b"rpza" => Some(Dec::Rpza(rpza2::Rpza::new(w, h))),
            _ => None,
        }
    }

    /// Codecs of one kind share a decoder ('jpeg' and 'mjpa' are both `Dec::Jpeg`).
    fn kind_of(fourcc: &[u8; 4]) -> Option<u8> {
        match fourcc {
            b"cvid" => Some(0),
            b"jpeg" | b"mjpa" => Some(1),
            b"rpza" => Some(2),
            _ => None,
        }
    }

    fn kind(&self) -> u8 {
        match self {
            Dec::Cvid(_) => 0,
            Dec::Jpeg => 1,
            Dec::Rpza(_) => 2,
        }
    }
}

/// Decoder for one movie file (video track frames as RGB24 600×240).
pub struct MoviePlayer {
    pub movie: Movie,
    /// the whole file; empty when the samples are read by ranges from `path` ([`crate::io::ranged`])
    data: Vec<u8>,
    path: std::path::PathBuf,
    dec: Option<Dec>,
    /// H1940: decoders of the other codecs a mixed-codec track's samples name (made on first use)
    alt: Vec<Dec>,
    /// (track, sample) currently in `rgb`
    cur: Option<(usize, usize)>,
    /// H340: text samples read by range (file offset → bytes) when `data` is empty — the captions read them every frame
    text_samples: std::collections::HashMap<u64, Vec<u8>>,
    pub rgb: Vec<u8>,
    pub width: usize,
    pub height: usize,
}

impl MoviePlayer {
    pub fn open(path: &Path) -> Result<MoviePlayer, String> {
        let (data, movie) = if crate::io::ranged() {
            (Vec::new(), Movie::open_header(path)?)
        } else {
            let data = crate::io::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
            let movie = Movie::parse(&data).ok_or_else(|| format!("{}: not a QuickTime movie", path.display()))?;
            (data, movie)
        };
        let v = movie.tracks.iter().find(|t| t.handler == Handler::Video);
        let (w, h) = v.map_or((FRAME_W, FRAME_H), |t| (t.width as usize, t.height as usize));
        let dec = v.and_then(|t| Dec::for_fourcc(&t.fourcc, w, h));
        Ok(MoviePlayer {
            movie,
            data,
            path: path.to_path_buf(),
            dec,
            alt: Vec::new(),
            cur: None,
            text_samples: Default::default(),
            rgb: vec![0; w * h * 3],
            width: w,
            height: h,
        })
    }

    pub fn has_video(&self) -> bool {
        self.dec.is_some()
    }

    /// Bytes held in memory (the file + the frame buffer), for cache accounting.
    pub fn mem_bytes(&self) -> usize {
        self.data.len() + self.rgb.len() + self.text_samples.values().map(Vec::len).sum::<usize>()
    }

    pub fn duration_ms(&self) -> u64 {
        self.movie.duration_ms()
    }

    /// H340: the captions (text tracks) shown at movie time `t_ms`, back to front. A movie read by ranges (the web
    /// front) reads each text sample once, on first show.
    pub fn captions_at(&mut self, t_ms: u64) -> Vec<captions::Caption> {
        if !self.data.is_empty() || !crate::io::ranged() {
            return captions::text_captions(&self.movie, &self.data, t_ms);
        }
        let (path, cache) = (&self.path, &mut self.text_samples);
        captions::text_captions_by(&self.movie, t_ms, |o, n| {
            cache.entry(o).or_insert_with(|| crate::io::read_range(path, o, n as usize).unwrap_or_default()).clone()
        })
    }

    /// The movie's text tracks as SubRip cues (movie-file time).
    pub fn caption_cues(&self) -> Vec<captions::Cue> {
        captions::movie_cues(&self.movie, &self.data)
    }

    fn sample(&self, tr: usize, s: usize) -> Vec<u8> {
        let (o, n) = self.movie.tracks[tr].samples[s];
        if self.data.is_empty() && crate::io::ranged() {
            return crate::io::read_range(&self.path, o, n as usize).unwrap_or_default();
        }
        self.data.get(o as usize..(o as usize + n as usize)).unwrap_or(&[]).to_vec()
    }

    /// Decode the frame shown at movie time `t_ms` into `self.rgb`. Returns false if there is no video there.
    pub fn seek(&mut self, t_ms: u64) -> Result<bool, String> {
        let Some((tr, s)) = self.movie.video_sample_at(t_ms) else { return Ok(false) };
        if self.cur == Some((tr, s)) {
            return Ok(true);
        }
        if self.dec.is_none() {
            return Ok(false);
        }
        // H1940: the sample's own codec (a mixed-codec track: NAV2..NAV6.NMV track 1 sample 2 is 'cvid' in a 'jpeg'
        // track); a codec without a decoder = no video there
        let fourcc = self.movie.tracks[tr].sample_fourcc(s);
        let Some(kind) = Dec::kind_of(&fourcc) else { return Ok(false) };
        let di = if self.dec.as_ref().map(Dec::kind) == Some(kind) {
            None
        } else if let Some(i) = self.alt.iter().position(|d| d.kind() == kind) {
            Some(i)
        } else {
            let Some(d) = Dec::for_fourcc(&fourcc, self.width, self.height) else { return Ok(false) };
            self.alt.push(d);
            Some(self.alt.len() - 1)
        };
        let is_jpeg = kind == Dec::kind_of(b"jpeg").unwrap();
        match is_jpeg {
            true => {
                let bytes = self.sample(tr, s);
                let fields = decode_jpeg_sample(&bytes)?;
                self.rgb.resize(self.width * self.height * 3, 0);
                for (fi, (px, info)) in fields.iter().enumerate() {
                    let (w, h) = (info.width as usize, info.height as usize);
                    for y in 0..h {
                        let rows = if fields.len() == 2 {
                            [y * 2 + fi, usize::MAX]
                        } else if h * 2 == self.height {
                            [y * 2, y * 2 + 1]
                        } else {
                            [y, usize::MAX]
                        };
                        for dy in rows {
                            if dy >= self.height {
                                continue;
                            }
                            for x in 0..w.min(self.width) {
                                let o = (dy * self.width + x) * 3;
                                match info.pixel_format {
                                    jpeg_decoder::PixelFormat::RGB24 => {
                                        self.rgb[o..o + 3].copy_from_slice(&px[(y * w + x) * 3..(y * w + x) * 3 + 3])
                                    }
                                    jpeg_decoder::PixelFormat::L8 => self.rgb[o..o + 3].fill(px[y * w + x]),
                                    _ => {}
                                }
                            }
                        }
                    }
                }
            }
            false => {
                // inter frames: decode forward from the current sample, or from the start of the run of samples of
                // this codec (0 in a one-codec track) when seeking back / other track / after a sample of another codec
                let t = &self.movie.tracks[tr];
                let mut run = s;
                while run > 0 && Dec::kind_of(&t.sample_fourcc(run - 1)) == Some(kind) {
                    run -= 1;
                }
                let from = match self.cur {
                    Some((t0, s0)) if t0 == tr && s0 < s && s0 >= run => s0 + 1,
                    _ => run,
                };
                for i in from..=s {
                    let bytes = self.sample(tr, i);
                    let d = match di {
                        None => self.dec.as_mut(),
                        Some(k) => self.alt.get_mut(k),
                    };
                    match d {
                        Some(Dec::Cvid(c)) => c.decode(&bytes)?,
                        Some(Dec::Rpza(r)) => r.decode(&bytes)?,
                        _ => {}
                    }
                }
                let d = match di {
                    None => self.dec.as_ref(),
                    Some(k) => self.alt.get(k),
                };
                match d {
                    Some(Dec::Cvid(c)) => self.rgb.copy_from_slice(&c.rgb),
                    Some(Dec::Rpza(r)) => self.rgb.copy_from_slice(&r.rgb),
                    _ => {}
                }
            }
        }
        self.cur = Some((tr, s));
        Ok(true)
    }
}

/// Decode one 'jpeg' / 'mjpa' video sample: one baseline JPEG, or (Motion-JPEG A) an interlaced frame of two fields,
/// the second at the APP1 'mjpg' header's "offset to next field" — field 1 on the even lines, field 2 on the odd ones.
pub fn decode_jpeg_sample(bytes: &[u8]) -> Result<Vec<(Vec<u8>, jpeg_decoder::ImageInfo)>, String> {
    let next = mjpa_next_field(bytes).filter(|&o| o < bytes.len());
    let fields: Vec<&[u8]> = match next {
        Some(o) => vec![&bytes[..o], &bytes[o..]],
        None => vec![bytes],
    };
    let mut out = Vec::with_capacity(fields.len());
    for fb in fields {
        let mut d = jpeg_decoder::Decoder::new(fb);
        let px = d.decode().map_err(|e| format!("jpeg: {e}"))?;
        let info = d.info().ok_or("jpeg: no info")?;
        out.push((px, info));
    }
    Ok(out)
}

/// Motion-JPEG A: the APP1 'mjpg' header of a field (QuickTime File Format, "Motion-JPEG"): reserved (4), tag 'mjpg', field
/// size, padded field size, offset to next field — the last one when it is not 0
fn mjpa_next_field(b: &[u8]) -> Option<usize> {
    let mut i = 2;
    while i + 4 <= b.len() && b[i] == 0xff {
        let m = b[i + 1];
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if m == 0xe1 && b.get(i + 8..i + 12) == Some(b"mjpg") {
            let o = b.get(i + 20..i + 24)?;
            let o = u32::from_be_bytes([o[0], o[1], o[2], o[3]]) as usize;
            return (o != 0).then_some(o);
        }
        if m == 0xda {
            return None;
        }
        i += 2 + len;
    }
    None
}
