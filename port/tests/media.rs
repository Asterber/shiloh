//! Media decoding against data/InstalledGame (skipped when absent): AssetRef blob → file, QuickTime sample lookup,
//! Photo-JPEG and Cinepak frames. The Cinepak frame hash was checked bit-exact against ffmpeg (same RGB24 bytes).
use std::path::PathBuf;
use xf::hdb::Hdb;
use xf::media::{MediaLib, MoviePlayer};

fn data() -> Option<PathBuf> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    d.join("XFiles.hdb").exists().then_some(d)
}

fn fnv(b: &[u8]) -> u64 {
    b.iter().fold(0xcbf29ce484222325u64, |h, &x| (h ^ x as u64).wrapping_mul(0x100000001b3))
}

#[test]
fn asset_blobs_resolve_to_files() {
    let Some(d) = data() else { return };
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let lib = MediaLib::new(&d);
    let c = lib.resolve(&h, 0x4d64).unwrap(); // SC03C.mov
    assert_eq!((c.name.as_str(), c.start, c.end), ("XV/19812.xmv", 0, 0));
    let c = lib.resolve(&h, 0x13ad1).unwrap(); // View 0x2c9 background, first entry = nav1.nmv
    assert_eq!((c.name.as_str(), c.start, c.end), ("nav1.nmv", 168001, 168001));
    assert!(c.is_still());
}

#[test]
fn photo_jpeg_and_cinepak_frames() {
    let Some(d) = data() else { return };
    let mut p = MoviePlayer::open(&d.join("navm.nmv")).unwrap();
    assert_eq!(p.movie.video_sample_at(54001), Some((0, 54))); // edit list: 1 s per frame
    assert!(p.seek(54001).unwrap());
    assert!(p.rgb.iter().any(|&x| x > 64));
    let mut v = MoviePlayer::open(&d.join("XV/19812.xmv")).unwrap();
    assert_eq!(v.duration_ms(), 14600);
    assert!(v.seek(5000).unwrap()); // sample 75 at 15 fps
    assert_eq!((v.width, v.height), (600, 240));
    assert_eq!(fnv(&v.rgb), FRAME75);
}

const FRAME75: u64 = 11005125199481838257; // fnv of ffmpeg rawvideo frame 75

/// x.pff (PFF container of PICT v2 pictures): every entry decodes; DirectBitsRect pictures 2 and 469 are bit-exact with
/// an independent Python PICT decoder (checked; JPEG bands differ by ≤ 2 levels, other
/// JPEG decoder). Picture 469 (asset 0xe4ae, first AssetRef of drawer InterfaceItem 0xe4bf) has exactly the size of the
/// item's region 282,214–370,268.
#[test]
fn pff_pictures() {
    use xf::media::pict::Pff;
    let Some(d) = data() else { return };
    let p = Pff::open(&d.join("x.pff")).unwrap();
    assert_eq!(p.len(), 727);
    for i in 0..p.len() {
        p.picture(i).unwrap_or_else(|e| panic!("entry {i}: {e}"));
    }
    let a = p.picture(2).unwrap();
    assert_eq!((a.width, a.height, fnv(&a.rgb)), (90, 24, PFF2));
    let b = p.picture(469).unwrap();
    assert_eq!((b.width, b.height, fnv(&b.rgb)), (88, 54, PFF469));
    let s = p.picture(57).unwrap(); // 640×480 in nine JPEG bands placed by the matrix translation
    assert_eq!((s.width, s.height), (640, 480));
    assert!(s.rgb[(470 * 640 + 320) * 3..].iter().take(3).any(|&x| x > 100)); // last band drawn (light frame border)
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let c = MediaLib::new(&d).resolve(&h, 0xe4ae).unwrap();
    assert_eq!((c.name.as_str(), c.start, c.end), ("x.pff", 469, 469));
}
const PFF2: u64 = 0x9120a1be1882d39d;
const PFF469: u64 = 0x3677c3a7d2c92bf7;

/// IMA4 sound tracks against ffmpeg (`-f s16le`, skipped without ffmpeg): mono .amv, stereo .xmv, and an .xmv whose sound
/// edit list starts at media frame 34 — sample-exact over the common length (tails differ by < 1 packet). (Survey
/// with a probe over XS/* and every 25th XV movie: 212 files identical over the common length; 3 .amv with a two-edit
/// list differ after the 16-frame gap between the edits, which ffmpeg keeps and the port skips; QDMC not decoded.)
#[test]
fn ima4_sound_matches_ffmpeg() {
    let Some(d) = data() else { return };
    for f in ["XS/26274.amv", "XV/19812.xmv", "XV/20193.xmv"] {
        let p = d.join(f);
        let pcm = xf::media::sound(&p).unwrap();
        let Ok(out) = std::process::Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&p)
            .args(["-map", "0:a:0", "-f", "s16le", "-"])
            .output()
        else {
            return; // no ffmpeg
        };
        let r: Vec<i16> = out.stdout.chunks_exact(2).map(|x| i16::from_le_bytes([x[0], x[1]])).collect();
        assert!(!r.is_empty(), "{f}: ffmpeg gave nothing");
        // lengths: the port cuts the tail at the edit's end, ffmpeg keeps the rest of the last packet when the only edit
        // starts at media time 0 (at most one 64-frame packet)
        let n = pcm.samples.len().min(r.len());
        assert!(pcm.samples.len().abs_diff(r.len()) < 64 * pcm.channels as usize, "{f}: {} vs {}", pcm.samples.len(), r.len());
        assert!(pcm.samples[..n] == r[..n], "{f}: samples differ");
    }
    let pcm = xf::media::sound(&d.join("XV/20193.xmv")).unwrap();
    assert_eq!((pcm.rate, pcm.channels, pcm.frames()), (22050, 2, 24990)); // 680/600 s from media frame 34
}

/// 'rpza' (dialogue EmotionIcon movie XG/28647.xmv, asset 0x6fe7 indifferent.mov, active after New Game with H60): every
/// sample equals ffmpeg's RGB555 frame (a probe: all 12 rpza files of XG, 624 of 632 samples
/// compared, 0 differ; the 8 others hold two rpza chunks in one sample, which ffmpeg rejects — the port decodes the first
/// chunk, as the chunk header says). Pinned: hash of the RGB24 frame at 1 s.
#[test]
fn rpza_frames() {
    let Some(d) = data() else { return };
    let mut p = MoviePlayer::open(&d.join("XG/28647.xmv")).unwrap();
    assert_eq!((p.width, p.height), (80, 60));
    assert!(p.has_video());
    assert!(p.seek(1000).unwrap());
    assert_eq!(fnv(&p.rgb), RPZA_1S);
}
const RPZA_1S: u64 = 7993029631284444011;
