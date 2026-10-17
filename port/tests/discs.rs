//! Game data from disc images (port feature, `xf::vfs` over `xf::iso`): synthetic images built in code (tiny made-up
//! files, no game bytes), merged with folders; plus an ignored check against the user's own images when present.
use std::path::{Path, PathBuf};
use xf::iso::build::image;
use xf::vfs::DataSet;

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xf-discs-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Disc 1: an install folder (MININST) with the database files, the first disc's movies at the root.
fn disc1() -> Vec<u8> {
    image(
        "XFILES1",
        &[
            ("MININST/XFiles.hdb", b"hdb-1"),
            ("MININST/XFiles.gam", b"gam-1"),
            ("MININST/XT/100.xtx", b"text"),
            ("MININST/XV/1.xmv", b"movie-1-mini"),
            ("XV/1.xmv", b"movie-1-root"),
            ("XV/1.HOT", b"HSPT"),
            ("NAV1.NMV", b"nav-1"),
            ("SETUP.EXE", b"MZ"),
        ],
        true,
        2048,
    )
}

/// Disc 2: movies only, raw 2352-byte sectors, no Joliet (upper-case 8.3 names).
fn disc2() -> Vec<u8> {
    image("XFILES2", &[("XV/2.xmv", b"movie-2"), ("XN/3.xmv", b"loop-3"), ("NAV2.NMV", b"nav-2")], false, 2352)
}

#[test]
fn discs_merge_in_layers() {
    let d = tmp("merge");
    // given in the "wrong" order: the volume labels order the discs
    std::fs::write(d.join("b.iso"), disc2()).unwrap();
    std::fs::write(d.join("a.bin"), disc1()).unwrap();
    let ds = DataSet::open(&[d.join("b.iso"), d.join("a.bin")]).unwrap();
    assert_eq!(ds.root, d.join("b.iso"));
    assert_eq!(ds.images.len(), 2);
    let layers = ds.describe();
    assert!(layers[0].ends_with("[XFILES1]/MININST"), "{layers:?}");
    assert!(layers[1].ends_with("[XFILES1]/"), "{layers:?}");
    assert!(layers[2].ends_with("[XFILES2]/"), "{layers:?}");
    // case-insensitive names; MININST first
    assert_eq!(ds.read(Path::new("xfiles.HDB")).unwrap(), b"hdb-1");
    assert_eq!(ds.read(Path::new("XV/1.XMV")).unwrap(), b"movie-1-mini");
    assert_eq!(ds.read(Path::new("xv/2.xmv")).unwrap(), b"movie-2");
    assert_eq!(ds.read(Path::new("nav2.nmv")).unwrap(), b"nav-2");
    assert_eq!(ds.size(Path::new("XN/3.XMV")).unwrap(), 6);
    let mut b = [0u8; 4];
    assert_eq!(ds.read_at(Path::new("XN/3.xmv"), 2, &mut b).unwrap(), 4);
    assert_eq!(&b, b"op-3");
    // the union of the layers; the install folder is not a sub-folder of the game folder
    let mut top: Vec<String> = ds.read_dir(Path::new("")).unwrap().into_iter().map(|e| e.name.to_lowercase()).collect();
    top.sort();
    assert_eq!(top, ["nav1.nmv", "nav2.nmv", "setup.exe", "xfiles.gam", "xfiles.hdb", "xn", "xt", "xv"]);
    let mut xv: Vec<String> =
        ds.read_dir(Path::new("XV")).unwrap().into_iter().map(|e| e.name.to_lowercase()).collect();
    xv.sort();
    assert_eq!(xv, ["1.hot", "1.xmv", "2.xmv"]);
    assert!(ds.read(Path::new("../a.bin")).is_err() && ds.read(Path::new("nope")).is_err());
    assert!(ds.is_dir(Path::new("xt")) && !ds.is_file(Path::new("xt")));
    assert!(ds.check().iter().all(|(fatal, _)| !fatal), "{:?}", ds.check());
    let _ = std::fs::remove_dir_all(&d);
}

/// An installed folder given first wins over the discs; the discs fill in what it lacks.
#[test]
fn installed_folder_first() {
    let d = tmp("installed");
    let inst = d.join("game");
    std::fs::create_dir_all(inst.join("XV")).unwrap();
    std::fs::write(inst.join("XFiles.hdb"), b"hdb-installed").unwrap();
    std::fs::write(inst.join("XV/1.xmv"), b"movie-1-installed").unwrap();
    std::fs::write(d.join("cd1.iso"), disc1()).unwrap();
    let ds = DataSet::open(&[inst.clone(), d.join("cd1.iso")]).unwrap();
    assert_eq!(ds.read(Path::new("XFiles.hdb")).unwrap(), b"hdb-installed");
    assert_eq!(ds.read(Path::new("XV/1.xmv")).unwrap(), b"movie-1-installed");
    assert_eq!(ds.read(Path::new("XFiles.gam")).unwrap(), b"gam-1");
    assert_eq!(ds.read(Path::new("xt/100.XTX")).unwrap(), b"text");
    let _ = std::fs::remove_dir_all(&d);
}

/// A folder of images: every image in it (a `.bin` that is no ISO 9660 image is skipped); `needs_vfs` decides when
/// the merged view is used at all.
#[test]
fn folder_of_images() {
    let d = tmp("folder");
    std::fs::write(d.join("x2.iso"), disc2()).unwrap();
    std::fs::write(d.join("x1.ISO"), disc1()).unwrap();
    std::fs::write(d.join("other.bin"), b"not a disc image").unwrap();
    assert!(xf::vfs::needs_vfs(&[d.clone()]));
    let ds = DataSet::open(&[d.clone()]).unwrap();
    assert_eq!(ds.images.len(), 2);
    assert_eq!(ds.read(Path::new("XV/2.XMV")).unwrap(), b"movie-2");
    // an installed folder alone is read as before (no merged view)
    let inst = d.join("inst");
    std::fs::create_dir_all(&inst).unwrap();
    std::fs::write(inst.join("XFiles.hdb"), b"h").unwrap();
    assert!(!xf::vfs::needs_vfs(&[inst.clone()]));
    assert!(xf::vfs::needs_vfs(&[inst, d.join("x1.ISO")]));
    // an empty folder is an error
    let empty = d.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(DataSet::open(&[empty]).is_err());
    let _ = std::fs::remove_dir_all(&d);
}

/// A disc with two languages (the EN DVD's `English/` and `Francais/`, each with MININST): the English one only.
#[test]
fn one_language_per_disc() {
    let d = tmp("lang");
    let img = image(
        "XFILES1",
        &[
            ("Francais/MININST/XFiles.hdb", b"hdb-fr"),
            ("Francais/XV/1.xmv", b"fr"),
            ("English/MININST/XFiles.hdb", b"hdb-en"),
            ("English/MININST/XFiles.gam", b"gam-en"),
            ("English/MEDINST/XV/2.xmv", b"en-2"),
            ("English/XV/1.xmv", b"en"),
        ],
        true,
        2048,
    );
    std::fs::write(d.join("dvd.mdf"), img).unwrap();
    let ds = DataSet::open(&[d.join("dvd.mdf")]).unwrap();
    assert_eq!(ds.read(Path::new("XFiles.hdb")).unwrap(), b"hdb-en");
    assert_eq!(ds.read(Path::new("XV/1.xmv")).unwrap(), b"en");
    assert_eq!(ds.read(Path::new("XV/2.xmv")).unwrap(), b"en-2");
    assert_eq!(ds.skipped.len(), 1);
    assert!(ds.skipped[0].ends_with("Francais"));
    let _ = std::fs::remove_dir_all(&d);
}

/// Without the database on the discs (only an installer archive): a fatal problem that says what to do.
#[test]
fn missing_database_is_reported() {
    let d = tmp("cab");
    let cab = vec![7u8; 10_000];
    let img = image("XFILES1", &[("DATA1.CAB", &cab), ("XV/1.xmv", b"m")], false, 2048);
    std::fs::write(d.join("cd1.iso"), img).unwrap();
    let ds = DataSet::open(&[d.join("cd1.iso")]).unwrap();
    let p = ds.check();
    let fatal: Vec<&String> = p.iter().filter(|x| x.0).map(|x| &x.1).collect();
    assert_eq!(fatal.len(), 2, "{p:?}");
    assert!(
        fatal[0].contains("XFiles.hdb") && fatal[0].contains(".CAB") && fatal[0].contains("--data"),
        "{}",
        fatal[0]
    );
    let _ = std::fs::remove_dir_all(&d);
}

/// Through the core's file access: mounted, the virtual folder works for `MediaLib` and `xf::io`; it is read-only;
/// other paths still reach the real file system.
#[test]
fn mounted_through_io() {
    let d = tmp("io");
    std::fs::write(d.join("cd1.iso"), disc1()).unwrap();
    std::fs::write(d.join("cd2.iso"), disc2()).unwrap();
    let root = xf::vfs::data_root(&[d.join("cd1.iso"), d.join("cd2.iso")], &mut |_| {}).unwrap();
    assert_eq!(xf::io::read(&root.join("XFILES.hdb")).unwrap(), b"hdb-1");
    assert_eq!(xf::io::read_range(&root.join("XV/2.xmv"), 6, 10).unwrap(), b"2");
    let lib = xf::media::MediaLib::new(&root);
    assert_eq!(xf::io::read(lib.find("xv/2.XMV").unwrap()).unwrap(), b"movie-2");
    assert_eq!(xf::io::read(lib.find("nav1.nmv").unwrap()).unwrap(), b"nav-1");
    assert!(lib.find("MININST/XFiles.hdb").is_none());
    assert!(xf::io::write(&root.join("x.x"), b"no").is_err());
    let real = d.join("real.txt");
    xf::io::write(&real, b"yes").unwrap();
    assert_eq!(xf::io::read(&real).unwrap(), b"yes");
    // a set without the GAM cannot start
    std::fs::write(d.join("cd3.iso"), disc2()).unwrap();
    let e = xf::vfs::data_root(&[d.join("cd3.iso")], &mut |_| {}).unwrap_err();
    assert!(e.contains("XFiles.hdb"), "{e}");
    let _ = std::fs::remove_dir_all(&d);
}

/// The user's own images (not distributed): `XF_TEST_IMAGES` (a path list, e.g. the RU CDs or the EN DVD image).
/// Opens the merged data, the HDB and GAM, and the movie index.
#[test]
#[ignore]
fn real_images() {
    let src = xf::vfs::env_sources("XF_TEST_IMAGES");
    if src.is_empty() {
        eprintln!("no disc images (set XF_TEST_IMAGES): skipped");
        return;
    }
    let root = xf::vfs::data_root(&src, &mut |l| eprintln!("{l}")).unwrap();
    let _h = xf::hdb::Hdb::open(root.join("XFiles.hdb")).unwrap();
    let _g = xf::hdb::Hdb::open(root.join("XFiles.gam")).unwrap();
    let lib = xf::media::MediaLib::new(&root);
    let movies = lib.entries().filter(|(k, _)| k.starts_with("xv/")).count();
    eprintln!("HDB and GAM open, {movies} files in XV/");
    assert!(movies > 100);
    assert!(lib.find("nav1.nmv").is_some());
}
