//! Regression fingerprint of the cvid / rpza decoders (`media::cinepak2`, `media::rpza2`). The expected values were
//! taken from the previous decoders, after an oracle test had shown the two byte-identical on every
//! cvid/rpza sample of all 2382 XV/XG/XN/NAV movies (162698 cvid + 632 rpza samples). Every 50th XV/XN file, every XG
//! file and the NAV*.NMV mixed-codec tracks (H1940) are decoded frame by frame; FNV-1a over all RGB24 frames.
use std::path::PathBuf;
use xf::media::qt::{Handler, Movie};
use xf::media::{cinepak2, rpza2};

fn fnv(h: &mut u64, b: &[u8]) {
    for &x in b {
        *h = (*h ^ x as u64).wrapping_mul(0x100000001b3);
    }
}

/// (cvid frames, cvid hash, rpza frames, rpza hash); each frame hashes its RGB24 bytes and then its Ok/Err.
fn fingerprint() -> Option<(usize, u64, usize, u64)> {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    if !d.join("XV").exists() {
        return None;
    }
    let mut files: Vec<PathBuf> = Vec::new();
    for sub in ["XV", "XG", "XN"] {
        let mut v: Vec<PathBuf> = std::fs::read_dir(d.join(sub)).unwrap().map(|e| e.unwrap().path()).collect();
        v.sort();
        files.extend(v.into_iter().step_by(if sub == "XG" { 1 } else { 50 }));
    }
    files.extend((1..=7).map(|n| d.join(format!("NAV{n}.NMV"))));
    let (mut nc, mut hc, mut nr, mut hr) = (0, 0xcbf29ce484222325u64, 0, 0xcbf29ce484222325u64);
    for f in &files {
        let Ok(data) = std::fs::read(f) else { continue };
        let Some(m) = Movie::parse(&data) else { continue };
        for t in m.tracks.iter().filter(|t| t.handler == Handler::Video) {
            let (w, h) = (t.width as usize, t.height as usize);
            let (mut c, mut r) = (cinepak2::Cinepak::new(w, h), rpza2::Rpza::new(w, h));
            for (s, &(off, size)) in t.samples.iter().enumerate() {
                let Some(b) = data.get(off as usize..off as usize + size as usize) else { continue };
                match &t.sample_fourcc(s) {
                    b"cvid" => {
                        nc += 1;
                        let ok = c.decode(b).is_ok() as u8;
                        fnv(&mut hc, &c.rgb);
                        fnv(&mut hc, &[ok]);
                    }
                    b"rpza" => {
                        nr += 1;
                        let ok = r.decode(b).is_ok() as u8;
                        fnv(&mut hr, &r.rgb);
                        fnv(&mut hr, &[ok]);
                    }
                    _ => {}
                }
            }
        }
    }
    Some((nc, hc, nr, hr))
}

#[test]
fn decoders_fingerprint() {
    let Some(got) = fingerprint() else { return };
    eprintln!("fingerprint {got:?}");
    assert_eq!(got, EXPECTED);
}

const EXPECTED: (usize, u64, usize, u64) = (4362, 2096483644656459188, 632, 1301792706500001898);
