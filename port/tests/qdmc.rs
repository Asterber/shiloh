//! QDMC music through the PCM cache (`xf::media::pcmcache`, `xf-prep`): runs only when the game data and ffmpeg are
//! present (skipped otherwise). xf-prep converts the 14 QDMC files into a temporary cache dir with the user's ffmpeg;
//! the New Game intro music XV/64421.xmv (asset 0xfba5) then comes out of `media::sound` as non-silent PCM of the
//! track's edit length (3789/600 s = 6.315 s at 44100 Hz stereo), and XV/64657.xmv with its two edits (3600 + 78 at
//! media frame 272792) as 6.130 s.
use std::path::PathBuf;
use std::process::Command;

#[test]
fn qdmc_intro_from_ffmpeg_cache() {
    let d = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/InstalledGame");
    if !d.join("XV/64421.xmv").is_file() || Command::new("ffmpeg").arg("-version").output().is_err() {
        eprintln!("skipped: no game data or no ffmpeg in PATH");
        return;
    }
    let cache = std::env::temp_dir().join(format!("xf-qdmc-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&cache);
    let out =
        Command::new(env!("CARGO_BIN_EXE_xf-prep")).arg("--data").arg(&d).arg("--cache").arg(&cache).output().unwrap();
    let so = String::from_utf8_lossy(&out.stdout);
    assert!(out.status.success(), "xf-prep failed:\n{so}\n{}", String::from_utf8_lossy(&out.stderr));
    assert!(so.contains("QDMC: 14 converted, 0 already cached, 0 failed"), "{so}");
    // a second run keeps every entry
    let again =
        Command::new(env!("CARGO_BIN_EXE_xf-prep")).arg("--data").arg(&d).arg("--cache").arg(&cache).output().unwrap();
    assert!(String::from_utf8_lossy(&again.stdout).contains("QDMC: 0 converted, 14 already cached"));

    xf::media::pcmcache::set_dir(&cache);
    let p = xf::media::sound(&d.join("XV/64421.xmv")).unwrap();
    assert_eq!((p.rate, p.channels), (44100, 2));
    assert_eq!(p.frames(), 3789 * 44100 / 600); // the edit's length (278491 of 278528 media frames)
    let rms = (p.samples.iter().map(|&x| (x as f64).powi(2)).sum::<f64>() / p.samples.len() as f64).sqrt() / 32768.0;
    let db = 20.0 * rms.log10();
    assert!(db > -40.0, "intro music nearly silent: {db:.1} dBFS");
    let p = xf::media::sound(&d.join("XV/64657.xmv")).unwrap();
    assert_eq!(p.frames(), 3600 * 44100 / 600 + 78 * 44100 / 600);
    std::fs::remove_dir_all(&cache).unwrap();
}
