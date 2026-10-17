//! Closed captions (RE_NOTES H340–H344): the game's captions are QuickTime **text tracks** inside the movies, drawn by
//! QuickTime itself; the engine only switches them on or off (Pref_bClosedCaptioning, var 0xa0d) and makes room for them
//! under the View. This module reads them (display only) and reads user-supplied `.srt` files in the same form.
//!
//! Text sample description ('text' stsd entry, Apple QuickTime File Format): display flags, justification, background
//! colour, default text box, default style (start char, line height, ascent, font id, face, size, colour), font name.
//! Every text track of the installed (RU) data has: flags 0, justification 1 (centred), background black, box
//! (2,2)–(598,18) in a 600×20 track translated to (0,240) of the movie (just under the 600×240 picture), Geneva (font 3)
//! 12, plain, white, line height 16, ascent 12. A sample = u16 text length + the text (Mac Roman) + optional atoms
//! ('tbox' overrides the box; 'styl' style runs — only the first run's size / colour is used here).
//!
//! User subtitles (`--subtitles-dir DIR`, H344): `DIR/<movie path>.srt` (e.g. `DIR/XV/23434.srt` for `XV/23434.xmv`), UTF-8
//! SubRip: blocks of an optional counter line, `HH:MM:SS,mmm --> HH:MM:SS,mmm`, text lines, a blank line. Times are the
//! movie **file's** time (as the text tracks), not the asset's range. Or `DIR/<movie path>` itself: a movie with text
//! tracks (e.g. the English retail CD's XV folder copied there) whose captions replace the installed movie's.
use super::qt::{atoms, be16, be32, Handler, Movie};

/// QuickTime display flags (Movies.h) the drawing honours
pub const DF_DONT_DISPLAY: u32 = 0x1;
pub const DF_KEYED_TEXT: u32 = 0x4000;

/// A text track's sample description.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TextDesc {
    pub display_flags: u32,
    /// 0 = left (teFlushDefault), 1 = centre, −1 = right, −2 = left
    pub justify: i32,
    pub bg: [u8; 3],
    /// default text box (left, top, right, bottom), track coordinates
    pub text_box: [i32; 4],
    pub line_h: i32,
    pub ascent: i32,
    pub font_id: u16,
    pub face: u8,
    pub size: u16,
    pub fg: [u8; 3],
    pub font_name: String,
}

fn rgb(b: &[u8], o: usize) -> [u8; 3] {
    [(be16(b, o) >> 8) as u8, (be16(b, o + 2) >> 8) as u8, (be16(b, o + 4) >> 8) as u8]
}

fn rect(b: &[u8], o: usize) -> [i32; 4] {
    // QuickDraw Rect: top, left, bottom, right
    let v = |i: usize| be16(b, o + 2 * i) as i16 as i32;
    [v(1), v(0), v(3), v(2)]
}

impl TextDesc {
    /// `e` = the stsd entry from its size field.
    pub fn parse(e: &[u8]) -> Option<TextDesc> {
        if e.len() < 58 || &e[4..8] != b"text" {
            return None;
        }
        let n = e[58] as usize;
        let font_name = e.get(59..59 + n).map(|b| b.iter().map(|&c| mac_roman(c)).collect()).unwrap_or_default();
        Some(TextDesc {
            display_flags: be32(e, 16),
            justify: be32(e, 20) as i32,
            bg: rgb(e, 24),
            text_box: rect(e, 30),
            line_h: be16(e, 42) as i32,
            ascent: be16(e, 44) as i32,
            font_id: be16(e, 46),
            face: e[48],
            size: be16(e, 50),
            fg: rgb(e, 52),
            font_name,
        })
    }
}

/// One caption on screen: what a text track shows at a movie time.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Caption {
    /// lines (a CR / LF in the sample breaks the line); may be blank — the box is still drawn
    pub text: String,
    /// the track's rect in movie coordinates (left, top, right, bottom): filled with `bg` unless keyed
    pub track: [i32; 4],
    /// the text box in movie coordinates
    pub text_box: [i32; 4],
    pub justify: i32,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub keyed: bool,
    pub ascent: i32,
    pub line_h: i32,
    pub size: u16,
    pub layer: i16,
}

/// Mac Roman → char (0x80..0xff by the Apple table; control characters kept).
pub fn mac_roman(c: u8) -> char {
    const HI: &str = "ÄÅÇÉÑÖÜáàâäãåçéèêëíìîïñóòôöõúùûü†°¢£§•¶ß®©™´¨≠ÆØ∞±≤≥¥µ∂∑∏π∫ªºΩæø¿¡¬√ƒ≈∆«»…\u{a0}ÀÃÕŒœ–—“”‘’÷◊ÿŸ⁄€‹›ﬁﬂ‡·‚„‰ÂÊÁËÈÍÎÏÌÓÔ\u{f8ff}ÒÚÛÙıˆ˜¯˘˙˚¸˝˛ˇ";
    if c < 0x80 {
        c as char
    } else {
        HI.chars().nth((c - 0x80) as usize).unwrap_or('?')
    }
}

/// Decode a text sample: (text, 'tbox' override, first 'styl' run's (size, colour)).
pub fn decode_sample(b: &[u8]) -> (String, Option<[i32; 4]>, Option<(u16, [u8; 3])>) {
    let n = be16(b, 0) as usize;
    let raw = b.get(2..2 + n).unwrap_or(&[]);
    let text: String = raw.iter().map(|&c| mac_roman(c)).collect();
    let (mut tbox, mut style) = (None, None);
    if let Some(rest) = b.get(2 + n..) {
        for (t, body) in atoms(rest) {
            match &t {
                b"tbox" if body.len() >= 8 => tbox = Some(rect(body, 0)),
                // count, then ScrpSTElement records (start char 4, height 2, ascent 2, font 2, face 1+1, size 2, colour 6)
                b"styl" if body.len() >= 22 => style = Some((be16(body, 2 + 14), rgb(body, 2 + 16))),
                _ => {}
            }
        }
    }
    (text, tbox, style)
}

/// The captions every text track of `movie` shows at movie time `t_ms` (all text tracks are on: the engine enables
/// every text track when captions are on, FUN_0046b560 — H341), in drawing order: back to front (higher layer first,
/// then track order — H342).
pub fn text_captions(movie: &Movie, data: &[u8], t_ms: u64) -> Vec<Caption> {
    text_captions_by(movie, t_ms, |o, n| data.get(o as usize..o as usize + n as usize).unwrap_or(&[]).to_vec())
}

/// [`text_captions`] with the samples read by `sample(file offset, size)` — a movie whose file is not held in memory
/// (the web front reads the movies by ranges, `crate::io::ranged`).
pub fn text_captions_by(movie: &Movie, t_ms: u64, mut sample: impl FnMut(u64, u32) -> Vec<u8>) -> Vec<Caption> {
    let t = t_ms * movie.timescale as u64 / 1000;
    let mut out: Vec<(i16, usize, Caption)> = Vec::new();
    for (i, tr) in movie.tracks.iter().enumerate() {
        if tr.handler != Handler::Text || tr.samples.is_empty() {
            continue;
        }
        let Some(d) = &tr.text else { continue };
        if d.display_flags & DF_DONT_DISPLAY != 0 {
            continue;
        }
        let Some(mt) = tr.media_time_at(t, movie.timescale) else { continue };
        let s = tr.sample_at(mt);
        if tr.times.get(s + 1).is_none() && mt >= tr.duration.max(1) {
            continue;
        }
        let (o, n) = tr.samples[s];
        let (text, tbox, style) = decode_sample(&sample(o, n));
        let b = tbox.unwrap_or(d.text_box);
        let (size, fg) = style.unwrap_or((d.size, d.fg));
        out.push((
            tr.layer,
            i,
            Caption {
                text,
                track: [tr.tx, tr.ty, tr.tx + tr.track_w as i32, tr.ty + tr.track_h as i32],
                text_box: [tr.tx + b[0], tr.ty + b[1], tr.tx + b[2], tr.ty + b[3]],
                justify: d.justify,
                fg,
                bg: d.bg,
                keyed: d.display_flags & DF_KEYED_TEXT != 0,
                ascent: d.ascent,
                line_h: d.line_h,
                size,
                layer: tr.layer,
            },
        ));
    }
    out.sort_by_key(|x| (std::cmp::Reverse(x.0), x.1));
    out.into_iter().map(|x| x.2).collect()
}

/// Does the movie have any text track (captions)?
pub fn has_captions(movie: &Movie) -> bool {
    movie.tracks.iter().any(|t| t.handler == Handler::Text && t.text.is_some())
}

/// A SubRip cue: movie-file time range (ms), text (lines joined by '\n').
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cue {
    pub start: u64,
    pub end: u64,
    pub text: String,
}

fn srt_time(s: &str) -> Option<u64> {
    let s = s.trim().replace('.', ",");
    let (hms, ms) = s.split_once(',').unwrap_or((&s, "0"));
    let p: Vec<u64> = hms.split(':').map(|x| x.trim().parse().ok()).collect::<Option<_>>()?;
    let (h, m, sec) = match p[..] {
        [h, m, s] => (h, m, s),
        [m, s] => (0, m, s),
        _ => return None,
    };
    let ms: u64 = ms.trim().parse().ok()?;
    Some(((h * 60 + m) * 60 + sec) * 1000 + ms)
}

/// Parse a SubRip (.srt) file (UTF-8, optional BOM, LF or CRLF). Malformed blocks are skipped.
pub fn parse_srt(src: &str) -> Vec<Cue> {
    let src = src.trim_start_matches('\u{feff}').replace("\r\n", "\n").replace('\r', "\n");
    let mut out = Vec::new();
    for block in src.split("\n\n") {
        let lines: Vec<&str> = block.lines().filter(|l| !l.trim().is_empty()).collect();
        let Some(ti) = lines.iter().position(|l| l.contains("-->")) else { continue };
        let Some((a, b)) = lines[ti].split_once("-->") else { continue };
        // a position spec after the end time ("X1:…") is ignored
        let b = b.split_whitespace().next().unwrap_or("");
        let (Some(start), Some(end)) = (srt_time(a), srt_time(b)) else { continue };
        out.push(Cue { start, end, text: lines[ti + 1..].join("\n") });
    }
    out
}

/// Text of the cues covering movie time `t_ms` (several overlapping cues: joined by '\n'), None if none.
pub fn cue_at(cues: &[Cue], t_ms: u64) -> Option<String> {
    let v: Vec<&str> = cues.iter().filter(|c| c.start <= t_ms && t_ms < c.end).map(|c| c.text.as_str()).collect();
    (!v.is_empty()).then(|| v.join("\n"))
}

/// Captions of a movie file as SubRip cues (for .srt files made from the user's own discs: the port never ships the
/// game's text). Blank samples are left out; each track's cues are listed in time order.
pub fn movie_cues(movie: &Movie, data: &[u8]) -> Vec<Cue> {
    let mts = movie.timescale.max(1) as u64;
    let mut out = Vec::new();
    for tr in movie.tracks.iter().filter(|t| t.handler == Handler::Text) {
        // walk the edit list: each non-empty edit shows media [media_time, media_time + dur) at movie time `at`
        let edits: Vec<(u64, i64, u64)> = if tr.edits.is_empty() {
            vec![(0, 0, tr.duration * mts / tr.timescale as u64)]
        } else {
            let mut at = 0;
            tr.edits
                .iter()
                .map(|e| {
                    let x = (at, e.media_time, e.dur);
                    at += e.dur;
                    x
                })
                .collect()
        };
        for (at, mt0, dur) in edits {
            if mt0 < 0 {
                continue;
            }
            let ts = tr.timescale as u64;
            let (m0, m1) = (mt0 as u64, mt0 as u64 + dur * ts / mts);
            for (s, &(o, n)) in tr.samples.iter().enumerate() {
                let s0 = tr.times[s];
                let s1 = tr.times.get(s + 1).copied().unwrap_or(tr.duration);
                let (a, b) = (s0.max(m0), s1.min(m1));
                if a >= b {
                    continue;
                }
                let bytes = data.get(o as usize..o as usize + n as usize).unwrap_or(&[]);
                let text = decode_sample(bytes).0.replace('\r', "\n").trim().to_string();
                if text.is_empty() {
                    continue;
                }
                let ms = |m: u64| (at * 1000 / mts) + (m - m0) * 1000 / ts;
                out.push(Cue { start: ms(a), end: ms(b), text });
            }
        }
    }
    out.sort_by_key(|c| (c.start, c.end));
    out
}

fn srt_stamp(ms: u64) -> String {
    format!("{:02}:{:02}:{:02},{:03}", ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000)
}

/// Write cues as SubRip text.
pub fn write_srt(cues: &[Cue]) -> String {
    let mut s = String::new();
    for (i, c) in cues.iter().enumerate() {
        s += &format!("{}\n{} --> {}\n{}\n\n", i + 1, srt_stamp(c.start), srt_stamp(c.end), c.text);
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srt_round_trip() {
        let src = "\u{feff}1\r\n00:00:01,387 --> 00:00:02,833\r\n( a door closes )\r\n\r\n2\r\n00:01:00.5 --> 00:01:02,000 X1:0\r\nline one\r\nline two\r\n";
        let c = parse_srt(src);
        assert_eq!(c.len(), 2);
        assert_eq!((c[0].start, c[0].end, c[0].text.as_str()), (1387, 2833, "( a door closes )"));
        assert_eq!((c[1].start, c[1].end, c[1].text.as_str()), (60_005, 62_000, "line one\nline two"));
        assert_eq!(cue_at(&c, 1386), None);
        assert_eq!(cue_at(&c, 1387).as_deref(), Some("( a door closes )"));
        assert_eq!(cue_at(&c, 2833), None);
        assert_eq!(parse_srt(&write_srt(&c)), c);
    }

    #[test]
    fn mac_roman_table() {
        assert_eq!(mac_roman(b'A'), 'A');
        assert_eq!(mac_roman(0x80), 'Ä');
        assert_eq!(mac_roman(0xd2), '“');
        assert_eq!(mac_roman(0xd5), '’');
        assert_eq!(mac_roman(0xc9), '…');
        assert_eq!(mac_roman(0xff), 'ˇ');
    }
}
