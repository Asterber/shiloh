//! H982: VCPhoto records (class 0x59) in saves — the original's first saves with a photo (fo-127-end.x ..,
//! after C108's sedan photo) and the port's saves keeping / adding photos (H793's album state).
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;
use xf::save::{Photo, SaveState};
use xf::world::Rules;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

fn session<'a>(h: &'a Hdb, g: &'a Hdb) -> Session<'a> {
    let mut s = Session::with_rules(h, g, &root().join("data/InstalledGame"), Rules::all());
    s.r.load_game();
    settle_timers(&mut s, 100, 120_000);
    s
}

fn open(name: &str) -> Option<Hdb> {
    let p = root().join("data/bot").join(name);
    (p.exists() && root().join("data/InstalledGame/XFiles.hdb").exists()).then(|| Hdb::open(p).unwrap())
}

/// The original's record: picture 0x1372e, not downloaded, a 2804-byte PICT thumbnail (80×60 frame); the fixed fields
/// equal the port's template for a new photo (H982), and the Load gives the album one photo in the camera (H793).
#[test]
fn h982_original_photo_record() {
    let Some(g) = open("fo-127-end.x") else { return };
    let st = SaveState::from_gam(&g);
    assert_eq!(st.photos.len(), 1);
    let ph = &st.photos[0];
    assert_eq!((ph.picture(), ph.downloaded()), (0x1372e, false));
    assert_eq!(ph.thumb.len(), 2804);
    assert_eq!(&ph.thumb[..12], &[0x0a, 0xf4, 0, 0, 0, 0, 0, 0x3c, 0, 0x50, 0, 0x11]);
    assert_eq!(Photo::new(0x1372e, false).rec, ph.rec);
    let h = Hdb::open(root().join("data/InstalledGame/XFiles.hdb")).unwrap();
    let s = session(&h, &g);
    assert_eq!(s.r.w.pc.photos, vec![(0x1372e, false)]);
}

/// A port save over fo-127-end.x keeps the photo byte for byte; a download (f95) is written and read back.
#[test]
fn h982_port_save_keeps_photo() {
    let Some(g) = open("fo-127-end.x") else { return };
    let h = Hdb::open(root().join("data/InstalledGame/XFiles.hdb")).unwrap();
    let mut s = session(&h, &g);
    let (b, _) = s.save_bytes().unwrap();
    assert_eq!(SaveState::from_gam(&Hdb::from_bytes(b)).photos, SaveState::from_gam(&g).photos);
    s.r.w.pc.photos[0].1 = true;
    let (b, _) = s.save_bytes().unwrap();
    xf::neodb::Db::new(b.clone()).validate().unwrap();
    let back = SaveState::from_gam(&Hdb::from_bytes(b));
    assert!(back.photos[0].downloaded());
    assert_eq!(back.photos[0].thumb, SaveState::from_gam(&g).photos[0].thumb);
}

/// A photo taken in a session loaded from a save without one (fo-126-end.x) adds class 0x59 to the port's save, which
/// passes the container checks and loads back into the album (before H982 the photo was lost).
#[test]
fn h982_port_save_adds_photo() {
    let Some(g) = open("fo-126-end.x") else { return };
    let h = Hdb::open(root().join("data/InstalledGame/XFiles.hdb")).unwrap();
    let mut s = session(&h, &g);
    assert!(s.r.w.pc.photos.is_empty());
    s.r.w.pc.photos.push((0x1372e, false));
    let (b, _) = s.save_bytes().unwrap();
    let db = xf::neodb::Db::new(b.clone());
    db.validate().unwrap();
    assert_eq!(db.class_list().unwrap().last(), Some(&0x59));
    let g2 = Hdb::from_bytes(b);
    let back = SaveState::from_gam(&g2).photos;
    assert_eq!(back.len(), 1);
    assert_eq!(back[0].rec, Photo::new(0x1372e, false).rec);
    // C130 / H1101: the record carries a thumbnail — the picture scaled to 80×60, a PICT v2 the port reads back
    let pic = xf::media::pict::decode(&back[0].thumb).unwrap();
    assert_eq!((pic.width, pic.height), (80, 60));
    assert!(pic.rgb.iter().any(|&v| v != 0), "the sedan's thumbnail is not black");
    let s2 = session(&h, &g2);
    assert_eq!(s2.r.w.pc.photos, vec![(0x1372e, false)]);
}

/// C130 (fo-154: the original freezes on a VCPhoto without its thumbnail blob) / H1101: the writer never writes one —
/// a thumbnail-less photo handed to `save::write` directly gets a black 80×60 PICT
#[test]
fn c130_writer_never_writes_thumbless_photo() {
    let Some(g) = open("fo-126-end.x") else { return };
    let mut st = SaveState::from_gam(&g);
    assert!(st.photos.is_empty());
    st.photos.push(Photo::new(0x1372e, true));
    let (b, _) = xf::save::write(&g, &st).unwrap();
    xf::neodb::Db::new(b.clone()).validate().unwrap();
    let back = SaveState::from_gam(&Hdb::from_bytes(b)).photos;
    assert_eq!(back.len(), 1);
    assert!(back[0].downloaded());
    let pic = xf::media::pict::decode(&back[0].thumb).unwrap();
    assert_eq!((pic.width, pic.height), (80, 60));
}
