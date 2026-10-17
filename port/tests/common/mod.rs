//! Shared helpers of the integration tests (`mod common;` in each test file).
//!
//! - The game's own files (`data/InstalledGame`, the bot saves in `data/bot`) are not in the repository: tests that need
//!   them return early without them, as before.
//! - The traces of the original (`traces/`, override with `XF_TRACES=DIR`) and the generated research tables
//!   (`generated/`) are not part of the public tree either: [`trace_text`] / [`generated_text`] return None with a note
//!   on stderr, and the test returns early.
//! - Expected game strings (logins, search keys, cheat codes, phone numbers, captions …) are never spelled out in the
//!   tests: they are read from the user's own HDB / exe at test time by object id ([`game_texts`], [`text_of`]), or
//!   compared by hash ([`fnv64`]).
#![allow(dead_code)]
use std::path::PathBuf;
use xf::front::Session;
use xf::hdb::Hdb;
use xf::world::{Rules, World};

/// The repository root (the parent of `port/`).
pub fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// The installed game folder, None (with a note) when it is not there.
pub fn data() -> Option<PathBuf> {
    let d = root().join("data/InstalledGame");
    if d.join("XFiles.hdb").exists() {
        Some(d)
    } else {
        eprintln!("skipping: {} not present", d.display());
        None
    }
}

/// The traces folder: `XF_TRACES` when set, else `traces/` of the repository.
pub fn traces_dir() -> PathBuf {
    std::env::var_os("XF_TRACES").map(PathBuf::from).unwrap_or_else(|| root().join("traces"))
}

/// A trace file's path (`name` relative to the traces folder, e.g. "fo-009.jsonl"); None with a note when it is absent
/// (a public checkout has no traces). A `.gz` next to it counts too (read by [`trace_text`]).
pub fn trace_path(name: &str) -> Option<PathBuf> {
    let p = traces_dir().join(name);
    if p.exists() || PathBuf::from(format!("{}.gz", p.display())).exists() {
        Some(p)
    } else {
        eprintln!("skipping: trace {} not present (traces of the original are not in the public tree)", p.display());
        None
    }
}

/// A trace file's text (plain or `.gz`), None with a note when it is absent.
pub fn trace_text(name: &str) -> Option<String> {
    let p = trace_path(name)?;
    Some(xf::e2e::read_trace(&p).unwrap_or_else(|| panic!("{}: unreadable", p.display())))
}

/// A parsed trace, None with a note when it is absent.
pub fn trace(name: &str) -> Option<Vec<xf::replay::TraceEv>> {
    let t = trace_text(name)?;
    Some(xf::replay::parse_trace(&t).unwrap_or_else(|e| panic!("{name}: {e:?}")))
}

/// A file under `generated/` (research tables, not in the public tree), None with a note when it is absent.
pub fn generated_text(rel: &str) -> Option<String> {
    let p = root().join("generated").join(rel);
    match std::fs::read_to_string(&p) {
        Ok(t) => Some(t),
        Err(_) => {
            eprintln!("skipping: {} not present (generated research tables are not in the public tree)", p.display());
            None
        }
    }
}

/// A text AssetRef's string as the game shows it (inline text or a string resource of the user's install).
pub fn text_of(w: &World, asset: u32) -> String {
    xf::world::procs::computer::asset_string(w, asset).unwrap_or_else(|| panic!("AssetRef {asset:#x} has no text"))
}

/// The strings of text AssetRefs, read from the user's install (every rule on, whatever rules the test itself uses);
/// None without the game data.
pub fn game_texts<const N: usize>(assets: [u32; N]) -> Option<[String; N]> {
    let d = root().join("data/InstalledGame");
    if !d.join("XFiles.hdb").exists() {
        return None;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(d.join("XFiles.gam")).unwrap();
    let s = Session::with_rules(&h, &g, &d, Rules::all());
    Some(assets.map(|a| text_of(&s.r.w, a)))
}

/// FNV-1a (64 bit) of a string's UTF-8 bytes: game text compared without spelling it out.
pub fn fnv64(s: &str) -> u64 {
    s.bytes().fold(0xcbf2_9ce4_8422_2325, |h, b| (h ^ b as u64).wrapping_mul(0x0100_0000_01b3))
}
