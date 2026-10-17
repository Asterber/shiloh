//! Wong's boat: what is heard during Asta's conversation (C295 / H1970; `traces/fo-136.jsonl`). The
//! original plays there only the restored 'Dock' loop 0x6f5e and the random boat sounds of trigger 0x122a1 (Timer 0x19,
//! RandGen on 0x5c7 → CopCall / CopRadio / Dispatch / Boat / CityBell / Gullsx3 / Goose / OneGull / Train2), besides the
//! conversation's own movies (XV). The port, from fo-135-end.x with fo-136's clicks (the window's live RandGen), must
//! sound the same set: no other XS sound or music (a player left from another Location, a wrong list, a menu sound).
use std::collections::BTreeSet;
use std::path::PathBuf;
use xf::front::{settle_timers, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// C295: the Dock loop + 0x122a1's lists (0x122a5 … 0x13e14 → these AssetRefs)
const BOAT: [u32; 10] = [0x6f5e, 0x6f60, 0x6f62, 0x6f64, 0x6f66, 0x6f68, 0x6f6a, 0x6f6c, 0x6f6e, 0x6f74];

#[test]
fn c295_asta_conversation_sounds_are_the_boats() {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot/fo-135-end.x");
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return;
    }
    let h = Hdb::open(d.join("XFiles.hdb")).unwrap();
    let g = Hdb::open(&save).unwrap();
    for seed in [7u32, 4242, 12345] {
        let mut s = Session::with_rules(&h, &g, &d, xf::front::rules_hyp());
        s.r.w.live_rand = Some(seed);
        s.r.load_game();
        settle_timers(&mut s, 0, 0);
        assert_eq!(s.r.w.view, Some(0x11d0), "Wong's boat");
        // fo-136 k0..k12: Arley's last rows, 0x11de, Asta 0x65f6 and her rows (waits about the original's step lengths)
        let steps = [
            (465, 312, 9000),
            (476, 320, 26000),
            (448, 312, 6500),
            (490, 312, 11000),
            (70, 210, 2000),
            (174, 217, 3000),
            (192, 320, 7000),
            (213, 320, 28000),
            (224, 220, 2000),
            (189, 312, 12000),
            (199, 312, 10000),
            (199, 320, 25000),
            (206, 320, 45000),
        ];
        let mut heard: BTreeSet<u32> = BTreeSet::new();
        let mut at_asta: BTreeSet<u32> = BTreeSet::new();
        let mut xs_at_asta: BTreeSet<u32> = BTreeSet::new();
        for (x, y, wait) in steps {
            s.hover_at(x, y);
            s.click_at(x, y);
            let t0 = s.now;
            while s.now - t0 < wait {
                s.tick(16);
                for c in s.audio_cues() {
                    let xs = s.clip(c.asset).is_some_and(|k| k.name.to_ascii_lowercase().starts_with("xs/"));
                    if xs {
                        heard.insert(c.asset);
                    }
                    if s.r.w.view == Some(0x65f6) {
                        at_asta.insert(c.asset);
                        if xs {
                            xs_at_asta.insert(c.asset);
                        }
                    }
                }
            }
        }
        assert!(!at_asta.is_empty(), "seed {seed}: Asta's View 0x65f6 reached and heard");
        let other: Vec<u32> = heard.iter().copied().filter(|a| !BOAT.contains(a)).collect();
        assert!(other.is_empty(), "seed {seed}: sounds not of the boat: {other:x?} (heard {heard:x?})");
        assert!(xs_at_asta.contains(&0x6f5e), "seed {seed}: the Dock loop plays on at Asta's ({xs_at_asta:x?})");
    }
}
