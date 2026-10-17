//! Tells whether the game data the other tests need is present. Most tests in this directory return early (and pass)
//! when `data/InstalledGame` or the bot saves in `data/bot` are missing, so a green `cargo test` without them checks
//! very little. Run `cargo test --release --no-default-features --test game_data -- --nocapture` to see the warning.
use std::path::PathBuf;

#[test]
fn game_data_present() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..");
    let hdb = root.join("data/InstalledGame/XFiles.hdb");
    let bot = root.join("data/bot");
    let saves = std::fs::read_dir(&bot)
        .map(|d| d.filter_map(|e| e.ok()).filter(|e| e.path().extension().is_some_and(|x| x == "x")).count())
        .unwrap_or(0);
    if !hdb.exists() {
        eprintln!("WARNING: {} missing: tests that need the game data pass without running", hdb.display());
    }
    if saves == 0 {
        eprintln!("WARNING: no saves in {}: tests that load a bot save pass without running", bot.display());
    }
    if hdb.exists() && saves > 0 {
        eprintln!("game data present: {} and {saves} saves in {}", hdb.display(), bot.display());
    }
}
