//! C307 (`traces/fo-262.jsonl`, `traces/fo-263.jsonl`, check PS1 / H2270): the home computer's photo album shows a
//! camera shot's photo as its own video frame crop (the original re-renders f64 at f68 and copies rect, FUN_0061d180),
//! the camera icon's plate photo as TakePhoto's picture 0x1372e. The port, driven along the bot's steps from the same
//! saves, draws the same pictures: block means (40×40 of the 160×120 enlargement at (420,32), 40×30 of the 80×60
//! thumbnails at (138,165) / (224,165)) measured on the original's screenshots `fo-262.jsonl.shots/k14.png` and
//! `fo-263.jsonl.shots/k15.png`. Skipped without the game data.
use std::path::PathBuf;
use xf::front::{self, script, Session};
use xf::hdb::Hdb;

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..")
}

/// fo-262 / fo-263: PDA map → home → desk 0xf20, the computer, shiloh, PHOTO, Download, thumbnail 0
const ROUTE: &str = "m:20,440;p:21,440;p:239,322;p:286,179;p:300,211;p:70,210;p:320,210;p:259,210;p:318,200;t:shiloh;k:13;p:68,164;p:171,124;p:178,195";

fn render_after(save: &str, steps: &str, rules: xf::world::Rules) -> Option<(Vec<u8>, Session<'static>)> {
    let d = root().join("data/InstalledGame");
    let save = root().join("data/bot").join(save);
    if !d.join("XFiles.hdb").exists() || !save.exists() {
        return None;
    }
    let h = Box::leak(Box::new(Hdb::open(d.join("XFiles.hdb")).unwrap()));
    let g = Box::leak(Box::new(Hdb::open(&save).unwrap()));
    let mut s = Session::with_rules(h, g, &d, rules);
    s.r.load_game();
    front::settle_timers(&mut s, 100, 900_000);
    for i in script::parse(steps).unwrap() {
        script::apply(&mut s, &i);
        front::settle_timers(&mut s, 100, 900_000);
    }
    Some((s.render(), s))
}

fn blocks(fb: &[u8], x0: usize, y0: usize, w: usize, h: usize, bw: usize, bh: usize) -> Vec<[i32; 3]> {
    let mut out = Vec::new();
    for by in (0..h).step_by(bh) {
        for bx in (0..w).step_by(bw) {
            let mut sum = [0i64; 3];
            for j in 0..bh {
                for i in 0..bw {
                    let o = ((y0 + by + j) * front::SCREEN_W + x0 + bx + i) * 3;
                    for c in 0..3 {
                        sum[c] += fb[o + c] as i64;
                    }
                }
            }
            out.push(sum.map(|v| (v / (bw * bh) as i64) as i32));
        }
    }
    out
}

/// mean absolute difference per channel over the blocks
fn diff(a: &[[i32; 3]], b: &[[i32; 3]]) -> i32 {
    let s: i32 = a.iter().zip(b).map(|(x, y)| (0..3).map(|c| (x[c] - y[c]).abs()).sum::<i32>()).sum();
    s / (a.len() as i32 * 3)
}

const K14_262_ENL: [[i32; 3]; 12] = [
    [59, 65, 63], [77, 82, 76], [64, 69, 63], [64, 70, 54], [49, 51, 48], [52, 54, 51],
    [48, 52, 51], [50, 53, 43], [52, 51, 42], [55, 55, 44], [54, 54, 44], [55, 55, 44],
];
const K14_262_S0: [[i32; 3]; 4] = [[61, 66, 63], [59, 64, 56], [54, 54, 46], [53, 54, 44]];
const K15_263_ENL: [[i32; 3]; 12] = [
    [60, 65, 61], [59, 64, 59], [67, 72, 68], [100, 104, 97], [44, 46, 46], [33, 37, 35],
    [33, 36, 35], [51, 56, 52], [43, 43, 38], [51, 51, 45], [45, 47, 41], [36, 37, 33],
];
const K15_263_S0: [[i32; 3]; 4] = [[34, 40, 44], [39, 44, 46], [39, 40, 36], [49, 48, 45]];
const K15_263_S1: [[i32; 3]; 4] = [[54, 59, 57], [74, 78, 74], [44, 44, 39], [37, 39, 35]];

#[test]
fn c307_album_shows_the_shot_as_its_frame_crop() {
    let Some((fb, s)) = render_after("fo-259-end.x", ROUTE, front::rules_hyp()) else { return };
    assert_eq!(s.r.w.pc.photos, vec![(0x5bbe, true)], "the shot photo, downloaded");
    assert_eq!(s.r.w.pc.enlarged, Some(0));
    let enl = blocks(&fb, 420, 32, 160, 120, 40, 40);
    let s0 = blocks(&fb, 138, 165, 80, 60, 40, 30);
    eprintln!("fo-262: enlargement {} thumbnail {}", diff(&enl, &K14_262_ENL), diff(&s0, &K14_262_S0));
    assert!(diff(&enl, &K14_262_ENL) <= 8, "fo-262 k14 enlargement: {} {enl:?}", diff(&enl, &K14_262_ENL));
    assert!(diff(&s0, &K14_262_S0) <= 8, "fo-262 k14 thumbnail: {} {s0:?}", diff(&s0, &K14_262_S0));
    // not the plate picture (fo-263's slot 0 / enlargement after slot 0 = 0x1372e)
    assert!(diff(&s0, &K15_263_S0) > 10, "the shot is not drawn as the plate");

    let Some((fb, s)) = render_after("fo-261-end.x", &format!("{ROUTE};p:264,195"), front::rules_hyp()) else { return };
    assert_eq!(s.r.w.pc.photos, vec![(0x1372e, true), (0x5bbe, true)]);
    assert_eq!(s.r.w.pc.enlarged, Some(1));
    let enl = blocks(&fb, 420, 32, 160, 120, 40, 40);
    let s0 = blocks(&fb, 138, 165, 80, 60, 40, 30);
    let s1 = blocks(&fb, 224, 165, 80, 60, 40, 30);
    eprintln!("fo-263: slot 0 {} slot 1 {} enlargement {}", diff(&s0, &K15_263_S0), diff(&s1, &K15_263_S1), diff(&enl, &K15_263_ENL));
    assert!(diff(&s0, &K15_263_S0) <= 8, "fo-263 k15 slot 0 (plate): {} {s0:?}", diff(&s0, &K15_263_S0));
    assert!(diff(&s1, &K15_263_S1) <= 8, "fo-263 k15 slot 1 (shot): {} {s1:?}", diff(&s1, &K15_263_S1));
    assert!(diff(&enl, &K15_263_ENL) <= 8, "fo-263 k15 enlargement: {} {enl:?}", diff(&enl, &K15_263_ENL));
}
