//! Where the game data is found without `--data` (port feature, `xf::locate`): the search order, the remembered
//! path, bare path arguments and the scan next to the program — on temp folders with made-up marker files and tiny
//! synthetic disc images (no game data needed).
use std::path::{Path, PathBuf};
use xf::iso::build::image;
use xf::locate::{dirs_to_scan, resolve, scan, Origin, Search};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("xf-locate-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d.canonicalize().unwrap()
}

/// An installed game folder: the two database files (made-up bytes).
fn install(d: &Path) -> PathBuf {
    std::fs::create_dir_all(d).unwrap();
    std::fs::write(d.join("XFiles.hdb"), b"hdb").unwrap();
    std::fs::write(d.join("XFiles.gam"), b"gam").unwrap();
    d.to_path_buf()
}

fn ru_disc() -> Vec<u8> {
    image("XFILES1", &[("MININST/XFiles.hdb", b"hdb"), ("MININST/XFiles.gam", b"gam"), ("NAV1.NMV", b"n")], true, 2048)
}

fn en_dvd() -> Vec<u8> {
    image(
        "XFILES_DVD",
        &[("English/MININST/XFILES.HDB", b"hdb"), ("English/MININST/XFILES.GAM", b"gam"), ("English/XV/1.xmv", b"m")],
        true,
        2048,
    )
}

/// An ISO image with no game files (another program's disc).
fn other_disc() -> Vec<u8> {
    image("UBUNTU", &[("README.TXT", b"hello"), ("casper/vmlinuz", b"k")], true, 2048)
}

fn quiet() -> impl FnMut(String) {
    |l| eprintln!("{l}")
}

#[test]
fn explicit_steps_win_in_order() {
    let d = tmp("order");
    let a = install(&d.join("a"));
    let b = install(&d.join("b"));
    let c = install(&d.join("c"));
    let base = Search { legacy: Some(install(&d.join("legacy"))), beside: vec![d.clone()], ..Default::default() };
    // --data first; bare paths join it
    let s = Search { flags: vec![a.clone()], env: vec![b.clone()], args: vec![c.clone()], ..base.clone() };
    let v = s.candidates(&mut quiet());
    assert_eq!(v.len(), 1);
    assert_eq!((v[0].origin, v[0].sources.clone()), (Origin::Flag, vec![a.clone(), c.clone()]));
    // XF_DATA over bare paths
    let s = Search { env: vec![b.clone()], args: vec![c.clone()], ..base.clone() };
    let v = s.candidates(&mut quiet());
    assert_eq!((v[0].origin, v[0].sources.clone()), (Origin::Env, vec![b.clone()]));
    // bare paths (drag and drop), several like repeated --data
    let s = Search { args: vec![c.clone(), a.clone()], ..base.clone() };
    let v = s.candidates(&mut quiet());
    assert_eq!((v.len(), v[0].origin, v[0].sources.clone()), (1, Origin::Args, vec![c.clone(), a.clone()]));
    // then the source tree's data/InstalledGame
    let v = base.candidates(&mut quiet());
    assert_eq!(v[0].origin, Origin::Legacy);
    assert!(Origin::Args.remember() && Origin::Beside.remember() && Origin::Picked.remember());
    assert!(!Origin::Flag.remember() && !Origin::Env.remember() && !Origin::Legacy.remember());
    assert!(!Origin::Remembered.remember());
}

#[test]
fn remembered_path_only_while_it_exists() {
    let d = tmp("remembered");
    let r = install(&d.join("mine"));
    let near = tmp("remembered-near");
    install(&near.join("Game"));
    let s = Search { remembered: vec![r.clone()], beside: vec![near.clone()], ..Default::default() };
    let v = s.candidates(&mut quiet());
    assert_eq!((v[0].origin, v[0].sources.clone()), (Origin::Remembered, vec![r.clone()]));
    let (root, f) = resolve(v, &mut quiet()).unwrap();
    assert_eq!((root, f.origin), (r.clone(), Origin::Remembered));
    // gone: the scan next to the program instead
    std::fs::remove_dir_all(&r).unwrap();
    assert!(s.candidates(&mut quiet()).is_empty());
    let (root, f) = s.resolve(&mut quiet()).unwrap();
    assert_eq!((root, f.origin, f.sources), (near.join("Game"), Origin::Beside, vec![near.join("Game")]));
    // a remembered folder that no longer holds the game: the next step
    let empty = d.join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    let s = Search { remembered: vec![empty], beside: vec![near.clone()], ..Default::default() };
    let (root, f) = s.resolve(&mut quiet()).unwrap();
    assert_eq!((root, f.origin), (near.join("Game"), Origin::Beside));
}

#[test]
fn settings_file_keeps_the_data_path() {
    let d = tmp("settings");
    let cfg = d.join("cfg/settings.txt");
    assert!(xf::locate::remembered(&cfg).is_empty());
    std::fs::create_dir_all(cfg.parent().unwrap()).unwrap();
    std::fs::write(&cfg, "volume = 40\n").unwrap();
    let g = install(&d.join("My Game = copy"));
    let iso = d.join("cd2.iso");
    std::fs::write(&iso, ru_disc()).unwrap();
    xf::locate::remember(&cfg, &[g.clone(), iso.clone()]).unwrap();
    assert_eq!(xf::locate::remembered(&cfg), vec![g.clone(), iso.clone()]);
    let text = std::fs::read_to_string(&cfg).unwrap();
    assert!(text.contains("volume = 40"), "{text}");
    // not an option of the screen: the web dialog's keys and the menu stay as they are
    assert!(xf::front::settings::opt(xf::front::settings::DATA).is_none());
}

#[test]
fn bare_path_without_game_data_is_an_error() {
    let d = tmp("args");
    let junk = d.join("photo.iso");
    std::fs::write(&junk, b"not a disc").unwrap();
    let empty = d.join("folder");
    std::fs::create_dir_all(&empty).unwrap();
    for p in [junk, empty, d.join("missing")] {
        let s = Search { args: vec![p.clone()], legacy: Some(install(&d.join("legacy"))), ..Default::default() };
        match resolve(s.candidates(&mut quiet()), &mut quiet()) {
            Err(Some((f, e))) => assert_eq!(f.origin, Origin::Args, "{e}"),
            other => panic!("{}: {other:?}", p.display()),
        }
    }
}

#[test]
fn scan_finds_install_folders_and_images_one_level_deep() {
    let d = tmp("scan");
    // unrelated things next to the program: left out
    std::fs::write(d.join("other.iso"), other_disc()).unwrap();
    std::fs::write(d.join("firmware.bin"), b"\x7fELF not an image").unwrap();
    std::fs::create_dir_all(d.join("web")).unwrap();
    std::fs::write(d.join("web/index.html"), b"<html>").unwrap();
    std::fs::create_dir_all(d.join("Photos")).unwrap();
    // two levels down: too deep
    install(&d.join("deep/er/Game"));
    // a hidden folder: left out
    install(&d.join(".cache"));
    assert!(scan(std::slice::from_ref(&d), &mut quiet()).is_empty());
    // an installed game in a sub-folder
    let g = install(&d.join("X-Files"));
    let v = scan(std::slice::from_ref(&d), &mut quiet());
    assert_eq!(v.len(), 1);
    assert_eq!((v[0].origin, v[0].sources.clone()), (Origin::Beside, vec![g.clone()]));
    // the images of a folder are one set (the unrelated image is not part of it)
    std::fs::create_dir_all(d.join("discs")).unwrap();
    std::fs::write(d.join("discs/xfiles1.iso"), ru_disc()).unwrap();
    std::fs::write(d.join("discs/movies.img"), other_disc()).unwrap();
    let v = scan(std::slice::from_ref(&d), &mut quiet());
    assert_eq!(v.len(), 2, "{v:?}");
    assert!(v.iter().any(|f| f.sources == vec![d.join("discs/xfiles1.iso")]), "{v:?}");
    // the program's own folder is an installed game
    let own = tmp("scan-own");
    install(&own);
    let v = scan(std::slice::from_ref(&own), &mut quiet());
    assert_eq!(v[0].sources, vec![own.clone()]);
}

#[test]
fn scan_prefers_complete_then_english() {
    let d = tmp("prefer");
    // a disc with movies only (no database): left out
    std::fs::write(d.join("xfiles2.iso"), image("XFILES2", &[("XV/2.xmv", b"m"), ("NAV2.NMV", b"n")], false, 2048))
        .unwrap();
    std::fs::create_dir_all(d.join("ru")).unwrap();
    std::fs::write(d.join("ru/xfiles1.iso"), ru_disc()).unwrap();
    std::fs::create_dir_all(d.join("zz-en")).unwrap();
    std::fs::write(d.join("zz-en/XFILES_DVD.iso"), en_dvd()).unwrap();
    let v = scan(std::slice::from_ref(&d), &mut quiet());
    assert_eq!(v.len(), 2, "{v:?}");
    assert_eq!(v[0].sources, vec![d.join("zz-en/XFILES_DVD.iso")], "{v:?}");
    assert!(v[0].why.contains("English"), "{}", v[0].why);
    // a more complete set wins over the English one: an installed game with every essential file
    let full = install(&d.join("full"));
    for f in ["XFiles.exe", "XFilesT.dll", "Jrn.ttr"] {
        std::fs::write(full.join(f), b"x").unwrap();
    }
    let v = scan(std::slice::from_ref(&d), &mut quiet());
    assert_eq!(v[0].sources, vec![full], "{v:?}");
    assert!(v[0].why.starts_with("complete"), "{}", v[0].why);
}

#[test]
fn scan_dirs_skip_home_and_roots() {
    let exe = Path::new("/opt/shiloh");
    let home = Path::new("/home/me");
    assert_eq!(dirs_to_scan(Some(exe), Some(home), Some(home)), vec![exe.to_path_buf()]);
    assert_eq!(dirs_to_scan(Some(exe), Some(Path::new("/")), Some(home)), vec![exe.to_path_buf()]);
    assert_eq!(dirs_to_scan(Some(exe), Some(exe), Some(home)), vec![exe.to_path_buf()]);
    assert_eq!(
        dirs_to_scan(Some(exe), Some(Path::new("/games")), Some(home)),
        vec![exe.to_path_buf(), PathBuf::from("/games")]
    );
}
