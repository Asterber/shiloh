//! `*.pff` picture containers and the QuickDraw PICT v2 pictures inside them.
//!
//! PFF (from the data, checked on x.pff, X7.PFF and every XG/*.pff): `"PFF "`, u32 LE count n,
//! u32 LE header size (= 12 + 8n), n × u32 LE end offsets (entry i spans `end[i-1] .. end[i]`, entry 0 starts at the
//! header size, the last end = file size), n × u32 LE per-entry frame durations (0 in x.pff; in the XG/*.pff
//! animations e.g. the eye cursor XG/07093.pff = 2000, 120, 1000, 1000). Every entry is a PICT v2
//! picture without the 512-byte file header. An AssetRef names an entry by its index: blob `start` = `end` = index for
//! x.pff / x7.pff stills, `0 .. k` for the XG/*.pff animations (frames).
//!
//! HYPOTHESIS H330 (static): the engine's PFF reader FUN_00445ba0 reads the count at +4, the n + 1
//! offsets from +8 (list +0x30; the first is the header size) and then n u32 values (list +0x48) — the frame durations,
//! in ms (FUN_00446100 → the frame's time; the eye's 2000 / 120 (blink) / 1000 / 1000 fit milliseconds).
//!
//! PICT (Apple QuickDraw, public spec "Imaging With QuickDraw" App. A): `u16 size, rect picFrame` (always
//! `0,0,h,w` here — no screen position), `0x0011 0x02ff` version 2, `0x0c00` header (24 bytes), then opcodes. The
//! installed files use: 0x001e DefHilite, 0x0001 Clip, 0x00a1 LongComment (Photoshop `8BIM`), 0x009a DirectBitsRect
//! (32-bit, packType 4 = per-row PackBits of planar A?RGB), 0x0098 PackBitsRect (8-bit indexed + colour table),
//! 0x8200 CompressedQuickTime ('jpeg' image description + Photo-JPEG data; the QuickTime fallback drawing after it is
//! ignored), 0x00ff end. Survey of x.pff (727 entries): 418 QuickTime JPEG, 301 DirectBitsRect, 3 PackBitsRect,
//! 4 with an unhandled pen opcode before any image (0x0009) — those are reported as errors.

use std::path::Path;

/// An RGB24 picture.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Picture {
    pub width: usize,
    pub height: usize,
    pub rgb: Vec<u8>,
    /// The alpha plane of a 32-bit DirectBitsRect with cmpCount 4 (planar A R G B rows, packType 4), one byte per
    /// pixel (0 = transparent, 255 = opaque); None = no alpha in the file, or a plane that never reaches 255 (H1023 /
    /// H1024)
    pub alpha: Option<Vec<u8>>,
}

/// A PFF container held in memory.
pub struct Pff {
    data: Vec<u8>,
    ranges: Vec<(usize, usize)>,
}

fn le32(b: &[u8], o: usize) -> Option<u32> {
    b.get(o..o + 4).map(|x| u32::from_le_bytes([x[0], x[1], x[2], x[3]]))
}
fn be16(b: &[u8], o: usize) -> Result<u16, String> {
    b.get(o..o + 2).map(|x| u16::from_be_bytes([x[0], x[1]])).ok_or_else(|| format!("pict: truncated at {o}"))
}
fn be32(b: &[u8], o: usize) -> Result<u32, String> {
    b.get(o..o + 4).map(|x| u32::from_be_bytes([x[0], x[1], x[2], x[3]])).ok_or_else(|| format!("pict: truncated at {o}"))
}

impl Pff {
    /// Bytes held in memory, for cache accounting.
    pub fn mem_bytes(&self) -> usize {
        self.data.len()
    }

    pub fn open(path: &Path) -> Result<Pff, String> {
        let data = crate::io::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        Pff::parse(data).map_err(|e| format!("{}: {e}", path.display()))
    }

    pub fn parse(data: Vec<u8>) -> Result<Pff, String> {
        if data.get(..4) != Some(b"PFF ") {
            return Err("not a PFF file".into());
        }
        let n = le32(&data, 4).ok_or("short header")? as usize;
        let hs = le32(&data, 8).ok_or("short header")? as usize;
        if hs != 12 + 8 * n || hs > data.len() {
            return Err(format!("header size {hs} != 12 + 8 × {n}"));
        }
        let mut ranges = Vec::with_capacity(n);
        let mut start = hs;
        for i in 0..n {
            let end = le32(&data, 12 + 4 * i).ok_or("short header")? as usize;
            if end < start || end > data.len() {
                return Err(format!("entry {i}: bad range {start}..{end}"));
            }
            ranges.push((start, end));
            start = end;
        }
        Ok(Pff { data, ranges })
    }

    pub fn len(&self) -> usize {
        self.ranges.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ranges.is_empty()
    }

    pub fn entry(&self, i: usize) -> Option<&[u8]> {
        self.ranges.get(i).map(|&(s, e)| &self.data[s..e])
    }

    /// H330: the per-entry frame durations (ms) of the header's second table
    pub fn durations(&self) -> Vec<u32> {
        let n = self.ranges.len();
        (0..n).map(|i| le32(&self.data, 12 + 4 * n + 4 * i).unwrap_or(0)).collect()
    }

    pub fn picture(&self, i: usize) -> Result<Picture, String> {
        decode(self.entry(i).ok_or_else(|| format!("no entry {i} (of {})", self.len()))?)
    }
}

/// PackBits (Apple TN1023) into exactly `n` bytes.
#[cfg(test)]
fn unpack_bits(src: &[u8], n: usize) -> Result<Vec<u8>, String> {
    unpack_units(src, n, 1)
}

/// PackBits over units of `u` bytes (packType 3 = 16-bit pixels packs words); counts are in units.
fn unpack_units(src: &[u8], n: usize, u: usize) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(n);
    let mut i = 0;
    while out.len() < n && i < src.len() {
        let h = src[i] as i8;
        i += 1;
        if h >= 0 {
            let c = (h as usize + 1) * u;
            out.extend_from_slice(src.get(i..i + c).ok_or("packbits: literal past the end")?);
            i += c;
        } else if h != -128 {
            let c = (1 - h as isize) as usize;
            let v = src.get(i..i + u).ok_or("packbits: run past the end")?;
            i += u;
            for _ in 0..c {
                out.extend_from_slice(v);
            }
        }
    }
    if out.len() < n {
        return Err(format!("packbits: {} of {n} bytes", out.len()));
    }
    out.truncate(n);
    Ok(out)
}

struct PixMap {
    row_bytes: usize,
    top: i32,
    left: i32,
    w: usize,
    h: usize,
    pack_type: u16,
    pixel_size: u16,
    cmp_count: u16,
}

/// PixMap record after rowBytes (bounds .. pmReserved), 44 bytes.
fn pixmap(p: &[u8], o: usize, row_bytes: usize) -> Result<PixMap, String> {
    let (top, left, bottom, right) =
        (be16(p, o)? as i16 as i32, be16(p, o + 2)? as i16 as i32, be16(p, o + 4)? as i16 as i32, be16(p, o + 6)? as i16 as i32);
    Ok(PixMap {
        row_bytes,
        top,
        left,
        w: (right - left).max(0) as usize,
        h: (bottom - top).max(0) as usize,
        pack_type: be16(p, o + 10)?,
        pixel_size: be16(p, o + 26)?,
        cmp_count: be16(p, o + 28)?,
    })
}

/// Rows of pixel data at `o` (each row PackBits-compressed with a byte / word count when rowBytes >= 8 and the pixmap
/// is packed). Returns the rows and the offset after them.
fn rows(p: &[u8], mut o: usize, pm: &PixMap, row_len: usize) -> Result<(Vec<Vec<u8>>, usize), String> {
    let mut out = Vec::with_capacity(pm.h);
    let unpacked = pm.row_bytes < 8 || pm.pack_type == 1 || (pm.pixel_size == 32 && pm.pack_type == 2);
    for _ in 0..pm.h {
        if unpacked {
            out.push(p.get(o..o + pm.row_bytes).ok_or("pict: pixel data past the end")?.to_vec());
            o += pm.row_bytes;
        } else {
            let n = if pm.row_bytes > 250 {
                o += 2;
                be16(p, o - 2)? as usize
            } else {
                o += 1;
                *p.get(o - 1).ok_or("pict: truncated")? as usize
            };
            let unit = if pm.pixel_size == 16 { 2 } else { 1 };
            out.push(unpack_units(p.get(o..o + n).ok_or("pict: row past the end")?, row_len, unit)?);
            o += n;
        }
    }
    Ok((out, o))
}

fn region_size(p: &[u8], o: usize) -> Result<usize, String> {
    Ok(be16(p, o)? as usize)
}

/// Decode a PICT v2 picture (no 512-byte header) into RGB24 of its picFrame size.
pub fn decode(p: &[u8]) -> Result<Picture, String> {
    let (ft, fl, fb, fr) = (be16(p, 2)? as i16 as i32, be16(p, 4)? as i16 as i32, be16(p, 6)? as i16 as i32, be16(p, 8)? as i16 as i32);
    if be16(p, 10)? != 0x0011 || be16(p, 12)? != 0x02ff {
        return Err("pict: not version 2".into());
    }
    let (w, h) = ((fr - fl).max(0) as usize, (fb - ft).max(0) as usize);
    let mut pic = Picture { width: w, height: h, rgb: vec![0; w * h * 3], alpha: None };
    // blit an image whose top-left is (x0, y0) in picture coordinates; `px(x, y)` gives the colour
    let blit = |pic: &mut Picture, x0: i32, y0: i32, iw: usize, ih: usize, px: &dyn Fn(usize, usize) -> [u8; 3]| {
        for y in 0..ih {
            let ty = y0 + y as i32 - ft;
            if ty < 0 || ty >= h as i32 {
                continue;
            }
            for x in 0..iw {
                let tx = x0 + x as i32 - fl;
                if tx < 0 || tx >= w as i32 {
                    continue;
                }
                let o = (ty as usize * w + tx as usize) * 3;
                pic.rgb[o..o + 3].copy_from_slice(&px(x, y));
            }
        }
    };
    let mut o = 14;
    // set once a 0x8200 band is drawn: whatever follows is fallback drawing for systems without QuickTime (the 8BIM
    // pictures add a dithered 8-bit PackBitsRect copy, truncated in x.pff 283, 284, …) — an error there ends decoding
    let mut qt = false;
    let r = (|| -> Result<(), String> { loop {
        o = (o + 1) & !1; // opcodes are word aligned in v2
        if o >= p.len() {
            break; // no OpEndPic (x.pff 283, 284, … end right after the last band's fallback text)
        }
        let op = be16(p, o)?;
        o += 2;
        match op {
            0x0c00 => o += 24,
            // drawing state and vector / text opcodes (QuickTime's fallback text after each 0x8200, pen placeholders):
            // skipped by their data length (Inside Macintosh: Imaging With QuickDraw, table A-2); not drawn
            0x0000 | 0x001c | 0x001e | 0x0038..=0x003f | 0x0048..=0x004f | 0x0058..=0x005f | 0x0078..=0x007f
            | 0x0088..=0x008f => {}
            0x0004 | 0x0011 => o += 1,
            0x0003 | 0x0005 | 0x0008 | 0x000d | 0x0015 | 0x0016 | 0x0023 | 0x00a0 => o += 2,
            0x0006 | 0x0007 | 0x000b | 0x000c | 0x000e | 0x000f | 0x0021 | 0x0068..=0x006f => o += 4,
            0x001a | 0x001b | 0x001d | 0x001f | 0x0022 => o += 6,
            0x0002 | 0x0009 | 0x000a | 0x0010 | 0x0020 | 0x0030..=0x0037 | 0x0040..=0x0047 | 0x0050..=0x0057 => o += 8,
            0x0060..=0x0067 => o += 12,
            0x0001 | 0x0070..=0x0077 | 0x0080..=0x0087 => o += region_size(p, o)?,
            0x0028 => o += 5 + *p.get(o + 4).ok_or("pict: truncated text")? as usize,
            0x0029 | 0x002a => o += 2 + *p.get(o + 1).ok_or("pict: truncated text")? as usize,
            0x002b => o += 3 + *p.get(o + 2).ok_or("pict: truncated text")? as usize,
            0x002c..=0x002f => o += 2 + be16(p, o)? as usize,
            0x00a1 => o += 4 + be16(p, o + 2)? as usize,
            0x00ff => break,
            0x009a | 0x009b => {
                let rb = be16(p, o + 4)? as usize & 0x3fff;
                let pm = pixmap(p, o + 6, rb)?;
                o += 4 + 2 + 44;
                o += 18; // srcRect, dstRect, mode
                if op == 0x009b {
                    o += region_size(p, o)?;
                }
                let row_len = if pm.pixel_size == 32 && pm.pack_type == 4 { pm.w * pm.cmp_count as usize } else { pm.row_bytes };
                let (rws, end) = rows(p, o, &pm, row_len)?;
                o = end;
                let (iw, cmp, ps, pt) = (pm.w, pm.cmp_count as usize, pm.pixel_size, pm.pack_type);
                let px = |x: usize, y: usize| -> [u8; 3] {
                    let r = &rws[y];
                    match (ps, pt) {
                        (32, 4) => {
                            let s = if cmp == 4 { 1 } else { 0 };
                            let g = |c: usize| r.get((s + c) * iw + x).copied().unwrap_or(0);
                            [g(0), g(1), g(2)]
                        }
                        (32, _) => {
                            let g = |c: usize| r.get(x * 4 + 1 + c).copied().unwrap_or(0);
                            [g(0), g(1), g(2)]
                        }
                        (16, _) => {
                            let v = u16::from_be_bytes([r.get(2 * x).copied().unwrap_or(0), r.get(2 * x + 1).copied().unwrap_or(0)]);
                            let c = |s: u16| (((v >> s) & 31) as u32 * 255 / 31) as u8;
                            [c(10), c(5), c(0)]
                        }
                        _ => [0, 0, 0],
                    }
                };
                blit(&mut pic, pm.left, pm.top, pm.w, pm.h, &px);
                if (ps, pt, cmp) == (32, 4, 4) {
                    // H1023: the A plane (first of the four planes of each row); pixels outside the image stay opaque
                    let a = pic.alpha.get_or_insert_with(|| vec![255; w * h]);
                    for y in 0..pm.h {
                        let ty = pm.top + y as i32 - ft;
                        if ty < 0 || ty >= h as i32 {
                            continue;
                        }
                        for x in 0..pm.w {
                            let tx = pm.left + x as i32 - fl;
                            if tx >= 0 && tx < w as i32 {
                                a[ty as usize * w + tx as usize] = rws[y].get(x).copied().unwrap_or(0);
                            }
                        }
                    }
                }
            }
            0x0098 | 0x0099 => {
                let rb = be16(p, o)? as usize & 0x3fff;
                let pm = pixmap(p, o + 2, rb)?;
                o += 2 + 44;
                // colour table: seed u32, flags u16, size u16 (n-1), entries (value, r, g, b) u16
                let flags = be16(p, o + 4)?;
                let n = be16(p, o + 6)? as usize + 1;
                let mut pal = [[0u8; 3]; 256];
                for i in 0..n {
                    let e = o + 8 + 8 * i;
                    let idx = if flags & 0x8000 != 0 { i } else { be16(p, e)? as usize & 0xff };
                    pal[idx.min(255)] = [(be16(p, e + 2)? >> 8) as u8, (be16(p, e + 4)? >> 8) as u8, (be16(p, e + 6)? >> 8) as u8];
                }
                o += 8 + 8 * n;
                o += 18;
                if op == 0x0099 {
                    o += region_size(p, o)?;
                }
                let (rws, end) = rows(p, o, &pm, pm.row_bytes)?;
                o = end;
                let bits = pm.pixel_size.max(1) as usize;
                let px = |x: usize, y: usize| -> [u8; 3] {
                    let r = &rws[y];
                    let bit = x * bits;
                    let byte = r.get(bit / 8).copied().unwrap_or(0) as usize;
                    let v = (byte >> (8 - bits - bit % 8)) & ((1 << bits) - 1);
                    pal[v]
                };
                blit(&mut pic, pm.left, pm.top, pm.w, pm.h, &px);
            }
            0x8200 => {
                let n = be32(p, o)? as usize;
                let body = p.get(o + 4..o + 4 + n).ok_or("pict: QuickTime data past the end")?;
                // version u16, matrix 36, matteSize u32, matteRect 8, mode u16, srcRect 8, accuracy u32, maskSize u32
                let matte = be32(body, 38)? as usize;
                let mask = be32(body, 64)? as usize;
                let src = (be16(body, 52)? as i16 as i32, be16(body, 54)? as i16 as i32);
                // matrix translation (Fixed 16.16): the band offset (x.pff 57: ty = 0, 55, 110, …)
                let (tx, ty) = (be32(body, 26)? as i32 >> 16, be32(body, 30)? as i32 >> 16);
                let id = 68 + matte + mask;
                let idsz = be32(body, id)? as usize;
                let codec = body.get(id + 4..id + 8).ok_or("pict: short image description")?;
                if codec != b"jpeg" {
                    return Err(format!("pict: QuickTime codec {:?} not supported", String::from_utf8_lossy(codec)));
                }
                let data = body.get(id + idsz..).ok_or("pict: no image data")?;
                let mut d = jpeg_decoder::Decoder::new(data);
                let px = d.decode().map_err(|e| format!("pict jpeg: {e}"))?;
                let info = d.info().ok_or("pict jpeg: no info")?;
                let (iw, ih) = (info.width as usize, info.height as usize);
                let gray = info.pixel_format == jpeg_decoder::PixelFormat::L8;
                let f = |x: usize, y: usize| -> [u8; 3] {
                    if gray {
                        [px[y * iw + x]; 3]
                    } else {
                        let i = (y * iw + x) * 3;
                        [px[i], px[i + 1], px[i + 2]]
                    }
                };
                // the matrix is a pure translation in every installed file
                blit(&mut pic, src.1 + tx, src.0 + ty, iw, ih, &f);
                // what follows is QuickTime's fallback drawing ("QuickTime and a … decompressor are needed …"), then
                // possibly the next band (x.pff 57: a 640×480 picture in 55-row JPEG bands)
                o += 4 + n;
                qt = true;
            }
            _ => return Err(format!("pict: opcode {op:#06x} at {} not supported", o - 2)),
        }
    } Ok(()) })();
    if let Err(e) = r {
        if !qt {
            return Err(e);
        }
    }
    // H1024: an alpha plane that never reaches 255 is not a coverage mask (the cursors x.pff 30–34 / 344–363, the box
    // 246 / 247 and its patch 711: 105 / 140 / 153 on every drawn pixel; H300 measured the box's black body leaving the
    // View × 0.33, not × (1 − 105/255) = 0.59) — dropped, those pictures stay keyed by blue as before
    if pic.alpha.as_ref().is_some_and(|a| !a.contains(&255)) {
        pic.alpha = None;
    }
    // no image opcode (x.pff 133, 317–319: 1×1 pen-pattern placeholders): the picture stays black
    Ok(pic)
}

/// PackBits (Apple TN1023) of one row: runs of 3..128 equal bytes as (1 − n, v), the rest as literals of ≤ 128.
fn pack_bits(src: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(src.len() + src.len() / 64 + 2);
    let mut lit: Vec<u8> = Vec::new();
    let flush = |out: &mut Vec<u8>, lit: &mut Vec<u8>| {
        for c in lit.chunks(128) {
            out.push((c.len() - 1) as u8);
            out.extend_from_slice(c);
        }
        lit.clear();
    };
    let mut i = 0;
    while i < src.len() {
        let mut n = 1;
        while i + n < src.len() && n < 128 && src[i + n] == src[i] {
            n += 1;
        }
        if n >= 3 {
            flush(&mut out, &mut lit);
            out.push((1 - n as i32) as i8 as u8);
            out.push(src[i]);
        } else {
            lit.extend_from_slice(&src[i..i + n]);
        }
        i += n;
    }
    flush(&mut out, &mut lit);
    out
}

/// H1101 (layout copied from the installed DirectBitsRect pictures, e.g. x.pff 1 / 2 / 12, and the
/// original's photo thumbnail's opcode order): a PICT v2 of `pic` — `size, picFrame 0,0,h,w`, `0x0011 0x02ff`, the
/// 0x0c00 extended header (version −2, 72 dpi, srcRect), 0x001e DefHilite, 0x0001 Clip (the frame), 0x009a
/// DirectBitsRect (baseAddr 0xff, rowBytes 0x8000 | 4w, packType 4, 72 dpi, pixelType 16 RGBDirect, pixelSize 32,
/// cmpCount 3, cmpSize 8; srcRect = dstRect = the frame, mode 0x40 ditherCopy; per row a u16 / u8 byte count and the
/// PackBits of the planar R, G, B bytes), 0x00ff. QuickDraw draws it without QuickTime (the original's own thumbnail is
/// a 0x8200 QuickTime-JPEG picture; the port has no JPEG encoder).
pub fn encode(pic: &Picture) -> Vec<u8> {
    let (w, h) = (pic.width as u16, pic.height as u16);
    let frame = |b: &mut Vec<u8>| {
        for v in [0u16, 0, h, w] {
            b.extend(v.to_be_bytes());
        }
    };
    let mut b = vec![0, 0];
    frame(&mut b);
    b.extend([0x00, 0x11, 0x02, 0xff, 0x0c, 0x00, 0xff, 0xfe, 0, 0]);
    b.extend(0x0048_0000u32.to_be_bytes());
    b.extend(0x0048_0000u32.to_be_bytes());
    frame(&mut b);
    b.extend([0, 0, 0, 0]);
    b.extend([0x00, 0x1e, 0x00, 0x01, 0x00, 0x0a]);
    frame(&mut b);
    let row_bytes = 4 * w as usize;
    b.extend([0x00, 0x9a, 0, 0, 0, 0xff]);
    b.extend((0x8000 | row_bytes as u16).to_be_bytes());
    frame(&mut b);
    b.extend([0, 0, 0, 4, 0, 0, 0, 0]); // pmVersion, packType 4, packSize
    b.extend(0x0048_0000u32.to_be_bytes());
    b.extend(0x0048_0000u32.to_be_bytes());
    b.extend([0, 0x10, 0, 0x20, 0, 3, 0, 8]); // pixelType, pixelSize, cmpCount, cmpSize
    b.extend([0; 12]); // planeBytes, pmTable, pmReserved
    frame(&mut b);
    frame(&mut b);
    b.extend([0x00, 0x40]);
    let (w, h) = (pic.width, pic.height);
    for y in 0..h {
        let mut row = vec![0u8; 3 * w];
        for x in 0..w {
            for c in 0..3 {
                row[c * w + x] = pic.rgb[(y * w + x) * 3 + c];
            }
        }
        let pk = if row_bytes < 8 { row } else { pack_bits(&row) };
        if row_bytes > 250 {
            b.extend((pk.len() as u16).to_be_bytes());
        } else if row_bytes >= 8 {
            b.push(pk.len() as u8);
        }
        b.extend(pk);
    }
    if b.len() % 2 == 1 {
        b.push(0);
    }
    b.extend([0x00, 0xff]);
    let n = (b.len() & 0xffff) as u16;
    b[..2].copy_from_slice(&n.to_be_bytes());
    b
}

/// `pic` scaled to `w`×`h` by nearest pixel (the album's scaling, H793)
pub fn scale(pic: &Picture, w: usize, h: usize) -> Picture {
    let mut rgb = vec![0; w * h * 3];
    if pic.width > 0 && pic.height > 0 {
        for y in 0..h {
            for x in 0..w {
                let (sx, sy) = (x * pic.width / w, y * pic.height / h);
                let s = (sy * pic.width + sx) * 3;
                rgb[(y * w + x) * 3..][..3].copy_from_slice(&pic.rgb[s..s + 3]);
            }
        }
    }
    Picture { width: w, height: h, rgb, alpha: None }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// H1101: the encoder's pictures decode back to the same pixels (packed rows, runs and literals)
    #[test]
    fn encode_round_trip() {
        let (w, h) = (80, 60);
        let rgb = (0..w * h * 3).map(|i| if (i / 3) % 7 < 3 { 200 } else { (i * 37 % 251) as u8 }).collect();
        let pic = Picture { width: w, height: h, rgb, alpha: None };
        let b = encode(&pic);
        assert_eq!(&b[2..14], &[0, 0, 0, 0, 0, 60, 0, 80, 0, 0x11, 0x02, 0xff]);
        assert_eq!(decode(&b).unwrap(), pic);
        let black = Picture { width: w, height: h, rgb: vec![0; w * h * 3], alpha: None };
        assert_eq!(decode(&encode(&black)).unwrap(), black);
        assert!(encode(&black).len() < 1000);
    }

    #[test]
    fn packbits() {
        // TN1023 example
        let src = [0xfe, 0xaa, 0x02, 0x80, 0x00, 0x2a, 0xfd, 0xaa, 0x03, 0x80, 0x00, 0x2a, 0x22, 0xf7, 0xaa];
        let out = unpack_bits(&src, 24).unwrap();
        assert_eq!(out[..3], [0xaa; 3]);
        assert_eq!(out[3..6], [0x80, 0x00, 0x2a]);
        assert_eq!(out.len(), 24);
    }

    #[test]
    fn container() {
        let mut b = b"PFF ".to_vec();
        b.extend(2u32.to_le_bytes());
        b.extend(28u32.to_le_bytes());
        b.extend(30u32.to_le_bytes());
        b.extend(33u32.to_le_bytes());
        b.extend([0; 8]);
        b.extend([1, 2, 3, 4, 5]);
        let p = Pff::parse(b).unwrap();
        assert_eq!(p.len(), 2);
        assert_eq!(p.entry(0), Some(&[1, 2][..]));
        assert_eq!(p.entry(1), Some(&[3, 4, 5][..]));
    }
}
