//! The decoder's tests: decoding what the encoder writes must give the input back, through model restarts on a full
//! memory and solid continuation. The real-data check (an installer on a disc image) is in tests/discs.rs.
use super::*;

/// Deterministic test input: words from a small vocabulary with a little noise (compressible, not trivial).
pub(crate) fn sample(n: usize, seed: u32) -> Vec<u8> {
    let words: [&[u8]; 12] =
        [b"the ", b"truth ", b"is ", b"out ", b"there ", b"agent ", b"file ", b"case ", b"\n", b"0x1f ", b"--", b". "];
    let mut s = seed.wrapping_mul(2654435761).wrapping_add(1);
    let mut out = Vec::with_capacity(n);
    while out.len() < n {
        s ^= s << 13;
        s ^= s >> 17;
        s ^= s << 5;
        if s.is_multiple_of(17) {
            out.push((s >> 8) as u8);
        } else {
            out.extend_from_slice(words[(s >> 4) as usize % words.len()]);
        }
    }
    out.truncate(n);
    out
}

/// Encode and decode `data`; the number of model restarts on the way.
fn roundtrip(data: &[u8], mb: u32, order: u32) -> u32 {
    let mut e = Model::new(mb);
    let packed = encode(&mut e, data, order);
    let mut d = Model::new(mb);
    let back = d.decode(&packed, order, data.len() + 16).unwrap();
    assert_eq!(back.len(), data.len(), "mb {mb} order {order}");
    assert!(back == data, "mb {mb} order {order}: decoded bytes differ");
    assert_eq!(d.restarts, e.restarts);
    d.restarts
}

#[test]
fn roundtrip_orders() {
    let data = sample(200_000, 1);
    for order in [2, 4, 6, 8, 16] {
        roundtrip(&data, 2, order);
    }
    roundtrip(b"", 1, 4);
    roundtrip(b"x", 1, 4);
    let all: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    roundtrip(&all, 1, 6);
}

/// 1 MB of model memory for 3 MB of noisy input: the model fills up and restarts (the allocator glues and splits
/// blocks on the way).
#[test]
fn roundtrip_memory_full() {
    let mut data = sample(3_000_000, 7);
    let mut s = 99u32;
    for b in data.iter_mut().step_by(3) {
        s = s.wrapping_mul(1103515245).wrapping_add(12345);
        *b ^= (s >> 16) as u8;
    }
    assert!(roundtrip(&data, 1, 16) > 0, "the model memory never ran full");
}

/// Solid blocks: the second stream continues the first's model (order 1), as the installer's blocks do.
#[test]
fn solid_blocks() {
    let a = sample(50_000, 3);
    let b = sample(50_000, 4);
    let mut e = Model::new(2);
    let pa = encode(&mut e, &a, 6);
    let pb = encode(&mut e, &b, 1);
    let mut d = Model::new(2);
    assert!(d.decode(&pa, 6, a.len()).unwrap() == a);
    assert!(d.decode(&pb, 1, b.len()).unwrap() == b);
    // a solid block needs a model first
    assert!(Model::new(1).decode(&pb, 1, 10).is_err());
}

/// A stream cut in half: an error or a different result, never a hang or a panic.
#[test]
fn damaged_input_ends() {
    let data = sample(20_000, 5);
    let mut e = Model::new(1);
    let mut packed = encode(&mut e, &data, 6);
    packed.truncate(packed.len() / 2);
    let r = Model::new(1).decode(&packed, 6, data.len());
    assert!(r.map_or(true, |b| b != data));
}
